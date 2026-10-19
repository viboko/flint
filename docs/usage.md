# Usage

```console
$ flint --help
Folder linter: validates that files and folders in a project match an allow-list, without looking inside any file.

Usage: flint [PATH]

Arguments:
  [PATH]  Directory to check (defaults to the current directory) [default: .]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Running flint

```bash
flint
flint path/to/project
```

By default, flint checks the current directory. Pass a path as the one
optional argument to check a different directory instead — flint runs
exactly as it would if invoked from inside that directory.

flint looks for files named `.flint` in the directory it's checking and
every subdirectory beneath it, and checks each one it finds against the
actual contents of its own directory. The directory it's checking must
have one: otherwise flint prints `No flint file found` and exits with
status `1`, without searching any further. See
[`docs/flint-file-format.md`](flint-file-format.md) for the full `.flint`
file format, including how multiple `.flint` files in one tree interact.

## Output

```console
$ flint
Missing 1 required entry:
- LICENSE

Found 2 unexpected entries:
- notes.txt
- src/old.rs
```

When nothing is wrong, flint prints `All files match`. Paths are printed
exactly as they are on disk, including a `.flint` file's own path when a
run involves more than one.

Anything flint couldn't do goes to standard error instead: a `.flint`
file it couldn't read or parse, or a folder it couldn't list, naming the
folder and why — which stops the run before anything is checked. flint
exits with status `0` when everything matched, and `1` when anything
didn't or anything failed.

## Command-line arguments

```text
flint [PATH]
```

`PATH` is optional and defaults to `.`, the current directory. When given,
flint checks `PATH` instead — exactly as if it had been invoked from
inside that directory (see "Running flint" above). The standard `--help`
and `--version` flags are also available. See
[`CHANGELOG.md`](../CHANGELOG.md) for what's shipped so far.
