*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the exact wording and layout of
...              flint's console output, not just which entries it names.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Matching Directory Prints The Success Message
    ${stdout}=    Plain Stdout Of Flint Run    readme.txt\n    readme.txt
    Should Be Equal    ${stdout}    All files match

A File Whose Variables Disagree Is Just Unexpected
    ${stdout}=    Plain Stdout Of Flint Run
    ...    {{ name }}/{{ name }}Manager.py\n    Apple/BananaManager.py    x.txt
    ${expected}=    Catenate
    ...    SEPARATOR=\n
    ...    Found 2 unexpected entries:
    ...    - Apple/BananaManager.py
    ...    - x.txt
    Should Be Equal    ${stdout}    ${expected}

A Group Conflict Lists Each Value With Its Files
    ${stdout}=    Plain Stdout Of Flint Run
    ...    <name>\n${SPACE * 4}{{ name }}.md\n${SPACE * 4}{{ name }}.txt\n
    ...    blah.md    blah.txt    foo.md
    ${expected}=    Catenate
    ...    SEPARATOR=\n
    ...    <name> needs one value per folder, but found 2 in ./:
    ...    - "blah": blah.md, blah.txt
    ...    - "foo": foo.md
    Should Be Equal    ${stdout}    ${expected}

Missing Entries Are Set Off From Unexpected Ones By A Blank Line
    ${stdout}=    Plain Stdout Of Flint Run    !README.md\n    x.txt
    ${expected}=    Catenate    SEPARATOR=\n
    ...    Missing 1 required entry:
    ...    - README.md
    ...    ${EMPTY}
    ...    Found 1 unexpected entry:
    ...    - x.txt
    Should Be Equal    ${stdout}    ${expected}

First Section Printed Has Nothing Before It
    [Documentation]    No blank line, and with a single .flint file, no label naming it.
    ${stdout}=    Plain Stdout Of Flint Run    ${EMPTY}    x.txt
    ${expected}=    Catenate    SEPARATOR=\n
    ...    Found 1 unexpected entry:
    ...    - x.txt
    Should Be Equal    ${stdout}    ${expected}

Counts Use A Singular Or Plural Noun To Match
    ${one}=    Plain Stdout Of Flint Run    !a.md\n    x.txt
    Should Contain    ${one}    Missing 1 required entry:
    Should Contain    ${one}    Found 1 unexpected entry:
    ${two}=    Plain Stdout Of Flint Run    !a.md\n!b.md\n    x.txt    y.txt
    Should Contain    ${two}    Missing 2 required entries:
    Should Contain    ${two}    Found 2 unexpected entries:

Output Is Plain Text With No Color Codes Or Emoji
    [Documentation]    So it reads the same piped, in a log, or through a screen reader.
    ${passing}=    Plain Stdout Of Flint Run    readme.txt\n    readme.txt
    ${failing}=    Plain Stdout Of Flint Run    !README.md\n    x.txt
    Should Match Regexp    ${passing}    ^[\\x20-\\x7e\\n]+$
    Should Match Regexp    ${failing}    ^[\\x20-\\x7e\\n]+$


*** Keywords ***
Plain Stdout Of Flint Run
    [Documentation]    Runs flint in a fresh directory holding ${flint_contents} as its
    ...    .flint file and an empty file at each of @{files} (parent directories
    ...    created as needed), and returns its stdout.
    [Arguments]    ${flint_contents}    @{files}
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    ${flint_contents}
    FOR    ${file}    IN    @{files}
        Create File    ${dir}/${file}
    END
    ${result}=    Run Flint In Directory    ${dir}
    RETURN    ${result.stdout}
