# Required in every folder

A required rule under a placeholder folder is checked in every folder
it matches. Here every package must have its own `package.json` and
`README.md` — and `web` has no `README.md`.

## Files

```text
packages/
packages/api/
packages/api/package.json
packages/api/README.md
packages/api/src/
packages/web/
packages/web/package.json
packages/web/src/
```

## .flint

```text
packages/
    {{ package }}/
        !package.json
        !README.md
        src/
```

## Output

```text
Missing 1 required entry:
- packages/web/README.md
```
