// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plain console output: words only — no color, no emoji, no ASCII art —
//! so it reads the same in a terminal, piped to a file, or aloud through
//! a screen reader. For the same reason the messages call the allow-list
//! "flint" rather than ".flint": a screen reader stumbles over the dot.
//! Real paths, which can't be reworded, are printed as they are.
//!
//! Everything here builds [`Line`]s rather than printing them, so the same
//! output can go to a terminal (`main.rs`) or anywhere else, such as a web
//! page.

use std::fmt::Display;

use crate::checker::{GroupValue, Violation};
use crate::run::{Report, ScopeStatus, Unlisted};

/// Which output stream a [`Line`] belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output: results.
    Stdout,
    /// Standard error: failures to read or parse a `.flint` file or
    /// directory.
    Stderr,
}

/// One line of output, without its trailing newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The stream it's written to.
    pub stream: Stream,
    /// The text itself.
    pub text: String,
}

impl Line {
    /// A line of `text` for standard output.
    fn stdout(text: impl Into<String>) -> Self {
        Self {
            stream: Stream::Stdout,
            text: text.into(),
        }
    }

    /// A line of `text` for standard error.
    fn stderr(text: impl Into<String>) -> Self {
        Self {
            stream: Stream::Stderr,
            text: text.into(),
        }
    }
}

/// Everything flint prints for `report`, in order: each scope's
/// violations or failure,
/// then the success message if the run passed — or just an error saying
/// so if the root has no `.flint` file of its own.
///
/// With exactly one `.flint` file in the whole tree (the common case),
/// violations are printed plainly; with more than one, each scope's are
/// labeled with its `.flint` file's path, and every on-disk path in them
/// is qualified by the scope's directory.
#[must_use]
pub fn render<E: Display>(report: &Report<E>) -> Vec<Line> {
    if report.scopes.is_empty() {
        return vec![Line::stderr(NO_FLINT_FILE)];
    }

    let qualify_output = report.scopes.len() > 1;
    let mut lines = Vec::new();
    let mut printed_violations = false;

    for scope in &report.scopes {
        let flint_path = scope_flint_path(&scope.scope);
        match &scope.status {
            ScopeStatus::Checked(violations) if violations.is_empty() => {}
            ScopeStatus::Checked(violations) => {
                if printed_violations {
                    lines.push(Line::stdout(""));
                }
                let block = if qualify_output {
                    scope_violation_lines(&scope.scope, violations)
                } else {
                    violation_lines(violations)
                };
                lines.extend(block.into_iter().map(Line::stdout));
                printed_violations = true;
            }
            ScopeStatus::ListError(Unlisted { dir, error }) => {
                lines.push(Line::stderr(format!(
                    "Failed to check {flint_path}, as {} couldn't be listed: {error}",
                    dir_path(dir)
                )));
            }
            ScopeStatus::ParseError(err) => {
                lines.push(Line::stderr(format!("Failed to parse {flint_path}: {err}")));
            }
            ScopeStatus::ReadError(err) => {
                lines.push(Line::stderr(format!("Failed to read {flint_path}: {err}")));
            }
        }
    }

    if report.passed() {
        lines.push(Line::stdout("All files match"));
    }
    lines
}

/// What flint prints when [`crate::run::run`] stops at a directory it
/// couldn't list.
#[must_use]
pub fn render_unlisted<E: Display>(unlisted: &Unlisted<E>) -> Line {
    Line::stderr(format!(
        "Couldn't list {}, so nothing was checked: {}",
        dir_path(&unlisted.dir),
        unlisted.error
    ))
}

/// What flint prints when the directory it checks has no `.flint` file:
/// without one there, it looks for none further down either.
pub const NO_FLINT_FILE: &str = "No flint file found in the folder being checked. \
Add one there, even one that allows everything, to check it and any flint files inside it.";

/// The path to show a user for `dir`, a `/`-joined path relative to the
/// invocation directory, or `""` for the invocation directory itself.
fn dir_path(dir: &str) -> &str {
    if dir.is_empty() { "." } else { dir }
}

/// The `.flint` file path to show a user for `scope_dir` — a `/`-joined
/// path to a directory containing its own `.flint` file, relative to the
/// invocation directory, or `""` for the invocation directory itself.
#[must_use]
pub fn scope_flint_path(scope_dir: &str) -> String {
    if scope_dir.is_empty() {
        ".flint".to_string()
    } else {
        format!("{scope_dir}/.flint")
    }
}

/// `name`, qualified with `scope_dir` if it's non-empty — used to show a
/// real on-disk path unambiguously when more than one `.flint` file is
/// involved in a single run.
fn prefixed(scope_dir: &str, name: &str) -> String {
    if scope_dir.is_empty() {
        name.to_string()
    } else {
        format!("{scope_dir}/{name}")
    }
}

/// `violation` with every real on-disk path it carries qualified by
/// `scope_dir` (see [`prefixed`]). Rule text — [`Violation::Missing`]'s
/// name, and a group's header — is left alone, since it's written
/// relative to the `.flint` file itself, not to a location on disk.
fn prefix_violation(scope_dir: &str, violation: &Violation) -> Violation {
    let mut violation = violation.clone();
    match &mut violation {
        Violation::Unexpected(name) => *name = prefixed(scope_dir, name),
        Violation::GroupConflict { folder, values, .. } => {
            *folder = if folder.is_empty() {
                scope_dir.to_string()
            } else {
                prefixed(scope_dir, folder)
            };
            for value in values {
                for file in &mut value.files {
                    *file = prefixed(scope_dir, file);
                }
            }
        }
        Violation::Missing { .. } => {}
    }
    violation
}

/// Every violation found in one scope (a directory with its own `.flint`
/// file), labeled with that file's path so it's unambiguous which `.flint`
/// file is responsible when more than one is involved in a single run.
/// Reuses [`violation_lines`] for the actual formatting, after qualifying
/// every real on-disk path in `violations` with `scope_dir` (see
/// `prefix_violation`).
fn scope_violation_lines(scope_dir: &str, violations: &[Violation]) -> Vec<String> {
    let qualified: Vec<Violation> = violations
        .iter()
        .map(|violation| prefix_violation(scope_dir, violation))
        .collect();
    let mut lines = vec![scope_flint_path(scope_dir)];
    lines.extend(violation_lines(&qualified));
    lines
}

/// Every violation found: a block per group header whose files disagree
/// in a folder, then required (`!`) entries with no match on disk, then
/// the list of entries present on disk that no rule allows, with a blank
/// line between blocks.
fn violation_lines(violations: &[Violation]) -> Vec<String> {
    let mut conflicts: Vec<Vec<String>> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    let mut unexpected: Vec<&str> = Vec::new();

    for violation in violations {
        match violation {
            Violation::GroupConflict {
                group,
                folder,
                values,
                ..
            } => conflicts.push(group_conflict_lines(group, folder, values)),
            Violation::Missing { name, .. } => missing.push(name.as_str()),
            Violation::Unexpected(name) => unexpected.push(name.as_str()),
        }
    }

    let mut lines = Vec::new();
    for block in conflicts {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(block);
    }
    if !missing.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(heading("Missing", "required", missing.len()));
        lines.extend(missing.iter().map(|name| bullet(name)));
    }
    if !unexpected.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(heading("Found", "unexpected", unexpected.len()));
        lines.extend(unexpected.iter().map(|name| bullet(name)));
    }
    lines
}

/// A group header's files disagreeing in `folder`: a heading, then one
/// `-` bullet per value, naming the files that gave it.
fn group_conflict_lines(group: &str, folder: &str, values: &[GroupValue]) -> Vec<String> {
    let folder = if folder.is_empty() {
        "./".to_string()
    } else {
        format!("{folder}/")
    };
    let mut lines = vec![format!(
        "{group} needs one value per folder, but found {} in {folder}:",
        values.len()
    )];
    lines.extend(
        values.iter().map(|GroupValue { value, files }| {
            bullet(&format!("\"{value}\": {}", files.join(", ")))
        }),
    );
    lines
}

/// A heading like "Missing 2 required entries:".
fn heading(verb: &str, adjective: &str, count: usize) -> String {
    let noun = if count == 1 { "entry" } else { "entries" };
    format!("{verb} {count} {adjective} {noun}:")
}

/// One `-` bullet naming an entry.
fn bullet(name: &str) -> String {
    format!("- {name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flint_file::{ParseError, ParseErrorKind};
    use crate::origin::Origin;
    use crate::run::ScopeReport;

    fn origin(line: usize) -> Origin {
        Origin {
            scope: String::new(),
            line,
        }
    }

    fn missing(name: &str) -> Violation {
        Violation::Missing {
            name: name.to_string(),
            origin: origin(1),
        }
    }

    fn unexpected(name: &str) -> Violation {
        Violation::Unexpected(name.to_string())
    }

    fn report(scopes: Vec<(&str, ScopeStatus<String>)>) -> Report<String> {
        Report {
            scopes: scopes
                .into_iter()
                .map(|(scope, status)| ScopeReport {
                    scope: scope.to_string(),
                    status,
                })
                .collect(),
        }
    }

    fn unlisted(dir: &str, error: &str) -> Unlisted<String> {
        Unlisted {
            dir: dir.to_string(),
            error: error.to_string(),
        }
    }

    /// `lines` as one string, one line each, with each stderr line marked
    /// with a leading `!`.
    fn text(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|line| match line.stream {
                Stream::Stdout => line.text.clone(),
                Stream::Stderr => format!("!{}", line.text),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn no_flint_file_is_an_error() {
        assert_eq!(render(&report(vec![])), vec![Line::stderr(NO_FLINT_FILE)]);
    }

    #[test]
    fn an_unlisted_directory_is_named() {
        assert_eq!(
            render_unlisted(&unlisted("Library/Mail", "Operation not permitted")),
            Line::stderr(
                "Couldn't list Library/Mail, so nothing was checked: Operation not permitted"
            )
        );
        assert_eq!(
            render_unlisted(&unlisted("", "denied")).text,
            "Couldn't list ., so nothing was checked: denied"
        );
    }

    #[test]
    fn a_clean_run_prints_only_success() {
        let lines = render(&report(vec![("", ScopeStatus::Checked(vec![]))]));
        assert_eq!(lines, vec![Line::stdout("All files match")]);
    }

    #[test]
    fn a_single_scope_s_violations_are_unlabeled_and_grouped() {
        let violations = vec![
            missing("README.md"),
            missing("LICENSE"),
            unexpected("Home/DetailsManager.py"),
            unexpected("stray.txt"),
        ];
        let lines = render(&report(vec![("", ScopeStatus::Checked(violations))]));
        assert_eq!(
            text(&lines),
            "Missing 2 required entries:
- README.md
- LICENSE

Found 2 unexpected entries:
- Home/DetailsManager.py
- stray.txt"
        );
        assert!(lines.iter().all(|line| line.stream == Stream::Stdout));
    }

    #[test]
    fn missing_entries_alone_need_no_leading_blank_line() {
        let lines = render(&report(vec![(
            "",
            ScopeStatus::Checked(vec![missing("a")]),
        )]));
        assert_eq!(text(&lines), "Missing 1 required entry:\n- a");
    }

    #[test]
    fn several_scopes_are_labeled_and_separated_and_clean_ones_skipped() {
        let lines = render(&report(vec![
            ("", ScopeStatus::Checked(vec![unexpected("a.txt")])),
            ("clean", ScopeStatus::Checked(vec![])),
            (
                "src",
                ScopeStatus::Checked(vec![
                    missing("*.md"),
                    unexpected("Home/DetailsManager.py"),
                    unexpected("b.txt"),
                ]),
            ),
        ]));
        assert_eq!(
            text(&lines),
            ".flint
Found 1 unexpected entry:
- a.txt

src/.flint
Missing 1 required entry:
- *.md

Found 2 unexpected entries:
- src/Home/DetailsManager.py
- src/b.txt"
        );
    }

    #[test]
    fn failures_go_to_stderr_with_no_success_message() {
        let parse_error = ParseError {
            line_number: 2,
            line: "{{".to_string(),
            kind: ParseErrorKind::UnbalancedBrace,
        };
        let lines = render(&report(vec![
            ("", ScopeStatus::ReadError("no access".to_string())),
            ("a", ScopeStatus::ParseError(parse_error.clone())),
            ("b", ScopeStatus::ListError(unlisted("b/private", "gone"))),
            ("c", ScopeStatus::Checked(vec![])),
        ]));
        assert_eq!(
            text(&lines),
            format!(
                "!Failed to read .flint: no access
!Failed to parse a/.flint: {parse_error}
!Failed to check b/.flint, as b/private couldn't be listed: gone"
            )
        );
    }

    #[test]
    fn a_failure_does_not_add_a_blank_line_before_later_violations() {
        let lines = render(&report(vec![
            ("", ScopeStatus::ReadError("no access".to_string())),
            ("a", ScopeStatus::Checked(vec![unexpected("x")])),
        ]));
        assert_eq!(lines[1].text, "a/.flint");
    }

    #[test]
    fn scope_flint_path_for_the_root_scope_is_bare() {
        assert_eq!(scope_flint_path(""), ".flint");
    }

    #[test]
    fn scope_flint_path_for_a_nested_scope_is_qualified() {
        assert_eq!(scope_flint_path("src/linux"), "src/linux/.flint");
    }

    #[test]
    fn prefixed_at_the_root_scope_is_unchanged() {
        assert_eq!(prefixed("", "extra.rs"), "extra.rs");
    }

    #[test]
    fn prefixed_at_a_nested_scope_gets_the_scope_dir_prepended() {
        assert_eq!(prefixed("src/linux", "extra.rs"), "src/linux/extra.rs");
    }

    #[test]
    fn prefix_violation_qualifies_unexpected_entries() {
        assert_eq!(
            prefix_violation("src", &unexpected("extra.rs")),
            unexpected("src/extra.rs")
        );
    }

    #[test]
    fn prefix_violation_leaves_missing_rule_text_alone() {
        assert_eq!(prefix_violation("src", &missing("*.md")), missing("*.md"));
    }

    fn group_conflict(folder: &str) -> Violation {
        Violation::GroupConflict {
            group: "<name>".to_string(),
            folder: folder.to_string(),
            values: vec![
                GroupValue {
                    value: "blah".to_string(),
                    files: vec![prefixed(folder, "blah.md"), prefixed(folder, "blah.txt")],
                },
                GroupValue {
                    value: "foo".to_string(),
                    files: vec![prefixed(folder, "foo.md")],
                },
            ],
            origin: origin(1),
        }
    }

    #[test]
    fn a_group_conflict_lists_each_value_s_files_before_other_sections() {
        let lines = render(&report(vec![(
            "",
            ScopeStatus::Checked(vec![group_conflict("src"), missing("README.md")]),
        )]));
        assert_eq!(
            text(&lines),
            "<name> needs one value per folder, but found 2 in src/:
- \"blah\": src/blah.md, src/blah.txt
- \"foo\": src/foo.md

Missing 1 required entry:
- README.md"
        );
    }

    #[test]
    fn a_group_conflict_in_the_flint_file_s_own_folder_names_it_dot_slash() {
        let lines = render(&report(vec![(
            "",
            ScopeStatus::Checked(vec![group_conflict("")]),
        )]));
        assert!(text(&lines).starts_with("<name> needs one value per folder, but found 2 in ./:"));
    }

    #[test]
    fn prefix_violation_qualifies_a_group_conflict_s_folder_and_files() {
        assert_eq!(
            prefix_violation("pkg", &group_conflict("")),
            Violation::GroupConflict {
                group: "<name>".to_string(),
                folder: "pkg".to_string(),
                values: vec![
                    GroupValue {
                        value: "blah".to_string(),
                        files: vec!["pkg/blah.md".to_string(), "pkg/blah.txt".to_string()],
                    },
                    GroupValue {
                        value: "foo".to_string(),
                        files: vec!["pkg/foo.md".to_string()],
                    },
                ],
                origin: origin(1),
            }
        );
        assert!(matches!(
            prefix_violation("pkg", &group_conflict("src")),
            Violation::GroupConflict { folder, .. } if folder == "pkg/src"
        ));
    }

    #[test]
    fn group_conflicts_are_set_off_from_each_other_by_a_blank_line() {
        let lines = render(&report(vec![(
            "",
            ScopeStatus::Checked(vec![group_conflict("a"), group_conflict("b")]),
        )]));
        let text = text(&lines);
        assert!(
            text.contains(
                "- \"foo\": a/foo.md\n\n<name> needs one value per folder, but found 2 in b/:"
            ),
            "{text}"
        );
    }

    /// One report for each kind of thing [`render`] can print, between
    /// them reaching every message it has.
    ///
    /// The tests below run over all of these, so they check
    /// every message without needing a real directory that produces it.
    /// When `render` gains a message, add a report here that prints it:
    /// [`violation_kind`] and [`status_kind`] stop compiling when a
    /// variant is added, as a reminder.
    fn every_kind_of_report() -> Vec<Report<String>> {
        let parse_error = ParseError {
            line_number: 2,
            line: "{{".to_string(),
            kind: ParseErrorKind::UnbalancedBrace,
        };
        vec![
            report(vec![]),
            report(vec![("", ScopeStatus::Checked(vec![]))]),
            report(vec![("", ScopeStatus::Checked(vec![missing("a")]))]),
            report(vec![(
                "",
                ScopeStatus::Checked(vec![missing("a"), missing("b")]),
            )]),
            report(vec![("", ScopeStatus::Checked(vec![unexpected("a")]))]),
            report(vec![(
                "",
                ScopeStatus::Checked(vec![unexpected("a"), unexpected("b")]),
            )]),
            report(vec![(
                "",
                ScopeStatus::Checked(vec![group_conflict(""), group_conflict("src")]),
            )]),
            report(vec![
                ("", ScopeStatus::Checked(vec![unexpected("a")])),
                (
                    "src",
                    ScopeStatus::Checked(vec![group_conflict(""), missing("b")]),
                ),
            ]),
            report(vec![("", ScopeStatus::ReadError("no access".to_string()))]),
            report(vec![("", ScopeStatus::ParseError(parse_error))]),
            report(vec![("", ScopeStatus::ListError(unlisted("a", "gone")))]),
        ]
    }

    fn violation_kind(violation: &Violation) -> &'static str {
        match violation {
            Violation::GroupConflict { .. } => "group conflict",
            Violation::Missing { .. } => "missing",
            Violation::Unexpected(_) => "unexpected",
        }
    }

    fn status_kind(status: &ScopeStatus<String>) -> &'static str {
        match status {
            ScopeStatus::Checked(_) => "checked",
            ScopeStatus::ListError(_) => "list error",
            ScopeStatus::ParseError(_) => "parse error",
            ScopeStatus::ReadError(_) => "read error",
        }
    }

    #[test]
    fn every_kind_of_report_covers_every_violation_and_status() {
        let reports = every_kind_of_report();
        let scopes = || reports.iter().flat_map(|report| &report.scopes);

        let mut statuses: Vec<_> = scopes().map(|scope| status_kind(&scope.status)).collect();
        statuses.sort_unstable();
        statuses.dedup();
        assert_eq!(
            statuses,
            ["checked", "list error", "parse error", "read error"]
        );

        let mut violations: Vec<_> = scopes()
            .filter_map(|scope| match &scope.status {
                ScopeStatus::Checked(violations) => Some(violations),
                _ => None,
            })
            .flatten()
            .map(violation_kind)
            .collect();
        violations.sort_unstable();
        violations.dedup();
        assert_eq!(violations, ["group conflict", "missing", "unexpected"]);
    }

    /// Whether `c` is an emoji or other pictograph, going by where
    /// Unicode's symbol blocks start (arrows, at U+2190) — so not
    /// punctuation like the dash in a parse error's message.
    fn is_emoji(c: char) -> bool {
        c >= '\u{2190}'
    }

    #[test]
    fn no_line_has_color_codes_or_emoji() {
        for report in every_kind_of_report() {
            for line in render(&report) {
                assert!(
                    !line.text.contains(|c| c == '\u{1b}' || is_emoji(c)),
                    "{:?}",
                    line.text
                );
            }
        }
    }

    /// A screen reader reads ".flint" as "dot flint", or worse, so the
    /// headings and the success message say "flint". A `.flint` file's
    /// own path, like any other path, is printed as it is.
    #[test]
    fn headings_and_the_success_message_name_no_dot_flint() {
        for report in every_kind_of_report() {
            for line in render(&report) {
                let is_heading = line.stream == Stream::Stdout && line.text.ends_with(':');
                if is_heading || line.text.starts_with("All files") {
                    assert!(!line.text.contains(".flint"), "{:?}", line.text);
                }
            }
        }
    }

    fn output_for(violations: Vec<Violation>) -> String {
        text(&render(&report(vec![(
            "",
            ScopeStatus::Checked(violations),
        )])))
    }

    #[test]
    fn one_missing_entry_is_singular() {
        assert_eq!(
            output_for(vec![missing("AGENTS.md")]),
            "Missing 1 required entry:
- AGENTS.md"
        );
    }

    #[test]
    fn two_missing_entries_are_plural() {
        assert_eq!(
            output_for(vec![missing("justfile"), missing("AGENTS.md")]),
            "Missing 2 required entries:
- justfile
- AGENTS.md"
        );
    }

    #[test]
    fn one_unexpected_entry_is_singular() {
        assert_eq!(
            output_for(vec![unexpected(".claude/skills/addFeature")]),
            "Found 1 unexpected entry:
- .claude/skills/addFeature"
        );
    }

    #[test]
    fn two_unexpected_entries_are_plural() {
        assert_eq!(
            output_for(vec![
                unexpected(".claude/skills/addFeature"),
                unexpected(".claude/skills/releaseBB"),
            ]),
            "Found 2 unexpected entries:
- .claude/skills/addFeature
- .claude/skills/releaseBB"
        );
    }
}
