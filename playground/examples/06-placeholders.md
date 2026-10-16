# Placeholders

A `{{ name }}` placeholder matches any text, so one rule can allow a
whole family of names.

Within one matched path, every use of the same placeholder must capture
the same text. Here each component's folder name must match the start of
the files inside it: `Modal/DialogStyles.css` captures both `Modal` and
`Dialog` for `name`, so that rule doesn't allow it. No other rule does
either, so flint reports it as unexpected.

## Files

```text
components/
components/Button/
components/Button/ButtonView.tsx
components/Button/ButtonStyles.css
components/Modal/
components/Modal/ModalView.tsx
components/Modal/DialogStyles.css
```

## .flint

```text
components/
    {{ name }}/
        {{ name }}View.tsx
        {{ name }}Styles.css
```

## Output

```text
Found 1 unexpected entry:
- components/Modal/DialogStyles.css
```
