# An allow-list

A `.flint` file lists what's allowed to exist in its folder, one name
per line. Anything else is reported, and flint fails.

Listing something doesn't make it required: `LICENSE` is listed here
but doesn't exist, and that's fine. Text after a `#` is a comment. The
`.flint` file never needs to list itself.

Try deleting `notes.txt`, or adding it to `.flint`.

## Files

```text
README.md
Cargo.toml
notes.txt
```

## .flint

```text
# Everything allowed in this folder
README.md
Cargo.toml
LICENSE
```

## Output

```text
Found 1 unexpected entry:
- notes.txt
```
