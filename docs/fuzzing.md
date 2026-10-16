# Fuzzing

flint is often run in CI against repositories (and pull requests) its
user didn't write, so both the `.flint` file and the directory it checks
are untrusted input (see [`SECURITY.md`](../SECURITY.md)). Fuzzing feeds
flint huge numbers of generated inputs, looking for any that make it
crash, hang, or use runaway memory.

It uses [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html),
which drives LLVM's [libFuzzer](https://llvm.org/docs/LibFuzzer.html).
libFuzzer is *coverage-guided*: it watches which branches of flint's code
each input reaches, keeps any input that reaches something new, and
mutates those to find more. Over time that explores far more of the
parser and checker than random input ever would.

Fuzzing only looks for crashes, hangs and memory blow-ups. It can't tell
whether flint gave the *right* lint result — that's what the unit and
end-to-end tests are for.

## What's fuzzed

One fuzz target, [`fuzz/fuzz_targets/check.rs`](../fuzz/fuzz_targets/check.rs).
It builds an in-memory directory tree from the input and runs the same
`flint::run::run` the CLI runs over a real disk — find the `.flint`
files, parse them, walk the directories they govern and check them —
then renders the output the CLI would print.

Each input is a `.flint` file, then a line that is exactly `---`, then
the directory listing — one `/`-separated relative path per line, with a
trailing `/` marking a directory:

```text
src/
  !{{ name }}.rs
---
src/main.rs
src/lib/
notes.txt
```

With no `---` line, the whole input is the `.flint` file and the listing
is empty. Listing lines a real directory couldn't produce (empty
components, `.` or `..`) are skipped, and every listed path's parent
directories are added automatically. A listed `.flint` file below the
root is a nested `.flint` file with no rules in it, so how several
`.flint` files in one tree combine (see "Multiple `.flint` files" in
[`flint-file-format.md`](flint-file-format.md)) is fuzzed too, if only
with empty ones below the root.

## Seeds and corpus

Two directories of inputs, both committed:

- **[`fuzz/seeds/check/`](../fuzz/seeds/check/)** — hand-written,
  readable starting points, one per documented `.flint` feature and
  parse error (see [`flint-file-format.md`](flint-file-format.md)'s
  "Examples" and "Parse errors" tables). They give the fuzzer a head
  start into the interesting parts of the parser. Tools never change
  these files.
- **[`fuzz/corpus/check/`](../fuzz/corpus/check/)** — machine-managed.
  Every input the fuzzer has found that reaches code no earlier input
  did, named by a hash of its contents. Each fuzzing run starts from it,
  so runs build on each other instead of rediscovering the same ground.
  After each run it's *minimized*: inputs whose coverage the others
  already reach are dropped, so it doesn't grow without bound.

## Running locally

Needs a nightly toolchain alongside the pinned one, and cargo-fuzz —
`just install-tools` installs both. That's also why neither recipe is
part of `just ci`/`lint`/`test-all`: nothing else in the project needs
nightly, so the replay runs as its own CI job instead (see [CI](#ci)).

| Recipe | What it does |
|---|---|
| `just fuzz-replay` | Runs every seed and corpus input through the target once, generating nothing new. Takes seconds. This is the PR check. |
| `just fuzz [minutes]` | Fuzzes for `minutes` (default 5), adding new inputs to `fuzz/corpus/check/`, then minimizes it and prints a summary: how big the corpus got, how it differs from the committed one, and a few of the new inputs. |

`just fuzz` writes each new input to the corpus the moment it's found, so
you can stop it early with Ctrl-C without losing anything — it still
minimizes and summarizes afterwards. (`just` itself then reports
"interrupted by SIGINT"; that's expected.)

While it runs, libFuzzer prints a status line per event, e.g.

```text
#48213	NEW    cov: 6120 ft: 21904 corp: 812/31Kb lim: 116 exec/s: 4821 rss: 87Mb
```

`cov` is how many code edges have been reached, `ft` a finer-grained
count of distinct behaviours, `corp` the corpus size so far, and
`exec/s` how many inputs per second it's trying. `NEW` lines mean an
input reached something new and was kept; `REDUCE` means a shorter input
was found that reaches the same thing. Expect lots of `NEW` lines at
first, then fewer as the easy ground is covered.

Growth to the corpus is ordinary data: commit it like any other change,
or leave it to the weekly run below.

## If the fuzzer finds a crash

The fuzzer stops, prints the panic or sanitizer report, and saves the
input that caused it under `fuzz/artifacts/check/` (gitignored).
Reproduce it with:

```bash
cargo +nightly fuzz run --fuzz-dir fuzz check fuzz/artifacts/check/<file>
```

Then fix the bug like any other (with a unit test for it, per
[`AGENTS.md`](../AGENTS.md)), and copy the crashing input into
`fuzz/seeds/check/` under a descriptive name. Seeds are never minimized
away, so it stays a permanent regression check — the corpus alone
wouldn't guarantee that, since once the bug is fixed, the input may no
longer reach anything the corpus doesn't already.

## CI

Two workflows:

- **`replay`**, in
  [`.github/workflows/fuzz.yml`](../.github/workflows/fuzz.yml) — on
  every PR and push to `main` that touches the code, the fuzz harness,
  or the workflow: `just fuzz-replay`. Deterministic,
  so a PR only fails it if it makes one of the committed inputs crash.
  It's a required status check, so the workflow starts on every PR and
  a `gate` job skips `replay` (which counts as passing) when the PR
  touches none of those.
- **`extend`**, in
  [`.github/workflows/fuzz-extend.yml`](../.github/workflows/fuzz-extend.yml)
  — weekly, or on demand from the Actions tab (with a custom
  duration): fuzzes `main` for 30 minutes via `just fuzz 30`.
  - If that finds a crash, the job fails (GitHub emails a failure
    notice) and uploads the crashing input as the `fuzz-crash`
    artifact. Nothing is pushed.
  - Otherwise, if the corpus changed, it pushes the result to the
    `fuzz-corpus-update` branch and opens a PR (or updates the open one)
    as the bot app. Using the app's token rather than the workflow's own
    is what makes CI run on that PR. Each run starts from `main`'s corpus
    plus anything still on that branch, so an unmerged week's finds
    aren't lost.

The bot app's credentials are the `BOT_APP_CLIENT_ID` repository variable
and `BOT_APP_PRIVATE_KEY` secret; the app needs read/write access to
contents and pull requests.
