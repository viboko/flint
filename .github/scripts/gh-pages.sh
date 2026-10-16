#!/usr/bin/env bash
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

# Usage: gh-pages.sh <target> [<dir>]
#
# Publishes the contents of <dir> to <target> on the gh-pages branch,
# replacing whatever was there, or removes <target> if no <dir> is
# given. Creates the branch if it doesn't exist yet. The targets are the
# site's layout (see docs/github-pages.md):
#
#   playground                the site's root: main's playground
#   coverage, benchmarks      main's reports
#   pr-<N>/playground, pr-<N>/coverage, pr-<N>/benchmarks
#                             the same for an open PR
#   pr-<N>                    everything for a PR (only to remove it,
#                             once the PR closes)
#
# Needs GH_TOKEN (contents: write) and REPO (owner/name) in the
# environment.
#
# Several workflows can race to push. Each run only ever touches its
# own target, so a rejected push is safely retried by redoing the whole
# update on top of the new tip.
set -euo pipefail

target=$1
src=${2:+$(cd "$2" && pwd)}

if ! [[ "$target" =~ ^(playground|coverage|benchmarks|pr-[0-9]+(/(playground|coverage|benchmarks))?)$ ]]; then
    echo "Unknown target: $target" >&2
    exit 2
fi
if [ -z "$src" ] && [ "$target" = playground ]; then
    echo "The playground at the root can only be replaced, not removed." >&2
    exit 2
fi

work=$(mktemp -d)
cd "$work"
git init -q
git remote add origin "https://x-access-token:${GH_TOKEN}@github.com/${REPO}.git"
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

for attempt in 1 2 3 4 5; do
    if git fetch -q --depth 1 origin gh-pages 2>/dev/null; then
        git checkout -q -f -B gh-pages FETCH_HEAD
    elif [ -z "$src" ]; then
        exit 0 # nothing published yet, so nothing to remove
    else
        git checkout -q -f --orphan "new-gh-pages-$attempt"
        git rm -rqf --ignore-unmatch .
    fi
    git clean -fdq

    if [ "$target" = playground ]; then
        # Everything at the top level is the playground's, except the
        # other targets' directories.
        find . -mindepth 1 -maxdepth 1 ! -name .git ! -name 'pr-*' \
            ! -name coverage ! -name benchmarks -exec rm -rf {} +
        cp -R "$src"/. .
    else
        rm -rf "$target"
        if [ -n "$src" ]; then
            mkdir -p "$(dirname "$target")"
            cp -R "$src" "$target"
        fi
    fi
    touch .nojekyll # serve files as-is, no Jekyll build

    git add -A
    if git diff --cached --quiet; then
        exit 0
    fi
    if [ -n "$src" ]; then
        git commit -q -m "Publish $target from ${GITHUB_SHA:-a local build}"
    else
        git commit -q -m "Remove $target"
    fi
    if git push -q origin HEAD:refs/heads/gh-pages; then
        exit 0
    fi
    echo "Push rejected (attempt $attempt) — retrying on the new tip." >&2
    sleep $((attempt * 3))
done

echo "Giving up after 5 attempts." >&2
exit 1
