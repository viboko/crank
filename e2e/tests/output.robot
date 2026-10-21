*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for crank's output-writing behavior.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Existing Output File Is Overwritten
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Create File    ${dir}/output.py    stale content
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    Hello widget

Output Path With Missing Parent Directory Is Created
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    generated/output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/generated/output.py
    Should Be Equal    ${output}    Hello widget

An Object Groups Output Path Supports Substitution
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: "{{ name }}.py"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/widget.py
    Should Be Equal    ${output}    Hello widget

Output Is Plain Text Without Colors Or Emoji
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    Should Be Equal    ${result.stdout}    Generated 1 file\n${SPACE}${SPACE}- output.py (created)
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    Should Be Equal    ${check.stdout}    Nothing would change

An Output Already Holding What Is Generated Is Not Rewritten
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/models.yaml    [{name: users}, {name: orders}]
    Create File    ${dir}/model.tera    model {{ name }}
    Create File    ${dir}/models/users.js    model users
    Create File    ${dir}/models/orders.js    stale content
    Set Modified Time    ${dir}/models/users.js    2001-02-03 04:05:06
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: models, array: models.yaml, tasks: [{template: model.tera, output:
    ...    "models/{{ name }}.js"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${expected}=    Catenate    SEPARATOR=\n
    ...    Generated 2 files
    ...    ${SPACE}${SPACE}- models/users.js (unchanged)
    ...    ${SPACE}${SPACE}- models/orders.js (modified)
    Should Be Equal    ${result.stdout}    ${expected}
    ${modified}=    Get Modified Time    ${dir}/models/users.js
    Should Be Equal    ${modified}    2001-02-03 04:05:06
    ${orders}=    Get File    ${dir}/models/orders.js
    Should Be Equal    ${orders}    model orders

Two Task Groups Writing The Same Output Write Nothing
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/a.tera    from a
    Create File    ${dir}/b.tera    from b
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: a, object: object.yaml, tasks: [{template: a.tera, output: o.txt}, {template: a.tera,
    ...    output: other.txt}]}, {name: b, object: object.yaml, tasks: [{template: b.tera, output: ./o.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    ${expected}=    Catenate
    ...    Two tasks generate the same file — "o.txt" from task group "a" (template a.tera), and "./o.txt" from
    ...    task group "b" (template b.tera) — so nothing was written
    Should Be Equal    ${result.stderr}    ${expected}
    File Should Not Exist    ${dir}/o.txt
    File Should Not Exist    ${dir}/other.txt
    File Should Not Exist    ${dir}/.crank.manifest
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stderr}    Two tasks generate the same file

Outputs Differing Only In Letter Case Collide
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/t.tera    text
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: a, object: object.yaml, tasks: [{template: t.tera, output: o.txt}, {template: t.tera,
    ...    output: O.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    differ only in letter case
    File Should Not Exist    ${dir}/o.txt

Outputs Differing Only In How An Accent Is Encoded Collide
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/t.tera    text
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: a, object: object.yaml, tasks: [{template: t.tera, output: "caf\u00e9.txt"},
    ...    {template: t.tera, output: "cafe\u0301.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    how accented letters are encoded
    File Should Not Exist    ${dir}/caf\u00e9.txt
