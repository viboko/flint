// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The file-or-directory distinction shared by parsing, template
//! matching, checking, and the filesystem walk.

/// Whether a `.flint` entry matches only directories or only files —
/// determined by a trailing `/` on the source line (directory) or its
/// absence (file). Also used to describe what an actual filesystem entry
/// is, so matching is just "does the actual kind equal the expected kind."
///
/// Symlinks are always classified as [`EntryKind::File`] here, regardless
/// of what they point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A regular file (or a symlink, of any target type).
    File,
    /// A real, non-symlink directory.
    Directory,
}
