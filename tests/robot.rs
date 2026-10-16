// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runs the Robot Framework end-to-end suite (`e2e/tests/`) from
//! `cargo test` — for mutation testing only. See
//! `docs/mutation-testing.md`.
//!
//! cargo-mutants judges each mutant by whether `cargo test` fails, and
//! can't run anything else. Without this wrapper it would only see the
//! unit tests, so every mutant in code that only the e2e suite checks
//! (`main.rs`, `output.rs`) would look untested.
//!
//! It does nothing unless `FLINT_ROBOT` names a `robot` executable (as
//! `just mutants` sets it), so plain `cargo test` stays unit tests only
//! and doesn't need Python. This is not a place for new tests: write
//! those in `e2e/tests/*.robot` (see `AGENTS.md`).

// Running `robot` and reporting on it is this file's job — `clippy.toml`'s
// bans are for the library.
#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let Some(robot) = env::var_os("FLINT_ROBOT") else {
        println!(
            "FLINT_ROBOT isn't set, so the Robot Framework suite was skipped \
             (`just test-e2e` runs it)."
        );
        return ExitCode::SUCCESS;
    };
    let status = Command::new(&robot)
        .env("FLINT_BINARY", env!("CARGO_BIN_EXE_flint"))
        // Only the console output is wanted: each mutant runs the suite
        // again, so skip writing Robot's XML/HTML reports every time.
        .args(["--console", "dotted"])
        .args(["--output", "NONE", "--log", "NONE", "--report", "NONE"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/e2e/tests"))
        .status();
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Couldn't run {}: {error}", robot.to_string_lossy());
            ExitCode::FAILURE
        }
    }
}
