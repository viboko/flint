*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the optional PATH argument.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Explicit Path Matching Flint File Passes
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Write Flint File    ${dir}    readme.txt\n
    ${result}=    Run Flint Against Path    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Explicit Path With Unexpected File Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Create File    ${dir}/intruder.txt    not allowed
    Write Flint File    ${dir}    readme.txt\n
    ${result}=    Run Flint Against Path    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    intruder.txt

Nonexistent Path Fails With A Clear Error
    ${dir}=    Create Empty Test Directory
    ${missing}=    Join Path    ${dir}    does-not-exist
    ${result}=    Run Flint Against Path    ${missing}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Can't check ${missing}

A Path To A File Fails With A Clear Error
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    ${result}=    Run Flint Against Path    ${dir}/readme.txt
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    it isn't a folder

No Argument Defaults To The Current Directory
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Write Flint File    ${dir}    readme.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
