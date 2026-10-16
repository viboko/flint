// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzz target: runs flint over an arbitrary in-memory directory tree
//! with arbitrary `.flint` text at its root — the same `flint::run::run`
//! the CLI uses on a real disk, then renders its output, so parsing, the
//! directory walk, checking and printing are all exercised together.
//!
//! An input is the root `.flint` file's contents, then a line that is
//! exactly `---`, then the directory listing: one `/`-separated relative
//! path per line, with a trailing `/` marking a directory. No `---` line
//! means an empty listing. A listed `.flint` file below the root is a
//! nested `.flint` file with no rules in it. See docs/fuzzing.md.

#![no_main]
// `fuzz_target!` expands to code that can write its input to a file
// (`RUST_LIBFUZZER_DEBUG_PATH`), which `clippy.toml` bans for the library,
// not for the fuzz harness.
#![allow(clippy::disallowed_types)]

use flint::memory_tree::MemoryTree;
use flint::output::render;
use flint::run::run;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // `flint` reads `.flint` with `fs::read_to_string`, so it never sees
    // invalid UTF-8.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (flint_text, listing) = split_input(text);

    let mut tree = MemoryTree::new();
    for line in listing.lines() {
        // A path no real directory entry could have (empty, `.` or `..`
        // components) is rejected, and skipped like it was never listed.
        let _ = match line.strip_suffix('/') {
            Some(path) => tree.add_directory(path),
            None => tree.add_file(line, ""),
        };
    }
    // Last, so a listed `.flint` can't replace the input's own rules.
    // (A listed `.flint/` directory with something inside it stays a
    // directory, leaving the root with no `.flint` file.)
    let _ = tree.add_file(".flint", flint_text);

    let report = run(&tree).expect("every directory in a MemoryTree can be listed");
    let _ = render(&report);
});

/// Splits an input at its first line that is exactly `---`.
fn split_input(text: &str) -> (&str, &str) {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches('\n') == "---" {
            return (&text[..offset], &text[offset + line.len()..]);
        }
        offset += line.len();
    }
    (text, "")
}
