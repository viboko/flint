// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! flint's browser playground: runs flint over a directory tree held in
//! memory, using the same `flint::run` and `flint::output` the CLI uses
//! over a real disk — so the playground shows exactly what the CLI would
//! print for the same files. Also home to the playground's examples (see
//! [`example`]).
//!
//! The page calls the two `#[wasm_bindgen]` functions, [`lint_json`] and
//! [`examples_json`], exchanging plain JSON.

pub mod example;

use flint::checker::{GroupValue, Violation};
use flint::memory_tree::{InvalidPath, MemoryTree};
use flint::output::scope_flint_path;
use flint::run::{Report, ScopeStatus};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

/// One file or directory in a tree to lint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// `/`-joined path relative to the tree's root.
    pub path: String,
    /// Whether this is a directory rather than a file.
    pub directory: bool,
    /// A file's contents — only ever read for a `.flint` file.
    #[serde(default)]
    pub contents: String,
}

impl TreeEntry {
    /// A file at `path` holding `contents`.
    #[must_use]
    pub fn file(path: &str, contents: &str) -> Self {
        Self {
            path: path.to_string(),
            directory: false,
            contents: contents.to_string(),
        }
    }

    /// A directory at `path`.
    #[must_use]
    pub fn directory(path: &str) -> Self {
        Self {
            path: path.to_string(),
            directory: true,
            contents: String::new(),
        }
    }
}

/// What flint makes of a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// Everything flint prints, stdout and stderr interleaved in order,
    /// one line per line.
    pub output: String,
    /// Whether flint would exit successfully.
    pub passed: bool,
    /// Every problem found, located for the page to highlight.
    pub problems: Vec<Problem>,
}

/// One problem flint found, located in the tree and, where a rule is
/// responsible, in a `.flint` file. Every path is relative to the tree's
/// root, whichever `.flint` file found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    /// An entry no rule allows.
    Unexpected {
        /// The entry.
        path: String,
        /// The `.flint` file whose check found it.
        flint_file: String,
        /// A sentence describing the problem, without its location.
        message: String,
    },
    /// A required rule with nothing matching it.
    Missing {
        /// What flint reports missing, inside the folder of the `.flint`
        /// file whose check found it: where the entry should be — a
        /// concrete path when flint can guess one, otherwise the rule's
        /// `{{ }}`/`*` pattern, which the page places in the deepest
        /// folder of it that exists. Ends in `/` for a folder.
        path: String,
        /// The `.flint` file the rule was written in.
        flint_file: String,
        /// The rule's 1-based line in it.
        line: usize,
        /// A sentence describing the problem, without its location.
        message: String,
    },
    /// A group header whose rules captured more than one value in one
    /// folder.
    GroupConflict {
        /// Every entry that gave one of the values.
        paths: Vec<String>,
        /// The `.flint` file the header was written in.
        flint_file: String,
        /// The header's 1-based line in it.
        line: usize,
        /// A sentence describing the problem, without its location.
        message: String,
    },
    /// A `.flint` file that couldn't be parsed.
    ParseError {
        /// The `.flint` file.
        flint_file: String,
        /// The 1-based line that's wrong.
        line: usize,
        /// The parse error, as flint prints it.
        message: String,
    },
}

/// What a group conflict found in `folder` (relative to the tree's root):
/// each value and the entries that gave it.
fn group_conflict_message(folder: &str, values: &[GroupValue]) -> String {
    let folder = if folder.is_empty() {
        "./".to_string()
    } else {
        format!("{folder}/")
    };
    let values: Vec<String> = values
        .iter()
        .map(|GroupValue { value, files }| format!("\"{value}\" ({})", files.join(", ")))
        .collect();
    format!(
        "Needs one value per folder, but found {} in {folder}: {}.",
        values.len(),
        values.join("; ")
    )
}

/// Every problem in `report`, in the order flint reports them.
fn problems<E>(report: &Report<E>) -> Vec<Problem> {
    let mut problems = Vec::new();
    for scope in &report.scopes {
        let in_scope = |name: &str| {
            if scope.scope.is_empty() {
                name.to_string()
            } else {
                format!("{}/{name}", scope.scope)
            }
        };
        match &scope.status {
            ScopeStatus::Checked(violations) => {
                problems.extend(violations.iter().map(|violation| {
                    match violation {
                        Violation::Unexpected(name) => Problem::Unexpected {
                            path: in_scope(name),
                            flint_file: scope_flint_path(&scope.scope),
                            message: "No rule allows this.".to_string(),
                        },
                        Violation::GroupConflict {
                            folder,
                            values,
                            origin,
                            ..
                        } => Problem::GroupConflict {
                            paths: values
                                .iter()
                                .flat_map(|value| &value.files)
                                .map(|file| in_scope(file.trim_end_matches('/')))
                                .collect(),
                            flint_file: scope_flint_path(&origin.scope),
                            line: origin.line,
                            message: group_conflict_message(
                                &if folder.is_empty() {
                                    scope.scope.clone()
                                } else {
                                    in_scope(folder)
                                },
                                values,
                            ),
                        },
                        Violation::Missing { name, origin } => Problem::Missing {
                            path: in_scope(name),
                            flint_file: scope_flint_path(&origin.scope),
                            line: origin.line,
                            message: "Required, but missing.".to_string(),
                        },
                    }
                }));
            }
            ScopeStatus::ParseError(err) => problems.push(Problem::ParseError {
                flint_file: scope_flint_path(&scope.scope),
                line: err.line_number,
                message: err.to_string(),
            }),
            // A MemoryTree can always be read and listed.
            ScopeStatus::ReadError(_) | ScopeStatus::ListError(_) => {}
        }
    }
    problems
}

/// Runs flint over `tree`.
///
/// # Errors
/// Returns `Err` if an entry's path couldn't exist in a real tree (see
/// [`InvalidPath`]).
pub fn lint(tree: &[TreeEntry]) -> Result<Outcome, InvalidPath> {
    let mut memory_tree = MemoryTree::new();
    for entry in tree {
        if entry.directory {
            memory_tree.add_directory(&entry.path)?;
        } else {
            memory_tree.add_file(&entry.path, &entry.contents)?;
        }
    }
    let report = flint::run::run(&memory_tree);
    let output: Vec<String> = flint::output::render(&report)
        .into_iter()
        .map(|line| line.text)
        .collect();
    Ok(Outcome {
        output: output.join("\n"),
        passed: report.passed(),
        problems: problems(&report),
    })
}

/// [`lint`] for the page: takes the tree as a JSON array of
/// [`TreeEntry`] objects, and returns the [`Outcome`] as JSON — or
/// `{"error": "..."}` if the tree isn't valid.
///
/// # Panics
/// Never: plain structs and strings always serialize.
#[wasm_bindgen(js_name = lint)]
#[must_use]
pub fn lint_json(tree: &str) -> String {
    let result = serde_json::from_str::<Vec<TreeEntry>>(tree)
        .map_err(|err| err.to_string())
        .and_then(|tree| lint(&tree).map_err(|err| err.to_string()));
    match result {
        Ok(outcome) => serde_json::to_string(&outcome),
        Err(error) => serde_json::to_string(&serde_json::json!({ "error": error })),
    }
    .expect("plain structs and strings always serialize")
}

/// Every example (see [`example`]), as a JSON array of
/// [`example::Example`] objects, in order.
///
/// # Panics
/// If an example doesn't parse — which the tests rule out before it can
/// ship.
#[wasm_bindgen(js_name = examples)]
#[must_use]
pub fn examples_json() -> String {
    let examples: Vec<example::Example> = EXAMPLE_SOURCES
        .iter()
        .map(|(id, source)| example::parse(id, source).expect("examples are tested to parse"))
        .collect();
    serde_json::to_string(&examples).expect("plain structs and strings always serialize")
}

/// Every example in `examples/`, as `(id, Markdown source)`, sorted by
/// id — embedded at build time by `build.rs`.
pub const EXAMPLE_SOURCES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/examples.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_tree_passes() {
        let outcome = lint(&[
            TreeEntry::file(".flint", "a.txt"),
            TreeEntry::file("a.txt", ""),
        ]);
        assert_eq!(
            outcome,
            Ok(Outcome {
                output: "All files match".to_string(),
                passed: true,
                problems: vec![],
            })
        );
    }

    #[test]
    fn output_lines_are_joined_in_order() {
        let outcome = lint(&[
            TreeEntry::file(".flint", "a/"),
            TreeEntry::file("a/.flint", "{{"),
            TreeEntry::directory("b"),
        ])
        .unwrap();
        // The root's violations on stdout, then the nested file's parse
        // error on stderr, in the order flint prints them.
        assert!(outcome.output.starts_with(
            ".flint\nFound 1 unexpected entry:\n- b\n\
             Failed to parse a/.flint: "
        ));
        assert!(!outcome.passed);
    }

    #[test]
    fn an_invalid_path_is_an_error() {
        assert_eq!(
            lint(&[TreeEntry::directory("a//b")]),
            Err(InvalidPath("a//b".to_string()))
        );
    }

    #[test]
    fn every_example_parses_and_prints_exactly_its_documented_output() {
        assert_ne!(EXAMPLE_SOURCES.len(), 0, "no examples were embedded");
        for (id, source) in EXAMPLE_SOURCES {
            let example = example::parse(id, source).unwrap_or_else(|err| panic!("{id}: {err}"));
            let outcome = lint(&example.tree).unwrap_or_else(|err| panic!("{id}: {err}"));
            let actual = outcome.output;
            assert!(
                actual == example.output,
                "{id}.md's `## Output` doesn't match what flint prints for it:\n\
                 --- documented ---\n{}\n--- actual ---\n{actual}\n---",
                example.output
            );
        }
    }

    #[test]
    fn problems_are_located_by_full_path_and_flint_line() {
        let outcome = lint(&[
            TreeEntry::file(".flint", "!README.md\npkg/\n    */\n        !LICENSE"),
            TreeEntry::file("pkg/a/.flint", "# rules\n{{ x }}_{{ x }}.txt"),
            TreeEntry::file("pkg/a/one_two.txt", ""),
            TreeEntry::file("pkg/a/stray", ""),
            TreeEntry::file("pkg/b/.flint", "{{"),
        ])
        .unwrap();
        assert_eq!(
            outcome.problems,
            vec![
                Problem::Missing {
                    path: "README.md".to_string(),
                    flint_file: ".flint".to_string(),
                    line: 1,
                    message: "Required, but missing.".to_string(),
                },
                // The root's `pkg/*/LICENSE`, imported into `pkg/a`'s
                // check, is reported missing where it belongs there.
                Problem::Missing {
                    path: "pkg/a/LICENSE".to_string(),
                    flint_file: ".flint".to_string(),
                    line: 4,
                    message: "Required, but missing.".to_string(),
                },
                Problem::Unexpected {
                    path: "pkg/a/one_two.txt".to_string(),
                    flint_file: "pkg/a/.flint".to_string(),
                    message: "No rule allows this.".to_string(),
                },
                Problem::Unexpected {
                    path: "pkg/a/stray".to_string(),
                    flint_file: "pkg/a/.flint".to_string(),
                    message: "No rule allows this.".to_string(),
                },
                Problem::ParseError {
                    flint_file: "pkg/b/.flint".to_string(),
                    line: 1,
                    message: outcome_parse_message(),
                },
            ]
        );
    }

    #[test]
    fn a_group_conflict_is_located_at_every_file_and_the_header_s_line() {
        let outcome = lint(&[
            TreeEntry::file(
                ".flint",
                "# rules\n<name>\n    {{ name }}/\n    {{ name }}.md",
            ),
            TreeEntry::directory("a"),
            TreeEntry::file("b.md", ""),
        ])
        .unwrap();
        assert_eq!(
            outcome.problems,
            vec![Problem::GroupConflict {
                paths: vec!["a".to_string(), "b.md".to_string()],
                flint_file: ".flint".to_string(),
                line: 2,
                message: "Needs one value per folder, but found 2 in ./: \"a\" (a/); \"b\" (b.md)."
                    .to_string(),
            }]
        );
    }

    #[test]
    fn a_group_conflict_in_a_nested_flint_file_s_folder_names_the_folder_from_the_root() {
        let outcome = lint(&[
            TreeEntry::file(".flint", "src/\n    <name>\n        {{ name }}.md"),
            TreeEntry::file("src/.flint", ""),
            TreeEntry::file("src/a.md", ""),
            TreeEntry::file("src/b.md", ""),
        ])
        .unwrap();
        assert!(matches!(
            &outcome.problems[..],
            [Problem::GroupConflict { message, .. }] if message.contains(" in src/:")
        ));
    }

    /// The parse error flint gives for a `.flint` file holding just `{{`.
    fn outcome_parse_message() -> String {
        flint::flint_file::parse("{{").unwrap_err().to_string()
    }

    #[test]
    fn lint_json_takes_and_returns_json() {
        let outcome: serde_json::Value = serde_json::from_str(&lint_json(
            r#"[{"path": ".flint", "directory": false, "contents": "a/"},
                {"path": "a", "directory": true},
                {"path": "b", "directory": false}]"#,
        ))
        .unwrap();
        assert_eq!(outcome["passed"], false);
        assert_eq!(
            outcome["problems"],
            serde_json::json!([{
                "kind": "unexpected",
                "path": "b",
                "flint_file": ".flint",
                "message": "No rule allows this.",
            }])
        );
        assert!(outcome["output"].as_str().unwrap().contains("- b"));
    }

    #[test]
    fn lint_json_reports_bad_input_as_an_error() {
        let bad_path: serde_json::Value =
            serde_json::from_str(&lint_json(r#"[{"path": "a//b", "directory": true}]"#)).unwrap();
        assert_eq!(
            bad_path,
            serde_json::json!({"error": "invalid path: \"a//b\""})
        );
        let not_json: serde_json::Value = serde_json::from_str(&lint_json("nope")).unwrap();
        assert!(not_json["error"].as_str().unwrap().starts_with("expected"));
    }

    #[test]
    fn examples_json_lists_every_example_in_order() {
        let examples: serde_json::Value = serde_json::from_str(&examples_json()).unwrap();
        let ids: Vec<&str> = examples
            .as_array()
            .unwrap()
            .iter()
            .map(|example| example["id"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = EXAMPLE_SOURCES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, expected);
        assert_eq!(examples[0]["tree"][0]["directory"], false);
    }
}
