*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the `object` field's `#selector` syntax.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Selector Picks Out A Sub Object From A List
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian, price: 17.99}, {name: Diabolo, price: 19.99}]
    Create File    ${dir}/template.tera    {{ name }}: {{ price }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: "object.yaml#pizzas[0]", tasks: [{template: template.tera, output:
    ...    output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    Hawaiian: 17.99

Selector With An Out Of Range Index Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian, price: 17.99}, {name: Diabolo, price: 19.99}]
    Create File    ${dir}/template.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: "object.yaml#pizzas[5]", tasks: [{template: template.tera, output:
    ...    output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    object.yaml
    Should Contain    ${result.stderr}    pizzas[5]
    File Should Not Exist    ${dir}/output.txt

Selector Resolving To An Array Instead Of An Object Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian, price: 17.99}, {name: Diabolo, price: 19.99}]
    Create File    ${dir}/template.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml#pizzas, tasks: [{template: template.tera, output:
    ...    output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    object.yaml
    Should Contain    ${result.stderr}    not an object
    File Should Not Exist    ${dir}/output.txt
