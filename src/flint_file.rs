// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of `.flint` file contents into a list of allowed entries.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::entry_kind::EntryKind;
use crate::format::Format;
use crate::origin::Origin;
use crate::template::{GroupMembership, Segment, Template, TemplateError};

/// One parsed `.flint` line: either a literal name matched verbatim, or a
/// line containing one or more `{{ variable }}` placeholders.
#[derive(Debug, Clone)]
pub enum Entry {
    /// A literal name, matched verbatim against a directory entry.
    Literal {
        /// The name to match, with any trailing `/` already stripped.
        name: String,
        /// Whether this entry requires a directory or a file.
        kind: EntryKind,
        /// Whether a leading `!` requires at least one match on disk, not
        /// just permitting one.
        required: bool,
        /// Where the rule was written.
        origin: Origin,
    },
    /// A line containing one or more `{{ variable }}` placeholders.
    Template(Template),
}

impl Entry {
    /// Where this entry's rule was written.
    #[must_use]
    pub fn origin(&self) -> &Origin {
        match self {
            Entry::Literal { origin, .. } => origin,
            Entry::Template(template) => template.origin(),
        }
    }
}

/// A `.flint` line with malformed `{{ }}` template syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number within the `.flint` file.
    pub line_number: usize,
    /// The raw source line, for display.
    pub line: String,
    /// What specifically was wrong with the line.
    pub kind: ParseErrorKind,
}

/// The specific way a `.flint` line's `{{ }}` syntax was malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// `{` or `}` used outside a well-formed `{{ variable }}` placeholder
    /// and not escaped as `\{` / `\}` — covers a lone `{`, a lone `}`, and
    /// an unterminated `{{ ...` with no closing `}}` before end of line.
    UnbalancedBrace,
    /// `{{}}` / `{{ }}` / `{{   }}` — no identifier between the braces.
    EmptyVariableName,
    /// The identifier (after trimming whitespace) doesn't match
    /// `[A-Za-z_][A-Za-z0-9_]*`.
    InvalidVariableName(String),
    /// The text after `|` in a `{{ name|filter }}` placeholder isn't one
    /// of the ten recognized format names.
    UnknownFormat(String),
    /// The same variable name is used with a `|format` filter in some
    /// occurrences and without one in others, within one line's path
    /// (including occurrences inherited from an indented-under header).
    /// Separate lines are never compared against each other.
    InconsistentVariableFormat(String),
    /// Two `{{ variable }}` placeholders with no literal text between
    /// them (e.g. `{{a}}{{b}}.txt`) — the value boundary is ambiguous.
    AdjacentVariables,
    /// A line consisting only of a `!` and/or trailing `/` marker, with no
    /// name left to apply it to (e.g. a bare `!`, a bare `/`, or `!/`).
    EmptyRule,
    /// `**` combined with other text, `*`, or `{{ variable }}` in the same
    /// path component (e.g. `pre**post`, `**post`, `{{ name }}**`).
    AnyDepthNotAlone,
    /// More than one `**` in the same `.flint` line.
    MultipleAnyDepth,
    /// A line is indented relative to the previous non-blank,
    /// non-comment-only line, but that previous line isn't directory-kind
    /// (no trailing `/`) — or there is no previous line at all (the very
    /// first content line in the file is indented).
    IndentWithoutDirectoryParent,
    /// `<` or `>` outside a group header and not escaped as `\<` / `\>`.
    UnescapedAngleBracket,
    /// A line starting with `<` that isn't a well-formed group header:
    /// something other than comma-separated variable names between the
    /// brackets (a `{{ }}` placeholder, a `|format` filter, nothing at
    /// all), no closing `>`, or anything but a comment after it (such as
    /// a trailing `/`).
    InvalidGroupHeader,
    /// A group header declaring a variable that an enclosing line (a
    /// directory, or another group header) already binds, or that it
    /// declares twice.
    GroupRebindsVariable(String),
    /// A group header declaring a variable that no line indented under it
    /// uses — including a header with nothing indented under it at all.
    UnusedGroupVariable(String),
    /// A line's leading whitespace doesn't cleanly extend or match an open
    /// indentation level: a dedent that doesn't land on any enclosing
    /// line's indentation, or leading whitespace of the same length as an
    /// enclosing level but different content (including tabs and spaces
    /// mixed incompatibly within one ancestor chain).
    InconsistentIndentation,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}: {} (in \"{}\")",
            self.line_number, self.kind, self.line
        )
    }
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::UnbalancedBrace => write!(
                f,
                "'{{' or '}}' used outside a `{{{{ variable }}}}` placeholder and not escaped as \\{{ or \\}}"
            ),
            ParseErrorKind::EmptyVariableName => {
                write!(f, "empty `{{{{ }}}}` placeholder: a variable needs a name")
            }
            ParseErrorKind::InvalidVariableName(name) => write!(
                f,
                "invalid variable name \"{name}\": names must start with a letter or underscore and contain only letters, digits, and underscores"
            ),
            ParseErrorKind::UnknownFormat(name) => {
                let expected = Format::filter_names().join(", ");
                write!(f, "unknown format \"{name}\": expected one of {expected}")
            }
            ParseErrorKind::InconsistentVariableFormat(name) => write!(
                f,
                "\"{name}\" is used with a `|format` filter in one place and without one in another that shares its value (the same line, a line it's indented under, or a rule under the same group header): apply a filter (or none) consistently"
            ),
            ParseErrorKind::AdjacentVariables => write!(
                f,
                "two `{{{{ variable }}}}` placeholders with no literal text between them: the boundary between their values would be ambiguous"
            ),
            ParseErrorKind::EmptyRule => write!(
                f,
                "a `!` and/or trailing `/` marker with nothing to apply it to: add a name, or remove the marker"
            ),
            ParseErrorKind::AnyDepthNotAlone => write!(
                f,
                "`**` must be the entire path component — it can't be combined with other text, `*`, or `{{{{ variable }}}}` in the same segment"
            ),
            ParseErrorKind::MultipleAnyDepth => {
                write!(f, "only one `**` is supported per `.flint` line")
            }
            ParseErrorKind::IndentWithoutDirectoryParent => write!(
                f,
                "this line is indented, but the previous line isn't a directory (add a trailing `/` to it), or there is no previous line to nest under"
            ),
            ParseErrorKind::UnescapedAngleBracket => write!(
                f,
                "'<' or '>' used outside a group header like `<name>`: escape it as \\< or \\>"
            ),
            ParseErrorKind::InvalidGroupHeader => write!(
                f,
                "a group header holds only variable names between `<` and `>`, like `<name>` or `<name, kind>`, with nothing after it but a comment"
            ),
            ParseErrorKind::GroupRebindsVariable(name) => write!(
                f,
                "\"{name}\" is already bound here: a group header must declare new variables, each once"
            ),
            ParseErrorKind::UnusedGroupVariable(name) => write!(
                f,
                "\"{name}\" is declared by this group header, but no rule indented under it uses `{{{{ {name} }}}}`"
            ),
            ParseErrorKind::InconsistentIndentation => write!(
                f,
                "this line's indentation doesn't match any enclosing line: dedent to exactly match an enclosing line's indentation, and don't mix tabs and spaces within the same nested block"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<TemplateError> for ParseErrorKind {
    fn from(error: TemplateError) -> Self {
        match error {
            TemplateError::AnyDepthNotAlone => ParseErrorKind::AnyDepthNotAlone,
            TemplateError::MultipleAnyDepth => ParseErrorKind::MultipleAnyDepth,
        }
    }
}

/// Parses the contents of a `.flint` file into the list of allowed
/// entries.
///
/// Blank/whitespace-only lines are skipped. `#` starts a comment that runs
/// to the end of the line, unless it's escaped as `\#`. `{` and `}` are
/// escaped as `\{` / `\}` when they should be literal characters rather
/// than part of a `{{ variable }}` placeholder. `*` is escaped as `\*` when
/// it should be a literal asterisk rather than a wildcard. A leading `!` is
/// escaped as `\!` when it should be a literal exclamation mark rather than
/// a required-entry marker. Leading/trailing whitespace around each
/// resulting entry is trimmed.
///
/// A line may be indented under a preceding directory-kind line (one
/// ending in `/`) to avoid repeating its path on every line — indenting
/// `main.py` under `src/` is exactly equivalent to writing `src/main.py`
/// on one line. Indentation can use spaces or tabs; the only requirement is
/// that it nest consistently — each line's leading whitespace must extend
/// (as a literal string prefix) the leading whitespace of the line it
/// nests under, and a dedent must land exactly on some enclosing line's
/// indentation. Blank and comment-only lines don't affect nesting at any
/// indentation. A line still needs its own trailing `/` to take indented
/// children — indentation never implies one.
///
/// # Errors
/// Returns `Err` if any line contains malformed `{{ }}` template syntax,
/// is a `!`/`/` marker with no name attached, misuses `**` (combined with
/// other content in one path component, or used more than once), is
/// indented under a line that isn't directory-kind (or has no preceding
/// line to nest under), or has leading whitespace that doesn't cleanly
/// nest or dedent — see [`ParseErrorKind`].
///
/// # Panics
/// Never. The internal indentation stack always contains at least its
/// root frame, which is pushed once up front and never popped.
pub fn parse(contents: &str) -> Result<Vec<Entry>, ParseError> {
    parse_in(contents, "")
}

/// [`parse`], for the `.flint` file in `scope`'s directory: every entry's
/// [`Origin`] names `scope` (see [`Origin::scope`]), where [`parse`] names
/// the root.
///
/// # Errors
/// Exactly as [`parse`].
///
/// # Panics
/// Never, as [`parse`].
pub fn parse_in(contents: &str, scope: &str) -> Result<Vec<Entry>, ParseError> {
    let mut entries = Vec::new();
    let mut indent_stack = vec![IndentFrame {
        indent: String::new(),
        child_prefix: Vec::new(),
        groups: Vec::new(),
    }];
    let mut pending_parent: Option<PendingParent> = None;
    let mut groups: Vec<OpenGroup> = Vec::new();

    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let error = |kind| ParseError {
            line_number,
            line: line.to_string(),
            kind,
        };
        let origin = Origin {
            scope: scope.to_string(),
            line: line_number,
        };

        let (required, content) = strip_required_marker(line);

        if let Some(header) = parse_group_header(content) {
            let variables = header.map_err(error)?;
            resolve_indent(
                &mut indent_stack,
                leading_whitespace(line),
                pending_parent.as_ref(),
                line_number,
                line,
            )?;
            let top = indent_stack
                .last()
                .expect("indent stack always has at least the root frame");
            groups
                .push(open_group(top, &groups, variables, required, origin, line).map_err(error)?);
            pending_parent = Some(PendingParent {
                segments: Vec::new(),
                parent: ParentKind::Group(groups.len() - 1),
            });
            continue;
        }

        let segments = tokenize(content).map_err(error)?;
        let segments = trim_edges(segments);
        let (segments, kind) = split_trailing_slash(segments);

        if segments.is_empty() {
            if required || kind == EntryKind::Directory {
                return Err(error(ParseErrorKind::EmptyRule));
            }
            continue; // blank/comment-only line: doesn't affect indentation nesting
        }

        resolve_indent(
            &mut indent_stack,
            leading_whitespace(line),
            pending_parent.as_ref(),
            line_number,
            line,
        )?;

        let top = indent_stack
            .last()
            .expect("indent stack always has at least the root frame");
        let source_line = format!("{}{}", segments_to_text(&top.child_prefix), content.trim());
        let mut effective_segments = top.child_prefix.clone();
        effective_segments.extend(segments.clone());

        check_variable_format_consistency(&effective_segments).map_err(error)?;
        #[expect(
            clippy::indexing_slicing,
            reason = "a group id is its index in `groups`, which only grows"
        )]
        for &id in &top.groups {
            groups[id].record_use(&effective_segments).map_err(error)?;
        }
        #[expect(
            clippy::indexing_slicing,
            reason = "a group id is its index in `groups`, which only grows"
        )]
        let memberships = top
            .groups
            .iter()
            .map(|&id| groups[id].membership.clone())
            .collect();

        entries.push(make_entry(
            effective_segments,
            path_separators(&top.child_prefix),
            kind,
            source_line,
            required,
            memberships,
            origin,
            line,
        )?);

        pending_parent = Some(PendingParent {
            segments,
            parent: if kind == EntryKind::Directory {
                ParentKind::Directory
            } else {
                ParentKind::File
            },
        });
    }

    check_group_variables_used(&groups)?;
    Ok(entries)
}

/// Checks that every one of `groups`' variables is used by some line
/// under its header.
///
/// # Errors
/// [`ParseErrorKind::UnusedGroupVariable`] at the first header with an
/// unused variable.
fn check_group_variables_used(groups: &[OpenGroup]) -> Result<(), ParseError> {
    for group in groups {
        if let Some(variable) = group.unused_variable() {
            return Err(ParseError {
                line_number: group.line_number,
                line: group.line.clone(),
                kind: ParseErrorKind::UnusedGroupVariable(variable.to_string()),
            });
        }
    }
    Ok(())
}

/// A `<name>` / `<name, kind>` group header seen while parsing, and what
/// the lines indented under it have done with its variables so far.
struct OpenGroup {
    /// What every template indented under it carries.
    membership: GroupMembership,
    /// The header's 1-based line number, for an error.
    line_number: usize,
    /// The header's raw line, for an error.
    line: String,
    /// Which of its variables some line under it uses.
    used: HashSet<String>,
    /// For each of its variables used so far, whether with a `|format`
    /// filter — every rule under one header shares the variable, so the
    /// filter must be used everywhere under it, or nowhere.
    has_format: HashMap<String, bool>,
}

impl OpenGroup {
    /// Records the header's variables that `segments` (one line's whole
    /// path, indented under the header) uses.
    ///
    /// # Errors
    /// [`ParseErrorKind::InconsistentVariableFormat`] if one of them uses
    /// a `|format` filter here but not under another line of this header,
    /// or the other way round.
    fn record_use(&mut self, segments: &[Segment]) -> Result<(), ParseErrorKind> {
        for segment in segments {
            let Segment::Variable(name, format) = segment else {
                continue;
            };
            if !self.membership.variables.contains(name) {
                continue;
            }
            self.used.insert(name.clone());
            let has_format = format.is_some();
            if *self.has_format.entry(name.clone()).or_insert(has_format) != has_format {
                return Err(ParseErrorKind::InconsistentVariableFormat(name.clone()));
            }
        }
        Ok(())
    }

    /// The first of the header's variables no line under it uses.
    fn unused_variable(&self) -> Option<&str> {
        self.membership
            .variables
            .iter()
            .find(|variable| !self.used.contains(*variable))
            .map(String::as_str)
    }
}

/// The newly opened header on `line`, declaring `variables` (and marked
/// `!` if `required`), at the depth of `frame` — whose path the header sits under, inside its
/// already-open headers (indices into `groups`).
///
/// # Errors
/// [`ParseErrorKind::GroupRebindsVariable`] if `frame`'s path or an
/// enclosing header already binds one of `variables`.
fn open_group(
    frame: &IndentFrame,
    groups: &[OpenGroup],
    variables: Vec<String>,
    required: bool,
    origin: Origin,
    line: &str,
) -> Result<OpenGroup, ParseErrorKind> {
    let prefix = &frame.child_prefix;
    let enclosing = &frame.groups;
    #[expect(
        clippy::indexing_slicing,
        reason = "a group id is its index in `groups`, which only grows"
    )]
    let already_bound = |variable: &String| {
        prefix
            .iter()
            .any(|segment| matches!(segment, Segment::Variable(name, _) if name == variable))
            || enclosing
                .iter()
                .any(|&id| groups[id].membership.variables.contains(variable))
    };
    if let Some(variable) = variables.iter().find(|variable| already_bound(variable)) {
        return Err(ParseErrorKind::GroupRebindsVariable(variable.clone()));
    }
    let prefix_text = segments_to_text(prefix);
    // Components are counted by the `/`s in literal segments, the only
    // place a `/` can appear, so an escaped `\*\*` isn't mistaken for `**`.
    let mut depth = 0;
    let mut any_depth = None;
    for segment in prefix {
        match segment {
            Segment::Literal(text) => depth += text.matches('/').count(),
            Segment::AnyDepth => any_depth = Some(depth),
            Segment::Variable(..) | Segment::Wildcard => {}
        }
    }
    Ok(OpenGroup {
        line_number: origin.line,
        line: line.to_string(),
        used: HashSet::new(),
        has_format: HashMap::new(),
        membership: GroupMembership {
            id: groups.len(),
            depth,
            any_depth,
            source_line: format!("{prefix_text}<{}>", variables.join(", ")),
            variables,
            origin,
            required,
        },
    })
}

/// If `content` (a line with any `!` marker already stripped) is a group
/// header — it starts with `<` — the variable names it declares, in
/// order. `None` for any other line.
///
/// # Errors
/// The inner `Err` is [`ParseErrorKind::InvalidGroupHeader`] for anything
/// but comma-separated names between `<` and `>` followed by at most a
/// comment, [`ParseErrorKind::InvalidVariableName`] for a name that isn't
/// an identifier, or [`ParseErrorKind::GroupRebindsVariable`] for a name
/// declared twice.
fn parse_group_header(content: &str) -> Option<Result<Vec<String>, ParseErrorKind>> {
    let rest = content.trim_start().strip_prefix('<')?;
    let Some((inside, after)) = rest.split_once('>') else {
        return Some(Err(ParseErrorKind::InvalidGroupHeader));
    };
    let after = after.trim_start();
    if !(after.is_empty() || after.starts_with('#')) {
        return Some(Err(ParseErrorKind::InvalidGroupHeader));
    }
    let mut variables: Vec<String> = Vec::new();
    for name in inside.split(',').map(str::trim) {
        if name.is_empty() || name.contains(['{', '}', '<', '|', '*', '/']) {
            return Some(Err(ParseErrorKind::InvalidGroupHeader));
        }
        if !is_valid_identifier(name) {
            return Some(Err(ParseErrorKind::InvalidVariableName(name.to_string())));
        }
        if variables.iter().any(|variable| variable == name) {
            return Some(Err(ParseErrorKind::GroupRebindsVariable(name.to_string())));
        }
        variables.push(name.to_string());
    }
    Some(Ok(variables))
}

/// One open indentation level while resolving nesting in [`parse`]. The
/// root frame (`indent: ""`) is always present and never popped, so
/// top-level (unindented) lines get an empty `child_prefix`, leaving
/// behavior for non-indented files identical to before this existed.
struct IndentFrame {
    /// The leading whitespace every line at this depth must match exactly.
    indent: String,
    /// The segments to prepend to any line at this depth — the
    /// accumulated path of every enclosing directory-kind line, already
    /// including a trailing `/` separator when non-empty.
    child_prefix: Vec<Segment>,
    /// Every group header enclosing this depth, outermost first, as
    /// indices into [`parse_in`]'s list of headers.
    groups: Vec<usize>,
}

/// What kind of line a [`PendingParent`] is, for whether lines can nest
/// under it.
enum ParentKind {
    /// A directory-kind line (trailing `/`): children nest inside it.
    Directory,
    /// A group header: children stay in the same folder, sharing its
    /// variables (the index into [`parse_in`]'s list of headers).
    Group(usize),
    /// A file: nothing can nest under it.
    File,
}

/// The most recently processed content line, kept in case the *next* line
/// is indented deeper and needs to nest under it.
struct PendingParent {
    /// This line's own segments (marker and trailing `/` already
    /// stripped), to be prepended (plus a `/` separator) as the
    /// `child_prefix` of a new, deeper [`IndentFrame`] if the next line
    /// nests under it. Empty for a group header, which adds nothing to
    /// the path.
    segments: Vec<Segment>,
    /// Whether this line is a directory, a group header (both can have
    /// indented children), or a file (which can't).
    parent: ParentKind,
}

/// Checks that every variable in `segments` (one line's fully expanded
/// path, including any prefix inherited from indentation) uses a `|format`
/// filter at every occurrence or at none, erroring on the first variable
/// that mixes the two. Scoped to one path because that's the only place
/// occurrences are ever reconciled against each other (see
/// [`crate::checker`]): two separate lines reusing a name are independent,
/// so `src/{{ name|snake }}.rs` and `tests/{{ name }}.rs` can coexist. A
/// header and its indented child share one path, so a mismatch between
/// them (`{{ name|camel }}/` wrapping `{{ name }}Manager.py`) is still
/// caught, as is one within a single line (`{{ name }}_{{ name|camel }}.txt`).
fn check_variable_format_consistency(segments: &[Segment]) -> Result<(), ParseErrorKind> {
    let mut variable_has_format: HashMap<&str, bool> = HashMap::new();
    for segment in segments {
        if let Segment::Variable(name, format) = segment {
            let has_format = format.is_some();
            if *variable_has_format.entry(name).or_insert(has_format) != has_format {
                return Err(ParseErrorKind::InconsistentVariableFormat(name.clone()));
            }
        }
    }
    Ok(())
}

/// The leading run of spaces and tabs at the start of `line`.
fn leading_whitespace(line: &str) -> &str {
    let trimmed = line.trim_start_matches([' ', '\t']);
    &line[..line.len() - trimmed.len()]
}

/// Re-renders `segments` back into `.flint` syntax, used to build the
/// fully expanded path for a nested line's `Template::source_line` (so
/// error messages show the whole path, not just the fragment typed on
/// that one line).
fn segments_to_text(segments: &[Segment]) -> String {
    let mut text = String::new();
    for segment in segments {
        match segment {
            Segment::Literal(literal) => text.push_str(literal),
            Segment::Variable(name, format) => {
                text.push_str("{{ ");
                text.push_str(name);
                if let Some(format) = format {
                    text.push('|');
                    text.push_str(format.filter_name());
                }
                text.push_str(" }}");
            }
            Segment::Wildcard => text.push('*'),
            Segment::AnyDepth => text.push_str("**"),
        }
    }
    text
}

/// Resolves `indent` (this line's leading whitespace) against the current
/// `stack` of open indentation levels, mutating it in place: pushing a new
/// frame if this line nests deeper than the previous one, popping back to
/// an enclosing frame on dedent, or leaving the stack alone for a sibling
/// at the same depth.
fn resolve_indent(
    stack: &mut Vec<IndentFrame>,
    indent: &str,
    pending_parent: Option<&PendingParent>,
    line_number: usize,
    line: &str,
) -> Result<(), ParseError> {
    let top = stack
        .last()
        .expect("indent stack always has at least the root frame");

    if indent.len() > top.indent.len() {
        if !indent.starts_with(&top.indent) {
            return Err(ParseError {
                line_number,
                line: line.to_string(),
                kind: ParseErrorKind::InconsistentIndentation,
            });
        }
        let mut child_prefix = top.child_prefix.clone();
        let mut groups = top.groups.clone();
        match pending_parent.map(|parent| (parent, &parent.parent)) {
            Some((parent, ParentKind::Directory)) => {
                child_prefix.extend(parent.segments.clone());
                child_prefix.push(Segment::Literal("/".to_string()));
            }
            Some((_, ParentKind::Group(id))) => groups.push(*id),
            Some((_, ParentKind::File)) | None => {
                return Err(ParseError {
                    line_number,
                    line: line.to_string(),
                    kind: ParseErrorKind::IndentWithoutDirectoryParent,
                });
            }
        }
        stack.push(IndentFrame {
            indent: indent.to_string(),
            child_prefix,
            groups,
        });
    } else {
        while stack.len() > 1
            && indent.len() < stack.last().expect("just checked len() > 1").indent.len()
        {
            stack.pop();
        }
        if stack
            .last()
            .expect("indent stack always has at least the root frame")
            .indent
            != indent
        {
            return Err(ParseError {
                line_number,
                line: line.to_string(),
                kind: ParseErrorKind::InconsistentIndentation,
            });
        }
    }

    Ok(())
}

/// Converts already fully-expanded `segments` (this line's own content,
/// prefixed with any enclosing indentation) into an [`Entry`] — a
/// [`Template`] (indented under `groups`) if any segment is a
/// `{{ variable }}`, `*`, or `**`, otherwise a literal name, which a
/// group header has no bearing on. The first `inherited` path components
/// are that enclosing indentation's. `line` is the raw source line
/// `origin` points at, for a parse error.
// Each argument is a distinct part of one parsed line.
#[allow(clippy::too_many_arguments)]
fn make_entry(
    segments: Vec<Segment>,
    inherited: usize,
    kind: EntryKind,
    source_line: String,
    required: bool,
    groups: Vec<GroupMembership>,
    origin: Origin,
    line: &str,
) -> Result<Entry, ParseError> {
    let line_number = origin.line;
    if segments.iter().any(|segment| {
        matches!(
            segment,
            Segment::Variable(..) | Segment::Wildcard | Segment::AnyDepth
        )
    }) {
        let template =
            Template::compile(segments, kind, source_line, origin, required).map_err(|error| {
                ParseError {
                    line_number,
                    line: line.to_string(),
                    kind: error.into(),
                }
            })?;
        Ok(Entry::Template(
            template.with_groups(groups).with_inherited(inherited),
        ))
    } else {
        let name = segments
            .into_iter()
            .map(|segment| match segment {
                Segment::Literal(text) => text,
                Segment::Variable(..) | Segment::Wildcard | Segment::AnyDepth => {
                    unreachable!(
                        "checked above: no Variable, Wildcard, or AnyDepth segments present"
                    )
                }
            })
            .collect();
        Ok(Entry::Literal {
            name,
            kind,
            required,
            origin,
        })
    }
}

/// How many path components `prefix` — an indentation level's inherited
/// path, each component followed by its `/` — holds. A `/` only ever
/// appears in a literal segment, as a separator.
fn path_separators(prefix: &[Segment]) -> usize {
    prefix
        .iter()
        .map(|segment| match segment {
            Segment::Literal(text) => text.matches('/').count(),
            Segment::Variable(..) | Segment::Wildcard | Segment::AnyDepth => 0,
        })
        .sum()
}

/// Detects and strips a leading `!` "required" marker, honoring `\!` as an
/// escape for a literal leading exclamation mark. `!` only means anything
/// as the first non-whitespace character of a line — anywhere else it's
/// already just an ordinary character to [`tokenize`].
fn strip_required_marker(line: &str) -> (bool, &str) {
    let trimmed = line.trim_start();
    if trimmed.starts_with(r"\!") {
        (false, &trimmed[1..]) // drop only the backslash, keep the `!`
    } else if let Some(rest) = trimmed.strip_prefix('!') {
        (true, rest)
    } else {
        (false, line)
    }
}

/// Splits a single `.flint` line into literal, variable, wildcard, and
/// any-depth (`**`) segments, stripping comments and resolving
/// `\#`/`\{`/`\}`/`\*` escapes along the way. Two or more consecutive `*`
/// (with nothing between them) become one `Segment::AnyDepth`; escaping
/// each `*` individually (`\*\*`) still produces a literal `**` instead,
/// since each `\*` is consumed by its own escape arm before this one is
/// ever reached.
pub(crate) fn tokenize(line: &str) -> Result<Vec<Segment>, ParseErrorKind> {
    let mut segments = Vec::new();
    let mut literal_buf = String::new();
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'#') => {
                literal_buf.push('#');
                chars.next();
            }
            '\\' if chars.peek() == Some(&'{') => {
                literal_buf.push('{');
                chars.next();
            }
            '\\' if chars.peek() == Some(&'}') => {
                literal_buf.push('}');
                chars.next();
            }
            '\\' if chars.peek() == Some(&'*') => {
                literal_buf.push('*');
                chars.next();
            }
            '\\' if matches!(chars.peek(), Some('<' | '>')) => {
                literal_buf.extend(chars.next());
            }
            '<' | '>' => return Err(ParseErrorKind::UnescapedAngleBracket),
            '#' => break,
            '*' => {
                if !literal_buf.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal_buf)));
                }
                if chars.peek() == Some(&'*') {
                    while chars.peek() == Some(&'*') {
                        chars.next(); // collapse a run of 3+ '*' into one AnyDepth, same as a bare run of `*`s collapses to one Wildcard
                    }
                    segments.push(Segment::AnyDepth);
                } else {
                    segments.push(Segment::Wildcard);
                }
            }
            '{' if chars.peek() == Some(&'{') => {
                chars.next(); // consume the second '{'

                if !literal_buf.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal_buf)));
                } else if matches!(segments.last(), Some(Segment::Variable(..))) {
                    return Err(ParseErrorKind::AdjacentVariables);
                }

                let mut raw = String::new();
                loop {
                    match chars.next() {
                        Some('}') if chars.peek() == Some(&'}') => {
                            chars.next();
                            break;
                        }
                        Some(ch) => raw.push(ch),
                        None => return Err(ParseErrorKind::UnbalancedBrace),
                    }
                }

                let (name_part, format_part) = match raw.split_once('|') {
                    Some((name, format)) => (name.trim(), Some(format.trim())),
                    None => (raw.trim(), None),
                };
                if name_part.is_empty() {
                    return Err(ParseErrorKind::EmptyVariableName);
                }
                if !is_valid_identifier(name_part) {
                    return Err(ParseErrorKind::InvalidVariableName(name_part.to_string()));
                }
                let format = format_part
                    .map(|filter_name| {
                        Format::parse(filter_name)
                            .ok_or_else(|| ParseErrorKind::UnknownFormat(filter_name.to_string()))
                    })
                    .transpose()?;
                segments.push(Segment::Variable(name_part.to_string(), format));
            }
            '{' | '}' => return Err(ParseErrorKind::UnbalancedBrace),
            _ => literal_buf.push(c),
        }
    }

    if !literal_buf.is_empty() {
        segments.push(Segment::Literal(literal_buf));
    }

    Ok(segments)
}

/// Whether `name` can name a `{{ variable }}`: an ASCII letter or `_`,
/// then any number of ASCII letters, digits and `_`.
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Trims leading whitespace off a leading literal segment and trailing
/// whitespace off a trailing literal segment (mirroring the old
/// whole-line `.trim()`), then drops any literal segments left empty by
/// that trim.
fn trim_edges(mut segments: Vec<Segment>) -> Vec<Segment> {
    if let Some(Segment::Literal(text)) = segments.first_mut() {
        *text = text.trim_start().to_string();
    }
    if let Some(Segment::Literal(text)) = segments.last_mut() {
        *text = text.trim_end().to_string();
    }
    segments.retain(|segment| !matches!(segment, Segment::Literal(text) if text.is_empty()));
    segments
}

/// Strips exactly one trailing `/` off the last literal segment, if
/// present, and reports whether that makes this entry directory-only.
///
/// A segment left empty by the strip (e.g. a line that was just `/`) is
/// dropped, same as `trim_edges` drops literals emptied by whitespace
/// trimming — letting the caller's existing "no segments left" check treat
/// it like a blank line, with no need for a dedicated error case.
fn split_trailing_slash(mut segments: Vec<Segment>) -> (Vec<Segment>, EntryKind) {
    let Some(Segment::Literal(text)) = segments.last_mut() else {
        return (segments, EntryKind::File);
    };
    let Some(stripped) = text.strip_suffix('/') else {
        return (segments, EntryKind::File);
    };
    let stripped = stripped.trim_end().to_string();

    if stripped.is_empty() {
        segments.pop();
    } else {
        *text = stripped;
    }
    (segments, EntryKind::Directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal_names(entries: Vec<Entry>) -> Vec<String> {
        entries
            .into_iter()
            .map(|entry| match entry {
                Entry::Literal { name, .. } => name,
                Entry::Template(_) => panic!("expected a literal entry"),
            })
            .collect()
    }

    fn literal_entries(entries: Vec<Entry>) -> Vec<(String, EntryKind)> {
        entries
            .into_iter()
            .map(|entry| match entry {
                Entry::Literal { name, kind, .. } => (name, kind),
                Entry::Template(_) => panic!("expected a literal entry"),
            })
            .collect()
    }

    #[test]
    fn empty_contents_yield_no_entries() {
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn whitespace_only_lines_are_ignored() {
        assert!(parse("   \n\t\n  \t \n").unwrap().is_empty());
    }

    #[test]
    fn comment_only_lines_are_ignored() {
        assert!(
            parse("# just a comment\n   # another one")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn plain_filenames_are_parsed() {
        assert_eq!(
            literal_names(parse("Cargo.toml\nsrc\n").unwrap()),
            vec!["Cargo.toml", "src"]
        );
    }

    #[test]
    fn trailing_comments_are_stripped() {
        assert_eq!(
            literal_names(parse("Cargo.toml # the manifest").unwrap()),
            vec!["Cargo.toml"]
        );
    }

    #[test]
    fn trailing_whitespace_is_trimmed() {
        assert_eq!(
            literal_names(parse("Cargo.toml   \n").unwrap()),
            vec!["Cargo.toml"]
        );
    }

    #[test]
    fn leading_whitespace_without_a_directory_parent_is_a_parse_error() {
        assert_eq!(
            parse("   Cargo.toml").unwrap_err().kind,
            ParseErrorKind::IndentWithoutDirectoryParent
        );
    }

    #[test]
    fn escaped_hash_is_kept_literally() {
        assert_eq!(
            literal_names(parse(r"weird\#file.txt").unwrap()),
            vec!["weird#file.txt"]
        );
    }

    #[test]
    fn escaped_hash_followed_by_real_comment() {
        assert_eq!(
            literal_names(parse(r"weird\#file.txt # now a real comment").unwrap()),
            vec!["weird#file.txt"]
        );
    }

    #[test]
    fn multiple_entries_across_lines() {
        assert_eq!(
            literal_names(parse("a.txt\n# comment\nb.txt\n\nc.txt # trailing\n").unwrap()),
            vec!["a.txt", "b.txt", "c.txt"]
        );
    }

    #[test]
    fn escaped_braces_produce_literal_text() {
        assert_eq!(
            literal_names(parse(r"weird\{file\}.txt").unwrap()),
            vec!["weird{file}.txt"]
        );
    }

    #[test]
    fn escaped_asterisk_is_kept_literally() {
        assert_eq!(
            literal_names(parse(r"weird\*file.txt").unwrap()),
            vec!["weird*file.txt"]
        );
    }

    #[test]
    fn wildcard_line_parses_as_a_template_entry() {
        let entries = parse("*.py").unwrap();
        assert!(matches!(entries[0], Entry::Template(_)));
    }

    #[test]
    fn any_depth_wildcard_parses_as_a_template_entry() {
        let entries = parse("src/**/*.py").unwrap();
        assert!(matches!(entries[0], Entry::Template(_)));
    }

    #[test]
    fn escaped_double_asterisk_is_kept_literally() {
        assert_eq!(
            literal_names(parse(r"weird\*\*file.txt").unwrap()),
            vec!["weird**file.txt"]
        );
    }

    #[test]
    fn any_depth_combined_with_other_text_is_a_parse_error() {
        assert_eq!(
            parse("pre**post").unwrap_err().kind,
            ParseErrorKind::AnyDepthNotAlone
        );
    }

    #[test]
    fn multiple_any_depth_in_one_line_is_a_parse_error() {
        assert_eq!(
            parse("a/**/b/**/c").unwrap_err().kind,
            ParseErrorKind::MultipleAnyDepth
        );
    }

    #[test]
    fn template_errors_convert_to_the_matching_parse_error_kind() {
        assert_eq!(
            ParseErrorKind::from(TemplateError::AnyDepthNotAlone),
            ParseErrorKind::AnyDepthNotAlone
        );
        assert_eq!(
            ParseErrorKind::from(TemplateError::MultipleAnyDepth),
            ParseErrorKind::MultipleAnyDepth
        );
    }

    #[test]
    fn wildcard_and_variable_can_combine_on_one_line() {
        let entries = parse("{{ name }}Manager*.py").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            template.matched_bindings("HomeManagerExtra.py"),
            Some(vec![("name".to_string(), "Home".to_string(), None)])
        );
    }

    #[test]
    fn single_variable_template_line_matches() {
        let entries = parse("{{ name }}Manager.py").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            template.matched_bindings("HomeManager.py"),
            Some(vec![("name".to_string(), "Home".to_string(), None)])
        );
    }

    #[test]
    fn multiple_variables_in_one_line() {
        let entries = parse("{{ name }}Manager.{{ extension }}").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            template.matched_bindings("HomeManager.py"),
            Some(vec![
                ("name".to_string(), "Home".to_string(), None),
                ("extension".to_string(), "py".to_string(), None),
            ])
        );
    }

    #[test]
    fn whitespace_inside_braces_is_flexible() {
        for line in ["{{name}}.txt", "{{ name }}.txt", "{{  name  }}.txt"] {
            let entries = parse(line).unwrap();
            let Entry::Template(template) = &entries[0] else {
                panic!("expected a template entry for {line:?}");
            };
            assert_eq!(
                template.matched_bindings("Home.txt"),
                Some(vec![("name".to_string(), "Home".to_string(), None)]),
                "line {line:?} did not parse the same as the others"
            );
        }
    }

    #[test]
    fn every_format_filter_name_tokenizes_and_matches_its_own_shape() {
        for (filter, text) in [
            ("camel", "homePage"),
            ("pascal", "HomePage"),
            ("snake", "home_page"),
            ("screamingSnake", "HOME_PAGE"),
            ("kebab", "home-page"),
            ("screamingKebab", "HOME-PAGE"),
            ("title", "Home Page"),
            ("train", "Home-Page"),
            ("dot", "home.page"),
            ("screamingDot", "HOME.PAGE"),
        ] {
            let line = format!("{{{{ name|{filter} }}}}.txt");
            let entries = parse(&line).unwrap();
            let Entry::Template(template) = &entries[0] else {
                panic!("expected a template entry for {line:?}");
            };
            let format = Format::parse(filter);
            assert_eq!(
                template.matched_bindings(&format!("{text}.txt")),
                Some(vec![("name".to_string(), text.to_string(), format)]),
                "line {line:?} did not parse and match as expected"
            );
        }
    }

    #[test]
    fn whitespace_around_pipe_is_flexible() {
        for line in [
            "{{name|camel}}.txt",
            "{{ name|camel }}.txt",
            "{{ name | camel }}.txt",
        ] {
            let entries = parse(line).unwrap();
            let Entry::Template(template) = &entries[0] else {
                panic!("expected a template entry for {line:?}");
            };
            assert_eq!(
                template.matched_bindings("home.txt"),
                Some(vec![(
                    "name".to_string(),
                    "home".to_string(),
                    Some(Format::Camel)
                )]),
                "line {line:?} did not parse the same as the others"
            );
        }
    }

    #[test]
    fn unknown_format_name_is_a_parse_error() {
        assert_eq!(
            parse("{{ name|kebob }}.txt").unwrap_err().kind,
            ParseErrorKind::UnknownFormat("kebob".to_string())
        );
    }

    #[test]
    fn empty_format_name_is_an_unknown_format_error() {
        assert_eq!(
            parse("{{ name| }}.txt").unwrap_err().kind,
            ParseErrorKind::UnknownFormat(String::new())
        );
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_within_one_line_is_a_parse_error() {
        assert_eq!(
            parse("{{ name }}_{{ name|camel }}.txt").unwrap_err().kind,
            ParseErrorKind::InconsistentVariableFormat("name".to_string())
        );
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_across_two_lines_is_not_a_parse_error() {
        assert!(parse("{{ name|camel }}.txt\n{{ name }}.md").is_ok());
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_under_sibling_directories_is_not_a_parse_error()
    {
        assert!(parse("src/\n  {{ name|snake }}.rs\ntests/\n  {{ name }}.rs\n").is_ok());
    }

    #[test]
    fn different_formats_for_the_same_variable_across_lines_is_not_a_parse_error() {
        assert!(parse("{{ name|camel }}/\n  {{ name|pascal }}Manager.py\n").is_ok());
    }

    #[test]
    fn comment_after_template_is_stripped() {
        let entries = parse("{{ name }}Manager.py # a comment").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            template.matched_bindings("HomeManager.py"),
            Some(vec![("name".to_string(), "Home".to_string(), None)])
        );
    }

    #[test]
    fn bare_unescaped_open_brace_is_a_parse_error() {
        assert_eq!(
            parse("weird{file.txt").unwrap_err().kind,
            ParseErrorKind::UnbalancedBrace
        );
    }

    #[test]
    fn bare_unescaped_close_brace_is_a_parse_error() {
        assert_eq!(
            parse("weird}file.txt").unwrap_err().kind,
            ParseErrorKind::UnbalancedBrace
        );
    }

    #[test]
    fn unterminated_template_is_a_parse_error() {
        assert_eq!(
            parse("{{ name Manager.py").unwrap_err().kind,
            ParseErrorKind::UnbalancedBrace
        );
    }

    #[test]
    fn empty_variable_name_is_a_parse_error() {
        assert_eq!(
            parse("{{}}Manager.py").unwrap_err().kind,
            ParseErrorKind::EmptyVariableName
        );
        assert_eq!(
            parse("{{ }}Manager.py").unwrap_err().kind,
            ParseErrorKind::EmptyVariableName
        );
    }

    #[test]
    fn invalid_variable_name_characters_are_a_parse_error() {
        assert_eq!(
            parse("{{ na-me }}.txt").unwrap_err().kind,
            ParseErrorKind::InvalidVariableName("na-me".to_string())
        );
    }

    #[test]
    fn variable_name_starting_with_digit_is_a_parse_error() {
        assert_eq!(
            parse("{{ 1name }}.txt").unwrap_err().kind,
            ParseErrorKind::InvalidVariableName("1name".to_string())
        );
    }

    #[test]
    fn adjacent_variables_with_no_literal_between_is_a_parse_error() {
        assert_eq!(
            parse("{{a}}{{b}}.txt").unwrap_err().kind,
            ParseErrorKind::AdjacentVariables
        );
    }

    #[test]
    fn error_reports_correct_line_number() {
        let err = parse("a.txt\nweird{file.txt\nb.txt").unwrap_err();
        assert_eq!(err.line_number, 2);
        assert_eq!(err.line, "weird{file.txt");
    }

    #[test]
    fn trailing_slash_marks_entry_as_directory_only() {
        assert_eq!(
            literal_entries(parse("src/").unwrap()),
            vec![("src".to_string(), EntryKind::Directory)]
        );
    }

    #[test]
    fn no_trailing_slash_marks_entry_as_file_only() {
        assert_eq!(
            literal_entries(parse("src").unwrap()),
            vec![("src".to_string(), EntryKind::File)]
        );
    }

    #[test]
    fn whitespace_before_trailing_slash_is_trimmed() {
        assert_eq!(
            literal_entries(parse("src /").unwrap()),
            vec![("src".to_string(), EntryKind::Directory)]
        );
    }

    #[test]
    fn bare_slash_line_is_a_parse_error() {
        assert_eq!(parse("/").unwrap_err().kind, ParseErrorKind::EmptyRule);
    }

    #[test]
    fn trailing_slash_on_template_line_marks_directory_only() {
        let entries = parse("{{ name }}/").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(template.kind(), EntryKind::Directory);
        assert_eq!(
            template.matched_bindings("src"),
            Some(vec![("name".to_string(), "src".to_string(), None)])
        );
    }

    #[test]
    fn required_marker_parses_literal_as_required() {
        let entries = parse("!README.md").unwrap();
        let Entry::Literal { name, required, .. } = &entries[0] else {
            panic!("expected a literal entry");
        };
        assert_eq!(name, "README.md");
        assert!(*required);
    }

    #[test]
    fn no_required_marker_defaults_to_not_required() {
        let entries = parse("README.md").unwrap();
        let Entry::Literal { required, .. } = &entries[0] else {
            panic!("expected a literal entry");
        };
        assert!(!*required);
    }

    #[test]
    fn required_marker_on_directory_entry() {
        let entries = parse("!src/").unwrap();
        let Entry::Literal {
            name,
            kind,
            required,
            ..
        } = &entries[0]
        else {
            panic!("expected a literal entry");
        };
        assert_eq!(name, "src");
        assert_eq!(*kind, EntryKind::Directory);
        assert!(*required);
    }

    #[test]
    fn required_marker_on_wildcard_line() {
        let entries = parse("!*.py").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert!(template.required());
    }

    #[test]
    fn required_marker_is_stripped_from_source_line() {
        let entries = parse("!{{ name }}Manager.py").unwrap();
        let Entry::Template(template) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(template.source_line(), "{{ name }}Manager.py");
    }

    #[test]
    fn escaped_required_marker_is_kept_literally_and_not_required() {
        let entries = parse(r"\!README.md").unwrap();
        let Entry::Literal { name, required, .. } = &entries[0] else {
            panic!("expected a literal entry");
        };
        assert_eq!(name, "!README.md");
        assert!(!*required);
    }

    #[test]
    fn bare_required_marker_is_a_parse_error() {
        assert_eq!(parse("!").unwrap_err().kind, ParseErrorKind::EmptyRule);
    }

    #[test]
    fn required_marker_with_only_trailing_slash_is_a_parse_error() {
        assert_eq!(parse("!/").unwrap_err().kind, ParseErrorKind::EmptyRule);
    }

    #[test]
    fn required_marker_with_only_a_comment_is_a_parse_error() {
        assert_eq!(
            parse("! # comment").unwrap_err().kind,
            ParseErrorKind::EmptyRule
        );
    }

    #[test]
    fn indented_child_expands_to_the_same_entry_as_flat_form() {
        assert_eq!(
            literal_entries(parse("src/\n  util.rs\n").unwrap()),
            vec![
                ("src".to_string(), EntryKind::Directory),
                ("src/util.rs".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn three_level_indentation_expands_correctly() {
        assert_eq!(
            literal_entries(parse("a/\n  b/\n    c.txt\n").unwrap()),
            vec![
                ("a".to_string(), EntryKind::Directory),
                ("a/b".to_string(), EntryKind::Directory),
                ("a/b/c.txt".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn dedent_returns_to_the_correct_ancestor_prefix() {
        assert_eq!(
            literal_entries(parse("a/\n  x/\n    p.txt\n  y/\n    q.txt\n").unwrap()),
            vec![
                ("a".to_string(), EntryKind::Directory),
                ("a/x".to_string(), EntryKind::Directory),
                ("a/x/p.txt".to_string(), EntryKind::File),
                ("a/y".to_string(), EntryKind::Directory),
                ("a/y/q.txt".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn indented_variable_template_expands_the_same_as_flat_form() {
        let entries =
            parse("{{ name }}/\n  {{ name }}Manager.py\n  {{ name }}Presenter.py\n").unwrap();
        assert_eq!(entries.len(), 3);

        let Entry::Template(manager) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert_eq!(manager.source_line(), "{{ name }}/{{ name }}Manager.py");
        assert_eq!(
            manager.matched_bindings("Home/HomeManager.py"),
            Some(vec![
                ("name".to_string(), "Home".to_string(), None),
                ("name".to_string(), "Home".to_string(), None),
            ])
        );

        let Entry::Template(presenter) = &entries[2] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            presenter.matched_bindings("Home/HomePresenter.py"),
            Some(vec![
                ("name".to_string(), "Home".to_string(), None),
                ("name".to_string(), "Home".to_string(), None),
            ])
        );
    }

    #[test]
    fn indented_formatted_variable_round_trips_the_format_suffix_and_matches_correctly() {
        let entries = parse("{{ name|camel }}/\n  {{ name|pascal }}Manager.py\n").unwrap();
        assert_eq!(entries.len(), 2);

        let Entry::Template(manager) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert_eq!(
            manager.source_line(),
            "{{ name|camel }}/{{ name|pascal }}Manager.py"
        );
        assert_eq!(
            manager.matched_bindings("foo/FooManager.py"),
            Some(vec![
                ("name".to_string(), "foo".to_string(), Some(Format::Camel)),
                ("name".to_string(), "Foo".to_string(), Some(Format::Pascal)),
            ])
        );
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_across_an_indented_header_and_child_is_a_parse_error()
     {
        assert_eq!(
            parse("{{ name|camel }}/\n  {{ name }}Manager.py\n")
                .unwrap_err()
                .kind,
            ParseErrorKind::InconsistentVariableFormat("name".to_string())
        );
    }

    #[test]
    fn mixing_formats_across_a_nested_child_and_its_parents_sibling_is_not_a_parse_error() {
        assert!(parse("{{ name|camel }}/\n  {{ name|pascal }}Manager.py\n{{ name }}.md\n").is_ok());
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_across_a_grandparent_and_grandchild_is_a_parse_error()
     {
        let error = parse("{{ name|camel }}/\n  docs/\n    {{ name }}.md\n").unwrap_err();
        assert_eq!(error.line_number, 3);
        assert_eq!(
            error.kind,
            ParseErrorKind::InconsistentVariableFormat("name".to_string())
        );
    }

    #[test]
    fn required_marker_on_indented_child_is_independent_of_its_sibling() {
        let entries =
            parse("{{ name }}/\n  !{{ name }}Manager.py\n  {{ name }}Presenter.py\n").unwrap();

        let Entry::Template(manager) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert!(manager.required());
        assert_eq!(manager.source_line(), "{{ name }}/{{ name }}Manager.py");

        let Entry::Template(presenter) = &entries[2] else {
            panic!("expected a template entry");
        };
        assert!(!presenter.required());
    }

    #[test]
    fn required_marker_on_directory_header_is_independent_of_its_children() {
        let entries = parse("!{{ name }}/\n  {{ name }}Manager.py\n").unwrap();

        let Entry::Template(header) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert!(header.required());
        assert_eq!(header.kind(), EntryKind::Directory);

        let Entry::Template(child) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert!(!child.required());
    }

    #[test]
    fn blank_and_comment_lines_are_transparent_inside_an_indented_block() {
        assert_eq!(
            literal_entries(parse("src/\n  a.txt\n\n  # a comment\n  b.txt\n").unwrap()),
            vec![
                ("src".to_string(), EntryKind::Directory),
                ("src/a.txt".to_string(), EntryKind::File),
                ("src/b.txt".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn tabs_can_be_used_for_indentation() {
        assert_eq!(
            literal_entries(parse("src/\n\tutil.rs\n").unwrap()),
            vec![
                ("src".to_string(), EntryKind::Directory),
                ("src/util.rs".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn different_indent_widths_are_fine_across_different_blocks() {
        assert_eq!(
            literal_entries(parse("a/\n  x.txt\nb/\n    y.txt\n").unwrap()),
            vec![
                ("a".to_string(), EntryKind::Directory),
                ("a/x.txt".to_string(), EntryKind::File),
                ("b".to_string(), EntryKind::Directory),
                ("b/y.txt".to_string(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn indenting_under_a_non_directory_line_is_a_parse_error() {
        assert_eq!(
            parse("a.txt\n  b.txt\n").unwrap_err().kind,
            ParseErrorKind::IndentWithoutDirectoryParent
        );
    }

    #[test]
    fn dedent_that_matches_no_open_level_is_a_parse_error() {
        assert_eq!(
            parse("a/\n  x/\n    p.txt\n c.txt\n").unwrap_err().kind,
            ParseErrorKind::InconsistentIndentation
        );
    }

    #[test]
    fn same_length_indentation_with_different_characters_is_a_parse_error() {
        assert_eq!(
            parse("a/\n  x.txt\n\t\ty.txt\n").unwrap_err().kind,
            ParseErrorKind::InconsistentIndentation
        );
    }

    #[test]
    fn deeper_indentation_that_does_not_extend_the_parent_is_a_parse_error() {
        assert_eq!(
            parse("a/\n  x/\n \t \ty.txt\n").unwrap_err().kind,
            ParseErrorKind::InconsistentIndentation
        );
    }

    #[test]
    fn indented_wildcard_child_expands_correctly() {
        let entries = parse("src/\n  *.py\n").unwrap();
        let Entry::Template(child) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert_eq!(child.source_line(), "src/*.py");
        assert_eq!(child.matched_bindings("src/main.py"), Some(vec![]));
    }

    #[test]
    fn any_depth_header_with_indented_child_expands_correctly() {
        let entries = parse("src/**/\n  main.py\n").unwrap();
        let Entry::Template(header) = &entries[0] else {
            panic!("expected a template entry");
        };
        assert_eq!(header.kind(), EntryKind::Directory);

        let Entry::Template(child) = &entries[1] else {
            panic!("expected a template entry");
        };
        assert_eq!(child.source_line(), "src/**/main.py");
        assert_eq!(
            child.matched_bindings("src/linux/nested/main.py"),
            Some(vec![])
        );
    }

    #[test]
    fn backslash_before_anything_but_an_escapable_character_is_kept_literally() {
        assert_eq!(
            literal_names(parse("a\\b.txt").unwrap()),
            vec!["a\\b.txt".to_string()]
        );
    }

    #[test]
    fn single_open_brace_before_a_double_close_is_a_parse_error() {
        // A likely typo for `{{name}}`: one brace short, so not a variable.
        assert_eq!(
            parse("{name}}.txt").unwrap_err().kind,
            ParseErrorKind::UnbalancedBrace
        );
    }

    #[test]
    fn variable_closed_by_a_single_brace_is_a_parse_error() {
        // A likely typo for `{{ name }}.txt`: one brace short, so the
        // variable is never closed.
        assert_eq!(
            parse("{{ name }.txt").unwrap_err().kind,
            ParseErrorKind::UnbalancedBrace
        );
    }

    fn origins(entries: &[Entry]) -> Vec<(&str, usize)> {
        entries
            .iter()
            .map(|entry| (entry.origin().scope.as_str(), entry.origin().line))
            .collect()
    }

    #[test]
    fn each_entry_records_the_line_it_was_written_on() {
        let entries = parse("# comment\n\na.txt\nsrc/\n    {{ name }}.rs\n!*.md").unwrap();
        assert_eq!(origins(&entries), vec![("", 3), ("", 4), ("", 5), ("", 6)]);
    }

    #[test]
    fn parse_in_records_the_flint_file_s_scope() {
        let entries = parse_in("a.txt\n{{ x }}.md", "src/linux").unwrap();
        assert_eq!(origins(&entries), vec![("src/linux", 1), ("src/linux", 2)]);
    }

    /// The template at `index` in `contents`' parsed entries.
    fn template_at(contents: &str, index: usize) -> Template {
        match parse(contents).unwrap().remove(index) {
            Entry::Template(template) => template,
            Entry::Literal { name, .. } => panic!("{name} is a literal, not a template"),
        }
    }

    fn parse_error(contents: &str) -> (usize, ParseErrorKind) {
        let error = parse(contents).unwrap_err();
        (error.line_number, error.kind)
    }

    #[test]
    fn a_group_header_adds_nothing_to_its_children_s_path() {
        let contents = "src/\n    <name>\n        {{ name }}.md\n        {{ name }}.txt\n";
        let md = template_at(contents, 1);
        assert_eq!(md.source_line(), "src/{{ name }}.md");
        assert_eq!(template_at(contents, 2).source_line(), "src/{{ name }}.txt");
        assert_eq!(
            md.groups(),
            [GroupMembership {
                id: 0,
                depth: 1,
                any_depth: None,
                variables: vec!["name".to_string()],
                source_line: "src/<name>".to_string(),
                origin: Origin {
                    scope: String::new(),
                    line: 2,
                },
                required: false,
            }]
        );
    }

    #[test]
    fn a_group_header_is_not_an_entry_itself() {
        assert_eq!(parse("<name>\n    {{ name }}.md\n").unwrap().len(), 1);
    }

    #[test]
    fn a_top_level_group_header_has_depth_zero_and_several_variables() {
        let template = template_at("<name, kind>\n    {{ name }}.{{ kind }}\n", 0);
        let group = &template.groups()[0];
        assert_eq!(group.depth, 0);
        assert_eq!(group.variables, ["name", "kind"]);
        assert_eq!(group.source_line, "<name, kind>");
    }

    #[test]
    fn nested_group_headers_are_listed_outermost_first() {
        let contents = "<a>\n    <b>\n        {{ a }}-{{ b }}.txt\n";
        let groups = template_at(contents, 0).groups().to_vec();
        assert_eq!(
            groups.iter().map(|group| group.id).collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn a_line_after_a_group_s_block_is_outside_it() {
        let contents = "<name>\n    {{ name }}.md\n{{ name }}.txt\n";
        assert_eq!(template_at(contents, 1).groups(), []);
    }

    #[test]
    fn a_literal_under_a_group_header_is_still_a_literal() {
        let entries = parse("<name>\n    {{ name }}.md\n    README.md\n").unwrap();
        assert!(matches!(&entries[1], Entry::Literal { name, .. } if name == "README.md"));
    }

    #[test]
    fn a_group_header_may_be_followed_by_a_comment() {
        assert!(parse("<name>  # one name per folder\n    {{ name }}.md\n").is_ok());
    }

    #[test]
    fn a_group_header_can_be_required() {
        let template = template_at("!<name>\n    {{ name }}.md\n", 0);
        assert!(template.groups()[0].required);
        assert!(!template.required());
    }

    #[test]
    fn a_group_header_with_a_trailing_slash_is_a_parse_error() {
        assert_eq!(
            parse_error("<name>/\n    {{ name }}.md\n"),
            (1, ParseErrorKind::InvalidGroupHeader)
        );
    }

    #[test]
    fn a_group_header_holding_anything_but_names_is_a_parse_error() {
        for header in [
            "<{{ name }}>",
            "<name|snake>",
            "<>",
            "<a,>",
            "<name",
            "<name>.md",
        ] {
            assert_eq!(
                parse_error(&format!("{header}\n    {{{{ name }}}}.md\n")),
                (1, ParseErrorKind::InvalidGroupHeader),
                "{header}"
            );
        }
    }

    #[test]
    fn a_group_header_name_that_is_not_an_identifier_is_a_parse_error() {
        assert_eq!(
            parse_error("<1name>\n"),
            (1, ParseErrorKind::InvalidVariableName("1name".to_string()))
        );
    }

    #[test]
    fn a_group_header_declaring_a_name_twice_is_a_parse_error() {
        assert_eq!(
            parse_error("<a, a>\n    {{ a }}.md\n"),
            (1, ParseErrorKind::GroupRebindsVariable("a".to_string()))
        );
    }

    #[test]
    fn a_group_header_rebinding_an_enclosing_directory_s_variable_is_a_parse_error() {
        assert_eq!(
            parse_error("{{ name }}/\n    <name>\n        {{ name }}.md\n"),
            (2, ParseErrorKind::GroupRebindsVariable("name".to_string()))
        );
    }

    #[test]
    fn a_group_header_rebinding_an_enclosing_group_s_variable_is_a_parse_error() {
        assert_eq!(
            parse_error("<name>\n    <name>\n        {{ name }}.md\n"),
            (2, ParseErrorKind::GroupRebindsVariable("name".to_string()))
        );
    }

    #[test]
    fn a_group_header_with_nothing_under_it_is_a_parse_error() {
        assert_eq!(
            parse_error("<name>\nREADME.md\n"),
            (1, ParseErrorKind::UnusedGroupVariable("name".to_string()))
        );
    }

    #[test]
    fn a_group_header_variable_no_rule_under_it_uses_is_a_parse_error() {
        assert_eq!(
            parse_error("<name, kind>\n    {{ name }}.md\n    README.md\n"),
            (1, ParseErrorKind::UnusedGroupVariable("kind".to_string()))
        );
    }

    #[test]
    fn a_group_header_under_any_depth_records_where_the_any_depth_is() {
        let template = template_at(
            "src/\n    **/\n        <name>\n            {{ name }}.md\n",
            2,
        );
        let group = &template.groups()[0];
        assert_eq!((group.depth, group.any_depth), (2, Some(1)));
        assert_eq!(group.source_line, "src/**/<name>");
    }

    #[test]
    fn mixing_formatted_and_unformatted_occurrences_across_a_group_s_rules_is_a_parse_error() {
        assert_eq!(
            parse_error("<name>\n    {{ name|camel }}.md\n    {{ name }}.txt\n"),
            (
                3,
                ParseErrorKind::InconsistentVariableFormat("name".to_string())
            )
        );
    }

    #[test]
    fn different_formats_across_a_group_s_rules_are_not_a_parse_error() {
        assert!(parse("<name>\n    {{ name|snake }}.rs\n    {{ name|pascal }}Test.rs\n").is_ok());
    }

    #[test]
    fn an_unescaped_angle_bracket_outside_a_group_header_is_a_parse_error() {
        assert_eq!(
            parse_error("a<b.txt"),
            (1, ParseErrorKind::UnescapedAngleBracket)
        );
        assert_eq!(
            parse_error("a>b.txt"),
            (1, ParseErrorKind::UnescapedAngleBracket)
        );
    }

    #[test]
    fn escaped_angle_brackets_are_kept_literally() {
        assert_eq!(
            literal_names(parse(r"\<odd\>.txt").unwrap()),
            vec!["<odd>.txt".to_string()]
        );
    }

    #[test]
    fn a_badly_indented_group_header_is_a_parse_error() {
        assert_eq!(
            parse_error("a.txt\n    <name>\n        {{ name }}.md\n"),
            (2, ParseErrorKind::IndentWithoutDirectoryParent)
        );
    }

    #[test]
    fn group_header_parse_errors_say_what_to_do() {
        for (contents, expected) in [
            ("a<b.txt", "escape it as \\< or \\>"),
            ("<name>/\n", "holds only variable names between `<` and `>`"),
            ("<a, a>\n", "\"a\" is already bound here"),
            ("<a>\n", "no rule indented under it uses `{{ a }}`"),
        ] {
            let message = parse(contents).unwrap_err().to_string();
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn unknown_format_error_lists_every_format_name() {
        let message = ParseErrorKind::UnknownFormat("kebob".to_string()).to_string();
        let listed = message
            .strip_prefix("unknown format \"kebob\": expected one of ")
            .expect(&message);
        assert_eq!(
            listed.split(", ").collect::<Vec<_>>(),
            Format::filter_names()
        );
    }

    /// The [`Template::inherited_count`] of each template `contents` holds.
    fn inherited_counts(contents: &str) -> Vec<usize> {
        parse(contents)
            .unwrap()
            .iter()
            .filter_map(|entry| match entry {
                Entry::Template(template) => Some(template.inherited_count()),
                Entry::Literal { .. } => None,
            })
            .collect()
    }

    #[test]
    fn a_template_inherits_no_folders_from_its_own_line() {
        assert_eq!(inherited_counts("!{{ name }}/sub/{{ name }}.txt\n"), [0]);
    }

    #[test]
    fn a_template_inherits_each_folder_of_the_lines_it_is_indented_under() {
        assert_eq!(
            inherited_counts(
                "{{ name }}/\n  !{{ name }}/{{ name }}.txt\n  lib/src/\n    !*.rs\nother/\n  *.md\n"
            ),
            [0, 1, 1, 3, 1]
        );
    }

    #[test]
    fn a_group_header_is_not_a_folder_a_template_inherits() {
        assert_eq!(inherited_counts("src/\n  <name>\n    {{ name }}.md\n"), [1]);
    }

    #[test]
    fn each_parse_error_message_names_its_problem() {
        let cases = [
            (
                ParseErrorKind::EmptyVariableName,
                "empty `{{ }}` placeholder",
            ),
            (
                ParseErrorKind::InvalidVariableName("na-me".to_string()),
                "invalid variable name \"na-me\"",
            ),
            (
                ParseErrorKind::InconsistentVariableFormat("name".to_string()),
                "\"name\" is used with a `|format` filter in one place and without one",
            ),
            (
                ParseErrorKind::AdjacentVariables,
                "two `{{ variable }}` placeholders with no literal text between them",
            ),
            (
                ParseErrorKind::EmptyRule,
                "a `!` and/or trailing `/` marker with nothing to apply it to",
            ),
            (
                ParseErrorKind::AnyDepthNotAlone,
                "`**` must be the entire path component",
            ),
            (
                ParseErrorKind::MultipleAnyDepth,
                "only one `**` is supported per `.flint` line",
            ),
            (
                ParseErrorKind::IndentWithoutDirectoryParent,
                "this line is indented, but the previous line isn't a directory",
            ),
            (
                ParseErrorKind::InconsistentIndentation,
                "this line's indentation doesn't match any enclosing line",
            ),
        ];
        for (kind, expected) in cases {
            let message = kind.to_string();
            assert!(
                message.starts_with(expected),
                "{kind:?} rendered as {message:?}"
            );
        }
    }
}
