*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for Tera loop/filter behavior in templates.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Loop With Loop Last Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    items: [alpha, beta, gamma]
    Create File
    ...    ${dir}/template.tera
    ...    {% for item in items %}{{ item }}{% if not loop.last %}, {% endif %}{% endfor %}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    alpha, beta, gamma

Upper Filter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name | upper }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    WIDGET

Lower Filter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: WIDGET
    Create File    ${dir}/template.tera    {{ name | lower }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    widget
