# A mistake in `.flint`

If a `.flint` file can't be read as rules, flint says which line is
wrong and fails, without checking that folder. Here line 2 opens a
placeholder with `{{` but closes it with a single `}`.

Try fixing it to `{{ name }}.md`.

## Files

```text
README.md
guide.md
```

## .flint

```text
README.md
{{ name }.md
```

## Output

```text
Failed to parse .flint: line 2: '{' or '}' used outside a `{{ variable }}` placeholder and not escaped as \{ or \} (in "{{ name }.md")
```
