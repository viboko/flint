#!/usr/bin/env bash
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

# Usage: pr-comment.sh <pr-number> <body-file> [update-only]
#
# Updates the PR's comment of the same kind in place if it has one, or
# posts a new one — unless `update-only` is given. The kind is the
# marker on the body's first line (e.g. `<!-- flint-bench-report -->`,
# MARKER in bench-report.py or coverage-report.py, PR_MARKER in
# mutants-report.py), which every comment body starts with. Needs
# GH_TOKEN (pull-requests: write) and REPO (owner/name) in the
# environment. Used by bench-publish.yml, coverage-publish.yml,
# playground-publish.yml and mutants-pr-publish.yml — see
# docs/benchmarking.md, docs/coverage.md, docs/github-pages.md and
# docs/mutation-testing.md.
set -euo pipefail

pr=$1
body=$2
mode=${3:-}

marker=$(head -n 1 "$body")
if ! [[ "$marker" =~ ^\<!--\ flint-[a-z-]+-report\ --\>$ ]]; then
    echo "The comment body doesn't start with a flint report marker." >&2
    exit 1
fi

# Collected in full rather than piped into `head`, which could close the
# pipe early and fail the whole command under pipefail.
ids=$(gh api --paginate "repos/$REPO/issues/$pr/comments" --jq \
    ".[] | select(.user.login == \"github-actions[bot]\" and (.body | startswith(\"$marker\"))) | .id")
existing=${ids%%$'\n'*}

if [ -n "$existing" ]; then
    gh api -X PATCH "repos/$REPO/issues/comments/$existing" -F body=@"$body" > /dev/null
elif [ "$mode" != update-only ]; then
    gh api -X POST "repos/$REPO/issues/$pr/comments" -F body=@"$body" > /dev/null
fi
