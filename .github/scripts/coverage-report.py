#!/usr/bin/env python3
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Summarizes flint's test coverage, and turns a base-vs-PR comparison of it
into the coverage PR comment.

`just coverage-collect` writes one lcov file and one HTML report per test
suite (unit, e2e, and combined for both together) and then runs this
script with `--summarize`, which reduces them to summary.json: for each suite,
each file under src/'s covered and uncovered line numbers, with the file's
trailing `#[cfg(test)]` module left out — those lines are the unit tests
themselves, which would otherwise count as covered just by running.

The other modes read summary.json files:

- `--summary <summary.json>` prints a per-file table (what `just coverage`
  shows locally).
- `--compare` compares the base commit's summary with the PR's and prints
  the Markdown PR comment, including how many of the lines the PR adds or
  changes under src/ are covered (from `git diff` between the two refs).
  Given the PR's `--report-dir`, each of the PR's percentages links to
  that suite's HTML report for the file.
- `--site <dir>` assembles the HTML reports into <dir> as they're
  published on GitHub Pages (see docs/github-pages.md): each suite's
  report as `<suite>/`, under an index page with the per-file table.
- `--skipped` and `--publish-failed` print the comment's two notices.

Like bench-report.py, the HTML report links are emitted as placeholders
(`__REPORT__:<path>`), because the comment is built in the untrusted,
read-only half of the CI pipeline, which doesn't know where the reports
will end up hosted. .github/workflows/coverage-publish.yml fills them in
with `--fill <comment file> --report-base <url>`, or, when the reports
aren't hosted, points them at the workflow run (`--run-url`), whose
artifact holds the reports.

See docs/coverage.md. Standard library only, so it runs on a stock CI
runner with no setup.
"""

import argparse
import html
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path
from urllib.parse import quote

MARKER = "<!-- flint-coverage-report -->"
SUITES = {"unit": "Unit tests", "e2e": "End-to-end tests", "combined": "Combined"}


# --- Summarizing lcov ---------------------------------------------------------


def test_module_start(source_path):
    """Returns the line number of the file's top-level `#[cfg(test)]`, if any.

    Same convention as .github/scripts/bench-gate.sh: unit tests live in a
    trailing `#[cfg(test)] mod tests { ... }`, so everything from that line
    on is test code.
    """
    try:
        lines = Path(source_path).read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for number, line in enumerate(lines, start=1):
        if line == "#[cfg(test)]":
            return number
    return None


def parse_lcov(lcov_path, root):
    """Returns {relative path: {line number: hit count}} for files under root."""
    files = {}
    current = None
    for line in Path(lcov_path).read_text(encoding="utf-8").splitlines():
        if line.startswith("SF:"):
            source = Path(line[3:])
            try:
                current = files.setdefault(source.relative_to(root).as_posix(), {})
            except ValueError:
                current = None  # outside the repo, e.g. a dependency
        elif line.startswith("DA:") and current is not None:
            number, count = line[3:].split(",")[:2]
            number = int(number)
            # A line can appear more than once (e.g. once per
            # monomorphization); it's covered if any of them ran.
            current[number] = max(current.get(number, 0), int(count))
        elif line == "end_of_record":
            current = None
    return files


def summarize(report_dir):
    root = Path.cwd().resolve()
    summary = {}
    for suite in SUITES:
        files = parse_lcov(Path(report_dir) / f"{suite}.lcov", root)
        summary[suite] = {}
        for path, hits in sorted(files.items()):
            # Only the code under test: not tests/robot.rs, the wrapper
            # that runs the e2e suite for mutation testing.
            if not path.startswith("src/"):
                continue
            cutoff = test_module_start(root / path)
            lines = {n: c for n, c in hits.items() if cutoff is None or n < cutoff}
            if lines:
                summary[suite][path] = {
                    "covered": sorted(n for n, c in lines.items() if c > 0),
                    "uncovered": sorted(n for n, c in lines.items() if c == 0),
                }
    return summary


# --- Shared helpers -----------------------------------------------------------


def counts(file_entry):
    """Returns (covered, total) for one file's entry, or (0, 0) if absent."""
    if file_entry is None:
        return 0, 0
    covered = len(file_entry["covered"])
    return covered, covered + len(file_entry["uncovered"])


def suite_totals(suite_summary):
    covered = total = 0
    for entry in suite_summary.values():
        c, t = counts(entry)
        covered += c
        total += t
    return covered, total


def percent(covered, total):
    return 100 * covered / total if total else None


def format_percent(covered, total):
    value = percent(covered, total)
    return "–" if value is None else f"{value:.1f}%"


def report_link(text, page):
    """Links text to a page of the HTML reports, if there is one."""
    if page is None:
        return text
    return f'<a href="__REPORT__:{page}">{text}</a>'


# --- Local summary ------------------------------------------------------------


def render_summary(summary):
    paths = sorted({p for suite in summary.values() for p in suite})
    rows = [["File", *SUITES.values()]]
    for path in paths:
        rows.append([path, *(format_percent(*counts(summary[s].get(path))) for s in SUITES)])
    totals = [suite_totals(summary[s]) for s in SUITES]
    rows.append(["Total", *(f"{format_percent(*t)} ({t[0]}/{t[1]} lines)" for t in totals)])

    widths = [max(len(row[i]) for row in rows) for i in range(len(rows[0]))]
    out = []
    for index, row in enumerate(rows):
        cells = [row[0].ljust(widths[0])] + [
            c.rjust(w) for c, w in zip(row[1:], widths[1:], strict=True)
        ]
        out.append("  ".join(cells))
        if index in (0, len(rows) - 2):
            out.append("  ".join("-" * w for w in widths))
    out.append("")
    out.append("Line coverage, excluding each file's #[cfg(test)] module.")
    out.append("HTML report (combined): target/coverage/report/combined/html/index.html")
    return "\n".join(out) + "\n"


# --- PR comment ---------------------------------------------------------------


def changed_lines(base_ref, head_ref):
    """Returns {path: set of line numbers} added or changed under src/."""
    diff = subprocess.run(
        ["git", "diff", "-U0", "--no-color", "--no-ext-diff", base_ref, head_ref, "--", "src/"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    changed = {}
    path = None
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif line.startswith("@@") and path is not None:
            match = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", line)
            start, length = int(match.group(1)), int(match.group(2) or 1)
            changed.setdefault(path, set()).update(range(start, start + length))
    return changed


def line_ranges(numbers):
    """[3, 4, 5, 9] -> [(3, 5), (9, 9)]."""
    ranges = []
    for n in sorted(numbers):
        if ranges and n == ranges[-1][1] + 1:
            ranges[-1] = (ranges[-1][0], n)
        else:
            ranges.append((n, n))
    return ranges


def file_link(path, blob_url, start=None, end=None):
    text = f"<code>{path}</code>"
    if start is not None:
        text = f"{start}" if start == end else f"{start}–{end}"
    if not blob_url:
        return text
    anchor = ""
    if start is not None:
        anchor = f"#L{start}" if start == end else f"#L{start}-L{end}"
    return f'<a href="{blob_url.rstrip("/")}/{path}{anchor}">{text}</a>'


def change_cell(base, pr):
    """Formats base → PR for one (covered, total) pair, marking any drop 🔴."""
    base_pct, pr_pct = percent(*base), percent(*pr)
    if pr_pct is None:
        return f"{format_percent(*base)} → removed" if base_pct is not None else "–"
    if base_pct is None:
        return f"{format_percent(*pr)} (new)"
    # Compare exactly (cross-multiplied), not the rounded percentages, so
    # even a drop too small to show at one decimal place is flagged.
    drop = pr[0] * base[1] < base[0] * pr[1]
    rise = pr[0] * base[1] > base[0] * pr[1]
    delta = pr_pct - base_pct
    if drop:
        shown = f"{delta:.1f}" if delta <= -0.05 else "-<0.1"
        return f"🔴 {format_percent(*pr)} ({shown})"
    if rise:
        shown = f"+{delta:.1f}" if delta >= 0.05 else "+<0.1"
        return f"🟢 {format_percent(*pr)} ({shown})"
    return format_percent(*pr)


def report_pages(report_dir, paths):
    """Returns {suite: {path or None: page}} for the PR's HTML reports.

    Each page is relative to a directory holding each suite's `html/`
    report as `<suite>/`, which is how the CI pipeline publishes them; the
    None entry is the suite's index. llvm-cov names each file's page after
    its absolute path, so this runs from the repo root, like --summarize.
    """
    root = Path.cwd().resolve().as_posix().lstrip("/")
    pages = {}
    for suite in SUITES:
        html_dir = Path(report_dir) / suite / "html"
        if not (html_dir / "index.html").is_file():
            continue
        pages[suite] = {None: f"{suite}/index.html"}
        for path in paths:
            page = f"coverage/{root}/{path}.html"
            if (html_dir / page).is_file():
                pages[suite][path] = quote(f"{suite}/{page}")
    return pages


def render_comparison(base, pr, changed, blob_url, run_url, pages):
    out = [MARKER, "## Test coverage: base vs. this PR", ""]

    totals = {s: (suite_totals(base[s]) if base else None, suite_totals(pr[s])) for s in SUITES}
    dropped = [
        SUITES[s] for s, (b, p) in totals.items() if b is not None and p[0] * b[1] < b[0] * p[1]
    ]
    combined_pr = totals["combined"][1]
    if base is None:
        out.append(
            f"**Combined line coverage: {format_percent(*combined_pr)}.** No coverage "
            "data from the base commit to compare against — see the workflow run."
        )
    elif dropped:
        out.append("**🔴 Coverage dropped:** " + ", ".join(dropped).lower() + ".")
    else:
        out.append("**No drop in coverage.**")
    out.append("")

    # Lines the PR adds or changes that the tests can reach at all (blank
    # lines, comments etc. aren't instrumented, so don't count).
    combined = pr["combined"]
    new_covered, new_uncovered = 0, {}
    for path, numbers in sorted(changed.items()):
        entry = combined.get(path)
        if entry is None:
            continue
        new_covered += len(numbers & set(entry["covered"]))
        missed = numbers & set(entry["uncovered"])
        if missed:
            new_uncovered[path] = missed
    new_total = new_covered + sum(len(m) for m in new_uncovered.values())
    if new_total:
        out.append(
            f"Lines this PR adds or changes under `src/`: **{new_covered} of {new_total} "
            f"covered** ({format_percent(new_covered, new_total)}) by the two suites combined."
        )
    else:
        out.append("This PR doesn't add or change any instrumented lines under `src/`.")
    out.append("")

    out.append("| Suite | Base | This PR |")
    out.append("|---|---:|---:|")
    for suite, (b, p) in totals.items():
        base_cell = format_percent(*b) if b is not None else "–"
        pr_cell = change_cell(b, p) if b is not None else format_percent(*p)
        pr_cell = report_link(pr_cell, pages.get(suite, {}).get(None))
        out.append(f"| {SUITES[suite]} | {base_cell} | {pr_cell} |")
    out.append("")

    if new_uncovered:
        out.append("<details><summary>Changed lines no test covers</summary>")
        out.append("")
        for path, missed in new_uncovered.items():
            ranges = ", ".join(file_link(path, blob_url, a, b) for a, b in line_ranges(missed))
            out.append(f"- {file_link(path, None)}: {ranges}")
        out.append("")
        out.append("</details>")
        out.append("")

    paths = {p for s in SUITES for p in pr[s]}
    if base:
        paths |= {p for s in SUITES for p in base[s]}
    paths = sorted(paths)

    def row(path):
        cells = []
        for s in SUITES:
            p = counts(pr[s].get(path))
            cell = change_cell(counts(base[s].get(path)), p) if base else format_percent(*p)
            cells.append(report_link(cell, pages.get(s, {}).get(path)))
        return f"| {file_link(path, blob_url)} | " + " | ".join(cells) + " |"

    header = ["| File | " + " | ".join(SUITES.values()) + " |", "|---|" + "---:|" * len(SUITES)]
    if base:
        moved = [
            p for p in paths if any(counts(base[s].get(p)) != counts(pr[s].get(p)) for s in SUITES)
        ]
        if moved:
            out.append("Files whose coverage changed:")
            out.append("")
            out.extend(header)
            out.extend(row(p) for p in moved)
            out.append("")
    out.append("<details><summary>All files</summary>")
    out.append("")
    out.extend(header)
    out.extend(row(p) for p in paths)
    out.append("")
    out.append("</details>")
    out.append("")

    links = "This PR's percentages link to that suite's HTML report, which shows which lines ran. "
    out.append(
        "Line coverage measured with cargo-llvm-cov, leaving out each file's "
        "`#[cfg(test)]` module. The end-to-end suite counts the lines the "
        "Robot Framework tests reach by running the binary. 🔴/🟢 mark any "
        "drop or rise from the base commit — coverage is deterministic, so "
        "unlike the benchmarks, even a small change is real. "
        f"{links if pages else ''}"
        f'<a href="{run_url}">Workflow run</a>.'
    )
    return "\n".join(out) + "\n"


SITE_STYLE = """
  :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
  body { max-width: 60rem; margin: 2rem auto; padding: 0 1rem; }
  table { border-collapse: collapse; width: 100%; }
  th, td { padding: 0.3rem 0.6rem; border-bottom: 1px solid #8884; }
  td:not(:first-child), th:not(:first-child) { text-align: right; }
  tfoot td { font-weight: bold; }
"""


def build_site(report_dir, site_dir, title):
    """Copies each suite's HTML report into site_dir/<suite>/, under an index.

    The index is the same per-file table `--summary` prints, each number
    linking to that suite's page for the file. Runs from the repo root,
    like --summarize, to find those pages.
    """
    summary = load_summary(Path(report_dir) / "summary.json")
    if summary is None:
        sys.exit(f"No coverage summary in {report_dir}")
    site_dir = Path(site_dir)
    site_dir.mkdir(parents=True, exist_ok=True)
    for suite in SUITES:
        shutil.copytree(Path(report_dir) / suite / "html", site_dir / suite, dirs_exist_ok=True)

    paths = sorted({p for s in SUITES for p in summary[s]})
    pages = report_pages(report_dir, paths)

    def cell(covered, total, page):
        text = html.escape(format_percent(covered, total))
        if page is not None:
            text = f'<a href="{page}">{text}</a>'
        return f"<td>{text}</td>"

    rows = []
    for path in paths:
        cells = "".join(
            cell(*counts(summary[s].get(path)), pages.get(s, {}).get(path)) for s in SUITES
        )
        rows.append(f"<tr><td><code>{html.escape(path)}</code></td>{cells}</tr>")
    totals = "".join(cell(*suite_totals(summary[s]), pages.get(s, {}).get(None)) for s in SUITES)
    headers = "".join(f"<th>{html.escape(name)}</th>" for name in SUITES.values())
    title = html.escape(title)
    page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title}</title>
<style>{SITE_STYLE}</style>
</head>
<body>
<h1>{title}</h1>
<p>Line coverage of flint's unit tests, its end-to-end tests and both
combined, leaving out each file's <code>#[cfg(test)]</code> module. Each
number links to that suite's report, which shows which lines ran.</p>
<table>
<thead><tr><th>File</th>{headers}</tr></thead>
<tbody>
{chr(10).join(rows)}
</tbody>
<tfoot><tr><td>Total</td>{totals}</tr></tfoot>
</table>
</body>
</html>
"""
    (site_dir / "index.html").write_text(page, encoding="utf-8")


def render_skipped():
    return (
        f"{MARKER}\n## Test coverage: skipped\n\n"
        "This PR no longer changes anything that affects coverage (code or tests "
        "under `src/` and `e2e/`, `Cargo.toml`/`Cargo.lock`, the `justfile`, or "
        "the coverage tooling itself), so coverage wasn't measured.\n"
    )


def render_publish_failed(run_url):
    return (
        f"{MARKER}\n## Test coverage: couldn't publish results\n\n"
        f'Coverage was measured, but posting it failed. See the <a href="{run_url}">'
        "publish run</a> for what went wrong; the next push to this PR will try again.\n"
    )


def fill_links(text, report_base, run_url):
    """Replaces the `__REPORT__:<path>` link placeholders with real URLs.

    With a report base, each becomes `<base>/<path>`; without one, they
    fall back to the run URL (whose artifact holds the reports). Plain
    string replacement, so no character in a URL needs escaping. The same
    as bench-report.py's.
    """
    if report_base:
        return text.replace("__REPORT__:", report_base.rstrip("/") + "/")
    if run_url:
        return re.sub(r'__REPORT__:[^"]*', lambda _: run_url, text)
    return text


def load_summary(path):
    if path is None or not Path(path).is_file():
        return None
    return json.loads(Path(path).read_text(encoding="utf-8"))


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--summarize",
        metavar="REPORT_DIR",
        type=Path,
        help="Reduce REPORT_DIR's lcov files to summary JSON (run from the repo root).",
    )
    mode.add_argument("--summary", metavar="SUMMARY", type=Path, help="Print a per-file table.")
    mode.add_argument("--compare", action="store_true", help="Print the PR comment.")
    mode.add_argument(
        "--fill",
        metavar="COMMENT",
        type=Path,
        help="Fill in the report links of an already-generated comment.",
    )
    mode.add_argument(
        "--site",
        metavar="SITE_DIR",
        type=Path,
        help="Assemble --report-dir's HTML reports under an index page in SITE_DIR.",
    )
    mode.add_argument("--skipped", action="store_true", help="Print the 'skipped' notice.")
    mode.add_argument(
        "--publish-failed",
        action="store_true",
        help="Print the 'couldn't publish' notice (pass the publish run as --run-url).",
    )
    parser.add_argument("--base", type=Path, help="The base commit's summary.json (--compare).")
    parser.add_argument("--pr", type=Path, help="The PR's summary.json (--compare).")
    parser.add_argument("--base-ref", help="Git ref of the base commit (--compare).")
    parser.add_argument("--head-ref", default="HEAD", help="Git ref of the PR (--compare).")
    parser.add_argument("--blob-url", help="URL that the PR's files are viewable under.")
    parser.add_argument(
        "--report-dir",
        type=Path,
        help="The report directory, whose <suite>/html reports to link to (--compare, --site).",
    )
    parser.add_argument("--title", default="flint test coverage", help="Index title (--site).")
    parser.add_argument("--report-base", help="URL the reports are served at (--fill).")
    parser.add_argument("--run-url", default="", help="URL of the workflow run.")
    args = parser.parse_args()

    if args.summarize:
        json.dump(summarize(args.summarize), sys.stdout, indent=1)
        sys.stdout.write("\n")
    elif args.summary:
        sys.stdout.write(render_summary(load_summary(args.summary)))
    elif args.compare:
        pr = load_summary(args.pr)
        if pr is None:
            sys.exit(f"No PR coverage summary at {args.pr}")
        changed = changed_lines(args.base_ref, args.head_ref) if args.base_ref else {}
        paths = {p for s in SUITES for p in pr[s]}
        pages = report_pages(args.report_dir, paths) if args.report_dir else {}
        report = render_comparison(
            load_summary(args.base), pr, changed, args.blob_url, args.run_url, pages
        )
        sys.stdout.write(report)
    elif args.fill:
        text = args.fill.read_text(encoding="utf-8")
        sys.stdout.write(fill_links(text, args.report_base, args.run_url))
    elif args.site:
        if not args.report_dir:
            sys.exit("--site needs --report-dir")
        build_site(args.report_dir, args.site, args.title)
    elif args.skipped:
        sys.stdout.write(render_skipped())
    else:
        sys.stdout.write(render_publish_failed(args.run_url))


if __name__ == "__main__":
    main()
