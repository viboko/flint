# Advanced groups

A `!` can go on a group's header, on the rules under it, or on both, and
each says something different about the files.

A `!` on a rule under a group requires that file only once its folder
has a value for the group. An empty folder has no value, so it passes:
the files are optional, but come as a complete set. A `!` on the header
itself, `!<a>`, requires every folder it sits in to have a value — at
least one of the group's files. Use both to require every file in every
folder.

So here, all three folders are empty, but only two are reported: the
third's files are only required once one of them exists. Add
`if_one_file_exists_then_the_other_must_exist/x.txt` and it reports
`x.md` missing too.

## Files

```text
at_least_one_file_must_exist/
both_files_must_exist/
if_one_file_exists_then_the_other_must_exist/
```

## .flint

```text
!at_least_one_file_must_exist/
    !<a>
        {{ a }}.txt
        {{ a }}.md
!both_files_must_exist/
    !<a>
        !{{ a }}.txt
        !{{ a }}.md
!if_one_file_exists_then_the_other_must_exist/
    <a>
        !{{ a }}.txt
        !{{ a }}.md
```

## Output

```text
Missing 2 required entries:
- at_least_one_file_must_exist/<a>
- both_files_must_exist/<a>
```
