*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for trailing-slash directory/file matching.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Directory Entry With Trailing Slash Matches A Real Directory
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    src/\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Directory Entry With Trailing Slash Rejects A File Of The Same Name
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src    not a directory
    Write Flint File    ${dir}    src/\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src

File Entry Without Trailing Slash Rejects A Directory Of The Same Name
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    src\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src

Symlink To A Directory Is Treated As A File
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/target_dir
    Create Directory Symlink    target_dir    ${dir}/link
    Write Flint File    ${dir}    target_dir/\nlink\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Symlink To A Directory Does Not Satisfy A Directory-Only Entry
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/target_dir
    Create Directory Symlink    target_dir    ${dir}/link
    Write Flint File    ${dir}    target_dir/\nlink/\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    link
