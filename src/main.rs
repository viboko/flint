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
use flint::output::{Line, Stream};
use flint::run::{DirectoryTree, FLINT_FILE_NAME, Unlisted, run};

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
    let (lines, passed) = check(&cli.path, cfg!(target_os = "macos"));
    for line in lines {
        match line.stream {
            Stream::Stdout => println!("{}", line.text),
            Stream::Stderr => eprintln!("{}", line.text),
        }
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Everything flint prints for checking `root`, and whether it passed.
/// `on_macos` says whether to explain a folder macOS keeps private (see
/// [`unlisted_lines`]).
fn check(root: &Path, on_macos: bool) -> (Vec<Line>, bool) {
    let failed = |text: String| {
        let line = Line {
            stream: Stream::Stderr,
            text,
        };
        (vec![line], false)
    };
    match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return failed(format!("Can't check {}: it isn't a folder", root.display())),
        Err(err) => return failed(format!("Can't check {}: {err}", root.display())),
    }
    match run(&Disk { root }) {
        Ok(report) => (flint::output::render(&report), report.passed()),
        Err(unlisted) => (unlisted_lines(&unlisted, on_macos), false),
    }
}

/// What flint prints when it stops at `unlisted`: the folder and why,
/// then, `on_macos`, how to let it see a folder the system keeps private.
fn unlisted_lines(unlisted: &Unlisted<io::Error>, on_macos: bool) -> Vec<Line> {
    let mut lines = vec![flint::output::render_unlisted(unlisted)];
    if on_macos && is_macos_private(&unlisted.error) {
        lines.push(Line {
            stream: Stream::Stderr,
            text: MACOS_PRIVATE_HINT.to_string(),
        });
    }
    lines
}

/// What `errno` macOS gives for listing a folder its privacy protection
/// keeps from the terminal (`EPERM`, "Operation not permitted"), unlike
/// ordinary permissions' `EACCES`.
const EPERM: i32 = 1;

/// Printed on macOS after a listing failed with [`EPERM`] (see
/// [`unlisted_lines`]).
const MACOS_PRIVATE_HINT: &str = "macOS keeps some folders private, such as Mail's and Safari's \
in Library. To check them, give your terminal Full Disk Access in System Settings, \
under Privacy & Security.";

/// Whether `error` is the one macOS gives for listing a folder it keeps
/// private (see [`EPERM`]).
fn is_macos_private(error: &io::Error) -> bool {
    error.raw_os_error() == Some(EPERM)
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    const EACCES: i32 = 13;

    /// A new, empty folder for one test, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("flint-main-{}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Writes `contents` to `path` inside it, adding any folders
        /// needed.
        fn file(&self, path: &str, contents: &str) -> &Self {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
            self
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// What [`check`] prints for `root`, one line each, stderr lines
    /// marked with a leading `!` — and whether it passed.
    fn checked(root: &Path, on_macos: bool) -> (String, bool) {
        let (lines, passed) = check(root, on_macos);
        let text = lines
            .iter()
            .map(|line| match line.stream {
                Stream::Stdout => line.text.clone(),
                Stream::Stderr => format!("!{}", line.text),
            })
            .collect::<Vec<_>>()
            .join("\n");
        (text, passed)
    }

    #[test]
    fn a_folder_that_matches_passes() {
        let dir = TempDir::new();
        dir.file(".flint", "a.txt\nsrc/\n    b.rs\n")
            .file("a.txt", "")
            .file("src/b.rs", "");
        assert_eq!(
            checked(&dir.0, false),
            ("All files match".to_string(), true)
        );
    }

    #[test]
    fn a_folder_with_a_stray_file_fails() {
        let dir = TempDir::new();
        dir.file(".flint", "a.txt\n").file("b.txt", "");
        assert_eq!(
            checked(&dir.0, false),
            ("Found 1 unexpected entry:\n- b.txt".to_string(), false)
        );
    }

    #[test]
    fn a_folder_without_a_flint_file_fails() {
        let dir = TempDir::new();
        dir.file("a.txt", "");
        assert_eq!(
            checked(&dir.0, false),
            (format!("!{}", flint::output::NO_FLINT_FILE), false)
        );
    }

    #[test]
    fn a_path_that_is_missing_or_not_a_folder_fails() {
        let dir = TempDir::new();
        dir.file("a.txt", "");
        let file = dir.0.join("a.txt");
        assert_eq!(
            checked(&file, false),
            (
                format!("!Can't check {}: it isn't a folder", file.display()),
                false
            )
        );
        let missing = dir.0.join("missing");
        let (text, passed) = checked(&missing, false);
        assert!(
            text.starts_with(&format!("!Can't check {}: ", missing.display())),
            "{text}"
        );
        assert!(!passed);
    }

    /// Unix only: Windows has no `chmod 000`.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_listed_stops_the_run() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new();
        dir.file(".flint", "**\n").file("secret/a.txt", "");
        let secret = dir.0.join("secret");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
        let listable = fs::read_dir(&secret).is_ok(); // as root, say
        let result = checked(&dir.0, true);
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o755)).unwrap();
        if listable {
            return;
        }
        // `EACCES`, not macOS's `EPERM`, so no hint.
        let (text, passed) = result;
        assert!(
            text.starts_with("!Couldn't list secret, so nothing was checked: "),
            "{text}"
        );
        assert!(!text.contains("Full Disk Access"), "{text}");
        assert!(!passed);
    }

    #[test]
    fn only_eperm_on_macos_adds_the_full_disk_access_hint() {
        let unlisted = |errno| Unlisted {
            dir: "Library/Mail".to_string(),
            error: io::Error::from_raw_os_error(errno),
        };
        let texts = |errno, on_macos| -> Vec<String> {
            unlisted_lines(&unlisted(errno), on_macos)
                .into_iter()
                .map(|line| {
                    assert_eq!(line.stream, Stream::Stderr);
                    line.text
                })
                .collect()
        };
        let error_line = |errno| flint::output::render_unlisted(&unlisted(errno)).text;
        assert_eq!(
            texts(EPERM, true),
            [error_line(EPERM), MACOS_PRIVATE_HINT.to_string()]
        );
        assert_eq!(texts(EPERM, false), [error_line(EPERM)]);
        assert_eq!(texts(EACCES, true), [error_line(EACCES)]);
        assert_eq!(texts(EACCES, false), [error_line(EACCES)]);
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
