*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for which keys a .crank file may hold.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Unknown Key Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/t.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: t.tera, output: output.py, mode: 644}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    unknown key "mode" at taskGroups[0].tasks[0]
    File Should Not Exist    ${dir}/output.py

Keys Starting With X Hold Anchors That Aliases Reuse
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a.yaml    name: alpha
    Create File    ${dir}/b.yaml    name: beta
    Create File    ${dir}/template.tera    Hello {{ name }}
    ${crank}=    Catenate    SEPARATOR=\n
    ...    x-tasks: &tasks
    ...    ${SPACE * 2}- template: template.tera
    ...    ${SPACE * 4}output: "{{ name }}.txt"
    ...    taskGroups:
    ...    ${SPACE * 2}- name: a
    ...    ${SPACE * 4}object: a.yaml
    ...    ${SPACE * 4}x-note: ignored
    ...    ${SPACE * 4}tasks: *tasks
    ...    ${SPACE * 2}- name: b
    ...    ${SPACE * 4}object: b.yaml
    ...    ${SPACE * 4}tasks: *tasks
    Write Crank File    ${dir}    ${crank}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${alpha}=    Get File    ${dir}/alpha.txt
    Should Be Equal    ${alpha}    Hello alpha
    ${beta}=    Get File    ${dir}/beta.txt
    Should Be Equal    ${beta}    Hello beta

Merge Key Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    ${crank}=    Catenate    SEPARATOR=\n
    ...    x-task: &task
    ...    ${SPACE * 2}template: template.tera
    ...    taskGroups:
    ...    ${SPACE * 2}- name: group
    ...    ${SPACE * 4}object: object.yaml
    ...    ${SPACE * 4}tasks:
    ...    ${SPACE * 6}- <<: *task
    ...    ${SPACE * 8}output: output.txt
    Write Crank File    ${dir}    ${crank}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    YAML merge keys (`<<`) aren't supported

A Crank File Without Task Groups Fails And Deletes Nothing
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/t.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: t.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Write Crank File    ${dir}    \# everything was here
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    missing field `taskGroups`
    File Should Exist    ${dir}/output.py
