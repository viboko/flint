// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Composing multiple `.flint` files' rules together when one directory's
//! `.flint` file is reached by another, ancestor `.flint` file's rule that
//! extends past its own directory — see `docs/flint-file-format.md`'s
//! "Multiple `.flint` files" section for the user-facing behavior this
//! implements.
//! Pure logic, no filesystem access.

use std::collections::HashSet;

use crate::checker::is_governed_directory;
use crate::flint_file::Entry;

/// If `name`'s path reaches strictly past `boundary` (a `/`-joined
/// directory path relative to `name`'s own frame), returns the remaining
/// path components (joined by `/`) — `name` rebased to be relative to
/// `boundary`'s own directory instead. Returns `None` if `name` doesn't
/// reach that deep, including when it terminates exactly at `boundary`.
fn rebase_literal_name(name: &str, boundary: &str) -> Option<String> {
    let name_components: Vec<&str> = name.split('/').collect();
    let boundary_components: Vec<&str> = boundary.split('/').collect();
    name_components
        .strip_prefix(boundary_components.as_slice())
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.join("/"))
}

/// `entry` rebased to be relative to `boundary` (a `/`-joined directory
/// path relative to `entry`'s own frame) — see `rebase_literal_name`
/// and `crate::template::Template::rebase_beneath`. Returns `None` if
/// `entry`'s path doesn't reach past `boundary`.
#[must_use]
pub fn rebase_entry(entry: &Entry, boundary: &str) -> Option<Entry> {
    match entry {
        Entry::Literal {
            name,
            kind,
            required,
            origin,
        } => rebase_literal_name(name, boundary).map(|name| Entry::Literal {
            name,
            kind: *kind,
            required: *required,
            origin: origin.clone(),
        }),
        Entry::Template(template) => template.rebase_beneath(boundary).map(Entry::Template),
    }
}

/// `entry` with any requirement (`!`) left to each of `descendant_scopes`
/// (paths relative to `entry`'s own frame) its path reaches past — the
/// "must exist" obligation there belongs to that nested `.flint` file's
/// own, independently-imported and independently-checked copy (see
/// [`rebase_entry`]), so it must not also be flagged missing at this
/// level. A rule that's checked once per real folder (see
/// `crate::template::Template::is_checked_per_instance`) stays required
/// in the folders with no `.flint` file of their own: `!{{ name }}/x`
/// still needs `b/x` when only `a/` has one. Any other rule has just the
/// one obligation, so its `!` is suppressed altogether. `entry` otherwise
/// still contributes to plain matching/governance here, unchanged.
#[must_use]
pub fn suppress_delegated_requirement<S: std::hash::BuildHasher>(
    entry: &Entry,
    descendant_scopes: &HashSet<String, S>,
) -> Entry {
    let mut reached: Vec<String> = descendant_scopes
        .iter()
        .filter(|scope| is_governed_directory(std::slice::from_ref(entry), scope))
        .cloned()
        .collect();
    if reached.is_empty() {
        return entry.clone();
    }
    reached.sort_unstable();
    match entry {
        Entry::Literal {
            name, kind, origin, ..
        } => Entry::Literal {
            name: name.clone(),
            kind: *kind,
            required: false,
            origin: origin.clone(),
        },
        Entry::Template(template) if template.is_checked_per_instance() => {
            Entry::Template(template.delegating_to(reached))
        }
        Entry::Template(template) => Entry::Template(template.without_required()),
    }
}

/// Every entry of `scopes` that's a strict-prefix ancestor of `scope`'s
/// own path — `scope` itself is never included. `scopes` and `scope` are
/// each `""` for the invocation directory itself, or a `/`-joined path
/// relative to it for anything nested (see
/// [`crate::checker::nested_scopes_relative_to`], the descendant-facing
/// counterpart of this function).
#[must_use]
pub fn ancestor_scopes<'a>(scopes: &[&'a str], scope: &str) -> Vec<&'a str> {
    scopes
        .iter()
        .copied()
        .filter(|candidate| is_ancestor_scope(candidate, scope))
        .collect()
}

/// Whether `candidate` is a strict ancestor of `scope`: `""` (the
/// invocation directory) for any other scope, otherwise a path `scope`
/// continues with a `/`.
fn is_ancestor_scope(candidate: &str, scope: &str) -> bool {
    if candidate == scope {
        return false;
    }
    if candidate.is_empty() {
        return true;
    }
    scope
        .strip_prefix(candidate)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The path from `ancestor`'s own directory to `scope`'s, as a `/`-joined
/// path relative to `ancestor` — e.g. `("a", "a/b/c")` gives `"b/c"`;
/// `("", "a/b/c")` gives `"a/b/c"` unchanged. `ancestor` must be a strict
/// prefix of `scope` (the caller only ever passes genuine ancestor
/// scopes, e.g. from [`ancestor_scopes`]).
fn relative_to_ancestor(ancestor: &str, scope: &str) -> String {
    if ancestor.is_empty() {
        scope.to_string()
    } else {
        scope
            .strip_prefix(ancestor)
            .and_then(|rest| rest.strip_prefix('/'))
            .expect("ancestor is a strict prefix of scope")
            .to_string()
    }
}

/// The per-origin rule sets that apply when checking `scope`'s own
/// directory: `scope`'s own entries, plus a rebased-and-requirement-
/// suppressed copy of any ancestor `.flint` file's entries that reach
/// into `scope`. Each `(String, Vec<Entry>)` pair is one origin `.flint`
/// file's own, independently-checkable contribution — kept separate so
/// `{{ variable }}` consistency stays scoped to the file that declared
/// it, exactly as it already is within a single `.flint` file. The
/// `String` is that origin's own scope path.
///
/// `ancestors` is every other scope directory that's a strict prefix of
/// `scope`'s own path (see [`ancestor_scopes`]), paired with its own
/// successfully-parsed entries — an ancestor `.flint` file that failed to
/// parse contributes nothing here. `descendant_scopes` is `scope`'s own
/// nested scopes, as paths relative to `scope` itself (see
/// [`crate::checker::nested_scopes_relative_to`]).
#[must_use]
pub fn effective_origins<S: std::hash::BuildHasher>(
    scope: &str,
    own_entries: &[Entry],
    ancestors: &[(&str, &[Entry])],
    descendant_scopes: &HashSet<String, S>,
) -> Vec<(String, Vec<Entry>)> {
    let mut origins = vec![(
        scope.to_string(),
        own_entries
            .iter()
            .map(|entry| suppress_delegated_requirement(entry, descendant_scopes))
            .collect(),
    )];

    for (ancestor, ancestor_entries) in ancestors {
        let relative = relative_to_ancestor(ancestor, scope);
        let imported: Vec<Entry> = ancestor_entries
            .iter()
            .filter_map(|entry| rebase_entry(entry, &relative))
            .map(|entry| suppress_delegated_requirement(&entry, descendant_scopes))
            .collect();
        if !imported.is_empty() {
            origins.push(((*ancestor).to_string(), imported));
        }
    }

    origins
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::Violation;
    use crate::flint_file::parse;

    fn entries(flint_contents: &str) -> Vec<Entry> {
        parse(flint_contents).unwrap()
    }

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn rebase_entry_on_a_literal_strips_the_boundary_prefix() {
        let entry = &entries("src/util.rs")[0];
        let rebased = rebase_entry(entry, "src").unwrap();
        assert!(matches!(rebased, Entry::Literal { ref name, .. } if name == "util.rs"));
    }

    #[test]
    fn rebase_entry_on_a_literal_returns_none_if_it_does_not_reach_that_far() {
        let entry = &entries("a.txt")[0];
        assert!(rebase_entry(entry, "src").is_none());
    }

    #[test]
    fn rebase_entry_on_a_template_delegates_to_rebase_beneath() {
        let entry = &entries("src/{{ platform }}/main.py")[0];
        let rebased = rebase_entry(entry, "src/linux").unwrap();
        let Entry::Template(template) = rebased else {
            panic!("expected a Template");
        };
        assert_eq!(
            template.matched_bindings("main.py"),
            Some(vec![("platform".to_string(), "linux".to_string(), None)])
        );
    }

    fn rebased_template(contents: &str, index: usize, boundary: &str) -> crate::template::Template {
        match rebase_entry(&entries(contents)[index], boundary) {
            Some(Entry::Template(template)) => template,
            other => panic!("expected a rebased template, got {other:?}"),
        }
    }

    #[test]
    fn rebase_entry_leaves_a_template_s_groups_behind() {
        let template = rebased_template("src/\n  <name>\n    {{ name }}.md\n", 1, "src");
        assert_eq!(template.groups(), []);
    }

    #[test]
    fn rebase_entry_drops_a_requirement_the_group_check_owns() {
        let contents = "src/\n  <name>\n    !{{ name }}.md\n    {{ name }}/\n      !x.txt\n";
        assert!(!rebased_template(contents, 1, "src").required());
        // `x.txt` is in a subfolder of the group's, so it's checked as usual.
        assert!(rebased_template(contents, 3, "src").required());
    }

    #[test]
    fn suppress_delegated_requirement_leaves_a_non_reaching_entry_untouched() {
        let entry = &entries("!README.md")[0];
        let untouched = suppress_delegated_requirement(entry, &set(&["src"]));
        assert!(matches!(untouched, Entry::Literal { required: true, .. }));
    }

    #[test]
    fn suppress_delegated_requirement_clears_required_for_a_reaching_literal() {
        let entry = &entries("!src/util.rs")[0];
        let suppressed = suppress_delegated_requirement(entry, &set(&["src"]));
        assert!(matches!(
            suppressed,
            Entry::Literal {
                required: false,
                ..
            }
        ));
    }

    #[test]
    fn suppress_delegated_requirement_clears_required_for_a_reaching_any_depth_template() {
        let entry = &entries("!src/**/{{ platform }}.py")[0];
        let suppressed = suppress_delegated_requirement(entry, &set(&["src"]));
        let Entry::Template(template) = suppressed else {
            panic!("expected a Template");
        };
        assert!(!template.required());
    }

    #[test]
    fn suppress_delegated_requirement_keeps_a_per_folder_template_required_elsewhere() {
        let entry = &entries("!{{ name }}/main.py")[0];
        let suppressed = suppress_delegated_requirement(entry, &set(&["a", "b/c"]));
        let actual = |name: &str| crate::checker::ActualEntry {
            name: name.to_string(),
            kind: crate::entry_kind::EntryKind::Directory,
        };
        // `a` has its own `.flint` file, and `b/c` isn't one the rule reaches.
        let violations = crate::checker::check(&[suppressed], &[actual("a"), actual("b")]);
        assert!(
            matches!(&violations[..], [Violation::Missing { name, .. }] if name == "b/main.py"),
            "{violations:?}"
        );
    }

    #[test]
    fn ancestor_scopes_finds_every_strict_prefix() {
        let scopes = ["", "src", "src/linux", "docs"];
        let refs: Vec<&str> = scopes.to_vec();
        let mut ancestors = ancestor_scopes(&refs, "src/linux");
        ancestors.sort_unstable();
        assert_eq!(ancestors, vec!["", "src"]);
    }

    #[test]
    fn ancestor_scopes_excludes_the_scope_itself_and_non_ancestors() {
        let scopes = ["", "src", "docs"];
        let refs: Vec<&str> = scopes.to_vec();
        let ancestors = ancestor_scopes(&refs, "src");
        assert_eq!(ancestors, vec![""]);
    }

    #[test]
    fn ancestor_scopes_ignores_a_false_prefix_match() {
        // "src" merely starts with the string "src" as a substring of
        // "srcfoo" — it isn't a real ancestor directory of "srcfoo/bar",
        // even though "srcfoo" itself genuinely is.
        let scopes = ["src", "srcfoo"];
        let refs: Vec<&str> = scopes.to_vec();
        assert_eq!(ancestor_scopes(&refs, "srcfoo/bar"), vec!["srcfoo"]);
    }

    #[test]
    fn effective_origins_with_no_reaching_ancestors_is_just_the_scopes_own_entries() {
        let own = entries("README.md");
        let origins = effective_origins("src", &own, &[], &HashSet::new());
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].0, "src");
    }

    #[test]
    fn effective_origins_imports_a_reaching_ancestor_rule_rebased_and_labeled() {
        let own = entries("");
        let root_entries = entries("src/util.rs");
        let origins = effective_origins("src", &own, &[("", &root_entries)], &HashSet::new());
        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0].0, "src");
        assert_eq!(origins[1].0, "");
        assert!(matches!(
            &origins[1].1[0],
            Entry::Literal { name, .. } if name == "util.rs"
        ));
    }

    #[test]
    fn effective_origins_omits_a_non_reaching_ancestor() {
        let own = entries("");
        let root_entries = entries("docs/");
        let origins = effective_origins("src", &own, &[("", &root_entries)], &HashSet::new());
        assert_eq!(origins.len(), 1);
    }

    #[test]
    fn effective_origins_end_to_end_cascading_is_not_flagged_unexpected() {
        // Root permits `src/util.rs`; `src` owns its own (empty) `.flint`
        // that says nothing about `util.rs` at all. Cascading means the
        // root's permission still applies inside `src`.
        let root_entries = entries("src/util.rs");
        let src_entries = entries("");
        let origins =
            effective_origins("src", &src_entries, &[("", &root_entries)], &HashSet::new());
        let actual = vec![crate::checker::ActualEntry {
            name: "util.rs".to_string(),
            kind: crate::entry_kind::EntryKind::File,
        }];
        let per_origin: Vec<Vec<Violation>> = origins
            .iter()
            .map(|(_, entries)| crate::checker::check(entries, &actual))
            .collect();
        let merged = crate::checker::merge_origin_violations(&per_origin);
        assert_eq!(merged, Vec::new());
    }

    #[test]
    fn rebase_entry_on_a_literal_returns_none_if_it_ends_exactly_at_the_boundary() {
        let entry = &entries("src/")[0];
        assert!(rebase_entry(entry, "src").is_none());
    }
}
