# Looking inside folders

flint looks inside every folder a rule allows, and reports anything
there that no rule allows. A rule can name something inside a folder:
indenting lines under a folder is shorthand for writing its path on
each line, so the `.flint` file below means exactly the same as
`src/main.rs` and `src/lib.rs`.

`tests/**` allows anything at all inside `tests/`, so its contents
aren't checked one by one.

## Files

```text
src/
src/main.rs
src/lib.rs
src/old.rs
tests/
tests/anything.rs
tests/fixtures/
```

## .flint

```text
src/
    main.rs
    lib.rs
tests/**
```

## Output

```text
Found 1 unexpected entry:
- src/old.rs
```
