*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the reserved `crank` variable.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Crank Filename Renders The Output File's Name
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // {{ crank.filename }}: {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml,
    ...    tasks: [{template: template.tera, output: "src/{{ name }}.rs"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/src/widget.rs
    Should Be Equal    ${output}    // widget.rs: widget

Data Defining Crank Is An Error
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    crank: mine
    Create File    ${dir}/template.tera    {{ crank }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    `crank` is a reserved name
    File Should Not Exist    ${dir}/output.txt
