#!/usr/bin/env python3
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Turns a Criterion base-vs-PR comparison into the benchmark PR comment.

Reads the output of `just bench -- --load-baseline pr --baseline base`
under target/criterion and prints a Markdown/HTML table: one row per
benchmark, one column per input size, each cell the percentage change in
mean time, marked with Criterion's own verdict for that benchmark (🔴
regressed, 🟢 improved, unmarked otherwise) and linking to its Criterion
report page. Each benchmark name expands to its description, taken from
the "What each benchmark covers" section of docs/benchmarking.md.

Links are emitted as placeholders (`__REPORT__:<path>`, `__RUN_URL__`)
because this runs in the untrusted, read-only half of the CI pipeline,
which doesn't know where the report will end up hosted.
.github/workflows/bench-publish.yml fills them in later by re-running
this script with `--fill <comment file>`. Passing `--report-base`
and/or `--run-url` fills in a freshly generated comment the same way
(e.g. `--report-base file://$PWD/target/criterion`, to preview locally).

Standard library only, so it runs on a stock CI runner with no setup.
"""

import argparse
import html
import json
import re
import sys
from pathlib import Path
from urllib.parse import quote

MARKER = "<!-- flint-bench-report -->"
DESCRIPTIONS_SECTION = "## What each benchmark covers"

# The exact sentences Criterion's HTML report uses for its verdict
# (criterion's src/html/mod.rs). Reading them from the rendered page,
# rather than re-deriving the verdict from the raw estimates, guarantees
# the comment always agrees with the page each number links to.
VERDICTS = {
    "Performance has regressed.": "regressed",
    "Performance has improved.": "improved",
    "Change within noise threshold.": "unchanged",
    "No change in performance detected.": "unchanged",
}
MARKS = {"regressed": "🔴 ", "improved": "🟢 ", "unchanged": "", "unknown": "⚠️ "}


def parse_descriptions(doc_path):
    """Maps each benchmark ID to its description paragraph(s).

    Each benchmark is a `### `<id>`` heading under DESCRIPTIONS_SECTION;
    its description is everything up to the next heading.
    """
    descriptions = {}
    in_section = False
    current = None
    for line in Path(doc_path).read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            in_section = line.strip() == DESCRIPTIONS_SECTION
            current = None
            continue
        if not in_section:
            continue
        heading = re.fullmatch(r"### `(.+)`\s*", line)
        if heading:
            current = heading.group(1)
            descriptions[current] = []
        elif line.startswith("#"):
            current = None
        elif current is not None:
            descriptions[current].append(line)
    # PR comments render every newline as a hard line break, so unwrap
    # each paragraph of the (72-column-wrapped) docs onto a single line.
    return {
        key: "\n\n".join(" ".join(p.split()) for p in "\n".join(lines).split("\n\n") if p.strip())
        for key, lines in descriptions.items()
    }


def read_verdict(report_page):
    """Returns (verdict, Criterion's own sentence for it)."""
    if not report_page.is_file():
        return "unknown", None
    text = report_page.read_text(encoding="utf-8")
    for sentence, verdict in VERDICTS.items():
        if sentence in text:
            return verdict, sentence
    return "unknown", None


def collect_results(criterion_dir, baseline_name):
    """Returns {row_id: {size: cell}} for every benchmark in the run."""
    rows = {}
    for meta_path in sorted(criterion_dir.glob(f"**/{baseline_name}/benchmark.json")):
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        bench_dir = meta_path.parent.parent
        row_id = meta["group_id"]
        if meta.get("function_id"):
            row_id += "/" + meta["function_id"]
        size = meta.get("value_str") or ""

        change_path = bench_dir / "change" / "estimates.json"
        if change_path.is_file():
            change = json.loads(change_path.read_text(encoding="utf-8"))
            mean = change["mean"]
            percent = mean["point_estimate"] * 100
            interval = (
                mean["confidence_interval"]["lower_bound"] * 100,
                mean["confidence_interval"]["upper_bound"] * 100,
            )
            verdict, sentence = read_verdict(bench_dir / "report" / "index.html")
        else:
            # Nothing on the base ref to compare against, e.g. a
            # benchmark this PR adds.
            percent = interval = sentence = None
            verdict = "new"

        rows.setdefault(row_id, {})[size] = {
            "percent": percent,
            "interval": interval,
            "sentence": sentence,
            "verdict": verdict,
            "report": meta["directory_name"] + "/report/index.html",
        }
    return rows


def size_sort_key(size):
    return (0, int(size)) if size.isdigit() else (1, size)


def report_link(path):
    return "__REPORT__:" + quote(path)


def render_cell(cell):
    if cell is None:
        return "–"
    if cell["verdict"] == "new":
        text = "new"
        tooltip = "No data from the base branch to compare against."
    else:
        text = f"{cell['percent']:+.1f}%"
        # The cell shows Criterion's best estimate, but its verdict rests
        # on the whole confidence interval, so a number past the
        # threshold can still be unmarked; the tooltip shows why.
        low, high = cell["interval"]
        tooltip = f"95% confidence interval: {low:+.1f}% to {high:+.1f}%."
        if cell["sentence"]:
            tooltip += f" Criterion: {cell['sentence']}"
    mark = MARKS.get(cell["verdict"], "")
    href = report_link(cell["report"])
    return f'{mark}<a href="{href}" title="{html.escape(tooltip)}">{text}</a>'


def render_name_cell(row_id, description):
    summary = f"<summary><code>{html.escape(row_id)}</code></summary>"
    if description is None:
        body = "⚠️ No description yet — see below."
    else:
        # Blank lines around the body let GitHub render it as Markdown
        # inside the surrounding HTML.
        body = description
    return f"<details>{summary}\n\n{body}\n\n</details>"


def render_report(rows, descriptions, doc_path, noise_threshold):
    # Rows follow the documentation's order; undocumented ones go last.
    doc_order = list(descriptions)
    row_ids = sorted(
        rows,
        key=lambda r: (doc_order.index(r) if r in doc_order else len(doc_order), r),
    )
    sizes = sorted({s for cells in rows.values() for s in cells}, key=size_sort_key)
    verdicts = [c["verdict"] for cells in rows.values() for c in cells.values()]

    out = [MARKER, "## Benchmarks: base vs. this PR", ""]
    counts = {v: verdicts.count(v) for v in ("regressed", "improved", "unknown", "new")}
    if counts["regressed"]:
        out.append(f"**🔴 {counts['regressed']} regressed** · 🟢 {counts['improved']} improved")
    elif counts["improved"]:
        out.append(f"**Nothing regressed** · 🟢 {counts['improved']} improved")
    else:
        out.append("**Nothing regressed or improved.**")
    if counts["new"] == len(verdicts):
        out.append(
            "\nNo benchmark data from the base branch to compare against — "
            "expected if this PR adds or restructures the benchmarks themselves."
        )
    out.append("")

    header = "".join(
        f"<th>{int(s):,}</th>" if s.isdigit() else f"<th>{html.escape(s)}</th>" for s in sizes
    )
    out.append("<table>")
    out.append(f"<tr><th>Benchmark (click to expand)</th>{header}</tr>")
    for row_id in row_ids:
        cells = "".join(f"<td>{render_cell(rows[row_id].get(s))}</td>" for s in sizes)
        name = render_name_cell(row_id, descriptions.get(row_id))
        out.append(f"<tr><td>\n\n{name}\n\n</td>{cells}</tr>")
    out.append("</table>")
    out.append("")

    out.append(
        "Each number is Criterion's best estimate of the change. 🔴/🟢 means "
        "Criterion is confident the change is beyond "
        f"±{noise_threshold:.0%}: its whole 95% confidence interval is past "
        "that threshold. A larger number without a mark means the interval "
        f"still reaches back inside ±{noise_threshold:.0%} — hover over a "
        "number to see it. Shared CI runners are noisy, so if the code "
        "didn't touch that path, re-run the workflow before worrying. "
        "Click a number for its full Criterion report."
    )
    if counts["unknown"]:
        out.append(
            "\n⚠️ = couldn't read Criterion's verdict from the report page "
            "(has Criterion changed its wording?)."
        )

    undocumented = [r for r in row_ids if r not in descriptions]
    if undocumented:
        out.append("")
        out.append(
            f"⚠️ **Undocumented benchmarks:** add a `` ### `<id>` `` heading under "
            f'"{DESCRIPTIONS_SECTION.lstrip("# ")}" in `{doc_path}` for: '
            + ", ".join(f"`{r}`" for r in undocumented)
        )

    out.append("")
    out.append(
        '<a href="__REPORT__:report/index.html">Full Criterion report</a> · '
        '<a href="__RUN_URL__">Workflow run</a>'
    )
    return "\n".join(out) + "\n"


def render_skipped():
    return (
        f"{MARKER}\n## Benchmarks: skipped\n\n"
        "This PR no longer changes implementation code under `src/` or "
        "`Cargo.lock` (only unit tests, or nothing there at all), so "
        "benchmarks weren't run.\n"
    )


def render_publish_failed():
    return (
        f"{MARKER}\n## Benchmarks: couldn't publish results\n\n"
        "The benchmarks ran, but posting their results failed. "
        'See the <a href="__RUN_URL__">publish run</a> for what went wrong; '
        "the next push to this PR will try again.\n"
    )


def fill_links(text, report_base, run_url):
    """Replaces the link placeholders with real URLs.

    With a report base, each `__REPORT__:<path>` becomes `<base>/<path>`;
    without one, report links fall back to the run URL (whose artifact
    holds the reports). Plain string replacement, so no character in a
    URL needs escaping.
    """
    if report_base:
        text = text.replace("__REPORT__:", report_base.rstrip("/") + "/")
    elif run_url:
        text = re.sub(r'__REPORT__:[^"]*', lambda _: run_url, text)
    run_url = run_url or report_base
    if run_url:
        text = text.replace("__RUN_URL__", run_url)
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--criterion-dir", type=Path, default=Path("target/criterion"))
    parser.add_argument(
        "--baseline-name",
        default="pr",
        help="Criterion baseline holding the PR's measurements (default: pr; "
        "a local `--baseline <name>` run stores them under `new`).",
    )
    parser.add_argument("--descriptions", default="docs/benchmarking.md")
    parser.add_argument(
        "--noise-threshold",
        type=float,
        default=0.01,
        help="The --noise-threshold the comparison ran with, for the legend "
        "(default: 0.01, Criterion's own default).",
    )
    parser.add_argument("--report-base", help="URL the Criterion report directory is served at.")
    parser.add_argument("--run-url", help="URL of the workflow run that produced the reports.")
    parser.add_argument("--skipped", action="store_true", help="Print the 'skipped' notice.")
    parser.add_argument(
        "--publish-failed",
        action="store_true",
        help="Print the 'couldn't publish' notice (pass the publish run as --run-url).",
    )
    parser.add_argument(
        "--fill",
        type=Path,
        metavar="COMMENT",
        help="Fill in the links of an already-generated comment instead of generating one.",
    )
    args = parser.parse_args()

    if args.fill:
        report = args.fill.read_text(encoding="utf-8")
    elif args.skipped:
        report = render_skipped()
    elif args.publish_failed:
        report = render_publish_failed()
    else:
        rows = collect_results(args.criterion_dir, args.baseline_name)
        if not rows:
            sys.exit(f"No '{args.baseline_name}' benchmark results under {args.criterion_dir}")
        report = render_report(
            rows, parse_descriptions(args.descriptions), args.descriptions, args.noise_threshold
        )

    sys.stdout.write(fill_links(report, args.report_base, args.run_url))


if __name__ == "__main__":
    main()
