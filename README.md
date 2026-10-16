![flint](docs/images/flint-logo.svg)

# flint

**flint** is a folder linter: a command-line tool that checks whether the
files and folders in a directory are the ones you expect, without looking
inside any of them. Think of it as `.gitignore`, but inverted — instead of
saying what to ignore, you say what's *allowed to exist*, and flint flags
everything else.

The idea is to catch structural drift in a project — stray files, misplaced
build output, accidental commits, typo'd filenames — as a fast, dumb,
language-agnostic check that runs before (or alongside) your real,
language-specific linters.

**[Try it in your browser](https://flint.viboko.dev/)**: the
playground runs flint as you type, with examples of each feature.

See the [full documentation](docs/README.md) for the `.flint` file
format, the command-line reference, and everything else flint checks.
Contributors can browse the internal API reference, built from the code's
doc comments, on [docs.rs](https://docs.rs/flint-cli) (or locally with
`just docs`).

## Example

Given a folder structure like:

```text
kiwi/
  KiwiView.py
  KiwiHandler.py
passion_fruit/
  PassionFruitView.py
  PassionFruitHandler.py
```

flint can validate that whole shape with a single `.flint` file:

```text
{{ page|snake }}/
  {{ page|pascal }}View.py
  {{ page|pascal }}Handler.py
```

Each line is a template: a real name matches if some value of `page`,
rendered in the given case style, produces it. This one rule permits any
number of `{{ page }}` directories, as long as
each is named in snake_case and its `View.py`/`Handler.py` files are named
to match in PascalCase — both `kiwi/` and `passion_fruit/` above satisfy
it, and a new `mango/` directory with its own `MangoView.py`/
`MangoHandler.py` would too, with no changes to `.flint` needed:

```bash
$ flint
All files match
```

See [`docs/flint-file-format.md`](docs/flint-file-format.md) for the full
`.flint` file syntax, including required entries, wildcards, and how
multiple `.flint` files in one tree interact.

## Why not X?

flint isn't the first tool to lint folder structure. Here's how it
compares with the ones you're most likely to have come across:

- **[ls-lint](https://github.com/loeffel-io/ls-lint)** is a fast,
  language-agnostic linter for file and directory *names*. Its rules are
  set per file extension and directory (e.g. `.ts: kebab-case`), so by
  default a file that no rule mentions passes. A name can refer to a
  parent directory's name (`regex:${0}`), but only literally, with no
  conversion between case styles. flint works the other way round:
  anything not listed in `.flint` is an error, and one `{{ variable }}`
  can appear as `snake_case` in a folder name and `PascalCase` in the
  files inside it.
- **[eslint-plugin-project-structure](https://github.com/Igorkowalski94/eslint-plugin-project-structure)**
  is the closest in spirit. Its `folder-structure` rule is an allowlist,
  and names can refer to their parent folder's name in a chosen case
  (`{FolderName}`, `{folderName}`). It's a good fit if your project
  already uses ESLint. flint doesn't need ESLint or Node.js, and has a
  simpler, more readable config format.
- **[folderslint](https://github.com/denisraslov/folderslint)** is also
  an allowlist, but of directory paths only: it checks where files live,
  not what they're called.
- **A `.gitignore` that starts with `*`** and re-includes allowed paths
  with `!` can stop unexpected files from being committed. It does so
  silently, though, with nothing reported, and it can't express naming
  rules.

## Installation

On macOS or Linux, with [Homebrew](https://brew.sh/):

```bash
brew install viboko/tap/flint-cli
```

The formula is called `flint-cli` because Homebrew's own `flint` is an
unrelated maths library; the command it installs is still `flint`.

With Rust 1.85 or newer, from [crates.io](https://crates.io/crates/flint-cli):

```bash
cargo install --locked flint-cli
```

Or download a prebuilt binary from the
[latest release](https://github.com/viboko/flint/releases/latest), for
Linux (x86_64 and arm64), macOS (Apple silicon and Intel) or Windows
(x86_64). Each archive has a `.sha256` checksum beside it, and a signed
build provenance attestation you can check with the
[GitHub CLI](https://cli.github.com/):

```bash
shasum -a 256 -c flint-v0.1.0-aarch64-apple-darwin.tar.gz.sha256
gh attestation verify flint-v0.1.0-aarch64-apple-darwin.tar.gz --repo viboko/flint
```

To build from a checkout of this repository instead, with
[rustup](https://rustup.rs/)'s `cargo`, which fetches the Rust version
the repo pins:

```bash
cargo install --locked --path .
```

## Development

This project uses [`just`](https://github.com/casey/just) as a task
runner. See the [`justfile`](justfile) for available recipes, or run:

```bash
just
```

`just ci` runs everything CI runs; see
[`docs/tooling.md`](docs/tooling.md) for what each check does.
`just coverage` shows how much of the code the tests reach — see
[`docs/coverage.md`](docs/coverage.md) — and `just mutants` whether they'd
notice it changing ([`docs/mutation-testing.md`](docs/mutation-testing.md)).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
