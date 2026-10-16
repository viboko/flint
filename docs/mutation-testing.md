# Mutation testing

[Test coverage](coverage.md) says which lines the tests *run*. Mutation
testing asks the harder question: would the tests *notice* if a line
were wrong? [cargo-mutants](https://mutants.rs) answers it by making
one small change ("mutant") to the code at a time: replacing a
function's body with a plausible return value, swapping `<` for `<=`
or `&&` for `||`, deleting a `!`. It then rebuilds and runs the tests.
A mutant the tests fail on is **caught**. One they still pass with is
**missed**: a change in behavior no test checks for.

It runs in two ways: over the whole codebase weekly (a few minutes),
and on each PR over only the code that PR changes (usually seconds),
posting the result as a comment. Locally, `just mutants` and
`just mutants-diff` do the same. Neither is part of `just ci`.

It isn't random: cargo-mutants applies a fixed set of rules to every
place in `src/` they fit, so the same code and tests always give the
same mutants and the same results. What changes them is new or changed
code (new mutants), new tests (newly caught ones), a cargo-mutants
upgrade (new rules), or `.cargo/mutants.toml`. Random *inputs* are what
[fuzzing](fuzzing.md) is for.

## Both test suites count

cargo-mutants judges each mutant by whether `cargo test` fails, and
can't run anything else. So [`tests/robot.rs`](../tests/robot.rs) runs
the Robot Framework suite from `cargo test` — but only when
`FLINT_ROBOT` names a `robot` executable, as `just mutants` sets it.
Otherwise it does nothing, so plain `cargo test` (and `just test`)
stays unit tests only. Without it, every mutant in code only the e2e
suite checks (`main.rs`, `output.rs`) would look untested.

It's a wrapper, not a second black-box test style: new e2e tests still
go in `e2e/tests/*.robot` (see [`AGENTS.md`](../AGENTS.md)).

## Running locally

Needs cargo-mutants (`just install-tools` installs it, at the version
[`mise.toml`](../mise.toml) pins — see
[Tool versions](tooling.md#tool-versions)). Then:

```bash
just mutants
```

It copies the tree to a temporary directory, so your checkout is never
modified, and tests 4 mutants at a time (about 5 minutes on a recent
laptop). Results are in `target/mutants.out/`: `missed.txt`,
`caught.txt`, `timeout.txt` and `unviable.txt` (mutants that didn't
compile, which are ignored), plus each mutant's diff and test log under
`diff/` and `log/`. Extra arguments are passed to cargo-mutants, e.g.
to re-check one file or one mutant after adding a test:

```bash
just mutants --file src/scope.rs
```

```bash
just mutants --re 'rebase_literal_name'
```

`just mutants-diff` tests only the mutants in code changed since a base
(default `main`, from where this branch left it), uncommitted changes
included — what the PR check runs, so usually seconds rather than
minutes. It only looks at code under test: a change to test code, or a
comment, produces no mutants. Extra arguments go to cargo-mutants too:

```bash
just mutants-diff main --list
```

## Triaging a missed mutant

Each missed mutant is one of two things:

- **A real gap.** Add a test that fails with the mutation applied (look
  at its diff in `target/mutants.out/diff/`), then re-check it with
  `--re`. Put the test where [`AGENTS.md`](../AGENTS.md)'s testing split
  says it belongs: a unit test for pure logic, an e2e test for what the
  CLI prints. One commit per gap keeps the history readable.
- **An equivalent mutant**: one that can't change behavior. For example,
  `upper_run_candidates` tries every word grouping of an acronym, so
  inverting its bit test just tries the same groupings in a different
  order. No test can catch these, so they're excluded in
  [`.cargo/mutants.toml`](../.cargo/mutants.toml) by a regex on the
  mutant's exact name (`just mutants --list` shows the names). Each
  exclusion has a comment saying why it's equivalent. The names include
  line numbers, which is deliberate: when the code moves, the exclusion
  stops matching and the mutant reappears as missed, to be re-checked,
  rather than a looser pattern quietly hiding some new mutant.

Mutants that make the tests hang (usually an infinite loop from
`index += 1` becoming `-= 1`) are reported as timeouts. A hang is as
good as a failure, but each costs a full timeout per run, so persistent
ones are excluded the same way.

## CI

[`mutants.yml`](../.github/workflows/mutants.yml) runs `just mutants`
on `main` every Monday, and on demand from the Actions tab. Its `issue`
job keeps one **"Mutation testing: missed mutants"** issue in sync
(via [`mutants-issue.sh`](../.github/scripts/mutants-issue.sh)): opened,
or reopened, with the list of missed mutants (from
[`mutants-report.py`](../.github/scripts/mutants-report.py)) linked to
their lines, while any are missed, and closed once none are. The same
report is in the run's step summary, and the `mutants-out` artifact
holds every diff and log for 14 days.

It's informational: missed mutants never fail the run, only a broken
one does (e.g. the tests failing before any mutation). The write token
for the issue is only given to a separate job that runs after the
tests, never to the job that runs mutated code.

### On each PR

Two more workflows, split exactly like the coverage ones (see "CI" in
[`benchmarking.md`](benchmarking.md#ci) for why), so that PRs from forks
are handled safely:

- **[`mutants-pr.yml`](../.github/workflows/mutants-pr.yml)** runs
  `just mutants-diff` against the PR's base commit. It first lists the
  mutants in the PR's changes, which only parses the source; most PRs
  (docs, tests, CI) have none, and skip building and testing entirely.
  Otherwise it tests them, writes the result to the run's step summary,
  and uploads it as the `mutants-pr-result` artifact (plus each mutant's
  diff and log as `mutants-out`).
- **[`mutants-pr-publish.yml`](../.github/workflows/mutants-pr-publish.yml)**
  runs when that finishes, from `main`, with a write token. It checks
  the artifact belongs to the PR's current head, then posts the comment
  or updates its earlier one (via
  [`pr-comment.sh`](../.github/scripts/pr-comment.sh)). The comment
  lists any missed mutant 🔴, linked to its line, with how to triage
  it. A PR that no longer has any mutants gets its earlier comment
  replaced with a "skipped" notice, but never a new one.

Like the weekly run, it's informational: a missed mutant never fails
the check. It's a prompt to add a test — or an exclusion, if the mutant
can't change behavior — in the same PR, before the weekly run would
find it. The diff only covers the PR's own lines, so it can't notice a
change elsewhere leaving older code less well tested; the weekly run
still catches that.

A consequence of the split: changes to `mutants-pr-publish.yml` or the
scripts it runs only take effect once merged to `main` — including on
the PR that adds them, which gets no comment.
