# Features

## Overall behavior

- flint checks the current directory by default, or a directory given as
  a command-line argument.
- It recurses through that directory and every subdirectory, looking for
  `.flint` files.
- Each `.flint` file found is checked against the actual contents of its
  own directory.
- If no `.flint` file exists anywhere in the tree, flint prints
  `No flint file found` and exits successfully — linting is opt-in per
  directory.
- Each `.flint` file is read as a plain-text **allow-list**: one name per
  line.
- Anything present on disk that isn't in the allow-list is reported, and
  flint exits with a non-zero status.
- A name listed in `.flint` doesn't have to exist on disk — the list says
  what's *permitted*, not what's *required* — unless it's marked `!`
  (see "Required entries" below).
- Symlinked directories are never followed while searching for `.flint`
  files; dot-named directories (`.git`, `.github`, ...) are searched like
  any other.

The sections below cover each feature `.flint` lines can use. See
[`flint-file-format.md`](flint-file-format.md) for the complete syntax
reference, with a worked example and unit test for every rule.

## Placeholders (`{{ variable }}`)

- A line containing one or more `{{ name }}` placeholders is a
  template: a real name matches if some non-empty value for each
  variable renders it (e.g. `{{ name }}Manager.py` matches
  `HomeManager.py`, with `name` = `Home`). flint works backwards from
  each real name to capture those values.
- The same variable must resolve to the same value everywhere it's used
  *within one matched entry* — e.g. both halves of
  `{{ name }}_{{ name }}.txt`, matched against one file.
- `.flint` is an allow-list: a file is fine if any rule allows it. A rule
  whose variables disagree for a file doesn't allow it, but another rule
  still can (e.g. a literal `README.md` beside `{{ name }}.md`).
- A file no rule allows is reported as unexpected.
- Two *separate* `.flint` lines that happen to reuse the same variable
  name are never reconciled against each other.
- A placeholder with no matching file on disk is fine — allowed doesn't
  mean required.

## Group headers (`<name>`)

- A line holding just `<name>` (or `<name, kind>`) is a group header: it
  matches nothing on disk and adds nothing to the path, but every rule
  indented under it shares its variables.
- In each real folder, every file those rules allow must agree on one
  value — so a plain folder like `src/` can require `{{ name }}.md` and
  `{{ name }}.txt` to match, as `{{ name }}/` already does for its own
  files. The value is found by search, not taken from the first file.
- A disagreement is reported once for the folder, listing each value and
  the files that gave it. A file a rule outside the group allows doesn't
  count, and neither does an entry that gives no value, such as a folder
  a `**/` under the header reaches.
- A folder with none of the group's files passes; once it has a value, a
  `!` rule under the header is required there (so the files are optional,
  but come as a complete set). With several variables, `<a, b>`, a `!`
  rule waits for a value for each one it uses. `!<name>` requires a set
  in every folder.
- Under a `**`, each real folder the `**` reaches has its own value, and
  `!<name>` needs a set in just one of them.
- `<` and `>` are reserved for headers; escape them as `\<` / `\>` in a
  filename.

## Format filters (`|format`)

- A placeholder can carry a `|format` filter (e.g. `{{ name|camel }}`,
  "`name` rendered in camelCase") to constrain it to a specific case
  style.
- Ten formats are supported: `camel`, `pascal`, `snake`, `screamingSnake`,
  `kebab`, `screamingKebab`, `title`, `train`, `dot`, `screamingDot`.
- A file whose name doesn't fit the requested shape simply doesn't match
  — it isn't a special error.
- Occurrences of the same variable under *different* formats are
  recognized as the same identifier, even though they capture different
  literal text (e.g. `foo` via `|camel` and `Foo` via `|pascal`).
- Repeats of the *same* format, within one matched entry, must still
  match literally.
- Within one rule's path, a variable uses a filter everywhere or nowhere;
  separate rules (e.g. `src/{{ name|snake }}.rs` and `tests/{{ name }}.rs`)
  are independent and needn't agree.
- Formats work on letters of any script: `crème_brûlée` is valid
  `|snake`, and the same identifier as `CrèmeBrûlée` via `|pascal`. A
  script with no capitals, like Japanese, fits every format, and so
  does an emoji.

See [`formats.md`](formats.md) for the full details, including how
acronyms, numbers and letters outside English are handled.

## Names in any language

- A rule and a name on disk can be in any language or script, and are
  compared exactly: capitals and accents both count.
- An accented letter matches whether it's stored as one character or as
  a letter and a combining accent (as macOS often stores a file name).
  Names are reported in the one-character form.
- On a file system that ignores case (the default on macOS and
  Windows), flint still compares the name a file was created with, so
  `readme.md` doesn't allow `README.md` — and `!foo` with `!FOO` can
  never both be satisfied there.
- A name that isn't valid Unicode (possible on Linux) stops the run with
  an error naming it, rather than being checked under a changed name.

See [`flint-file-format.md`](flint-file-format.md#names-in-any-language)
for the details.

## Wildcards (`*`)

- `*` matches zero or more characters, never crossing a `/`.
- A leading `*` never matches a dotfile, mirroring shell globs (`*.py`
  doesn't match `.py` or `.hidden.py`).
- `*` can combine with `{{ variable }}` placeholders on the same line.
- Escape a literal `*` in a filename with `\*`.

## Any-depth wildcard (`**`)

- `**` matches zero or more entire path components (e.g. `src/**/*.py`
  permits `.py` files anywhere under `src`, at any depth, including
  directly in `src`).
- It must be the entire content of its path component, and at most one
  is allowed per line.
- Its absorption skips dot-named directories, the same way a leading `*`
  skips dotfiles — except at the very end of a line, where `target/**`
  means everything in `target`, dot names included.

## Required entries (`!`)

- A leading `!` marks a `.flint` line as **required**: at least one match
  on disk is mandatory, not just permitted.
- Composes with everything else — `!src/` requires a directory; `!*.md`
  requires any one matching file; `!{{ name }}Manager.py` requires any
  one structural match.
- A `!` covers every folder its own line names:
  `!{{ name }}/{{ name }}.txt` requires the folder, and the file in each
  such folder — the same as `!{{ name }}/` with `!{{ name }}.txt`
  indented under it. The folders a line is indented under aren't
  required by it; it's checked inside each one that exists.
- A missing required entry is its own reported error, distinct from an
  unexpected file.
- A missing entry's error names a concrete guessed filename (e.g.
  `foo/BlahPresenter.py`) whenever flint already knows enough to render
  its template, rather than raw `{{ }}` syntax. A variable it doesn't
  know, or a `*`, stays as written beside the ones it does
  (`Home/{{ other }}.Home`).

## Nested paths (`/`)

- A rule can reference a nested path with `/` (e.g. `src/util.rs`).
- flint looks inside every directory a rule permits and checks its
  contents too, so a bare `src/` permits only an empty `src` folder, and
  `src/**` permits anything in it. Dot-named folders like `.git/` are no
  exception (`.git/**`).
- A directory some rule reaches into is *implicitly permitted* to exist;
  no separate rule for the directory itself is needed.
- `*` and `{{ variable }}` both work in **any** path component, not just
  the last (e.g. `src/{{ platform }}/main.py`).

## Indentation nesting

- A line can be indented under a preceding directory-kind line (one
  ending in `/`) instead of repeating its full path.
- This is purely a shorthand: it means exactly the same thing as writing
  the full nested path on one line.
- It nests to any depth, and composes with everything else (`{{ variable }}`,
  `*`, `**`, `!`) exactly as the equivalent flat form would.
- Indentation can use spaces or tabs, as long as it nests consistently.

## Multiple `.flint` files (subfolders)

- flint recurses through every subdirectory looking for more `.flint`
  files, checking each one it finds against the actual contents of *its
  own* directory.
- A parent `.flint` file's rule that reaches into a subdirectory with its
  own `.flint` file **composes** with that file's own rules — an entry
  there is permitted if either file permits it.
- An imported rule keeps the values its path captured on the way in:
  inside `Home/`, the root's `{{ name }}/{{ name }}.py` still needs
  `name` to be `Home`, and a missing entry it requires is named
  `Home/Home.py`.
- A missing entry is reported under the `.flint` file whose rule requires
  it, even when the folder it belongs in has a `.flint` file of its own
  — so adding a `.flint` file to a folder, even an empty one, never
  changes what a parent's required rule reports.
- A `<name>` group header is checked once, from its own `.flint` file,
  across every folder below — a nested `.flint` file, even an empty one,
  can't split its files between two checks.
- When more than one `.flint` file is involved in a run, every reported
  error is labeled with the `.flint` file it came from.
- A folder with its own `.flint` file still has to be permitted by the
  enclosing one; its `.flint` file describes what's inside.
- Symlinked directories are never followed while searching for `.flint`
  files. Dot-named directories are searched like any other.

See [`flint-file-format.md`](flint-file-format.md) for the full `.flint`
file format, [`usage.md`](usage.md) for the command-line reference, and
[`../CHANGELOG.md`](../CHANGELOG.md) for release history.

> **Note for contributors/agents:** this page must stay in sync with what
> the tool actually does. See [`AGENTS.md`](../AGENTS.md).
