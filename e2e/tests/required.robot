*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `!` required-entry markers.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Required File Present Passes
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/README.md    readme
    Write Flint File    ${dir}    !README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Required File Missing Fails With Message Naming It
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing
    Should Contain    ${result.stdout}    README.md

Required Wildcard Satisfied By Any Matching File Passes
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/CHANGELOG.md    changelog
    Write Flint File    ${dir}    !*.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Required Wildcard With No Match Fails
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !*.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing
    Should Contain    ${result.stdout}    *.md

Escaped Leading Exclamation Mark Is Matched Literally
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/!important.txt    literal exclamation mark
    Write Flint File    ${dir}    \\!important.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Bare Required Marker Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint

A File Whose Variables Disagree Does Not Satisfy A Required Rule
    [Documentation]    `Apple/BananaManager.py` fits the shape of `!{{ name }}Manager.py`
    ...    under `Apple/`, but captures `Banana` where the folder captured `Apple`, so
    ...    the rule doesn't allow it, and `Apple/AppleManager.py` is still missing.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Apple/BananaManager.py    manager
    Write Flint File    ${dir}    {{ name }}/\n${SPACE}!{{ name }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Apple/AppleManager.py
    Should Contain    ${result.stdout}    Apple/BananaManager.py

A Required Line Requires The Folders It Names
    [Documentation]    `!{{ name }}/{{ name }}.txt` is short for `!{{ name }}/` with
    ...    `!{{ name }}.txt` under it, so an empty `blah/` lacks its `blah/` folder.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/blah
    Write Flint File    ${dir}    {{ name }}/\n${SPACE}!{{ name }}/{{ name }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing 1 required entry
    Should Contain    ${result.stdout}    blah/blah/blah.txt

A Required Line Is Satisfied By Its Folder And File
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/blah/blah/blah.txt    text
    Write Flint File    ${dir}    {{ name }}/\n${SPACE}!{{ name }}/{{ name }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Required Line At The Top Level Requires Its Folder
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    !{{ name }}/{{ name }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    {{ name }}/{{ name }}.txt
