// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tier 1: pure in-memory benchmarks for flint's parsing and checking
//! logic (`flint_file::parse`, `checker::check`). No filesystem access —
//! see `benches/e2e.rs` for the tier that measures real disk I/O.
//!
//! These count CPU instructions under Valgrind (via Gungraun) rather than
//! timing anything: each benchmark runs once, and an unchanged build
//! gives near enough the same count every run, on any machine. That's
//! what lets CI fail a PR over them — see docs/benchmarking.md. Valgrind
//! doesn't run on macOS, so there they need a Linux container.
//!
//! The scaling dimensions are the number of directory entries and the
//! number of `.flint` allow-list lines (literal or `{{ variable }}`
//! template), each at 100, 1,000 and 10,000.
//!
//! Only the body of a `#[library_benchmark]` function is counted. Its
//! `setup` function builds the input beforehand, uncounted, and leaks it
//! so that freeing it isn't counted either.

// Gungraun's macros expand to code that prints, calls
// `std::process::exit` and (on Linux, in `#[library_benchmark]`) opens a
// `std::fs::File` — banned by `clippy.toml` for the library, not for a
// bench harness.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]
// Gungraun's macros also expand to undocumented modules, constants and
// functions, and reject a doc comment or `#[allow]` on a benchmark
// function, so the missing-docs lints can only be lifted for the whole
// file. Everything written here by hand is still documented.
#![allow(missing_docs, clippy::missing_docs_in_private_items)]

use std::fmt::Write as _;
use std::hint::black_box;

use flint::checker::{self, ActualEntry, Violation};
use flint::entry_kind::EntryKind;
use flint::flint_file::{self, Entry};
use gungraun::prelude::*;

/// The rules and the directory listing one `checker::check` call
/// compares.
type CheckInput = (&'static [Entry], &'static [ActualEntry]);

/// A `.flint` file of `n` literal rules: `file_0.txt`, `file_1.txt`, …
fn literal_flint_contents(n: usize) -> &'static str {
    let mut contents = String::new();
    for i in 0..n {
        writeln!(contents, "file_{i}.txt").expect("writing to a String never fails");
    }
    contents.leak()
}

/// A `.flint` file of `n` template rules, each with a variable of its
/// own: `{{ name_0 }}_thing.txt`, …
fn template_flint_contents(n: usize) -> &'static str {
    let mut contents = String::new();
    for i in 0..n {
        writeln!(contents, "{{{{ name_{i} }}}}_thing.txt")
            .expect("writing to a String never fails");
    }
    contents.leak()
}

/// `contents`, parsed.
fn parsed(contents: &str) -> &'static [Entry] {
    flint_file::parse(contents)
        .expect("generated .flint contents always parse")
        .leak()
}

/// A listing of `n` files, named as [`literal_flint_contents`]'s rules
/// name them.
fn literal_actual_entries(n: usize) -> &'static [ActualEntry] {
    (0..n)
        .map(|i| ActualEntry {
            name: format!("file_{i}.txt"),
            kind: EntryKind::File,
        })
        .collect::<Vec<_>>()
        .leak()
}

/// `n` literal rules, and the `n` files they name.
fn all_matching(n: usize) -> CheckInput {
    (parsed(literal_flint_contents(n)), literal_actual_entries(n))
}

/// `n` templates against a fixed number of files.
fn scaling_template_count(n: usize) -> CheckInput {
    const FILE_COUNT: usize = 200;
    (
        parsed(template_flint_contents(n)),
        literal_actual_entries(FILE_COUNT),
    )
}

/// A fixed number of templates against `n` files.
fn scaling_file_count(n: usize) -> CheckInput {
    const TEMPLATE_COUNT: usize = 20;
    (
        parsed(template_flint_contents(TEMPLATE_COUNT)),
        literal_actual_entries(n),
    )
}

#[library_benchmark]
#[bench::literal_100(args = (100), setup = literal_flint_contents)]
#[bench::literal_1000(args = (1_000), setup = literal_flint_contents)]
#[bench::literal_10000(args = (10_000), setup = literal_flint_contents)]
#[bench::template_100(args = (100), setup = template_flint_contents)]
#[bench::template_1000(args = (1_000), setup = template_flint_contents)]
#[bench::template_10000(args = (10_000), setup = template_flint_contents)]
fn parse(contents: &str) -> Vec<Entry> {
    flint_file::parse(black_box(contents)).expect("generated .flint contents always parse")
}

// All entries match: the realistic common case, and a clean read on
// `checker::check`'s literal-name `HashSet` lookup cost alone.
#[library_benchmark]
#[bench::n_100(args = (100), setup = all_matching)]
#[bench::n_1000(args = (1_000), setup = all_matching)]
#[bench::n_10000(args = (10_000), setup = all_matching)]
fn check_literal_all_matching((entries, actual): CheckInput) -> Vec<Violation> {
    checker::check(black_box(entries), black_box(actual))
}

// Holds the actual-entry count fixed and grows the number of templates,
// isolating the template-count half of `checker::check`'s O(templates ×
// files) matching loop.
#[library_benchmark]
#[bench::n_100(args = (100), setup = scaling_template_count)]
#[bench::n_1000(args = (1_000), setup = scaling_template_count)]
#[bench::n_10000(args = (10_000), setup = scaling_template_count)]
fn check_scaling_template_count((entries, actual): CheckInput) -> Vec<Violation> {
    checker::check(black_box(entries), black_box(actual))
}

// Holds the template count fixed and grows the number of actual entries,
// isolating the file-count half of the same matching loop.
#[library_benchmark]
#[bench::n_100(args = (100), setup = scaling_file_count)]
#[bench::n_1000(args = (1_000), setup = scaling_file_count)]
#[bench::n_10000(args = (10_000), setup = scaling_file_count)]
fn check_scaling_file_count((entries, actual): CheckInput) -> Vec<Violation> {
    checker::check(black_box(entries), black_box(actual))
}

library_benchmark_group!(
    name = logic,
    benchmarks = [
        parse,
        check_literal_all_matching,
        check_scaling_template_count,
        check_scaling_file_count
    ]
);

main!(library_benchmark_groups = logic);
