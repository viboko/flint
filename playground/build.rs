// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embeds every example in `examples/` into the crate (see
//! `EXAMPLE_SOURCES` in `src/lib.rs`), so the tests check exactly the
//! examples the playground ships with.

// Reading `examples/` and writing generated code is a build script's job —
// `clippy.toml`'s bans are for the library.
#![allow(clippy::disallowed_macros, clippy::disallowed_methods)]
// A build script fails the build by panicking; it has no other way to.
#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

fn main() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("examples");
    println!("cargo::rerun-if-changed={}", dir.display());

    let mut examples: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_string();
            // README.md describes the examples; it isn't one.
            (id != "README").then(|| (id, path.display().to_string()))
        })
        .collect();
    examples.sort();

    let mut code = String::from("&[\n");
    for (id, path) in &examples {
        writeln!(code, "    ({id:?}, include_str!({path:?})),").unwrap();
    }
    code.push(']');
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("examples.rs"),
        code,
    )
    .unwrap();
}
