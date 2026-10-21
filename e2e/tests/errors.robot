*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for crank's error reporting.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Malformed Yaml In Crank File Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Write Crank File    ${dir}    taskGroups: [this is not: valid: yaml:
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank

Unquoted Output Starting With A Substitution Says To Quote It
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    ${crank}=    Catenate    SEPARATOR=\n
    ...    taskGroups:
    ...    ${SPACE * 2}- name: group
    ...    ${SPACE * 4}object: object.yaml
    ...    ${SPACE * 4}tasks:
    ...    ${SPACE * 6}- template: template.tera
    ...    ${SPACE * 8}output: {{ name }}.txt
    Write Crank File    ${dir}    ${crank}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    output
    Should Contain    ${result.stderr}    must be quoted

Missing Data File Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: missing.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    missing.yaml

Missing Template File Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: missing.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    missing.tera

Undefined Template Variable Names The Variable In The Error
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ nmae }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    template.tera
    Should Contain    ${result.stderr}    nmae

One Broken Task Does Not Stop Other Tasks From Running
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: missing.tera, output: broken.py},
    ...    {template: template.tera, output: ok.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    ${output}=    Get File    ${dir}/ok.py
    Should Be Equal    ${output}    Hello widget
    File Should Not Exist    ${dir}/broken.py

Missing Task Group Name Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank

Duplicate Task Group Names Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a.yaml    name: alpha
    Create File    ${dir}/b.yaml    name: beta
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: dup, object: a.yaml, tasks: [{template: template.tera, output: a.py}]}, {name: dup,
    ...    object: b.yaml, tasks: [{template: template.tera, output: b.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank
    Should Contain    ${result.stderr}    dup
