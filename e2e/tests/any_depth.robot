*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `**` (any-depth) wildcards.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Any Depth Wildcard Matches File At The Top Level
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/main.py    top level app
    Write Flint File    ${dir}    src/**/*.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Any Depth Wildcard Matches File Several Levels Deep
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/linux/nested
    Create File    ${dir}/src/linux/nested/main.py    deep app
    Write Flint File    ${dir}    src/**/*.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Any Depth Wildcard Skips Dot Directories
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/.git
    Create File    ${dir}/src/.git/main.py    should not be reached
    Write Flint File    ${dir}    src/**/*.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/.git

File Not Matching The Suffix Is Reported As Unexpected
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/linux
    Create File    ${dir}/src/linux/notes.txt    not a python file
    Write Flint File    ${dir}    src/**/*.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/linux/notes.txt

Required Any Depth Rule With No Match Fails
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    !src/**/*.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing
    Should Contain    ${result.stdout}    src/**/*.py

Second Double Star In One Line Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    a/**/b/**/c\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
