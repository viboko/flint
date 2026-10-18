// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `flint` command: checks a directory against its `.flint` files
//! and prints the result.

// This is flint's I/O layer — the one place that reads the disk and
// prints. `clippy.toml` bans both everywhere else.
#![allow(clippy::disallowed_macros, clippy::disallowed_methods)]

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use flint::entry_kind::EntryKind;
use flint::output::Stream;
use flint::run::{DirectoryTree, FLINT_FILE_NAME, Report, ScopeStatus, run};

// `about` (bare) and `version` (bare) pull from Cargo.toml's `description`
// and `version` fields, so the --help/--version text stays in sync with
// the manifest without duplicating it here.
#[derive(Parser)]
#[command(name = "flint", version, about, long_about = None)]
/// The command-line arguments.
struct Cli {
    /// Directory to check (defaults to the current directory)
    #[arg(default_value = ".")]
    path: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = cli.path.as_path();

    match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            eprintln!("Can't check {}: it isn't a folder", root.display());
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("Can't check {}: {err}", root.display());
            return ExitCode::FAILURE;
        }
    }

    let report = run(&Disk { root });
    for line in flint::output::render(&report) {
        match line.stream {
            Stream::Stdout => println!("{}", line.text),
            Stream::Stderr => eprintln!("{}", line.text),
        }
    }
    if cfg!(target_os = "macos") && macos_protected_any(&report) {
        eprintln!("{MACOS_PROTECTED_HINT}");
    }

    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// What `errno` macOS gives for listing a folder its privacy protection
/// keeps from the terminal (`EPERM`, "Operation not permitted"), unlike
/// ordinary permissions' `EACCES`.
const EPERM: i32 = 1;

/// Printed once, on macOS, after any listing failed with [`EPERM`].
const MACOS_PROTECTED_HINT: &str = "macOS keeps some folders private, such as Mail's and Safari's \
in Library. To check them, give your terminal Full Disk Access in System Settings, \
under Privacy & Security.";

/// Whether any directory in `report` couldn't be listed for the reason
/// macOS gives for a folder it keeps private (see [`EPERM`]).
fn macos_protected_any(report: &Report<io::Error>) -> bool {
    let in_scopes = report
        .scopes
        .iter()
        .filter_map(|scope| match &scope.status {
            ScopeStatus::ListError(unlisted) => Some(unlisted),
            _ => None,
        });
    report
        .skipped
        .iter()
        .chain(in_scopes)
        .any(|unlisted| unlisted.error.raw_os_error() == Some(EPERM))
}

/// The real filesystem under `root`, for [`run`] to walk.
struct Disk<'a> {
    /// The directory flint was asked to check.
    root: &'a Path,
}

impl Disk<'_> {
    /// The filesystem path of `dir`, a `/`-joined path relative to `root`
    /// (`""` for `root` itself).
    fn path(&self, dir: &str) -> PathBuf {
        if dir.is_empty() {
            self.root.to_path_buf()
        } else {
            self.root.join(dir)
        }
    }
}

impl DirectoryTree for Disk<'_> {
    type Error = std::io::Error;

    /// A symlink is always classified as [`EntryKind::File`] here,
    /// regardless of what it points at — `DirEntry::file_type` doesn't
    /// follow symlinks, so `is_dir()` is naturally `false` for every
    /// symlink, which also means a run never recurses into a symlinked
    /// directory.
    ///
    /// A name that isn't valid Unicode fails the listing — see
    /// [`unicode_name`].
    fn list(&self, dir: &str) -> std::io::Result<Vec<(String, EntryKind)>> {
        let mut entries = Vec::new();
        for dir_entry in fs::read_dir(self.path(dir))? {
            let dir_entry = dir_entry?;
            let kind = if dir_entry.file_type()?.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            };
            entries.push((unicode_name(dir_entry.file_name())?, kind));
        }
        Ok(entries)
    }

    fn has_flint_file(&self, dir: &str) -> bool {
        self.path(dir).join(FLINT_FILE_NAME).is_file()
    }

    fn read_flint_file(&self, dir: &str) -> std::io::Result<String> {
        fs::read_to_string(self.path(dir).join(FLINT_FILE_NAME))
    }
}

/// `name`, a directory entry's, as text — or an error naming it, if it
/// isn't valid Unicode (on Unix, UTF-8). The directory is named by
/// whoever reports the failed listing. A `.flint` file is text, so no rule could name such an entry,
/// and swapping the bad bytes for `\u{fffd}` to carry on would check, and
/// report, a name that isn't the one on disk.
fn unicode_name(name: OsString) -> io::Result<String> {
    name.into_string().map_err(|name| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the name {name:?} isn't valid Unicode, so flint can't check it"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flint::run::{ScopeReport, Unlisted};

    fn unlisted(errno: i32) -> Unlisted<io::Error> {
        Unlisted {
            dir: "Library/Mail".to_string(),
            error: io::Error::from_raw_os_error(errno),
        }
    }

    #[test]
    fn only_eperm_counts_as_macos_keeping_a_folder_private() {
        const EACCES: i32 = 13;
        let skipped = |errno| Report {
            scopes: Vec::new(),
            skipped: vec![unlisted(errno)],
        };
        assert!(macos_protected_any(&skipped(EPERM)));
        assert!(!macos_protected_any(&skipped(EACCES)));

        let in_scope = |errno| Report {
            scopes: vec![ScopeReport {
                scope: String::new(),
                status: ScopeStatus::ListError(unlisted(errno)),
            }],
            skipped: Vec::new(),
        };
        assert!(macos_protected_any(&in_scope(EPERM)));
        assert!(!macos_protected_any(&in_scope(EACCES)));
    }

    #[test]
    fn a_unicode_name_is_kept_exactly() {
        for name in [
            "plain.txt",
            "caf\u{e9}",
            "cafe\u{301}",
            "日本語",
            "\u{fffd}",
        ] {
            assert_eq!(unicode_name(OsString::from(name)).unwrap(), name);
        }
    }

    /// Unix only: a Windows name is UTF-16, and a broken one is built
    /// another way.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_unicode_is_an_error_naming_it() {
        use std::os::unix::ffi::OsStringExt;

        // `café.txt` in Latin-1, where `é` is the single byte E9.
        let err = unicode_name(OsString::from_vec(b"caf\xe9.txt".to_vec())).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            err.to_string(),
            r#"the name "caf\xE9.txt" isn't valid Unicode, so flint can't check it"#
        );
    }
}
