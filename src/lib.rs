// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core library for the `flint` folder linter.
//!
//! Parsing, checking and the whole run over a directory tree live here as
//! pure functions so they can be unit tested without touching the
//! filesystem — [`run`] reaches a tree only through its `DirectoryTree`
//! trait. `main.rs` is the thin I/O layer that implements that trait over
//! the real disk and prints the result.

pub mod checker;
pub mod entry_kind;
pub mod flint_file;
mod format;
pub mod memory_tree;
pub mod origin;
pub mod output;
pub mod run;
pub mod scope;
pub mod template;
