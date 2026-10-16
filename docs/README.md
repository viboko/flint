# flint documentation

Full documentation for [flint](../README.md), a folder linter.

- [**Playground**](../playground/README.md) — trying flint in the
  browser, and the examples it ships with.
- [**Features**](features.md) — what flint checks today.
- [**Usage**](usage.md) — running flint, command-line arguments,
  `--help` output.
- [**The `.flint` file format**](flint-file-format.md) — the allow-list
  syntax: literals, `{{ variable }}` templates, wildcards, required
  entries, nested paths, escaping, and how multiple `.flint` files in one
  tree interact.
- [**Case-style formats**](formats.md) — the ten `|format` filters
  (`camel`, `pascal`, `snake`, ...) and the word-splitting rules behind
  them.
- [**Test coverage**](coverage.md) — how much of the code each test
  suite exercises, how it's reported on PRs, and how to see it locally.
- [**Mutation testing**](mutation-testing.md) — whether the tests
  would notice a change to the code, run weekly, and how to triage what
  it finds.
- [**Benchmarking**](benchmarking.md) — how flint's performance is
  measured and how to run the benchmarks locally.
- [**The GitHub Pages site**](github-pages.md) — what's published at
  flint.viboko.dev (the playground, and coverage and benchmark reports
  for `main` and each PR), and how.
- [**Fuzzing**](fuzzing.md) — how flint is fuzzed for crashes and hangs,
  the seeds and corpus, and how to run the fuzzer locally.
- [**Architecture**](architecture.md) — how the codebase fits together:
  module responsibilities, dependencies, request flow, and which test
  suites and benchmarks cover which parts.
- [**Tooling**](tooling.md) — every recipe in the [`justfile`](../justfile),
  what it wraps, and why — including generating browsable API docs
  (rustdoc) from this codebase's doc comments.

Looking to contribute? See [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the
repo root, and [`AGENTS.md`](../AGENTS.md) if you're working in this repo
with an AI coding agent.
