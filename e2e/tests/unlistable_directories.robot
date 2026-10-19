*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for folders flint isn't allowed to list.

Resource         ../resources/flint.resource

Test Setup       Skip Unless Permissions Can Hide A Folder
Test Teardown    Restore Permissions And Clean Up


*** Variables ***
# Every folder the current test hid, for its teardown to make listable again.
@{HIDDEN_DIRS}    @{EMPTY}


*** Test Cases ***
A Folder That Cannot Be Listed Stops The Run Before Checking
    [Documentation]    `**` allows anything in `secret`, but flint can't see what's
    ...    there, nor whether a flint file below it adds rules.
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    **\n
    Create File    ${dir}/secret/notes.txt    private
    Make Folder Unlistable    ${dir}/secret
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Couldn't list secret, so nothing was checked:
    Should Be Empty    ${result.stdout}

A Folder No Rule Allows Still Stops The Run
    [Documentation]    flint looks for flint files everywhere, so it meets `secret` even
    ...    though `secret` itself would be reported as unexpected.
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    readme.txt\n
    Create File    ${dir}/readme.txt    hello
    Create File    ${dir}/secret/notes.txt    private
    Make Folder Unlistable    ${dir}/secret
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Couldn't list secret, so nothing was checked:
    Should Be Empty    ${result.stdout}


*** Keywords ***
Skip Unless Permissions Can Hide A Folder
    [Documentation]    Windows has no `chmod 000`, and root can list any folder anyway.
    Skip If    sys.platform == 'win32'    Windows folder permissions work differently.
    ${is_root}=    Evaluate    os.geteuid() == 0    modules=os
    Skip If    ${is_root}    Root can list a folder whatever its permissions.

Make Folder Unlistable
    [Documentation]    Takes away every permission on ${path}, so it can't be listed.
    [Arguments]    ${path}
    Evaluate    os.chmod($path, 0)    modules=os
    VAR    @{HIDDEN_DIRS}=    @{HIDDEN_DIRS}    ${path}    scope=TEST    # robocop: off=VAR06

Restore Permissions And Clean Up
    [Documentation]    Test Teardown: makes each hidden folder listable again, so it
    ...    can be removed, then removes the test's directories.
    FOR    ${path}    IN    @{HIDDEN_DIRS}
        Evaluate    os.chmod($path, 0o755)    modules=os
    END
    Cleanup Test Directories
