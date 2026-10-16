# Benchmarking flint

This is dev tooling for catching accidental performance regressions, not
a behavior change — it doesn't need the `README.md`/`docs/usage.md`/
`CHANGELOG.md` updates described in [`AGENTS.md`](../AGENTS.md)'s
docs-honesty rule. If a change here ever *does* change flint's observable
behavior, that change needs those updates as normal.

## What's measured, and what isn't

flint is explicitly non-recursive today (see `AGENTS.md`'s "Scope
discipline"), so the benchmarks only exercise the scaling dimensions that
actually exist: number of directory entries, and number of `.flint`
allow-list lines (literal or `{{ variable }}` template). Depth/fan-out
axes would be speculative until recursion is an actual feature — add them
then, not now.

There are two tiers, both under `benches/`. They measure different
things with different tools, and CI treats them differently (see "CI"
below):

- **`benches/logic.rs`** — instruction counts, with
  [Gungraun](https://github.com/gungraun/gungraun). Calls
  `flint_file::parse` and `checker::check` directly with synthetic
  in-memory data, under [Valgrind](https://valgrind.org/), which counts
  the CPU instructions each call executes. Each benchmark runs once, and
  an unchanged build gives nearly the same count on every run, on any
  machine — so a PR that raises a count by more than 1% fails its check.
  This is where flint's actual O(n) logic lives, so it's the primary
  regression signal. What it can't see is time itself: a change can
  execute the same number of instructions and still be slower (by
  missing the CPU's cache more often, say), and nothing the operating
  system does on flint's behalf is counted. Valgrind doesn't run on
  macOS, so there these need Docker (see "Running locally").
- **`benches/e2e.rs`** — wall-clock time, with
  [Criterion](https://github.com/bheisler/criterion.rs). Spawns the real
  compiled `flint` binary (`env!("CARGO_BIN_EXE_flint")`, not the
  `assert_cmd` crate — this is a benchmark, not a Rust-level integration
  test, so it doesn't blur `AGENTS.md`'s testing-split rule) against
  real generated directories of 100 / 1,000 / 10,000 files. This exists
  because in-memory benchmarks alone can't prove the actual disk-reading
  path is efficient — a regression there (an accidental per-file
  `stat()` call, non-linear scaling, etc.) would be invisible to
  `benches/logic.rs`, and most of its cost is inside the operating
  system, where Valgrind can't count. Timings on a shared CI runner are
  noisy, so this tier is reported on each PR but never fails it.

Deliberately **not** built: an automated Big-O classifier. Fitting a
runtime series to a complexity class is a known but noisy technique —
useful for a human eyeballing a graph, unreliable as an automated CI gate
on shared-runner data. Use the graph (below) for that judgment call.

## What each benchmark covers

flint works in two steps: read the `.flint` file into a list of rules
(`flint_file::parse`), then compare that list against what's actually in
the directory (`checker::check`). The benchmarks below cover those two
steps in isolation, plus the whole program end to end. Every one is
run at sizes 100, 1,000 and 10,000 — what that number counts is noted
in each description.

The first five are the instruction-count benchmarks, named as Gungraun
prints them (after its `logic::logic::` prefix, and with the size in
place of `<n>`).

### `parse` `literal_<n>`

Reads a `.flint` file of plain names (`file_0.txt`, `file_1.txt`, …)
into rules. No directory is involved. Size = number of lines.

### `parse` `template_<n>`

Reads a `.flint` file where every line is a `{{ variable }}` pattern
(`{{ name_0 }}_thing.txt`, …). Size = number of lines. Patterns take
much more work to compile than plain names, so this executes roughly
80× as many instructions as the literal case.

### `check_literal_all_matching` `n_<n>`

Checks an in-memory directory listing against plain-name rules, where
every file is allowed. Size = number of rules, with the same number of
files. This is the everyday case, and measures how fast flint answers
"is this name on the list?". The lookup is a hash set whose seed is
random per run, so this count is the one that varies between runs of
the same build, by up to about 0.2%.

### `check_scaling_template_count` `n_<n>`

Checks a fixed 200 files against a growing number of patterns. Size =
number of patterns. Every pattern is tried against every file, so this
isolates how cost grows as patterns are added.

### `check_scaling_file_count` `n_<n>`

The mirror image: a fixed 20 patterns against a growing number of files.
Size = number of files. Isolates how cost grows as the directory gets
bigger.

### `flint binary (real directory)`

The wall-clock benchmark. This heading must match its Criterion ID
exactly: `.github/scripts/bench-report.py` reads this section to let
readers expand a benchmark's description in the PR comment, and flags
any Criterion benchmark without one. Add a heading here whenever you
add one.

Runs the real compiled `flint` program, exactly as a user would, on a
real temporary folder of empty files with a `.flint` file listing each
by name. Size = number of files. Covers what the in-memory benchmarks
skip — process startup, reading the folder from disk, reading the
`.flint` file — so it's the slowest and noisiest of the six.

## Running locally

### Instruction counts

```bash
just bench-logic          # Linux
just bench-logic-docker   # macOS, or anywhere else with Docker
```

`bench-logic` needs Valgrind, and `gungraun-runner` at exactly the
version of the `gungraun` crate in `Cargo.lock` — the recipe checks, and
prints the `cargo install` command if it's missing or out of date.
`bench-logic-docker` runs the same thing in a Linux container that it
builds with both installed; its build output and saved baselines live
in the `flint-bench-logic` Docker volume rather than `target/`. A run
takes a minute or two, nearly all of it in the two template benchmarks
at size 10,000.

To compare your branch against `main`:

```bash
git checkout main
just bench-logic -- --save-baseline=main
git checkout <your-branch>
just bench-logic -- --baseline=main
```

Gungraun prints each benchmark's counts beside the baseline's, with the
change as a percentage. `Instructions` is the figure CI checks. The
others come from Valgrind's simulation of a CPU cache: `Estimated
Cycles` weights cache misses to approximate time, and is worth a look
when a change moves data around without changing how much work is done.
To fail on a regression the way CI does, add
`--callgrind-limits='ir=1%'`.

Counts are only comparable between runs in the same environment: a
different Rust, Valgrind or C library version changes them, so a
baseline saved by `bench-logic` means nothing to `bench-logic-docker`,
or after the Rust version in
[`rust-toolchain.toml`](../rust-toolchain.toml) changes.

### Wall-clock time

```bash
just bench
```

To compare two points in history (e.g. your branch against `main`):

```bash
git checkout main
just bench -- --save-baseline main
git checkout <your-branch>
just bench -- --baseline main
```

`--baseline <name>` (unlike `--save-baseline`, see the CI section below)
compares against that named baseline directly without overwriting it, so
this two-step sequence already gives you a real, complete comparison —
Criterion prints a percentage change per benchmark, using its own
statistical model (bootstrapped confidence intervals) to distinguish a
real change from run-to-run noise, and `target/criterion/report/index.html`
renders the same comparison as an HTML report (see "How to read the
wall-clock report" below).

Neither recipe is part of `just ci`, `just lint`, or `just test-all`:
the wall-clock benchmark is slower and noisier than the checks that
gate merges, and the instruction counts can't run everywhere `just ci`
does. CI runs both separately — see "CI" below.

## How to read the wall-clock report

**The HTML report** (`target/criterion/report/index.html`, or
per-benchmark under `target/criterion/<group>/<id>/report/index.html`):

- The violin plot shows the distribution of sample timings — a tighter,
  more symmetric shape means a cleaner measurement; a wide or
  multi-lobed shape means more noise (background load, thermal
  throttling, etc.) and the number should be trusted less.
- Once a real comparison is in play (a `--baseline` run, or CI's
  regenerated comparison — see below), the report adds a "Change: +X%"
  or "-X%" line and a regression/improvement line plot overlaying the
  new run against the baseline. Criterion only calls this out as an
  actual regression/improvement (as opposed to "Change within noise
  threshold") when the statistical test clears its significance
  threshold — a small percentage change with that caveat is expected
  noise, not a real signal.

**The PR comment** (built by `.github/scripts/bench-report.py`): one
row per benchmark, one column per size, each cell the percentage change
in mean time from the PR's base to the PR. The marks are Criterion's own
verdict, read from the same report page the number links to:

- 🔴 **regressed** / 🟢 **improved** — the change passed Criterion's
  statistical test *and* its whole confidence interval cleared the
  noise threshold ("Performance has regressed/improved"). CI raises
  that threshold from Criterion's default ±1% to ±10%
  (`NOISE_THRESHOLD` in `.github/workflows/bench.yml`), for the reason
  below.
- no mark — "No change in performance detected" or "Change within noise
  threshold". The number shown is Criterion's best estimate, so it can
  be past the threshold and still unmarked, when the interval reaches
  back inside it (e.g. +12.3%, with an interval of +8.9% to +16.3%).
  Hover over any number to see its interval and Criterion's verdict.
- `new` — nothing on the base branch to compare against (e.g. a
  benchmark this PR adds).

Shared GitHub runners vary by several percent between identical runs.
Rebuilding also shifts benchmarks whose code didn't change: editing one
module can move where unrelated code lands in memory, which can speed
it up or slow it down by a few percent, consistently. (The first PR to
run this saw `flint_file::parse` 7–9% slower at every size from a
change to `checker.rs` alone.) At ±1% that marked 13 of 18 cells 🔴;
±10% left exactly the 5 real regressions. The cost is that a real
slowdown under 10% isn't marked — its number is still shown, so a
consistent few-percent change across sizes is still worth a look. A
change that grows with size (as a regression from O(n) to O(n²)
would) is a real signal; re-running the workflow settles a borderline
one. Click a benchmark's name for what
it covers (from "What each benchmark covers" above), or a number for
its full Criterion page.

## Where baseline data lives

`--save-baseline <name>` / `--baseline <name>` write to
`target/criterion/<group>/<benchmark-id>/<baseline-name>/` (`estimates.json`,
`sample.json`, etc.) — inside `target/`, so it's gitignored. This is why
the local before/after workflow above works: a plain `git checkout`
never touches gitignored/untracked paths (it only touches files that
differ between the two commits' trees), so `target/criterion` survives a
branch switch on its own. It does **not** survive `cargo clean` or
`git clean` — those explicitly remove untracked files, including it.

Gungraun's `--save-baseline=<name>` / `--baseline=<name>` work the same
way, under `target/gungraun/` — or, for `just bench-logic-docker`, in
the `flint-bench-logic` Docker volume (`docker volume rm
flint-bench-logic` clears it).

## CI

### Instruction counts: a gate

**`.github/workflows/bench-logic.yml`** runs on every PR. Its `gate`
job runs `.github/scripts/bench-gate.sh` (described below) over `src/`,
`Cargo.toml`, `Cargo.lock`, `benches/logic.rs` and the workflow itself,
and skips the rest if none of those changed beyond unit tests.
Otherwise the `instruction-counts` job installs Valgrind and
`gungraun-runner`, runs `just bench-logic` on the PR's base commit
(`--save-baseline=base`), then on the PR (`--baseline=base`) with a
limit of 1% on each benchmark's instruction count (`INSTRUCTION_LIMIT`
in the workflow). Any benchmark over the limit fails the job, and the
job's log names it. Both runs use the same runner and toolchain, so
the counts are comparable.

Why 1%: an unchanged build's counts vary by up to about 0.2%, all of it
in `check_literal_all_matching`, whose hash set is seeded randomly on
each run; the other benchmarks vary by a few instructions in millions.
1% clears that comfortably. A change can also shift counts in code it
didn't touch, by changing what the compiler inlines — rarely by this
much, but if the check fails on code the PR doesn't go near, that's the
likely cause. There is no way to override the check from within a PR
except by editing the limit, which shows up in the diff: if a slowdown
is the price of a change worth making, raise the limit in that PR, say
why, and lower it again afterwards.

The job is skipped, not failed, on a PR that can't have changed
performance, and GitHub counts a skipped job as passing. That's what
makes it usable as a **required status check**, which is how it should
be set up (`instruction-counts`, in the ruleset for `main`). It passes
without comparing anything when the base commit's benchmarks don't run
— a PR that updates Gungraun, for instance, since the runner installed
is the PR's version. The job's log and a warning annotation say so.

### Wall-clock time: a report

Two workflows for PRs, split so that PRs from forks are handled
safely, plus one for `main`:

- **`.github/workflows/bench.yml`** runs on every PR that touches
  `src/` or `Cargo.lock` (a dependency update can change performance
  too). A `gate` job runs `.github/scripts/bench-gate.sh`, which
  compares each changed file with its trailing `#[cfg(test)]` module
  removed, and skips benchmarking if the only `src/` changes were unit
  tests — those can't change performance. Otherwise the `bench` job runs
  `just bench` on the PR's base commit (`--save-baseline base`), then on
  the PR (`--save-baseline pr`), on the same runner. Since
  `--save-baseline <name>` only ever compares against a previous run
  saved under the *same* name, a final pass reloads both
  (`--load-baseline pr --baseline base`, no re-measurement) to rewrite
  the HTML reports as a genuine base-vs-PR comparison.
  `.github/scripts/bench-report.py` turns that into the comment
  described above, and the job uploads the comment plus the HTML
  reports as the `bench-result` artifact. It also writes the table to
  the run's step summary.
- **`.github/workflows/bench-publish.yml`** runs when `bench.yml`
  finishes. It publishes the reports to `/pr-<N>/benchmarks/` on the
  [GitHub Pages site](github-pages.md), fills the comment's links in
  with their URLs, and posts the comment, or updates its earlier one
  (via `.github/scripts/pr-comment.sh`). When the PR closes,
  `pages-cleanup.yml` deletes `/pr-<N>/`. Its runs don't show up in the
  PR's checks (no `workflow_run` run does), so if publishing fails it
  replaces the comment with a "couldn't publish results" notice linking
  to the failed run — otherwise the failure would only be visible in
  the Actions tab. It can't do this for a fork's PR whose artifact it
  couldn't validate: GitHub doesn't say which PR such a run was for.

Why two workflows: a PR's workflows run that PR's code, and for a PR
from a fork GitHub only grants a read-only token, so `bench.yml` can't
comment or push. `bench-publish.yml` is triggered by `workflow_run`,
which always runs the workflow as it is on `main` with a write token —
safe only because it never executes PR code. It treats the artifact as untrusted: it checks that the PR number in it
belongs to a PR whose head is the commit that was benchmarked, and only
hosts reports on Pages for same-repo PRs (a fork's benchmark code also
writes its report pages, so hosting those would let anyone publish
arbitrary HTML under this repo's Pages domain). A fork's PR still gets
the comment, with links to the workflow run instead. A consequence:
changes to `bench-publish.yml` or the scripts it uses
(`gh-pages.sh`) only take effect once merged to `main`.

A third workflow, **`.github/workflows/bench-main.yml`**, benchmarks
`main` on each push that changes implementation code and publishes the
reports to `/benchmarks/`, with no comparison.

GitHub Pages must be enabled once — see "Setup" in
[`github-pages.md`](github-pages.md). Until it is, the comment's links
point at the workflow run, whose `bench-result` artifact holds the
reports.

This check is **informational, not a gate**: don't make it a
required status check. A slowdown never fails it — only a benchmark
that fails to build or run on the PR does. A base commit whose
benchmarks fail (e.g. this PR restructures the suite) is tolerated, and
the comment shows `new` instead of a comparison.

Nothing is persisted beyond each PR's comment, its `/pr-<N>/benchmarks/`
reports (removed on close), the 14-day artifact and `main`'s latest
reports at `/benchmarks/` — no historical dashboard, no timing data
committed to the repo.
