# Contributing to flint

Thanks for considering a contribution. flint is a small tool with one
maintainer, so it uses the standard GitHub fork-and-pull-request
workflow — anyone can propose a change without needing write access to
the repository first.

## Making a change

1. **Fork the repository** and clone your fork.
2. **Branch off an up-to-date `main`**:
   `git checkout main && git pull && git checkout -b <descriptive-name>`.
3. **Make your change.** See [`AGENTS.md`](AGENTS.md) for this project's
   conventions — code shape, the unit-test/e2e-test split, and linting
   expectations. Despite the name, it's written for any contributor, not
   just AI coding agents.
4. **Keep the docs honest.** If your change affects what flint actually
   does, update [`docs/features.md`](docs/features.md),
   [`docs/flint-file-format.md`](docs/flint-file-format.md) and/or
   [`docs/usage.md`](docs/usage.md) (whichever applies), and add a bullet
   to [`CHANGELOG.md`](CHANGELOG.md) under `[Unreleased]`, in the same
   change. A stale README or docs page is treated as a bug.
5. **Run `just ci`** — every static check in `just lint`, plus the
   unit tests, the Robot Framework end-to-end suite, the
   oldest-dependencies build and the crate package check — and confirm
   it's green. [`docs/tooling.md`](docs/tooling.md) lists what each
   recipe does, or run `just` to list them. `just install-tools` sets up
   every tool it needs at the versions CI uses (see
   [Tool versions](docs/tooling.md#tool-versions)).
6. **Push your branch and open a pull request** against this repository's
   `main`. Describe *why* the change is needed, not just what it does.

## Reporting security issues

Please don't open a public issue or pull request for a security
vulnerability — see [`SECURITY.md`](SECURITY.md) for how to report one
privately.

## License

flint is dual-licensed under [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT), at the user's option. By submitting a
contribution, you agree it's licensed under the same terms — no separate
Contributor License Agreement is required. Each file says so in an SPDX
header at its top (see [`AGENTS.md`](AGENTS.md#license-headers)); the
license texts are in [`LICENSES/`](LICENSES/), in the layout the
[REUSE](https://reuse.software) spec describes.
