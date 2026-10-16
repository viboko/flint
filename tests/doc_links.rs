// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checks that every Markdown link into a Rust source file by line range
//! — `` [`name`](../src/checker.rs#L10-L20) `` — still covers the item
//! its text names. lychee can't check these fragments (see
//! `lychee.toml`), and they go stale whenever lines above the item move.
//!
//! The range an item should have runs from the first attribute directly
//! above it (`#[test]`, `#[derive(...)]`), or from the item's own line if
//! it has none, to its closing brace. Doc comments are left out.
//!
//! Run with `UPDATE_DOC_LINKS=1` (or `just fix-doc-links`) to rewrite
//! every stale range in place instead of failing.

// Reading and rewriting the docs on disk is this test's job —
// `clippy.toml`'s bans are for the library.
#![allow(clippy::disallowed_methods)]
// A test reports failure by panicking, and this file's helpers sit outside
// any `#[test]` function, where `clippy.toml`'s test exemptions don't reach.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use regex::{Captures, Regex};

/// Directories never searched for Markdown: build output, virtualenvs
/// and version control, none of which hold the repo's own docs.
const SKIPPED_DIRS: &[&str] = &[".git", ".venv", "node_modules", "target"];

#[test]
fn line_range_links_cover_the_item_they_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = env::var_os("UPDATE_DOC_LINKS").is_some();
    let link = Regex::new(r"\[`(\w+)`\]\(([^()#\s]+\.rs)#L(\d+)-L(\d+)\)").unwrap();
    let any_range_link = Regex::new(r"\]\([^()\s]*\.rs#L").unwrap();
    // Examples of the link shape shown as code, rather than links: a
    // fenced block, or an inline span in double backticks (the only way
    // to put the link's own backticks inside one).
    let code = Regex::new(r"(?ms)^```.*?^```|``.+?``").unwrap();

    let mut problems = String::new();
    let mut checked = 0;
    for markdown in markdown_files(root) {
        let text = fs::read_to_string(&markdown).unwrap();
        let spans: Vec<_> = code.find_iter(&text).map(|m| m.range()).collect();
        let in_code = |at: usize| spans.iter().any(|span| span.contains(&at));
        let shown = markdown.strip_prefix(root).unwrap().display().to_string();

        let understood = link
            .find_iter(&text)
            .filter(|m| !in_code(m.start()))
            .count();
        let found = any_range_link
            .find_iter(&text)
            .filter(|m| !in_code(m.start()))
            .count();
        if found != understood {
            let _ = writeln!(
                problems,
                "{shown}: {} link(s) into a .rs file by line range aren't of the form \
                 [`name`](path.rs#L<start>-L<end>), so can't be checked",
                found - understood
            );
        }

        let rewritten = link.replace_all(&text, |caps: &Captures| {
            let whole = &caps[0];
            if in_code(caps.get(0).unwrap().start()) {
                return whole.to_owned();
            }
            checked += 1;
            let at = format!("{shown}:{}", line_of(&text, caps.get(0).unwrap().start()));
            let name = &caps[1];
            let target = markdown.parent().unwrap().join(&caps[2]);
            let linked = (caps[3].parse().unwrap(), caps[4].parse().unwrap());
            match item_range(&target, name) {
                Err(why) => {
                    let _ = writeln!(problems, "{at}: `{name}` in {}: {why}", &caps[2]);
                    whole.to_owned()
                }
                Ok(actual) if actual == linked => whole.to_owned(),
                Ok((start, end)) => {
                    if !update {
                        let _ = writeln!(
                            problems,
                            "{at}: `{name}` in {} is at #L{start}-L{end}, not #L{}-L{}",
                            &caps[2], linked.0, linked.1
                        );
                    }
                    format!("[`{name}`]({}#L{start}-L{end})", &caps[2])
                }
            }
        });
        if update && rewritten != text {
            fs::write(&markdown, rewritten.as_ref()).unwrap();
        }
    }

    assert!(checked > 0, "found no line-range links at all");
    assert!(
        problems.is_empty(),
        "stale line-range links (run `just fix-doc-links` to fix any that \
         are only out of date):\n{problems}"
    );
}

/// Every `.md` file under `dir`, skipping [`SKIPPED_DIRS`], sorted so
/// failures are reported in a stable order.
fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            if !SKIPPED_DIRS.contains(&entry.file_name().to_str().unwrap_or_default()) {
                found.extend(markdown_files(&path));
            }
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// The 1-based line number of byte `offset` in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

/// The 1-based, inclusive line range of the one item called `name` in
/// `file`: from its first attribute to its closing brace. Relies on the
/// file being rustfmt-formatted, so the closing brace is the first line
/// after the item that is just `}` at the item's own indentation.
fn item_range(file: &Path, name: &str) -> Result<(usize, usize), String> {
    let source = fs::read_to_string(file).map_err(|err| format!("can't read the file: {err}"))?;
    let lines: Vec<&str> = source.lines().collect();
    let declaration = Regex::new(&format!(
        r"^\s*(pub(\([^)]*\))?\s+)?((async|const|unsafe)\s+)*(fn|struct|enum|trait|type|mod)\s+{name}\b"
    ))
    .unwrap();

    let declared: Vec<usize> = (0..lines.len())
        .filter(|&i| declaration.is_match(lines[i]))
        .collect();
    let item = match declared[..] {
        [] => return Err("no item with that name".to_owned()),
        [item] => item,
        _ => {
            let at: Vec<String> = declared.iter().map(|i| format!("L{}", i + 1)).collect();
            return Err(format!("ambiguous: declared at {}", at.join(", ")));
        }
    };

    // Walk up over the attributes and doc comments directly above, and
    // start at the topmost attribute.
    let mut start = item;
    let mut above = item;
    while above > 0 {
        let line = lines[above - 1].trim_start();
        if line.starts_with("#[") {
            start = above - 1;
        } else if !line.starts_with("///") {
            break;
        }
        above -= 1;
    }

    if lines[item].trim_end().ends_with(';') {
        return Ok((start + 1, item + 1));
    }
    let indent = &lines[item][..lines[item].len() - lines[item].trim_start().len()];
    let closing = format!("{indent}}}");
    let end = (item + 1..lines.len())
        .find(|&i| lines[i].trim_end() == closing)
        .ok_or_else(|| format!("no closing `}}` after L{}", item + 1))?;
    Ok((start + 1, end + 1))
}
