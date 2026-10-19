# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).
While the version stays below `1.0.0`, breaking changes may land in any
minor release.

## [Unreleased]

### Changed

- **Breaking:** flint now needs a `.flint` file in the directory it's
  checking. Without one it prints `No flint file found` to standard
  error and exits with status `1`, straight away, rather than searching
  the whole tree and exiting successfully — so running it somewhere
  never meant to be checked, such as a home directory, is quick and
  clear. A tree that only had `.flint` files further down needs one at
  the top too; a single `**` rule allows anything.
- A folder flint can't list now stops the run with an error naming it
  and saying why — `Couldn't list Library/Mail, so nothing was checked:
  Operation not permitted` — rather than `Failed to scan .`, which named
  the folder being checked instead.
- On macOS, when a folder can't be listed because the system keeps it
  private (such as Mail's in `~/Library`), flint says how to give the
  terminal Full Disk Access.
- A `PATH` that doesn't exist, or isn't a folder, is reported as
  `Can't check PATH: ...`.

## [0.1.0] - 2026-10-16

Initial release, installable with Homebrew (`brew install
viboko/tap/flint-cli`), from crates.io (`cargo install flint-cli`), or
as a prebuilt binary attached to the GitHub Release.
