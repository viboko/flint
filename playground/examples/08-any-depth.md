# Any depth

`**` stands for any number of folders, including none. So
`src/**/*.rs` allows Rust files anywhere under `src`, however deeply
nested — and only Rust files.

## Files

```text
src/
src/main.rs
src/net/
src/net/mod.rs
src/net/http/
src/net/http/client.rs
src/net/README.md
```

## .flint

```text
src/
    **/
        *.rs
```

## Output

```text
Found 1 unexpected entry:
- src/net/README.md
```
