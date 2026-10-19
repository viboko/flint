*** Comments ***
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for file and folder names outside
...              plain English: accented letters, however the disk stores them,
...              other scripts, and names that aren't valid Unicode at all.

Resource         ../resources/flint.resource

Test Teardown    Cleanup Test Directories


*** Variables ***
# `é` is one character here, the way a keyboard types it. The keywords
# below store it decomposed instead, the way macOS often does: as `e`
# followed by a combining accent.
${CAFE}    café


*** Test Cases ***
Names In Other Scripts Match Their Rules
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/größe.txt    size
    Create File    ${dir}/日本語/ファイル.txt    file
    Create File    ${dir}/привет мир.md    hello world
    Write Flint File    ${dir}    !größe.txt\n!日本語/\n${SPACE}!ファイル.txt\n!привет мир.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

An Unlisted Name In Another Script Is Reported As Written
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/größe.txt    size
    Write Flint File    ${dir}    grosse.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    größe.txt

A Name Stored Decomposed Matches A Rule Typed Composed
    ${dir}=    Create Empty Test Directory
    Create Decomposed File    ${dir}    ${CAFE}.txt
    Write Flint File    ${dir}    !${CAFE}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Name Stored Composed Matches A Rule Typed Decomposed
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/${CAFE}.txt    menu
    Write Decomposed Flint File    ${dir}    !${CAFE}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

A Folder Stored Decomposed Is Looked Inside And Reported Composed
    ${dir}=    Create Empty Test Directory
    Create Decomposed File    ${dir}    ${CAFE}/menu.txt
    Create Decomposed File    ${dir}    ${CAFE}/stray.txt
    Write Flint File    ${dir}    ${CAFE}/\n${SPACE}menu.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    ${CAFE}/stray.txt
    Should Not Contain    ${result.stdout}    menu.txt

A Flint File In A Folder Stored Decomposed Is Read
    ${dir}=    Create Empty Test Directory
    Write Flint File    ${dir}    */\n
    Create Decomposed File    ${dir}    ${CAFE}/.flint    !README.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    ${CAFE}/.flint
    Should Contain    ${result.stdout}    README.md

An Accent Is Not Ignored
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/cafe.txt    no accent
    Write Flint File    ${dir}    ${CAFE}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    cafe.txt

Case Filters Accept Accented Names
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/crème_brûlée/CrèmeBrûlée.txt    dessert
    Create File    ${dir}/CRÈME-BRÛLÉE.md    dessert
    Create File    ${dir}/Crème Brûlée.ini    dessert
    ${rules}=    Catenate    SEPARATOR=\n
    ...    {{ name|snake }}/
    ...    ${SPACE}!{{ name|pascal }}.txt
    ...    !{{ name|screamingKebab }}.md
    ...    !{{ name|title }}.ini
    Write Flint File    ${dir}    ${rules}\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Case Filters Accept Emoji
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/😀-😀/😀_😀    smile
    Create File    ${dir}/🎉-party/PARTY_🎉    party
    Write Flint File    ${dir}    {{ name|kebab }}/\n${SPACE}{{ name|snake }}\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    🎉-party/PARTY_🎉
    Should Not Contain    ${result.stdout}    😀

Case Filters Accept A Name Stored Decomposed
    ${dir}=    Create Empty Test Directory
    Create Decomposed File    ${dir}    ${CAFE}_bar.txt
    Write Flint File    ${dir}    !{{ name|snake }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0

Case Filters Check The Case Of Accented Letters
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/Über.txt    capital
    Create File    ${dir}/élan.txt    lowercase
    Write Flint File    ${dir}    {{ name|snake }}.txt\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    Über.txt
    Should Not Contain    ${result.stdout}    élan.txt

A Missing File's Guessed Name Keeps Its Accents
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/große_straße
    Write Flint File    ${dir}    {{ name|snake }}/\n${SPACE}!{{ name|screamingSnake }}.md\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    große_straße/GROẞE_STRAẞE.md

Rules Differing Only In Case Need A File Each
    [Documentation]    True on every file system, though one that ignores case —
    ...    the default on macOS and Windows — can't hold both files at once.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/foo    lowercase
    Write Flint File    ${dir}    !foo\n!FOO\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stdout}    FOO

A Name That Is Not Unicode Fails The Run And Is Named
    [Documentation]    Linux only: macOS refuses to create such a name, and Windows
    ...    names aren't bytes.
    Skip If    sys.platform != 'linux'    Only Linux allows a file name that isn't valid UTF-8.
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/menus
    # `café.txt` in Latin-1, where `é` is the single byte E9.
    Evaluate    open(os.path.join(os.fsencode($dir), b'menus', b'caf\\xe9.txt'), 'w').close()
    Write Flint File    ${dir}    **\n
    ${result}=    Run Flint In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}
    ...    Couldn't list menus, so nothing was checked: the name "caf\\xE9.txt" isn't valid Unicode
    Should Not Contain    ${result.stdout}    All files match


*** Keywords ***
Create Decomposed File
    [Documentation]    Creates ${path} inside ${dir}, holding ${content}, with every
    ...    accented letter in ${path} stored as a letter and a combining accent.
    ...    Python does it, since Robot Framework's own keywords compose a path first.
    [Arguments]    ${dir}    ${path}    ${content}=${EMPTY}
    ${file}=    Evaluate    pathlib.Path($dir, unicodedata.normalize('NFD', $path))
    Evaluate    $file.parent.mkdir(parents=True, exist_ok=True)
    Evaluate    $file.write_text($content, encoding='utf-8')

Write Decomposed Flint File
    [Documentation]    Writes ${content} as the .flint file inside ${dir}, with every
    ...    accented letter stored as a letter and a combining accent.
    [Arguments]    ${dir}    ${content}
    ${decomposed}=    Evaluate    unicodedata.normalize('NFD', $content)
    Evaluate    pathlib.Path($dir, '.flint').write_text($decomposed, encoding='utf-8')
