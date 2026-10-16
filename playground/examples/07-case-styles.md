# Case styles

A placeholder can require a case style: `{{ name|kebab }}` only
matches kebab-case text like `date-picker`, and `{{ name|pascal }}`
only PascalCase like `DatePicker`.

Different case styles of the same placeholder still have to be the same
words: `date-picker` and `DatePicker` agree, but `nav-bar` and
`Navbar` don't (`Navbar` is one word), so the rule doesn't allow
`Navbar.tsx`. Nor does it allow a name that doesn't fit the case style
at all, like `user_card.tsx`.

## Files

```text
components/
components/date-picker/
components/date-picker/DatePicker.tsx
components/nav-bar/
components/nav-bar/Navbar.tsx
components/user-card/
components/user-card/user_card.tsx
```

## .flint

```text
components/
    {{ name|kebab }}/
        {{ name|pascal }}.tsx
```

## Output

```text
Found 2 unexpected entries:
- components/nav-bar/Navbar.tsx
- components/user-card/user_card.tsx
```
