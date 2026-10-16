# Accented names

Names can be in any language, and flint compares them exactly:
`über` isn't `uber`, so `Uber.md` doesn't belong in the `über` folder.

Case styles know about capitals in every script, so `crème-brûlée`
and `CrèmeBrûlée` are the same words. A script with no capitals, like
Japanese, fits every case style: `日本語` is both kebab-case and
PascalCase. So does an emoji: `🎉-party` goes with `🎉Party`.

Changing case never changes how many letters a name has. The capital
of `ß` here is the single letter `ẞ`, so `große-straße` goes with
`GroßeStraße`, and not with `GrosseStrasse`.

An accented letter can be stored as one character, or as a plain letter
followed by a separate accent, which is how macOS often stores a file
name. The two look the same, and flint treats them as the same: the
`café` folder below is stored the second way, and the file inside it
the first.

## Files

```text
menu/
menu/crème-brûlée/
menu/crème-brûlée/CrèmeBrûlée.md
menu/café/
menu/café/Café.md
menu/日本語/
menu/日本語/日本語.md
menu/🎉-party/
menu/🎉-party/🎉Party.md
menu/große-straße/
menu/große-straße/GroßeStraße.md
menu/große-straße/GrosseStrasse.md
menu/über/
menu/über/Uber.md
```

## .flint

```text
menu/
    {{ name|kebab }}/
        {{ name|pascal }}.md
```

## Output

```text
Found 2 unexpected entries:
- menu/große-straße/GrosseStrasse.md
- menu/über/Uber.md
```
