*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Shared keywords for driving the compiled flint binary as a black box.

Library          OperatingSystem
Library          Process
Library          String


*** Variables ***
${FLINT_BINARY}    %{FLINT_BINARY}
# Every directory the current test created, for its teardown to remove.
@{TEST_DIRS}       @{EMPTY}


*** Keywords ***
Create Empty Test Directory
    [Documentation]    Creates a fresh, empty temp directory and returns its path.
    ...    Cleanup Test Directories removes it again in the test's teardown.
    ${suffix}=    Generate Random String    8    [LETTERS][NUMBERS]
    ${dir}=    Join Path    ${TEMPDIR}    flint-e2e-${suffix}
    Create Directory    ${dir}
    # Test scope is the point: it's how the teardown finds this test's
    # directories, and it resets to empty for the next test.
    VAR    @{TEST_DIRS}=    @{TEST_DIRS}    ${dir}    scope=TEST    # robocop: off=VAR06
    RETURN    ${dir}

Cleanup Test Directories
    [Documentation]    Test Teardown: removes every directory the test created with
    ...    Create Empty Test Directory.
    FOR    ${dir}    IN    @{TEST_DIRS}
        Remove Directory    ${dir}    recursive=True
    END

Write Flint File
    [Documentation]    Writes ${content} as the .flint file inside ${dir}.
    [Arguments]    ${dir}    ${content}
    Create File    ${dir}/.flint    ${content}

Write Nested Flint File
    [Documentation]    Writes ${content} as the .flint file inside ${dir}/${subpath},
    ...    creating ${subpath} first if needed.
    [Arguments]    ${dir}    ${subpath}    ${content}
    Create Directory    ${dir}/${subpath}
    Create File    ${dir}/${subpath}/.flint    ${content}

Create Directory Symlink
    [Documentation]    Creates ${link} as a symbolic link to the directory ${target}
    ...    (relative to ${link}'s folder, or absolute). Through Python rather
    ...    than `ln -s`, whose Git Bash version on Windows copies the folder
    ...    instead of linking it.
    [Arguments]    ${target}    ${link}
    Evaluate    os.symlink($target, $link, target_is_directory=True)    modules=os

Run Flint In Directory
    [Documentation]    Runs the flint binary with ${dir} as its working directory.
    [Arguments]    ${dir}
    ${result}=    Run Process    ${FLINT_BINARY}    cwd=${dir}
    RETURN    ${result}

Run Flint Against Path
    [Documentation]    Runs the flint binary from ${TEMPDIR}, passing ${path} as its
    ...    PATH argument, so the check runs against ${path} without it being the cwd.
    [Arguments]    ${path}
    ${result}=    Run Process    ${FLINT_BINARY}    ${path}    cwd=${TEMPDIR}
    RETURN    ${result}
