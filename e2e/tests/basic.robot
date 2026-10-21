*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the crank binary: basic rendering.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
No Crank File Reports Friendly Message And Succeeds
    ${dir}=    Create Empty Test Directory
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    Should Contain    ${result.stdout}    No crank file found

Single Group With Single Task Renders Output
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    Hello widget

Multiple Groups And Multiple Tasks Are All Rendered
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a.yaml    name: alpha
    Create File    ${dir}/b.yaml    name: beta
    Create File    ${dir}/one.tera    One: {{ name }}
    Create File    ${dir}/two.tera    Two: {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: a, object: a.yaml, tasks: [{template: one.tera, output: a-one.txt}, {template:
    ...    two.tera, output: a-two.txt}]}, {name: b, object: b.yaml, tasks: [{template: one.tera, output:
    ...    b-one.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${a_one}=    Get File    ${dir}/a-one.txt
    ${a_two}=    Get File    ${dir}/a-two.txt
    ${b_one}=    Get File    ${dir}/b-one.txt
    Should Be Equal    ${a_one}    One: alpha
    Should Be Equal    ${a_two}    Two: alpha
    Should Be Equal    ${b_one}    One: beta
