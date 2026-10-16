# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

set shell := ["bash", "-uc"]

# Where the virtualenv puts its executables, and the suffix on the ones
# Cargo builds: both differ on Windows, where the e2e suite also runs in CI
venv_bin := if os_family() == "windows" { ".venv/Scripts" } else { ".venv/bin" }
exe := if os_family() == "windows" { ".exe" } else { "" }

# The wasm-bindgen crate version playground/Cargo.lock pins, which the
# wasm-bindgen CLI must match exactly
wasm_bindgen_version := `awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print }' playground/Cargo.lock`

# The gungraun-runner version Cargo.lock pins, which the installed
# gungraun-runner binary must match exactly
gungraun_version := `awk '/^name = "gungraun-runner"$/ { getline; gsub(/version = |"/, ""); print }' Cargo.lock`

# List available recipes
default:
    @just --list

# Build the debug binary
build:
    cargo build

# Build the optimized release binary
build-release:
    cargo build --release

# Run flint (pass args after `--`, e.g. `just run -- --version`)
run *ARGS:
    cargo run -- {{ ARGS }}

# Build browsable API docs from doc comments and open them (rustdoc —
# see docs/tooling.md for how this fits the rest of the recipes)
[doc("Build the API docs (rustdoc) and open them")]
docs:
    cargo doc --no-deps --document-private-items --open

# Check that rustdoc builds without warnings (e.g. a doc comment linking
# to a private item) — what CI runs; same flags as `docs`, minus `--open`
[doc("Check that rustdoc builds without warnings (what CI runs)")]
docs-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# docs/usage.md opens with a `$ flint --help` block: check it's what
# `--help` really prints, so a new or reworded argument can't leave it
# stale. A recipe rather than a unit test, so the crate doesn't need
# docs/usage.md to build its tests.
[doc("Check docs/usage.md's `flint --help` block matches the real help text")]
usage-check: build
    #!/usr/bin/env bash
    set -euo pipefail
    # The lines between `$ flint --help` and the block's closing fence.
    documented="$(awk '/^\$ flint --help$/ { inside = 1; next } inside && /^```/ { exit } inside' docs/usage.md)"
    if [ -z "$documented" ]; then
        echo 'docs/usage.md should open with a `$ flint --help` console block' >&2
        exit 1
    fi
    if ! diff -u --label docs/usage.md --label 'flint --help' \
        <(printf '%s\n' "$documented") <(./target/debug/flint --help); then
        echo "docs/usage.md's \`flint --help\` block is stale: paste in the real help text (+ lines)." >&2
        exit 1
    fi

# Run Rust unit tests, and the playground's (which check every
# playground example's documented output)
[doc("Run the Rust unit tests, and the playground's")]
test:
    cargo test
    cargo test --manifest-path playground/Cargo.toml

# Create/refresh the Python virtualenv used for the Robot Framework suite
# and its linter (needs Python 3.10+, as python3, python3.1x or python)
[doc("Create/refresh the Python virtualenv for the e2e suite and its linter")]
venv:
    #!/usr/bin/env bash
    set -euo pipefail
    new_enough='import sys; sys.exit(sys.version_info < (3, 10))'
    # Plain python3 can be an older system Python that comes first on
    # PATH (macOS's /usr/bin/python3 is 3.9) even with a newer one
    # installed, so fall back to versioned names, newest first, and then
    # to plain python, which is all Windows has.
    python=""
    for candidate in python3 python3.14 python3.13 python3.12 python3.11 python3.10 python; do
        if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c "$new_enough" 2>/dev/null; then
            python="$candidate"
            break
        fi
    done
    if [ -z "$python" ]; then
        echo "Python 3.10 or newer is needed, but python3 is $(python3 --version 2>&1)." >&2
        echo "Install a newer one (e.g. brew install python) and rerun." >&2
        exit 1
    fi
    # A .venv made by an older Python can't install Robocop, so rebuild it.
    if [ -d .venv ] && ! {{ venv_bin }}/python -c "$new_enough" 2>/dev/null; then
        echo "Rebuilding .venv with $("$python" --version 2>&1)..." >&2
        rm -rf .venv
    fi
    if [ ! -d .venv ]; then
        "$python" -m venv .venv
    fi
    {{ venv_bin }}/pip install -q --require-hashes -r e2e/requirements.txt

[doc("Recompile e2e/requirements.txt, with hashes, from e2e/requirements.in")]
pip-lock:
    cd e2e && uv pip compile --quiet --universal --python-version 3.10 --generate-hashes requirements.in -o requirements.txt

# Run the Robot Framework end-to-end suite against the debug binary
test-e2e: build venv
    #!/usr/bin/env bash
    set -euo pipefail
    FLINT_BINARY="{{ justfile_directory() }}/target/debug/flint{{ exe }}" {{ venv_bin }}/robot --outputdir target/robot e2e/tests

# Run unit tests and end-to-end tests
test-all: test test-e2e

# Run the tests against the oldest version of each direct dependency
# Cargo.toml allows, rather than the newest ones Cargo.lock pins — so a
# floor like `regex = "1.5.1"` stays true. Needs nightly to resolve them
# (installs it if missing); builds and tests with the default toolchain.
[doc("Run the tests against the oldest dependency versions Cargo.toml allows")]
test-minimal-versions:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup run nightly cargo --version >/dev/null 2>&1 \
        || rustup toolchain install nightly --profile minimal --no-self-update
    # Resolving rewrites Cargo.lock, so the real one is set aside and put
    # back afterwards, whether or not the tests pass.
    mkdir -p target
    cp Cargo.lock target/Cargo.lock.saved
    trap 'mv target/Cargo.lock.saved Cargo.lock' EXIT
    # Oldest versions for direct dependencies only: their own
    # dependencies' floors are theirs to keep, not flint's.
    cargo +nightly update -Z direct-minimal-versions
    # A target dir of its own, so the old versions' build doesn't replace
    # the normal one in target/debug.
    CARGO_TARGET_DIR=target/minimal-versions cargo test --locked

# Check what `cargo publish` would upload: exactly the files below, no
# more and no fewer, and that they build on their own. Cargo.toml's
# `include` decides what goes in; this list is the second key, so a
# change to either shows up as a diff here. Adding a file to the crate
# means adding it to both.
[doc("Check the crates.io package contains exactly the expected files and builds")]
package-check:
    #!/usr/bin/env bash
    set -euo pipefail
    expected='.cargo_vcs_info.json
    Cargo.lock
    Cargo.toml
    Cargo.toml.orig
    LICENSE-APACHE
    LICENSE-MIT
    README.md
    src/checker.rs
    src/entry_kind.rs
    src/flint_file.rs
    src/format.rs
    src/lib.rs
    src/main.rs
    src/memory_tree.rs
    src/origin.rs
    src/output.rs
    src/run.rs
    src/scope.rs
    src/template.rs'
    # --allow-dirty so it runs mid-change too; the list comes from
    # Cargo.toml's `include`, not from what's committed.
    if ! diff -u --label expected --label actual \
        <(sed 's/^ *//' <<<"$expected") \
        <(cargo package --list --allow-dirty 2>/dev/null); then
        echo "The crates.io package's files changed (- expected, + actual)." >&2
        echo "Fix Cargo.toml's \`include\`, or update the list in the justfile's package-check." >&2
        exit 1
    fi
    # Builds the packaged copy in target/package/, which catches a file
    # the build needs but `include` leaves out.
    cargo package --allow-dirty

# Mutation testing: mutate the code under src/ one change at a time and
# check that the tests (unit + e2e) catch each one. Takes a few minutes;
# pass extra args after the recipe name, e.g. `just mutants --file
# src/scope.rs`. Needs cargo-mutants — see docs/mutation-testing.md.
[doc("Mutation testing of everything under src/ (takes a few minutes)")]
[positional-arguments]
mutants *ARGS: venv
    FLINT_ROBOT="$(pwd)/{{ venv_bin }}/robot" cargo mutants --jobs 4 "$@"

# Mutation testing of only the code changed since BASE (default: main),
# uncommitted changes included — what the PR check runs. Usually seconds
# to a minute. Extra args go to cargo-mutants, e.g. `just mutants-diff
# main --list`.
[doc("Mutation testing of only the code changed since BASE (default: main)")]
[positional-arguments]
mutants-diff base="main" *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    base="$1"
    shift
    # Against the working tree, which is what cargo-mutants checks the
    # diff's new side against, and from the merge base, so changes made
    # on BASE since this branch left it aren't counted.
    mkdir -p target
    git diff "$(git merge-base "$base" HEAD)" -- src/ > target/mutants.diff
    just mutants --in-diff target/mutants.diff "$@"

# Measure line coverage of both test suites, print a per-file summary
# and open the combined HTML report (needs cargo-llvm-cov — see
# docs/coverage.md)
[doc("Measure line coverage of both test suites and open the HTML report")]
coverage: coverage-collect
    {{ venv_bin }}/python .github/scripts/coverage-report.py --summary target/coverage/report/summary.json
    open target/coverage/report/combined/html/index.html 2>/dev/null || xdg-open target/coverage/report/combined/html/index.html

# Run both test suites against an instrumented build and write lcov,
# summary and HTML reports to target/coverage/report/ (what CI runs, for
# the PR and for its base commit)
[doc("Write coverage reports for both test suites to target/coverage/report/")]
coverage-collect: venv
    #!/usr/bin/env bash
    set -euo pipefail
    # A target dir of its own, so the instrumented build never replaces
    # the normal one in target/debug.
    export CARGO_TARGET_DIR="$(pwd)/target/coverage"
    # eval, not `source <(...)`: macOS's bash 3.2 silently sources nothing
    # from a process substitution.
    eval "$(cargo llvm-cov show-env --sh 2>/dev/null)"
    cargo llvm-cov clean --workspace
    report="$CARGO_TARGET_DIR/report"
    rm -rf "$report" "$CARGO_TARGET_DIR/unit-profraw"
    mkdir -p "$report"

    # Each instrumented process writes a .profraw file into
    # $CARGO_TARGET_DIR, and `report` merges whichever ones are there —
    # so the unit tests' files are set aside while the e2e suite runs,
    # to report each suite on its own before reporting them together.
    cargo test
    cargo llvm-cov report --lcov --output-path "$report/unit.lcov"
    cargo llvm-cov report --html --output-dir "$report/unit"
    mkdir "$CARGO_TARGET_DIR/unit-profraw"
    mv "$CARGO_TARGET_DIR"/*.profraw "$CARGO_TARGET_DIR/unit-profraw/"

    cargo build
    FLINT_BINARY="$CARGO_TARGET_DIR/debug/flint" {{ venv_bin }}/robot \
        --outputdir "$CARGO_TARGET_DIR/robot" e2e/tests
    cargo llvm-cov report --lcov --output-path "$report/e2e.lcov"
    cargo llvm-cov report --html --output-dir "$report/e2e"

    mv "$CARGO_TARGET_DIR"/unit-profraw/*.profraw "$CARGO_TARGET_DIR/"
    cargo llvm-cov report --lcov --output-path "$report/combined.lcov"
    cargo llvm-cov report --html --output-dir "$report/combined"
    {{ venv_bin }}/python "{{ justfile_directory() }}/.github/scripts/coverage-report.py" \
        --summarize "$report" > "$report/summary.json"

# Run the wall-clock benchmark (Criterion): the real binary against real
# directories. Pass extra args after `--`, e.g.
# `just bench -- --save-baseline main`. Not part of `ci`/`lint`/`test-all`
# — see docs/benchmarking.md for why, and for the save/compare
# workflow.
[doc("Run the wall-clock benchmark (Criterion)")]
bench *ARGS:
    cargo bench --bench e2e {{ ARGS }}

# Run the instruction-count benchmarks (Gungraun, under Valgrind) of the
# in-memory parsing and checking logic. Linux only: on macOS, use
# `just bench-logic-docker`. Needs Valgrind, and gungraun-runner at the
# version Cargo.lock pins. Pass extra args after `--`, e.g.
# `just bench-logic -- --save-baseline=main`. See docs/benchmarking.md.
[doc("Run the instruction-count benchmarks (Gungraun; Linux only)")]
bench-logic *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v valgrind >/dev/null; then
        echo "error: Valgrind not found. It doesn't run on macOS — use \`just bench-logic-docker\` there." >&2
        exit 1
    fi
    # The runner only works with the exact gungraun crate version the
    # benchmarks were built against.
    want="{{ gungraun_version }}"
    have="$(gungraun-runner --version 2>/dev/null | cut -d' ' -f2 || true)"
    if [ "$have" != "$want" ]; then
        echo "error: need gungraun-runner $want (found: ${have:-none}) — cargo install gungraun-runner --version $want --locked" >&2
        exit 1
    fi
    cargo bench --bench logic {{ ARGS }}

# `just bench-logic` in a Linux container, for machines Valgrind doesn't
# run on. Needs Docker. Builds an image with Valgrind and the right
# gungraun-runner, on the Rust version rust-toolchain.toml pins, on first
# use, and keeps its build output and saved baselines in the
# `flint-bench-logic` Docker volume, not in target/.
[doc("Run `just bench-logic` in a Linux container (needs Docker)")]
bench-logic-docker *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{ gungraun_version }}"
    rust="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
    docker build --quiet --tag flint-bench-logic \
        --build-arg RUST_VERSION="$rust" --build-arg RUNNER_VERSION="$want" - <<'EOF'
    ARG RUST_VERSION
    FROM rust:${RUST_VERSION}
    ARG RUNNER_VERSION
    RUN apt-get update \
        && apt-get install -y --no-install-recommends valgrind \
        && rm -rf /var/lib/apt/lists/*
    RUN cargo install gungraun-runner --version "$RUNNER_VERSION" --locked
    EOF
    # Gungraun turns address-space randomization off for each run, which
    # Docker's default sandbox forbids. Leaving it on only adds a little
    # noise to the simulated cache figures, not to instruction counts.
    docker run --rm \
        --volume "{{ justfile_directory() }}":/src:ro \
        --volume flint-bench-logic:/target \
        --env CARGO_TARGET_DIR=/target \
        --env GUNGRAUN_ALLOW_ASLR=true \
        --workdir /src \
        flint-bench-logic cargo bench --bench logic {{ ARGS }}

# Build the browser playground into playground/target/site/ (needs the
# wasm32-unknown-unknown Rust target and wasm-bindgen-cli at the version
# playground/Cargo.lock pins — see playground/README.md)
[doc("Build the browser playground into playground/target/site/")]
playground-build:
    #!/usr/bin/env bash
    set -euo pipefail
    # The wasm-bindgen CLI only works with the exact wasm-bindgen crate
    # version the module was built against.
    want="{{ wasm_bindgen_version }}"
    have="$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)"
    if [ "$have" != "$want" ]; then
        echo "The playground needs wasm-bindgen-cli $want (found: ${have:-none}). Install it with:" >&2
        echo "  cargo install --locked wasm-bindgen-cli --version $want" >&2
        exit 1
    fi
    cargo build --manifest-path playground/Cargo.toml --lib --release --target wasm32-unknown-unknown
    site=playground/target/site
    rm -rf "$site"
    mkdir -p "$site"
    wasm-bindgen --target web --no-typescript --out-dir "$site/pkg" \
        playground/target/wasm32-unknown-unknown/release/flint_playground.wasm
    cp -R playground/web/. "$site/"
    echo "Built $site/ — 'just playground-serve' serves it locally."

# Regenerate the playground's favicon PNG files and .ico from
# playground/web/favicon/favicon.svg (needs ImageMagick; the outputs are
# committed, so only after changing the logo)
[doc("Regenerate the playground's favicons from favicon.svg (needs ImageMagick)")]
playground-favicons:
    #!/usr/bin/env bash
    set -euo pipefail
    cd playground/web/favicon
    # Rendered large, then scaled down, for clean edges; no timestamps,
    # so an unchanged logo regenerates byte-identical files.
    png=(-strip -define png:exclude-chunks=date,time)
    render() {
        magick -background none -density 2400 favicon.svg -resize "$1x$1" "${png[@]}" "PNG32:$2"
    }
    render 16 favicon-16x16.png
    render 32 favicon-32x32.png
    magick favicon-16x16.png favicon-32x32.png favicon.ico
    # Home-screen icons sit on the page's background, since iOS fills
    # anything transparent with black. The bolt is narrow, so it takes up
    # 75% of the icon's height (crank's round logo takes 72%) to carry
    # about the same weight beside other icons.
    tile() {
        local bolt=$(( $1 * 75 / 100 ))
        magick -size "$1x$1" xc:'#f6f5f2' \
            \( -background none -density 2400 favicon.svg -resize "${bolt}x${bolt}" \) \
            -gravity center -composite "${png[@]}" "$2"
    }
    tile 180 apple-touch-icon.png
    tile 192 android-chrome-192x192.png
    tile 512 android-chrome-512x512.png

# Build the browser playground and serve it at http://localhost:8001
playground-serve: playground-build
    python3 -m http.server --directory playground/target/site 8001

# libFuzzer options shared by every fuzz recipe: inputs up to 4 KiB, and
# any single input taking over 10s counts as a hang. See docs/fuzzing.md.
fuzz_flags := "-max_len=4096 -timeout=10"

# Replay every seed and corpus input through the fuzz target once, without
# generating new inputs (what the PR check runs; needs nightly + cargo-fuzz)
[doc("Replay every seed and corpus input through the fuzz target once")]
fuzz-replay:
    cargo +nightly fuzz run --fuzz-dir fuzz check fuzz/corpus/check fuzz/seeds/check -- -runs=0 {{ fuzz_flags }}

# Fuzz for MINUTES minutes (Ctrl-C to stop early), growing
# fuzz/corpus/check, then minimize it. Needs nightly + cargo-fuzz.
[doc("Fuzz for MINUTES minutes, growing and then minimizing the corpus")]
fuzz minutes="5":
    #!/usr/bin/env bash
    set -euo pipefail
    minutes="{{ minutes }}"
    if ! [[ "$minutes" =~ ^[0-9]+$ ]]; then
        echo "minutes must be a whole number, got: $minutes" >&2
        exit 1
    fi
    corpus=fuzz/corpus/check
    mkdir -p "$corpus"
    before=$(find "$corpus" -type f | wc -l | tr -d ' ')

    # Ctrl-C stops the fuzzer, not this script, so an interrupted run
    # still gets minimized and summarized. New inputs are written to the
    # corpus as they're found, so stopping early loses nothing.
    trap 'echo; echo "Stopping early..."' INT
    status=0
    cargo +nightly fuzz run --fuzz-dir fuzz check "$corpus" fuzz/seeds/check -- -max_total_time=$((minutes * 60)) {{ fuzz_flags }} || status=$?
    trap - INT
    # 72 is libFuzzer's exit code for an interrupted run, 130 cargo-fuzz's
    # own when Ctrl-C reaches it first. Anything else is a real failure.
    if [ "$status" -ne 0 ] && [ "$status" -ne 72 ] && [ "$status" -ne 130 ]; then
        echo >&2
        echo "The fuzzer found a crash or hang — the input that triggered it is in fuzz/artifacts/check/." >&2
        echo "Reproduce it with: cargo +nightly fuzz run --fuzz-dir fuzz check fuzz/artifacts/check/<file>" >&2
        exit "$status"
    fi

    grown=$(find "$corpus" -type f | wc -l | tr -d ' ')
    echo
    echo "Minimizing the corpus (dropping inputs whose coverage others already reach)..."
    cargo +nightly fuzz cmin --fuzz-dir fuzz check "$corpus" -- {{ fuzz_flags }} >/dev/null 2>&1 \
        || { echo "Minimizing failed — rerun 'cargo +nightly fuzz cmin --fuzz-dir fuzz check $corpus' to see why." >&2; exit 1; }
    after=$(find "$corpus" -type f | wc -l | tr -d ' ')

    echo
    echo "Corpus: $before inputs before, $grown after fuzzing, $after after minimizing."
    # Relative to the committed corpus, which is what a commit of this
    # run would change.
    changes=$(git status --porcelain --untracked-files=all -- "$corpus")
    added=$(grep -c '^??' <<<"$changes" || true)
    removed=$(grep -c '^ D' <<<"$changes" || true)
    echo "Compared to the committed corpus: $added added, $removed removed."
    if [ "$added" -gt 0 ]; then
        echo
        echo "A few of the new inputs (non-printing bytes shown cat -v style):"
        shown=0
        while read -r file && [ "$shown" -lt 3 ]; do
            echo "--- $file"
            head -c 300 "$file" | cat -v
            echo
            shown=$((shown + 1))
        done < <(grep '^??' <<<"$changes" | cut -c4-)
    fi

# Remove build artifacts (including Robot Framework output, which lives under target/)
clean:
    cargo clean

# Format the code (including the fuzz harness and the playground)
fmt:
    cargo fmt
    cargo fmt --manifest-path fuzz/Cargo.toml
    cargo fmt --manifest-path playground/Cargo.toml

# Check formatting without changing files (what CI runs)
fmt-check:
    cargo fmt --check
    cargo fmt --manifest-path fuzz/Cargo.toml --check
    cargo fmt --manifest-path playground/Cargo.toml --check

# Strict clippy (clippy::all + clippy::pedantic, warnings denied)
clippy:
    cargo clippy --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
    cargo clippy --manifest-path playground/Cargo.toml --all-targets -- -D warnings

# Check dependency licenses, advisories, and bans — for the CLI, and for
# the playground, whose WebAssembly module ships to visitors' browsers
[doc("Check dependency licenses, advisories, and bans")]
deny:
    cargo deny check
    cargo deny --manifest-path playground/Cargo.toml check

# Minimum supported Rust version, from Cargo.toml's `rust-version`
msrv_version := `sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml`

# Check flint's library and binary still build on its minimum supported
# Rust version (installs that toolchain if it's missing)
[doc("Check flint builds on its minimum supported Rust version")]
msrv:
    rustup run {{ msrv_version }} rustc --version >/dev/null 2>&1 \
        || rustup toolchain install {{ msrv_version }} --profile minimal --no-self-update
    cargo +{{ msrv_version }} check --locked

# Find dependencies declared in Cargo.toml (or fuzz/ or playground/'s
# Cargo.toml) that the code never uses
[doc("Find unused dependencies")]
machete:
    cargo machete

# Lint GitHub Actions workflows (actionlint also runs shellcheck over
# their inline `run:` scripts, so it needs shellcheck installed too)
[doc("Lint GitHub Actions workflows")]
actionlint:
    actionlint

# Audit GitHub Actions workflows and Dependabot config for security
# problems (offline unless GH_TOKEN is set — CI also runs the online
# audits; see docs/tooling.md)
[doc("Audit GitHub Actions workflows and Dependabot config for security problems")]
zizmor:
    zizmor .

# Lint the repo's standalone shell scripts
shellcheck:
    shellcheck .github/scripts/*.sh .claude/hooks/*.sh

# Lint the repo's Markdown files (rules and file list in
# .markdownlint-cli2.yaml)
[doc("Lint the repo's Markdown files")]
markdownlint:
    markdownlint-cli2

# Check links between the repo's own files, including #heading
# fragments, without touching the network (config in lychee.toml)
[doc("Check links between the repo's own files (offline)")]
links:
    lychee --no-progress --offline .

# Rewrite every Markdown link into a .rs file by line range
# (`checker.rs#L10-L20`) to cover the item its text names again, after
# lines above it moved (`cargo test` checks them; see tests/doc_links.rs)
[doc("Fix doc links' line ranges into source files")]
fix-doc-links:
    UPDATE_DOC_LINKS=1 cargo test --test doc_links

# Check external links too (not part of `lint` — a site being down
# shouldn't fail it; CI runs this weekly instead). Set GITHUB_TOKEN to
# avoid GitHub rate-limiting github.com links.
[doc("Check external links too")]
links-online:
    lychee --no-progress .

# Spell-check code, comments and docs (config in _typos.toml; `typos -w`
# applies its suggested fixes)
[doc("Spell-check code, comments and docs")]
typos:
    typos

# Check every tracked file follows .editorconfig: UTF-8, LF line endings,
# a final newline, no trailing whitespace, spaces not tabs. Indent width
# isn't checked: rustfmt, ruff, Biome and Robocop already own it, and
# docstrings, raw strings and the license text indent by other rules.
[doc("Check files follow .editorconfig's whitespace rules")]
editorconfig:
    editorconfig-checker -disable-indent-size

# Lint and format-check the repo's Python (the CI report scripts), per
# ruff.toml
[doc("Lint and format-check the repo's Python")]
ruff:
    ruff check
    ruff format --check

# Lint and format-check the playground's JavaScript, CSS and HTML, and
# the SVGs, per biome.jsonc (`biome check --write .` fixes most findings in place)
[doc("Lint and format-check the playground's JavaScript, CSS and HTML, and the SVGs")]
biome:
    biome ci --error-on-warnings .

# Check the playground site's SEO basics: robots.txt and a well-formed
# sitemap.xml exist, and the sitemap, robots.txt and every page's
# canonical link and Open Graph URLs are absolute https URLs on the
# site's own domain (playground/web/CNAME)
[doc("Check the playground site's SEO basics")]
seo:
    #!/usr/bin/env bash
    set -euo pipefail
    web=playground/web
    fail() { echo "$1" >&2; exit 1; }
    base="https://$(tr -d '[:space:]' < "$web/CNAME")/"
    test -f "$web/robots.txt" || fail "missing $web/robots.txt"
    test -f "$web/sitemap.xml" || fail "missing $web/sitemap.xml"
    python3 -c 'import sys, xml.etree.ElementTree as et; et.parse(sys.argv[1])' "$web/sitemap.xml" \
        || fail "$web/sitemap.xml isn't well-formed XML"
    grep -qx "Sitemap: ${base}sitemap.xml" "$web/robots.txt" \
        || fail "$web/robots.txt doesn't point at ${base}sitemap.xml"
    grep -q "<loc>${base}</loc>" "$web/sitemap.xml" \
        || fail "$web/sitemap.xml doesn't list $base"
    for page in "$web"/*.html; do
        grep -q "<link rel=\"canonical\" href=\"${base}\">" "$page" \
            || fail "$page: the canonical link is missing, or isn't $base"
        grep -q "<meta property=\"og:url\" content=\"${base}\">" "$page" \
            || fail "$page: og:url is missing, or isn't $base"
        if grep -q 'property="og:image"' "$page"; then
            grep -q "<meta property=\"og:image\" content=\"${base}[^\"]*\">" "$page" \
                || fail "$page: og:image isn't an absolute URL under $base"
            image="$(sed -n "s|.*<meta property=\"og:image\" content=\"${base}\([^\"]*\)\">.*|\1|p" "$page")"
            test -f "$web/$image" || fail "$page: og:image $image doesn't exist in $web"
        fi
    done
    echo "SEO checks passed"

# Lint and format-check the Robot Framework suite (config in
# e2e/robocop.toml; `.venv/bin/robocop format e2e` fixes formatting)
[doc("Lint and format-check the Robot Framework suite")]
robocop: venv
    {{ venv_bin }}/robocop check e2e
    {{ venv_bin }}/robocop format --check --diff e2e

# Check every file states its copyright and license, per the REUSE spec:
# an SPDX header at its top, or an entry in REUSE.toml for files that
# can't carry one
[doc("Check every file states its copyright and license (REUSE)")]
reuse: venv
    {{ venv_bin }}/reuse lint

# Run flint against its own repository root (dogfooding)
flint: build
    ./target/debug/flint

# The static checks that take seconds once the build is cached:
# formatting, clippy, Markdown lint, spelling. Claude Code's `Stop` hook
# (.claude/hooks/stop-lint.sh) runs these before an agent can finish.
[doc("Run the quick static checks (formatting, clippy, Markdown, spelling)")]
lint-fast: fmt-check clippy markdownlint typos

# All static checks: formatting, clippy, MSRV build, cargo-deny, unused
# dependencies, workflow lint and security audit, shell script lint,
# Python lint, JavaScript/CSS/HTML lint, playground SEO, Robot Framework
# lint, Markdown lint, link check, spelling, .editorconfig, REUSE,
# flint, docs, usage docs
[doc("Run all static checks")]
lint: fmt-check clippy msrv deny machete actionlint zizmor shellcheck ruff biome seo robocop markdownlint links typos editorconfig reuse flint docs-check usage-check

# Everything CI runs
ci: lint test-all test-minimal-versions package-check

# Install/refresh local dev tooling at the versions CI uses: every tool
# mise.toml pins, the Rust toolchain rust-toolchain.toml pins, plus
# nightly Rust (fuzzing), llvm-tools (coverage), the wasm32 target and
# wasm-bindgen-cli (the playground), and the Python venv. Needs rustup
# and mise — see docs/tooling.md.
[doc("Install/refresh local dev tooling at the versions CI uses")]
install-tools:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v mise >/dev/null 2>&1; then
        echo "Please install mise first (see docs/tooling.md)" >&2
        exit 1
    fi
    mise install
    # rustup also installs rust-toolchain.toml's toolchain on first use,
    # but the components below need it in place already.
    rustup toolchain install
    # Fuzzing (see docs/fuzzing.md) needs a nightly toolchain alongside
    # it, for the sanitizer flags cargo-fuzz builds with.
    rustup toolchain install nightly --profile minimal
    # Coverage (see docs/coverage.md) needs LLVM's profiling tools,
    # matched to the toolchain.
    rustup component add llvm-tools-preview
    # The browser playground (see playground/README.md) builds to
    # WebAssembly, with the wasm-bindgen CLI at exactly the version
    # playground/Cargo.lock pins.
    rustup target add wasm32-unknown-unknown
    want="{{ wasm_bindgen_version }}"
    if [ "$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)" != "$want" ]; then
        cargo install --locked wasm-bindgen-cli --version "$want"
    fi
    just venv
    # The recipes run each tool by name, so they only get mise's pinned
    # versions if mise is on PATH ahead of any other install.
    case "$(command -v typos || true)" in
        "$(mise which typos)" | */mise/shims/typos) ;;
        *)
            echo >&2
            echo "Installed, but mise isn't activated in this shell, so the recipes won't use these versions." >&2
            echo "See docs/tooling.md for how to activate it." >&2
            ;;
    esac
