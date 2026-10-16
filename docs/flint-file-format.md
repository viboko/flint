# The `.flint` file format

`flint` looks for files named `.flint` in the directory it's checking (the
current directory by default, or a given path — see
[`docs/usage.md`](usage.md)) and every subdirectory beneath it (see
"Multiple `.flint` files" below for the full recursive behavior). If none
exist anywhere, flint prints `No flint file found` and exits successfully —
folder linting is opt-in per directory.

Each `.flint` file found is read independently, as a plain-text
**allow-list**: one name per line, matching the names of files and folders
inside *that* `.flint` file's own directory. A rule can reference a nested
path with `/` (e.g. `src/util.rs`), and flint looks inside every folder a
rule allows, checking its contents too — so a bare `src/` allows only an
empty `src` folder, and `src/**` allows anything in it (see "Rules" below
for the full nested-path behavior).

```text
# .flint for a small project
Cargo.toml
Cargo.lock
src/
README.md
.git/
```

## Rules

- **One name per line.** Each non-empty line (after comments are stripped)
  is an allowed name.
- **Blank lines are ignored**, including lines that only contain
  whitespace.
- **Trailing whitespace is trimmed** from each name. Leading whitespace is
  trimmed too, *unless* it's indentation nesting the line under a
  preceding directory (see "Indentation nests a path under a preceding
  directory" below) — a line indented with nothing valid to nest under is
  a parse error, not silently trimmed.
- **`#` starts a comment** that runs to the end of the line:

  ```text
  Cargo.toml   # the package manifest
  ```

- **A line containing `{{ variable }}` placeholders is a template.**
  Rather than naming one file, it describes a whole family of names: a
  real file or folder matches the rule if there's some value for each
  variable that, substituted into the template, renders exactly that
  name. e.g.:

  ```text
  {{ name }}Manager.py
  ```

  matches `HomeManager.py`, because substituting `name` = `Home`
  renders `HomeManager.py` — and in general matches any file where
  `Manager.py` is preceded by one or more characters. The difference
  from a templating engine is direction: flint is never handed the
  variables' values up front. It works backwards from each real name to
  find values that would render it (this is what "capturing" a value
  means throughout these docs), and only runs the template forwards
  when it has values in hand — see "A missing entry's report names a
  concrete guessed filename" below. Whitespace
  inside the braces is flexible — `{{name}}`, `{{ name }}`, and
  `{{  name  }}` are all equivalent. Variable names must start with a
  letter or underscore and contain only letters, digits, and underscores.
  A matched value is never empty. A line can use more than one
  placeholder, e.g. `{{ name }}Manager.{{ extension }}`.
- **The same variable name must resolve to the same value *within a
  single matched entry* — never across two different `.flint` lines.**
  `{{ variable }}` consistency is checked per match, not per file: it
  applies to multiple occurrences of the same variable captured from *one*
  real file or path, and nowhere else. A variable used twice on one line
  (`{{ name }}_{{ name }}.txt`) must resolve consistently for the one file
  it matches — `Home_Home.txt` passes, but that rule doesn't allow
  `Home_Details.txt`. If no other rule allows it either, it's reported as
  unexpected (see "What counts as an error" below). The same applies
  across a single *nested* path's components: `{{ name }}/{{ name }}Manager.py`
  requires its own directory and filename components to agree for
  whichever one real path it matches (see "A `/` inside a rule references
  a nested path" below) — so `Apple/AppleManager.py` passes, but that
  rule doesn't allow a stray `Apple/BananaManager.py`.

  Two *separate* `.flint` lines that happen to reuse the same variable
  name are never reconciled against each other, no matter how they're
  laid out. `.flint` containing

  ```text
  {{ name }}.py
  {{ name }}.md
  ```

  permits `foo.py` and `foo.md` to coexist, but is equally happy with
  `foo.py` and `bar.md` together — the two lines don't share a real
  directory, so there's nothing forcing them to agree, and a mismatch is
  not an error. This is deliberate: a variable's value only ever needs to
  agree with *itself*, for the one real entry it was captured from. The
  only ways to tie two separate rules to the same concrete value are
  nesting them under a shared directory-capturing rule (see "Indentation
  nests a path under a preceding directory" below) — since that forces
  both rules to be checked against the same real directory by
  construction — or under a `<name>` group header (see "A group header
  shares a variable across the rules under it" below).

  A placeholder with no matching file on disk is fine, same as a literal
  name with no matching file — allowed doesn't mean required.
- **A `|format` filter constrains a placeholder to a case style, and
  reconciles differently-cased occurrences of the same variable.** Read as
  a template, `{{ name|camel }}` means "`name`, rendered in camelCase" —
  so writing it instead of `{{ name }}` requires the matched text to
  be shaped like `camelCase`; the ten recognized filters are `camel`,
  `pascal`, `snake`, `screamingSnake`, `kebab`, `screamingKebab`, `title`,
  `train`, `dot`, and `screamingDot`. A file whose name doesn't fit the
  requested shape simply doesn't match
  that placeholder (falling through to unexpected, same as any other
  structural mismatch), rather than producing a special error. The
  same-match consistency rule above extends across formats too:
  `{{ name|camel }}` and `{{ name|pascal }}` are recognized as the *same*
  identifier even though they capture different literal text (`foo` vs
  `Foo`) — they're two renderings of one underlying value — and two
  occurrences using the *same* format must still match literally — this
  is what lets a nested rule like `{{ name|camel }}/`
  wrapping `{{ name|pascal }}Manager.py` require its directory and
  filename to agree despite being spelled in different cases. Within one
  rule's path — a single line, plus any directory lines it's indented
  under — a variable must use a `|format` filter at every occurrence, or
  none: mixing `{{ name }}` and `{{ name|camel }}` there is a parse
  error. Separate rules are independent, so `src/{{ name|snake }}.rs`
  and `tests/{{ name }}.rs` (flat or nested under `src/` and `tests/`)
  are fine together. See [`docs/formats.md`](formats.md) for the full
  list of formats, worked examples, and how acronyms/numbers are handled.
- **`*` matches zero or more characters, never crossing a `/`.** A line may
  contain one or more `*` wildcards, e.g.:

  ```text
  *.py
  ```

  This matches any file ending in `.py`, e.g. `Home.py` — but when
  the `*` is the first thing on the line, a matched name is never allowed
  to start with `.`, the same way shell wildcards skip dotfiles. So
  `*.py` does *not* match `.py` or `.hidden.py`, even though both
  end in `.py`. A rule like `.git*`, which starts with a literal `.`
  rather than a `*`, is unaffected by this and matches `.gitignore` and
  `.git` just fine. `*` can be combined with `{{ variable }}` placeholders
  on the same line (e.g. `{{ name }}Manager*.py`).
- **A `/` inside a rule references a nested path.** `src/util.rs` permits
  a file `util.rs` inside a directory `src`. A directory a rule's path
  reaches through ("governs") is *implicitly permitted* to exist:
  `src/util.rs` alone is enough, no separate `src/` rule is needed.
- **flint looks inside every folder a rule permits**, and reports anything
  in it no rule allows, exactly as if that folder were the top level. So a
  bare `src/`, with nothing reaching inside it, permits only an *empty*
  `src` folder; `src/**` permits anything in it (see `**` below). A folder
  no rule permits is reported as unexpected on its own, without its
  contents. The same goes for dot-named folders like `.git/` and
  `.github/` — they're part of the project too, so `.git/` alone means an
  empty `.git`, and `.git/**` permits whatever git puts there. `*` and
  `{{ variable }}` both work in **any** path component, not just the
  last:

  ```text
  src/*/main.py
  src/{{ platform }}/main.py
  ```

  The first permits any single subdirectory of `src` to contain
  `main.py`; the second does too, but also captures the subdirectory
  name as `platform` — a value that's only ever compared against *itself*
  within one nested path, per the consistency rule described above:
  `{{ platform }}/main.py` and `{{ platform }}/config.py` are two
  separate lines, so `src/linux/main.py` and `src/macos/config.py` can
  coexist just fine, each independently capturing its own `platform`.
  Dotfile exclusion for a leading `*` applies per path component, not
  just to the whole line — `*/main.py` does not match
  `.hidden/main.py`.
- **`**` matches zero or more entire path components.** It must be the
  entire content of its path component — it can't blend with literal text,
  `*`, or `{{ variable }}` in the same component (`pre**post` is a parse
  error), and at most one `**` is allowed per line. e.g.:

  ```text
  src/**/*.py
  ```

  permits `.py` files anywhere under `src`, at any depth, including
  directly in `src` itself (zero directories absorbed by `**`).
  **`**`'s absorption skips dot-named directories** (`.git`, `.venv`,
  ...), the same way a leading `*` skips dotfiles — `src/**/*.py` will
  never descend into a stray `.git/` under `src`, and that folder is
  reported as unexpected rather than silently permitted. This only
  applies to what `**` *implicitly* absorbs: an explicit literal prefix
  like `.git/**` is unaffected, the same way `.git*` is already
  unaffected for `*`. And **a `**` at the very end of a line means the
  whole folder**: `target/**` permits everything inside `target`, dot
  names included (`target/.rustc_info.json`,
  `target/debug/.fingerprint/`), since there's no other way to say so. `!`
  composes as expected: `!src/**/*.py` is satisfied by any one
  `.py` file anywhere under `src`.
- **Indentation nests a path under a preceding directory**, as a
  convenience for avoiding repetition — this is purely a different way to
  *write* the same rules described above, not a new capability. A line
  indented under a preceding line that ends in `/` is exactly equivalent
  to writing that preceding line's path as a prefix on the indented line:

  ```text
  src/
    util.rs
  ```

  means exactly the same thing as `src/util.rs` on one line, and nests to
  any depth:

  ```text
  {{ name }}/
    {{ name }}Manager.py
    {{ name }}Presenter.py
  ```

  means exactly the same thing as
  `{{ name }}/{{ name }}Manager.py` and
  `{{ name }}/{{ name }}Presenter.py` on two separate lines — each one
  its own nested path, so `name` agrees between a directory and its own
  children (same rule as above), but two different `{{ name }}`
  directories (e.g. `Apple/` and `Banana/`) never have to agree with each
  other. This is what makes it possible to describe many repeated,
  identically-shaped folders in one short rule:

  ```text
  {{ name }}/
    !{{ name }}Manager.py
    !{{ name }}Presenter.py
  ```

  permits any number of `{{ name }}` directories, and requires **each
  one independently** to contain both a `{{ name }}Manager.py` and a
  `{{ name }}Presenter.py` of its own — a required entry nested under
  a directory-capturing variable is checked once per real matching
  directory, not "somewhere in the whole tree." So

  ```text
  Apple/
    AppleManager.py
    ApplePresenter.py
  Banana/
    BananaManager.py
  ```

  reports `Banana/BananaPresenter.py` as missing even though `Apple/`
  has its own `ApplePresenter.py` — `Apple/`'s files don't satisfy
  `Banana/`'s obligation. (This per-instance required check only applies
  to a template whose path reaches through a governed directory; a
  required entry with no directory component, like `!README.md`, and a
  required `**` entry, like `!src/**/*.py`, both keep the simpler "at
  least one match anywhere" behavior, since there's no single directory
  instance to check them within.)

  A `!` covers every folder its own line names, not just the last
  component: `!{{ name }}/{{ name }}.txt` is short for

  ```text
  !{{ name }}/
    !{{ name }}.txt
  ```

  so the folder is required, and so is the file in each such folder.
  Indented under `{{ name }}/`, that line makes an empty `blah/` fail,
  with `blah/blah/blah.txt` reported as missing: `blah/` has no `blah/`
  folder of its own. The folders a line is indented under are a
  different matter — the line doesn't require them, and is only checked
  inside the ones that exist. Every marker composes per-line,
  independent of the header it's nested under — nesting doesn't propagate
  `!` from a header to its children or back: marking the header itself
  required (`!{{ name }}/`) requires at least one matching directory to
  exist, independent of whether any child matched, and independent of the
  children's own per-instance requiredness.
  Indentation can use spaces or tabs, and different blocks in the same
  file can use different widths (2 spaces here, 4 there) — the only
  requirement is that it nests *consistently*: each line's leading
  whitespace must extend (as a literal prefix) the leading whitespace of
  the line it nests under, and a dedent must land exactly on some
  enclosing line's indentation, the same way YAML or Python require.
  Blank and comment-only lines don't affect nesting at any indentation. A
  line still needs its own trailing `/` to take indented children —
  indentation alone never implies one, so indenting under a line with no
  trailing `/` is a parse error, as is indentation that doesn't cleanly
  nest or dedent.
- **A group header shares a variable across the rules under it.** A
  folder like `{{ name }}/` ties its files' names to its own, but a plain
  folder like `src/` captures nothing for them to agree with. A line
  holding just `<name>` — a *group header* — fills that gap. It matches
  nothing on disk and adds nothing to the path; the lines indented under
  it stay in the same folder, but share `name`:

  ```text
  src/
    <name>
      {{ name }}.md
      {{ name }}.txt
  ```

  In each real folder, every file those rules allow must agree on one
  value for `name`. Nothing anchors it — the value is whatever lets
  every such file fit, whichever order the files and rules come in — so
  `src/blah.md`, `src/blah.txt`, or both together pass, while
  `src/blah.md` with `src/foo.txt` (or with `src/foo.md`) fails.
  There's exactly one value per folder: `monkey.md`, `monkey.txt`,
  `zebra.md` and `zebra.txt` together fail too, and a folder per set is
  the way to allow several. Since no single file is to blame, a
  disagreement is reported once for the folder, listing each value
  found and the files that gave it:

  ```text
  src/<name> needs one value per folder, but found 2 in src/:
  - "blah": src/blah.md, src/blah.txt
  - "foo": src/foo.md
  ```

  Only entries that give a value are listed: a folder a `**/` under the
  header merely reaches fits whatever the value is, so it's no part of
  the disagreement.

  A file some rule *outside* the group allows doesn't count toward the
  value — it's already fine — so a `README.md` line beside the group
  lets `src/README.md` sit next to `src/blah.md`. Rules indented further
  under the header share its value too, so `<name>` over both
  `{{ name }}/` (with `{{ name }}.py` inside) and `{{ name }}.md`
  requires `Home/Home.py` to go with `Home.md`. A header under a
  templated folder like `{{ dir }}/` has one value per real `{{ dir }}`
  folder. A header can declare several variables, `<name, kind>`, each
  shared the same way. Different `|format` filters for the variable work
  across the group's rules (`{{ name|snake }}.rs` with
  `{{ name|pascal }}Test.rs` pairs `home_page.rs` with
  `HomePageTest.rs`), but because the rules share the variable, mixing a
  filter with no filter under one header is a parse error.

  A folder holding none of the group's files has no value, and passes.
  Once it has one, a `!` rule directly under the header is required
  there, named with that value: with `!{{ name }}.md` and
  `!{{ name }}.txt`, `src/blah.md` alone reports `src/blah.txt` missing,
  while an empty `src/` is fine — so the files are optional, but come as
  a complete set. Under a header with several variables, each one gets
  its value the same way, and a `!` rule waits for a value for every
  one it uses: under `<a, b>`, `src/foo.txt` alone requires
  `!{{ a }}.jpg` as `src/foo.jpg`, but nothing has given `b` a value
  yet, so `!{{ b }}.md` isn't required. To require a set in every folder
  the header sits in,
  mark the header itself, `!<name>`: an empty `src/` then reports
  `src/<name>` missing. A folder whose files disagree reports just the
  disagreement. A missing file's name also uses what the folder's own
  name says: under `{{ name|snake }}/`, a `!{{ name|title }}.md` missing
  from `blah/` is reported as `blah/Blah.md`, the same as it is with no
  header in between.

  Under a `**`, every real folder the `**` reaches is its own instance:

  ```text
  src/
    **/
      <name>
        {{ name }}.md
        {{ name }}.txt
  ```

  passes `src/a/blah.md` with `src/a/blah.txt`, and separately fails
  `src/b/c/foo.md` with `src/b/c/bar.txt`. Since that's every folder
  under `src/`, at any depth, `!<name>` there doesn't require a set in
  each one — only at least one set somewhere under `src/`, reporting
  `src/**/<name>` missing if there's none, the same way `!src/**/*.md`
  needs only one match anywhere. A `!` rule under the header is still
  required in each folder that has a value.

  A header holds only bare variable names between `<` and `>`, and must
  introduce them: giving it a trailing `/` or anything but a comment
  after it, declaring a variable an enclosing line already
  binds, and declaring one no line under it uses are all parse
  errors. To use `<` or `>` in a name, escape it (see
  "Escaping characters" below).
- **A missing entry's report names a concrete guessed filename whenever
  flint already knows enough to build one, leaving raw `{{ }}` syntax
  only for what it doesn't know.** This is the template run forwards:
  each variable in a missing entry's rule that flint knows a value for
  is substituted in, to render the name it expected to find. In the
  `Banana/` example just above, `name` is
  known directly from the real directory's own name (`Banana`), so the
  report reads `Banana/BananaPresenter.py`. The same guessing also
  applies when the governing directory is a plain literal name rather
  than a variable, as long as some other required or optional entry in
  that *same* directory is already satisfied and shares the missing
  entry's variable:

  ```text
  foo/
    !{{ name|pascal }}Manager.py
    !{{ name|pascal }}Presenter.py
  ```

  with only `foo/BlahManager.py` on disk reports the second entry as
  missing `foo/BlahPresenter.py`, not
  `foo/{{ name|pascal }}Presenter.py` — inferred from the Manager file
  that's already there. If nothing in that directory instance pins the
  variable down (an empty `foo/`, or two sibling matches that disagree on
  the value), the report falls back to the raw `{{ }}` rule text
  unchanged. Each variable is
  filled in on its own, so a rule with one known variable and one
  unknown names the known one and keeps the other as written: inside a
  real `Home/`, a missing `{{ name }}/!{{ other }}.{{ name }}` is
  reported as `Home/{{ other }}.Home`, and a missing
  `{{ name }}/!*.{{ name }}` as `Home/*.Home`. This is never
  guessed *across* directory instances — a value known for `Apple/` is
  never borrowed to guess something missing in `Banana/`, consistent with
  the "checked independently per real directory instance" rule above.
  When a guessed value needs re-casing into a different `|format` than
  the file it came from, an ambiguous run of capital letters (an acronym
  like `XML`) or a digit after a letter in a camelCase/PascalCase name
  is read as separate words rather than merged (unless lowercase letters
  carry on after the digit), and a `snake_case`/`kebab-case` segment is
  always treated as one whole word, digits included — see
  [`docs/formats.md`](formats.md) for the full case-style rules this
  reuses.
- **`.flint` is an allow-list: a file is fine if *any* rule allows it.**
  Rules only ever add to what's allowed, so two rules can never
  conflict. Given

  ```text
  {{ name }}/
      {{ name }}.md
      README.md
  ```

  `apple/README.md` fits the shape of `{{ name }}.md` but captures
  `README` where the folder captured `apple`, so that rule doesn't allow
  it — but the `README.md` rule does, so it's fine. One rule not
  allowing a file only matters if no rule allows it at all.
- **A single file can match more than one rule at once, and that's fine.**
  Nothing ties a file to exactly one line in `.flint`: given

  ```text
  {{ name }}Manager.py
  Foo{{ type }}.py
  ```

  the file `FooManager.py` matches *both* rules (`name` = `Foo` from
  the first, `type` = `Manager` from the second). Each rule's variables
  are its own, so there's nothing to reconcile between them. This is
  intended behavior, not a coincidence to guard against.
- **Two placeholders directly next to each other are rejected.**
  `{{a}}{{b}}.txt` is a parse error, because there's no way to know where
  one variable's value ends and the next begins — put literal text (even
  a single character) between them.
- **A trailing `/` restricts a rule to directories; no trailing `/`
  restricts it to files.** `src/` only matches a directory named `src` — a
  file named `src` is reported as unexpected. Conversely, plain `a.txt`
  only matches a file named `a.txt` — a directory with that name is
  reported as unexpected. This applies to `{{ variable }}` lines too:
  `{{ name }}/` matches any directory, capturing its name.
  **Symlinks are always treated as files** for this purpose, regardless of
  what they point at — a symlink to a directory does not satisfy a
  trailing-slash rule; only a real directory does.
- **A leading `!` marks an entry as required**, not just permitted: at
  least one match on disk is mandatory, e.g.:

  ```text
  !README.md
  ```

  fails unless a file named `README.md` actually exists. `!` composes with
  everything else — `!src/` requires a directory named `src`; `!*.md`
  requires *any* file matching `*.md` (not literally one named `*.md`);
  `!{{ name }}Manager.py`, written with no directory component, requires
  any one file it allows, anywhere. Only a file the rule actually allows
  counts: one that fits its shape but whose variables disagree (see
  above) doesn't satisfy it. A required entry that's missing is reported
  as its own error, distinct from an unexpected file. A required entry
  nested under a `{{ variable }}` directory is checked differently — once
  per real matching directory, not "anywhere" — and a required line that
  names folders of its own, like `!{{ name }}/{{ name }}.txt`, requires
  each of them too: see "Indentation nests a path under a preceding
  directory" above.
- **A line that's only a `!` and/or trailing `/` marker, with no name left
  to apply it to, is a parse error** — a bare `!`, a bare `/`, `!/`, and
  `! # comment` (nothing before the comment) are all rejected, rather than
  silently treated as blank. A genuinely blank or comment-only line (no
  marker at all) is still ignored, as above.
- **`.flint` never needs to list itself.** The file is always implicitly
  allowed.
- **An allowed name doesn't have to exist on disk.** Listing a name in
  `.flint` means "this name is *permitted* to be here," not "this name
  *must* be here." Only names present on disk that *aren't* in the
  allow-list are reported as errors.
- **Nothing else is auto-ignored.** In particular, version-control and
  build directories like `.git`, `target`, or `.venv` are **not**
  special-cased — if they exist in the directory you're linting, list
  them explicitly, along with what may be inside (e.g. `.git/**`). This
  is deliberate, not a gap: flint doesn't assume any particular VCS is in use, and there's no reliable
  way to auto-detect "the" VCS directory when more than one is present
  (e.g. a stray `.hg/` left over from a migration in what's now a git
  repo) — silently ignoring either would hide exactly the kind of
  stray-directory noise flint exists to catch.

## Escaping characters

Five kinds of characters are meaningful to `flint`'s parser and need
escaping with a backslash to use literally in a name: `#` (starts a
comment), `{`/`}` (start a `{{ variable }}` placeholder), `*` (a wildcard),
`<`/`>` (a group header), and a leading `!` (a required-entry marker).

- **Escape a literal `#` in a filename with `\#`.** This is the only way to
  reference a file whose name actually contains a `#` character:

  ```text
  weird\#file.txt
  ```

  parses as the filename `weird#file.txt`.
- **Escape a literal `{` or `}` in a filename with `\{` / `\}`.** This is
  required to reference a file whose name actually contains a brace
  character:

  ```text
  weird\{file\}.txt
  ```

  parses as the filename `weird{file}.txt`. A bare `{` or `}` that isn't
  part of a well-formed `{{ variable }}` placeholder and isn't escaped is
  a parse error, and flint refuses to run, printing a message that points
  at the offending line (see "Parse errors" below).
- **Escape a literal `*` in a filename with `\*`.** This is the only way to
  reference a file whose name actually contains an asterisk:

  ```text
  weird\*file.txt
  ```

  parses as the filename `weird*file.txt`. Escaping each `*` individually
  also produces a literal `**` where needed: `weird\*\*file.txt` parses as
  `weird**file.txt`, not an any-depth wildcard.
- **Escape a literal `<` or `>` in a filename with `\<` / `\>`.** Both
  are reserved for group headers like `<name>`, so a bare one anywhere
  else is a parse error:

  ```text
  \<draft\>.md
  ```

  parses as the filename `<draft>.md`.
- **Escape a literal leading `!` in a filename with `\!`.** Only needed at
  the very start of a line — `!` has no special meaning anywhere else:

  ```text
  \!important.txt
  ```

  parses as the (non-required) filename `!important.txt`.

## Examples

| Flint rule(s) | Matches | Doesn't match | Test |
|---|---|---|---|
| `a.txt` | `a.txt` (a file) | `intruder.txt`; a *directory* named `a.txt` | [`entries_not_in_allow_list_are_violations`](../src/checker.rs#L1357-L1362)<br>[`file_only_entry_matches_a_file_but_not_a_directory`](../src/checker.rs#L1474-L1482) |
| `src/` | a *directory* named `src` | a *file* named `src` | [`directory_only_entry_matches_a_directory_but_not_a_file`](../src/checker.rs#L1467-L1472) |
| `src/` | an *empty* directory named `src` | anything inside `src` (no rule reaches inside it) | [`a_bare_directory_rule_permits_only_an_empty_directory`](../src/run.rs#L1075-L1084) |
| `{{ name }}/` | any directory (`name` = its name) | any file, even with the same name | [`trailing_slash_on_template_line_marks_directory_only`](../src/flint_file.rs#L1344-L1355)<br>[`template_with_trailing_slash_only_matches_directories`](../src/checker.rs#L1484-L1489) |
| `{{ name }}Manager.py` | `HomeManager.py` (`name` = `Home`) | `Manager.py` (empty value not allowed) | [`single_variable_matches_and_extracts_value`](../src/template.rs#L857-L867)<br>[`no_structural_match_returns_none`](../src/template.rs#L899-L907) |
| `{{ name }}Manager.{{ extension }}` | `HomeManager.py` (`name` = `Home`, `extension` = `py`) | `HomeManager` (no `.` before the extension) | [`multiple_variables_extract_in_order`](../src/template.rs#L909-L923)<br>[`multiple_variables_require_the_literal_separator_to_match`](../src/template.rs#L949-L957) |
| `{{ name }}Manager.py`<br>`{{ name }}Presenter.py` | `HomeManager.py`<br>`HomePresenter.py`, **or** `HomeManager.py`<br>`DetailsPresenter.py` — two separate lines, so mismatched names are fine too | — | [`consistent_template_match_across_two_lines_is_not_a_violation`](../src/checker.rs#L1377-L1382)<br>[`flat_sibling_templates_are_never_reconciled_against_each_other`](../src/checker.rs#L1384-L1393) |
| `{{ name }}_{{ name }}.txt` | `Home_Home.txt` | `Home_Details.txt` (both occurrences come from the one file, and disagree) | [`same_variable_twice_in_one_line_consistent_match_is_fine`](../src/checker.rs#L1453-L1458)<br>[`same_variable_twice_in_one_line_disagreeing_is_unexpected`](../src/checker.rs#L1408-L1416) |
| `{{ name }}.txt` | `Aaa.txt` and `Bbb.txt` together — a lone rule has nothing to reconcile against across files | — | [`single_template_line_can_match_multiple_files_independently`](../src/checker.rs#L1418-L1425) |
| `{{ name }}Manager.py`<br>`Foo{{ type }}.py` | `FooManager.py` (matches both rules at once — see above) | — | [`multiple_rules_matching_the_same_file_is_fine`](../src/checker.rs#L1460-L1465) |
| `weird\{file\}.txt` | `weird{file}.txt` | — | [`escaped_braces_produce_literal_text`](../src/flint_file.rs#L1027-L1033) |
| `weird\#file.txt` | `weird#file.txt` | — | [`escaped_hash_is_kept_literally`](../src/flint_file.rs#L1003-L1009) |
| `*.py` | `Home.py`, `Details.py` | `notes.txt`; `.py` or `.hidden.py` (leading `*` excludes dotfiles) | [`wildcard_rule_allows_any_matching_file_and_flags_the_rest`](../src/checker.rs#L1491-L1496)<br>[`wildcard_rule_does_not_match_a_dotfile`](../src/checker.rs#L1507-L1512) |
| `.git*` | `.gitignore`, `.git` | — | [`wildcard_after_a_literal_leading_dot_can_match_a_dotfile`](../src/template.rs#L984-L992) |
| `{{ name }}Manager*.py` | `HomeManagerExtra.py` (`name` = `Home`) | — | [`wildcard_and_variable_can_combine_on_one_line`](../src/flint_file.rs#L1091-L1101) |
| `weird\*file.txt` | `weird*file.txt` | — | [`escaped_asterisk_is_kept_literally`](../src/flint_file.rs#L1035-L1041) |
| `!README.md` | required: fails unless `README.md` exists | — | [`required_literal_missing_on_disk_is_a_violation`](../src/checker.rs#L1514-L1518)<br>[`required_literal_present_on_disk_is_fine`](../src/checker.rs#L1520-L1524) |
| `!*.md` | required, satisfied by *any* `.md` file | fails only if *no* file matches `*.md` | [`required_wildcard_is_satisfied_by_any_one_match`](../src/checker.rs#L1535-L1542)<br>[`required_wildcard_missing_reports_source_line`](../src/checker.rs#L1544-L1548) |
| `\!important.txt` | `!important.txt` | — | [`escaped_required_marker_is_kept_literally_and_not_required`](../src/flint_file.rs#L1411-L1419) |
| `src/util.rs` | a file `util.rs` inside directory `src` (with `src` implicitly permitted) | `util.rs` at the top level; `src/nested/util.rs` | [`nested_literal_rule_matches_nested_actual_entry`](../src/checker.rs#L1889-L1893) |
| `src/*/main.py` | `main.py` inside *any* immediate subdirectory of `src` | `src/main.py` (wrong depth); `src/linux/nested/main.py` (too deep) | [`nested_wildcard_rule_matches_nested_actual_entry`](../src/checker.rs#L1895-L1899) |
| `{{ platform }}/main.py`<br>`{{ platform }}/config.py` | `linux/main.py`<br>`linux/config.py`, **or** `linux/main.py`<br>`macos/config.py` — two separate lines, so different subdirectories are independent | — | [`cross_directory_variable_values_are_independent_per_instance`](../src/checker.rs#L1950-L1959) |
| `{{ name }}/{{ name }}Manager.py` | `Apple/AppleManager.py` | `Apple/BananaManager.py` (one path's own two components disagree) | [`compound_nested_template_occurrences_must_still_agree_within_one_match`](../src/checker.rs#L1582-L1599) |
| `{{ name }}/`<br>`&nbsp;&nbsp;{{ name }}.md`<br>`&nbsp;&nbsp;README.md` | `apple/apple.md`<br>`apple/README.md` — `{{ name }}.md` doesn't allow `README.md`, but the `README.md` rule does | `apple/notes.md` | [`a_literal_rule_allows_an_entry_a_template_nearly_matched`](../src/checker.rs#L1601-L1610) |
| `!src/util.rs` | required: fails unless `src/util.rs` exists | — | [`required_nested_rule_reports_missing_with_nested_path`](../src/checker.rs#L1944-L1948) |
| `src/**/*.py` | `.py` files under `src` at any depth, including directly in `src` | `src/notes.txt`; `src/.git/` (`**` with more after it doesn't reach into dot-named directories) | [`any_depth_matches_zero_or_more_absorbed_components`](../src/template.rs#L1093-L1101)<br>[`any_depth_skips_a_dot_named_absorbed_directory`](../src/template.rs#L1110-L1114) |
| `.git/**` | anything inside `.git`, at any depth | — | [`explicit_literal_dot_prefix_before_any_depth_is_unaffected_by_dotfile_skip`](../src/template.rs#L1178-L1189) |
| `target/**` | anything inside `target`, at any depth, dot names included | — | [`a_trailing_any_depth_allows_everything_in_a_directory_dot_names_included`](../src/run.rs#L1099-L1109)<br>[`trailing_any_depth_matches_dot_names_at_any_depth`](../src/template.rs#L1124-L1135) |
| `src/`<br>`&nbsp;&nbsp;util.rs` | identical to `src/util.rs` above — a file `util.rs` inside directory `src` | `util.rs` at the top level | [`indented_child_expands_to_the_same_entry_as_flat_form`](../src/flint_file.rs#L1439-L1448) |
| `{{ name }}/`<br>`&nbsp;&nbsp;!{{ name }}Manager.py`<br>`&nbsp;&nbsp;!{{ name }}Presenter.py` | `Apple/AppleManager.py`<br>`Apple/ApplePresenter.py`, **and, independently,** `Banana/BananaManager.py`<br>`Banana/BananaPresenter.py` | `Banana/BananaManager.py` alone — `Apple/`'s own `ApplePresenter.py` doesn't satisfy `Banana/`'s obligation | [`required_marker_on_indented_child_is_independent_of_its_sibling`](../src/flint_file.rs#L1554-L1569)<br>[`required_template_is_satisfied_independently_per_matching_directory_instance`](../src/checker.rs#L1668-L1701) |
| `{{ name }}/`<br>`&nbsp;&nbsp;!{{ name }}/{{ name }}.txt` | `blah/blah/blah.txt`; no folders at all | an empty `blah/` (`blah/blah/blah.txt` missing — the `!` requires the `blah/blah/` folder too); `blah/blah/` with no `blah.txt` in it | [`a_required_line_requires_each_folder_it_names`](../src/checker.rs#L2789-L2798)<br>[`a_required_line_s_folder_still_needs_its_file`](../src/checker.rs#L2800-L2807)<br>[`a_required_line_does_not_require_the_folders_it_is_indented_under`](../src/checker.rs#L2816-L2820) |
| `{{ name\|camel }}/`<br>`&nbsp;&nbsp;{{ name\|pascal }}Manager.py` | `foo/FooManager.py` (`name` is the same identifier, `foo`/`Foo`, spelled per format) | `foo/BarManager.py` (different identifier, within this one nested path) | [`cross_format_occurrences_of_the_same_identifier_are_consistent`](../src/checker.rs#L1961-L1975)<br>[`cross_format_disagreement_is_unexpected`](../src/checker.rs#L1977-L1994) |
| `{{ name\|snake }}.txt` | `home_page.txt` | `HomePage.txt` (wrong shape — unexpected) | [`formatted_variable_matches_only_its_own_shape_and_carries_the_format_through`](../src/template.rs#L869-L897)<br>[`format_shape_mismatch_is_unexpected`](../src/checker.rs#L2013-L2018) |
| `A_{{ name\|pascal }}.txt`<br>`B_{{ name\|pascal }}.txt` | `A_XMLFile.txt`<br>`B_XmlFile.txt` together — two separate lines, never reconciled, even under the same format | — | [`same_format_flat_siblings_are_never_reconciled`](../src/checker.rs#L1996-L2004) |
| `src/`<br>`&nbsp;&nbsp;<name>`<br>`&nbsp;&nbsp;&nbsp;&nbsp;{{ name }}.md`<br>`&nbsp;&nbsp;&nbsp;&nbsp;{{ name }}.txt` | `src/blah.md`; `src/blah.txt`; or both | `src/blah.md` with `src/foo.txt` or `src/foo.md` (one group conflict, listing both values) | [`a_group_allows_one_file_or_a_matching_pair`](../src/checker.rs#L2238-L2251)<br>[`a_group_s_files_must_share_one_value`](../src/checker.rs#L2253-L2263)<br>[`a_group_s_files_under_the_same_rule_must_share_one_value_too`](../src/checker.rs#L2265-L2275) |
| `<name>`<br>`&nbsp;&nbsp;{{ name }}.md`<br>`&nbsp;&nbsp;{{ name }}-notes.md` | `a.md`<br>`a-notes.md` (`name` = `a` fits both, though `a-notes.md` also fits `{{ name }}.md`) | — | [`a_group_value_is_found_whichever_order_files_and_rules_come_in`](../src/checker.rs#L2309-L2315) |
| `src/`<br>`&nbsp;&nbsp;<name>`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ name }}.md`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ name }}.txt` | `src/blah.md`<br>`src/blah.txt`; an empty `src/` | `src/blah.md` alone (`src/blah.txt` missing) | [`a_group_s_required_rules_require_nothing_in_a_folder_with_none_of_its_files`](../src/checker.rs#L2418-L2421)<br>[`a_group_s_missing_file_is_named_with_the_folder_s_value`](../src/checker.rs#L2456-L2462) |
| `src/`<br>`&nbsp;&nbsp;<a, b>`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ a }}.txt`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ a }}.jpg`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ b }}.md`<br>`&nbsp;&nbsp;&nbsp;&nbsp;!{{ b }}.gif` | `src/foo.txt`<br>`src/foo.jpg` (nothing gives `b` a value, so its files aren't required) | `src/foo.txt` alone (`src/foo.jpg` missing) | [`a_group_s_required_rule_waits_for_a_value_for_each_variable_it_uses`](../src/checker.rs#L2464-L2477) |
| `src/`<br>`&nbsp;&nbsp;!<name>`<br>`&nbsp;&nbsp;&nbsp;&nbsp;{{ name }}.md`<br>`&nbsp;&nbsp;&nbsp;&nbsp;{{ name }}.txt` | `src/blah.txt`; any file the rules under it allow | an empty `src/` (`src/<name>` missing) | [`a_required_group_header_requires_a_set_in_its_folder`](../src/checker.rs#L2423-L2428) |

## Parse errors

A malformed `.flint` file is rejected outright (see "What counts as an
error" below) rather than partially applied. These are the specific ways a
line can be malformed:

| Invalid flint rule | Reason | Test |
|---|---|---|
| `weird{file.txt` | a stray `{` that isn't escaped and doesn't open a `{{ ... }}` placeholder | [`bare_unescaped_open_brace_is_a_parse_error`](../src/flint_file.rs#L1248-L1254) |
| `weird}file.txt` | a stray `}` that isn't escaped | [`bare_unescaped_close_brace_is_a_parse_error`](../src/flint_file.rs#L1256-L1262) |
| `{{ name Manager.py` | `{{` was opened but never closed with `}}` | [`unterminated_template_is_a_parse_error`](../src/flint_file.rs#L1264-L1270) |
| `{{}}Manager.py` | no identifier between the braces | [`empty_variable_name_is_a_parse_error`](../src/flint_file.rs#L1272-L1282) |
| `{{ na-me }}.txt` | `-` isn't a valid identifier character | [`invalid_variable_name_characters_are_a_parse_error`](../src/flint_file.rs#L1284-L1290) |
| `{{ 1name }}.txt` | identifiers can't start with a digit | [`variable_name_starting_with_digit_is_a_parse_error`](../src/flint_file.rs#L1292-L1298) |
| `{{a}}{{b}}.txt` | no literal text between two placeholders — the split point would be ambiguous | [`adjacent_variables_with_no_literal_between_is_a_parse_error`](../src/flint_file.rs#L1300-L1306) |
| `{{ name\|kebob }}.txt` | `kebob` isn't one of the ten recognized format names (`camel`, `pascal`, `snake`, `screamingSnake`, `kebab`, `screamingKebab`, `title`, `train`, `dot`, `screamingDot`) | [`unknown_format_name_is_a_parse_error`](../src/flint_file.rs#L1196-L1202) |
| `{{ name }}_{{ name\|camel }}.txt` | the same variable, `name`, is used with a `\|format` filter in one place and without one elsewhere in the same rule's path | [`mixing_formatted_and_unformatted_occurrences_within_one_line_is_a_parse_error`](../src/flint_file.rs#L1212-L1218) |
| `pre**post` | `**` combined with other text in the same path component | [`any_depth_combined_with_other_text_is_a_parse_error`](../src/flint_file.rs#L1063-L1069) |
| `a/**/b/**/c` | more than one `**` in the same line | [`multiple_any_depth_in_one_line_is_a_parse_error`](../src/flint_file.rs#L1071-L1077) |
| `/` (just a slash) | a trailing-`/` marker with no name before it | [`bare_slash_line_is_a_parse_error`](../src/flint_file.rs#L1339-L1342) |
| `!` (just an exclamation mark) | a required marker with no name after it | [`bare_required_marker_is_a_parse_error`](../src/flint_file.rs#L1421-L1424) |
| `!/` | both markers, still no name | [`required_marker_with_only_trailing_slash_is_a_parse_error`](../src/flint_file.rs#L1426-L1429) |
| `! # comment` | a required marker followed only by a comment | [`required_marker_with_only_a_comment_is_a_parse_error`](../src/flint_file.rs#L1431-L1437) |
| `a.txt`<br>`&nbsp;&nbsp;b.txt` | `a.txt` has no trailing `/`, so `b.txt` has nothing valid to nest under | [`indenting_under_a_non_directory_line_is_a_parse_error`](../src/flint_file.rs#L1623-L1629) |
| `a/`<br>`&nbsp;&nbsp;x/`<br>`&nbsp;&nbsp;&nbsp;&nbsp;p.txt`<br>`&nbsp;c.txt` | the last line's indentation (1 space) doesn't match any enclosing line's (0 or 2 spaces) | [`dedent_that_matches_no_open_level_is_a_parse_error`](../src/flint_file.rs#L1631-L1637) |
| `<name>/`; `<{{ name }}>`; `<name\|snake>` | a group header holds only variable names, with nothing after it but a comment | [`a_group_header_with_a_trailing_slash_is_a_parse_error`](../src/flint_file.rs#L1813-L1819)<br>[`a_group_header_holding_anything_but_names_is_a_parse_error`](../src/flint_file.rs#L1821-L1837) |
| `{{ name }}/`<br>`&nbsp;&nbsp;<name>` | `name` is already bound by the enclosing folder | [`a_group_header_rebinding_an_enclosing_directory_s_variable_is_a_parse_error`](../src/flint_file.rs#L1855-L1861) |
| `<name>`<br>`&nbsp;&nbsp;README.md` | no line under the header uses `name` | [`a_group_header_variable_no_rule_under_it_uses_is_a_parse_error`](../src/flint_file.rs#L1879-L1885)<br>[`a_group_header_with_nothing_under_it_is_a_parse_error`](../src/flint_file.rs#L1871-L1877) |
| `<name>`<br>`&nbsp;&nbsp;{{ name\|camel }}.md`<br>`&nbsp;&nbsp;{{ name }}.txt` | rules under one header share `name`, so a filter must be used on all of them or none | [`mixing_formatted_and_unformatted_occurrences_across_a_group_s_rules_is_a_parse_error`](../src/flint_file.rs#L1898-L1907) |
| `a<b.txt` | `<` and `>` are reserved for group headers; escape them as `\<` / `\>` | [`an_unescaped_angle_bracket_outside_a_group_header_is_a_parse_error`](../src/flint_file.rs#L1914-L1924) |

## Multiple `.flint` files

flint doesn't stop at the directory it's checking: it recurses through
every subdirectory looking for more `.flint` files, and checks each one it
finds against the actual contents of *its own* directory. The folder
itself still has to be permitted by the enclosing `.flint` file, like any
other folder: a nested `.flint` file describes what's inside a folder, not
whether the folder may exist. (So `packages/` alone, with
`packages/foo/.flint` below it, reports `packages/foo` as unexpected —
`packages/` would need `*/` indented under it, say.)

```text
.
├── .flint            # root/.flint, governs everything not delegated below
├── Cargo.toml
├── src/
└── packages/
    ├── foo/
    │   └── .flint     # checked against packages/foo/'s own contents
    └── bar/
        └── .flint     # likewise, against packages/bar/'s own contents
```

When more than one `.flint` file is involved in a run, every reported
error is labeled with the path of the `.flint` file it came from, and
every disk path shown is relative to the invocation directory (not to
that `.flint` file's own directory) so it's unambiguous. With only one
`.flint` file in the whole tree (the common case), output is unchanged
from a single-`.flint`-file run — nothing is labeled.

**Symlinked directories are never followed.** A symlink is already
always treated as a file for rule-matching purposes (see "Rules" above);
consistently, flint also never descends into one while searching for
`.flint` files, even if it points at a real directory. Dot-named
directories are searched like any other.

### Rules compose (cascade) across `.flint` files

When a parent `.flint` file's rule reaches into a folder with its own
`.flint` file, both files' rules apply there, and a required entry is
reported missing at most once, under the file that wrote it. A rule only
ever permits or requires, never forbids, so two files' rules can never
conflict.

## Names in any language

A name can be in any language or script, in a rule and on disk:
`größe.txt`, `日本語/`, `привет мир.md`. A `*` or a `{{ variable }}`
takes any characters too.

- **A name is compared exactly, letter for letter.** Capitals and
  accents both count: `README.md` isn't `readme.md`, and `café.txt` isn't
  `cafe.txt`. Lookalikes are different characters as well — a Cyrillic
  `а` isn't a Latin `a`, and the single-character ligature `ﬁ` isn't
  `fi`.
- **How an accented letter is stored doesn't matter.** `é` can be stored
  as one character, or as `e` followed by a combining accent. The two
  look identical, and a rule typed one way matches a name stored the
  other way — a keyboard usually gives the first, and macOS often stores
  a file name as the second. flint composes both the `.flint` file and
  every name it reads to the one-character form (Unicode's NFC) before
  comparing them, and reports names in that form.

  Linux lets one folder hold both spellings as two separate files. flint
  then compares each of those two as it's stored, so only the
  one-character spelling can match a rule that names it.
- **A file system that ignores case changes nothing.** On macOS and
  Windows, `README.md` and `readme.md` are the same file, but the file
  keeps the name it was created with, and that's the name flint
  compares: the rule `readme.md` doesn't allow a file created as
  `README.md`. One consequence is that this pair of rules can never both
  be satisfied there, since the two files can't exist side by side:

  ```text
  !foo
  !FOO
  ```

- **A name that isn't valid Unicode stops the run.** Linux allows a file
  name to be any bytes at all — say, one written in an older encoding
  like Latin-1. A `.flint` file is text, so no rule could name such a
  file, and flint doesn't guess: it prints the name, with the bad bytes
  escaped (`"caf\xE9.txt"`), and exits with status `1` without checking
  anything.

`|format` case-style filters know about capitals in every script, too —
see [`formats.md`](formats.md#letters-outside-english). Variable names
are the one thing limited to English letters: `{{ größe }}` is a parse
error.

## What counts as an error

Any file or folder present in a `.flint` file's own directory, or inside a
directory some rule permits (see "Rules" above), that isn't
`.flint` itself, and that no rule in that `.flint` file allows, is
reported as unexpected. A literal rule allows exactly its own name; a
`{{ variable }}` or `*` wildcard rule allows a name that fits its shape,
as long as every occurrence of each variable captures the same value
(see above). Allowing also requires the directory/file kind to agree — a directory is only
matched by a rule ending in `/`, and a file is only matched by a rule that
doesn't (see "Rules" above). A governed directory itself is never reported
as an error, even with no rule directly permitting it as a leaf. When any
errors are found anywhere in the tree, flint prints each one and exits
with status `1`.

A file that fits the shape of a `{{ variable }}` rule, but whose
variables disagree, isn't allowed by that rule: unless another rule
allows it, it's reported as unexpected, like any other file.

The files under a group header that don't share one value in a folder
are reported once, as a group conflict listing each value and its files
(see "A group header shares a variable across the rules under it"
above), rather than as unexpected entries.

An entry marked `!` (required) that nothing on disk satisfies is
reported as its own "missing" error, separate from unexpected files.
Only an entry the rule allows satisfies it, so a file whose variables
disagree leaves it missing — and is reported as unexpected, too.

A `.flint` file with any of the mistakes in "Parse errors" above is a
parse error: flint prints a message identifying the file, the line, and
the problem, and doesn't check that `.flint` file's own directory at
all — but every other `.flint` file found elsewhere in the tree is
unaffected and still parsed and checked normally (see "Multiple `.flint`
files" above). The run as a whole still exits with status `1` if any
`.flint` file anywhere failed to parse.
