*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the flint binary.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
No Flint File Reports Friendly Message And Succeeds
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    Should Contain    ${result.stdout}    No flint file found

Directory Matching Flint File Passes
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Create File    ${dir}/main.rs    fn main() {}
    Write Flint File    ${dir}    readme.txt\nmain.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Unexpected File Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Create File    ${dir}/intruder.txt    not allowed
    Write Flint File    ${dir}    readme.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    intruder.txt

Allowed File Missing On Disk Is Not An Error
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Write Flint File    ${dir}    readme.txt\nnever-created.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Comments And Escaped Hash Are Handled
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/readme.txt    hello
    Create File    ${dir}/weird#file.txt    hash in the name
    Write Flint File    ${dir}    readme.txt # the project readme\nweird\\#file.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Flint File Itself Is Never Required To Be Listed
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    ${EMPTY}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
