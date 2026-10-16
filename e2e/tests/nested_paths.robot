*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `/`-separated multi-level paths.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Nested Literal Path Is Permitted
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/util.rs    fn util() {}
    Write Flint File    ${dir}    src/util.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Bare Folder Rule Allows Only An Empty Folder
    [Documentation]    flint looks inside every folder a rule allows; `docs/` alone says
    ...    nothing may be inside. The unexpected `docs/anything` folder is reported,
    ...    but not its contents.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/docs/anything
    Create File    ${dir}/docs/anything/whatever.md    notes
    Write Flint File    ${dir}    docs/\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    docs/anything
    Should Not Contain    ${result.stdout}    whatever.md

A Trailing Any Depth Allows Everything In A Folder
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/target/debug/app    binary
    Create File    ${dir}/target/.rustc_info.json    {}
    Create File    ${dir}/target/debug/.fingerprint/x    hash
    Write Flint File    ${dir}    target/**\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Unexpected File In Governed Directory Is Reported With Full Path
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/util.rs    fn util() {}
    Create File    ${dir}/src/extra.rs    fn extra() {}
    Write Flint File    ${dir}    src/util.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/extra.rs

Intermediate Directory Needs No Explicit Rule
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/nested
    Create File    ${dir}/src/nested/deep.rs    fn deep() {}
    Write Flint File    ${dir}    src/nested/deep.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Wildcard Intermediate Path Component Matches Any Directory Name
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/linux
    Create Directory    ${dir}/src/macos
    Create File    ${dir}/src/linux/main.py    linux app
    Create File    ${dir}/src/macos/main.py    macos app
    Write Flint File    ${dir}    src/*/main.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Variable Intermediate Path Component Is Independent Per Directory
    [Documentation]    `src/{{ platform }}/main.py` and `src/{{ platform }}/config.py`
    ...    are two separate `.flint` lines — different real `platform` subdirectories
    ...    never have to agree, so `linux` and `macos` can coexist here.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src/linux
    Create Directory    ${dir}/src/macos
    Create File    ${dir}/src/linux/main.py    linux app
    Create File    ${dir}/src/macos/config.py    macos info
    Write Flint File    ${dir}    src/{{ platform }}/main.py\nsrc/{{ platform }}/config.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Required Nested Path Missing Fails
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    !src/util.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing
    Should Contain    ${result.stdout}    src/util.rs
