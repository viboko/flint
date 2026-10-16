// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Comparison of allowed entries against the names actually present on
//! disk, including consistency-checking of `{{ variable }}` template
//! placeholders.
//!
//! **`.flint` is an allow-list: an entry is fine if any one rule allows
//! it.** A template only allows an entry when its shape fits *and* every
//! variable it captures agrees with itself (see below). An entry no rule
//! allows is [`Violation::Unexpected`].
//!
//! **`{{ variable }}` consistency is scoped to a single matched entry, and
//! never spans more than one.** Multiple occurrences of the same variable
//! within *one* template's match — e.g. both halves of
//! `{{ name }}_{{ name }}.txt` matched against one file, or the two path
//! components of `{{ name }}/{{ name }}Manager.py` matched against one
//! nested path — must agree with each other (see `reconcile_match`).
//! Two *different* `.flint` lines are never reconciled against each other,
//! no matter how many times they reuse the same variable name: a lone
//! `{{ name }}.txt` rule can match any number of differently-named files,
//! and `{{ name }}Manager.py` / `{{ name }}Presenter.py` written
//! flat (no shared directory) never have to agree on `name`. The only way
//! to tie two separate rules to the same real value is nesting them under
//! a shared directory-capturing rule, since that forces them to share one
//! real path prefix by construction.
//!
//! `!` (required) entries follow the same "one real directory instance"
//! principle: a required template whose path reaches through a governed
//! directory (more than one `/`-separated component, no `**`) is checked
//! independently within *every* real directory matching the folders its
//! line is indented under, rather than "matched somewhere on disk", and
//! requires each folder its own line names along with the last — see
//! `required_template_missing_per_instance`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::entry_kind::EntryKind;
use crate::flint_file::{self, Entry};
use crate::format::{self, Format, Groupings};
use crate::origin::Origin;
use crate::template::{GroupMembership, Segment, Template};

/// One real filesystem entry found inside the linted directory or a
/// directory some rule's path reaches into, gathered by `crate::run`'s
/// directory walk and passed in as plain data so this module stays
/// filesystem-free. `name` is a bare name for a top-level entry, or a
/// `/`-joined relative path for anything nested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActualEntry {
    /// Its name, or its path relative to the linted directory.
    pub name: String,
    /// Whether it's a file, a directory or a symlink.
    pub kind: EntryKind,
}

/// One value a group header's variables took in a folder, and the files
/// that gave it — see [`Violation::GroupConflict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupValue {
    /// The value as one file captured it, or for a header with several
    /// variables, each variable's value in header order, joined by `, `.
    pub value: String,
    /// Every file that gave this value, sorted.
    pub files: Vec<String>,
}

/// A single problem found comparing `.flint` against the actual directory
/// contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The files under a `<name>` group header in one folder don't all
    /// fit one value for its variables. No single file is to blame, so
    /// this lists every value found instead.
    GroupConflict {
        /// The header as written, with its enclosing path, e.g.
        /// `src/<name>`.
        group: String,
        /// The real folder, `""` for the `.flint` file's own.
        folder: String,
        /// Each distinct value, in the order first found.
        values: Vec<GroupValue>,
        /// Where the header was written.
        origin: Origin,
    },
    /// A file or folder present on disk that no rule allows.
    Unexpected(String),
    /// A `!`-marked entry has no match anywhere on disk (or, for a
    /// per-instance required template, no match within one specific real
    /// directory instance).
    Missing {
        /// The clean rule (name, or the `{{ }}`/`*` shape as written) with
        /// no leading `!` — or for a per-instance template, the path
        /// expected inside that instance.
        name: String,
        /// Where the required rule was written.
        origin: Origin,
    },
}

/// Everything tracked for one variable name while reconciling the
/// occurrences captured by a *single* template match — see this module's
/// doc comment for why state never survives past one match.
///
/// Two independent checks apply to every new occurrence: repeats of the
/// *same* format must match `by_format`'s entry for that format literally
/// (this is what catches e.g. `XMLFile` and `XmlFile` both written via
/// `|pascal` as a real disagreement, or two `|snake` occurrences spelled
/// differently); occurrences under *different* formats only need to admit
/// at least one common word-grouping with `candidates`, which narrows
/// as more formats are seen — this is what recognizes
/// `{{ name|camel }}` and `{{ name|pascal }}` as the same identifier
/// despite capturing different literal text. See `format::groupings`'s
/// doc comment for why cross-format matching has to be permissive like
/// this in the first place.
///
/// A group header's check (see [`group_violations`]) tracks the same
/// state across every file in one folder instead of across one match.
#[derive(Clone)]
struct VariableState {
    /// The literal value of the very first occurrence seen for this
    /// variable in this match.
    first_value: String,
    /// The format `first_value` was captured under.
    first_format: Option<Format>,
    /// The literal value already seen for each distinct format used for
    /// this variable so far in this match. An unformatted variable only
    /// ever has the `None` key, since format-presence is uniform per
    /// variable, enforced at parse time by
    /// `ParseErrorKind::InconsistentVariableFormat` — so for an
    /// unformatted variable, this check alone is exactly plain literal
    /// equality between its (at most two, within one match) occurrences.
    by_format: HashMap<Option<Format>, String>,
    /// The word-groupings compatible with every occurrence seen so far,
    /// across all formats.
    candidates: Groupings,
}

/// The word-groupings a single occurrence's captured `value` admits:
/// `value` itself as one opaque word when unformatted (making the
/// cross-format check below reduce to plain equality), or
/// `format::groupings` otherwise. That reads any value at all, so there's
/// nothing to fail here — though `value` always fits `format`, having
/// been captured by that format's own constraining regex.
fn occurrence_candidates(value: &str, format: Option<Format>) -> Groupings {
    match format {
        None => Groupings::opaque(value),
        Some(format) => format::groupings(value, format),
    }
}

impl VariableState {
    /// The state after a variable's first occurrence, `value` captured
    /// under `format`.
    fn new(value: &str, format: Option<Format>) -> Self {
        VariableState {
            first_value: value.to_string(),
            first_format: format,
            by_format: HashMap::from([(format, value.to_string())]),
            candidates: occurrence_candidates(value, format),
        }
    }

    /// Adds another occurrence, `value` captured under `format`, returning
    /// whether it agrees with the earlier ones, per this type's doc
    /// comment.
    fn admit(&mut self, value: &str, format: Option<Format>) -> bool {
        match self.by_format.get(&format) {
            Some(existing) if existing != value => return false,
            Some(_) => {}
            None => {
                self.by_format.insert(format, value.to_string());
            }
        }
        match self
            .candidates
            .narrow(&occurrence_candidates(value, format))
        {
            Some(narrowed) => {
                self.candidates = narrowed;
                true
            }
            None => false,
        }
    }

    /// The words of the value settled on, for rendering under any format.
    fn words(&self) -> Vec<String> {
        format::canonical_words(&self.first_value, self.first_format)
    }
}

/// Whether every occurrence of the same variable within `bindings` — one
/// template's captured `(variable, value, format)` triples from matching
/// a *single* real entry — agrees with the others, per
/// [`VariableState`]'s doc comment.
fn bindings_agree(bindings: &[(String, String, Option<Format>)]) -> bool {
    let mut state_by_variable: HashMap<&str, VariableState> = HashMap::new();
    for (variable, value, format) in bindings {
        match state_by_variable.get_mut(variable.as_str()) {
            Some(state) => {
                if !state.admit(value, *format) {
                    return false;
                }
            }
            None => {
                state_by_variable.insert(variable.as_str(), VariableState::new(value, *format));
            }
        }
    }
    true
}

/// Whether `template` allows the entry `name` — its shape fits *and* its
/// variables agree. A file that fits the shape but whose variables
/// disagree is no different from one that doesn't fit at all: it neither
/// permits an entry nor satisfies a required (`!`) rule.
fn allows(template: &Template, name: &str) -> bool {
    template
        .matched_bindings(name)
        .is_some_and(|bindings| bindings_agree(&bindings))
}

/// Compares parsed `.flint` `entries` against `actual_entries` (the real
/// directory listing) and returns every problem found.
///
/// An entry is fine if any one rule allows it: a literal name, a template
/// whose shape fits and whose variables agree (see this module's doc
/// comment), or — for a directory — a rule whose path reaches through it.
/// Anything else is reported as [`Violation::Unexpected`], sorted
/// alphabetically. Names with no matching entry on
/// disk are fine by default: they're permitted, not required — *unless*
/// the entry is marked `!` (required), in which case a match is mandatory
/// (see `missing_violations` for the "anywhere" vs. per-instance
/// distinction); a required entry with no (sufficient) match is reported
/// as [`Violation::Missing`], in `.flint` declaration order, before any
/// unexpected entries. Rules under a `<name>` group header must also
/// agree on one value per folder (see [`check_groups`]), and any
/// [`Violation::GroupConflict`] comes first of all.
///
/// This is [`check_rules`] and [`check_groups`] together, for a single
/// `.flint` file whose `actual_entries` are everything it governs.
#[must_use]
pub fn check(entries: &[Entry], actual_entries: &[ActualEntry]) -> Vec<Violation> {
    let (conflicts, missing) = check_groups(entries, actual_entries, |_| false);
    with_group_violations(check_rules(entries, actual_entries), conflicts, missing)
}

/// `rule_violations` (as [`check_rules`] or [`merge_origin_violations`]
/// returns them) with a group check's `conflicts` added first, and its
/// `missing` entries after the rules' own.
#[must_use]
pub fn with_group_violations(
    rule_violations: Vec<Violation>,
    conflicts: Vec<Violation>,
    missing: Vec<Violation>,
) -> Vec<Violation> {
    let (rule_missing, unexpected): (Vec<Violation>, Vec<Violation>) = rule_violations
        .into_iter()
        .partition(|violation| !matches!(violation, Violation::Unexpected(_)));
    conflicts
        .into_iter()
        .chain(rule_missing)
        .chain(missing)
        .chain(unexpected)
        .collect()
}

/// [`check`], without the group check: every entry each rule allows, and
/// every required rule's [`Violation::Missing`] — except a rule directly
/// under a group header, whose requirement [`check_groups`] checks.
#[must_use]
pub fn check_rules(entries: &[Entry], actual_entries: &[ActualEntry]) -> Vec<Violation> {
    let literals: HashSet<(&str, EntryKind)> = entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Literal { name, kind, .. } => Some((name.as_str(), *kind)),
            Entry::Template(_) => None,
        })
        .collect();

    let mut sorted_actual: Vec<&ActualEntry> = actual_entries.iter().collect();
    sorted_actual.sort_by(|a, b| a.name.cmp(&b.name));

    let mut allowed_by_template: HashSet<&str> = HashSet::new();
    for entry in entries {
        let Entry::Template(template) = entry else {
            continue;
        };
        for actual in &sorted_actual {
            if template.kind() == actual.kind && allows(template, &actual.name) {
                allowed_by_template.insert(actual.name.as_str());
            }
        }
    }

    let missing = missing_violations(entries, actual_entries);

    let unexpected = sorted_actual
        .iter()
        .filter(|actual| {
            !(literals.contains(&(actual.name.as_str(), actual.kind))
                || allowed_by_template.contains(actual.name.as_str())
                || (actual.kind == EntryKind::Directory
                    && is_governed_directory(entries, &actual.name)))
        })
        .map(|actual| Violation::Unexpected(actual.name.clone()));

    missing.into_iter().chain(unexpected).collect()
}

/// Every group header in `entries`: its [`Violation::GroupConflict`]s,
/// then its [`Violation::Missing`]s, among `actual_entries` — see
/// `group_violations`. `allowed_elsewhere` says whether a rule that isn't
/// in `entries` at all allows an entry, such as one from the `.flint`
/// file of a nested folder it's in: like one of `entries`' own rules
/// outside the group, that takes it out of the group's reckoning.
#[must_use]
pub fn check_groups(
    entries: &[Entry],
    actual_entries: &[ActualEntry],
    allowed_elsewhere: impl Fn(&ActualEntry) -> bool,
) -> (Vec<Violation>, Vec<Violation>) {
    let mut sorted_actual: Vec<&ActualEntry> = actual_entries.iter().collect();
    sorted_actual.sort_by(|a, b| a.name.cmp(&b.name));
    group_violations(entries, &sorted_actual, &allowed_elsewhere)
}

/// Whether `entry` allows `actual`: a literal of the same name and kind,
/// or a template of the same kind that allows it.
#[must_use]
pub fn entry_allows(entry: &Entry, actual: &ActualEntry) -> bool {
    match entry {
        Entry::Literal { name, kind, .. } => *name == actual.name && *kind == actual.kind,
        Entry::Template(template) => {
            template.kind() == actual.kind && allows(template, &actual.name)
        }
    }
}

/// Whether `template`'s `!` is checked by [`group_violations`] rather
/// than [`missing_violations`] — see [`Template::is_checked_by_group`].
fn checked_by_group(template: &Template) -> bool {
    template.is_checked_by_group()
}

/// One way a file fits a group's rules: the value it gives each of the
/// group's variables that the rule it fits uses, as `(variable, value,
/// format)`.
type Reading = Vec<(String, String, Option<Format>)>;

/// Every group header's [`Violation::GroupConflict`]s, then its
/// [`Violation::Missing`]s, among `sorted_actual`.
///
/// A group header binds its variables across every rule indented under
/// it, in each real folder it sits in: each folder is one *instance*, and
/// everything in it that fits one of those rules must share one value for
/// each variable. There's no anchor — the value is whatever lets every
/// such file fit, found by search (see [`find_group_value`]), so the
/// order files and rules happen to be listed in never matters. A file a
/// rule outside the group allows doesn't count toward the value: it's
/// already fine. When no value fits every file, the instance gets one
/// [`Violation::GroupConflict`] listing each value found, rather than
/// blaming any one file.
///
/// An instance holding none of the group's files is fine — unless the
/// header itself is marked `!` (`!<name>`), which requires a set in every
/// instance. Once an instance has a value, each `!` rule directly in its
/// folder is required there too, named with that value — as long as it
/// gives every one of the header's variables the rule uses (see
/// [`has_its_values`]). In an instance
/// whose files conflict, the conflict is reported instead.
fn group_violations(
    entries: &[Entry],
    sorted_actual: &[&ActualEntry],
    allowed_elsewhere: &dyn Fn(&ActualEntry) -> bool,
) -> (Vec<Violation>, Vec<Violation>) {
    let mut members: BTreeMap<usize, (&GroupMembership, Vec<&Template>)> = BTreeMap::new();
    for entry in entries {
        if let Entry::Template(template) = entry {
            for group in template.groups() {
                members
                    .entry(group.id)
                    .or_insert((group, Vec::new()))
                    .1
                    .push(template);
            }
        }
    }

    let mut conflicts = Vec::new();
    let mut missing = Vec::new();
    for (group, group_members) in members.values() {
        let mut files_by_folder = group_files(
            entries,
            group,
            group_members,
            sorted_actual,
            allowed_elsewhere,
        );
        let instances = if group.any_depth.is_some() {
            // Every folder a `**` reaches is an instance, so only those
            // holding the group's files are worth checking — and a
            // required header is satisfied by a set in any one of them.
            if files_by_folder.is_empty() && is_group_required(group, group_members) {
                missing.push(Violation::Missing {
                    name: group.source_line.clone(),
                    origin: group.origin.clone(),
                });
            }
            files_by_folder.keys().cloned().collect()
        } else {
            group_instances(group, group_members, sorted_actual)
        };
        for folder in instances {
            let files = files_by_folder.remove(&folder).unwrap_or_default();
            let Some(value) = find_group_value(&files) else {
                conflicts.push(group_conflict(group, &folder, &files));
                continue;
            };
            if files.is_empty() {
                if is_group_required(group, group_members) {
                    missing.push(group_set_missing(group, &folder));
                }
                continue;
            }
            missing.extend(
                group_members
                    .iter()
                    .filter(|member| member.required() && checked_by_group(member))
                    .filter(|member| member.groups().last().is_some_and(|g| g.id == group.id))
                    .filter(|member| has_its_values(member, group, &value))
                    .filter(|member| !satisfies(member, group, &folder, &files, &value))
                    .map(|member| group_missing(member, group, &folder, &value)),
            );
        }
    }
    (conflicts, missing)
}

/// Whether the folder's `value` gives every one of `group`'s variables
/// that `member` uses: a header like `<a, b>` shares each variable on its
/// own, so a `!` rule using `b` is only required once the folder's files
/// give `b` a value — and only then can its missing file be named.
fn has_its_values(
    member: &Template,
    group: &GroupMembership,
    value: &HashMap<String, VariableState>,
) -> bool {
    group
        .variables
        .iter()
        .filter(|variable| member.uses_variable(variable))
        .all(|variable| value.contains_key(variable))
}

/// Whether `group`'s header is marked `!`, as every member's copy of it
/// says.
fn is_group_required(group: &GroupMembership, members: &[&Template]) -> bool {
    members.iter().any(|member| {
        member
            .groups()
            .iter()
            .any(|g| g.id == group.id && g.required)
    })
}

/// The [`Violation::Missing`] for a required (`!<name>`) header in
/// `folder`, which holds none of its rules' files: the header itself,
/// e.g. `src/<name>`, since there's no value to name a file with.
fn group_set_missing(group: &GroupMembership, folder: &str) -> Violation {
    let header = format!("<{}>", group.variables.join(", "));
    Violation::Missing {
        name: if folder.is_empty() {
            header
        } else {
            format!("{folder}/{header}")
        },
        origin: group.origin.clone(),
    }
}

/// Every real folder `group` sits in, sorted, for a header with no `**`
/// in its path: the `.flint` file's own folder (`""`) at depth 0,
/// otherwise each real directory at its depth that the path leading to
/// the header matches.
fn group_instances(
    group: &GroupMembership,
    members: &[&Template],
    sorted_actual: &[&ActualEntry],
) -> Vec<String> {
    if group.depth == 0 {
        return vec![String::new()];
    }
    sorted_actual
        .iter()
        .filter(|actual| {
            actual.kind == EntryKind::Directory
                && path_depth(&actual.name) == group.depth
                && members
                    .iter()
                    .any(|member| member.rebase_beneath(&actual.name).is_some())
        })
        .map(|actual| actual.name.clone())
        .collect()
}

/// Every entry that one of `group`'s `members` allows and no rule
/// outside the group does — in `entries`, or per `allowed_elsewhere` — sorted, each with every [`Reading`] of it — one
/// per member that allows it — under the real folder that reading puts it
/// in as far as `group` is concerned (see [`Template::group_folder`]).
fn group_files<'a>(
    entries: &[Entry],
    group: &GroupMembership,
    members: &[&Template],
    sorted_actual: &[&'a ActualEntry],
    allowed_elsewhere: &dyn Fn(&ActualEntry) -> bool,
) -> BTreeMap<String, Vec<(&'a ActualEntry, Vec<Reading>)>> {
    let in_group = |entry: &Entry| match entry {
        Entry::Template(template) => template.groups().iter().any(|g| g.id == group.id),
        Entry::Literal { .. } => false,
    };
    let allowed_outside = |actual: &ActualEntry| {
        allowed_elsewhere(actual)
            || entries
                .iter()
                .any(|entry| !in_group(entry) && entry_allows(entry, actual))
    };

    let mut files_by_folder: BTreeMap<String, Vec<(&ActualEntry, Vec<Reading>)>> = BTreeMap::new();
    for actual in sorted_actual
        .iter()
        .filter(|actual| !allowed_outside(actual))
    {
        for member in members.iter().filter(|member| member.kind() == actual.kind) {
            let Some(reading) = reading(member, group, &actual.name) else {
                continue;
            };
            let files = files_by_folder
                .entry(member.group_folder(group, &actual.name))
                .or_default();
            match files.last_mut() {
                Some((last, readings)) if std::ptr::eq(*last, *actual) => readings.push(reading),
                _ => files.push((actual, vec![reading])),
            }
        }
    }
    files_by_folder
}

/// How `name` fits `member`, if `member` allows it: the value it gives
/// each of `group`'s variables `member` uses (its first occurrence —
/// any others agree, or `member` wouldn't allow it).
fn reading(member: &Template, group: &GroupMembership, name: &str) -> Option<Reading> {
    let bindings = member.matched_bindings(name)?;
    if !bindings_agree(&bindings) {
        return None;
    }
    let mut reading: Reading = Vec::new();
    for (variable, value, format) in bindings {
        if group.variables.contains(&variable) && !reading.iter().any(|(v, ..)| *v == variable) {
            reading.push((variable, value, format));
        }
    }
    Some(reading)
}

/// The most steps [`find_group_value`]'s search takes before giving up
/// and reporting a conflict. A file almost always fits a group's rules
/// only one way, making the search linear; this only bounds pathological
/// input, such as many files each fitting many rules.
const GROUP_SEARCH_STEP_LIMIT: usize = 100_000;

/// One value for each group variable that every file in `files` fits
/// under at least one of its readings, if there is one. Variables no
/// file's reading uses are absent.
fn find_group_value(
    files: &[(&ActualEntry, Vec<Reading>)],
) -> Option<HashMap<String, VariableState>> {
    fn search(
        files: &[(&ActualEntry, Vec<Reading>)],
        value: &HashMap<String, VariableState>,
        steps: &mut usize,
    ) -> Option<HashMap<String, VariableState>> {
        let Some(((_, readings), rest)) = files.split_first() else {
            return Some(value.clone());
        };
        for reading in readings {
            *steps += 1;
            if *steps > GROUP_SEARCH_STEP_LIMIT {
                return None;
            }
            if let Some(narrowed) = admit_reading(value, reading) {
                if let Some(found) = search(rest, &narrowed, steps) {
                    return Some(found);
                }
            }
        }
        None
    }
    search(files, &HashMap::new(), &mut 0)
}

/// `value` narrowed by `reading`, or `None` if they disagree.
fn admit_reading(
    value: &HashMap<String, VariableState>,
    reading: &Reading,
) -> Option<HashMap<String, VariableState>> {
    let mut narrowed = value.clone();
    for (variable, captured, format) in reading {
        match narrowed.get_mut(variable) {
            Some(state) => {
                if !state.admit(captured, *format) {
                    return None;
                }
            }
            None => {
                narrowed.insert(variable.clone(), VariableState::new(captured, *format));
            }
        }
    }
    Some(narrowed)
}

/// Whether some file in `files` satisfies the required `member` in
/// `folder`, with a reading that fits the folder's `value`.
fn satisfies(
    member: &Template,
    group: &GroupMembership,
    folder: &str,
    files: &[(&ActualEntry, Vec<Reading>)],
    value: &HashMap<String, VariableState>,
) -> bool {
    files.iter().any(|(actual, _)| {
        actual.kind == member.kind()
            && member.group_folder(group, &actual.name) == folder
            && reading(member, group, &actual.name)
                .is_some_and(|reading| admit_reading(value, &reading).is_some())
    })
}

/// The [`Violation::Missing`] for the required `member`, in `folder`:
/// its leaf rendered with the folder's `value`, and whatever `folder`'s
/// own name gives the variables in the path leading to it. A variable
/// neither names stays as written.
fn group_missing(
    member: &Template,
    group: &GroupMembership,
    folder: &str,
    value: &HashMap<String, VariableState>,
) -> Violation {
    let leaf = leaf_rule_text(member.source_line(), member.cut() + group.depth);
    let mut resolved = bound_words(member);
    resolved.extend(folder_words(member, folder));
    resolved.extend(
        value
            .iter()
            .map(|(variable, state)| (variable.clone(), state.words())),
    );
    let leaf = render_leaf(leaf, &resolved);
    Violation::Missing {
        name: if folder.is_empty() {
            leaf
        } else {
            format!("{folder}/{leaf}")
        },
        origin: member.origin().clone(),
    }
}

/// The words `folder`'s own name gives each variable in the path leading
/// `member` to it, for [`render_leaf`]: `blah` gives `{{ name|snake }}/`
/// the name `blah`. None when a `**` leaves open which part of `folder`
/// is which.
fn folder_words(member: &Template, folder: &str) -> HashMap<String, Vec<String>> {
    member
        .governing_prefix_bindings(folder)
        .unwrap_or_default()
        .into_iter()
        .map(|(variable, value, format)| (variable, format::canonical_words(&value, format)))
        .collect()
}

/// The [`Violation::GroupConflict`] for `group` in `folder`, whose
/// `files` fit no one value: files grouped by the value of their first
/// reading, compared by words so differently-cased spellings of one
/// identifier stay together. A file with a reading that gives no value
/// at all — a folder a `**/` under the header reaches, say — fits
/// whatever the value is, so it's no part of the disagreement and isn't
/// listed.
fn group_conflict(
    group: &GroupMembership,
    folder: &str,
    files: &[(&ActualEntry, Vec<Reading>)],
) -> Violation {
    let mut values: Vec<(Vec<Option<Vec<String>>>, GroupValue)> = Vec::new();
    for (actual, readings) in files {
        let Some(reading) = readings
            .first()
            .filter(|_| !readings.iter().any(Vec::is_empty))
        else {
            continue;
        };
        let captured = |variable: &String| reading.iter().find(|(v, ..)| v == variable);
        let key: Vec<Option<Vec<String>>> = group
            .variables
            .iter()
            .map(|variable| {
                captured(variable).map(|(_, value, format)| format::canonical_words(value, *format))
            })
            .collect();
        let file = match actual.kind {
            EntryKind::Directory => format!("{}/", actual.name),
            EntryKind::File => actual.name.clone(),
        };
        if let Some((_, existing)) = values.iter_mut().find(|(k, _)| *k == key) {
            existing.files.push(file);
        } else {
            let value = group
                .variables
                .iter()
                .map(|variable| captured(variable).map_or("*", |(_, value, _)| value.as_str()))
                .collect::<Vec<_>>()
                .join(", ");
            values.push((
                key,
                GroupValue {
                    value,
                    files: vec![file],
                },
            ));
        }
    }
    Violation::GroupConflict {
        group: group.source_line.clone(),
        folder: folder.to_string(),
        values: values.into_iter().map(|(_, value)| value).collect(),
        origin: group.origin.clone(),
    }
}

/// Every [`Violation::Missing`] produced by `entries`' required (`!`)
/// rules against `actual_entries`, in `.flint` declaration order (with a
/// per-instance template's own violations sorted by instance path — see
/// [`required_template_missing_per_instance`]).
fn missing_violations(entries: &[Entry], actual_entries: &[ActualEntry]) -> Vec<Violation> {
    entries
        .iter()
        .flat_map(|entry| match entry {
            Entry::Literal {
                name,
                kind,
                required: true,
                origin,
            } => {
                let matched = actual_entries
                    .iter()
                    .any(|actual| actual.kind == *kind && actual.name == *name);
                if matched {
                    Vec::new()
                } else {
                    let name = if *kind == EntryKind::Directory {
                        format!("{name}/")
                    } else {
                        name.clone()
                    };
                    vec![Violation::Missing {
                        name,
                        origin: origin.clone(),
                    }]
                }
            }
            Entry::Template(template) if template.required() && !checked_by_group(template) => {
                required_template_missing(entries, template, actual_entries)
            }
            Entry::Literal {
                required: false, ..
            }
            | Entry::Template(_) => Vec::new(),
        })
        .collect()
}

/// Where required `template`'s obligation isn't met. A template whose
/// path has more than one component, and no `**`, is checked folder by
/// folder along that path (see
/// [`required_template_missing_per_instance`]); every other required
/// template — flat (one component), or open-ended via `**` — keeps the
/// simpler "matched anywhere on disk" check, since there's no single
/// well-defined directory instance to check it within.
fn required_template_missing(
    entries: &[Entry],
    template: &Template,
    actual_entries: &[ActualEntry],
) -> Vec<Violation> {
    if template.is_checked_per_instance() {
        return required_template_missing_per_instance(entries, template, actual_entries);
    }
    let matched = actual_entries
        .iter()
        .any(|actual| actual.kind == template.kind() && allows(template, &actual.name));
    if matched {
        Vec::new()
    } else {
        vec![Violation::Missing {
            name: own_rule_text(template),
            origin: template.origin().clone(),
        }]
    }
}

/// Every [`Violation::Missing`] of a required `template` whose path has
/// more than one component, checked folder by folder rather than
/// "somewhere on disk".
///
/// The `!` covers everything its own line names, so
/// `!{{ name }}/{{ name }}.txt` is short for `!{{ name }}/` with
/// `!{{ name }}.txt` indented under it: the folder is required, and so is
/// the file in each such folder. The folders the line is indented under
/// (see [`Template::inherited_count`]) aren't required by it: each real
/// one is an instance the rule is checked within, independently, and
/// with none there's nothing to check (vacuously satisfied) — such a
/// folder's own presence, if required, is its own `!{{ name }}/`-style
/// entry. A line indented under nothing is checked once, from the top.
///
/// Instances are visited in sorted order for deterministic output. A
/// folder holding a `.flint` file of its own is left to that file's
/// check, which has its own copy of the rule (see
/// [`Template::delegating_to`]).
///
/// A violation names a concrete guessed path in place of the rule's raw
/// `{{ }}` syntax whenever [`guess_missing_leaf`] can build one from
/// bindings already known within that same folder — see its doc comment —
/// leaving whatever it can't resolve as written.
fn required_template_missing_per_instance(
    entries: &[Entry],
    template: &Template,
    actual_entries: &[ActualEntry],
) -> Vec<Violation> {
    let inherited = template.inherited_count();
    let instances = if inherited == 0 {
        vec![""]
    } else {
        let mut instances: Vec<&str> = actual_entries
            .iter()
            .filter(|actual| {
                actual.kind == EntryKind::Directory
                    && path_depth(&actual.name) == inherited
                    && reaches(template, &actual.name)
            })
            .map(|actual| actual.name.as_str())
            .collect();
        instances.sort_unstable();
        instances
    };

    let mut missing = Vec::new();
    for instance in instances {
        missing_within(entries, template, instance, actual_entries, &mut missing);
    }
    missing
}

/// Whether `folder` is one `template`'s path leads through: its leading
/// components fit `folder`'s, and their variables agree with each other
/// and with any `template` already has.
fn reaches(template: &Template, folder: &str) -> bool {
    template
        .governing_prefix_bindings(folder)
        .is_some_and(|captured| {
            let bindings: Vec<_> = template.bound().iter().cloned().chain(captured).collect();
            bindings_agree(&bindings)
        })
}

/// Whether the entry `name` is somewhere inside `folder` (`""` being the
/// top, which everything is inside).
fn is_inside(name: &str, folder: &str) -> bool {
    folder.is_empty()
        || name
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Adds to `missing` what required `template` still needs inside the real
/// `folder` (`""` for the top), which its path leads through — see
/// [`required_template_missing_per_instance`]. That's the rest of the
/// rule's path, reported once, if `folder` lacks the very next component;
/// otherwise whatever each subfolder that is one in turn lacks.
fn missing_within(
    entries: &[Entry],
    template: &Template,
    folder: &str,
    actual_entries: &[ActualEntry],
    missing: &mut Vec<Violation>,
) {
    // One with a `.flint` file of its own is checked from there.
    if template.is_delegated(folder) {
        return;
    }
    let depth = if folder.is_empty() {
        0
    } else {
        path_depth(folder)
    };
    let mut subfolders: Vec<&str> = Vec::new();
    let satisfied = if depth + 1 == template.component_count() {
        // Matched as a whole path, so the leaf's variables must agree
        // with the ones `folder`'s own name captured, or it doesn't
        // satisfy it.
        actual_entries.iter().any(|actual| {
            actual.kind == template.kind()
                && is_inside(&actual.name, folder)
                && allows(template, &actual.name)
        })
    } else {
        subfolders = actual_entries
            .iter()
            .filter(|actual| {
                actual.kind == EntryKind::Directory
                    && path_depth(&actual.name) == depth + 1
                    && is_inside(&actual.name, folder)
                    && reaches(template, &actual.name)
            })
            .map(|actual| actual.name.as_str())
            .collect();
        subfolders.sort_unstable();
        !subfolders.is_empty()
    };
    if !satisfied {
        let name = if folder.is_empty() {
            own_rule_text(template)
        } else {
            let leaf = leaf_rule_text(template.source_line(), template.cut() + depth);
            let rendered = guess_missing_leaf(entries, template, folder, leaf, actual_entries);
            format!("{folder}/{rendered}")
        };
        missing.push(Violation::Missing {
            name,
            origin: template.origin().clone(),
        });
    }
    for subfolder in subfolders {
        missing_within(entries, template, subfolder, actual_entries, missing);
    }
}

/// A concrete guess for `template`'s unsatisfied leaf (`leaf`, the raw
/// trailing `.flint` source text `leaf_rule_text` already sliced off) sat
/// within the real directory instance `boundary` (the leaf being one or
/// more trailing path components) — with whatever nothing
/// in that instance resolves left as written, so it's `leaf` unchanged if
/// there's nothing there to build a guess from.
///
/// Bindings for the leaf's variables are gathered from two sources, both
/// scoped to this one instance — never another (that's the invariant
/// [`required_template_missing_per_instance`]'s own doc comment
/// establishes, and this function must not violate it by reaching into a
/// different real directory):
/// - `template.governing_prefix_bindings(boundary)` — if the path leading
///   to `boundary` is itself templated, the real directory's own name
///   already tells us the value, no other file needed.
/// - every *other* template in `entries`, rebased beneath this same
///   `boundary` and matched against `actual_entries` — an
///   already-satisfied sibling rule (required or not) tells us what a
///   variable it shares with the missing leaf is really called here (a
///   rule that doesn't allow its file tells us nothing, so it's skipped). Only
///   *this* `boundary`'s own real entries are consulted, so a sibling
///   satisfied under a different instance can never leak in.
///
/// A variable is only resolved if every candidate binding found for it
/// canonicalizes (via [`format::canonical_words`]) to the same word list —
/// candidates can arrive under different `|format` filters (flint allows
/// that for the same variable name, only formatted-vs-unformatted must be
/// consistent), so this is the same "different spelling, same identifier"
/// reasoning `reconcile_match` already relies on, just requiring exact
/// agreement rather than merely a shared candidate, since a rendered guess
/// needs one definite answer, not a permissive check. Zero candidates, or
/// disagreeing ones, leave that variable unresolved.
///
/// Each variable is filled in on its own — see [`render_leaf`] — so a leaf
/// with more than one variable names the ones resolved here even when
/// another stays as written.
fn guess_missing_leaf(
    entries: &[Entry],
    template: &Template,
    boundary: &str,
    leaf: &str,
    actual_entries: &[ActualEntry],
) -> String {
    let mut candidates_by_variable: HashMap<String, Vec<(String, Option<Format>)>> = HashMap::new();

    for (variable, value, format) in template.bound() {
        candidates_by_variable
            .entry(variable.clone())
            .or_default()
            .push((value.clone(), *format));
    }
    // Always `Some`: `boundary` is a folder the caller found this path
    // leads through.
    let prefix = template.governing_prefix_bindings(boundary);
    for (variable, value, format) in prefix.expect("`boundary` is a folder this path leads through")
    {
        candidates_by_variable
            .entry(variable)
            .or_default()
            .push((value, format));
    }

    for entry in entries {
        let Entry::Template(other) = entry else {
            continue;
        };
        if std::ptr::eq(other, template) {
            continue; // the very rule we're trying to satisfy can't help guess itself
        }
        let Some(rebased) = other.rebase_beneath(boundary) else {
            continue;
        };
        for actual in actual_entries {
            let Some(bindings) = actual
                .name
                .strip_prefix(boundary)
                .and_then(|rest| rest.strip_prefix('/'))
                .filter(|_| actual.kind == rebased.kind())
                .and_then(|relative| rebased.matched_bindings(relative))
                .filter(|_| allows(other, &actual.name))
            else {
                continue;
            };
            for (variable, value, format) in bindings {
                candidates_by_variable
                    .entry(variable)
                    .or_default()
                    .push((value, format));
            }
        }
    }

    let mut resolved: HashMap<String, Vec<String>> = HashMap::new();
    for (variable, occurrences) in &candidates_by_variable {
        let mut words = occurrences
            .iter()
            .map(|(value, format)| format::canonical_words(value, *format));
        let first = words
            .next()
            .expect("only ever inserted alongside its first push");
        if words.all(|other| other == first) {
            resolved.insert(variable.clone(), first);
        }
    }

    render_leaf(leaf, &resolved)
}

/// Renders `leaf` (comment-free `.flint` source text for one or more
/// trailing path components, as produced by `leaf_rule_text`) with
/// `resolved` substituted in for every `{{ variable }}` occurrence it has
/// a value for, using [`format::render_words`] for its own `|format`.
///
/// With a value for every variable, and no `*` or `**`, that's one
/// concrete name, so the rule's `\` escapes are dropped too. Otherwise
/// it's still a rule: a variable with no entry in `resolved`, a wildcard
/// (there's no captured value to substitute for one) and an escape each
/// stay as written, around the values that are known.
fn render_leaf(leaf: &str, resolved: &HashMap<String, Vec<String>>) -> String {
    let concrete = flint_file::tokenize(leaf).ok().and_then(|segments| {
        let mut rendered = String::new();
        for segment in segments {
            match segment {
                Segment::Literal(text) => rendered.push_str(&text),
                Segment::Variable(name, format) => {
                    rendered.push_str(&format::render_words(resolved.get(&name)?, format));
                }
                Segment::Wildcard | Segment::AnyDepth => return None,
            }
        }
        Some(rendered)
    });
    concrete.unwrap_or_else(|| fill_known(leaf, resolved))
}

/// `leaf` as written, except for each `{{ variable }}` `resolved` has a
/// value for, which is rendered in its place — see [`render_leaf`].
fn fill_known(leaf: &str, resolved: &HashMap<String, Vec<String>>) -> String {
    let mut filled = String::new();
    let mut rest = leaf;
    while !rest.is_empty() {
        let variable = rest
            .strip_prefix("{{")
            .and_then(|_| rest.find("}}"))
            .map(|close| close + 2);
        let end = variable.unwrap_or_else(|| {
            // An escaped character is kept with its `\`, so `\{` never
            // opens a variable.
            let chars = if rest.starts_with('\\') { 2 } else { 1 };
            rest.char_indices()
                .nth(chars)
                .map_or(rest.len(), |(at, _)| at)
        });
        let (piece, after) = rest.split_at(end);
        // Only a whole `{{ variable }}` piece tokenizes to a variable.
        let known = match flint_file::tokenize(piece).as_deref() {
            Ok([Segment::Variable(name, format)]) => resolved
                .get(name)
                .map(|words| format::render_words(words, *format)),
            _ => None,
        };
        filled.push_str(known.as_deref().unwrap_or(piece));
        rest = after;
    }
    filled
}

/// The words of each value `template` already has from the folders it was
/// rebased beneath, for [`render_leaf`].
fn bound_words(template: &Template) -> HashMap<String, Vec<String>> {
    template
        .bound()
        .iter()
        .map(|(variable, value, format)| {
            (variable.clone(), format::canonical_words(value, *format))
        })
        .collect()
}

/// `template`'s own path as written — what rebasing hasn't cut off its
/// source line — with the values it already has from the folders it was
/// rebased beneath filled in: inside `Home/`, `{{ name }}/{{ name }}.swift`
/// is `Home.swift`.
fn own_rule_text(template: &Template) -> String {
    let text = leaf_rule_text(template.source_line(), template.cut());
    render_leaf(text, &bound_words(template))
}

/// The trailing `boundary_depth` `/`-separated components' worth of
/// `.flint` source text from `source_line` — e.g. with `boundary_depth ==
/// 1`, `"{{ name }}/{{ name }}Manager.py"` becomes
/// `"{{ name }}Manager.py"`. Safe to split on a plain `/` here: a
/// `.flint` path component can never itself contain a literal `/`, so
/// `source_line`'s `/` characters line up exactly with the template's own
/// path-component boundaries.
fn leaf_rule_text(source_line: &str, boundary_depth: usize) -> &str {
    source_line
        .splitn(boundary_depth + 1, '/')
        .last()
        .unwrap_or(source_line)
}

/// The number of `/`-separated components in `path`.
fn path_depth(path: &str) -> usize {
    path.split('/').count()
}

/// Combines the results of running [`check`] once per applicable origin
/// rule set against the *same* `actual_entries` (see the crate's
/// cascading-`.flint`-file design, `docs/flint-file-format.md`'s "Multiple
/// `.flint` files" section) into one final list.
///
/// [`Violation::GroupConflict`] and [`Violation::Missing`] are kept from
/// every origin verbatim — each reflects that one origin's own group
/// header or required entry, independent of the others. [`Violation::Unexpected`] is different: since every origin is
/// just another *source of permission* for the same directory, an entry
/// is only genuinely unexpected if *no* applicable origin permits it — so
/// it's kept only when every origin's own `check` result flagged it
/// unexpected too (set intersection, not union).
#[must_use]
pub fn merge_origin_violations(per_origin: &[Vec<Violation>]) -> Vec<Violation> {
    let mut conflicts = Vec::new();
    let mut missing = Vec::new();
    let mut unexpected: Option<BTreeSet<&str>> = None;

    for violations in per_origin {
        let mut here: BTreeSet<&str> = BTreeSet::new();
        for violation in violations {
            match violation {
                Violation::GroupConflict { .. } => conflicts.push(violation.clone()),
                Violation::Missing { .. } => missing.push(violation.clone()),
                Violation::Unexpected(name) => {
                    here.insert(name.as_str());
                }
            }
        }
        unexpected = Some(match unexpected {
            None => here,
            Some(previous) => previous.intersection(&here).copied().collect(),
        });
    }

    conflicts
        .into_iter()
        .chain(missing)
        .chain(
            unexpected
                .unwrap_or_default()
                .into_iter()
                .map(|name| Violation::Unexpected(name.to_string())),
        )
        .collect()
}

/// Whether `path` (a `/`-joined relative path to a real, existing
/// directory) is an *intermediate* directory that some rule's path
/// reaches through — e.g. `src` and `src/linux` are both governed by the
/// single rule `src/linux/main.py`. A governed directory is implicitly
/// permitted to exist (even with no rule directly permitting it as a
/// leaf) and is where `crate::run`'s directory walk recurses into.
///
/// A directory no rule's path reaches through is *not* governed, though a
/// rule can still allow it as a leaf (see [`permits_directory`]).
#[must_use]
pub fn is_governed_directory(entries: &[Entry], path: &str) -> bool {
    entries.iter().any(|entry| match entry {
        Entry::Literal { name, .. } => literal_matches_directory_prefix(name, path),
        Entry::Template(template) => template.matches_directory_prefix(path),
    })
}

/// Whether some rule in `entries` permits the directory at `path` (a
/// `/`-joined relative path to a real, existing directory): a rule that
/// allows it, like `src/` or `{{ name }}/`, or one whose path reaches
/// through it (see [`is_governed_directory`]). A permitted directory's
/// contents are checked against the rules too, so a bare `src/`, with
/// nothing reaching inside it, permits only an empty `src/` — `crate::run`'s
/// directory walk goes into every permitted directory. A directory no rule
/// permits is reported as unexpected, and its contents aren't listed.
#[must_use]
pub fn permits_directory(entries: &[Entry], path: &str) -> bool {
    let directory = ActualEntry {
        name: path.to_string(),
        kind: EntryKind::Directory,
    };
    is_governed_directory(entries, path)
        || entries.iter().any(|entry| entry_allows(entry, &directory))
}

/// The `Entry::Literal` counterpart to `Template::matches_directory_prefix`
/// — `name` must have strictly more `/`-separated components than `path`,
/// and the first that-many components of `name` must equal `path` exactly.
fn literal_matches_directory_prefix(name: &str, path: &str) -> bool {
    let name_components: Vec<&str> = name.split('/').collect();
    let path_components: Vec<&str> = path.split('/').collect();
    name_components.len() > path_components.len() && name_components.starts_with(&path_components)
}

/// Every entry of `scopes` that lies inside `scope`'s own directory, with
/// `scope`'s own path stripped off — e.g. given `scope = "src"` and
/// `scopes` containing `"src/linux"`, returns `{"linux"}`. `scope` and every
/// entry of `scopes` is `""` for the invocation directory itself, or a
/// `/`-joined path relative to it for anything nested.
///
/// Used to stop a scope's own directory walk from descending into a
/// nested directory that owns its own `.flint` file, since that directory
/// is checked by its own, independent pass instead of being folded into
/// this one.
#[must_use]
pub fn nested_scopes_relative_to<S: std::hash::BuildHasher>(
    scopes: &HashSet<&str, S>,
    scope: &str,
) -> HashSet<String> {
    scopes
        .iter()
        .filter(|s| **s != scope)
        .filter_map(|s| {
            if scope.is_empty() {
                Some((*s).to_string())
            } else {
                s.strip_prefix(scope)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .map(str::to_string)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flint_file::parse;

    fn unexpected(name: &str) -> Violation {
        Violation::Unexpected(name.to_string())
    }

    fn files(names: &[&str]) -> Vec<ActualEntry> {
        names
            .iter()
            .map(|name| ActualEntry {
                name: (*name).to_string(),
                kind: EntryKind::File,
            })
            .collect()
    }

    fn dirs(names: &[&str]) -> Vec<ActualEntry> {
        names
            .iter()
            .map(|name| ActualEntry {
                name: (*name).to_string(),
                kind: EntryKind::Directory,
            })
            .collect()
    }

    fn entries(flint_contents: &str) -> Vec<Entry> {
        parse(flint_contents).unwrap()
    }

    /// `violation` with its origins cleared.
    fn without_origin(mut violation: Violation) -> Violation {
        match &mut violation {
            Violation::Missing { origin, .. } | Violation::GroupConflict { origin, .. } => {
                *origin = Origin::default();
            }
            Violation::Unexpected(_) => {}
        }
        violation
    }

    /// [`super::check`], with every violation's origin cleared, so a test
    /// about *what* is found needn't spell out where each rule was written
    /// — the `origin_*` tests check that.
    fn check(entries: &[Entry], actual_entries: &[ActualEntry]) -> Vec<Violation> {
        super::check(entries, actual_entries)
            .into_iter()
            .map(without_origin)
            .collect()
    }

    /// A [`Violation::Missing`] for `name`, with no origin (see [`check`]).
    fn missing(name: &str) -> Violation {
        Violation::Missing {
            name: name.to_string(),
            origin: Origin::default(),
        }
    }

    #[test]
    fn no_entries_means_no_violations() {
        assert_eq!(check(&entries("a.txt"), &files(&[])), Vec::new());
    }

    #[test]
    fn matching_entries_are_not_violations() {
        let allowed = entries("a.txt\nb.txt");
        let actual = files(&["a.txt", "b.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn allowed_entries_missing_on_disk_are_fine() {
        let allowed = entries("a.txt\nb.txt");
        let actual = files(&["a.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn entries_not_in_allow_list_are_violations() {
        let allowed = entries("a.txt");
        let actual = files(&["a.txt", "intruder.txt"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected("intruder.txt")]);
    }

    #[test]
    fn violations_are_sorted() {
        let actual = files(&["z.txt", "a.txt", "m.txt"]);
        assert_eq!(
            check(&entries(""), &actual),
            vec![
                unexpected("a.txt"),
                unexpected("m.txt"),
                unexpected("z.txt"),
            ]
        );
    }

    #[test]
    fn consistent_template_match_across_two_lines_is_not_a_violation() {
        let allowed = entries("{{ name }}Manager.py\n{{ name }}Presenter.py");
        let actual = files(&["HomeManager.py", "HomePresenter.py"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn flat_sibling_templates_are_never_reconciled_against_each_other() {
        // `{{ name }}Manager.py` and `{{ name }}Presenter.py` are two
        // separate, un-nested `.flint` lines: they never share a real
        // directory instance, so mismatched names are fine — this is the
        // flip side of the previous test, not a disagreement.
        let allowed = entries("{{ name }}Manager.py\n{{ name }}Presenter.py");
        let actual = files(&["HomeManager.py", "DetailsPresenter.py"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn unmatched_template_line_is_fine() {
        let allowed = entries("{{ name }}Manager.py");
        assert_eq!(check(&allowed, &files(&[])), Vec::new());
    }

    #[test]
    fn mix_of_literal_and_template_entries() {
        let allowed = entries("README.md\n{{ name }}Manager.py");
        let actual = files(&["README.md", "HomeManager.py", "intruder.txt"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected("intruder.txt")]);
    }

    #[test]
    fn same_variable_twice_in_one_line_disagreeing_is_unexpected() {
        let allowed = entries("{{ name }}_{{ name }}.txt");
        let actual = files(&["Home_Details.txt"]);
        assert_eq!(
            check(&allowed, &actual),
            vec![unexpected("Home_Details.txt")]
        );
    }

    #[test]
    fn single_template_line_can_match_multiple_files_independently() {
        // A lone `{{ name }}.txt` rule has nothing to reconcile against
        // across files — each match is checked on its own.
        let allowed = entries("{{ name }}.txt");
        let actual = files(&["Bbb.txt", "Aaa.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn entries_whose_variables_disagree_are_reported_as_unexpected_in_name_order() {
        let allowed = entries("{{ name }}/{{ name }}Manager.py");
        let actual = vec![
            ActualEntry {
                name: "Apple".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "Apple/BananaManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "intruder.txt".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![
                unexpected("Apple/BananaManager.py"),
                unexpected("intruder.txt"),
            ]
        );
    }

    #[test]
    fn same_variable_twice_in_one_line_consistent_match_is_fine() {
        let allowed = entries("{{ name }}_{{ name }}.txt");
        let actual = files(&["Home_Home.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn multiple_rules_matching_the_same_file_is_fine() {
        let allowed = entries("{{ name }}Manager.py\nFoo{{ type }}.py");
        let actual = files(&["FooManager.py"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn directory_only_entry_matches_a_directory_but_not_a_file() {
        let allowed = entries("src/");
        assert_eq!(check(&allowed, &dirs(&["src"])), Vec::new());
        assert_eq!(check(&allowed, &files(&["src"])), vec![unexpected("src")]);
    }

    #[test]
    fn file_only_entry_matches_a_file_but_not_a_directory() {
        let allowed = entries("a.txt");
        assert_eq!(check(&allowed, &files(&["a.txt"])), Vec::new());
        assert_eq!(
            check(&allowed, &dirs(&["a.txt"])),
            vec![unexpected("a.txt")]
        );
    }

    #[test]
    fn template_with_trailing_slash_only_matches_directories() {
        let allowed = entries("{{ name }}/");
        assert_eq!(check(&allowed, &dirs(&["src"])), Vec::new());
        assert_eq!(check(&allowed, &files(&["src"])), vec![unexpected("src")]);
    }

    #[test]
    fn wildcard_rule_allows_any_matching_file_and_flags_the_rest() {
        let allowed = entries("*.py");
        let actual = files(&["Home.py", "Details.py", "notes.txt"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected("notes.txt")]);
    }

    #[test]
    fn wildcard_rule_still_enforces_file_vs_directory_kind() {
        let allowed = entries("*.py");
        assert_eq!(
            check(&allowed, &dirs(&["Home.py"])),
            vec![unexpected("Home.py")]
        );
    }

    #[test]
    fn wildcard_rule_does_not_match_a_dotfile() {
        let allowed = entries("*.py");
        let actual = files(&[".py"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected(".py")]);
    }

    #[test]
    fn required_literal_missing_on_disk_is_a_violation() {
        let allowed = entries("!README.md");
        assert_eq!(check(&allowed, &files(&[])), vec![missing("README.md")]);
    }

    #[test]
    fn required_literal_present_on_disk_is_fine() {
        let allowed = entries("!README.md");
        assert_eq!(check(&allowed, &files(&["README.md"])), Vec::new());
    }

    #[test]
    fn required_directory_of_wrong_kind_is_still_missing() {
        let allowed = entries("!src/");
        assert_eq!(
            check(&allowed, &files(&["src"])),
            vec![missing("src/"), unexpected("src"),]
        );
    }

    #[test]
    fn required_wildcard_is_satisfied_by_any_one_match() {
        let allowed = entries("!*.md");
        assert_eq!(
            check(&allowed, &files(&["README.md", "CHANGELOG.md"])),
            Vec::new()
        );
    }

    #[test]
    fn required_wildcard_missing_reports_source_line() {
        let allowed = entries("!*.md");
        assert_eq!(check(&allowed, &files(&[])), vec![missing("*.md")]);
    }

    #[test]
    fn missing_violations_appear_in_flint_declaration_order() {
        let allowed = entries("!b.txt\n!a.txt");
        assert_eq!(
            check(&allowed, &files(&[])),
            vec![missing("b.txt"), missing("a.txt"),]
        );
    }

    #[test]
    fn missing_violations_come_before_unexpected() {
        let allowed = entries("{{ name }}/{{ name }}Manager.py\n!README.md");
        let actual = vec![
            ActualEntry {
                name: "Apple".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "Apple/BananaManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "intruder.txt".to_string(),
                kind: EntryKind::File,
            },
        ];
        let violations = check(&allowed, &actual);
        assert_eq!(violations[0], missing("README.md"));
        assert!(matches!(violations[1], Violation::Unexpected { .. }));
        assert_eq!(violations[2], unexpected("intruder.txt"));
    }

    #[test]
    fn compound_nested_template_occurrences_must_still_agree_within_one_match() {
        let allowed = entries("{{ name }}/{{ name }}Manager.py");
        let actual = vec![
            ActualEntry {
                name: "Apple".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "Apple/BananaManager.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![unexpected("Apple/BananaManager.py")]
        );
    }

    #[test]
    fn a_literal_rule_allows_an_entry_a_template_nearly_matched() {
        let allowed = entries("{{ name }}/\n  {{ name }}.md\n  README.md\n");
        let actual = [
            dirs(&["apple"]),
            files(&["apple/apple.md", "apple/README.md"]),
        ]
        .concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn a_wildcard_rule_allows_an_entry_a_template_nearly_matched() {
        let allowed = entries("{{ name }}/\n  {{ name }}.md\n*/*.md\n");
        let actual = [dirs(&["apple"]), files(&["apple/notes.md"])].concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn a_consistent_template_allows_an_entry_another_template_nearly_matched() {
        let allowed = entries("{{ a }}_{{ a }}.txt\n{{ b }}.txt\n");
        assert_eq!(check(&allowed, &files(&["x_y.txt"])), Vec::new());
    }

    #[test]
    fn a_file_whose_variables_disagree_under_every_rule_is_unexpected() {
        let allowed = entries("{{ a }}_{{ a }}.txt\n{{ b }}_{{ b }}.{{ c }}\n");
        assert_eq!(
            check(&allowed, &files(&["x_y.txt"])),
            vec![unexpected("x_y.txt")]
        );
    }

    #[test]
    fn a_file_whose_variables_disagree_does_not_satisfy_a_required_template() {
        let allowed = entries("!{{ name }}_{{ name }}.txt");
        assert_eq!(
            check(&allowed, &files(&["x_y.txt"])),
            vec![missing("{{ name }}_{{ name }}.txt"), unexpected("x_y.txt"),]
        );
    }

    #[test]
    fn a_file_whose_variables_disagree_does_not_satisfy_a_required_template_per_instance() {
        let allowed = entries("{{ name }}/\n  !{{ name }}Manager.py\n");
        let actual = [dirs(&["Apple"]), files(&["Apple/BananaManager.py"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![
                missing("Apple/AppleManager.py"),
                unexpected("Apple/BananaManager.py"),
            ]
        );
    }

    #[test]
    fn guessed_missing_leaf_ignores_a_sibling_that_does_not_allow_its_file() {
        // `a-b-c.txt` fits the shape with `x` = `a`, but since that rule
        // doesn't allow it, its `x` isn't used to guess the missing file.
        let allowed = entries("d/\n  {{ x }}-{{ y }}-{{ y }}.txt\n  !{{ x }}.md\n");
        let actual = [dirs(&["d"]), files(&["d/a-b-c.txt"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("d/{{ x }}.md"), unexpected("d/a-b-c.txt"),]
        );
    }

    #[test]
    fn required_template_is_satisfied_independently_per_matching_directory_instance() {
        let allowed = entries("{{ name }}/\n  !{{ name }}Manager.py\n  !{{ name }}Presenter.py\n");
        let actual = vec![
            ActualEntry {
                name: "Apple".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "Apple/AppleManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "Apple/ApplePresenter.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "Banana".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "Banana/BananaManager.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        // Apple's own instance is fully satisfied and contributes nothing;
        // Banana's is missing its Presenter — rendered as a concrete guess
        // since `name` is resolvable directly from the governing `{{ name }}/`
        // directory's own real name, "Banana" (see `guess_missing_leaf`).
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("Banana/BananaPresenter.py")]
        );
    }

    #[test]
    fn required_template_with_no_matching_directory_instances_has_no_missing_violations() {
        let allowed = entries("{{ name }}/\n  !{{ name }}Manager.py\n");
        assert_eq!(check(&allowed, &files(&[])), Vec::new());
    }

    #[test]
    fn guessed_missing_leaf_is_inferred_from_a_satisfied_sibling_in_the_same_instance() {
        // The motivating example: a literal governing directory contributes
        // nothing itself, but the already-satisfied Manager sibling tells
        // us what `name` really is for the still-missing Presenter.
        let allowed =
            entries("foo/\n  !{{ name|pascal }}Manager.py\n  !{{ name|pascal }}Presenter.py\n");
        let actual = vec![
            ActualEntry {
                name: "foo".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "foo/BlahManager.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("foo/BlahPresenter.py")]
        );
    }

    #[test]
    fn guessed_missing_leaf_falls_back_to_raw_template_when_nothing_in_the_instance_resolves_it() {
        let allowed =
            entries("foo/\n  !{{ name|pascal }}Manager.py\n  !{{ name|pascal }}Presenter.py\n");
        let actual = vec![ActualEntry {
            name: "foo".to_string(),
            kind: EntryKind::Directory,
        }];
        assert_eq!(
            check(&allowed, &actual),
            vec![
                missing("foo/{{ name|pascal }}Manager.py"),
                missing("foo/{{ name|pascal }}Presenter.py"),
            ]
        );
    }

    #[test]
    fn guessed_missing_leaf_falls_back_to_raw_template_when_sibling_sources_disagree() {
        let allowed = entries(
            "foo/\n  !{{ name }}Manager.py\n  !{{ name }}Other.txt\n  !{{ name }}Presenter.py\n",
        );
        let actual = vec![
            ActualEntry {
                name: "foo".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "foo/BlahManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "foo/QuxOther.txt".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("foo/{{ name }}Presenter.py")]
        );
    }

    #[test]
    fn guessed_missing_leaf_never_borrows_bindings_from_a_different_directory_instance() {
        // `suffix` only ever appears in the leaves, shared between Manager
        // and Presenter within *one* instance — resolvable only via a
        // same-instance sibling (source B). macos's instance is fully
        // satisfied and must never leak its own `suffix` value into linux's
        // still-missing Presenter.
        let allowed =
            entries("{{ platform }}/\n  !{{ suffix }}Manager.py\n  !{{ suffix }}Presenter.py\n");
        let actual = vec![
            ActualEntry {
                name: "linux".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "linux/BetaManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "macos".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "macos/GammaManager.py".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "macos/GammaPresenter.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("linux/BetaPresenter.py")]
        );
    }

    #[test]
    fn guessed_missing_leaf_renders_a_snake_sourced_value_under_a_different_target_format() {
        // Confirms the acronym-word rule end to end: a `|snake`-captured
        // `xml_parser` (words ["xml", "parser"]) renders as `XmlParser`
        // under `|pascal` for the guessed Presenter, not `XMLParser`.
        let allowed = entries("foo/\n  !{{ name|snake }}.txt\n  !{{ name|pascal }}.md\n");
        let actual = vec![
            ActualEntry {
                name: "foo".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "foo/xml_parser.txt".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(check(&allowed, &actual), vec![missing("foo/XmlParser.md")]);
    }

    #[test]
    fn nested_scopes_relative_to_root_strips_nothing() {
        let scopes: HashSet<&str> = ["", "src", "src/linux"].into_iter().collect();
        assert_eq!(
            nested_scopes_relative_to(&scopes, ""),
            ["src", "src/linux"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
    }

    #[test]
    fn nested_scopes_relative_to_a_nested_scope_strips_its_own_prefix() {
        let scopes: HashSet<&str> = ["", "src", "src/linux", "docs"].into_iter().collect();
        assert_eq!(
            nested_scopes_relative_to(&scopes, "src"),
            ["linux"].into_iter().map(str::to_string).collect()
        );
    }

    #[test]
    fn nested_scopes_relative_to_ignores_a_false_prefix_match() {
        // "srcfoo" merely starts with the string "src" — it isn't a
        // subdirectory of "src", and must not be treated as one.
        let scopes: HashSet<&str> = ["src", "srcfoo"].into_iter().collect();
        assert_eq!(nested_scopes_relative_to(&scopes, "src"), HashSet::new());
    }

    #[test]
    fn nested_scopes_relative_to_excludes_the_scope_itself() {
        let scopes: HashSet<&str> = ["src"].into_iter().collect();
        assert_eq!(nested_scopes_relative_to(&scopes, "src"), HashSet::new());
    }

    #[test]
    fn is_governed_directory_true_for_literal_intermediate_component() {
        let allowed = entries("src/util.rs");
        assert!(is_governed_directory(&allowed, "src"));
        assert!(!is_governed_directory(&allowed, "src/util.rs"));
        assert!(!is_governed_directory(&allowed, "other"));
    }

    #[test]
    fn is_governed_directory_true_for_wildcard_intermediate_component() {
        let allowed = entries("src/*/main.py");
        assert!(is_governed_directory(&allowed, "src"));
        assert!(is_governed_directory(&allowed, "src/linux"));
        assert!(is_governed_directory(&allowed, "src/macos"));
        assert!(!is_governed_directory(&allowed, "other"));
    }

    #[test]
    fn is_governed_directory_true_for_variable_intermediate_component() {
        let allowed = entries("src/{{ platform }}/main.py");
        assert!(is_governed_directory(&allowed, "src"));
        assert!(is_governed_directory(&allowed, "src/linux"));
    }

    #[test]
    fn nested_literal_rule_matches_nested_actual_entry() {
        let allowed = entries("src/util.rs");
        assert_eq!(check(&allowed, &files(&["src/util.rs"])), Vec::new());
    }

    #[test]
    fn nested_wildcard_rule_matches_nested_actual_entry() {
        let allowed = entries("src/*/main.py");
        assert_eq!(check(&allowed, &files(&["src/linux/main.py"])), Vec::new());
    }

    #[test]
    fn intermediate_governed_directory_is_not_unexpected() {
        let allowed = entries("src/util.rs");
        let actual = vec![
            ActualEntry {
                name: "src".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "src/util.rs".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn sibling_file_in_governed_directory_is_unexpected() {
        let allowed = entries("src/util.rs");
        let actual = vec![
            ActualEntry {
                name: "src".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "src/util.rs".to_string(),
                kind: EntryKind::File,
            },
            ActualEntry {
                name: "src/extra.rs".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(check(&allowed, &actual), vec![unexpected("src/extra.rs")]);
    }

    #[test]
    fn ungoverned_directory_with_no_matching_rule_is_unexpected() {
        let allowed = entries("src/util.rs");
        let actual = dirs(&["other"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected("other")]);
    }

    #[test]
    fn required_nested_rule_reports_missing_with_nested_path() {
        let allowed = entries("!src/util.rs");
        assert_eq!(check(&allowed, &files(&[])), vec![missing("src/util.rs")]);
    }

    #[test]
    fn cross_directory_variable_values_are_independent_per_instance() {
        // `linux/main.py` and `macos/config.py` each independently
        // capture `platform` from whichever real directory they matched —
        // they're two different `{{ platform }}` instances, and never
        // needed to agree with each other in the first place.
        let allowed = entries("{{ platform }}/main.py\n{{ platform }}/config.py");
        let actual = files(&["linux/main.py", "macos/config.py"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn cross_format_occurrences_of_the_same_identifier_are_consistent() {
        let allowed = entries("{{ name|camel }}/\n  {{ name|pascal }}Manager.py\n");
        let actual = vec![
            ActualEntry {
                name: "foo".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "foo/FooManager.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn cross_format_disagreement_is_unexpected() {
        let allowed = entries("{{ name|camel }}/\n  {{ name|pascal }}Manager.py\n");
        let actual = vec![
            ActualEntry {
                name: "foo".to_string(),
                kind: EntryKind::Directory,
            },
            ActualEntry {
                name: "foo/BarManager.py".to_string(),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            check(&allowed, &actual),
            vec![unexpected("foo/BarManager.py")]
        );
    }

    #[test]
    fn same_format_flat_siblings_are_never_reconciled() {
        // `A_{{ name|pascal }}.txt` and `B_{{ name|pascal }}.txt` are two
        // separate, un-nested lines — never reconciled against each other,
        // regardless of format, same as any other pair of flat siblings.
        let allowed = entries("A_{{ name|pascal }}.txt\nB_{{ name|pascal }}.txt");
        let actual = files(&["A_XMLFile.txt", "B_XmlFile.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn same_format_flat_siblings_are_never_reconciled_even_with_no_shared_grouping() {
        let allowed = entries("A_{{ name|snake }}.txt\nB_{{ name|snake }}.txt");
        let actual = files(&["A_parse_xml_file.txt", "B_parse_x_m_l_file.txt"]);
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn format_shape_mismatch_is_unexpected() {
        let allowed = entries("{{ name|snake }}.txt");
        let actual = files(&["HomePage.txt"]);
        assert_eq!(check(&allowed, &actual), vec![unexpected("HomePage.txt")]);
    }

    #[test]
    fn any_depth_rule_matches_deeply_nested_actual_entry() {
        let allowed = entries("src/**/*.py");
        assert_eq!(
            check(&allowed, &files(&["src/linux/nested/main.py"])),
            Vec::new()
        );
    }

    #[test]
    fn any_depth_rule_flags_non_matching_deep_file_as_unexpected() {
        let allowed = entries("src/**/*.py");
        assert_eq!(
            check(&allowed, &files(&["src/linux/notes.txt"])),
            vec![unexpected("src/linux/notes.txt")]
        );
    }

    #[test]
    fn required_any_depth_rule_is_satisfied_by_any_deep_match() {
        let allowed = entries("!src/**/*.py");
        assert_eq!(
            check(&allowed, &files(&["src/linux/nested/main.py"])),
            Vec::new()
        );
    }

    #[test]
    fn required_any_depth_rule_missing_reports_source_line() {
        let allowed = entries("!src/**/*.py");
        assert_eq!(check(&allowed, &files(&[])), vec![missing("src/**/*.py")]);
    }

    #[test]
    fn merge_origin_violations_with_a_single_origin_is_unchanged() {
        let violations = vec![unexpected("intruder.txt")];
        assert_eq!(
            merge_origin_violations(std::slice::from_ref(&violations)),
            violations
        );
    }

    #[test]
    fn merge_origin_violations_keeps_unexpected_only_if_every_origin_agrees() {
        let permitted_by_one_origin = vec![Vec::new(), vec![unexpected("a.txt")]];
        assert_eq!(
            merge_origin_violations(&permitted_by_one_origin),
            Vec::new()
        );

        let unexpected_by_every_origin = vec![vec![unexpected("a.txt")], vec![unexpected("a.txt")]];
        assert_eq!(
            merge_origin_violations(&unexpected_by_every_origin),
            vec![unexpected("a.txt")]
        );
    }

    #[test]
    fn merge_origin_violations_keeps_missing_from_every_origin() {
        let per_origin = vec![vec![missing("README.md")], vec![missing("LICENSE")]];
        assert_eq!(
            merge_origin_violations(&per_origin),
            vec![missing("README.md"), missing("LICENSE"),]
        );
    }

    #[test]
    fn merge_origin_violations_orders_missing_then_unexpected() {
        let per_origin = vec![
            vec![unexpected("a.txt"), missing("README.md")],
            vec![unexpected("a.txt")],
        ];
        assert_eq!(
            merge_origin_violations(&per_origin),
            vec![missing("README.md"), unexpected("a.txt")]
        );
    }

    #[test]
    fn merge_origin_violations_drops_an_entry_another_origin_allows() {
        let per_origin = vec![vec![unexpected("x_y.txt")], Vec::new()];
        assert_eq!(merge_origin_violations(&per_origin), Vec::new());
    }

    #[test]
    fn required_template_is_not_satisfied_by_a_non_matching_entry_of_the_same_kind() {
        let allowed = entries("!*.md\nnotes.txt");
        assert_eq!(
            check(&allowed, &files(&["notes.txt"])),
            vec![missing("*.md")]
        );
    }

    #[test]
    fn per_instance_required_check_ignores_a_file_where_its_directory_would_be() {
        // `notes.txt` sits where a `{{ name }}/` instance would, but it's a
        // file, so there's no instance for a Manager to be missing from.
        let allowed = entries("{{ name }}/\n  !{{ name }}Manager.py\n*.txt\n");
        assert_eq!(check(&allowed, &files(&["notes.txt"])), Vec::new());
    }

    #[test]
    fn required_template_three_levels_deep_is_checked_per_innermost_instance() {
        let allowed = entries("{{ a }}/\n  {{ b }}/\n    !README.md\n");
        let actual = [dirs(&["x", "x/y", "x/z"]), files(&["x/y/README.md"])].concat();
        assert_eq!(check(&allowed, &actual), vec![missing("x/z/README.md")]);
    }

    #[test]
    fn required_any_depth_template_with_a_nested_prefix_is_missing_when_nothing_matches() {
        // Not a per-instance check, despite reaching through `src/{{ m }}/`:
        // with `**`, a match anywhere satisfies it, so with no `src/` at all
        // it's missing, rather than vacuously satisfied by zero instances.
        let allowed = entries("!src/{{ m }}/**/*.py\n");
        assert_eq!(
            check(&allowed, &files(&[])),
            vec![missing("src/{{ m }}/**/*.py")]
        );
    }

    #[test]
    fn required_any_depth_template_with_a_nested_prefix_needs_only_one_match() {
        // One match anywhere satisfies it, even with `src/a/` holding no
        // `lib/` at all: unlike a rule with no `**`, it isn't checked once
        // per `src/{{ m }}/` folder.
        let allowed = entries("!src/{{ m }}/lib/**/*.py\nsrc/{{ m }}/\n");
        let actual = [
            dirs(&["src", "src/a", "src/b", "src/b/lib", "src/b/lib/x"]),
            files(&["src/b/lib/x/main.py"]),
        ]
        .concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn same_format_disagreement_is_unexpected() {
        // `n` first appears as camel `foo`, then as pascal `Foo` (the same
        // identifier), then as pascal `Bar`. The disagreement is between the
        // two pascal values, so it should name `Foo`, not the first `foo`.
        let line = "{{ n|camel }}-{{ n|pascal }}-{{ n|pascal }}.txt";
        assert_eq!(
            check(&entries(line), &files(&["foo-Foo-Bar.txt"])),
            vec![unexpected("foo-Foo-Bar.txt")]
        );
    }

    fn origin(line: usize) -> Origin {
        Origin {
            scope: String::new(),
            line,
        }
    }

    #[test]
    fn origin_of_a_missing_literal_is_its_rule_s_line() {
        let violations = super::check(&entries("a.txt\n!README.md\n!src/"), &files(&[]));
        assert_eq!(
            violations,
            vec![
                Violation::Missing {
                    name: "README.md".to_string(),
                    origin: origin(2),
                },
                Violation::Missing {
                    name: "src/".to_string(),
                    origin: origin(3),
                },
            ]
        );
    }

    #[test]
    fn origin_of_a_missing_template_is_its_rule_s_line() {
        let violations = super::check(&entries("\n!*.md"), &files(&[]));
        assert_eq!(
            violations,
            vec![Violation::Missing {
                name: "*.md".to_string(),
                origin: origin(2),
            }]
        );
    }

    #[test]
    fn origin_of_a_per_instance_missing_template_is_the_indented_line() {
        let allowed = entries("{{ name }}/\n    !README.md");
        assert_eq!(
            super::check(&allowed, &dirs(&["a"])),
            vec![Violation::Missing {
                name: "a/README.md".to_string(),
                origin: origin(2),
            }]
        );
    }

    /// A [`Violation::GroupConflict`] for `group` in `folder`, with each
    /// value and its files.
    fn group_conflict(group: &str, folder: &str, values: &[(&str, &[&str])]) -> Violation {
        Violation::GroupConflict {
            group: group.to_string(),
            folder: folder.to_string(),
            values: values
                .iter()
                .map(|(value, files)| GroupValue {
                    value: (*value).to_string(),
                    files: files.iter().map(|file| (*file).to_string()).collect(),
                })
                .collect(),
            origin: Origin::default(),
        }
    }

    const PAIRED: &str = "src/\n    <name>\n        {{ name }}.md\n        {{ name }}.txt\n";

    fn src(names: &[&str]) -> Vec<ActualEntry> {
        [dirs(&["src"]), files(names)].concat()
    }

    #[test]
    fn a_group_allows_one_file_or_a_matching_pair() {
        for names in [
            &["src/blah.md"][..],
            &["src/blah.txt"],
            &["src/blah.md", "src/blah.txt"],
        ] {
            assert_eq!(
                check(&entries(PAIRED), &src(names)),
                Vec::new(),
                "{names:?}"
            );
        }
    }

    #[test]
    fn a_group_s_files_must_share_one_value() {
        assert_eq!(
            check(&entries(PAIRED), &src(&["src/blah.md", "src/foo.txt"])),
            vec![group_conflict(
                "src/<name>",
                "src",
                &[("blah", &["src/blah.md"]), ("foo", &["src/foo.txt"])]
            )]
        );
    }

    #[test]
    fn a_group_s_files_under_the_same_rule_must_share_one_value_too() {
        assert_eq!(
            check(&entries(PAIRED), &src(&["src/blah.md", "src/foo.md"])),
            vec![group_conflict(
                "src/<name>",
                "src",
                &[("blah", &["src/blah.md"]), ("foo", &["src/foo.md"])]
            )]
        );
    }

    #[test]
    fn a_group_conflict_lists_every_file_under_each_value() {
        assert_eq!(
            check(
                &entries(PAIRED),
                &src(&["src/blah.md", "src/blah.txt", "src/foo.md"])
            ),
            vec![group_conflict(
                "src/<name>",
                "src",
                &[
                    ("blah", &["src/blah.md", "src/blah.txt"]),
                    ("foo", &["src/foo.md"])
                ]
            )]
        );
    }

    #[test]
    fn a_group_allows_only_one_set_per_folder() {
        let names = [
            "src/monkey.md",
            "src/monkey.txt",
            "src/zebra.md",
            "src/zebra.txt",
        ];
        assert!(matches!(
            check(&entries(PAIRED), &src(&names))[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_group_value_is_found_whichever_order_files_and_rules_come_in() {
        // `a-notes.md` sorts first and fits `{{ name }}.md` as `a-notes`,
        // but `name` = `a` is the one value every file fits.
        let allowed = entries("<name>\n    {{ name }}.md\n    {{ name }}-notes.md\n");
        assert_eq!(check(&allowed, &files(&["a.md", "a-notes.md"])), Vec::new());
    }

    #[test]
    fn a_file_another_rule_allows_does_not_count_toward_a_group_s_value() {
        let allowed = entries("src/\n    <name>\n        {{ name }}.md\n        README.md\n");
        assert_eq!(
            check(&allowed, &src(&["src/blah.md", "src/README.md"])),
            Vec::new()
        );
    }

    #[test]
    fn a_file_only_a_group_allows_counts_toward_its_value() {
        assert!(matches!(
            check(&entries(PAIRED), &src(&["src/blah.md", "src/README.md"]))[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_group_reconciles_differently_formatted_rules() {
        let allowed = entries("<name>\n    {{ name|snake }}.rs\n    {{ name|pascal }}Test.rs\n");
        assert_eq!(
            check(&allowed, &files(&["home_page.rs", "HomePageTest.rs"])),
            Vec::new()
        );
        assert!(matches!(
            check(&allowed, &files(&["home_page.rs", "OtherTest.rs"]))[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_group_under_a_templated_folder_has_one_value_per_real_folder() {
        let allowed =
            entries("{{ dir }}/\n    <name>\n        {{ name }}.md\n        {{ name }}.txt\n");
        let actual = [
            dirs(&["a", "b"]),
            files(&["a/x.md", "a/x.txt", "b/y.md", "b/z.txt"]),
        ]
        .concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![group_conflict(
                "{{ dir }}/<name>",
                "b",
                &[("y", &["b/y.md"]), ("z", &["b/z.txt"])]
            )]
        );
    }

    #[test]
    fn a_group_s_value_reaches_into_a_subfolder_and_lists_it_with_a_slash() {
        let allowed =
            entries("<name>\n    {{ name }}/\n        {{ name }}.swift\n    {{ name }}.md\n");
        let actual = [dirs(&["Home"]), files(&["Home/Home.swift", "Home.md"])].concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
        let actual = [dirs(&["Home"]), files(&["Home/Home.swift", "Other.md"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![group_conflict(
                "<name>",
                "",
                &[
                    ("Home", &["Home/", "Home/Home.swift"]),
                    ("Other", &["Other.md"])
                ]
            )]
        );
    }

    #[test]
    fn a_group_conflict_leaves_out_an_entry_that_gives_no_value() {
        let allowed = entries("<y>\n    **/\n        {{ y }}.png\n        {{ y }}.gif\n");
        let actual = [dirs(&["a", "b"]), files(&["a/foo.gif", "a/foo2.png"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![group_conflict(
                "<y>",
                "",
                &[("foo", &["a/foo.gif"]), ("foo2", &["a/foo2.png"])]
            )]
        );
    }

    #[test]
    fn a_group_with_several_variables_shares_each_of_them() {
        let allowed =
            entries("<name, kind>\n    {{ name }}.{{ kind }}\n    {{ name }}_test.{{ kind }}\n");
        assert_eq!(check(&allowed, &files(&["a.rs", "a_test.rs"])), Vec::new());
        assert_eq!(
            check(&allowed, &files(&["a.rs", "a_test.py"])),
            vec![group_conflict(
                "<name, kind>",
                "",
                &[("a, rs", &["a.rs"]), ("a_test, py", &["a_test.py"])]
            )]
        );
    }

    const REQUIRED_PAIR: &str =
        "src/\n    <name>\n        !{{ name }}.md\n        !{{ name }}.txt\n";

    #[test]
    fn a_group_s_required_rules_require_nothing_in_a_folder_with_none_of_its_files() {
        assert_eq!(check(&entries(REQUIRED_PAIR), &src(&[])), Vec::new());
    }

    #[test]
    fn a_required_group_header_requires_a_set_in_its_folder() {
        let allowed = entries("src/\n    !<name>\n        {{ name }}.md\n        {{ name }}.txt\n");
        assert_eq!(check(&allowed, &src(&[])), vec![missing("src/<name>")]);
        assert_eq!(check(&allowed, &src(&["src/blah.txt"])), Vec::new());
    }

    #[test]
    fn a_required_group_header_requires_a_set_in_every_folder_it_sits_in() {
        let allowed = entries("{{ dir }}/\n    !<name, kind>\n        {{ name }}.{{ kind }}\n");
        let actual = [dirs(&["a", "b"]), files(&["a/x.rs"])].concat();
        assert_eq!(check(&allowed, &actual), vec![missing("b/<name, kind>")]);
    }

    #[test]
    fn a_file_another_rule_allows_does_not_satisfy_a_required_group_header() {
        let allowed = entries("!<name>\n    {{ name }}.md\nREADME.md\n");
        assert_eq!(
            check(&allowed, &files(&["README.md"])),
            vec![missing("<name>")]
        );
    }

    #[test]
    fn a_required_group_header_and_required_rules_need_a_complete_set() {
        let allowed = entries("!<name>\n    !{{ name }}.md\n    !{{ name }}.txt\n");
        assert_eq!(check(&allowed, &files(&[])), vec![missing("<name>")]);
        assert_eq!(
            check(&allowed, &files(&["blah.md"])),
            vec![missing("blah.txt")]
        );
    }

    #[test]
    fn a_group_s_missing_file_is_named_with_the_folder_s_value() {
        assert_eq!(
            check(&entries(REQUIRED_PAIR), &src(&["src/blah.md"])),
            vec![missing("src/blah.txt")]
        );
    }

    #[test]
    fn a_group_s_required_rule_waits_for_a_value_for_each_variable_it_uses() {
        let allowed = entries(
            "src/\n    <a, b>\n        !{{ a }}.txt\n        !{{ a }}.jpg\n        !{{ b }}.md\n        !{{ b }}.gif\n",
        );
        assert_eq!(
            check(&allowed, &src(&["src/foo.txt"])),
            vec![missing("src/foo.jpg")]
        );
        assert_eq!(
            check(&allowed, &src(&["src/foo.txt", "src/bar.md"])),
            vec![missing("src/foo.jpg"), missing("src/bar.gif")]
        );
    }

    #[test]
    fn a_group_s_required_rule_using_several_variables_waits_for_all_of_them() {
        let allowed =
            entries("<a, b>\n    !{{ a }}.txt\n    !{{ a }}-{{ b }}.md\n    {{ b }}.gif\n");
        assert_eq!(check(&allowed, &files(&["foo.txt"])), Vec::new());
        assert_eq!(
            check(&allowed, &files(&["foo.txt", "x.gif"])),
            vec![missing("foo-x.md")]
        );
    }

    #[test]
    fn a_group_s_missing_file_is_rendered_in_its_own_format() {
        let allowed = entries("<name>\n    !{{ name|snake }}.rs\n    !{{ name|pascal }}Test.rs\n");
        assert_eq!(
            check(&allowed, &files(&["home_page.rs"])),
            vec![missing("HomePageTest.rs")]
        );
    }

    #[test]
    fn a_group_s_missing_file_is_named_with_its_folder_s_name_too() {
        let allowed = entries(
            "{{ name|snake }}/\n    <foo>\n        !{{ name|title }}.txt\n        !{{ name|title }}.md\n        {{ foo }}.png\n",
        );
        let actual = [dirs(&["blah"]), files(&["blah/Blah.txt"])].concat();
        assert_eq!(check(&allowed, &actual), vec![missing("blah/Blah.md")]);
    }

    #[test]
    fn a_group_conflict_is_reported_instead_of_its_missing_files() {
        assert!(matches!(
            check(
                &entries(REQUIRED_PAIR),
                &src(&["src/blah.md", "src/foo.txt"])
            )[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_required_rule_in_a_group_s_subfolder_is_checked_per_subfolder() {
        let allowed = entries("<name>\n    {{ name }}/\n        !README.md\n    {{ name }}.md\n");
        let actual = [dirs(&["Home"]), files(&["Home.md"])].concat();
        assert_eq!(check(&allowed, &actual), vec![missing("Home/README.md")]);
    }

    #[test]
    fn group_conflicts_come_before_missing_and_unexpected_entries() {
        let allowed = entries(&format!("!LICENSE\n{PAIRED}"));
        let violations = check(&allowed, &src(&["src/a.md", "src/b.md", "stray"]));
        assert!(matches!(violations[0], Violation::GroupConflict { .. }));
        assert_eq!(violations[1..], [missing("LICENSE"), unexpected("stray")]);
    }

    #[test]
    fn merge_origin_violations_keeps_group_conflicts_from_every_origin() {
        let conflict = group_conflict("<name>", "", &[("a", &["a.md"]), ("b", &["b.md"])]);
        assert_eq!(
            merge_origin_violations(&[vec![conflict.clone()], vec![missing("README.md")]]),
            vec![conflict, missing("README.md")]
        );
    }

    #[test]
    fn origin_of_a_group_conflict_is_the_header_s_line() {
        let allowed = entries("\n<name>\n    {{ name }}.md\n");
        assert!(matches!(
            &super::check(&allowed, &files(&["a.md", "b.md"]))[..],
            [Violation::GroupConflict { origin: at, .. }] if *at == origin(2)
        ));
    }

    #[test]
    fn a_required_rule_in_a_group_s_subfolder_needs_no_subfolder_to_exist() {
        // `!{{ name }}.swift` sits in `{{ name }}/`, so it's required once
        // per such folder, not once per group folder — none exists here.
        let allowed =
            entries("<name>\n    {{ name }}/\n        !{{ name }}.swift\n    {{ name }}.md\n");
        assert_eq!(check(&allowed, &files(&["Home.md"])), Vec::new());
    }

    #[test]
    fn only_real_folders_at_a_group_s_depth_are_its_instances() {
        // Neither the file `notes.txt` nor the subfolder `a/b` is a
        // `{{ dir }}/` folder, so neither needs a set of its own.
        let allowed = entries(
            "*.txt\n{{ dir }}/\n    !<name>\n        {{ name }}.md\n        {{ name }}/\n            {{ name }}.swift\n",
        );
        let actual = [
            dirs(&["a", "a/b"]),
            files(&["notes.txt", "a/b.md", "a/b/b.swift"]),
        ]
        .concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn a_literal_rule_for_a_folder_does_not_take_a_file_out_of_a_group() {
        let allowed = entries("<name>\n    {{ name }}.md\n    {{ name }}.txt\na.md/\n");
        assert!(matches!(
            check(&allowed, &files(&["a.md", "b.txt"]))[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_wildcard_rule_for_folders_does_not_take_a_file_out_of_a_group() {
        let allowed = entries("<name>\n    {{ name }}.md\n    {{ name }}.txt\n*.md/\n");
        assert!(matches!(
            check(&allowed, &files(&["a.md", "b.txt"]))[..],
            [Violation::GroupConflict { .. }]
        ));
    }

    #[test]
    fn a_variable_outside_a_group_s_header_is_not_shared() {
        let allowed = entries("<name>\n    {{ name }}.{{ ext }}\n");
        assert_eq!(check(&allowed, &files(&["a.md", "a.txt"])), Vec::new());
    }

    /// A [`Reading`] giving `a` the value `value`.
    fn reading_of(value: &str) -> Reading {
        vec![("a".to_string(), value.to_string(), None)]
    }

    #[test]
    fn a_group_search_gives_up_after_its_step_limit() {
        // Every file but the last fits `a` = `x` two ways, and the last
        // only fits `y`: trying `x` first branches 2^17 times before
        // failing, past the limit, so `y` — which every file fits — is
        // never reached.
        let entries: Vec<ActualEntry> = files(&["f"]);
        let mut files: Vec<(&ActualEntry, Vec<Reading>)> =
            vec![(&entries[0], vec![reading_of("x"), reading_of("y")])];
        for _ in 0..17 {
            files.push((
                &entries[0],
                vec![reading_of("x"), reading_of("x"), reading_of("y")],
            ));
        }
        files.push((&entries[0], vec![reading_of("y")]));
        assert!(find_group_value(&files).is_none());
        // Cut short, the same search finds `y` within the limit.
        files.drain(1..8);
        assert!(find_group_value(&files).is_some());
    }

    const ANY_DEPTH_PAIR: &str =
        "src/\n    **/\n        <name>\n            {{ name }}.md\n            {{ name }}.txt\n";

    #[test]
    fn a_group_under_any_depth_has_one_value_per_real_folder_it_reaches() {
        let actual = [
            dirs(&["src", "src/a", "src/b", "src/b/c"]),
            files(&[
                "src/a/blah.md",
                "src/a/blah.txt",
                "src/b/c/foo.md",
                "src/b/c/bar.txt",
                "src/x.md",
            ]),
        ]
        .concat();
        assert_eq!(
            check(&entries(ANY_DEPTH_PAIR), &actual),
            vec![group_conflict(
                "src/**/<name>",
                "src/b/c",
                &[("bar", &["src/b/c/bar.txt"]), ("foo", &["src/b/c/foo.md"])]
            )]
        );
    }

    #[test]
    fn a_group_under_any_depth_shares_its_value_into_a_subfolder() {
        let allowed = entries(
            "src/\n    **/\n        <name>\n            {{ name }}/\n                {{ name }}.swift\n            {{ name }}.md\n",
        );
        let actual = [
            dirs(&["src", "src/a", "src/a/Home"]),
            files(&["src/a/Home/Home.swift", "src/a/Other.md"]),
        ]
        .concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![group_conflict(
                "src/**/<name>",
                "src/a",
                &[
                    ("Home", &["src/a/Home/Home.swift"]),
                    ("Other", &["src/a/Other.md"])
                ]
            )]
        );
    }

    const ANY_DEPTH_REQUIRED: &str =
        "src/\n    **/\n        !<name>\n            {{ name }}.md\n            !{{ name }}.txt\n";

    #[test]
    fn a_required_group_header_under_any_depth_needs_one_set_anywhere() {
        let empty = dirs(&["src", "src/a", "src/b"]);
        assert_eq!(
            check(&entries(ANY_DEPTH_REQUIRED), &empty),
            vec![missing("src/**/<name>")]
        );
        let one_set = [empty, files(&["src/b/foo.md", "src/b/foo.txt"])].concat();
        assert_eq!(check(&entries(ANY_DEPTH_REQUIRED), &one_set), Vec::new());
    }

    #[test]
    fn a_required_rule_under_any_depth_is_required_in_each_folder_with_a_value() {
        let actual = [
            dirs(&["src", "src/a", "src/b"]),
            files(&["src/a/foo.md", "src/a/foo.txt", "src/b/bar.md"]),
        ]
        .concat();
        assert_eq!(
            check(&entries(ANY_DEPTH_REQUIRED), &actual),
            vec![missing("src/b/bar.txt")]
        );
    }

    #[test]
    fn a_file_that_nearly_matches_a_group_s_rule_does_not_count_toward_its_value() {
        let allowed = entries("<name>\n    {{ name }}_{{ name }}.md\n    {{ name }}.txt\n");
        assert_eq!(
            check(&allowed, &files(&["a.txt", "x_y.md"])),
            vec![unexpected("x_y.md")]
        );
    }

    #[test]
    fn a_rule_with_its_own_any_depth_shares_the_group_s_folder() {
        // `**/{{ name }}.md` reaches any depth, but its group is the
        // `.flint` file's own folder, so `x/y/a.md` must agree with `a.txt`.
        let allowed = entries("<name>\n    {{ name }}.txt\n    **/{{ name }}.md\n");
        let actual = [dirs(&["x", "x/y"]), files(&["a.txt", "x/y/a.md"])].concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
        let actual = [dirs(&["x"]), files(&["a.txt", "x/b.md"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![group_conflict(
                "<name>",
                "",
                &[("a", &["a.txt"]), ("b", &["x/b.md"])]
            )]
        );
    }

    #[test]
    fn two_spellings_of_one_identifier_under_the_same_format_still_disagree() {
        // `XMLFile` and `XmlFile` read as the same words, but under the one
        // format `|pascal` they must be spelled the same too.
        let allowed = entries("{{ n|pascal }}/\n    {{ n|pascal }}.txt\n");
        let actual = [dirs(&["XMLFile"]), files(&["XMLFile/XmlFile.txt"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![unexpected("XMLFile/XmlFile.txt")]
        );
    }

    /// A lone folder named `name`, for a rule that requires something in it.
    fn folder(name: &str) -> Vec<ActualEntry> {
        vec![ActualEntry {
            name: name.to_string(),
            kind: EntryKind::Directory,
        }]
    }

    #[test]
    fn missing_leaf_names_the_variable_it_knows_beside_one_it_does_not() {
        // `name` is the folder's own name; nothing says what `blah` is.
        let allowed = entries("{{ name|title }}/\n  !{{ blah|snake }}.{{ name|kebab }}\n");
        assert_eq!(
            check(&allowed, &folder("All The World")),
            vec![missing("All The World/{{ blah|snake }}.all-the-world")]
        );
    }

    #[test]
    fn missing_leaf_names_the_variable_it_knows_beside_a_wildcard() {
        let allowed = entries("{{ name }}/\n  !*.{{ name }}.md\n");
        assert_eq!(
            check(&allowed, &folder("Home")),
            vec![missing("Home/*.Home.md")]
        );
    }

    #[test]
    fn missing_leaf_with_an_unknown_variable_keeps_the_rest_as_written() {
        // Still a rule, so the escapes and the unknown variable's own
        // spacing stay; `\{` doesn't open a variable.
        let allowed = entries("{{ name }}/\n  !é\\{{{blah}}\\#{{ name }}\\\n");
        assert_eq!(
            check(&allowed, &folder("Home")),
            vec![missing("Home/é\\{{{blah}}\\#Home\\")]
        );
    }

    #[test]
    fn missing_leaf_with_every_variable_known_drops_its_escapes() {
        let allowed = entries("{{ name }}/\n  !\\#{{ name }}\n");
        assert_eq!(
            check(&allowed, &folder("Home")),
            vec![missing("Home/#Home")]
        );
    }

    #[test]
    fn a_required_line_requires_each_folder_it_names() {
        // Short for `!{{ name }}/` with `!{{ name }}.txt` under it: an
        // empty `blah/` lacks its `blah/` folder, file and all.
        let allowed = entries("{{ name }}/\n    !{{ name }}/{{ name }}.txt\n");
        assert_eq!(
            check(&allowed, &folder("blah")),
            vec![missing("blah/blah/blah.txt")]
        );
    }

    #[test]
    fn a_required_line_s_folder_still_needs_its_file() {
        let allowed = entries("{{ name }}/\n    !{{ name }}/{{ name }}.txt\n");
        assert_eq!(
            check(&allowed, &dirs(&["blah", "blah/blah"])),
            vec![missing("blah/blah/blah.txt")]
        );
    }

    #[test]
    fn a_required_line_is_satisfied_by_its_folder_and_file() {
        let allowed = entries("{{ name }}/\n    !{{ name }}/{{ name }}.txt\n");
        let actual = [dirs(&["blah", "blah/blah"]), files(&["blah/blah/blah.txt"])].concat();
        assert_eq!(check(&allowed, &actual), Vec::new());
    }

    #[test]
    fn a_required_line_does_not_require_the_folders_it_is_indented_under() {
        let allowed = entries("{{ name }}/\n    !{{ name }}/{{ name }}.txt\n");
        assert_eq!(check(&allowed, &files(&[])), Vec::new());
    }

    #[test]
    fn a_required_line_names_a_literal_folder_it_lacks() {
        let allowed = entries("{{ name }}/\n    !sub/{{ name }}.txt\n");
        assert_eq!(
            check(&allowed, &folder("blah")),
            vec![missing("blah/sub/blah.txt")]
        );
    }

    #[test]
    fn a_required_line_at_the_top_requires_its_folder() {
        // Nothing to fill `name` in from, so the rule is named as written.
        let allowed = entries("!{{ name }}/{{ name }}.txt\n");
        assert_eq!(
            check(&allowed, &files(&[])),
            vec![missing("{{ name }}/{{ name }}.txt")]
        );
    }

    #[test]
    fn a_required_line_at_the_top_is_required_in_each_folder_it_names() {
        let allowed = entries("!{{ name }}/{{ name }}.txt\n");
        let actual = [dirs(&["a", "b"]), files(&["b/b.txt"])].concat();
        assert_eq!(check(&allowed, &actual), vec![missing("a/a.txt")]);
    }

    #[test]
    fn a_required_line_requires_every_folder_along_its_path() {
        // `a/` has its `lib/` but no `src/` in it; `b/` has no `lib/`.
        let allowed = entries("{{ name }}/\n    !lib/src/{{ name }}.txt\n");
        let actual = dirs(&["a", "a/lib", "b"]);
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("a/lib/src/a.txt"), missing("b/lib/src/b.txt")]
        );
    }

    #[test]
    fn a_folder_whose_name_disagrees_is_not_the_one_a_required_line_names() {
        // `a/b/` isn't `{{ name }}/{{ name }}/`, so `a/` still lacks `a/a/`.
        let allowed = entries("!{{ name }}/{{ name }}/x.txt\n");
        let actual = [dirs(&["a", "a/b"]), files(&["a/b/x.txt"])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("a/a/x.txt"), unexpected("a/b/x.txt")]
        );
    }

    #[test]
    fn a_file_is_not_a_folder_a_required_line_names() {
        let allowed = entries("!{{ name }}/x.txt\n*\n");
        assert_eq!(
            check(&allowed, &files(&["a"])),
            vec![missing("{{ name }}/x.txt")]
        );
    }

    #[test]
    fn a_folder_deeper_than_a_required_line_s_next_one_does_not_stand_in_for_it() {
        // `a/b/` is two folders down, where `{{ name }}/` is one.
        let allowed = entries("a/\n    !{{ name }}/x/y.txt\nb/**\n");
        let actual = [dirs(&["a", "b", "b/c"]), files(&[])].concat();
        assert_eq!(
            check(&allowed, &actual),
            vec![missing("a/{{ name }}/x/y.txt")]
        );
    }

    #[test]
    fn a_required_line_leaves_a_folder_with_its_own_flint_file_to_that_file() {
        let Entry::Template(template) = &entries("!{{ name }}/sub/x.txt\n")[0] else {
            panic!("expected a template entry");
        };
        let allowed = [Entry::Template(
            template.delegating_to(vec!["a".to_string()]),
        )];
        // `a/` exists, so the rule's folder isn't missing, and what `a/`
        // lacks is its own file's to report; `b/` is still checked here.
        assert_eq!(check(&allowed, &dirs(&["a"])), Vec::new());
        assert_eq!(
            check(&allowed, &dirs(&["a", "b"])),
            vec![missing("b/sub/x.txt")]
        );
    }
}
