# Test coverage

flint measures **line coverage** for both test suites with
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), which uses
the Rust compiler's own instrumentation (`-C instrument-coverage`).
CI posts it on every PR that touches code or tests, compared with the
PR's base commit, publishes `main`'s to
<https://flint.viboko.dev/coverage/>, and `just coverage` shows the same
numbers locally.

## What's measured

Coverage is recorded by the compiled code itself, not by the test
framework. So the Robot Framework suite counts too, even though it only
sees flint as a black box: it runs an instrumented `flint` binary, and
every run writes down which lines it reached. Three numbers come out:

- **Unit tests**: lines reached by `cargo test`. `main.rs` is at 0%
  here by design — it's the I/O layer, and only the end-to-end suite
  exercises it (see [`AGENTS.md`](../AGENTS.md)'s testing split).
- **End-to-end tests**: lines reached by the Robot Framework suite
  running the binary.
- **Combined**: lines reached by either. This is the number that says
  how much of flint no test exercises at all.

Each file's trailing `#[cfg(test)]` module is left out of the numbers:
those lines are the unit tests themselves, which count as covered just
by running. (This is the same convention
[`bench-gate.sh`](../.github/scripts/bench-gate.sh) relies on: the test
module is the last item in each file.) The HTML report still shows them.

Benchmarks and the fuzz target aren't measured: they're not tests of
behavior.

Coverage only says a line *ran*, not that any test checks what it does.
[Mutation testing](mutation-testing.md) covers that.

## Running locally

Needs cargo-llvm-cov and the `llvm-tools-preview` rustup component —
`just install-tools` installs both, cargo-llvm-cov at the version
[`mise.toml`](../mise.toml) pins (see
[Tool versions](tooling.md#tool-versions)). Then:

```bash
just coverage
```

This builds an instrumented copy of flint under `target/coverage/` (so
the normal build in `target/debug/` is left alone), runs the unit tests
and the e2e suite against it, prints a per-file table of all three
numbers, and opens the combined HTML report, which shows every line's
hit count. `just coverage-collect` does the same minus the printing and
opening — it's what CI runs.

Everything it writes is under `target/coverage/report/`:

| File | What it is |
|---|---|
| `unit.lcov`, `e2e.lcov`, `combined.lcov` | Each suite's raw line coverage, in [lcov](https://github.com/linux-test-project/lcov) format — editor plugins such as VS Code's Coverage Gutters can show it inline. |
| `summary.json` | The same, reduced by [`coverage-report.py`](../.github/scripts/coverage-report.py) to each file's covered and uncovered lines, test modules removed. What the PR comment is built from. |
| `unit/html/`, `e2e/html/`, `combined/html/` | Each suite's HTML report, showing every line's hit count. `just coverage` opens `combined/html/index.html`. |

## CI

Three workflows. The PR pair is split exactly like the benchmark ones
(see "CI" in [`benchmarking.md`](benchmarking.md#ci) for why), so that
PRs from forks are handled safely:

- **[`coverage.yml`](../.github/workflows/coverage.yml)** runs on every
  PR. A `gate` job skips PRs that can't change coverage — anything not
  touching `src/`, `e2e/`, `Cargo.toml`, `Cargo.lock`, the `justfile` or
  the coverage tooling itself. Otherwise the `coverage` job runs
  `just coverage-collect` twice: on the PR's base commit, checked out in
  a separate worktree but measured with *the PR's* recipe and script,
  so both sides are measured the same way even when the PR changes how;
  then on the PR. `coverage-report.py --compare` turns the two
  summaries into the comment, also writing it to the run's step
  summary, and the job uploads it as the `coverage-result` artifact,
  along with the PR's HTML reports (assembled by `coverage-report.py
  --site`).
- **[`coverage-publish.yml`](../.github/workflows/coverage-publish.yml)**
  runs when `coverage.yml` finishes, from `main`, with a write token. It
  checks the artifact's PR number belongs to a PR whose head is the
  commit that was measured, publishes the HTML reports to
  `/pr-<N>/coverage/` on the [GitHub Pages site](github-pages.md),
  fills the comment's report links in with their URLs, then posts the
  comment, or updates its earlier one (via
  [`pr-comment.sh`](../.github/scripts/pr-comment.sh), shared with the
  benchmarks). If that fails it posts a "couldn't publish" notice
  instead, since `workflow_run` runs don't show up in a PR's checks.
- **[`coverage-main.yml`](../.github/workflows/coverage-main.yml)**
  measures `main` on each push that could change its coverage, and
  publishes the reports to `/coverage/` on the same site.

Only same-repo PRs get their reports hosted: a fork's PR runs its own
recipe, which writes the report pages, so hosting them would let anyone
publish arbitrary HTML on this repo's Pages domain. A fork's report
links point at the workflow run instead, as do everyone's until GitHub
Pages is enabled. See [`github-pages.md`](github-pages.md).

A consequence of the split: changes to `coverage-publish.yml` or the
scripts it runs only take effect once merged to `main` — including on
the PR that adds them, which gets no comment.

### Reading the comment

- A headline: **🔴 Coverage dropped** (naming which suites) if any
  suite's total dropped at all, otherwise **No drop in coverage**.
  Coverage is deterministic, so unlike the benchmarks there's no noise
  threshold: even a drop too small to show at one decimal place is
  real, and marked.
- **Lines this PR adds or changes under `src/`**, and how many of them
  either suite covers. Only lines the compiler instruments count —
  blank lines, comments and most declarations aren't code that runs.
  Any that no test reaches are listed, linked to the line.
- Each suite's total on the base commit and on the PR, 🔴 for a drop
  and 🟢 for a rise.
- Per file, the same three numbers — first for just the files whose
  coverage changed, then (collapsed) for every file.

Each of the PR's percentages links to that suite's HTML report — for a
file, its page, which highlights the lines that didn't run; for a
suite's total, the report's index. The base commit's numbers aren't
linked: only the PR's reports are kept.

If the base commit couldn't be measured (e.g. the PR restructures the
test suites so its recipe no longer works on the old code), the comment
shows the PR's numbers alone and the run has a warning saying why.

Like the benchmarks, the coverage check is **informational, not a
gate**: a drop never fails it — only tests failing on the PR do — and
it shouldn't be made a required status check. Not every line is worth
a test (e.g. an error path for a filesystem failure that's hard to
provoke), so a drop is a prompt to look, not a rule.

Nothing is persisted beyond each PR's comment, its `/pr-<N>/coverage/`
reports (deleted when it closes), the 14-day artifacts and `main`'s
latest reports at `/coverage/` — no history of past numbers.
