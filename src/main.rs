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
use flint::run::{DirectoryTree, FLINT_FILE_NAME, run};

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

    let report = match run(&Disk { root }) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("Failed to scan {}: {err}", root.display());
            return ExitCode::FAILURE;
        }
    };

    for line in flint::output::render(&report) {
        match line.stream {
            Stream::Stdout => println!("{}", line.text),
            Stream::Stderr => eprintln!("{}", line.text),
        }
    }

    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
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
            entries.push((unicode_name(dir_entry.file_name(), dir)?, kind));
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

/// `name`, an entry of `dir` (a `/`-joined path, `""` for the root), as
/// text — or an error naming it, if it isn't valid Unicode (on Unix,
/// UTF-8). A `.flint` file is text, so no rule could name such an entry,
/// and swapping the bad bytes for `\u{fffd}` to carry on would check, and
/// report, a name that isn't the one on disk.
fn unicode_name(name: OsString, dir: &str) -> io::Result<String> {
    name.into_string().map_err(|name| {
        let place = if dir.is_empty() {
            String::new()
        } else {
            format!(" in {dir}")
        };
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the name {name:?}{place} isn't valid Unicode, so flint can't check it"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unicode_name_is_kept_exactly() {
        for name in [
            "plain.txt",
            "caf\u{e9}",
            "cafe\u{301}",
            "日本語",
            "\u{fffd}",
        ] {
            assert_eq!(unicode_name(OsString::from(name), "src").unwrap(), name);
        }
    }

    /// Unix only: a Windows name is UTF-16, and a broken one is built
    /// another way.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_unicode_is_an_error_naming_it() {
        use std::os::unix::ffi::OsStringExt;

        // `café.txt` in Latin-1, where `é` is the single byte E9.
        let latin1 = || OsString::from_vec(b"caf\xe9.txt".to_vec());
        let err = unicode_name(latin1(), "src/menus").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            err.to_string(),
            r#"the name "caf\xE9.txt" in src/menus isn't valid Unicode, so flint can't check it"#
        );
        assert_eq!(
            unicode_name(latin1(), "").unwrap_err().to_string(),
            r#"the name "caf\xE9.txt" isn't valid Unicode, so flint can't check it"#
        );
    }
}
