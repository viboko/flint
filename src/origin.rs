// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where a rule was written, carried from parsing through to any violation
//! the rule produces — so a violation can point back at the exact `.flint`
//! line responsible, even one imported from an ancestor `.flint` file.

/// The `.flint` file and line a rule was written on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Origin {
    /// The directory containing the `.flint` file: `""` for the root of
    /// the run, or a `/`-joined path relative to it — the same path that
    /// names the file's scope in [`crate::run::ScopeReport`].
    pub scope: String,
    /// The 1-based line number within that `.flint` file. For a line
    /// indented under a directory, the indented line's own number.
    pub line: usize,
}
