# Required entries

Put `!` in front of a rule to make it required: at least one thing
matching it must exist, not just be allowed to.

Here `LICENSE` is required but missing. `CHANGELOG.md` is missing
too, but that's fine — it's only allowed.

## Files

```text
README.md
src/
```

## .flint

```text
!README.md
!LICENSE
!src/
CHANGELOG.md
```

## Output

```text
Missing 1 required entry:
- LICENSE
```
