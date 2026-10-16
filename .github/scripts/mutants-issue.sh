#!/usr/bin/env bash
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

# Usage: mutants-issue.sh <report-file> <missed-count>
#
# Keeps one open "Mutation testing" tracking issue in sync with the
# weekly report: opens it (or reopens the most recent one) when mutants
# were missed, updates its body to the latest report, and closes it with
# a comment once every mutant is caught. Found by the marker the report
# starts with (MARKER in mutants-report.py). Needs GH_TOKEN (issues:
# write) and REPO (owner/name) in the environment. Used by
# .github/workflows/mutants.yml — see docs/mutation-testing.md.
set -euo pipefail

report=$1
missed=$2
marker="<!-- flint-mutants-report -->"
title="Mutation testing: missed mutants"

# Most recently created first. Collected in full rather than piped into
# `head`, which could close the pipe early and fail under pipefail.
numbers=$(gh api --paginate "repos/$REPO/issues?state=all&creator=github-actions%5Bbot%5D&sort=created&direction=desc" \
    --jq ".[] | select(.pull_request == null and (.body // \"\" | startswith(\"$marker\"))) | \"\(.number) \(.state)\"")
latest=${numbers%%$'\n'*}
number=${latest%% *}
state=${latest#* }

if [ "$missed" -gt 0 ]; then
    if [ -z "$latest" ]; then
        gh api -X POST "repos/$REPO/issues" -f title="$title" -F body=@"$report" > /dev/null
    else
        gh api -X PATCH "repos/$REPO/issues/$number" -F body=@"$report" -f state=open > /dev/null
    fi
elif [ -n "$latest" ] && [ "$state" = open ]; then
    gh api -X PATCH "repos/$REPO/issues/$number" -F body=@"$report" -f state=closed > /dev/null
    gh api -X POST "repos/$REPO/issues/$number/comments" \
        -f body="Every mutant is caught as of the latest weekly run, so closing this." > /dev/null
fi
