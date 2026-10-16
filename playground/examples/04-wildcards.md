# Wildcards

`*` matches any run of characters within a name, so `*.md` allows
every Markdown file.

Like a shell glob, a leading `*` never matches a name starting with
`.`, so `.draft.md` isn't allowed by `*.md` — dotfiles have to be
listed on purpose.

## Files

```text
README.md
CONTRIBUTING.md
.draft.md
Makefile
notes.txt
```

## .flint

```text
*.md
Makefile
```

## Output

```text
Found 2 unexpected entries:
- .draft.md
- notes.txt
```
