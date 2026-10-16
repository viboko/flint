// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The playground's examples: one Markdown file each in `examples/`, read
//! by people on GitHub, loaded into the playground by its example buttons,
//! and checked by this crate's tests to produce exactly the output they
//! show. See `playground/README.md` for the format.

use serde::Serialize;

use crate::TreeEntry;

/// One example, parsed from its Markdown file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Example {
    /// The file name without `.md`, e.g. `01-allow-list`.
    pub id: String,
    /// The `# ` heading.
    pub title: String,
    /// The Markdown between the title and the first `## ` section.
    pub explanation: String,
    /// Every file and directory in the example's tree, `.flint` files
    /// (with their contents) included.
    pub tree: Vec<TreeEntry>,
    /// What flint prints for the tree.
    pub output: String,
}

/// Parses example `id`'s Markdown `source`:
///
/// - a `# Title` first line;
/// - the explanation, up to the first `## ` heading;
/// - a `## Files` section listing the tree's files and directories, one
///   path per line, with a trailing `/` for a directory;
/// - one section per `.flint` file, headed by its path (`## .flint`,
///   ``## `src/.flint` ``) and holding its contents;
/// - a `## Output` section holding what flint prints.
///
/// Every section holds exactly one fenced code block and nothing else.
///
/// # Errors
/// Returns a description of the first thing that doesn't follow that
/// format.
pub fn parse(id: &str, source: &str) -> Result<Example, String> {
    let mut lines = source.lines();
    let title = lines
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .ok_or("the first line isn't a `# ` title")?
        .trim()
        .to_string();

    let mut explanation = Vec::new();
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in lines {
        if let Some(heading) = line.strip_prefix("## ") {
            sections.push((heading.trim().trim_matches('`').to_string(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        } else {
            explanation.push(line);
        }
    }

    let mut tree = Vec::new();
    let mut files_seen = false;
    let mut output = None;
    for (heading, body) in &sections {
        let block = code_block(body).map_err(|err| format!("`## {heading}`: {err}"))?;
        if heading == "Files" {
            files_seen = true;
            for line in block.iter().filter(|line| !line.trim().is_empty()) {
                tree.push(match line.strip_suffix('/') {
                    Some(path) => TreeEntry::directory(path),
                    None => TreeEntry::file(line, ""),
                });
            }
        } else if heading == "Output" {
            output = Some(block.join("\n"));
        } else if heading == ".flint" || heading.ends_with("/.flint") {
            tree.push(TreeEntry::file(heading, &(block.join("\n") + "\n")));
        } else {
            return Err(format!(
                "unknown section `## {heading}` (expected Files, Output or a .flint path)"
            ));
        }
    }

    if !files_seen {
        return Err("no `## Files` section".to_string());
    }
    Ok(Example {
        id: id.to_string(),
        title,
        explanation: explanation.join("\n").trim().to_string(),
        tree,
        output: output.ok_or("no `## Output` section")?,
    })
}

/// The lines inside the one fenced code block that makes up `body`,
/// which may otherwise hold only blank lines.
fn code_block<'a>(body: &[&'a str]) -> Result<Vec<&'a str>, String> {
    let mut rest = body.iter().skip_while(|line| line.trim().is_empty());
    if !rest.next().is_some_and(|line| line.starts_with("```")) {
        return Err("expected a fenced code block".to_string());
    }
    let mut block = Vec::new();
    for line in rest.by_ref() {
        if *line == "```" {
            return if rest.all(|line| line.trim().is_empty()) {
                Ok(block)
            } else {
                Err("unexpected text after the code block".to_string())
            };
        }
        block.push(*line);
    }
    Err("unterminated code block".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FENCE: &str = "```";

    fn source(sections: &str) -> String {
        format!("# A title\n\nSome *explanation*.\n\nMore.\n\n{sections}")
    }

    #[test]
    fn a_complete_example_parses() {
        let source = source(&format!(
            "## Files\n\n{FENCE}text\nREADME.md\nsrc/\n\nsrc/main.rs\n{FENCE}\n\n\
             ## .flint\n\n{FENCE}text\nREADME.md\n{FENCE}\n\n\
             ## `src/.flint`\n\n{FENCE}text\nmain.rs\n{FENCE}\n\n\
             ## Output\n\n{FENCE}text\nline 1\n  line 2\n{FENCE}\n"
        ));
        assert_eq!(
            parse("01-x", &source),
            Ok(Example {
                id: "01-x".to_string(),
                title: "A title".to_string(),
                explanation: "Some *explanation*.\n\nMore.".to_string(),
                tree: vec![
                    TreeEntry::file("README.md", ""),
                    TreeEntry::directory("src"),
                    TreeEntry::file("src/main.rs", ""),
                    TreeEntry::file(".flint", "README.md\n"),
                    TreeEntry::file("src/.flint", "main.rs\n"),
                ],
                output: "line 1\n  line 2".to_string(),
            })
        );
    }

    #[test]
    fn a_missing_title_is_an_error() {
        assert_eq!(
            parse("x", "Title\n"),
            Err("the first line isn't a `# ` title".to_string())
        );
    }

    #[test]
    fn missing_sections_are_errors() {
        let output = format!("## Output\n\n{FENCE}text\nok\n{FENCE}\n");
        assert_eq!(
            parse("x", &source(&output)),
            Err("no `## Files` section".to_string())
        );
        let files = format!("## Files\n\n{FENCE}text\n{FENCE}\n");
        assert_eq!(
            parse("x", &source(&files)),
            Err("no `## Output` section".to_string())
        );
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let source = source(&format!("## Notes\n\n{FENCE}text\n{FENCE}\n"));
        assert_eq!(
            parse("x", &source),
            Err("unknown section `## Notes` (expected Files, Output or a .flint path)".to_string())
        );
    }

    #[test]
    fn a_section_must_be_one_code_block() {
        let no_block = source("## Files\n\nprose\n");
        assert_eq!(
            parse("x", &no_block),
            Err("`## Files`: expected a fenced code block".to_string())
        );
        let unterminated = source(&format!("## Files\n\n{FENCE}text\na\n"));
        assert_eq!(
            parse("x", &unterminated),
            Err("`## Files`: unterminated code block".to_string())
        );
        let trailing = source(&format!("## Files\n\n{FENCE}text\na\n{FENCE}\nprose\n"));
        assert_eq!(
            parse("x", &trailing),
            Err("`## Files`: unexpected text after the code block".to_string())
        );
    }
}
