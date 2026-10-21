*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end test for crank's optional PATH argument.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Crank Runs Against An Explicit Path Argument
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank Against Path    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    Hello widget
