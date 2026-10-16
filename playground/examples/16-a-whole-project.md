# A whole project

A `.flint` file for a typical Rust project, using most of the above
together. Everything here is allowed, so flint passes.

Try adding a file somewhere, or naming a source file in camelCase.

## Files

```text
Cargo.toml
Cargo.lock
README.md
LICENSE-MIT
LICENSE-APACHE
src/
src/main.rs
src/lib.rs
src/file_format.rs
tests/
tests/cli.rs
docs/
docs/usage.md
docs/images/
docs/images/logo.png
target/
target/debug/
.github/
.github/workflows/
.github/workflows/ci.yml
.gitignore
```

## .flint

```text
# Rust
!Cargo.toml
!Cargo.lock
!src/
    {{ module|snake }}.rs
tests/
    {{ test|snake }}.rs
target/**     # build output: anything goes

# Documentation
!README.md
!LICENSE-*
docs/
    **/
        *.md
        *.png

# Tooling
.github/
    workflows/
        *.yml
.gitignore
```

## Output

```text
All files match
```
