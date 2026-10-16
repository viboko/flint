# Case-style formats

A `{{ variable }}` placeholder can carry a `|format` filter — e.g.
`{{ name|camel }}` — to constrain it to a specific case style, and to
recognize differently-cased occurrences of the same variable as the same
underlying identifier. Since a `.flint` rule is a template (see
[`docs/flint-file-format.md`](flint-file-format.md)), `{{ name|camel }}`
reads as "`name`, rendered in camelCase": a real name matches if it's
what that rendering would produce, and `{{ name|camel }}` and
`{{ name|pascal }}` agree when they're two renderings of the same
`name`. This document covers the ten formats in full, along with the
word-splitting rules that make cross-format matching work. See
[`docs/flint-file-format.md`](flint-file-format.md) for the shorter
summary, the parse-error list, and the general `{{ variable }}` behavior
this builds on.

## The ten formats

| Format | Filter | Example |
|---|---|---|
| camelCase | `{{ name\|camel }}` | `parseXMLFile` |
| PascalCase | `{{ name\|pascal }}` | `ParseXMLFile` |
| snake_case | `{{ name\|snake }}` | `parse_x_m_l_file` |
| SCREAMING_SNAKE_CASE | `{{ name\|screamingSnake }}` | `PARSE_X_M_L_FILE` |
| kebab-case | `{{ name\|kebab }}` | `parse-x-m-l-file` |
| SCREAMING-KEBAB-CASE | `{{ name\|screamingKebab }}` | `PARSE-X-M-L-FILE` |
| Title Case | `{{ name\|title }}` | `Parse X M L File` |
| Train-Case | `{{ name\|train }}` | `Parse-X-M-L-File` |
| dot.case | `{{ name\|dot }}` | `parse.x.m.l.file` |
| SCREAMING.DOT.CASE | `{{ name\|screamingDot }}` | `PARSE.X.M.L.FILE` |

Whitespace around `|` is flexible, the same as inside `{{ }}` —
`{{name|camel}}`, `{{ name|camel }}`, and `{{ name | camel }}` are all
equivalent. A file whose name doesn't fit the requested format's shape
simply doesn't match that placeholder at all — it's reported as an
ordinary unexpected file (if nothing else permits it), not a special
error.

## How word-splitting works

Every format is built on one shared idea: a captured value decomposes
into a list of lowercase **words**, and two occurrences (possibly under
different formats) are the same identifier if their word lists agree.

**For `snake`/`screamingSnake`/`kebab`/`screamingKebab`/`dot`/`screamingDot`**,
splitting is simple: the literal `_`, `-`, or `.` *is* the word boundary,
and nothing else is. Each token between separators is letters of the
expected case and digits, in any mix, and is exactly one word: in
`parse_file2` or `parse2files` the `2` is part of a word, since a word of
its own would have a separator beside it (`parse_file_2`,
`parse_2_files`).

**For `title`/`train`**, splitting is separator-based too (a literal
space or `-`), but each token must additionally start with a capital (or
a digit): `Parse XML File`, `Parse Xml File`, `Parse X M L File`,
`Parse2files`, `Parse2Files`, `Parse File2` and `Watch YouTube` are all
valid `title`, while `Watch iPhone` isn't. A digit belongs to its word
here too, unless a separator is beside it. A capital partway through a
token, though, may or may not start a new word — `YouTube` is the word
`youtube`, or `you` and `tube`; `Parse2Files` is `parse2files`, or
`parse2` and `files`; `XML` is `xml`, or `x`, `m` and `l` — the same way
a run of capitals is ambiguous in `camel`/`pascal` (see below).

**For `camel`/`pascal`**, there's no separator to lean on, so the rule is:
a word boundary falls before every uppercase letter. Applied to
`parseXMLFile`, a boundary falls before each of `X`, `M`, `L`, and `F`,
giving the words `parse`, `x`, `m`, `l`, `file` — though see below for
why `xml` is read as one word too. A lowercase letter carries on the
word before it, so `parse2files` is one word and `parse2Files` at least
two — and a digit may or may not start a new word (see below):
`parseFile2` is `parse`, `file`, `2` or `parse`, `file2`.

### Acronyms and numbers are ambiguous on purpose

That boundary-before-every-capital rule looks unusual next to the more
familiar convention where `XML` in `XMLFile` is treated as one word
(giving `xml_file` in snake_case). The reason flint doesn't do that:
**case-only text can't distinguish "an intentional acronym" from "a run
of single-letter words."** `XML` and `X`+`M`+`L` are indistinguishable
from inside the run. Digits have the same problem, as does any other
character with no case: a word that starts with one can't be given a
capital, so nothing in `parseFile2` says whether the `2` is its own
word, or in `File12` whether the `2` is. Resolving that
ambiguity one specific way would need either a hardcoded dictionary of
known acronyms (incomplete — it can never cover every project's own
acronyms) or remembering whichever occurrence of the variable gets
parsed first (non-deterministic — the result would depend on line order
in `.flint`).

Instead, flint treats each such gap — in a `camel`/`pascal` value,
between two capitals or before a digit (or any other character with no
case), and in a
`title`/`train` value, before a capital partway through a token — as
**possibly a word break, or not**, independently of every other gap. So
`parseXMLFile` (camel) is recognized as the same identifier as *any* of:

- `parse_xml_file` (the whole acronym kept as one word)
- `parse_x_m_l_file` (every letter split out)
- `parse_xm_l_file`, `parse_x_ml_file` (partial groupings — legal, if
  unlikely to be written by hand)

and `parseFile2` as either `parse_file_2` or `parse_file2`, as long as
both occurrences involved actually admit a shared grouping. Likewise
`123` (pascal) is the same identifier as `123`, `12-3`, `1-23` or
`1-2-3` (kebab) — it's how every one of them is spelled in PascalCase,
so a rule asking for the PascalCase name of a `1-2-3` folder is asking
for `123`.
This can never make two identifiers that differ in actual *characters*
falsely match — every candidate word is a contiguous substring of that
occurrence's own text, so a genuine difference in characters (a dropped
or swapped letter) can never produce an overlapping candidate. Only
*regrouping* of identical characters is reconciled, never a difference in
the characters themselves.

### Rendering a guessed value picks one grouping deterministically

The "every possible grouping" treatment above is for *consistency
checking*, where being permissive is the whole point. Guessing a missing
entry's filename (see
[`docs/flint-file-format.md`](flint-file-format.md)'s "A missing
entry's report names a concrete guessed filename" bullet) is a
different kind of question — it needs exactly one rendered answer, not a set of equally
valid readings. For that, flint always resolves an ambiguous run of
capital letters to its most-split reading, one word per letter, rather
than merging it into a guessed acronym: a value already captured as
`parseXMLFile` renders under `|snake` as `parse_x_m_l_file`, not
`parse_xml_file`. A digit run after a letter in a `camel`/`pascal` value
is likewise its own word — `parseFile2` renders as `parse_file_2` —
unless lowercase letters carry on after it, which make it part of their
word: `parse2files` stays `parse2files`. Digits next to each other
stay together, so `parseFile25` renders as `parse_file_25`. A
separator-based value
(`snake`, `kebab`, `dot`, `screamingDot`, `title`, `train` — any format
from the table above with a literal separator) already says where its
words break — each separator-delimited segment is one word, as written,
digits included, regardless of that segment's own casing — so
`parse_file2` renders under `|kebab` as `parse-file2`, `Watch YouTube`
under `|snake` as `watch_youtube`, and `xml_parser`
renders under `|pascal` as `XmlParser`, not `XMLParser`: rendering always
title-cases a word fresh, rather than preserving whatever casing it had
in the format it was captured under.

## Cross-occurrence consistency

The consistency rule from
[`docs/flint-file-format.md`](flint-file-format.md) — occurrences of
the same variable *captured from the same matched entry* must resolve to
the same value, but two separate `.flint` lines never have to agree with
each other — extends across formats too, with two different policies
depending on whether two occurrences within one match share a format:

- **Different formats are reconciled permissively.** `{{ name|camel }}`
  and `{{ name|pascal }}` are recognized as the same identifier as long as
  they admit at least one shared word grouping — this is what lets

  ```text
  {{ name|camel }}/
    {{ name|pascal }}Manager.py
  ```

  correctly require `foo/FooManager.py` (the directory and filename
  components of that one nested path agreeing), not merely tolerate
  whatever casing shows up.
- **The same format, repeated within one match, must match literally.**
  A directory-and-filename pair like

  ```text
  {{ name|pascal }}/
    {{ name|pascal }}Manager.py
  ```

  doesn't allow `XMLFile/XmlFileManager.py`, even
  though both spellings admit the shared `xml`+`file` grouping —
  permissive cross-format matching alone would let this slide, so it's
  caught by a separate, stricter check: repeats of one format, within the
  same match, are compared as plain literal text, the same way an
  unformatted `{{ variable }}` always has been. This is also what would
  catch `parse_xml_file` and `parse_x_m_l_file` both captured via
  `{{ name|snake }}` from the two components of one nested path — two
  different word lists with nothing in common, under any policy. Two
  *separate* `.flint` lines using the same format, like
  `A_{{ name|pascal }}.txt` and `B_{{ name|pascal }}.txt`, are never
  reconciled against each other at all — `A_XMLFile.txt` and
  `B_XmlFile.txt` can coexist, same as any other pair of flat sibling
  rules (see `docs/flint-file-format.md`).

## Mixing formatted and unformatted occurrences

Within one rule's path — a single line, plus any directory lines it's
indented under — and across the rules under one `<name>` group header,
a variable must use a `|format` filter at *every* occurrence, or none.
`{{ name }}_{{ name|camel }}.txt`, `{{ name|camel }}/` with
`{{ name }}Manager.py` indented under it, or `<name>` over both
`{{ name|camel }}.md` and `{{ name }}.txt`, is a parse error, since
there'd be no consistent way to decide whether the unformatted occurrence's literal text should be compared as-is or
reinterpreted under some format. Separate rules are never reconciled
against each other, so they don't need to agree:

```text
src/
  {{ name|snake }}.rs
tests/
  {{ name }}.rs
```

is fine. See the "Parse errors" table in
[`docs/flint-file-format.md`](flint-file-format.md#parse-errors) for the
authoritative list of parse errors, including this one.

## Letters outside English

Every format works on letters and digits of any script, not just `a`–`z`
and `0`–`9`. `{{ name|snake }}` accepts `crème_brûlée`, and recognizes
it as the same identifier as `CrèmeBrûlée` (`|pascal`),
`CRÈME-BRÛLÉE` (`|screamingKebab`) or `Crème Brûlée` (`|title`).
Greek, Cyrillic and every other script with capitals work the same way:
`привет_мир` is `ПриветМир`.

- **An accent is part of its letter.** `über` and `uber` are different
  identifiers, in every format. A capital with an accent is still a
  capital, so `écoleÉlémentaire` (`|camel`) is `école` and
  `élémentaire`, and `Über` doesn't fit `|snake`.
- **A script with no capitals fits every format.** Japanese, Chinese,
  Korean, Hebrew, Arabic, Hindi and Thai, among others, have no upper
  or lower case, so their characters behave like digits do: `日本語` is
  valid `|snake`, `|screamingSnake`, `|camel` and `|pascal` alike. Words
  in them are only ever marked by a separator (`日本語_ファイル`) or
  by a capital from another script (`日本語File` is `日本語` and `file`).
  With neither, as in `camel`/`pascal`, a word may start at any of them,
  just as at a digit: `日本語` there is the same identifier as `日本語`
  or `日本_語` in `|snake`, and is spelled `日本語` in any other format.
- **The separator is always the plain one**: `_`, `-`, `.` or a space.
  A lookalike from another script, like the full-width `＿` or a
  non-breaking space, isn't a separator and isn't a letter, so a name
  using one fits no format. Neither does a name with punctuation or a
  symbol in it.
- **An emoji fits every format.** It has no upper or lower case, so it
  behaves just like a character of a script with no capitals: `😀` is
  valid in all ten formats, `😀-😀` (`|kebab`) is the same identifier
  as `😀_😀` (`|snake`), and `party🎉Time` (`|camel`) is `party🎉` and
  `time`. An emoji made of several characters is allowed whole — one
  with a skin tone (`👍🏽`), a flag, or several joined into one
  (`👨‍👩‍👧`) — and each of those characters counts as its own when
  two names are compared, so `👍🏽` and `👍` are different identifiers.
  The invisible joiner such an emoji is built with is allowed anywhere
  in a name as a result. A symbol that isn't an emoji, like `→` or `€`,
  still fits no format.
- **Recasing a letter never changes how many letters there are.**
  German `ß` is the one letter this matters for: its usual capital is
  two letters, `SS`, but flint uses the one-letter capital, `ẞ`. So
  `straße` (`|snake`) is the same identifier as `STRAẞE`
  (`|screamingSnake`), which is also how a guessed name is spelled —
  and `STRASSE` is a different identifier, since turning `SS` back into
  `ß` would be a guess (`MASSE` could be `masse` or `maße`). `ß` itself
  is a lowercase letter, so `STRAßE` doesn't fit `|screamingSnake`.
- **Every kind of `i` is the same letter.** Turkish has two — dotted
  `i`/`İ` and dotless `ı`/`I` — where English pairs `i` with `I`. Which
  pairing a name means depends on its language, which a file name
  doesn't say, so flint treats all four as one letter when comparing
  across formats: `DIYARBAKIR` matches both `diyarbakır` and
  `diyarbakir`, and `İSTANBUL` matches `istanbul`. When it has to spell
  a guessed name, it uses the English pairing (`istanbul` becomes
  `ISTANBUL`).
- **Greek's two lowercase sigmas are the same letter**, since both are
  `Σ` in capitals: `οδός` (`|snake`) matches `ΟΔΌΣ`
  (`|screamingSnake`).
- **Digits of any script are digits**: `file٣` (`|camel`) is `file` and
  `٣`, or the one word `file٣`, exactly as `file3` would be.
- **An accent stored as its own character stays with its letter.** flint
  composes a letter and a combining accent into one character where
  Unicode has one (see
  [`flint-file-format.md`](flint-file-format.md#names-in-any-language)),
  but some pairs have none — `x̄` is always `x` and a combining macron.
  The accent is then simply part of the word its letter is in.

These comparisons use Unicode's own tables, with no setting for a
language: the same `.flint` file gives the same result on every machine.

## Edge cases

- **A leading digit run has no case**, so it's valid for both `camel` and
  `pascal`: `25Values` canonicalizes to `[25, values]` either way, and
  `{{ name|camel }}` and `{{ name|pascal }}` placeholders both accept it
  identically. This is harmless — each occurrence still declares its own
  format, so there's no runtime ambiguity from it.
- **A single-word value** round-trips through every format the same way a
  multi-word one does — `value` is simply one word in every format's word
  list, with no separators or case boundaries to speak of.
- **A digit may or may not start a new word in `camel`/`pascal`** —
  `read25Values` is `[read, 25, values]` or `[read25, values]`, or with
  the `2` and the `5` as separate words, `[read, 2, 5, values]` — but
  in every separator-based format it starts one
  only with a separator beside it: `read_25_values`, `READ-25-VALUES`,
  `Read 25 Values`, `Read-25-Values`, `read.25.values` and
  `READ.25.VALUES` are all `[read, 25, values]`, and `read25values`
  (snake) or `Read25values` (title) is the one word `[read25values]`.
- **A capital after a digit starts a new word in `camel`/`pascal`**, so
  `Parse2Files` is `[parse, 2, files]` or `[parse2, files]` there. In
  `title`/`train` it may or may not start one, like any capital partway
  through a token, so `Parse2Files` is `[parse2files]` or
  `[parse2, files]` — but never `[parse, 2, files]`, which would need
  `Parse 2 Files`.
