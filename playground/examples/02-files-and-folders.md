# Files and folders

A trailing `/` makes a rule match only a folder; without one, it
matches only a file. So `docs/` allows a folder called `docs`, and
`build` allows a *file* called `build` — here there's a folder called
`build` and a file called `docs`, so neither is allowed.

A folder's contents are checked too, so on its own, `assets/` allows
only an *empty* `assets` folder: `logo.png` and `fonts/` are reported.
Writing `assets/**` instead would allow the folder and anything in it;
the next examples show how to say exactly what goes inside.

## Files

```text
build/
build/output.bin
docs
assets/
assets/logo.png
assets/fonts/
```

## .flint

```text
docs/
build
assets/
```

## Output

```text
Found 4 unexpected entries:
- assets/fonts
- assets/logo.png
- build
- docs
```
