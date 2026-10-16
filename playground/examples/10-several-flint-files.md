# Several `.flint` files

Any folder can have its own `.flint` file, checked against that
folder's own contents. When there's more than one, flint labels each
problem with the `.flint` file it came from.

A rule in a parent `.flint` file that reaches into such a folder adds
to that folder's own rules. Here the root allows a `LICENSE` in every
package, so `packages/api` can have one without its own `.flint` file
listing it.

## Files

```text
README.md
packages/
packages/api/
packages/api/LICENSE
packages/api/package.json
packages/api/src/
packages/web/
packages/web/package.json
packages/web/notes.md
```

## .flint

```text
!README.md
packages/
    */
        LICENSE
```

## packages/api/.flint

```text
!package.json
src/
```

## packages/web/.flint

```text
!package.json
```

## Output

```text
packages/web/.flint
Found 1 unexpected entry:
- packages/web/notes.md
```
