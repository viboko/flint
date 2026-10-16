# Dot names

Hidden files and folders, like `.github/`, are part of a project like
anything else: flint checks them, and finds `.flint` files inside them
too.

But a wildcard never matches a dot name by accident, the same way shell
globs don't: `*.md` doesn't allow `.notes.md`, and `src/**/*.rs` doesn't
reach into `src/.cache/`, so both are reported. Name them to allow them.

The one exception is a `**` at the end of a line, which means
"everything in this folder": `target/**` allows `target/.rustc_info.json`
too.

## Files

```text
.github/
.github/workflows/
.github/workflows/ci.yml
.notes.md
README.md
src/
src/main.rs
src/.cache/
src/.cache/build.rs
target/
target/.rustc_info.json
target/debug/
target/debug/app
```

## .flint

```text
*.md
src/
    **/*.rs
target/**
.github/
    workflows/
        *.yml
```

## Output

```text
Found 2 unexpected entries:
- .notes.md
- src/.cache
```
