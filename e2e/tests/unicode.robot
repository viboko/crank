*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for non-ASCII text — Japanese, Russian,
...              Korean, Polish, and French, plus emoji — in file and directory
...              names, the `.crank` file, data files, templates, pocket names and
...              contents, and selectors, run against the real filesystem.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Non ASCII File Names Data And Output Paths Round Trip
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/データ
    Create Directory    ${dir}/шаблоны
    Create File    ${dir}/データ/ピザ.yaml    name: Zażółć gęślą jaźń 🍕\n
    Create File    ${dir}/шаблоны/меню.tera    «{{ name }}» — 한국어 ✓\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: メニュー, object: データ/ピザ.yaml, tasks: [{template: шаблоны/меню.tera, output: "출력/{{ name
    ...    }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    Should Be Equal    ${result.stdout}    Generated 1 file\n${SPACE}${SPACE}- 출력/Zażółć gęślą jaźń 🍕.txt (created)
    ${output}=    Get File    ${dir}/출력/Zażółć gęślą jaźń 🍕.txt
    Should Be Equal    ${output}    «Zażółć gęślą jaźń 🍕» — 한국어 ✓\n
    ${manifest}=    Get File    ${dir}/.crank.manifest
    ${expected}=    Catenate    SEPARATOR=${EMPTY}
    ...    version: 1\ntaskGroups:\n
    ...    ${SPACE}${SPACE}メニュー:\n${SPACE}${SPACE}- 출력/Zażółć gęślą jaźń 🍕.txt\n
    Should Be Equal    ${manifest}    ${expected}
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    Should Be Equal    ${check.stdout}    Nothing would change

Renaming A Non ASCII Output Removes The Stale File
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: борщ
    Create File    ${dir}/template.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: g, object: object.yaml, tasks: [{template: template.tera, output: "{{ name }}.txt"}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    File Should Exist    ${dir}/борщ.txt
    Create File    ${dir}/object.yaml    name: 김치
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    Should Contain    ${second.stdout}    Removed 1 stale file\n${SPACE}${SPACE}- борщ.txt
    File Should Not Exist    ${dir}/борщ.txt
    File Should Exist    ${dir}/김치.txt

A Pocket Named After Non ASCII Data Keeps Its Hand Edits
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/menu.yaml    - {id: a, name: 寿司}\n- {id: b, name: Żurek}\n- {id: c, name: crème}\n
    Create File    ${dir}/dish.tera    {{ name }}\n// notes-{{ name }} [\ndefault\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: dishes, array: menu.yaml, tasks: [{template: dish.tera, output: "{{ id }}.txt"}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/a.txt    寿司\n// notes-寿司 [\n手で編集した 🍣\n// ]\n
    Create File    ${dir}/b.txt    Żurek\n// notes-Żurek [\nzupa na zakwasie\n// ]\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${a}=    Get File    ${dir}/a.txt
    Should Be Equal    ${a}    寿司\n// notes-寿司 [\n手で編集した 🍣\n// ]\n
    ${b}=    Get File    ${dir}/b.txt
    Should Be Equal    ${b}    Żurek\n// notes-Żurek [\nzupa na zakwasie\n// ]\n
    ${c}=    Get File    ${dir}/c.txt
    Should Be Equal    ${c}    crème\n// notes-crème [\ndefault\n// ]\n

A Non ASCII Selector Picks Its Sub Object
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/menu.yaml    {メニュー: {ピザ: [{name: マルゲリータ}, {name: ディアボロ}]}}
    Create File    ${dir}/template.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: g, object: "menu.yaml#メニュー/ピザ[1]", tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/out.txt
    Should Be Equal    ${output}    ディアボロ

Snake Case Keeps Letters In Any Script
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    names: ["Crème Brûlée", "Zażółć Gęślą", "приветМир", "日本語 テキスト", "🍕 Pizza"]
    Create File    ${dir}/template.tera    {% for n in names %}{{ n | snake_case }}\n{% endfor %}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: g, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/out.txt
    Should Be Equal    ${output}    crème_brûlée\nzażółć_gęślą\nпривет_мир\n日本語_テキスト\npizza\n

A Non ASCII Filter Name Fails The Run Up Front
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: x
    Create File    ${dir}/template.tera    {{ name }}
    Create File    ${dir}/odwróć.rhai    fn filter(value) { value }
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: g, object: object.yaml, tasks: [{template: template.tera, output:
    ...    out.txt}]}]\nfilters: {odwróć: odwróć.rhai}\n
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Filter `odwróć` can't be called from a template
    File Should Not Exist    ${dir}/out.txt

A Non ASCII Path Argument Works
    ${dir}=    Create Empty Test Directory
    ${project}=    Join Path    ${dir}    プロジェクト 🚀
    Create Directory    ${project}
    Create File    ${project}/object.yaml    name: widget
    Create File    ${project}/template.tera    {{ name }}
    Write Crank File
    ...    ${project}
    ...    taskGroups: [{name: g, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank Against Path    ${project}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${project}/out.txt
    Should Be Equal    ${output}    widget
