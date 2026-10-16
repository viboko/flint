// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! An in-memory directory tree, for running flint without a real disk —
//! by the fuzz target, the browser playground, and unit tests.

use std::collections::BTreeMap;
use std::fmt;

use crate::entry_kind::EntryKind;
use crate::run::{DirectoryTree, FLINT_FILE_NAME};

/// A directory tree held in memory: directories, and files with their
/// contents. Every path is `/`-joined and relative to the tree's root.
#[derive(Debug, Clone, Default)]
pub struct MemoryTree {
    /// Every entry in the tree, keyed by path — sorted, so a directory's
    /// contents are one contiguous range (see [`MemoryTree::list`]).
    entries: BTreeMap<String, Node>,
}

/// What's at one path in a [`MemoryTree`].
#[derive(Debug, Clone)]
enum Node {
    /// A directory; what's inside it has paths of its own.
    Directory,
    /// A file, with its contents.
    File(String),
}

/// A path [`MemoryTree`] can't hold: empty, or with an empty, `.` or `..`
/// component — no real directory entry can be named any of those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPath(pub String);

impl fmt::Display for InvalidPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid path: {:?}", self.0)
    }
}

impl std::error::Error for InvalidPath {}

/// A directory or file asked for that isn't in a [`MemoryTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound(pub String);

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} not found", self.0)
    }
}

impl std::error::Error for NotFound {}

impl MemoryTree {
    /// An empty tree: just the root directory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file at `path` holding `contents`, adding any missing
    /// parent directories. A parent that was a file becomes a directory,
    /// and `path` itself stays a directory if it already is one — a real
    /// tree can't have a file with something inside it.
    ///
    /// # Errors
    /// Returns `Err` if `path` isn't a valid path (see [`InvalidPath`]).
    pub fn add_file(&mut self, path: &str, contents: &str) -> Result<(), InvalidPath> {
        self.add_parents(path)?;
        if !self.has_children(path) {
            self.entries
                .insert(path.to_string(), Node::File(contents.to_string()));
        }
        Ok(())
    }

    /// Adds a directory at `path`, plus any missing parent directories. A
    /// file already at `path` or any parent becomes a directory.
    ///
    /// # Errors
    /// Returns `Err` if `path` isn't a valid path (see [`InvalidPath`]).
    pub fn add_directory(&mut self, path: &str) -> Result<(), InvalidPath> {
        self.add_parents(path)?;
        self.entries.insert(path.to_string(), Node::Directory);
        Ok(())
    }

    /// Checks `path` is valid, then makes each of its parents a directory.
    fn add_parents(&mut self, path: &str) -> Result<(), InvalidPath> {
        let components: Vec<&str> = path.split('/').collect();
        if components
            .iter()
            .any(|c| c.is_empty() || *c == "." || *c == "..")
        {
            return Err(InvalidPath(path.to_string()));
        }
        for (slash, _) in path.match_indices('/') {
            self.entries
                .insert(path[..slash].to_string(), Node::Directory);
        }
        Ok(())
    }

    /// Whether anything is inside `path`.
    fn has_children(&self, path: &str) -> bool {
        !self.children(path).is_empty()
    }

    /// Every entry directly inside `dir` (`""` for the root), with its
    /// name relative to `dir`, sorted by name.
    fn children(&self, dir: &str) -> Vec<(&str, &Node)> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        self.entries
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
            .filter_map(|(path, node)| {
                let name = &path[prefix.len()..];
                (!name.contains('/')).then_some((name, node))
            })
            .collect()
    }

    /// Whether `dir` is a directory (`""`, the root, always is).
    fn is_directory(&self, dir: &str) -> bool {
        dir.is_empty() || matches!(self.entries.get(dir), Some(Node::Directory))
    }

    /// The path of the `.flint` file in `dir` (`""` for the root).
    fn flint_file_path(dir: &str) -> String {
        if dir.is_empty() {
            FLINT_FILE_NAME.to_string()
        } else {
            format!("{dir}/{FLINT_FILE_NAME}")
        }
    }
}

impl DirectoryTree for MemoryTree {
    type Error = NotFound;

    /// The entries directly inside `dir`, sorted by name.
    fn list(&self, dir: &str) -> Result<Vec<(String, EntryKind)>, NotFound> {
        if !self.is_directory(dir) {
            return Err(NotFound(dir.to_string()));
        }
        Ok(self
            .children(dir)
            .into_iter()
            .map(|(name, node)| {
                let kind = match node {
                    Node::Directory => EntryKind::Directory,
                    Node::File(_) => EntryKind::File,
                };
                (name.to_string(), kind)
            })
            .collect())
    }

    fn has_flint_file(&self, dir: &str) -> bool {
        matches!(
            self.entries.get(&Self::flint_file_path(dir)),
            Some(Node::File(_))
        )
    }

    fn read_flint_file(&self, dir: &str) -> Result<String, NotFound> {
        let path = Self::flint_file_path(dir);
        match self.entries.get(&path) {
            Some(Node::File(contents)) => Ok(contents.clone()),
            Some(Node::Directory) | None => Err(NotFound(path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(tree: &MemoryTree, dir: &str) -> Vec<(String, EntryKind)> {
        tree.list(dir).unwrap()
    }

    fn entry(name: &str, kind: EntryKind) -> (String, EntryKind) {
        (name.to_string(), kind)
    }

    #[test]
    fn a_new_tree_has_an_empty_root() {
        assert_eq!(listing(&MemoryTree::new(), ""), Vec::new());
    }

    #[test]
    fn adding_a_file_adds_its_parent_directories() {
        let mut tree = MemoryTree::new();
        tree.add_file("a/b/c.txt", "").unwrap();
        assert_eq!(listing(&tree, ""), vec![entry("a", EntryKind::Directory)]);
        assert_eq!(listing(&tree, "a"), vec![entry("b", EntryKind::Directory)]);
        assert_eq!(listing(&tree, "a/b"), vec![entry("c.txt", EntryKind::File)]);
    }

    #[test]
    fn listing_shows_only_direct_children_sorted() {
        let mut tree = MemoryTree::new();
        tree.add_file("b.txt", "").unwrap();
        tree.add_file("a/deep.txt", "").unwrap();
        tree.add_directory("c").unwrap();
        tree.add_file("a-z.txt", "").unwrap();
        assert_eq!(
            listing(&tree, ""),
            vec![
                entry("a", EntryKind::Directory),
                entry("a-z.txt", EntryKind::File),
                entry("b.txt", EntryKind::File),
                entry("c", EntryKind::Directory),
            ]
        );
        assert_eq!(
            listing(&tree, "a"),
            vec![entry("deep.txt", EntryKind::File)]
        );
    }

    #[test]
    fn a_directory_s_listing_excludes_a_sibling_sharing_its_prefix() {
        let mut tree = MemoryTree::new();
        tree.add_file("a/x", "").unwrap();
        tree.add_file("ab/y", "").unwrap();
        assert_eq!(listing(&tree, "a"), vec![entry("x", EntryKind::File)]);
    }

    #[test]
    fn a_parent_that_was_a_file_becomes_a_directory() {
        let mut tree = MemoryTree::new();
        tree.add_file("a", "").unwrap();
        tree.add_file("a/b", "").unwrap();
        assert_eq!(listing(&tree, ""), vec![entry("a", EntryKind::Directory)]);
    }

    #[test]
    fn a_directory_with_contents_stays_one_when_added_as_a_file() {
        let mut tree = MemoryTree::new();
        tree.add_file("a/b", "").unwrap();
        tree.add_file("a", "").unwrap();
        assert_eq!(listing(&tree, ""), vec![entry("a", EntryKind::Directory)]);
    }

    #[test]
    fn an_empty_directory_added_as_a_file_becomes_a_file() {
        let mut tree = MemoryTree::new();
        tree.add_directory("a").unwrap();
        tree.add_file("a", "").unwrap();
        assert_eq!(listing(&tree, ""), vec![entry("a", EntryKind::File)]);
    }

    #[test]
    fn a_file_added_as_a_directory_becomes_one() {
        let mut tree = MemoryTree::new();
        tree.add_file("a", "").unwrap();
        tree.add_directory("a").unwrap();
        assert_eq!(listing(&tree, ""), vec![entry("a", EntryKind::Directory)]);
    }

    #[test]
    fn invalid_paths_are_rejected() {
        let mut tree = MemoryTree::new();
        for path in ["", "/a", "a/", "a//b", "./a", "a/..", "."] {
            assert_eq!(tree.add_file(path, ""), Err(InvalidPath(path.to_string())));
            assert_eq!(tree.add_directory(path), Err(InvalidPath(path.to_string())));
        }
        assert_eq!(listing(&tree, ""), Vec::new());
    }

    #[test]
    fn listing_a_file_or_a_missing_directory_fails() {
        let mut tree = MemoryTree::new();
        tree.add_file("a.txt", "").unwrap();
        assert_eq!(tree.list("a.txt"), Err(NotFound("a.txt".to_string())));
        assert_eq!(tree.list("nope"), Err(NotFound("nope".to_string())));
    }

    #[test]
    fn flint_files_are_found_and_read_in_any_directory() {
        let mut tree = MemoryTree::new();
        tree.add_file(".flint", "root rules").unwrap();
        tree.add_file("a/.flint", "nested rules").unwrap();
        assert!(tree.has_flint_file(""));
        assert!(tree.has_flint_file("a"));
        assert_eq!(tree.read_flint_file(""), Ok("root rules".to_string()));
        assert_eq!(tree.read_flint_file("a"), Ok("nested rules".to_string()));
    }

    #[test]
    fn a_missing_flint_file_or_a_flint_directory_is_not_one() {
        let mut tree = MemoryTree::new();
        tree.add_directory("a/.flint").unwrap();
        assert!(!tree.has_flint_file(""));
        assert!(!tree.has_flint_file("a"));
        assert_eq!(
            tree.read_flint_file(""),
            Err(NotFound(".flint".to_string()))
        );
        assert_eq!(
            tree.read_flint_file("a"),
            Err(NotFound("a/.flint".to_string()))
        );
    }

    #[test]
    fn errors_describe_the_path() {
        assert_eq!(
            InvalidPath("a//b".to_string()).to_string(),
            "invalid path: \"a//b\""
        );
        assert_eq!(NotFound("a".to_string()).to_string(), "a not found");
    }
}
