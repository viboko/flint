# Tooling

flint uses [`just`](https://github.com/casey/just) as its task runner.
Run `just` (or `just --list`) to see every recipe with a one-line
description; this page explains what each one wraps and why, grouped the
same way the [`justfile`](../justfile) itself is.

`just --list` shows only the last line of the comment above a recipe, so
a recipe whose comment runs over more than one line also gets a
`[doc("...")]` attribute with a one-line summary. `just --list` shows
that instead, and the full comment stays in the file.

## Tool versions

Every tool the recipes run is pinned to an exact version, and CI
installs the same versions, so `just ci` gives the same answer locally
as in GitHub Actions, and a tool's new release can't fail a PR that
didn't change anything.

- **Rust** — [`rust-toolchain.toml`](../rust-toolchain.toml) names the
  toolchain (with rustfmt and clippy), and rustup uses it automatically
  for everything run in the repo. The exceptions name their toolchain
  explicitly: `msrv` (see below), and nightly for the fuzzing recipes
  and `test-minimal-versions`, which stays unpinned.
- **Everything else** — [`mise.toml`](../mise.toml) lists every other
  tool (`just`, the linters, the cargo subcommands) for
  [mise](https://mise.jdx.dev/) to install, and
  [`mise.lock`](../mise.lock) records each download's checksum. Each
  workflow installs the tools it needs through
  [`.github/actions/setup-tools`](../.github/actions/setup-tools/action.yml),
  which reads the same two files, and then runs the same `just` recipe
  you would.

The two exceptions pinned elsewhere are the Python packages, in
[`e2e/requirements.in`](../e2e/requirements.in), and
`wasm-bindgen-cli` and `gungraun-runner`, whose versions have to match
the crates in `Cargo.lock` and `playground/Cargo.lock`. `just pip-lock`
compiles `requirements.in` into
[`e2e/requirements.txt`](../e2e/requirements.txt), which pins every
transitive dependency too, with the hash of each file pip may
download; `just venv` installs it with `--require-hashes`, so a
tampered or swapped package fails to install. Dependabot keeps both
files up to date.

To set up: install [rustup](https://rustup.rs/) and
[mise](https://mise.jdx.dev/getting-started.html), [activate
mise](https://mise.jdx.dev/getting-started.html#activate-mise) in your
shell (so its versions come first on `PATH`), run `mise trust` once in
the repo, then `just install-tools`.

Dependabot can't update these files, so bump them deliberately:

- **A tool**: change its version in `mise.toml`, run `mise lock` to
  update `mise.lock`, then `just install-tools`.
- **Rust**: change `channel` in `rust-toolchain.toml`. A new Rust
  usually brings new clippy lints, so run `just lint` and fix (or
  justify allowing) whatever they find in the same change.

## Building and running

| Recipe | Wraps | Notes |
|---|---|---|
| `build` | `cargo build` | Debug binary, at `target/debug/flint`. |
| `build-release` | `cargo build --release` | Optimized binary, at `target/release/flint`. ([Installation](../README.md#installation) uses `cargo install --locked --path .` instead, which builds the same binary and copies it to `~/.cargo/bin`.) |
| `run *ARGS` | `cargo run --` | Build-and-run in one step, e.g. `just run -- --version`. |
| `clean` | `cargo clean` | Removes `target/` — which also holds the Robot Framework output and the benchmarks' saved baselines, since those are written there too. |

## Static checks

| Recipe | Wraps | Notes |
|---|---|---|
| `fmt` | `cargo fmt` | Rewrites files in place. |
| `fmt-check` | `cargo fmt --check` | Non-mutating; what CI runs. |
| `clippy` | `cargo clippy` | `clippy::all` + `clippy::pedantic`, `-D warnings`, over the main crate, `fuzz/` and `playground/`, plus `missing_docs` and `clippy::missing_docs_in_private_items`: every item outside `#[cfg(test)]` code needs a doc comment. Also selected `restriction` lints that ban code that can panic (`unwrap()`, `panic!`, `todo!`, `unimplemented!`, `[]` indexing and slicing) outside unit tests, and caps on a function's cognitive complexity (20) and length (100 lines). [`clippy.toml`](../clippy.toml) also bans file-system access, running or exiting processes, and printing, so the library stays pure — see [`AGENTS.md`](../AGENTS.md)'s "Code shape". |
| `msrv` | `cargo +<rust-version> check --locked` | Checks the library and binary still build on flint's minimum supported Rust version — the `rust-version` in [`Cargo.toml`](../Cargo.toml) — installing that toolchain with `rustup` first if it's missing. Only what `cargo install` builds is covered: dev-dependencies like Criterion may need a newer Rust, which is fine. See "Minimum supported Rust version" below. |
| `deny` | `cargo deny check` | Dependency licenses, security advisories, and duplicate/banned crates, per [`deny.toml`](../deny.toml) — for the CLI, and for the [playground](../playground/README.md), whose WebAssembly module (with its own dependencies) ships to visitors' browsers. The fuzz harness isn't checked: nothing from it ships. |
| `package-check` | `cargo package --list` + `cargo package` | Checks what `cargo publish` would upload to crates.io is exactly the files listed in the recipe, then builds that packaged copy on its own in `target/package/`. Which files go in is decided by `include` in [`Cargo.toml`](../Cargo.toml), an allow-list: the source, the README and the licenses — none of the docs, tests, CI, agent or tool config. The list in the recipe is a second copy, so a change to either fails until both agree, and the build catches a file the crate needs but `include` leaves out. Cargo drops the test and bench targets from the published manifest, since their files aren't included; its warnings about that are expected. |
| `machete` | [`cargo machete`](https://github.com/bnjbvr/cargo-machete) | Dependencies declared in `Cargo.toml`, `fuzz/Cargo.toml` or `playground/Cargo.toml` that the code never uses — each one is extra build time and supply-chain surface for nothing. It works by searching the source for each dependency's name as written in the manifest, so a dependency whose code is imported under another name should be renamed to match (as `fuzz/Cargo.toml` does with `flint = { package = "flint-cli", ... }`) rather than added to an ignore list. |
| `actionlint` | [`actionlint`](https://github.com/rhysd/actionlint) | Checks `.github/workflows/*.yml`: syntax, expression and context typos, action inputs, and — via `shellcheck` — the shell inside each `run:` step. Without `shellcheck` installed it silently skips that last part. |
| `zizmor` | [`zizmor`](https://docs.zizmor.sh/) | Security audit of `.github/workflows/*.yml` and `.github/dependabot.yml`: over-broad token permissions, actions not pinned to a commit SHA, untrusted values expanded into shell scripts, credentials left on disk after checkout, cache poisoning, dangerous triggers. Locally it runs only its offline audits unless `GH_TOKEN` is set; CI also runs the online ones (e.g. checking each pinned SHA really belongs to the action's repo). A finding that's a deliberate, justified exception is suppressed inline with a `# zizmor: ignore[<audit>]` comment explaining why. |
| `shellcheck` | [`shellcheck`](https://www.shellcheck.net/) on `.github/scripts/*.sh` and `.claude/hooks/*.sh` | Common shell bugs: unquoted variables, word splitting, ignored errors. Recipes in the `justfile` aren't covered — shellcheck can't read that format. |
| `ruff` | [`ruff`](https://docs.astral.sh/ruff/) `check` + `ruff format --check` | The repo's Python — the CI report scripts [`bench-report.py`](../.github/scripts/bench-report.py), [`coverage-report.py`](../.github/scripts/coverage-report.py) and [`mutants-report.py`](../.github/scripts/mutants-report.py). Lint rules (likely bugs, unused names, import order, outdated idioms) and formatting are both configured in [`ruff.toml`](../ruff.toml). `ruff format` and `ruff check --fix` fix most findings in place. |
| `biome` | [Biome](https://biomejs.dev/) `ci --error-on-warnings` | Lints and format-checks the [playground](../playground/README.md)'s JavaScript, CSS and HTML under `playground/web/`, and every SVG (the logo and the favicon): likely bugs, accessibility problems in the HTML and SVG (e.g. an SVG with no `<title>`), malformed markup, CSS rules that override each other in a confusing order, and formatting (two-space indents, 100 columns). It doesn't check SVG-specific meaning, such as whether path data is valid. Warnings fail it too. Config, including how it skips gitignored build output, is in [`biome.jsonc`](../biome.jsonc). `biome check --write .` fixes most findings in place; a deliberate exception is suppressed inline with a `biome-ignore` comment saying why. |
| `seo` | `grep` + Python's XML parser | The [playground](../playground/README.md) site's SEO basics: `robots.txt` and a well-formed `sitemap.xml` exist under `playground/web/`, and the sitemap, `robots.txt`'s `Sitemap:` line and every page's canonical link, `og:url` and `og:image` are absolute `https` URLs on the domain in `playground/web/CNAME` (and the `og:image` file exists). Checks the source files directly — the site is static, so they're served as they are. |
| `robocop` | [Robocop](https://robocop.dev/) `check` + `format --check` | Lints and format-checks the Robot Framework suite under `e2e/`: deprecated syntax, inconsistent naming and assignment style, over-long tests and lines. Installed into `.venv` by `venv` (so it needs Python 3.10+). Config, including the rules turned off and why, is in [`e2e/robocop.toml`](../e2e/robocop.toml). `.venv/bin/robocop format e2e` fixes formatting in place, and `.venv/bin/robocop check --fix e2e` many lint findings. |
| `markdownlint` | [`markdownlint-cli2`](https://github.com/DavidAnson/markdownlint-cli2) | Every tracked `*.md` file: heading structure, list and code-fence spacing, code fences without a language, broken table rows (e.g. an unescaped `\|` inside a table cell's code span, which GitHub reads as a column break). Rules and the file list are in [`.markdownlint-cli2.yaml`](../.markdownlint-cli2.yaml), with a comment on each rule that's turned off or tuned — notably line length, since prose is hand-wrapped but tables and links can't be. Many findings can be fixed in place with `markdownlint-cli2 --fix`. |
| `links` | [`lychee`](https://lychee.cli.rs/) `--offline` | Every link in the Markdown files that points within the repo: the target file exists, and a `#heading` fragment matches a real heading. Offline, so it's fast and can't fail because some site is down. It can't check line-range fragments into source files (`checker.rs#L10-L20`) — lychee only finds fragments in Markdown and HTML — so `test` checks those instead (see `fix-doc-links`). Config in [`lychee.toml`](../lychee.toml). |
| `links-online` | [`lychee`](https://lychee.cli.rs/) | Same, plus every external link. Not part of `lint`; [`links.yml`](../.github/workflows/links.yml) runs it weekly instead (see below). Set `GITHUB_TOKEN` locally to avoid github.com rate-limiting links to it. |
| `typos` | [`typos`](https://github.com/crate-ci/typos) | Spell check of everything tracked — code identifiers, comments, docs, workflows — tuned for few false positives: it only flags known misspellings rather than every word it doesn't recognize. `typos -w` applies its suggested fixes. A word it wrongly flags goes in [`_typos.toml`](../_typos.toml) under `[default.extend-words]`; that file also excludes the fuzzer-generated corpus, which is full of mangled words on purpose. |
| `reuse` | [`reuse`](https://reuse.software) `lint` | Every file states its copyright and license, per the [REUSE spec](https://reuse.software/spec/): an SPDX header at its top (see [`AGENTS.md`](../AGENTS.md#license-headers)), or, for files that can't carry a comment — Markdown, JSON, lockfiles, images, the fuzzer's inputs — a path pattern in [`REUSE.toml`](../REUSE.toml). Also checks every license named has its text in `LICENSES/`. Installed into `.venv` by `venv`, alongside Robocop. |
| `editorconfig` | [`editorconfig-checker`](https://github.com/editorconfig-checker/editorconfig-checker) `-disable-indent-size` | Every tracked file follows [`.editorconfig`](../.editorconfig): UTF-8, LF line endings, a final newline, no trailing whitespace, and spaces rather than tabs for indentation. `.editorconfig` also tells editors these rules, and the two-space indent the YAML, JSON and Biome-formatted files use. Indent *width* isn't checked — rustfmt, ruff, Biome and Robocop own it, and docstrings, raw strings and the license text legitimately indent by other rules. The fuzzer's corpus and seeds are exempt (their `.editorconfig` section unsets every rule), since they're malformed on purpose. |
| `flint` | the built `flint` binary, against this repo's own root | Dogfooding: builds first, then runs flint against the repo's own [`.flint`](../.flint). |
| `docs-check` | `cargo doc`, `RUSTDOCFLAGS="-D warnings"` | See "API docs" below. |
| `usage-check` | `flint --help` + `diff` | Builds flint and checks the `$ flint --help` block at the top of [`docs/usage.md`](usage.md) is exactly what `--help` prints, so a new or reworded argument can't leave it stale. When it fails, paste in the real help text (the `+` lines of its diff). |
| `lint` | all of the above, except `links-online` | Every static check together. |
| `lint-fast` | `fmt-check` + `clippy` + `markdownlint` + `typos` | The checks that take seconds once the build is cached — a quick pass while working. Claude Code runs it from a `Stop` hook (see [`AGENTS.md`](../AGENTS.md)'s "Linting is not optional"). Doesn't replace `lint`. |

### Minimum supported Rust version

flint's MSRV is the `rust-version` in [`Cargo.toml`](../Cargo.toml),
currently 1.85: the oldest Rust that supports edition 2024. Cargo
refuses to build flint on anything older with a clear error, rather
than failing on some newer language feature halfway through, and its
resolver prefers dependency versions that still support the MSRV.

Raising it is fine when a dependency or a language feature needs it, but
do it on purpose: bump `rust-version`, note it in
[`CHANGELOG.md`](../CHANGELOG.md), and treat it like any other
minor-version change. `just msrv` fails as soon as the code needs a
newer Rust, so it can't drift up unnoticed.

### CI-only security checks

Two more checks run only in GitHub Actions, with no `just` recipe: both
need GitHub's own infrastructure and report into the repository's
**Security** tab rather than failing a build. Both skip themselves while
the repository is private, since GitHub only offers code scanning for
free on public repositories.

| Workflow | What it does |
|---|---|
| [`codeql.yml`](../.github/workflows/codeql.yml) | [CodeQL](https://codeql.github.com/) static analysis for security bugs, on every push to `main`, every PR, and weekly. Covers the Rust code, the workflows themselves, and the Python scripts under `.github/scripts/`. |
| [`scorecard.yml`](../.github/workflows/scorecard.yml) | [OpenSSF Scorecard](https://scorecard.dev/): scores the repository's security practices as a whole — branch protection, token permissions, pinned dependencies, fuzzing, a security policy, and so on — on every push to `main` and weekly. |

### Weekly external link check

[`links.yml`](../.github/workflows/links.yml) runs `just links-online`'s
check every Monday (or on demand from the Actions tab): external links
can rot without anyone touching the repo, but checking them on every PR
would let an unrelated site outage block it. A failure here just means
a link needs updating or, if the site is gone for good, removing.

[Dependabot](../.github/dependabot.yml) also keeps Cargo dependencies
and the workflows' SHA-pinned actions up to date, waiting a week after
each release before proposing it (security updates aren't delayed).

### Release dry run

The release builds live in
[`release-build.yml`](../.github/workflows/release-build.yml), a
reusable workflow with two callers.
[`release.yml`](../.github/workflows/release.yml) runs it when a version
tag is pushed (see [`docs/agent/release.md`](agent/release.md)) and then
publishes, and
[`release-dry-run.yml`](../.github/workflows/release-dry-run.yml) runs
it on every PR as a dry run: it builds and packages each target, checks
the binary carries its dependency list for `cargo audit bin`, and
writes its SBOM, stopping short of the attestation and the GitHub
Release, which only exist in `release.yml`. So the first real run on a
tag isn't the first time any of it has run, and a PR's checks list only
jobs that can run on it. Every PR builds all five targets: Linux on
x86_64 and arm64, macOS on Apple silicon and Intel, and Windows on
x86_64. Its `reproducible` job also builds the Linux binary twice, from
two directories, and fails unless the results are identical, and a tag
only publishes once that passes.

### Playground publishing

[`playground.yml`](../.github/workflows/playground.yml) builds the
[browser playground](../playground/README.md) on every PR that touches
flint's code or the playground, and
[`playground-main.yml`](../.github/workflows/playground-main.yml) builds
it on each such push to `main` and publishes it to the root of the
[GitHub Pages site](github-pages.md), which also hosts the coverage and
benchmark reports and each PR's playground preview. Both build with
[`playground-build.yml`](../.github/workflows/playground-build.yml), a
reusable workflow.

## Testing

| Recipe | Wraps | Notes |
|---|---|---|
| `test` | `cargo test` | Unit tests only, for flint itself and for the [playground](../playground/README.md) — whose tests also check every playground example prints exactly its documented output. Also [`tests/doc_links.rs`](../tests/doc_links.rs), which checks each Markdown link into a `.rs` file by line range — `` [`name`](../src/checker.rs#L10-L20) `` — still covers the item its text names, from its first attribute to its closing brace. (`tests/robot.rs` also runs the e2e suite, but only when `FLINT_ROBOT` is set — see `mutants` below.) |
| `fix-doc-links` | `cargo test`, `UPDATE_DOC_LINKS=1` | Rewrites every stale line range `test` reports to the item's current lines. A link to an item that's been renamed or removed still fails; fix that by hand. |
| `venv` | `python3 -m venv` + `pip install` | Creates/refreshes `.venv`, the virtualenv the Robot Framework suite, Robocop and `reuse` run in, from [`e2e/requirements.txt`](../e2e/requirements.txt), checking each package's hash. Needs Python 3.10 or newer (macOS's own is 3.9 — `brew install python`). It uses `python3` if that's new enough, otherwise the newest `python3.1x` on `PATH` — so a newer Python works even when the system one comes first on `PATH`. A `.venv` built by an older Python is rebuilt. |
| `pip-lock` | `uv pip compile` | Recompiles [`e2e/requirements.txt`](../e2e/requirements.txt) from [`e2e/requirements.in`](../e2e/requirements.in), resolving for every platform and Python 3.10 onwards and recording each file's hash. Run it after editing `requirements.in`. Needs `uv`, which `mise.toml` pins. |
| `test-e2e` | `robot` (Robot Framework) | Builds the debug binary first, then runs `e2e/tests/*.robot` against it. |
| `test-all` | `test` + `test-e2e` | Unit tests and end-to-end tests together. |
| `test-minimal-versions` | `cargo +nightly update -Z direct-minimal-versions` + `cargo test --locked` | Runs `cargo test` against the oldest version of each direct dependency that [`Cargo.toml`](../Cargo.toml) allows, instead of the newest ones `Cargo.lock` pins. A requirement like `regex = "1"` lets Cargo pick any 1.x, so without this nothing checks flint still works with the oldest — this is what keeps those floors honest. Builds in `target/minimal-versions/`, and puts `Cargo.lock` back afterwards. Dev-dependencies are resolved too, so a floor can be set by what Criterion needs rather than by flint's own code; when a dependency bump fails this, raise the floor in `Cargo.toml` to the version the error names. Only the resolving needs nightly, which it installs if missing. |
| `mutants *ARGS` | [`cargo mutants`](https://mutants.rs) | Mutation testing: makes one small change to the code at a time and checks the tests (unit and e2e, via [`tests/robot.rs`](../tests/robot.rs)) catch it. A few minutes; results in `target/mutants.out/`. See [`mutation-testing.md`](mutation-testing.md). Not part of `ci`. |
| `mutants-diff [base] *ARGS` | `cargo mutants --in-diff` | Mutation testing of only the code changed since `base` (default `main`), uncommitted changes included; usually seconds. What the PR check runs. See [`mutation-testing.md`](mutation-testing.md). |
| `coverage` | `coverage-collect` + [`coverage-report.py`](../.github/scripts/coverage-report.py) `--summary` | Prints each file's line coverage from the unit tests, the e2e suite and both combined, then opens the HTML report. See [`coverage.md`](coverage.md). |
| `coverage-collect` | [`cargo llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) | Runs both suites against an instrumented build in `target/coverage/` and writes lcov, summary and HTML reports to `target/coverage/report/`; what [`coverage.yml`](../.github/workflows/coverage.yml) runs. Needs cargo-llvm-cov and the `llvm-tools-preview` rustup component. Not part of `ci` — coverage is reported, not enforced. |
| `ci` | `lint` + `test-all` + `test-minimal-versions` + `package-check` | Everything [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs, across its `ci`, `compat` and `e2e` jobs. |

## Benchmarking

| Recipe | Wraps | Notes |
|---|---|---|
| `bench *ARGS` | `cargo bench --bench e2e` (Criterion) | Times the real binary against real directories. See [`benchmarking.md`](benchmarking.md) for the two tiers, comparing runs, and why this isn't part of `ci`/`lint`/`test-all`. |
| `bench-logic *ARGS` | `cargo bench --bench logic` ([Gungraun](https://github.com/gungraun/gungraun)) | Counts the CPU instructions the in-memory parsing and checking logic executes, under Valgrind — the benchmark a PR can fail. Linux only; needs Valgrind and a `gungraun-runner` matching `Cargo.lock`, which the recipe checks. |
| `bench-logic-docker *ARGS` | `bench-logic`, in a container | The same, in a Linux container it builds with Valgrind and `gungraun-runner` installed — for macOS, where Valgrind doesn't run. Needs Docker. |

## Playground

| Recipe | Wraps | Notes |
|---|---|---|
| `playground` | `cargo build --target wasm32-unknown-unknown` + `wasm-bindgen` | Builds the [browser playground](../playground/README.md) into `playground/target/site/`. Needs the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at exactly the version `playground/Cargo.lock` pins (`install-tools` installs both); it says which version to install if it finds another. |
| `playground-serve` | `playground` + `python3 -m http.server` | Builds the playground and serves it at <http://localhost:8001>. |
| `favicons` | ImageMagick's `magick` | Regenerates the playground's favicon PNG files and `favicon.ico` from `playground/web/favicon/favicon.svg`. The outputs are committed, so this is only needed after changing the logo. Needs ImageMagick (`brew install imagemagick`). |

## Fuzzing

| Recipe | Wraps | Notes |
|---|---|---|
| `fuzz-replay` | `cargo +nightly fuzz run ... -runs=0` | Replays every seed and corpus input once; what the PR check runs. |
| `fuzz [minutes]` | `cargo +nightly fuzz run` + `cargo fuzz cmin` | Fuzzes for `minutes` (default 5) to grow the corpus, then minimizes it and summarizes the change. See [`fuzzing.md`](fuzzing.md) for both, and why neither is part of `ci`/`lint`/`test-all`. |

Both need nightly Rust and cargo-fuzz. `fmt`, `fmt-check` and `clippy`
also cover the fuzz harness (on stable — only running it needs nightly),
and the [playground](../playground/README.md).

## Setup

| Recipe | Wraps | Notes |
|---|---|---|
| `install-tools` | `mise install` + `rustup` + `cargo install` + `python3 -m venv` | Installs/refreshes the local dev tools the other recipes need, at the versions CI uses (see "Tool versions" above): everything in `mise.toml`, the `rust-toolchain.toml` toolchain, a nightly toolchain (for fuzzing), the `llvm-tools-preview` component (for coverage), the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` (for the playground), and the Python venv (Robot Framework, Robocop and `reuse`, via `venv`). Needs rustup and mise, and warns if mise isn't activated in the shell, since then the recipes would find some other install first. |

## API docs

| Recipe | Wraps | Notes |
|---|---|---|
| `docs` | `cargo doc --no-deps --document-private-items --open` | Renders every `///`/`//!` doc comment into cross-linked HTML under `target/doc/` and opens it. Private items are included because most of flint's logic is `pub(crate)` or private rather than public API (see "Code shape" in [`AGENTS.md`](../AGENTS.md)). [docs.rs](https://docs.rs/flint-cli) hosts the same view for each published release, via `[package.metadata.docs.rs]` in [`Cargo.toml`](../Cargo.toml). |
| `docs-check` | `cargo doc`, `RUSTDOCFLAGS="-D warnings"` | Same build as `docs`, minus `--open`; part of `lint`/`ci`. Fails on things `cargo doc` only warns about, e.g. an intra-doc link to a private, renamed, or removed item. |
