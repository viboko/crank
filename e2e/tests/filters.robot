*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for custom Rhai-scripted Tera filters.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Custom Rhai Filter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name | backwards }}
    Create File
    ...    ${dir}/backwards.rhai
    ...    fn filter(value) {\nlet result = "";\nfor ch in value {\nresult = ch.to_string() + result;\n}\nresult\n}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    output.txt}]}]\nfilters: {backwards: backwards.rhai}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    tegdiw

Broken Filter Script Fails The Whole Run Before Any Output Is Written
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name | upper }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    output.txt}]}]\nfilters: {broken: missing.rhai}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    missing.rhai
    File Should Not Exist    ${dir}/output.txt

Redeclaring A Built In Filter Name Fails The Whole Run
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name | wrap }}
    Create File    ${dir}/identity.rhai    fn filter(value) { value }
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    output.txt}]}]\nfilters: {wrap: identity.rhai}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    wrap
    File Should Not Exist    ${dir}/output.txt

A Custom Filter Colliding With A Tera Built In Fails Its Render
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}
    Create File    ${dir}/identity.rhai    fn filter(value) { value }
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    output.txt}]}]\nfilters: {upper: identity.rhai}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    upper
    File Should Not Exist    ${dir}/output.txt

Built In Snake Case Filter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: userName
    Create File    ${dir}/template.tera    {{ name | snake_case }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    user_name

Built In Wrap Filter With A Named Parameter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    text: the quick brown fox jumps
    Create File    ${dir}/template.tera    {{ text | wrap(width=10) }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    the quick\nbrown fox\njumps

Custom Filter With A Named Parameter Renders Correctly
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: world
    Create File    ${dir}/template.tera    {{ name | greet(suffix="!") }}
    Create File    ${dir}/greet.rhai    fn filter(value, args) {\nvalue + args.get("suffix")\n}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output:
    ...    output.txt}]}]\nfilters: {greet: greet.rhai}
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    world!
