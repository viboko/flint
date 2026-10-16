*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `|format` case-style filters on
...              `{{ variable }}` template placeholders.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Variables ***
# One file name per format, each in that format's shape and with its own
# extension, for `All Ten Formats Match Their Own Shape`.
&{FORMAT_EXAMPLES}
...                   camel=homePage.txt
...                   pascal=HomePage.md
...                   snake=home_page.rs
...                   screamingSnake=HOME_PAGE.json
...                   kebab=home-page.yaml
...                   screamingKebab=HOME-PAGE.toml
...                   title=Home Page.ini
...                   train=Home-Page.cfg
...                   dot=home.page.log
...                   screamingDot=HOME.PAGE.conf


*** Test Cases ***
Camel And Pascal Occurrences Of The Same Variable Are Consistent
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/foo
    Create File    ${dir}/foo/FooManager.py    manager
    Write Flint File    ${dir}    {{ name|camel }}/\n${SPACE}{{ name|pascal }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Cross Format Disagreement Is Reported As Unexpected
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/foo
    Create File    ${dir}/foo/BarManager.py    manager
    Write Flint File    ${dir}    {{ name|camel }}/\n${SPACE}{{ name|pascal }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    foo/BarManager.py

Same Format Repeated On Separate Lines Is Never Reconciled
    [Documentation]    `A_{{ name|pascal }}.txt` and `B_{{ name|pascal }}.txt` are two
    ...    separate `.flint` lines — never reconciled against each other, format or not.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/A_XMLFile.txt    a
    Create File    ${dir}/B_XmlFile.txt    b
    Write Flint File    ${dir}    A_{{ name|pascal }}.txt\nB_{{ name|pascal }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Snake Format Rejects Non Snake Shaped Names
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/HomePage.txt    not snake case
    Write Flint File    ${dir}    {{ name|snake }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    HomePage.txt

Unknown Format Name Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name|kebob }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
    Should Contain    ${result.stderr}    unknown format
    Should Contain    ${result.stderr}    screamingKebab, title, train, dot, screamingDot

Mixing Formatted And Unformatted Occurrences Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name }}_{{ name|camel }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
    Should Contain    ${result.stderr}    name

Mixing Formatted And Unformatted Occurrences Across Separate Rules Is Allowed
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create Directory    ${dir}/tests
    Create File    ${dir}/src/home_page.rs    lib
    Create File    ${dir}/tests/HomePage.rs    test
    Write Flint File    ${dir}    src/\n${SPACE}{{ name|snake }}.rs\ntests/\n${SPACE}{{ name }}.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Mixing Formatted And Unformatted Occurrences Across A Header And Child Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name|camel }}/\n${SPACE}{{ name }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
    Should Contain    ${result.stderr}    line 2

All Ten Formats Match Their Own Shape
    ${dir}=    Create Empty Test Directory
    # Each file gets a .flint line of its own, e.g. `{{ name|camel }}.txt`.
    VAR    ${flint}=    ${EMPTY}
    FOR    ${format}    ${file}    IN    &{FORMAT_EXAMPLES}
        Create File    ${dir}/${file}    ${format}
        ${extension}=    Fetch From Right    ${file}    .
        VAR    ${flint}=    ${flint}{{ name|${format} }}.${extension}\n
    END
    Write Flint File    ${dir}    ${flint}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Train And Pascal Occurrences Of The Same Variable Are Consistent
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/Foo-Bar
    Create File    ${dir}/Foo-Bar/FooBarManager.py    manager
    Write Flint File    ${dir}    {{ name|train }}/\n${SPACE}{{ name|pascal }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Title And Train Formats Accept An All Caps Initialism
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Parse-XML-File.txt    train-case
    Create File    ${dir}/Parse XML File.md    title case
    Write Flint File    ${dir}    {{ name|train }}.txt\n{{ name|title }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Digit May Be Part Of A Word Or A Word Of Its Own
    [Documentation]    `parse_file2` and `parse_file_2` both match `parseFile2`:
    ...    a digit beside a letter may or may not start a new word.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/parseFile2
    Create File    ${dir}/parseFile2/parse_file2.rs    joined
    Create Directory    ${dir}/parseFile3
    Create File    ${dir}/parseFile3/parse_file_3.rs    split
    Write Flint File    ${dir}    {{ name|camel }}/\n${SPACE}{{ name|snake }}.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Word That Is Only Digits Is Found Again In Pascal Case
    [Documentation]    `1-2-3` is `123` in PascalCase, which has no way to
    ...    mark a word starting with a digit — so `123` is what's required,
    ...    and it has to be accepted when it's there.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/1-2-3
    Create File    ${dir}/1-2-3/123    the three words run together
    Create Directory    ${dir}/file-1-2
    Write Flint File    ${dir}    {{ name|kebab }}/\n${SPACE}!{{ name|pascal }}\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    file-1-2/File12
    Should Not Contain    ${result.stdout}    123

A Separator Before A Digit Is Still A Word Break
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/parse_file_2
    Create File    ${dir}/parse_file_2/parsefile2.txt    no break between parse and file
    Write Flint File    ${dir}    {{ name|snake }}/\n${SPACE}{{ name|kebab }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    parse_file_2/parsefile2.txt

Title And Train Formats Allow Capitals Inside A Word
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Watch YouTube.txt    a capital inside a word
    Create File    ${dir}/Parse2Files.txt    a capital after a digit
    Create File    ${dir}/Watch-iPhone.md    a word starting lowercase
    Write Flint File    ${dir}    {{ name|title }}.txt\n{{ name|train }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Watch-iPhone.md
    Should Not Contain    ${result.stdout}    YouTube
    Should Not Contain    ${result.stdout}    Parse2Files
