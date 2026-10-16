# The GitHub Pages site

flint's GitHub Pages site, <https://flint.viboko.dev/>, hosts the
[browser playground](../playground/README.md) and the HTML reports from
the [coverage](coverage.md) and [benchmark](benchmarking.md) workflows,
for `main` and for each open PR:

| Path | What's there | Published by |
|---|---|---|
| `/` | `main`'s playground | [`playground-main.yml`](../.github/workflows/playground-main.yml), on each push to `main` that could change it |
| `/coverage/` | `main`'s coverage reports | [`coverage-main.yml`](../.github/workflows/coverage-main.yml), on each push to `main` that could change coverage |
| `/benchmarks/` | `main`'s benchmark reports | [`bench-main.yml`](../.github/workflows/bench-main.yml), on each push to `main` that changes implementation code |
| `/pr-<N>/playground/` | PR #N's playground, to try before merging | [`playground-publish.yml`](../.github/workflows/playground-publish.yml), which also comments the link on the PR |
| `/pr-<N>/coverage/` | PR #N's coverage reports, linked from its coverage comment | [`coverage-publish.yml`](../.github/workflows/coverage-publish.yml) |
| `/pr-<N>/benchmarks/` | PR #N's benchmark reports, linked from its benchmark comment | [`bench-publish.yml`](../.github/workflows/bench-publish.yml) |

The playground lives at the root, rather than under `/playground/`, so
the bare domain serves it: Pages serves files exactly where they are on
the branch, with no rewrites.

Each `/coverage/` index page is the per-file table `just coverage`
prints, each number linking to that suite's page for the file. Each
`/benchmarks/` directory leads to Criterion's own report index.

A PR's `pr-<N>/` directory is removed when it closes or merges, by
[`pages-cleanup.yml`](../.github/workflows/pages-cleanup.yml). What it
showed then lives on under the top-level paths, once `main`'s next run
publishes them.

## How it's published

Pages serves the `gh-pages` branch. Every workflow writes to it through
[`gh-pages.sh`](../.github/scripts/gh-pages.sh), which replaces one path
in the table above and leaves everything else alone — including the
`CNAME` file, which the playground brings with it (see
[`playground/README.md`](../playground/README.md#publishing)). Workflows
racing to push retry on top of each other's commits.

What runs on `main` is trusted code, so those workflows build in a
read-only job and publish from a second job with a write token. PRs are
different: a PR's workflows run that PR's code, which for a fork could
be anyone's, so they only upload an artifact, and a second workflow
triggered by `workflow_run` publishes it. That workflow always runs as
it is on `main` and never executes the PR's code. It checks the
artifact's PR number belongs to a PR whose head is the commit that was
built, and **only hosts same-repo PRs**: a fork's build produces its
own HTML, so hosting it would let anyone publish arbitrary pages on this
domain. A fork's report links point at its workflow run instead, and
it gets no playground preview. See "CI" in
[`benchmarking.md`](benchmarking.md#ci) for the full reasoning.

[`robots.txt`](../playground/web/robots.txt) keeps search engines out of
the reports and previews; they're reachable, just not listed.

Every publish is a commit on `gh-pages`, so the branch grows with each
push to `main`. If it ever gets unwieldy, it can be squashed to a single
commit of its current contents and force-pushed — nothing depends on its
history.

## Setup

Pages must be enabled once, under Settings → Pages → "Deploy from a
branch", branch `gh-pages`, folder `/ (root)`. The branch is created by
the first publish, so enable it after that. Until it's enabled, report
links point at the workflow runs, whose artifacts hold the reports, and
PRs get no playground preview comment.
