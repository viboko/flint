// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One whole flint run over a directory tree: finding every `.flint` file,
//! reading and parsing each, walking the directories each one governs, and
//! checking them — everything but printing the result and setting the exit
//! code, which stay in `main`.
//!
//! The tree itself is reached only through the [`DirectoryTree`] trait, so
//! this module stays filesystem-free: `main.rs` implements it over the real
//! disk, and [`crate::memory_tree::MemoryTree`] implements it in memory, for
//! the unit tests below, the fuzz target and the browser playground. Both
//! run exactly this code, so neither can drift from the other.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::checker::{self, ActualEntry, Violation};
use crate::entry_kind::EntryKind;
use crate::flint_file::{self, Entry, ParseError};
use crate::scope;

/// The name of the file flint looks for in every directory.
pub const FLINT_FILE_NAME: &str = ".flint";

/// Read access to a directory tree, as much as a run needs. Every
/// directory is named by its `/`-joined path relative to the tree's root,
/// or `""` for the root itself.
pub trait DirectoryTree {
    /// What went wrong reading part of the tree.
    type Error;

    /// The name and kind of every entry directly inside `dir`, in any
    /// order. A symlink is always [`EntryKind::File`], whatever it points
    /// at, so a run never follows one into another directory.
    ///
    /// # Errors
    /// Returns `Err` if `dir` can't be listed.
    fn list(&self, dir: &str) -> Result<Vec<(String, EntryKind)>, Self::Error>;

    /// Whether `dir` contains a `.flint` file that can be read as one —
    /// unlike [`Self::list`], following a symlink to see what it points
    /// at.
    fn has_flint_file(&self, dir: &str) -> bool;

    /// The contents of `dir`'s own `.flint` file.
    ///
    /// # Errors
    /// Returns `Err` if the file can't be read.
    fn read_flint_file(&self, dir: &str) -> Result<String, Self::Error>;
}

/// Everything one run found, one [`ScopeReport`] per `.flint` file, sorted
/// by scope path (so the root's own comes first). Empty if the root has no
/// `.flint` file of its own, in which case nothing else is looked for.
#[derive(Debug)]
pub struct Report<E> {
    /// One entry per directory containing a `.flint` file.
    pub scopes: Vec<ScopeReport<E>>,
    /// Every directory that couldn't be listed while looking for `.flint`
    /// files, so any inside it went unfound — sorted by path, and leaving
    /// out any a scope's own [`ScopeStatus::ListError`] already reports.
    pub skipped: Vec<Unlisted<E>>,
}

impl<E> Report<E> {
    /// Whether the root has a `.flint` file, every `.flint` file was read
    /// and parsed, and every directory matched its rules — what decides
    /// flint's exit code. A [`Self::skipped`] directory doesn't fail it:
    /// no `.flint` file found governs what's in it.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.scopes.is_empty()
            && self
                .scopes
                .iter()
                .all(|scope| matches!(&scope.status, ScopeStatus::Checked(v) if v.is_empty()))
    }
}

/// A directory that couldn't be listed, and why.
#[derive(Debug)]
pub struct Unlisted<E> {
    /// The directory, `""` for the root or a `/`-joined path relative to
    /// it.
    pub dir: String,
    /// What went wrong listing it.
    pub error: E,
}

/// The result for one scope: a directory containing its own `.flint` file.
#[derive(Debug)]
pub struct ScopeReport<E> {
    /// The scope's directory, `""` for the root or a `/`-joined path
    /// relative to it.
    pub scope: String,
    /// What checking it found.
    pub status: ScopeStatus<E>,
}

/// What a step that lists directories gives: `T`, or the first directory
/// it couldn't list.
type Listed<T, E> = Result<T, Unlisted<E>>;

/// What happened checking one scope.
#[derive(Debug)]
pub enum ScopeStatus<E> {
    /// The scope was checked; these are its violations, possibly none.
    /// Every path in them is relative to the scope's own directory.
    Checked(Vec<Violation>),
    /// The scope's `.flint` file couldn't be parsed.
    ParseError(ParseError),
    /// The scope's `.flint` file couldn't be read.
    ReadError(E),
    /// One of the directories the scope governs couldn't be listed, so
    /// the scope couldn't be checked.
    ListError(Unlisted<E>),
}

/// Runs flint over `tree`: finds every scope (`discover_scopes`),
/// reads and parses each one's `.flint` file, then checks each scope's
/// directory against its own rules plus any reaching ancestor's (see
/// [`scope::effective_origins`]).
///
/// Nothing is looked for unless the root has a `.flint` file of its own:
/// one there is what says the tree is meant to be checked, and saves
/// walking a whole tree that isn't (a home directory, say) to find out.
///
/// A failure inside one scope is reported as that scope's
/// [`ScopeStatus`], and the other scopes are still checked; a directory
/// that can't be listed while looking for scopes is noted in
/// [`Report::skipped`], and the search carries on past it.
pub fn run<T: DirectoryTree>(tree: &T) -> Report<T::Error> {
    if !tree.has_flint_file("") {
        return Report {
            scopes: Vec::new(),
            skipped: Vec::new(),
        };
    }
    let tree = &Composed::new(tree);
    let (scopes, mut skipped) = discover_scopes(tree);
    let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();

    // Every scope's own `.flint` file, read and parsed once up front — an
    // ancestor scope's entries are imported into any descendant scope
    // whose rules they reach into (see `check_scope`), reusing this same
    // parse rather than re-reading the ancestor's file once per reaching
    // descendant. A scope whose own file failed to read or parse
    // contributes nothing to any descendant that reaches into it (see
    // `crate::scope`'s module docs).
    let parsed: Vec<Result<Vec<Entry>, ScopeStatus<T::Error>>> = scope_refs
        .iter()
        .map(|&scope| parse_scope(tree, scope))
        .collect();
    let entries_by_scope: HashMap<&str, &[Entry]> = scope_refs
        .iter()
        .zip(&parsed)
        .filter_map(|(&scope, result)| result.as_ref().ok().map(|e| (scope, e.as_slice())))
        .collect();

    let mut checked: Vec<Option<ScopeStatus<T::Error>>> = scope_refs
        .iter()
        .zip(&parsed)
        .map(|(&scope, result)| {
            result.as_ref().ok().map(|own_entries| {
                match check_scope(tree, scope, own_entries, &scope_refs, &entries_by_scope) {
                    Ok(violations) => ScopeStatus::Checked(violations),
                    Err(unlisted) => ScopeStatus::ListError(unlisted),
                }
            })
        })
        .collect();
    report_missing_where_written(&scope_refs, &mut checked);
    let failed_scope_dirs: HashSet<&str> = checked
        .iter()
        .filter_map(|status| match status {
            Some(ScopeStatus::ListError(unlisted)) => Some(unlisted.dir.as_str()),
            _ => None,
        })
        .collect();
    skipped.retain(|unlisted| !failed_scope_dirs.contains(unlisted.dir.as_str()));

    let scopes = scopes
        .into_iter()
        .zip(parsed)
        .zip(checked)
        .map(|((scope, parsed), checked)| ScopeReport {
            scope,
            status: match (parsed, checked) {
                (_, Some(status)) | (Err(status), None) => status,
                (Ok(_), None) => unreachable!("every parsed scope was checked"),
            },
        })
        .collect();
    Report { scopes, skipped }
}

/// Moves each [`Violation::Missing`] that a scope's check found for a rule
/// imported from an ancestor `.flint` file to that ancestor's own
/// violations, named by its path from there. The scope's check is the one
/// that sees the folder's contents (see [`check_scope`]), but the rule is
/// the ancestor's: a reader looking for it in the scope's own `.flint`
/// file wouldn't find it. `statuses` holds each of `scopes`' results, in
/// the same order.
///
/// One is left where it was found only if the ancestor's own check
/// failed, so has no violations to add it to.
fn report_missing_where_written<E>(scopes: &[&str], statuses: &mut [Option<ScopeStatus<E>>]) {
    let checked: HashMap<&str, usize> = scopes
        .iter()
        .zip(statuses.iter())
        .enumerate()
        .filter(|(_, (_, status))| matches!(status, Some(ScopeStatus::Checked(_))))
        .map(|(index, (scope, _))| (*scope, index))
        .collect();

    let mut moved: Vec<(usize, Violation)> = Vec::new();
    for (scope, status) in scopes.iter().zip(statuses.iter_mut()) {
        let Some(ScopeStatus::Checked(violations)) = status else {
            continue;
        };
        violations.retain(|violation| {
            let Violation::Missing { name, origin } = violation else {
                return true;
            };
            let Some(&written_in) = checked.get(origin.scope.as_str()) else {
                return true;
            };
            if origin.scope == *scope {
                return true;
            }
            // `origin.scope` is an ancestor of `scope`, so a prefix of it.
            let folder = scope[origin.scope.len()..].trim_start_matches('/');
            moved.push((
                written_in,
                Violation::Missing {
                    name: join(folder, name),
                    origin: origin.clone(),
                },
            ));
            false
        });
    }

    for (written_in, violation) in moved {
        let Some(Some(ScopeStatus::Checked(violations))) = statuses.get_mut(written_in) else {
            unreachable!("only a checked scope is moved to");
        };
        let at = missing_position(violations, &violation);
        violations.insert(at, violation);
    }
}

/// Where among `violations` (ordered as [`checker::with_group_violations`]
/// leaves them) to add `missing`: with the other missing entries, in the
/// order of the lines that require them, then by name.
fn missing_position(violations: &[Violation], missing: &Violation) -> usize {
    let key = |violation: &Violation| match violation {
        Violation::Missing { name, origin } => Some((origin.line, name.clone())),
        Violation::GroupConflict { .. } | Violation::Unexpected(_) => None,
    };
    let new = key(missing);
    violations
        .iter()
        .position(|violation| match violation {
            Violation::GroupConflict { .. } => false,
            Violation::Missing { .. } => key(violation) > new,
            Violation::Unexpected(_) => true,
        })
        .unwrap_or(violations.len())
}

/// `text` with each accented letter as one character where Unicode has
/// one (its NFC form). The same letter can be stored either way — `é`, or
/// `e` followed by a combining accent, which is how macOS often stores a
/// file name — and the two look identical but don't compare equal.
fn compose(text: &str) -> Cow<'_, str> {
    if is_nfc(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}

/// A tree seen with every name and `.flint` file composed (see
/// [`compose`]), so that a rule matches a name however each happens to be
/// stored, and the rest of this module never meets the difference.
///
/// The one exception is a directory holding two names that compose to
/// the same thing, which Linux allows: composing would merge two
/// different entries, so each of them keeps the name it's stored under.
struct Composed<'a, T> {
    /// The tree as it's stored.
    tree: &'a T,
    /// The path each directory is stored under, by the path [`Self::list`]
    /// gave it — only for those where the two differ.
    stored: RefCell<HashMap<String, String>>,
}

impl<'a, T> Composed<'a, T> {
    /// `tree`, seen composed.
    fn new(tree: &'a T) -> Self {
        Composed {
            tree,
            stored: RefCell::new(HashMap::new()),
        }
    }

    /// The path `dir` is stored under in the tree itself. A run only ever
    /// asks about a directory after listing its parent, which is when a
    /// differing path is recorded.
    fn stored(&self, dir: &str) -> String {
        self.stored
            .borrow()
            .get(dir)
            .map_or_else(|| dir.to_string(), String::clone)
    }
}

impl<T: DirectoryTree> DirectoryTree for Composed<'_, T> {
    type Error = T::Error;

    fn list(&self, dir: &str) -> Result<Vec<(String, EntryKind)>, T::Error> {
        let stored_dir = self.stored(dir);
        let listing = self.tree.list(&stored_dir)?;
        if stored_dir == dir && listing.iter().all(|(name, _)| is_nfc(name)) {
            return Ok(listing); // nothing to compose, here or below
        }
        let composed: Vec<Cow<'_, str>> = listing.iter().map(|(name, _)| compose(name)).collect();
        let mut uses: HashMap<&str, usize> = HashMap::new();
        for name in &composed {
            *uses.entry(name).or_default() += 1;
        }
        let mut stored = self.stored.borrow_mut();
        let entries = listing
            .iter()
            .zip(&composed)
            .map(|((stored_name, kind), composed)| {
                let name = if uses.get(&**composed).is_some_and(|&count| count > 1) {
                    stored_name
                } else {
                    &**composed
                };
                let path = join(dir, name);
                let stored_path = join(&stored_dir, stored_name);
                // Only a directory's is ever asked for, but recording a
                // file's too does no harm.
                if path != stored_path {
                    stored.insert(path, stored_path);
                }
                (name.to_string(), *kind)
            })
            .collect();
        Ok(entries)
    }

    fn has_flint_file(&self, dir: &str) -> bool {
        self.tree.has_flint_file(&self.stored(dir))
    }

    fn read_flint_file(&self, dir: &str) -> Result<String, T::Error> {
        let contents = self.tree.read_flint_file(&self.stored(dir))?;
        Ok(compose(&contents).into_owned())
    }
}

/// Reads and parses `scope`'s own `.flint` file.
fn parse_scope<T: DirectoryTree>(
    tree: &T,
    scope: &str,
) -> Result<Vec<Entry>, ScopeStatus<T::Error>> {
    let contents = tree
        .read_flint_file(scope)
        .map_err(ScopeStatus::ReadError)?;
    flint_file::parse_in(&contents, scope).map_err(ScopeStatus::ParseError)
}

/// Checks `scope`'s own directory against its own entries, plus anything
/// imported from a reaching ancestor `.flint` file (see
/// [`scope::effective_origins`] for the cascading-`.flint`-file design
/// this implements), stopping short of any nested directory that is
/// itself a scope in `all_scopes` (see [`discover_scopes`] and
/// [`read_directory_entries`]). `parsed` holds every scope whose own
/// `.flint` file parsed.
fn check_scope<T: DirectoryTree>(
    tree: &T,
    scope: &str,
    own_entries: &[Entry],
    all_scopes: &[&str],
    parsed: &HashMap<&str, &[Entry]>,
) -> Listed<Vec<Violation>, T::Error> {
    let scope_set: HashSet<&str> = all_scopes.iter().copied().collect();
    let descendant_scopes = checker::nested_scopes_relative_to(&scope_set, scope);

    let ancestors: Vec<(&str, &[Entry])> = scope::ancestor_scopes(all_scopes, scope)
        .into_iter()
        .filter_map(|ancestor| parsed.get(ancestor).map(|entries| (ancestor, *entries)))
        .collect();

    let origins = scope::effective_origins(scope, own_entries, &ancestors, &descendant_scopes);

    // Governance (whether to recurse deeper while walking the directory,
    // and whether a directory reached that way counts as permitted) uses
    // every applicable origin's entries unioned together — any one of
    // them reaching into a subdirectory is enough to look inside it.
    let governance_entries: Vec<Entry> = origins
        .iter()
        .flat_map(|(_, entries)| entries.iter().cloned())
        .collect();

    let actual_entries =
        read_directory_entries(tree, scope, &governance_entries, &descendant_scopes)?;

    // Checking itself runs once per origin, against the same actual
    // entries, so each origin's `{{ variable }}` bindings stay isolated
    // exactly as they already are within a single `.flint` file — then
    // `merge_origin_violations` combines the results (see its own docs).
    let per_origin: Vec<Vec<Violation>> = origins
        .iter()
        .map(|(_, entries)| checker::check_rules(entries, &actual_entries))
        .collect();
    let rule_violations = checker::merge_origin_violations(&per_origin);

    let has_groups = own_entries
        .iter()
        .any(|entry| matches!(entry, Entry::Template(template) if !template.groups().is_empty()));
    if !has_groups {
        return Ok(rule_violations);
    }
    let imported_here: Vec<Entry> = origins
        .iter()
        .skip(1) // the scope's own entries come first
        .flat_map(|(_, entries)| entries.iter().cloned())
        .collect();
    let (conflicts, missing) = check_groups_below(
        tree,
        scope,
        own_entries,
        &governance_entries,
        &imported_here,
        &descendant_scopes,
        all_scopes,
        parsed,
    )?;
    Ok(checker::with_group_violations(
        rule_violations,
        conflicts,
        missing,
    ))
}

/// The group check for `scope`'s own `.flint` file (`own_entries`, with no
/// requirement suppressed), across everything below `scope` that
/// `governance_entries` reach — including the folders of nested `.flint`
/// files, which `check_scope` otherwise leaves to those files' own checks.
/// A group is only ever checked here, from the file that declared it, so
/// that a nested `.flint` file, even an empty one, can't split its files
/// between two checks that never see each other's.
///
/// A file still counts as allowed outside the group if any rule that
/// applies where it is allows it: `imported_here` (ancestors' rules
/// reaching into `scope`) for one in `scope`'s own check, or the rules of
/// the nested `.flint` file whose check it's in (its own, and those
/// imported into it from anywhere but `scope`, whose own rules
/// `checker::check_groups` already considers).
#[allow(clippy::too_many_arguments)] // all needed, and private to this module
fn check_groups_below<T: DirectoryTree>(
    tree: &T,
    scope: &str,
    own_entries: &[Entry],
    governance_entries: &[Entry],
    imported_here: &[Entry],
    descendant_scopes: &HashSet<String>,
    all_scopes: &[&str],
    parsed: &HashMap<&str, &[Entry]>,
) -> Listed<(Vec<Violation>, Vec<Violation>), T::Error> {
    let everything: Vec<ActualEntry> =
        read_directory_entries(tree, scope, governance_entries, &HashSet::new())?
            .into_iter()
            .filter(|actual| !is_nested_flint_file(actual, descendant_scopes))
            .collect();

    let scope_set: HashSet<&str> = all_scopes.iter().copied().collect();
    let nested_rules: Vec<(&str, Vec<Entry>)> = descendant_scopes
        .iter()
        .map(|nested| {
            let nested_scope = join(scope, nested);
            let ancestors: Vec<(&str, &[Entry])> =
                scope::ancestor_scopes(all_scopes, &nested_scope)
                    .into_iter()
                    .filter(|ancestor| *ancestor != scope)
                    .filter_map(|ancestor| parsed.get(ancestor).map(|entries| (ancestor, *entries)))
                    .collect();
            let own = parsed
                .get(nested_scope.as_str())
                .copied()
                .unwrap_or_default();
            let below = checker::nested_scopes_relative_to(&scope_set, &nested_scope);
            let rules = scope::effective_origins(&nested_scope, own, &ancestors, &below)
                .into_iter()
                .flat_map(|(_, entries)| entries)
                .collect();
            (nested.as_str(), rules)
        })
        .collect();

    let allowed_elsewhere = |actual: &ActualEntry| {
        let owner = nested_rules
            .iter()
            .filter(|(nested, _)| {
                actual
                    .name
                    .strip_prefix(*nested)
                    .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|(nested, _)| nested.len());
        match owner {
            Some((nested, rules)) => {
                let relative = ActualEntry {
                    name: actual.name[nested.len() + 1..].to_string(),
                    kind: actual.kind,
                };
                rules
                    .iter()
                    .any(|entry| checker::entry_allows(entry, &relative))
            }
            None => imported_here
                .iter()
                .any(|entry| checker::entry_allows(entry, actual)),
        }
    };
    Ok(checker::check_groups(
        own_entries,
        &everything,
        allowed_elsewhere,
    ))
}

/// Whether `actual` is the `.flint` file of one of `nested_scopes`, which
/// is never one of the files a rule describes.
fn is_nested_flint_file(actual: &ActualEntry, nested_scopes: &HashSet<String>) -> bool {
    actual.kind == EntryKind::File
        && actual
            .name
            .strip_suffix(&format!("/{FLINT_FILE_NAME}"))
            .is_some_and(|folder| nested_scopes.contains(folder))
}

/// `name` inside `dir`, both in this module's `/`-joined path convention,
/// where `""` names the root — so joining either side with `""` leaves the
/// other unchanged.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Recursively finds every directory containing a `.flint` file — these
/// are the "scopes" [`check_scope`] evaluates, each independently of the
/// others. The root is always considered, regardless of its name; every
/// other directory is identified by its `/`-joined path relative to it.
/// Sorted, so the root comes first.
///
/// Symlinked directories are never followed, since [`DirectoryTree::list`]
/// reports every symlink as a file — the same mechanism
/// [`read_directory_entries`] relies on to never recurse into one.
/// Dot-named directories (`.github`, `.config`, ...) are searched like any
/// other: what's in them is as much a part of the project's layout.
///
/// A directory that can't be listed is skipped, along with everything
/// in it, and returned second, sorted by path — so one the user can't
/// read, such as a folder macOS keeps private, doesn't stop the rest of
/// the tree being checked.
fn discover_scopes<T: DirectoryTree>(tree: &T) -> (Vec<String>, Vec<Unlisted<T::Error>>) {
    let mut scopes = Vec::new();
    let mut skipped = Vec::new();
    discover_scopes_at(tree, "", &mut scopes, &mut skipped);
    scopes.sort();
    skipped.sort_by(|a, b| a.dir.cmp(&b.dir));
    (scopes, skipped)
}

/// Recursive helper for [`discover_scopes`].
fn discover_scopes_at<T: DirectoryTree>(
    tree: &T,
    dir: &str,
    scopes: &mut Vec<String>,
    skipped: &mut Vec<Unlisted<T::Error>>,
) {
    if tree.has_flint_file(dir) {
        scopes.push(dir.to_string());
    }
    match tree.list(dir) {
        Ok(listing) => {
            for (name, kind) in listing {
                if kind == EntryKind::Directory {
                    discover_scopes_at(tree, &join(dir, &name), scopes, skipped);
                }
            }
        }
        Err(error) => skipped.push(Unlisted {
            dir: dir.to_string(),
            error,
        }),
    }
}

/// Lists the entries inside `scope`'s directory, as paths relative to it,
/// excluding its own `.flint` file and recursing into every subdirectory
/// some `entries` rule permits (see [`checker::permits_directory`]), so
/// that its contents are checked too — a directory no rule permits is
/// reported unexpected on its own, without its contents. Recursion also
/// stops at any directory listed in `nested_scopes`: that directory owns
/// its own `.flint` file and is checked by its own, independent
/// [`check_scope`] pass instead, not folded into this one.
fn read_directory_entries<T: DirectoryTree>(
    tree: &T,
    scope: &str,
    entries: &[Entry],
    nested_scopes: &HashSet<String>,
) -> Listed<Vec<ActualEntry>, T::Error> {
    let mut actual = Vec::new();
    collect_entries(tree, scope, "", entries, nested_scopes, &mut actual)?;
    Ok(actual)
}

/// Recursive helper for [`read_directory_entries`]. `relative_prefix` is
/// `""` at the scope's own root and a `/`-joined path (e.g. `"src"`,
/// `"src/linux"`) for anything nested; the `.flint` file is only excluded
/// at the root, since a nested directory can't contain the scope's own
/// `.flint` file being read.
fn collect_entries<T: DirectoryTree>(
    tree: &T,
    scope: &str,
    relative_prefix: &str,
    entries: &[Entry],
    nested_scopes: &HashSet<String>,
    out: &mut Vec<ActualEntry>,
) -> Listed<(), T::Error> {
    let dir = join(scope, relative_prefix);
    let listing = tree.list(&dir).map_err(|error| Unlisted { dir, error })?;
    for (name, kind) in listing {
        if relative_prefix.is_empty() && name == FLINT_FILE_NAME {
            continue;
        }
        let relative_path = join(relative_prefix, &name);
        if kind == EntryKind::Directory
            && checker::permits_directory(entries, &relative_path)
            && !nested_scopes.contains(&relative_path)
        {
            collect_entries(tree, scope, &relative_path, entries, nested_scopes, out)?;
        }
        out.push(ActualEntry {
            name: relative_path,
            kind,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_tree::{MemoryTree, NotFound};
    use crate::origin::Origin;

    /// A tree with `files` (each `(path, contents)`) and `dirs` in it.
    fn tree(files: &[(&str, &str)], dirs: &[&str]) -> MemoryTree {
        let mut tree = MemoryTree::new();
        for (path, contents) in files {
            tree.add_file(path, contents).unwrap();
        }
        for path in dirs {
            tree.add_directory(path).unwrap();
        }
        tree
    }

    fn checked(report: &Report<NotFound>) -> Vec<(&str, &[Violation])> {
        report
            .scopes
            .iter()
            .map(|scope| match &scope.status {
                ScopeStatus::Checked(violations) => (scope.scope.as_str(), violations.as_slice()),
                other => panic!("{} wasn't checked: {other:?}", scope.scope),
            })
            .collect()
    }

    fn unexpected(name: &str) -> Violation {
        Violation::Unexpected(name.to_string())
    }

    #[test]
    fn a_tree_without_a_flint_file_has_no_scopes_and_fails() {
        let report = run(&tree(&[("a.txt", "")], &[]));
        assert!(report.scopes.is_empty());
        assert!(!report.passed());
    }

    #[test]
    fn without_a_flint_file_at_the_root_nothing_else_is_looked_for() {
        // Listing anything would fail, and be noted in `skipped`.
        let failing = FailingTree {
            inner: tree(&[("a/.flint", "x.txt"), ("a/y.txt", "")], &[]),
            broken_dir: "",
            broken_flint: "none",
        };
        let report = run(&failing);
        assert!(report.scopes.is_empty());
        assert!(report.skipped.is_empty());
        assert!(!report.passed());
    }

    #[test]
    fn a_group_reaching_into_a_folder_with_its_own_flint_file_is_checked_from_its_own() {
        let tree = tree(
            &[
                (
                    ".flint",
                    "src/\n    <name>\n        {{ name }}.md\n        {{ name }}.txt\n",
                ),
                ("src/.flint", ""),
                ("src/a.md", ""),
                ("src/b.txt", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![
                (
                    String::new(),
                    vec!["conflict src/<name> in src".to_string()]
                ),
                ("src".to_string(), vec![]),
            ]
        );
    }

    /// The group in [`a_group_s_files_on_both_sides_of_a_nested_flint_file_must_agree`]
    /// and friends: `docs/` captures nothing, so only the group ties
    /// `{{ name }}.txt` to `docs/{{ name }}.md`.
    const SPLIT_GROUP: &str = "<name>\n    {{ name }}.txt\n    docs/\n        {{ name }}.md\n";

    #[test]
    fn a_group_s_files_on_both_sides_of_a_nested_flint_file_must_agree() {
        for nested in [None, Some(""), Some("# docs has its own rules\n")] {
            let mut files = vec![(".flint", SPLIT_GROUP), ("a.txt", ""), ("docs/b.md", "")];
            if let Some(contents) = nested {
                files.push(("docs/.flint", contents));
            }
            assert!(
                matches!(
                    &violations_in(&tree(&files, &[]))[0].1[..],
                    [conflict] if conflict == "conflict <name> in "
                ),
                "with docs/.flint {nested:?}"
            );
        }
    }

    /// `inner`, except that listing `failing` fails from its `fail_from`th
    /// listing on (1-based), so a test can let earlier passes over the
    /// tree succeed.
    struct FailingList<'a> {
        inner: &'a MemoryTree,
        failing: &'a str,
        fail_from: usize,
        listings: std::cell::Cell<usize>,
    }

    impl DirectoryTree for FailingList<'_> {
        type Error = NotFound;

        fn list(&self, dir: &str) -> Result<Vec<(String, EntryKind)>, NotFound> {
            if dir == self.failing {
                self.listings.set(self.listings.get() + 1);
                if self.listings.get() >= self.fail_from {
                    return Err(NotFound(dir.to_string()));
                }
            }
            self.inner.list(dir)
        }

        fn has_flint_file(&self, dir: &str) -> bool {
            self.inner.has_flint_file(dir)
        }

        fn read_flint_file(&self, dir: &str) -> Result<String, NotFound> {
            self.inner.read_flint_file(dir)
        }
    }

    #[test]
    fn a_folder_the_group_check_cannot_list_fails_its_flint_file_s_check() {
        // `docs/` is listed while finding `.flint` files, then by the
        // root's group check — its own `.flint` file's check comes later.
        let inner = tree(
            &[
                (".flint", SPLIT_GROUP),
                ("docs/.flint", ""),
                ("a.txt", ""),
                ("docs/a.md", ""),
            ],
            &[],
        );
        let tree = FailingList {
            inner: &inner,
            failing: "docs",
            fail_from: 2,
            listings: std::cell::Cell::new(0),
        };
        let report = run(&tree);
        assert!(matches!(
            &report.scopes[0].status,
            ScopeStatus::ListError(Unlisted { dir, .. }) if dir == "docs"
        ));
    }

    #[test]
    fn a_nested_flint_file_is_never_one_of_a_group_s_files() {
        // `docs/{{ name }}` would read `docs/.flint` as `name` = `.flint`.
        let tree = tree(
            &[
                (
                    ".flint",
                    "<name>\n    {{ name }}.txt\n    docs/\n        {{ name }}\n",
                ),
                ("docs/.flint", ""),
                ("a.txt", ""),
                ("docs/a", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![(String::new(), vec![]), ("docs".to_string(), vec![])]
        );
    }

    #[test]
    fn a_nested_flint_file_s_own_rule_takes_a_file_out_of_the_group() {
        let tree = tree(
            &[
                (".flint", SPLIT_GROUP),
                ("docs/.flint", "README.md\n"),
                ("a.txt", ""),
                ("docs/a.md", ""),
                ("docs/README.md", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![(String::new(), vec![]), ("docs".to_string(), vec![])]
        );
    }

    #[test]
    fn a_rule_imported_from_further_up_takes_a_file_out_of_a_nested_group() {
        // `pkg/.flint`'s group sees `pkg/README.md`, but the root's
        // `pkg/README.md` rule, imported into `pkg/`, allows it.
        let tree = tree(
            &[
                (".flint", "pkg/README.md\n"),
                ("pkg/.flint", "<name>\n    {{ name }}.md\n"),
                ("pkg/a.md", ""),
                ("pkg/README.md", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![(String::new(), vec![]), ("pkg".to_string(), vec![])]
        );
    }

    #[test]
    fn a_required_group_header_is_checked_once_from_its_own_flint_file() {
        let flint = ".flint";
        let with = |files: &[&str]| {
            let mut all: Vec<(&str, &str)> = vec![
                (flint, "src/\n    !<name>\n        {{ name }}.md\n"),
                ("src/.flint", ""),
            ];
            all.extend(files.iter().map(|file| (*file, "")));
            violations_in(&tree(&all, &["src"]))
        };
        assert_eq!(
            with(&["src/a.md"]),
            vec![(String::new(), vec![]), ("src".to_string(), vec![])]
        );
        assert_eq!(
            with(&[]),
            vec![
                (String::new(), vec!["missing src/<name>".to_string()]),
                ("src".to_string(), vec![]),
            ]
        );
    }

    /// Every scope's violations in `tree`, as `(scope, violation texts)`,
    /// each violation as its `Debug` text — enough to tell which entry
    /// and why, without spelling out every field.
    fn violations_in(tree: &MemoryTree) -> Vec<(String, Vec<String>)> {
        checked(&run(tree))
            .into_iter()
            .map(|(scope, violations)| {
                (
                    scope.to_string(),
                    violations
                        .iter()
                        .map(|violation| match violation {
                            Violation::Missing { name, .. } => format!("missing {name}"),
                            Violation::Unexpected(name) => format!("unexpected {name}"),
                            Violation::GroupConflict { group, folder, .. } => {
                                format!("conflict {group} in {folder}")
                            }
                        })
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_flint_file_in_one_folder_leaves_a_required_rule_checked_in_the_others() {
        let root = "{{ name|snake }}/\n    !{{ name|pascal }}\n";
        let without = tree(&[(".flint", root)], &["blah", "cafe"]);
        assert_eq!(
            violations_in(&without),
            vec![(
                String::new(),
                vec![
                    "missing blah/Blah".to_string(),
                    "missing cafe/Cafe".to_string()
                ]
            )]
        );
        let with = tree(&[(".flint", root), ("cafe/.flint", "")], &["blah"]);
        assert_eq!(
            violations_in(&with),
            vec![
                (
                    String::new(),
                    vec![
                        "missing blah/Blah".to_string(),
                        "missing cafe/Cafe".to_string()
                    ]
                ),
                ("cafe".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn a_folder_s_value_carries_into_its_own_flint_file_s_check() {
        let tree = tree(
            &[
                (".flint", "{{ name }}/\n    {{ name }}.swift\n"),
                ("Home/.flint", ""),
                ("Home/Home.swift", ""),
                ("Home/Other.swift", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![
                (String::new(), vec![]),
                (
                    "Home".to_string(),
                    vec!["unexpected Other.swift".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn a_folder_s_value_carries_across_formats() {
        let tree = tree(
            &[
                (".flint", "{{ name|camel }}/\n    {{ name|pascal }}.swift\n"),
                ("foo/.flint", ""),
                ("foo/Foo.swift", ""),
                ("foo/Bar.swift", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree)[1].1,
            vec!["unexpected Bar.swift".to_string()]
        );
    }

    #[test]
    fn a_folder_s_value_carries_into_a_group_member_imported_below_the_group() {
        let tree = tree(
            &[
                (
                    ".flint",
                    "<name>\n    {{ name }}/\n        {{ name }}.md\n    {{ name }}.txt\n",
                ),
                ("Home/.flint", ""),
                ("Home.txt", ""),
                ("Home/Other.md", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree)[1].1,
            vec!["unexpected Other.md".to_string()]
        );
    }

    #[test]
    fn an_imported_required_rule_is_reported_from_the_flint_file_that_wrote_it() {
        let tree = tree(
            &[
                (
                    ".flint",
                    "!src/*/main.py\n{{ x }}/\n    !README.md\n    !{{ x }}.swift\n",
                ),
                ("src/.flint", ""),
            ],
            &["src/a"],
        );
        assert_eq!(
            violations_in(&tree),
            vec![
                (
                    String::new(),
                    vec![
                        "missing src/a/main.py".to_string(),
                        "missing src/README.md".to_string(),
                        "missing src/src.swift".to_string(),
                    ]
                ),
                ("src".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn an_imported_missing_entry_sits_among_its_flint_file_s_own_in_rule_order() {
        let tree = tree(
            &[
                (
                    ".flint",
                    "!first.txt\n!b/second.txt\n!b/third.txt\n!last.txt\nb/\nstray/\n",
                ),
                ("b/.flint", ""),
                ("extra.txt", ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![
                (
                    String::new(),
                    vec![
                        "missing first.txt".to_string(),
                        "missing b/second.txt".to_string(),
                        "missing b/third.txt".to_string(),
                        "missing last.txt".to_string(),
                        "unexpected extra.txt".to_string(),
                    ]
                ),
                ("b".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn an_imported_missing_entry_stays_put_if_its_own_flint_file_s_check_failed() {
        // The root's own check can't list `other`, so it has no violations
        // to add `src/util.rs` to.
        let inner = tree(
            &[(".flint", "!src/util.rs\nother/\n"), ("src/.flint", "")],
            &["other"],
        );
        let failing = FailingList {
            inner: &inner,
            failing: "other",
            fail_from: 2,
            listings: std::cell::Cell::new(0),
        };
        let report = run(&failing);
        assert!(matches!(
            &report.scopes[0].status,
            ScopeStatus::ListError(_)
        ));
        assert!(matches!(
            &report.scopes[1].status,
            ScopeStatus::Checked(violations)
                if matches!(&violations[..], [Violation::Missing { name, .. }] if name == "util.rs")
        ));
    }

    #[test]
    fn an_imported_required_rule_checked_per_folder_uses_the_carried_value() {
        let tree = tree(
            &[
                (
                    ".flint",
                    "{{ a }}/\n    {{ b }}/\n        !{{ a }}-{{ b }}.txt\n",
                ),
                ("p/.flint", ""),
            ],
            &["p/q"],
        );
        assert_eq!(
            violations_in(&tree)[0],
            (String::new(), vec!["missing p/q/p-q.txt".to_string()])
        );
    }

    #[test]
    fn the_root_flint_file_is_never_reported_as_unexpected() {
        let report = run(&tree(&[(".flint", "a.txt"), ("a.txt", "")], &[]));
        assert_eq!(checked(&report), vec![("", &[][..])]);
        assert!(report.passed());
    }

    #[test]
    fn unlisted_entries_are_reported_and_fail_the_run() {
        let report = run(&tree(&[(".flint", "a.txt"), ("b.txt", "")], &["c"]));
        assert_eq!(
            checked(&report),
            vec![("", &[unexpected("b.txt"), unexpected("c")][..])]
        );
        assert!(!report.passed());
    }

    #[test]
    fn a_bare_directory_rule_permits_only_an_empty_directory() {
        let report = run(&tree(&[(".flint", "src/"), ("src/anything.rs", "")], &[]));
        assert_eq!(
            checked(&report),
            vec![("", &[unexpected("src/anything.rs")][..])]
        );
        let report = run(&tree(&[(".flint", "src/")], &["src"]));
        assert!(report.passed());
    }

    #[test]
    fn a_directory_allowed_by_a_template_is_walked_too() {
        let files = [
            (".flint", "{{ name }}/\n*/\n"),
            ("a/x.txt", ""),
            ("b/y.txt", ""),
        ];
        assert_eq!(
            checked(&run(&tree(&files, &[]))),
            vec![("", &[unexpected("a/x.txt"), unexpected("b/y.txt")][..])]
        );
    }

    #[test]
    fn a_trailing_any_depth_allows_everything_in_a_directory_dot_names_included() {
        let files = [
            (".flint", "target/**\n"),
            ("target/a.rs", ""),
            ("target/deep/er/b.rs", ""),
            ("target/.rustc_info.json", ""),
            ("target/debug/.fingerprint/c", ""),
        ];
        assert!(run(&tree(&files, &[])).passed());
    }

    #[test]
    fn any_depth_with_more_after_it_never_reaches_into_a_dot_folder() {
        let files = [
            (".flint", "src/**/*.rs\n"),
            ("src/a.rs", ""),
            ("src/deep/b.rs", ""),
            ("src/.cache/c.rs", ""),
        ];
        assert_eq!(
            checked(&run(&tree(&files, &[]))),
            vec![("", &[unexpected("src/.cache")][..])]
        );
    }

    #[test]
    fn a_dot_named_directory_is_walked_like_any_other() {
        let files = [(".flint", ".github/\n"), (".github/stray.yml", "")];
        assert_eq!(
            checked(&run(&tree(&files, &[]))),
            vec![("", &[unexpected(".github/stray.yml")][..])]
        );
    }

    #[test]
    fn an_unexpected_directory_s_contents_are_not_listed() {
        let files = [
            (".flint", "a.txt\n"),
            ("a.txt", ""),
            ("junk/one", ""),
            ("junk/two", ""),
        ];
        assert_eq!(
            checked(&run(&tree(&files, &[]))),
            vec![("", &[unexpected("junk")][..])]
        );
    }

    #[test]
    fn a_governed_directory_is_walked() {
        let files = [
            (".flint", "src/main.rs"),
            ("src/main.rs", ""),
            ("src/extra.rs", ""),
        ];
        let report = run(&tree(&files, &[]));
        assert_eq!(
            checked(&report),
            vec![("", &[unexpected("src/extra.rs")][..])]
        );
    }

    #[test]
    fn only_the_scope_s_own_flint_file_is_skipped() {
        // A `.flint` *directory* in a governed subdirectory isn't a scope's
        // file (it can't be read as one), so it's reported like anything
        // else would be.
        let report = run(&tree(&[(".flint", "src/a.rs")], &["src/.flint"]));
        assert_eq!(
            checked(&report),
            vec![("", &[unexpected("src/.flint")][..])]
        );
    }

    #[test]
    fn nested_scopes_are_found_at_any_depth_and_sorted() {
        let files = [("z/.flint", ""), ("a/b/c/.flint", ""), (".flint", "a/\nz/")];
        let report = run(&tree(&files, &[]));
        let scopes: Vec<&str> = report.scopes.iter().map(|s| s.scope.as_str()).collect();
        assert_eq!(scopes, vec!["", "a/b/c", "z"]);
    }

    #[test]
    fn a_tree_with_only_nested_flint_files_is_not_checked() {
        let report = run(&tree(&[("a/.flint", "x.txt"), ("a/y.txt", "")], &[]));
        assert!(report.scopes.is_empty());
        assert!(!report.passed());
    }

    #[test]
    fn dot_named_directories_are_searched_for_scopes() {
        let report = run(&tree(&[(".flint", ".git/"), (".git/.flint", "")], &[]));
        let scopes: Vec<&str> = report.scopes.iter().map(|s| s.scope.as_str()).collect();
        assert_eq!(scopes, vec!["", ".git"]);
    }

    #[test]
    fn a_dot_named_file_does_not_stop_scope_discovery() {
        // A file is never recursed into, dot-named or not.
        let report = run(&tree(
            &[(".flint", "a/"), (".hidden", ""), ("a/.flint", "")],
            &[],
        ));
        let scopes: Vec<&str> = report.scopes.iter().map(|s| s.scope.as_str()).collect();
        assert_eq!(scopes, vec!["", "a"]);
    }

    #[test]
    fn a_directory_named_flint_is_not_a_scope() {
        let report = run(&tree(&[], &[".flint"]));
        assert!(report.scopes.is_empty());
    }

    #[test]
    fn a_nested_scope_is_not_walked_by_its_parent() {
        let files = [
            (".flint", "packages/foo/README.md"),
            ("packages/foo/.flint", "README.md"),
            ("packages/foo/README.md", ""),
            ("packages/foo/stray.txt", ""),
        ];
        let report = run(&tree(&files, &[]));
        assert_eq!(
            checked(&report),
            vec![
                ("", &[][..]),
                ("packages/foo", &[unexpected("stray.txt")][..])
            ]
        );
    }

    #[test]
    fn a_reaching_ancestor_rule_also_permits_entries_in_a_nested_scope() {
        let files = [
            (".flint", "packages/\n    foo/\n        LICENSE"),
            ("packages/foo/.flint", "README.md"),
            ("packages/foo/README.md", ""),
            ("packages/foo/LICENSE", ""),
        ];
        let report = run(&tree(&files, &[]));
        assert_eq!(
            checked(&report),
            vec![("", &[][..]), ("packages/foo", &[][..])]
        );
    }

    #[test]
    fn a_reaching_ancestor_rule_governs_walking_a_nested_scope() {
        // Only the ancestor's rule reaches into `src/`, so only it can make
        // the nested scope's walk look inside.
        let files = [
            (".flint", "a/\n    src/\n        main.rs"),
            ("a/.flint", "src/"),
            ("a/src/main.rs", ""),
            ("a/src/extra.rs", ""),
        ];
        let report = run(&tree(&files, &[]));
        assert_eq!(
            checked(&report),
            vec![("", &[][..]), ("a", &[unexpected("src/extra.rs")][..])]
        );
    }

    #[test]
    fn a_parse_error_is_reported_for_its_own_scope_only() {
        let files = [(".flint", "a/\nb.txt"), ("a/.flint", "{{"), ("c.txt", "")];
        let report = run(&tree(&files, &[]));
        assert!(matches!(
            &report.scopes[0].status,
            ScopeStatus::Checked(v) if *v == [unexpected("c.txt")]
        ));
        assert_eq!(report.scopes[1].scope, "a");
        assert!(matches!(
            &report.scopes[1].status,
            ScopeStatus::ParseError(err) if err.line_number == 1
        ));
        assert!(!report.passed());
    }

    #[test]
    fn an_ancestor_that_failed_to_parse_contributes_nothing() {
        let files = [
            (".flint", "{{\na/\n    b.txt"),
            ("a/.flint", ""),
            ("a/b.txt", ""),
        ];
        let report = run(&tree(&files, &[]));
        assert!(matches!(
            report.scopes[0].status,
            ScopeStatus::ParseError(_)
        ));
        assert!(matches!(
            &report.scopes[1].status,
            ScopeStatus::Checked(v) if *v == [unexpected("b.txt")]
        ));
    }

    #[test]
    fn a_passing_run_needs_every_scope_clean() {
        let files = [(".flint", "a/"), ("a/.flint", ""), ("a/b.txt", "")];
        let report = run(&tree(&files, &[]));
        assert!(!report.passed());
        let files = [(".flint", "a/"), ("a/.flint", "b.txt"), ("a/b.txt", "")];
        assert!(run(&tree(&files, &[])).passed());
    }

    /// A [`MemoryTree`] that fails to list `broken_dir`, and to read
    /// `broken_flint`'s `.flint` file, as a real disk can.
    struct FailingTree {
        inner: MemoryTree,
        broken_dir: &'static str,
        broken_flint: &'static str,
    }

    impl DirectoryTree for FailingTree {
        type Error = NotFound;

        fn list(&self, dir: &str) -> Result<Vec<(String, EntryKind)>, NotFound> {
            if dir == self.broken_dir {
                return Err(NotFound(dir.to_string()));
            }
            self.inner.list(dir)
        }

        fn has_flint_file(&self, dir: &str) -> bool {
            self.inner.has_flint_file(dir)
        }

        fn read_flint_file(&self, dir: &str) -> Result<String, NotFound> {
            if dir == self.broken_flint {
                return Err(NotFound(dir.to_string()));
            }
            self.inner.read_flint_file(dir)
        }
    }

    #[test]
    fn a_directory_that_cannot_be_listed_while_finding_scopes_is_skipped() {
        // `sub` isn't one the root's rules look inside, so only finding
        // scopes lists it.
        let failing = FailingTree {
            inner: tree(&[(".flint", "z/"), ("z/.flint", "")], &["sub"]),
            broken_dir: "sub",
            broken_flint: "none",
        };
        let report = run(&failing);
        assert_eq!(
            checked(&report),
            vec![("", &[unexpected("sub")][..]), ("z", &[][..])]
        );
        let skipped: Vec<(&str, &NotFound)> = report
            .skipped
            .iter()
            .map(|unlisted| (unlisted.dir.as_str(), &unlisted.error))
            .collect();
        assert_eq!(skipped, vec![("sub", &NotFound("sub".to_string()))]);
    }

    #[test]
    fn a_directory_that_cannot_be_listed_is_not_skipped_where_its_scope_fails() {
        // `.config` fails both while finding scopes and once the walk
        // reaches it; the root's own failure is the one reported.
        let failing = FailingTree {
            inner: tree(&[(".flint", ".config/x")], &[".config"]),
            broken_dir: ".config",
            broken_flint: "none",
        };
        let report = run(&failing);
        assert!(matches!(
            &report.scopes[0].status,
            ScopeStatus::ListError(Unlisted { dir, .. }) if dir == ".config"
        ));
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn a_root_that_cannot_be_listed_fails_its_own_check() {
        let failing = FailingTree {
            inner: tree(&[(".flint", "")], &[]),
            broken_dir: "",
            broken_flint: "none",
        };
        let report = run(&failing);
        assert!(matches!(
            &report.scopes[..],
            [ScopeReport { status: ScopeStatus::ListError(Unlisted { dir, .. }), .. }]
                if dir.is_empty()
        ));
        assert!(report.skipped.is_empty());
        assert!(!report.passed());
    }

    #[test]
    fn failing_to_read_a_flint_file_is_reported_for_its_scope() {
        let failing = FailingTree {
            inner: tree(&[(".flint", "a/"), ("a/.flint", "")], &[]),
            broken_dir: "none",
            broken_flint: "a",
        };
        let report = run(&failing);
        assert!(matches!(report.scopes[0].status, ScopeStatus::Checked(_)));
        assert!(matches!(
            &report.scopes[1].status,
            ScopeStatus::ReadError(NotFound(dir)) if dir == "a"
        ));
        assert!(!report.passed());
    }

    #[test]
    fn failing_to_list_a_governed_directory_is_reported_for_its_scope() {
        // `.config` is listed fine while finding `.flint` files, and fails
        // once the walk reaches it.
        let inner = tree(&[(".flint", ".config/x")], &[".config"]);
        let failing = FailingList {
            inner: &inner,
            failing: ".config",
            fail_from: 2,
            listings: std::cell::Cell::new(0),
        };
        let report = run(&failing);
        assert!(matches!(
            &report.scopes[0].status,
            ScopeStatus::ListError(Unlisted { dir, .. }) if dir == ".config"
        ));
        assert!(!report.passed());
    }

    #[test]
    fn a_violation_points_at_the_flint_file_and_line_its_rule_came_from() {
        let files = [
            (".flint", "a/\n    !LICENSE"),
            ("a/.flint", "# readme\n!README.md"),
        ];
        let report = run(&tree(&files, &[]));
        let origin = |scope: &str, line| Origin {
            scope: scope.to_string(),
            line,
        };
        assert_eq!(
            checked(&report),
            vec![
                (
                    "",
                    &[Violation::Missing {
                        name: "a/LICENSE".to_string(),
                        origin: origin("", 2),
                    }][..]
                ),
                (
                    "a",
                    &[Violation::Missing {
                        name: "README.md".to_string(),
                        origin: origin("a", 2),
                    }][..]
                ),
            ]
        );
    }

    /// The violations of a tree with one `.flint` file, at its root.
    fn root_violations(flint: &str, files: &[&str], dirs: &[&str]) -> Vec<String> {
        let mut all: Vec<(&str, &str)> = vec![(".flint", flint)];
        all.extend(files.iter().map(|path| (*path, "")));
        let mut scopes = violations_in(&tree(&all, dirs));
        assert_eq!(scopes.len(), 1);
        scopes.remove(0).1
    }

    /// `café`, with `é` as one character — how a keyboard types it.
    const COMPOSED: &str = "caf\u{e9}";
    /// `café`, with `é` as `e` and a combining accent — how macOS often
    /// stores it.
    const DECOMPOSED: &str = "cafe\u{301}";

    #[test]
    fn a_name_matches_however_its_accents_are_stored() {
        for (rule, on_disk) in [
            (COMPOSED, DECOMPOSED),
            (DECOMPOSED, COMPOSED),
            (COMPOSED, COMPOSED),
            (DECOMPOSED, DECOMPOSED),
        ] {
            let file = format!("{on_disk}.txt");
            for flint in [format!("{rule}.txt\n"), format!("!{rule}.txt\n")] {
                assert_eq!(
                    root_violations(&flint, &[&file], &[]),
                    Vec::<String>::new(),
                    "{flint:?} against {file:?}"
                );
            }
            assert_eq!(
                root_violations(&format!("{rule}/\n"), &[], &[on_disk]),
                Vec::<String>::new(),
                "{rule:?}/ against {on_disk:?}/"
            );
        }
    }

    #[test]
    fn a_name_is_reported_with_its_accents_composed() {
        assert_eq!(
            root_violations("", &["nai\u{308}ve.txt"], &[DECOMPOSED]),
            vec![
                format!("unexpected {COMPOSED}"),
                "unexpected na\u{ef}ve.txt".to_string()
            ]
        );
        assert_eq!(
            root_violations(&format!("!{DECOMPOSED}.txt\n"), &[], &[]),
            vec![format!("missing {COMPOSED}.txt")]
        );
    }

    #[test]
    fn a_folder_stored_decomposed_is_still_looked_inside() {
        let flint = format!("{COMPOSED}/\n    menu.txt\n    {COMPOSED}/\n        !prix.txt\n");
        let menu = format!("{DECOMPOSED}/menu.txt");
        let stray = format!("{DECOMPOSED}/stray.txt");
        let inner = format!("{DECOMPOSED}/{DECOMPOSED}");
        assert_eq!(
            root_violations(&flint, &[&menu, &stray], &[&inner]),
            vec![
                format!("missing {COMPOSED}/{COMPOSED}/prix.txt"),
                format!("unexpected {COMPOSED}/stray.txt"),
            ]
        );
        // A plainly named folder inside it is stored under a different
        // path than it's known by, too.
        let deep = format!("{DECOMPOSED}/plain/deeper/stray.txt");
        assert_eq!(
            root_violations(
                &format!("{COMPOSED}/\n    plain/\n        deeper/\n"),
                &[&deep],
                &[]
            ),
            vec![format!("unexpected {COMPOSED}/plain/deeper/stray.txt")]
        );
    }

    #[test]
    fn a_flint_file_in_a_folder_stored_decomposed_is_found_and_read() {
        let nested = format!("{DECOMPOSED}/{DECOMPOSED}/.flint");
        let stray = format!("{DECOMPOSED}/{DECOMPOSED}/stray.txt");
        let tree = tree(
            &[
                (".flint", "*/\n    */\n        !LICENSE\n"),
                (&nested, "!README.md\n"),
                (&stray, ""),
            ],
            &[],
        );
        assert_eq!(
            violations_in(&tree),
            vec![
                (
                    String::new(),
                    vec![format!("missing {COMPOSED}/{COMPOSED}/LICENSE")]
                ),
                (
                    format!("{COMPOSED}/{COMPOSED}"),
                    vec![
                        "missing README.md".to_string(),
                        "unexpected stray.txt".to_string(),
                    ]
                ),
            ]
        );
    }

    #[test]
    fn two_names_differing_only_in_how_accents_are_stored_stay_two_names() {
        // Linux allows both in one folder; neither can be composed
        // without becoming the other, so each is compared as it's stored.
        let (composed, decomposed) = (format!("{COMPOSED}.txt"), format!("{DECOMPOSED}.txt"));
        assert_eq!(
            root_violations(&format!("{COMPOSED}.txt\n"), &[&composed, &decomposed], &[]),
            vec![format!("unexpected {decomposed}")]
        );
        // A rule is always composed, so it can't name the other one.
        assert_eq!(
            root_violations(
                &format!("{DECOMPOSED}.txt\n"),
                &[&composed, &decomposed],
                &[]
            ),
            vec![format!("unexpected {decomposed}")]
        );
        // Folders too, each still looked inside under its own name.
        let files = [
            format!("{COMPOSED}/a.txt"),
            format!("{DECOMPOSED}/b.txt"),
            "plain.txt".to_string(),
        ];
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        assert_eq!(
            root_violations("*/\n    a.txt\nplain.txt\n", &files, &[]),
            vec![format!("unexpected {DECOMPOSED}/b.txt")]
        );
    }

    #[test]
    fn composing_covers_more_than_latin_accents() {
        for (rule, on_disk) in [
            // Korean: one syllable block, or its three letters.
            ("\u{d55c}", "\u{1112}\u{1161}\u{11ab}"),
            // Japanese: `が`, or `か` and a voicing mark.
            ("\u{304c}", "\u{304b}\u{3099}"),
            // Vietnamese: two accents on one letter, in either order.
            ("\u{1ec7}", "e\u{323}\u{302}"),
            ("\u{1ec7}", "e\u{302}\u{323}"),
            // The ångström sign is the letter `Å`.
            ("\u{c5}", "\u{212b}"),
        ] {
            assert_eq!(
                root_violations(&format!("!{rule}.txt\n"), &[&format!("{on_disk}.txt")], &[]),
                Vec::<String>::new(),
                "{rule:?} against {on_disk:?}"
            );
        }
    }

    #[test]
    fn composing_never_makes_two_different_letters_match() {
        for (rule, on_disk, reported) in [
            (COMPOSED, "cafe", "cafe"),
            ("cafe", COMPOSED, COMPOSED),
            ("cafe", DECOMPOSED, COMPOSED),
            // Lookalikes that aren't the same character stored another
            // way: the `ﬁ` ligature, a full-width `ａ`, Cyrillic `а`.
            ("file", "\u{fb01}le", "\u{fb01}le"),
            ("a", "\u{ff41}", "\u{ff41}"),
            ("a", "\u{430}", "\u{430}"),
        ] {
            assert_eq!(
                root_violations(&format!("{rule}.txt\n"), &[&format!("{on_disk}.txt")], &[]),
                vec![format!("unexpected {reported}.txt")],
                "{rule:?} against {on_disk:?}"
            );
        }
    }

    #[test]
    fn a_case_filter_reads_a_name_stored_decomposed() {
        let decomposed = "cre\u{300}me_bru\u{302}le\u{301}e";
        assert_eq!(
            root_violations(
                "{{ name|snake }}/\n    !{{ name|pascal }}.txt\n",
                &[&format!(
                    "{decomposed}/Cre\u{300}meBru\u{302}le\u{301}e.txt"
                )],
                &[]
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            root_violations(
                "{{ name|snake }}/\n    !{{ name|pascal }}.txt\n",
                &[],
                &[decomposed]
            ),
            vec!["missing cr\u{e8}me_br\u{fb}l\u{e9}e/Cr\u{e8}meBr\u{fb}l\u{e9}e.txt".to_string()]
        );
    }

    #[test]
    fn names_differing_only_in_case_are_different_names() {
        let flint = "!foo\n!FOO\n";
        assert_eq!(
            root_violations(flint, &["foo", "FOO"], &[]),
            Vec::<String>::new()
        );
        assert_eq!(root_violations(flint, &["foo"], &[]), vec!["missing FOO"]);
        assert_eq!(
            root_violations(flint, &["Foo"], &[]),
            vec!["missing foo", "missing FOO", "unexpected Foo"]
        );
        assert_eq!(
            root_violations("!\u{e9}t\u{e9}.txt\n", &["\u{c9}t\u{e9}.txt"], &[]),
            vec!["missing \u{e9}t\u{e9}.txt", "unexpected \u{c9}t\u{e9}.txt"]
        );
    }

    #[test]
    fn rules_and_names_in_any_script_match_literally() {
        let flint = "\
!größe.txt
!日本語/
    !ファイル.txt
!Ελληνικά.md
!привет мир.txt
!שלום.txt
!🙂.txt
";
        let files = [
            "größe.txt",
            "日本語/ファイル.txt",
            "Ελληνικά.md",
            "привет мир.txt",
            "שלום.txt",
            "🙂.txt",
        ];
        assert_eq!(root_violations(flint, &files, &[]), Vec::<String>::new());
        assert_eq!(
            root_violations(flint, &["grosse.txt", "日本語/ファイル.txt"], &[]),
            vec![
                "missing größe.txt",
                "missing Ελληνικά.md",
                "missing привет мир.txt",
                "missing שלום.txt",
                "missing 🙂.txt",
                "unexpected grosse.txt",
            ]
        );
    }

    #[test]
    fn wildcards_and_variables_take_names_in_any_script() {
        assert_eq!(
            root_violations(
                "*.txt\nà_*_ü.md\n{{ name }}/\n    !{{ name }}.rs\n**/\n    données.csv\n",
                &[
                    "日本語.txt",
                    "à_ñ_ü.md",
                    "größe/größe.rs",
                    "größe/données.csv",
                    "ελ/ελ.rs",
                    "ελ/λε.rs",
                ],
                &[]
            ),
            vec!["unexpected ελ/λε.rs"]
        );
        // One `*` is any run of characters, not of bytes.
        assert_eq!(
            root_violations("a*b\n", &["aéb", "a語語b", "a🙂b", "ab"], &[]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_group_s_value_is_compared_however_its_accents_are_stored() {
        let flint = "<name>\n    {{ name }}.md\n    {{ name }}.txt\n";
        assert_eq!(
            root_violations(
                flint,
                &[&format!("{COMPOSED}.md"), &format!("{DECOMPOSED}.txt")],
                &[]
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            root_violations(flint, &[&format!("{COMPOSED}.md"), "cafe.txt"], &[]),
            vec!["conflict <name> in "]
        );
    }
}
