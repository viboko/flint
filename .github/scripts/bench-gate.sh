#!/usr/bin/env bash
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

# Usage: bench-gate.sh <base-ref> <head-ref> [<path>...]
#
# Prints "true" if the diff between the two refs changes implementation
# code under the given paths (by default src/ and the dependency
# lockfile), or "false" if every change there is confined to unit tests.
# Used by .github/workflows/bench.yml and bench-logic.yml to skip
# benchmarking PRs that can't have changed performance — see
# docs/benchmarking.md.
#
# Unit tests live inline in each source file as a trailing
# `#[cfg(test)] mod tests { ... }`, so a path filter alone can't tell a
# test-only edit from an implementation one. Instead, each changed file
# is compared with everything from its top-level `#[cfg(test)]` line to
# end-of-file removed. This relies on the test module being the last
# item in the file (the standard Rust convention, and true of every
# file in src/ today) — code placed after it would be treated as test
# code and never trigger a benchmark run.
set -euo pipefail

base=$1
head=$2
shift 2
if [ $# -eq 0 ]; then
    set -- src/ Cargo.lock
fi

without_tests() {
    # A file that doesn't exist on one side (added/deleted) is empty there.
    git show "$1:$2" 2>/dev/null | sed '/^#\[cfg(test)\]$/,$d' || true
}

while IFS= read -r -d '' path; do
    case "$path" in
        *.rs)
            if [ "$(without_tests "$base" "$path")" = "$(without_tests "$head" "$path")" ]; then
                echo "test-only change: $path" >&2
                continue
            fi
            ;;
    esac
    echo "implementation change: $path" >&2
    echo true
    exit 0
done < <(git diff -z --name-only "$base" "$head" -- "$@")

echo false
