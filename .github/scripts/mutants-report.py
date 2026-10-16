#!/usr/bin/env python3
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Turns a cargo-mutants run into a mutation-testing report.

Reads `outcomes.json` from a `mutants.out` directory (what `just mutants`
and `just mutants-diff` write under target/) and prints Markdown: the
outcome counts, then every missed mutant — a change to the code that no
test noticed — grouped by file, each linking to its line. Timed-out
mutants are listed separately.

Two flavors of the same report:

- The weekly whole-codebase run: .github/workflows/mutants.yml writes it
  to the run's step summary and keeps a tracking issue's body in sync
  with it.
- `--pr`: a PR's comment, for a run of only the code that PR changes
  (.github/workflows/mutants-pr.yml, posted by mutants-pr-publish.yml).
  `--skipped` and `--publish-failed` print that comment's two notices.

See docs/mutation-testing.md, including how to triage what it lists.

Standard library only, so it runs on a stock CI runner with no setup.
"""

import argparse
import html
import json
import re
import sys
from pathlib import Path

MARKER = "<!-- flint-mutants-report -->"
PR_MARKER = "<!-- flint-mutants-pr-report -->"


def load_outcomes(mutants_out):
    return json.loads((Path(mutants_out) / "outcomes.json").read_text(encoding="utf-8"))


def mutants_with(outcomes, summary):
    """Returns (file, line, description) for each mutant with that outcome."""
    found = []
    for outcome in outcomes["outcomes"]:
        mutant = outcome["scenario"]
        if outcome["summary"] != summary or not isinstance(mutant, dict):
            continue
        mutant = mutant["Mutant"]
        line = mutant["span"]["start"]["line"]
        # The name is `<file>:<line>:<column>: <what was changed>`.
        description = re.sub(r"^[^:]+:\d+:\d+: ", "", mutant["name"])
        found.append((mutant["file"], line, description))
    return sorted(found)


def line_link(path, line, blob_url):
    text = f"{path}:{line}"
    if not blob_url:
        return f"<code>{text}</code>"
    return f'<a href="{blob_url.rstrip("/")}/{path}#L{line}"><code>{text}</code></a>'


def render_list(mutants, blob_url):
    out = []
    for path, line, description in mutants:
        out.append(f"- {line_link(path, line, blob_url)}: <code>{html.escape(description)}</code>")
    return out


def render_report(outcomes, blob_url, run_url, pr=False):
    missed = mutants_with(outcomes, "MissedMutant")
    timeouts = mutants_with(outcomes, "Timeout")

    if pr:
        out = [PR_MARKER, "## Mutation testing: this PR's changes", ""]
        scope = "the code this PR adds or changes under `src/`"
    else:
        out = [MARKER, "## Mutation testing", ""]
        scope = "the code"
    if missed:
        out.append(
            f"**🔴 {len(missed)} missed** — each is a change to {scope} that every "
            "test (unit and end-to-end) still passed with."
        )
    else:
        out.append(f"**Every mutant of {scope} was caught.** 🟢")
    out.append("")
    out.append("| Caught | Missed | Timed out | Unviable | Total |")
    out.append("|---:|---:|---:|---:|---:|")
    out.append(
        f"| {outcomes['caught']} | {outcomes['missed']} | {outcomes['timeout']} "
        f"| {outcomes['unviable']} | {outcomes['total_mutants']} |"
    )
    out.append("")

    if missed:
        out.append("### Missed")
        out.append("")
        out.extend(render_list(missed, blob_url))
        out.append("")
        out.append(
            "For each one: if it's a real gap, add a test that fails with the "
            "mutation applied. If the mutation can't change behavior (an "
            "*equivalent mutant*), exclude it in `.cargo/mutants.toml` with a "
            "comment saying why. "
            + (
                "`just mutants-diff` re-checks this PR's changes locally. "
                if pr
                else "`just mutants --file <path>` re-checks one file. "
            )
            + "See `docs/mutation-testing.md`."
        )
        out.append("")
    if timeouts:
        out.append("<details><summary>Timed out</summary>")
        out.append("")
        out.append(
            "These made the tests hang rather than fail — usually an infinite "
            "loop, which a real bug would cause too. Worth excluding if they "
            "keep recurring, since each one costs a full timeout."
        )
        out.append("")
        out.extend(render_list(timeouts, blob_url))
        out.append("")
        out.append("</details>")
        out.append("")

    footer = f"cargo-mutants {outcomes['cargo_mutants_version']}."
    if run_url:
        footer += (
            f' <a href="{run_url}">Workflow run</a> (its <code>mutants-out</code> '
            "artifact has each mutant's diff and test log)."
        )
    out.append(footer)
    return "\n".join(out) + "\n"


def render_skipped():
    return (
        f"{PR_MARKER}\n## Mutation testing: skipped\n\n"
        "This PR no longer changes any code under `src/` that cargo-mutants can "
        "mutate (test code and comments don't count), so there was nothing to "
        "test.\n"
    )


def render_publish_failed(run_url):
    return (
        f"{PR_MARKER}\n## Mutation testing: couldn't publish results\n\n"
        f'Mutation testing ran, but posting its results failed. See the <a href="{run_url}">'
        "publish run</a> for what went wrong; the next push to this PR will try again.\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("mutants_out", type=Path, nargs="?", help="The mutants.out directory.")
    parser.add_argument("--pr", action="store_true", help="Word it as a PR comment.")
    parser.add_argument("--skipped", action="store_true", help="Print the PR's 'skipped' notice.")
    parser.add_argument(
        "--publish-failed",
        action="store_true",
        help="Print the PR's 'couldn't publish' notice (pass the publish run as --run-url).",
    )
    parser.add_argument("--blob-url", help="URL the tested commit's files are viewable under.")
    parser.add_argument("--run-url", help="URL of the workflow run.")
    args = parser.parse_args()

    if args.skipped:
        report = render_skipped()
    elif args.publish_failed:
        report = render_publish_failed(args.run_url)
    elif args.mutants_out is None:
        parser.error("the mutants.out directory is required")
    else:
        outcomes = load_outcomes(args.mutants_out)
        report = render_report(outcomes, args.blob_url, args.run_url, args.pr)
    sys.stdout.write(report)


if __name__ == "__main__":
    main()
