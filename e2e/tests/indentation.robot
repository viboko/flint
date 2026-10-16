*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for indentation-based nesting in `.flint` files.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Indented Nested Literal Path Is Permitted
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/util.rs    fn util() {}
    Write Flint File    ${dir}    src/\n\ \ util.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Unexpected File In Indented Governed Directory Is Reported With Full Path
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/util.rs    fn util() {}
    Create File    ${dir}/src/extra.rs    fn extra() {}
    Write Flint File    ${dir}    src/\n\ \ util.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/extra.rs

Required Child In Indented Block Missing Fails With Full Expanded Path
    [Documentation]    A required entry nested under a `{{ variable }}` directory is
    ...    checked per real matching directory instance — here, the one real `Home/`
    ...    directory that exists is missing its own required file. The missing
    ...    filename is guessed as `HomeManager.py`, not raw `{{ }}` syntax, since
    ...    `name` is resolvable directly from `Home/`'s own real name.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/Home
    Write Flint File    ${dir}    {{ name }}/\n\ \ !{{ name }}Manager.py\n\ \ {{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing
    Should Contain    ${result.stdout}    Home/HomeManager.py

Required Child Present With Optional Sibling Absent Passes
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/Home
    Create File    ${dir}/Home/HomeManager.py    manager
    Write Flint File    ${dir}    {{ name }}/\n\ \ !{{ name }}Manager.py\n\ \ {{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Indented Variable Consistency Is Still Enforced Within One Nested Path
    [Documentation]    `Home/DetailsPresenter.py` matches `{{ name }}/{{ name }}Presenter.py`
    ...    with the directory and filename components disagreeing (Home vs Details) —
    ...    a disagreement within one path, still caught even though it spans two `.flint` lines.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/Home
    Create File    ${dir}/Home/HomeManager.py    manager
    Create File    ${dir}/Home/DetailsPresenter.py    presenter
    Write Flint File    ${dir}    {{ name }}/\n\ \ {{ name }}Manager.py\n\ \ {{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Home/DetailsPresenter.py

Tabs Can Be Used For Indentation
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Create File    ${dir}/src/util.rs    fn util() {}
    Write Flint File    ${dir}    src/\n\tutil.rs\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Indenting Under A Non Directory Line Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    a.txt\n\ \ b.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint

Inconsistent Dedent Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    a/\n\ \ x/\n\ \ \ \ p.txt\n\ c.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
