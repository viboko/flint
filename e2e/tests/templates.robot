*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `{{ variable }}` template placeholders.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Matching Template Pair Passes
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/HomeManager.py    manager
    Create File    ${dir}/HomePresenter.py    presenter
    Write Flint File    ${dir}    {{ name }}Manager.py\n{{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Mismatched Flat Template Pair Passes (Not Reconciled Across Lines)
    [Documentation]    Two separate, un-nested `.flint` lines never have to agree
    ...    on a variable's value — see docs/usage.md's consistency rule.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/HomeManager.py    manager
    Create File    ${dir}/DetailsPresenter.py    presenter
    Write Flint File    ${dir}    {{ name }}Manager.py\n{{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Compound Nested Template With Mismatched Components Is Unexpected
    [Documentation]    Unlike two separate lines, the two components of *one* nested
    ...    path (here, the directory and filename) still have to agree.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Apple/BananaManager.py    manager
    Write Flint File    ${dir}    {{ name }}/{{ name }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    unexpected
    Should Contain    ${result.stdout}    Apple/BananaManager.py

Another Rule Allows A File A Placeholder Rule Doesn't
    [Documentation]    `.flint` is an allow-list: `README.md` doesn't fit
    ...    `{{ name }}/{{ name }}.md` (it captures both `apple` and `README`), but
    ...    the literal `README.md` rule allows it, so it isn't reported.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/apple/apple.md    notes
    Create File    ${dir}/apple/README.md    readme
    Write Flint File    ${dir}    {{ name }}/\n${SPACE}{{ name }}.md\n${SPACE}README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Every Matching Nested Directory Is Permitted Independently
    [Documentation]    Many differently-named `{{ name }}` directories can coexist,
    ...    each with its own required files, and satisfying one required entry in
    ...    one directory never satisfies the same rule for a different directory.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Apple/AppleManager.py    manager
    Create File    ${dir}/Apple/ApplePresenter.py    presenter
    Create File    ${dir}/Banana/BananaManager.py    manager
    Create File    ${dir}/Banana/BananaPresenter.py    presenter
    Write Flint File    ${dir}    {{ name }}/\n\ \ !{{ name }}Manager.py\n\ \ !{{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Nested Directory Missing Its Own Required File Fails Per Instance
    [Documentation]    Banana's missing Presenter is reported with a concrete guessed
    ...    filename, not raw `{{ }}` syntax, since `name` is resolvable directly from
    ...    the governing directory's own real name — see "Missing Entry Filename Is
    ...    Guessed From A Satisfied Sibling" for the sibling-inference case.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Apple/AppleManager.py    manager
    Create File    ${dir}/Apple/ApplePresenter.py    presenter
    Create File    ${dir}/Banana/BananaManager.py    manager
    Write Flint File    ${dir}    {{ name }}/\n\ \ !{{ name }}Manager.py\n\ \ !{{ name }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Banana/BananaPresenter.py
    Should Not Contain    ${result.stdout}    {{ name }}
    Should Not Contain    ${result.stdout}    Apple

Missing Entry Filename Is Guessed From A Satisfied Sibling
    [Documentation]    The governing directory here is literal, not templated, so
    ...    the guess for the missing Presenter can only come from the already-present
    ...    Manager sibling within the same directory.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/foo/BlahManager.py    manager
    Write Flint File    ${dir}    foo/\n\ \ !{{ name|pascal }}Manager.py\n\ \ !{{ name|pascal }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    foo/BlahPresenter.py
    Should Not Contain    ${result.stdout}    {{ name

Missing Entry Filename Names The Variable It Knows Beside One It Does Not
    [Documentation]    `name` is the governing directory's own real name, but nothing
    ...    says what `blah` is, so only `blah` keeps its raw `{{ }}` text.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/All The World
    Write Flint File    ${dir}    {{ name|title }}/\n\ \ !{{ blah|snake }}.{{ name|kebab }}\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    All The World/{{ blah|snake }}.all-the-world
    Should Not Contain    ${result.stdout}    {{ name

Missing Entry Filename Falls Back To Raw Template When Nothing Resolves It
    [Documentation]    An empty directory has no sibling and no templated governing
    ...    directory to infer a value from, so both missing entries keep today's raw
    ...    `{{ }}` template text.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/foo
    Write Flint File    ${dir}    foo/\n\ \ !{{ name|pascal }}Manager.py\n\ \ !{{ name|pascal }}Presenter.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    foo/{{ name|pascal }}Manager.py
    Should Contain    ${result.stdout}    foo/{{ name|pascal }}Presenter.py

Escaped Braces In Filename Are Matched Literally
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/weird{file}.txt    braces in the name
    Write Flint File    ${dir}    weird\\{file\\}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Template Variable With No Matching File On Disk Is Fine
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Unrelated Unexpected File Still Reported Alongside Templates
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/HomeManager.py    manager
    Create File    ${dir}/intruder.txt    not allowed
    Write Flint File    ${dir}    {{ name }}Manager.py\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    intruder.txt

Malformed Template Syntax Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name }}{{ other }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to parse .flint
    Should Contain    ${result.stderr}    ambiguous
