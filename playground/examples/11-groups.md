# Groups

A plain folder like `report/` doesn't capture a name the way
`{{ name }}/` does, so its files can't be required to match it. A
`<name>` group header fills that gap: it matches nothing on disk, but
every rule indented under it shares `name`, so in each folder, every
file those rules allow must agree on one value.

Here the report and its exported PDF should share a name, but the PDF's
has a typo. Neither file is to blame on its own, so flint lists each
value it found.

## Files

```text
report/
report/q3-result.pdf
report/q3-results.md
```

## .flint

```text
report/
    <name>
        {{ name }}.md
        {{ name }}.pdf
```

## Output

```text
report/<name> needs one value per folder, but found 2 in report/:
- "q3-result": report/q3-result.pdf
- "q3-results": report/q3-results.md
```
