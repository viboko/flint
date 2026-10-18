*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for recursive discovery of multiple
...              `.flint` files across subdirectories.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Nested Flint File Several Levels Deep Is Discovered And Its Violation Attributed To It
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    a/\n
    Write Nested Flint File    ${dir}    a/b/c    README.md\n
    Create File    ${dir}/a/b/c/README.md    readme
    Create File    ${dir}/a/b/c/stray.txt    not allowed
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    a/b/c/.flint
    Should Contain    ${result.stdout}    a/b/c/stray.txt

Nested Flint File With No Violations Reports Nothing For That Scope
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    packages/\n${SPACE * 4}*/\n
    Write Nested Flint File    ${dir}    packages/foo    README.md\n
    Create File    ${dir}/packages/foo/README.md    readme
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Root And Nested Scope Violations Are Both Reported And Labeled With Their Flint File
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/intruder.txt    not allowed
    Write Flint File    ${dir}    packages/\n
    Write Nested Flint File    ${dir}    packages/foo    README.md\n
    Create File    ${dir}/packages/foo/stray.txt    not allowed
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    intruder.txt
    Should Contain    ${result.stdout}    packages/foo/.flint
    Should Contain    ${result.stdout}    packages/foo/stray.txt

A Clean Scope Prints Nothing When Another Scope Has Violations
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    clean/\ndirty/\n
    Write Nested Flint File    ${dir}    clean    README.md\n
    Create File    ${dir}/clean/README.md    readme
    Write Nested Flint File    ${dir}    dirty    README.md\n
    Create File    ${dir}/dirty/stray.txt    not allowed
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    dirty/.flint
    Should Not Contain    ${result.stdout}    clean/.flint

Exit Code Reflects A Violation In A Nested Scope Even When The Root Scope Is Clean
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    packages/\n
    Write Nested Flint File    ${dir}    packages/foo    README.md\n
    Create File    ${dir}/packages/foo/stray.txt    not allowed
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1

Symlinked Directory Containing A Flint File Is Not Discovered
    ${dir}=    Create Empty Test Directory
    ${outside}=    Create Empty Test Directory
    Create File    ${outside}/.flint    x/\n
    Create File    ${outside}/y.txt    should never be checked
    Create Directory Symlink    ${outside}    ${dir}/link
    Write Flint File    ${dir}    link\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Dot Named Directory Containing A Flint File Is Discovered
    ${dir}=    Create Empty Test Directory
    Write Nested Flint File    ${dir}    .hidden    x/\n
    Create File    ${dir}/.hidden/y.txt    checked by .hidden/.flint
    Write Flint File    ${dir}    .hidden/\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    .hidden/.flint
    Should Contain    ${result.stdout}    .hidden/y.txt

A Nested Flint File's Folder Must Still Be Allowed By Its Parent
    [Documentation]    A nested `.flint` file describes its folder's contents; whether
    ...    the folder may exist at all is still up to the parent's rules.
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    packages/\n
    Write Nested Flint File    ${dir}    packages/foo    README.md\n
    Create File    ${dir}/packages/foo/README.md    readme
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    packages/foo

Parse Error In One Scope Does Not Block A Sibling Scope
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    good/\nbad/\n
    Write Nested Flint File    ${dir}    good    README.md\n
    Create File    ${dir}/good/intruder.txt    not allowed
    Write Nested Flint File    ${dir}    bad    !\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    good/intruder.txt
    Should Contain    ${result.stderr}    Failed to parse bad/.flint

A Parent Rule Reaching Into A Scope Permits A File The Scope Does Not List Itself
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    src/util.rs\n
    Write Nested Flint File    ${dir}    src    ${EMPTY}
    Create File    ${dir}/src/util.rs    fn util() {}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Required Parent Rule Reaching Into A Scope Is Satisfied By Content Found There
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !src/util.rs\n
    Write Nested Flint File    ${dir}    src    ${EMPTY}
    Create File    ${dir}/src/util.rs    fn util() {}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Required Parent Rule Reaching Into A Scope Is Reported Missing Exactly Once
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !src/util.rs\n
    Write Nested Flint File    ${dir}    src    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    ${count}=    Get Count    ${result.stdout}    Missing
    Should Be Equal As Integers    ${count}    1

Cascading Is A Union: A File Neither Side Permits Is Still Unexpected
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    src/util.rs\n
    Write Nested Flint File    ${dir}    src    ${EMPTY}
    Create File    ${dir}/src/util.rs    fn util() {}
    Create File    ${dir}/src/stray.rs    not permitted by either file
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/stray.rs

No Flint File Anywhere In The Tree Is An Error
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/nested
    Create File    ${dir}/src/nested/file.txt    hello
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    No flint file found

Nested Flint Files Need One At The Root
    [Documentation]    Without a flint file where it starts, flint looks no further, so
    ...    one in a folder below isn't checked.
    ${dir}=    Create Empty Test Directory
    Write Nested Flint File    ${dir}    src    util.rs\n
    Create File    ${dir}/src/stray.rs    not permitted
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Be Empty    ${result.stdout}
    Should Contain    ${result.stderr}    No flint file found

A Folder's Value Carries Into Its Own Flint File's Check
    [Documentation]    The root's `{{ name }}/{{ name }}.swift`, imported into `Home/`'s
    ...    own check, still needs `name` to be `Home`.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Home/Home.swift    view
    Create File    ${dir}/Home/Other.swift    view
    Write Flint File    ${dir}    {{ name }}/\n${SPACE * 4}{{ name }}.swift\n
    Write Flint File    ${dir}/Home    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Home/Other.swift
    Should Not Contain    ${result.stdout}    Home/Home.swift

An Imported Required Rule Is Reported Missing From The Flint File That Wrote It
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/Home
    Write Flint File    ${dir}    {{ name }}/\n${SPACE * 4}!{{ name }}.swift\n
    Write Flint File    ${dir}/Home    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    - Home/Home.swift
    Should Not Contain    ${result.stdout}    {{ name }}
    Should Not Contain    ${result.stdout}    Home/.flint

A Flint File In One Folder Leaves A Required Rule Checked In The Others
    [Documentation]    An empty `cafe/.flint` changes nothing: both folders still
    ...    need their file, and both are reported from the root `.flint` file,
    ...    which is where the rule is written.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/blah
    Create Directory    ${dir}/cafe
    Write Flint File    ${dir}    {{ name|snake }}/\n${SPACE * 4}!{{ name|pascal }}\n
    Write Flint File    ${dir}/cafe    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    - blah/Blah
    Should Contain    ${result.stdout}    - cafe/Cafe
    Should Not Contain    ${result.stdout}    cafe/.flint

A Nested Flint File Cannot Split A Group's Files
    [Documentation]    The root's group ties `a.txt` to `docs/b.md`, and an empty
    ...    `docs/.flint` must change nothing.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a.txt    text
    Create File    ${dir}/docs/b.md    notes
    Write Flint File
    ...    ${dir}
    ...    <name>\n${SPACE * 4}{{ name }}.txt\n${SPACE * 4}docs/\n${SPACE * 8}{{ name }}.md\n
    Write Flint File    ${dir}/docs    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    <name> needs one value per folder
    Should Contain    ${result.stdout}    docs/b.md
