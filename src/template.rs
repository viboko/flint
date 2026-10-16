// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compilation and matching of `.flint` lines that contain `{{ variable }}`
//! placeholders and/or `*` wildcards, across one or more `/`-separated path
//! components.

use std::fmt::Write as _;
use std::sync::OnceLock;

use regex::Regex;

use crate::entry_kind::EntryKind;
use crate::format::Format;
use crate::origin::Origin;

/// One piece of an already-tokenized `.flint` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    /// Literal text, matched verbatim.
    Literal(String),
    /// A `{{ name }}` or `{{ name|format }}` placeholder; the `String` is
    /// the logical variable name (e.g. `"name"`), not the internal regex
    /// group name. `Some(format)` if a `|format` filter was present.
    Variable(String, Option<Format>),
    /// A `*` wildcard: matches zero or more characters, never crossing a
    /// `/`. Carries no value (unlike `Variable`) since it isn't captured.
    Wildcard,
    /// A `**` wildcard: matches zero or more entire path components. Must
    /// be the only thing in its path component (enforced in
    /// `Template::compile`) — it can't blend with literal text, `*`, or
    /// `{{ variable }}` in the same component.
    AnyDepth,
}

/// One `/`-separated path component's compiled pattern. A `.flint` line
/// with no `/` compiles to a single `Component`; a line like
/// `src/{{ platform }}/main.py` compiles to three.
#[derive(Debug, Clone)]
struct Component {
    /// Anchored matcher for just this component (never sees a `/`, since
    /// the line was already split on `/` boundaries before this was
    /// built).
    regex: Matcher,
    /// Occurrence order within this component: internal regex group name
    /// -> logical variable name -> the format it was captured with, if
    /// any. Group names only need to be unique within one component's own
    /// `Regex`, since each component compiles to an independent `Regex`
    /// with its own capture namespace.
    group_to_variable: Vec<(String, String, Option<Format>)>,
    /// Whether this component's pattern is wildcard-led. Rust's `regex`
    /// crate has no look-around, so the shell-glob rule "a leading `*`
    /// never matches a dotfile" can't live inside `regex` itself — it's
    /// checked as a plain string-prefix test at match time instead.
    starts_with_wildcard: bool,
}

/// One component's anchored regex, in up to two versions.
///
/// A `|format` filter's shape takes in the letters of every script, which
/// makes a regex that's slow to compile — several hundred times slower
/// than the same shape over ASCII letters alone. So that one, `full`, is
/// only compiled on first meeting a name that needs it: one with a
/// character outside ASCII. Any other name is matched by `ascii`, which
/// gives the same answer for it (see [`Format::ascii_regex_fragment`]).
#[derive(Debug, Clone)]
struct Matcher {
    /// The regex for an ASCII name.
    ascii: Regex,
    /// The pattern `full` is compiled from, or `None` if it's the same
    /// as `ascii`'s — true of any component with no `|format` filter.
    full_pattern: Option<String>,
    /// The regex for any other name, once compiled.
    full: OnceLock<Regex>,
}

impl Matcher {
    /// Compiles `ascii_pattern` now, and keeps `full_pattern` to compile
    /// when first needed.
    fn new(ascii_pattern: &str, full_pattern: String) -> Self {
        Matcher {
            ascii: compile_pattern(ascii_pattern),
            full_pattern: (full_pattern != ascii_pattern).then_some(full_pattern),
            full: OnceLock::new(),
        }
    }

    /// The regex that decides whether `actual` matches, compiling the
    /// full one if this is the first name to need it.
    fn regex_for(&self, actual: &str) -> &Regex {
        match &self.full_pattern {
            Some(pattern) if !actual.is_ascii() => {
                self.full.get_or_init(|| compile_pattern(pattern))
            }
            _ => &self.ascii,
        }
    }

    /// [`Regex::captures`], with the regex for `actual`.
    fn captures<'a>(&self, actual: &'a str) -> Option<regex::Captures<'a>> {
        self.regex_for(actual).captures(actual)
    }

    /// [`Regex::is_match`], with the regex for `actual`.
    fn is_match(&self, actual: &str) -> bool {
        self.regex_for(actual).is_match(actual)
    }
}

/// Compiles a pattern built by [`ComponentBuilder`].
fn compile_pattern(pattern: &str) -> Regex {
    Regex::new(pattern).expect(
        "component regex is always well-formed: literals are escaped and group names are always `v<index>`",
    )
}

/// Accumulates one [`Component`]'s pattern while [`Template::compile`]
/// walks the tokenized segments, starting a fresh builder each time it
/// crosses a `/` boundary.
struct ComponentBuilder {
    /// The pattern so far, with each `|format` filter's ASCII fragment.
    pattern: String,
    /// `pattern`, but with each `|format` filter's any-script fragment
    /// in place of its ASCII one — see [`Matcher`].
    full_pattern: String,
    /// Becomes [`Component`]'s field of the same name.
    group_to_variable: Vec<(String, String, Option<Format>)>,
    /// Becomes [`Component`]'s field of the same name.
    starts_with_wildcard: bool,
}

impl ComponentBuilder {
    /// A builder for an empty component.
    fn new() -> Self {
        ComponentBuilder {
            pattern: String::from(r"\A"),
            full_pattern: String::from(r"\A"),
            group_to_variable: Vec::new(),
            starts_with_wildcard: false,
        }
    }

    /// Adds `text`, to be matched exactly.
    fn push_literal(&mut self, text: &str) {
        self.push_pattern(&regex::escape(text));
    }

    /// Adds a `{{ name }}` variable, captured in a named group of its
    /// own and limited to `format`'s shape, if any.
    fn push_variable(&mut self, name: String, format: Option<Format>) {
        let group_name = format!("v{}", self.group_to_variable.len());
        let any = || "[^/]+".to_string();
        let fragment = format.map_or_else(any, Format::ascii_regex_fragment);
        let full_fragment = format.map_or_else(any, Format::regex_fragment);
        write!(self.pattern, "(?P<{group_name}>{fragment})")
            .expect("writing to a String never fails");
        write!(self.full_pattern, "(?P<{group_name}>{full_fragment})")
            .expect("writing to a String never fails");
        self.group_to_variable.push((group_name, name, format));
    }

    /// Adds a `*`, which matches any run of characters but `/`.
    fn push_wildcard(&mut self) {
        if self.pattern == r"\A" {
            self.starts_with_wildcard = true;
        }
        self.push_pattern(r"[^/]*");
    }

    /// Adds `text` to the pattern, in both its versions.
    fn push_pattern(&mut self, text: &str) {
        self.pattern.push_str(text);
        self.full_pattern.push_str(text);
    }

    /// Anchors the pattern's end and compiles the [`Component`].
    fn finish(mut self) -> Component {
        self.push_pattern(r"\z");
        Component {
            regex: Matcher::new(&self.pattern, self.full_pattern),
            group_to_variable: self.group_to_variable,
            starts_with_wildcard: self.starts_with_wildcard,
        }
    }
}

/// Groups `segments` into one `Vec<Segment>` per `/`-separated path
/// component, splitting literal text on internal `/` occurrences (a
/// segment like `"src/"` can contain more than one). Every other segment
/// kind (`Variable`, `Wildcard`, `AnyDepth`) is pushed into whichever
/// group is currently open — `AnyDepth` needs no special handling here
/// since the literal `/` characters around it already force the
/// boundaries; [`Template::compile`] validates its placement afterward.
fn group_into_components(segments: Vec<Segment>) -> Vec<Vec<Segment>> {
    let mut groups: Vec<Vec<Segment>> = vec![Vec::new()];
    for segment in segments {
        match segment {
            Segment::Literal(text) => {
                let mut remaining = text.as_str();
                while let Some(slash_pos) = remaining.find('/') {
                    let (before, after) = remaining.split_at(slash_pos);
                    if !before.is_empty() {
                        groups
                            .last_mut()
                            .expect("groups always has at least one element")
                            .push(Segment::Literal(before.to_string()));
                    }
                    groups.push(Vec::new());
                    remaining = &after[1..];
                }
                if !remaining.is_empty() {
                    groups
                        .last_mut()
                        .expect("groups always has at least one element")
                        .push(Segment::Literal(remaining.to_string()));
                }
            }
            other => groups
                .last_mut()
                .expect("groups always has at least one element")
                .push(other),
        }
    }
    groups
}

/// Builds one path component's [`Component`] from its segments (already
/// validated to contain no `Segment::AnyDepth`).
fn build_component(segments: Vec<Segment>) -> Component {
    let mut builder = ComponentBuilder::new();
    for segment in segments {
        match segment {
            Segment::Literal(text) => builder.push_literal(&text),
            Segment::Variable(name, format) => builder.push_variable(name, format),
            Segment::Wildcard => builder.push_wildcard(),
            Segment::AnyDepth => {
                unreachable!("Template::compile validates AnyDepth out of component groups")
            }
        }
    }
    builder.finish()
}

/// Matches `components` against `actual` component-for-component
/// (requires equal length — callers ensure this by construction) —
/// per-component dotfile exclusion, then regex match, collecting
/// `{{ variable }}` bindings (with the format each was captured under, if
/// any) in order. Returns `None` on the first component that doesn't
/// match.
fn match_components(
    components: &[Component],
    actual: &[&str],
) -> Option<Vec<(String, String, Option<Format>)>> {
    let mut bindings = Vec::new();
    for (component, actual) in components.iter().zip(actual) {
        if component.starts_with_wildcard && actual.starts_with('.') {
            return None;
        }
        let captures = component.regex.captures(actual)?;
        for (group, variable, format) in &component.group_to_variable {
            let value = captures
                .name(group)
                .expect("group name comes from this component's own regex")
                .as_str()
                .to_string();
            bindings.push((variable.clone(), value, *format));
        }
    }
    Some(bindings)
}

/// Whether `actual` (which may be shorter than `components`) matches as
/// far as it goes — used for governance, where `actual` is a candidate
/// intermediate directory's path and may not yet reach the full pattern's
/// depth. No bindings are collected since governance doesn't need them.
fn components_match_partial(components: &[Component], actual: &[&str]) -> bool {
    components.iter().zip(actual).all(|(component, actual)| {
        if component.starts_with_wildcard && actual.starts_with('.') {
            return false;
        }
        component.regex.is_match(actual)
    })
}

/// Why a tokenized `.flint` line couldn't be compiled into a
/// [`Template`]. `flint_file` converts this into its own `ParseErrorKind`
/// (adding the line number and source text) when reporting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// `**` combined with other text, `*`, or `{{ variable }}` in the same
    /// path component (e.g. `pre**post`, `**post`, `{{ name }}**`).
    AnyDepthNotAlone,
    /// More than one `**` in the same `.flint` line.
    MultipleAnyDepth,
}

/// One `<name>` / `<name, kind>` group header a [`Template`] is indented
/// under. Every template under the same header, in the same real folder,
/// must share one value for each of the header's variables — see
/// `checker`'s group check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupMembership {
    /// Identifies the header within its own `.flint` file.
    pub(crate) id: usize,
    /// How many `/`-separated path components lead to the folder the
    /// header sits in, counting a `**` as one — 0 for the template's own
    /// top level. That folder is what's shared: each real folder there is
    /// one instance.
    pub(crate) depth: usize,
    /// If that path contains a `**`, how many components come before it.
    /// The header's folder then has no fixed depth: each real folder the
    /// `**` reaches is an instance.
    pub(crate) any_depth: Option<usize>,
    /// The header's variables, in the order written.
    pub(crate) variables: Vec<String>,
    /// The header as written, with its enclosing path, e.g.
    /// `src/<name>` — for messages.
    pub(crate) source_line: String,
    /// Where the header was written.
    pub(crate) origin: Origin,
    /// Whether a leading `!` requires each real folder the header sits in
    /// to hold a set of its rules' files, not just permit one.
    pub(crate) required: bool,
}

/// A compiled `.flint` line containing one or more `{{ variable }}`
/// placeholders, `*` wildcards, and/or (at most one) `**` wildcard, in one
/// or more `/`-separated path components. Built by `flint_file`'s
/// tokenizer, matched by `checker`.
#[derive(Debug, Clone)]
pub struct Template {
    /// Original `.flint` line text (trimmed, `!` marker stripped), kept
    /// for provenance in missing messages.
    source_line: String,
    /// Where the line was written.
    origin: Origin,
    /// Whether this template requires a directory or a file, set by a
    /// trailing `/` on the source line (already stripped from `regex` by
    /// the time it gets here). Only ever applies to the *final* path
    /// component — intermediate components are always directories,
    /// definitionally.
    kind: EntryKind,
    /// One compiled matcher per `/`-separated path component, in order.
    /// The whole path if there's no `**`; otherwise, everything *before*
    /// the `**`.
    components: Vec<Component>,
    /// `Some(suffix)` if this template contains a `**` — the components
    /// *after* it, matched against the last `suffix.len()` actual path
    /// components, with `**` freely absorbing everything in between (zero
    /// or more components, none of which may be dot-named — see
    /// `matched_bindings`). `None` if there's no `**`.
    any_depth_suffix: Option<Vec<Component>>,
    /// Whether a leading `!` requires at least one match on disk, not just
    /// permitting one.
    required: bool,
    /// Every group header this template is indented under, outermost
    /// first.
    groups: Vec<GroupMembership>,
    /// Values already captured by components [`Self::rebase_beneath`] cut
    /// off — the real folders this template was imported into — as
    /// `(variable, value, format)`, outermost first. Reported ahead of
    /// every match's own, so a match must agree with them.
    bound: Vec<(String, String, Option<Format>)>,
    /// How many of `source_line`'s leading `/`-separated components
    /// [`Self::rebase_beneath`] has cut off, in total.
    cut: usize,
    /// The folders, as paths in this template's own frame, whose own
    /// `.flint` file checks this template's `!` in place of this copy —
    /// see [`Self::delegating_to`].
    delegated: Vec<String>,
    /// How many of `source_line`'s leading `/`-separated components come
    /// from the lines this one is indented under, rather than from its
    /// own line — see [`Self::inherited_count`].
    inherited: usize,
}

impl Template {
    /// Builds a `Template` from `segments`, which must already be valid
    /// (produced by `flint_file`'s tokenizer, which rejects empty/invalid
    /// identifiers and adjacent-variable ambiguity before this is called).
    ///
    /// First groups `segments` into one `Vec<Segment>` per `/`-separated
    /// path component — a literal segment like `"src/"` can itself
    /// contain more than one `/`, so it's scanned for internal
    /// occurrences the same way `flint_file::split_trailing_slash` scans
    /// for a trailing one. `Segment::AnyDepth` (`**`) flows through this
    /// grouping like any other segment; no special-casing is needed there
    /// since the literal `/` characters already do the boundary-forcing.
    ///
    /// Then validates and splits: a component containing `**` mixed with
    /// anything else is [`TemplateError::AnyDepthNotAlone`]; more than
    /// one lone-`**` component is [`TemplateError::MultipleAnyDepth`].
    /// With exactly one lone `**` component, everything before it becomes
    /// `components` and everything after becomes `any_depth_suffix`, each
    /// built into [`Component`]s via [`ComponentBuilder`] exactly as
    /// before this method needed to know about `**` at all.
    ///
    /// # Errors
    /// Returns `Err` if `**` is combined with other content in one path
    /// component, or appears more than once.
    ///
    /// # Panics
    /// Never, for the `Regex` compilation itself: literal segments are
    /// passed through `regex::escape`, and internal group names are
    /// always `v<index>`, which is always valid `regex` group-name
    /// syntax. The `Regex::new(...).expect(...)` inside exists only
    /// because the `regex` API always returns `Result`, not because
    /// failure is reachable from valid input.
    pub(crate) fn compile(
        segments: Vec<Segment>,
        kind: EntryKind,
        source_line: String,
        origin: Origin,
        required: bool,
    ) -> Result<Template, TemplateError> {
        let groups = group_into_components(segments);

        let mut any_depth_index = None;
        for (index, group) in groups.iter().enumerate() {
            let has_any_depth = group.iter().any(|s| matches!(s, Segment::AnyDepth));
            if !has_any_depth {
                continue;
            }
            if group.len() != 1 {
                return Err(TemplateError::AnyDepthNotAlone);
            }
            if any_depth_index.is_some() {
                return Err(TemplateError::MultipleAnyDepth);
            }
            any_depth_index = Some(index);
        }

        let (prefix_groups, any_depth_suffix_groups) = match any_depth_index {
            None => (groups, None),
            Some(index) => {
                let mut groups = groups;
                let suffix = groups.split_off(index + 1);
                groups.pop(); // drop the `[AnyDepth]` marker group itself
                (groups, Some(suffix))
            }
        };

        let components = prefix_groups.into_iter().map(build_component).collect();
        let any_depth_suffix =
            any_depth_suffix_groups.map(|groups| groups.into_iter().map(build_component).collect());

        Ok(Template {
            source_line,
            origin,
            kind,
            components,
            any_depth_suffix,
            required,
            groups: Vec::new(),
            bound: Vec::new(),
            cut: 0,
            delegated: Vec::new(),
            inherited: 0,
        })
    }

    /// This template, with the first `inherited` components of its path
    /// coming from the lines it's indented under.
    pub(crate) fn with_inherited(mut self, inherited: usize) -> Template {
        self.inherited = inherited;
        self
    }

    /// How many of this template's leading components come from the lines
    /// its own line is indented under, not counting any rebasing has cut
    /// off: 1 for `!{{ name }}.py` under `{{ name }}/`, 0 for
    /// `!{{ name }}/{{ name }}.py` on one line. A `!` requires only what
    /// its own line names, so the folders before that are where it's
    /// checked, and the ones after are required along with the last.
    pub(crate) fn inherited_count(&self) -> usize {
        self.inherited.saturating_sub(self.cut)
    }

    /// Values already captured from the folders this template was
    /// rebased beneath, outermost first (see [`Self::rebase_beneath`]).
    pub(crate) fn bound(&self) -> &[(String, String, Option<Format>)] {
        &self.bound
    }

    /// How many of [`Self::source_line`]'s leading components rebasing has
    /// cut off: the rest is this template's own path, as written.
    pub(crate) fn cut(&self) -> usize {
        self.cut
    }

    /// This template, indented under `groups` (outermost first).
    pub(crate) fn with_groups(mut self, groups: Vec<GroupMembership>) -> Template {
        self.groups = groups;
        self
    }

    /// Every group header this template is indented under, outermost
    /// first.
    pub(crate) fn groups(&self) -> &[GroupMembership] {
        &self.groups
    }

    /// Whether this template's `!` is checked by `checker`'s group check
    /// rather than its own: it sits directly in the folder of the
    /// innermost group header it's indented under.
    pub(crate) fn is_checked_by_group(&self) -> bool {
        self.groups
            .last()
            .is_some_and(|group| self.components_after(group) == Some(1))
    }

    /// How many path components this template has after `group`'s
    /// folder — 1 for a rule directly in it — or `None` if a `**` after
    /// the header leaves that open.
    pub(crate) fn components_after(&self, group: &GroupMembership) -> Option<usize> {
        match (&self.any_depth_suffix, group.any_depth) {
            (None, _) => Some(self.components.len() - group.depth),
            // The header's own `**` is this template's only one.
            (Some(suffix), Some(_)) => Some(self.components.len() + 1 + suffix.len() - group.depth),
            (Some(_), None) => None,
        }
    }

    /// The real folder that `path` — a match for this template — sits in
    /// as far as `group` is concerned: its first `group.depth` components,
    /// or, with a `**` in the header's path, everything but the
    /// components after the header.
    pub(crate) fn group_folder(&self, group: &GroupMembership, path: &str) -> String {
        let mut components: Vec<&str> = path.split('/').collect();
        let kept = match (group.any_depth, self.components_after(group)) {
            (Some(_), Some(after)) => components.len().saturating_sub(after),
            _ => group.depth,
        };
        components.truncate(kept);
        components.join("/")
    }

    /// The original `.flint` line text this template was parsed from, used
    /// as provenance in missing messages.
    pub(crate) fn source_line(&self) -> &str {
        &self.source_line
    }

    /// Where this template's line was written.
    pub(crate) fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Whether this template requires a directory or a file.
    pub(crate) fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Whether a leading `!` requires at least one match on disk, not just
    /// permitting one.
    pub(crate) fn required(&self) -> bool {
        self.required
    }

    /// The number of `/`-separated path components before any `**` (or
    /// the whole path, if there's no `**`) — used by `checker`'s
    /// per-instance required-entry check to find the real directory that
    /// directly contains this template's leaf.
    pub(crate) fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Whether a `!` on this template is checked folder by folder along
    /// its path, rather than once anywhere on disk: its path has more
    /// than one component, and no `**`.
    pub(crate) fn is_checked_per_instance(&self) -> bool {
        self.component_count() > 1 && !self.has_any_depth()
    }

    /// Whether `variable` appears anywhere in this template's own path.
    pub(crate) fn uses_variable(&self, variable: &str) -> bool {
        self.components
            .iter()
            .chain(self.any_depth_suffix.iter().flatten())
            .flat_map(|component| &component.group_to_variable)
            .any(|(_, name, _)| name == variable)
    }

    /// Whether this template contains a `**`. `checker` falls back to its
    /// simpler "matched anywhere on disk" required-entry check for these,
    /// since `**` absorbs an unbounded number of directories, so "the
    /// directory directly containing the leaf" isn't fixed the way it is
    /// for every other template shape.
    pub(crate) fn has_any_depth(&self) -> bool {
        self.any_depth_suffix.is_some()
    }

    /// Attempts to match `path` (a `/`-joined relative path, or a bare
    /// name for a top-level entry) against this template in its entirety.
    /// Returns `None` if the shape doesn't match at all — including when
    /// `path` doesn't have the same number of `/`-separated components as
    /// this template.
    ///
    /// On a structural match, returns
    /// `(logical_variable, captured_value, format)` triples in occurrence
    /// order across all components, including one entry per occurrence
    /// when a variable is used more than once — preceded by any values
    /// already captured from the folders this template was rebased
    /// beneath (see [`Self::rebase_beneath`]), so that the same
    /// consistency check compares a match against them too. This deliberately does
    /// *not* check whether repeated occurrences of the same logical
    /// variable agree with each other — that consistency check applies
    /// uniformly across within-line, cross-line, and (for nested paths)
    /// cross-directory repeats, so it's the caller's (`checker`'s) job,
    /// not special-cased here.
    ///
    /// A component whose pattern starts with `*` never matches a dotfile
    /// (`.py`, `.hidden`, ...) in that position, mirroring shell globs
    /// — checked here as a plain prefix test rather than in the regex,
    /// since `regex` has no look-around to express "this position isn't a
    /// literal dot". A `**`, if present, never absorbs a dot-named
    /// component either, for the same reason `*` doesn't.
    pub(crate) fn matched_bindings(
        &self,
        path: &str,
    ) -> Option<Vec<(String, String, Option<Format>)>> {
        let mut bindings = self.bound.clone();
        bindings.extend(self.own_bindings(path)?);
        Some(bindings)
    }

    /// [`Self::matched_bindings`], without the values bound by rebasing.
    fn own_bindings(&self, path: &str) -> Option<Vec<(String, String, Option<Format>)>> {
        let path_components: Vec<&str> = path.split('/').collect();
        let Some(suffix) = &self.any_depth_suffix else {
            if path_components.len() != self.components.len() {
                return None;
            }
            return match_components(&self.components, &path_components);
        };

        let (prefix, rest) = path_components.split_at_checked(self.components.len())?;
        let (absorbed, tail) = rest.split_at_checked(rest.len().checked_sub(suffix.len())?)?;
        if !self.any_depth_absorbs(absorbed) {
            return None;
        }

        let mut bindings = match_components(&self.components, prefix)?;
        bindings.extend(match_components(suffix, tail)?);
        Some(bindings)
    }

    /// Whether `path` (a `/`-joined relative path to a real, existing
    /// directory) is an *intermediate* directory this template's path
    /// reaches through — used to decide whether to recurse into a
    /// directory during the filesystem walk, and whether that directory
    /// is implicitly permitted to exist even with no rule directly
    /// permitting it as a leaf.
    ///
    /// Without a `**`, `path` must have strictly fewer components than
    /// this template, matching as far as it goes. With a `**`, once
    /// `path` reaches the fixed prefix's depth and the prefix matches,
    /// every depth beyond it is governed unconditionally — `**` can
    /// always absorb more — except that none of the absorbed components
    /// may be dot-named, the same rule `matched_bindings` applies at the
    /// leaf.
    pub(crate) fn matches_directory_prefix(&self, path: &str) -> bool {
        let path_components: Vec<&str> = path.split('/').collect();
        if self.any_depth_suffix.is_none() {
            if path_components.len() >= self.components.len() {
                return false;
            }
            return components_match_partial(&self.components, &path_components);
        }

        let Some((prefix, absorbed)) = path_components.split_at_checked(self.components.len())
        else {
            return components_match_partial(&self.components, &path_components);
        };
        if match_components(&self.components, prefix).is_none() {
            return false;
        }
        self.any_depth_absorbs(absorbed)
    }

    /// Whether this template's `**` may absorb `components`. A `**` with
    /// more after it, like `src/**/*.py`, never reaches into a dot-named
    /// folder, the same way a leading `*` never matches a dot name. A
    /// trailing `**`, like `target/**`, means the whole folder, so it
    /// absorbs anything, dot names included.
    fn any_depth_absorbs(&self, components: &[&str]) -> bool {
        self.any_depth_suffix.as_ref().is_some_and(Vec::is_empty)
            || !components
                .iter()
                .any(|component| component.starts_with('.'))
    }

    /// If this template's path reaches strictly past `boundary` (a
    /// `/`-joined directory path relative to this template's own frame —
    /// see [`Self::matches_directory_prefix`]), returns the "remainder":
    /// a new `Template` describing what's left to match, now relative to
    /// `boundary`'s own directory instead. Returns `None` if this
    /// template doesn't reach that far, including when its whole path
    /// terminates exactly at `boundary` with nothing left inside it.
    ///
    /// Cutting a `**`'s any-depth absorption at `boundary` still leaves
    /// `**` in the remainder (with the same suffix) — `**` has no fixed
    /// consumed depth, so it can always absorb more beneath `boundary`
    /// too, regardless of how deep `boundary` itself already reaches.
    ///
    /// `source_line` and `required` are carried over unchanged — the
    /// remainder is still governed by the same original rule, just
    /// expressed relative to a deeper starting point — and so are the
    /// values `boundary`'s own components captured: `{{ name }}/` cut at
    /// `Home` leaves a remainder whose `{{ name }}` must still be `Home`
    /// (see [`Self::matched_bindings`]). The
    /// remainder belongs to no group header, since a group is only ever
    /// checked from its own `.flint` file. Used to import an
    /// ancestor `.flint` file's rule into a nested directory that owns
    /// its own `.flint` file, when the ancestor's rule reaches into it
    /// (see `docs/flint-file-format.md`'s "Multiple `.flint` files" section).
    pub(crate) fn rebase_beneath(&self, boundary: &str) -> Option<Template> {
        let boundary_components: Vec<&str> = boundary.split('/').collect();
        let k = boundary_components.len();

        // `cut`: how many of this template's own components `boundary`
        // used up — all `k`, unless a `**` absorbed some of them.
        let (components, any_depth_suffix, captured, cut) = match &self.any_depth_suffix {
            None => {
                let (cut_off, rest) = self.split_beyond(k)?;
                let captured = match_components(cut_off, &boundary_components)?;
                (rest.to_vec(), None, captured, k)
            }
            Some(suffix) => {
                let prefix_len = self.components.len();
                if let Some((within, absorbed)) = boundary_components.split_at_checked(prefix_len) {
                    let captured = match_components(&self.components, within)?;
                    if !self.any_depth_absorbs(absorbed) {
                        return None;
                    }
                    (Vec::new(), Some(suffix.clone()), captured, prefix_len)
                } else {
                    let (cut_off, rest) = self.split_beyond(k)?;
                    let captured = match_components(cut_off, &boundary_components)?;
                    (rest.to_vec(), Some(suffix.clone()), captured, k)
                }
            }
        };
        let mut bound = self.bound.clone();
        bound.extend(captured);

        Some(Template {
            source_line: self.source_line.clone(),
            origin: self.origin.clone(),
            kind: self.kind,
            components,
            any_depth_suffix,
            required: self.required && !self.is_checked_by_group(),
            // A group is checked once, from the `.flint` file that
            // declared it, across every folder below — never by an
            // imported copy (see `crate::run`). So a copy leaves its groups
            // behind, along with a `!` the group check owns.
            groups: Vec::new(),
            bound,
            cut: self.cut + cut,
            // Paths in the frame this copy has just left.
            delegated: Vec::new(),
            inherited: self.inherited,
        })
    }

    /// The bindings captured by matching just the leading `boundary`-deep
    /// components of this template's path against `boundary` itself (a
    /// `/`-joined real directory path, in this template's own frame — same
    /// convention as [`Self::rebase_beneath`]) — e.g. for
    /// `{{ name }}/{{ name }}Manager.py` and `boundary == "Apple"`,
    /// returns `[("name", "Apple", None)]`. This is exactly the
    /// information [`Self::rebase_beneath`] discards once it has confirmed
    /// the prefix matches, recovered here for callers that need to know
    /// *what* a governing directory's own template captured — e.g. to
    /// guess a missing sibling entry's expected value from the real
    /// directory it would live in, without needing a match against any
    /// other real file.
    ///
    /// Returns `None` if `boundary` has as many or more components than
    /// this template's whole path (there'd be nothing left for a
    /// leaf — mirrors [`Self::rebase_beneath`]'s own guard for the
    /// no-`**`, `k >= self.components.len()` case) or if `boundary`
    /// doesn't structurally match this template's leading components at
    /// all.
    pub(crate) fn governing_prefix_bindings(
        &self,
        boundary: &str,
    ) -> Option<Vec<(String, String, Option<Format>)>> {
        let boundary_components: Vec<&str> = boundary.split('/').collect();
        let (cut_off, _) = self.split_beyond(boundary_components.len())?;
        match_components(cut_off, &boundary_components)
    }

    /// This template's fixed components split after the first `k`, if
    /// at least one is left after them.
    fn split_beyond(&self, k: usize) -> Option<(&[Component], &[Component])> {
        self.components
            .split_at_checked(k)
            .filter(|(_, rest)| !rest.is_empty())
    }

    /// This template with `required` forced to `false` — used when a
    /// rule reaches past a directory that owns its own `.flint` file: it
    /// still contributes to matching/governance at this level, but its
    /// "must exist" obligation is delegated entirely to that nested
    /// `.flint` file's own, independently-imported and
    /// independently-checked copy (see [`Self::rebase_beneath`]), so it
    /// must not also be flagged missing here too.
    pub(crate) fn without_required(&self) -> Template {
        let mut copy = self.clone();
        copy.required = false;
        copy
    }

    /// This template, still required, but leaving each of `folders`
    /// (paths in this template's own frame) to the `.flint` file it
    /// holds, whose check gets its own copy of this rule (see
    /// [`Self::rebase_beneath`]). Unlike [`Self::without_required`], the
    /// folders this rule reaches that have no `.flint` file of their own
    /// are still checked here — for a rule `checker` checks once per real
    /// folder, where one folder having a `.flint` file says nothing about
    /// the others.
    pub(crate) fn delegating_to(&self, folders: Vec<String>) -> Template {
        let mut copy = self.clone();
        copy.delegated = folders;
        copy
    }

    /// Whether `folder`, or a folder it's inside, has a `.flint` file of
    /// its own that checks this template's `!` there — see
    /// [`Self::delegating_to`].
    pub(crate) fn is_delegated(&self, folder: &str) -> bool {
        self.delegated.iter().any(|delegated| {
            folder
                .strip_prefix(delegated.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(segments: Vec<Segment>) -> Template {
        Template::compile(
            segments,
            EntryKind::File,
            String::new(),
            Origin::default(),
            false,
        )
        .unwrap()
    }

    #[test]
    fn single_variable_matches_and_extracts_value() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("Manager.py".to_string()),
        ]);
        assert_eq!(
            template.matched_bindings("HomeManager.py"),
            Some(vec![("name".to_string(), "Home".to_string(), None)])
        );
    }

    #[test]
    fn formatted_variable_matches_only_its_own_shape_and_carries_the_format_through() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), Some(Format::Snake)),
            Segment::Literal(".txt".to_string()),
        ]);
        assert_eq!(
            template.matched_bindings("home_page.txt"),
            Some(vec![(
                "name".to_string(),
                "home_page".to_string(),
                Some(Format::Snake)
            )])
        );
        // Other five shapes don't fit `|snake`'s constrained capture regex.
        for rejected in [
            "HomePage.txt",
            "homePage.txt",
            "HOME_PAGE.txt",
            "home-page.txt",
            "HOME-PAGE.txt",
        ] {
            assert_eq!(
                template.matched_bindings(rejected),
                None,
                "{rejected:?} should not match a `|snake` placeholder"
            );
        }
    }

    #[test]
    fn no_structural_match_returns_none() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("Manager.py".to_string()),
        ]);
        assert_eq!(template.matched_bindings("Manager.py"), None);
        assert_eq!(template.matched_bindings("HomeManagerX.py"), None);
    }

    #[test]
    fn multiple_variables_extract_in_order() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("Manager.".to_string()),
            Segment::Variable("extension".to_string(), None),
        ]);
        assert_eq!(
            template.matched_bindings("HomeManager.py"),
            Some(vec![
                ("name".to_string(), "Home".to_string(), None),
                ("extension".to_string(), "py".to_string(), None),
            ])
        );
    }

    #[test]
    fn literal_segments_are_matched_verbatim_even_with_regex_metacharacters() {
        let template = compile(vec![Segment::Literal("a.b+c".to_string())]);
        assert_eq!(template.matched_bindings("a.b+c"), Some(vec![]));
        assert_eq!(template.matched_bindings("aXbXc"), None);
    }

    #[test]
    fn same_variable_twice_yields_two_unreconciled_occurrences() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("_".to_string()),
            Segment::Variable("name".to_string(), None),
            Segment::Literal(".txt".to_string()),
        ]);
        assert_eq!(
            template.matched_bindings("Home_Details.txt"),
            Some(vec![
                ("name".to_string(), "Home".to_string(), None),
                ("name".to_string(), "Details".to_string(), None),
            ])
        );
    }

    #[test]
    fn multiple_variables_require_the_literal_separator_to_match() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("Manager.".to_string()),
            Segment::Variable("extension".to_string(), None),
        ]);
        assert_eq!(template.matched_bindings("HomeManager"), None);
    }

    #[test]
    fn leading_wildcard_matches_varying_lengths_including_zero() {
        let template = compile(vec![Segment::Wildcard, Segment::Literal(".py".to_string())]);
        assert_eq!(template.matched_bindings("HomeManager.py"), Some(vec![]));
        assert_eq!(template.matched_bindings(".py"), None);
    }

    #[test]
    fn leading_wildcard_does_not_match_a_dotfile() {
        let template = compile(vec![Segment::Wildcard, Segment::Literal(".py".to_string())]);
        assert_eq!(template.matched_bindings(".py"), None);
        assert_eq!(template.matched_bindings(".hidden.py"), None);
    }

    #[test]
    fn non_leading_wildcard_can_still_match_zero_characters() {
        let template = compile(vec![
            Segment::Literal("a".to_string()),
            Segment::Wildcard,
            Segment::Literal(".txt".to_string()),
        ]);
        assert_eq!(template.matched_bindings("a.txt"), Some(vec![]));
        assert_eq!(template.matched_bindings("abc.txt"), Some(vec![]));
    }

    #[test]
    fn wildcard_after_a_literal_leading_dot_can_match_a_dotfile() {
        let template = compile(vec![
            Segment::Literal(".git".to_string()),
            Segment::Wildcard,
        ]);
        assert_eq!(template.matched_bindings(".gitignore"), Some(vec![]));
        assert_eq!(template.matched_bindings(".git"), Some(vec![]));
    }

    #[test]
    fn wildcard_never_matches_across_a_slash() {
        let template = compile(vec![
            Segment::Wildcard,
            Segment::Literal(".txt".to_string()),
        ]);
        assert_eq!(template.matched_bindings("a/b.txt"), None);
    }

    #[test]
    fn two_component_literal_pattern_requires_exactly_two_path_components() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert_eq!(template.matched_bindings("src/util.rs"), Some(vec![]));
        assert_eq!(template.matched_bindings("util.rs"), None);
        assert_eq!(template.matched_bindings("src/nested/util.rs"), None);
    }

    #[test]
    fn wildcard_in_intermediate_component_matches_any_directory_name() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Wildcard,
            Segment::Literal("/main.py".to_string()),
        ]);
        assert_eq!(template.matched_bindings("src/linux/main.py"), Some(vec![]));
        assert_eq!(template.matched_bindings("src/macos/main.py"), Some(vec![]));
        assert_eq!(template.matched_bindings("src/main.py"), None);
        assert_eq!(template.matched_bindings("src/linux/nested/main.py"), None);
    }

    #[test]
    fn variable_in_intermediate_component_captures_directory_name() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Variable("platform".to_string(), None),
            Segment::Literal("/main.py".to_string()),
        ]);
        assert_eq!(
            template.matched_bindings("src/linux/main.py"),
            Some(vec![("platform".to_string(), "linux".to_string(), None)])
        );
    }

    #[test]
    fn dotfile_exclusion_applies_per_component() {
        let template = compile(vec![
            Segment::Wildcard,
            Segment::Literal("/main.py".to_string()),
        ]);
        assert_eq!(template.matched_bindings(".hidden/main.py"), None);
        assert_eq!(template.matched_bindings("visible/main.py"), Some(vec![]));
    }

    #[test]
    fn literal_intermediate_component_is_unaffected_by_dotfile_exclusion() {
        let template = compile(vec![
            Segment::Literal(".git/".to_string()),
            Segment::Wildcard,
        ]);
        assert_eq!(template.matched_bindings(".git/HEAD"), Some(vec![]));
    }

    #[test]
    fn matches_directory_prefix_is_true_only_at_intermediate_depths() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Wildcard,
            Segment::Literal("/main.py".to_string()),
        ]);
        assert!(template.matches_directory_prefix("src"));
        assert!(template.matches_directory_prefix("src/linux"));
        assert!(!template.matches_directory_prefix("src/linux/main.py"));
        assert!(!template.matches_directory_prefix("src/linux/extra"));
        assert!(!template.matches_directory_prefix("other"));
    }

    #[test]
    fn matches_directory_prefix_with_variable_intermediate_component() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Variable("platform".to_string(), None),
            Segment::Literal("/main.py".to_string()),
        ]);
        assert!(template.matches_directory_prefix("src"));
        assert!(template.matches_directory_prefix("src/linux"));
        assert!(template.matches_directory_prefix("src/macos"));
        assert!(!template.matches_directory_prefix("other/linux"));
    }

    fn src_any_depth_py() -> Template {
        compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::AnyDepth,
            Segment::Literal("/".to_string()),
            Segment::Wildcard,
            Segment::Literal(".py".to_string()),
        ])
    }

    #[test]
    fn any_depth_matches_zero_or_more_absorbed_components() {
        let template = src_any_depth_py();
        assert_eq!(template.matched_bindings("src/main.py"), Some(vec![]));
        assert_eq!(
            template.matched_bindings("src/linux/nested/main.py"),
            Some(vec![])
        );
    }

    #[test]
    fn any_depth_requires_prefix_and_suffix_to_still_match() {
        let template = src_any_depth_py();
        assert_eq!(template.matched_bindings("other/main.py"), None);
        assert_eq!(template.matched_bindings("src/App.txt"), None);
    }

    #[test]
    fn any_depth_skips_a_dot_named_absorbed_directory() {
        let template = src_any_depth_py();
        assert_eq!(template.matched_bindings("src/.git/main.py"), None);
    }

    /// `target/**`: a `**` with nothing after it.
    fn trailing_any_depth() -> Template {
        compile(vec![
            Segment::Literal("target/".to_string()),
            Segment::AnyDepth,
        ])
    }

    #[test]
    fn trailing_any_depth_matches_dot_names_at_any_depth() {
        let template = trailing_any_depth();
        assert_eq!(
            template.matched_bindings("target/.rustc_info.json"),
            Some(vec![])
        );
        assert_eq!(
            template.matched_bindings("target/debug/.fingerprint/x"),
            Some(vec![])
        );
    }

    #[test]
    fn trailing_any_depth_governs_a_dot_named_directory() {
        assert!(trailing_any_depth().matches_directory_prefix("target/debug/.fingerprint"));
        assert!(!src_any_depth_py().matches_directory_prefix("src/.git"));
    }

    #[test]
    fn trailing_any_depth_rebases_beneath_a_dot_named_directory() {
        let rebased = trailing_any_depth()
            .rebase_beneath("target/.cache")
            .unwrap();
        assert_eq!(rebased.matched_bindings("x"), Some(vec![]));
        assert!(src_any_depth_py().rebase_beneath("src/.git").is_none());
    }

    #[test]
    fn any_depth_with_empty_prefix_matches_at_any_depth() {
        let template = compile(vec![
            Segment::AnyDepth,
            Segment::Literal("/file.txt".to_string()),
        ]);
        assert_eq!(template.matched_bindings("file.txt"), Some(vec![]));
        assert_eq!(template.matched_bindings("a/b/file.txt"), Some(vec![]));
        assert_eq!(template.matched_bindings("file.txt.bak"), None);
    }

    #[test]
    fn variable_in_suffix_captures_correctly_through_any_depth() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::AnyDepth,
            Segment::Literal("/".to_string()),
            Segment::Variable("name".to_string(), None),
            Segment::Literal(".py".to_string()),
        ]);
        assert_eq!(
            template.matched_bindings("src/linux/Home.py"),
            Some(vec![("name".to_string(), "Home".to_string(), None)])
        );
    }

    #[test]
    fn explicit_literal_dot_prefix_before_any_depth_is_unaffected_by_dotfile_skip() {
        let template = compile(vec![
            Segment::Literal(".git/".to_string()),
            Segment::AnyDepth,
        ]);
        assert_eq!(template.matched_bindings(".git/HEAD"), Some(vec![]));
        assert_eq!(
            template.matched_bindings(".git/refs/heads/main"),
            Some(vec![])
        );
    }

    #[test]
    fn any_depth_combined_with_other_text_is_an_error() {
        let result = Template::compile(
            vec![
                Segment::Literal("pre".to_string()),
                Segment::AnyDepth,
                Segment::Literal("post".to_string()),
            ],
            EntryKind::File,
            String::new(),
            Origin::default(),
            false,
        );
        assert_eq!(result.unwrap_err(), TemplateError::AnyDepthNotAlone);
    }

    #[test]
    fn any_depth_with_trailing_text_in_same_component_is_an_error() {
        let result = Template::compile(
            vec![Segment::AnyDepth, Segment::Literal("post".to_string())],
            EntryKind::File,
            String::new(),
            Origin::default(),
            false,
        );
        assert_eq!(result.unwrap_err(), TemplateError::AnyDepthNotAlone);
    }

    #[test]
    fn multiple_any_depth_in_one_line_is_an_error() {
        let result = Template::compile(
            vec![
                Segment::Literal("a/".to_string()),
                Segment::AnyDepth,
                Segment::Literal("/b/".to_string()),
                Segment::AnyDepth,
                Segment::Literal("/c".to_string()),
            ],
            EntryKind::File,
            String::new(),
            Origin::default(),
            false,
        );
        assert_eq!(result.unwrap_err(), TemplateError::MultipleAnyDepth);
    }

    #[test]
    fn matches_directory_prefix_is_unbounded_beneath_any_depth() {
        let template = src_any_depth_py();
        assert!(template.matches_directory_prefix("src"));
        assert!(template.matches_directory_prefix("src/linux"));
        assert!(template.matches_directory_prefix("src/linux/nested/deep"));
        assert!(!template.matches_directory_prefix("other"));
    }

    #[test]
    fn matches_directory_prefix_stops_at_a_dot_named_absorbed_directory() {
        let template = src_any_depth_py();
        assert!(!template.matches_directory_prefix("src/.git"));
    }

    #[test]
    fn source_line_is_kept_verbatim() {
        let template = Template::compile(
            vec![Segment::Literal("x".to_string())],
            EntryKind::File,
            "{{ name }}Manager.py".to_string(),
            Origin::default(),
            false,
        )
        .unwrap();
        assert_eq!(template.source_line(), "{{ name }}Manager.py");
    }

    #[test]
    fn rebase_beneath_a_literal_intermediate_component_strips_it() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        let rebased = template.rebase_beneath("src").unwrap();
        assert_eq!(rebased.matched_bindings("util.rs"), Some(vec![]));
        assert_eq!(rebased.matched_bindings("other.rs"), None);
    }

    #[test]
    fn rebase_beneath_returns_none_when_boundary_reaches_the_leaf() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert!(template.rebase_beneath("src/util.rs").is_none());
        assert!(template.rebase_beneath("src/util.rs/extra").is_none());
    }

    #[test]
    fn rebase_beneath_returns_none_for_a_non_matching_boundary() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert!(template.rebase_beneath("lib").is_none());
    }

    #[test]
    fn rebase_beneath_keeps_the_value_the_boundary_captured() {
        let template = compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Variable("platform".to_string(), None),
            Segment::Literal("/main.py".to_string()),
        ]);
        let rebased = template.rebase_beneath("src/linux").unwrap();
        assert_eq!(
            rebased.matched_bindings("main.py"),
            Some(vec![("platform".to_string(), "linux".to_string(), None)])
        );
    }

    #[test]
    fn rebase_beneath_twice_keeps_both_boundaries_values_outermost_first() {
        let template = compile(vec![
            Segment::Variable("a".to_string(), None),
            Segment::Literal("/".to_string()),
            Segment::Variable("b".to_string(), None),
            Segment::Literal("/x.txt".to_string()),
        ]);
        let rebased = template
            .rebase_beneath("one")
            .unwrap()
            .rebase_beneath("two")
            .unwrap();
        assert_eq!(
            rebased.matched_bindings("x.txt"),
            Some(vec![
                ("a".to_string(), "one".to_string(), None),
                ("b".to_string(), "two".to_string(), None),
            ])
        );
    }

    #[test]
    fn rebase_beneath_at_the_start_of_any_depth_keeps_any_depth_in_the_remainder() {
        let template = src_any_depth_py();
        let rebased = template.rebase_beneath("src").unwrap();
        assert_eq!(rebased.matched_bindings("main.py"), Some(vec![]));
        assert_eq!(
            rebased.matched_bindings("linux/nested/main.py"),
            Some(vec![])
        );
    }

    #[test]
    fn rebase_beneath_deep_inside_any_depth_still_keeps_any_depth_in_the_remainder() {
        let template = src_any_depth_py();
        let rebased = template.rebase_beneath("src/linux/nested").unwrap();
        assert_eq!(rebased.matched_bindings("main.py"), Some(vec![]));
        assert_eq!(rebased.matched_bindings("deeper/main.py"), Some(vec![]));
    }

    #[test]
    fn rebase_beneath_refuses_a_dot_named_component_absorbed_by_any_depth() {
        let template = src_any_depth_py();
        assert!(template.rebase_beneath("src/.git").is_none());
    }

    #[test]
    fn governing_prefix_bindings_captures_a_variable_consumed_by_the_boundary() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("/".to_string()),
            Segment::Variable("name".to_string(), None),
            Segment::Literal("Manager.py".to_string()),
        ]);
        assert_eq!(
            template.governing_prefix_bindings("Apple"),
            Some(vec![("name".to_string(), "Apple".to_string(), None)])
        );
    }

    #[test]
    fn governing_prefix_bindings_is_empty_for_a_variable_free_literal_prefix() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert_eq!(template.governing_prefix_bindings("src"), Some(vec![]));
    }

    #[test]
    fn governing_prefix_bindings_returns_none_when_boundary_reaches_the_leaf() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert_eq!(template.governing_prefix_bindings("src/util.rs"), None);
    }

    #[test]
    fn governing_prefix_bindings_returns_none_for_a_non_matching_boundary() {
        let template = compile(vec![Segment::Literal("src/util.rs".to_string())]);
        assert_eq!(template.governing_prefix_bindings("lib"), None);
    }

    #[test]
    fn without_required_clears_the_flag_but_keeps_everything_else() {
        let template = Template::compile(
            vec![Segment::Literal("x".to_string())],
            EntryKind::File,
            "!x".to_string(),
            Origin::default(),
            true,
        )
        .unwrap();
        assert!(template.required());
        let unrequired = template.without_required();
        assert!(!unrequired.required());
        assert_eq!(unrequired.source_line(), "!x");
        assert_eq!(unrequired.matched_bindings("x"), Some(vec![]));
    }

    #[test]
    fn delegating_to_stays_required_and_names_the_folders_left_to_their_own_flint_file() {
        let template = Template::compile(
            vec![
                Segment::Variable("name".to_string(), None),
                Segment::Literal("/x".to_string()),
            ],
            EntryKind::File,
            "{{ name }}/x".to_string(),
            Origin::default(),
            true,
        )
        .unwrap();
        assert!(!template.is_delegated("a"));
        let delegating = template.delegating_to(vec!["a".to_string(), "b/c".to_string()]);
        assert!(delegating.required());
        assert!(delegating.is_delegated("a"));
        assert!(delegating.is_delegated("a/below"));
        assert!(delegating.is_delegated("b/c"));
        assert!(!delegating.is_delegated("b"));
        assert!(!delegating.is_delegated("ab"));
        assert!(!delegating.is_delegated("other"));
        // A copy rebased into another folder is in another frame.
        assert!(!delegating.rebase_beneath("a").unwrap().is_delegated("a"));
    }

    #[test]
    fn kind_reflects_what_was_passed_to_compile() {
        let template = Template::compile(
            vec![Segment::Literal("x".to_string())],
            EntryKind::Directory,
            String::new(),
            Origin::default(),
            false,
        )
        .unwrap();
        assert_eq!(template.kind(), EntryKind::Directory);
    }

    #[test]
    fn any_depth_template_does_not_match_a_path_too_short_for_its_prefix_and_suffix() {
        let template = src_any_depth_py();
        assert_eq!(template.matched_bindings("src"), None);
        assert_eq!(template.matched_bindings("main.py"), None);
    }

    /// `src/{{ module }}/**/*.py`: a `**` template whose fixed prefix has
    /// two components.
    fn src_module_any_depth_py() -> Template {
        compile(vec![
            Segment::Literal("src/".to_string()),
            Segment::Variable("module".to_string(), None),
            Segment::Literal("/".to_string()),
            Segment::AnyDepth,
            Segment::Literal("/".to_string()),
            Segment::Wildcard,
            Segment::Literal(".py".to_string()),
        ])
    }

    #[test]
    fn matches_directory_prefix_partway_into_a_multi_component_any_depth_prefix() {
        let template = src_module_any_depth_py();
        assert!(template.matches_directory_prefix("src"));
        assert!(!template.matches_directory_prefix("lib"));
    }

    #[test]
    fn rebase_beneath_partway_into_a_multi_component_any_depth_prefix() {
        let template = src_module_any_depth_py();
        let rebased = template.rebase_beneath("src").unwrap();
        assert_eq!(
            rebased.matched_bindings("parser/deep/main.py"),
            Some(vec![("module".to_string(), "parser".to_string(), None)])
        );
        assert!(template.rebase_beneath("lib").is_none());
    }

    /// `{{ name|snake }}.txt`.
    fn snake_txt() -> Template {
        compile(vec![
            Segment::Variable("name".to_string(), Some(Format::Snake)),
            Segment::Literal(".txt".to_string()),
        ])
    }

    #[test]
    fn a_formatted_variable_captures_letters_of_any_script() {
        let template = snake_txt();
        for name in ["home_page", "crème_brûlée", "привет_мир", "日本語_ファイル"]
        {
            assert_eq!(
                template.matched_bindings(&format!("{name}.txt")),
                Some(vec![(
                    "name".to_string(),
                    name.to_string(),
                    Some(Format::Snake)
                )])
            );
        }
        for name in ["HomePage", "Crème_brûlée", "ПРИВЕТ_МИР", "crème→brûlée"] {
            assert_eq!(template.matched_bindings(&format!("{name}.txt")), None);
        }
    }

    #[test]
    fn the_any_script_regex_is_only_compiled_for_a_name_outside_ascii() {
        let template = snake_txt();
        let matcher = &template.components[0].regex;
        assert!(matcher.full_pattern.is_some());
        assert!(template.matched_bindings("home_page.txt").is_some());
        assert!(template.matched_bindings("HomePage.txt").is_none());
        assert!(matcher.full.get().is_none());
        assert!(template.matched_bindings("crème_brûlée.txt").is_some());
        assert!(matcher.full.get().is_some());
        // Still right for an ASCII name afterwards.
        assert!(template.matched_bindings("home_page.txt").is_some());
        assert!(template.matched_bindings("HomePage.txt").is_none());
    }

    #[test]
    fn a_component_without_a_format_has_just_the_one_regex() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("_".to_string()),
            Segment::Wildcard,
            Segment::Literal(".txt".to_string()),
        ]);
        let matcher = &template.components[0].regex;
        assert!(matcher.full_pattern.is_none());
        assert_eq!(
            template.matched_bindings("größe_日本語.txt"),
            Some(vec![("name".to_string(), "größe".to_string(), None)])
        );
        assert!(matcher.full.get().is_none());
    }

    #[test]
    fn a_literal_and_a_wildcard_match_characters_not_bytes() {
        let template = compile(vec![
            Segment::Literal("é".to_string()),
            Segment::Wildcard,
            Segment::Literal("語.txt".to_string()),
        ]);
        assert!(template.matched_bindings("é語.txt").is_some());
        assert!(template.matched_bindings("é🙂ü語.txt").is_some());
        assert!(template.matched_bindings("e語.txt").is_none());
        assert!(template.matched_bindings("é語").is_none());
    }

    #[test]
    fn inherited_count_is_zero_for_a_line_indented_under_nothing() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("/x.txt".to_string()),
        ]);
        assert_eq!(template.inherited_count(), 0);
    }

    #[test]
    fn inherited_count_drops_by_each_component_rebasing_cuts_off() {
        let template = compile(vec![
            Segment::Variable("name".to_string(), None),
            Segment::Literal("/a/b/x.txt".to_string()),
        ])
        .with_inherited(2);
        assert_eq!(template.inherited_count(), 2);
        let inside = template.rebase_beneath("Home").unwrap();
        assert_eq!(inside.inherited_count(), 1);
        // Past the inherited folders, the rest is all the line's own.
        assert_eq!(inside.rebase_beneath("a").unwrap().inherited_count(), 0);
        assert_eq!(
            template
                .rebase_beneath("Home/a/b")
                .unwrap()
                .inherited_count(),
            0
        );
    }

    #[test]
    fn matches_directory_prefix_keeps_a_leading_wildcard_out_of_dot_folders() {
        let template = compile(vec![
            Segment::Wildcard,
            Segment::Literal("/main.py".to_string()),
        ]);
        assert!(template.matches_directory_prefix("src"));
        assert!(!template.matches_directory_prefix(".git"));
    }
}
