# Agent instructions for flint

This file exists so that work stays consistent across sessions/agents that
have no memory of each other. If you're an agent working in this repo,
read this first.

## What flint is

A folder linter, written in Rust: it checks that the files/folders in a
directory match an allow-list in a `.flint` file, without looking inside
any file. See [`README.md`](README.md), [`docs/usage.md`](docs/usage.md),
and [`docs/flint-file-format.md`](docs/flint-file-format.md) for the
current, user-facing spec — those are the source of truth for behavior,
not this file.

## The rule that matters most: keep the docs honest

**Every time you change what flint actually does, update in the same
change:**

- `docs/features.md`
- `docs/flint-file-format.md` (the `.flint` file spec) and/or
  `docs/usage.md` (CLI usage), whichever the change actually affects
- `CHANGELOG.md` (add an entry under `[Unreleased]`, or a new version
  section if you're also bumping the version)
- the repo's own root [`.flint`](.flint) file, if the set of top-level
  files/folders changed

Stale docs are treated as a bug. Don't describe planned or aspirational
behavior as current — only what's implemented and tested.

`docs/flint-file-format.md`'s "Examples" and "Parse errors" tables link
each row to the specific unit test that proves it, by exact line range
(e.g. `../src/checker.rs#L146-L154`), and `docs/architecture.md` links to
a few types the same way. Any change that moves lines in those source
files can leave the ranges pointing at the wrong code. `cargo test`
catches that (`tests/doc_links.rs`), and `just fix-doc-links` rewrites
the stale ranges — run it in the same change. Write a new link as
`` [`item_name`](../src/file.rs#L1-L1) `` (that exact shape, or the check
fails on it) with any range, and `just fix-doc-links` fills in the right
one.

## License headers

Every file states its copyright and license in an SPDX header at its
top, as a comment in the file's own syntax:

```text
// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
```

Give a new file the same two lines (after a shebang, if it has one).
Files that can't carry a comment — Markdown, JSON, lockfiles, images,
the fuzzer's inputs — are covered by a path pattern in
[`REUSE.toml`](REUSE.toml) instead; add a pattern there for a new kind
of such file. `just reuse` checks every file is covered.

## Code shape

- `src/lib.rs` + modules (`flint_file`, `checker`, `template`,
  `format`, `entry_kind`, `origin`, `scope`, `run`, `memory_tree`,
  `output`) hold
  **pure** logic: string/data in, string/data out, no filesystem access.
  This is what unit tests exercise directly. `run` drives a whole run — finding
  `.flint` files, walking the directories they govern, checking them —
  but reaches the tree only through its `DirectoryTree` trait;
  `memory_tree` implements that trait in memory, for unit tests, the
  fuzz target and the playground.
- `src/main.rs` is the thin I/O layer: implements `DirectoryTree` over
  the real disk, calls `run`, prints, sets the exit code. Keep new I/O
  here, not in the library. [`clippy.toml`](clippy.toml) enforces this:
  `std::fs`, `Path`'s disk queries, `std::process`, `print!`/`println!`
  and friends fail `just clippy` anywhere but in a file that opts out
  with an `#![allow(...)]` saying why (`main.rs`, the tests and benches
  that need real I/O).
- See [`docs/architecture.md`](docs/architecture.md) for how these
  modules depend on each other, a diagram of `main`'s request flow, and
  which test suites/benchmarks cover which part.
- When adding a feature, prefer extending a pure function and adding unit
  tests for it over adding untested logic to `main.rs`.

## Testing split (don't blur this)

- **Unit tests** (`#[cfg(test)] mod tests` inline, standard Rust
  convention): exercise the pure logic in isolation — parsing edge cases,
  comparison logic. Fast, no process spawning, no filesystem.
- **End-to-end tests** (`e2e/tests/*.robot`, Robot Framework): exercise the
  compiled binary as a black box — real temp directories, real files, real
  process invocation, asserting on stdout and exit code. This is what
  proves the tool actually works from a user's perspective, independent of
  implementation language.
- New behavior should get both: a unit test for the logic, an e2e test for
  the observable CLI behavior.
- **Playground examples** (`playground/examples/*.md`, see
  [`playground/README.md`](playground/README.md)) double as tests: `just
  test` runs flint over each one and fails if its documented output is
  wrong. When a change alters what an example prints, update the example
  — after checking the new output is really what it should show.
- **Coverage** of both suites is measured with cargo-llvm-cov and posted
  on each PR (see [`docs/coverage.md`](docs/coverage.md)); `just coverage`
  shows it locally, including which lines no test reaches. A drop is
  marked 🔴 in the PR comment — informational, not a gate, but look at it.
- **Fuzzing** (`fuzz/`, see [`docs/fuzzing.md`](docs/fuzzing.md)) only
  looks for crashes and hangs, and runs separately from `just ci`. When
  fixing a crash it found, also add the crashing input to
  `fuzz/seeds/check/`. The fuzz target runs the same `run` as the CLI,
  over an in-memory tree. Don't add Rust-level `assert_cmd`-style
  integration tests — that's what the Robot Framework suite is for, and we
  don't want two competing black-box test styles. The two files under
  `tests/` aren't either: `robot.rs` only runs the Robot suite from
  `cargo test` so mutation testing can see it, and `doc_links.rs` checks
  the docs' links into source files by line range.
- **Mutation testing** (`just mutants`, weekly in CI; `just mutants-diff`
  for just your changes, which each PR also gets as a comment; see
  [`docs/mutation-testing.md`](docs/mutation-testing.md)) checks the tests
  would notice each small change to the code. When it reports a missed
  mutant, add a test that catches it, or, if the mutant can't change
  behavior, exclude it in `.cargo/mutants.toml` with a comment saying why.

## Linting is not optional

`just lint` (fmt-check + clippy with `clippy::all` + `clippy::pedantic` at
`-D warnings` + msrv + `cargo-deny` + cargo-machete + actionlint +
zizmor + shellcheck + ruff + biome + seo + robocop + markdownlint + links +
typos + editorconfig + reuse + flint + docs-check + usage-check) must
be clean before work is considered done. Don't silence a clippy lint with
`#[allow(...)]` (or a Biome one with `biome-ignore`) without a comment
explaining why it's a false positive here — if it's a real tradeoff,
prefer fixing the code.

`unsafe` code is forbidden crate-wide (`[lints.rust]` in
[`Cargo.toml`](Cargo.toml)). Don't lift that to make something compile
— find a safe approach, or raise it as an explicit decision.

Code outside tests mustn't panic on its input: `unwrap()`, `panic!`,
`todo!`, `unimplemented!` and `[]` indexing or slicing all fail clippy
(selected `restriction` lints, in each crate's `Cargo.toml`). Return an
error, or use a form that can't panic (`.get()`, `split_at_checked`,
`strip_prefix`). Where an invariant really does rule the failure out,
say which with `.expect("why it holds")` or a scoped
`#[expect(clippy::indexing_slicing, reason = "...")]`. Unit tests are
exempt. [`clippy.toml`](clippy.toml) also caps a function's cognitive
complexity at 20 and its length at 100 lines — split a function that
goes over rather than raising the cap.

Every item needs a doc comment — private ones too, not just the public
API (`missing_docs` and `clippy::missing_docs_in_private_items`, in each
crate's `Cargo.toml`). Code under `#[cfg(test)]` is exempt. Say what the
item is or does for its caller, not how it's written.

In Claude Code, a `Stop` hook ([`.claude/settings.json`](.claude/settings.json),
[`.claude/hooks/stop-lint.sh`](.claude/hooks/stop-lint.sh)) runs `just
lint-fast` — formatting, clippy, markdownlint and typos — whenever the
agent tries to finish its turn, and sends any failure back to fix first.
After three blocked attempts in a row it lets the turn end, so a check
that can't pass there doesn't loop forever. It's a floor, not the bar:
the rest of `just lint` still applies.

`just ci` reproduces exactly what GitHub Actions runs
(`.github/workflows/ci.yml`); run it before considering a change finished.

## Process

For branching, committing, and opening a PR, follow
[`CONTRIBUTING.md`](CONTRIBUTING.md) — it applies to any contributor,
agent or human. Beyond that general workflow, step-by-step conventions
specific to working on flint live under [`docs/agent/`](docs/agent/):

- [`docs/agent/adding-a-feature.md`](docs/agent/adding-a-feature.md) —
  the implement → test → docs → changelog → PR sequence for any behavior
  change.
- [`docs/agent/release.md`](docs/agent/release.md) — semver policy and
  the version-bump checklist.

Claude Code users can also trigger the matching skills (`/add-feature`,
`/release`), which just redirect to these files.

## Scope discipline

flint recurses through subdirectories looking for `.flint` files and
checks each one against its own directory, composing with any ancestor
`.flint` file's rule that reaches into it (symlinks never followed — see
`docs/flint-file-format.md`'s "Multiple `.flint` files" section), and it
looks inside every folder a rule permits; `.git`/`target`/etc. aren't
auto-ignored, and there's no config beyond `.flint` itself.
Don't add speculative flags or config-file features "while you're in
there" — each capability should come from an explicit request or plan,
get its own docs update, and its own changelog entry.
