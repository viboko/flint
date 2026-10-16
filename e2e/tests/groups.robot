*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `<name>` group headers, which share a
...              variable's value across the rules indented under them, one value
...              per real folder.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Variables ***
${PAIRED}         src/\n${SPACE * 4}<name>\n${SPACE * 8}{{ name }}.md\n${SPACE * 8}{{ name }}.txt\n
${REQUIRED}       src/\n${SPACE * 4}<name>\n${SPACE * 8}!{{ name }}.md\n${SPACE * 8}!{{ name }}.txt\n
${REQUIRED_AB}    SEPARATOR=
...               src/\n${SPACE * 4}<a, b>\n${SPACE * 8}!{{ a }}.txt\n${SPACE * 8}!{{ a }}.jpg\n
...               ${SPACE * 8}!{{ b }}.md\n${SPACE * 8}!{{ b }}.gif\n


*** Test Cases ***
Files Sharing One Value Pass
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/blah.md    notes
    Create File    ${dir}/src/blah.txt    text
    Write Flint File    ${dir}    ${PAIRED}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Files Disagreeing On The Value Fail With Every Value Listed
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/blah.md    notes
    Create File    ${dir}/src/foo.txt    text
    Write Flint File    ${dir}    ${PAIRED}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/<name> needs one value per folder
    Should Contain    ${result.stdout}    src/blah.md
    Should Contain    ${result.stdout}    src/foo.txt

A Folder That Gives No Value Is Left Out Of The Disagreement
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a/foo.gif    image
    Create File    ${dir}/a/foo2.png    image
    Create Directory    ${dir}/b
    Write Flint File    ${dir}    <y>\n${SPACE * 4}**/\n${SPACE * 8}{{ y }}.png\n${SPACE * 8}{{ y }}.gif\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    <y> needs one value per folder, but found 2 in ./:
    Should Contain    ${result.stdout}    - "foo": a/foo.gif\n- "foo2": a/foo2.png
    Should Not Contain    ${result.stdout}    b/

Two Files Under The Same Rule Must Share The Value Too
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/blah.md    notes
    Create File    ${dir}/src/foo.md    notes
    Write Flint File    ${dir}    ${PAIRED}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    needs one value per folder

A File Another Rule Allows Does Not Count Toward The Value
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/blah.md    notes
    Create File    ${dir}/src/README.md    readme
    Write Flint File    ${dir}    src/\n${SPACE * 4}<name>\n${SPACE * 8}{{ name }}.md\n${SPACE * 4}README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Required Rules In A Group Require Nothing In An Empty Folder
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    ${REQUIRED}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Required Group Header Requires A Set In Its Folder
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/src
    Write Flint File    ${dir}    src/\n${SPACE * 4}!<name>\n${SPACE * 8}{{ name }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/<name>

A Missing File In A Group Is Named With The Folder's Value
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/blah.md    notes
    Write Flint File    ${dir}    ${REQUIRED}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    src/blah.txt

A Group's Required Rule Waits For A Value For Each Variable It Uses
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/foo.txt    text
    Write Flint File    ${dir}    ${REQUIRED_AB}
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Missing 1 required entry:\n- src/foo.jpg
    Should Not Contain    ${result.stdout}    {{ b }}

A Missing File In A Group Is Named With Its Folder's Name Too
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/blah/x.png    notes
    Write Flint File
    ...    ${dir}
    ...    {{ name|snake }}/\n${SPACE * 4}<foo>\n${SPACE * 8}!{{ name|title }}.md\n${SPACE * 8}{{ foo }}.png\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    blah/Blah.md

A Group Header Variable Nothing Uses Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    <name>\n${SPACE * 4}README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    no rule indented under it uses

An Unescaped Angle Bracket In A Rule Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    a<b.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    escape it as

Escaped Angle Brackets Are Matched Literally
    Skip If    sys.platform == 'win32'    Windows doesn't allow < or > in a file name.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/<odd>.txt    literal brackets
    Write Flint File    ${dir}    \\<odd\\>.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Group Under Any Depth Has One Value Per Real Folder
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/src/a/blah.md    notes
    Create File    ${dir}/src/a/blah.txt    text
    Create File    ${dir}/src/b/c/foo.md    notes
    Create File    ${dir}/src/b/c/bar.txt    text
    Write Flint File
    ...    ${dir}
    ...    src/\n${SPACE * 4}**/\n${SPACE * 8}<name>\n${SPACE * 12}{{ name }}.md\n${SPACE * 12}{{ name }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    found 2 in src/b/c/
    Should Not Contain    ${result.stdout}    src/a/

A Malformed Group Header Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    <name>/\n${SPACE * 4}{{ name }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    holds only variable names

A Group Header Rebinding A Folder's Variable Is A Parse Error
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    {{ name }}/\n${SPACE * 4}<name>\n${SPACE * 8}{{ name }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    "name" is already bound here
