*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `*` wildcard matching.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Wildcard Rule Allows Any Matching File
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Home.py    home
    Create File    ${dir}/Details.py    details
    Write Flint File    ${dir}    *.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Wildcard Rule Flags Non Matching File As Unexpected
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Home.py    home
    Create File    ${dir}/notes.txt    not allowed
    Write Flint File    ${dir}    *.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    notes.txt

Wildcard Rule Does Not Match A Dotfile
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/.py    hidden
    Write Flint File    ${dir}    *.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    .py

Escaped Asterisk In Filename Is Matched Literally
    Skip If    sys.platform == 'win32'    Windows doesn't allow * in a file name.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/weird*file.txt    literal asterisk
    Write Flint File    ${dir}    weird\\*file.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
