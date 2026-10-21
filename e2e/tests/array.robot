*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for the `array` task group field: running a
...              group's tasks once per item in a YAML sequence, instead of once against a
...              single `object`.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Array Group Generates One File Per Item
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian, price: 17.99}, {name: Diabolo, price: 19.99}]
    Create File    ${dir}/pizza.tera    {{ name }} costs {{ price }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas", tasks: [{template: pizza.tera, output: "pizza-{{
    ...    name|lower }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${hawaiian}=    Get File    ${dir}/pizza-hawaiian.txt
    Should Be Equal    ${hawaiian}    Hawaiian costs 17.99
    ${diabolo}=    Get File    ${dir}/pizza-diabolo.txt
    Should Be Equal    ${diabolo}    Diabolo costs 19.99

Array With No Selector Uses The Data Files Top Level Sequence
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/pizzas.yaml    - {name: Hawaiian}\n- {name: Diabolo}\n
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: pizzas.yaml, tasks: [{template: pizza.tera, output: "pizza-{{ name
    ...    }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    File Should Exist    ${dir}/pizza-Hawaiian.txt
    File Should Exist    ${dir}/pizza-Diabolo.txt

Both Object And Array Fields Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}]
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, array: "object.yaml#pizzas", tasks: [{template:
    ...    pizza.tera, output: "pizza-{{ name }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank

Neither Object Nor Array Field Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File    ${dir}    taskGroups: [{name: group, tasks: [{template: pizza.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank

Array Group Task Output With No Substitution Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}]
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas", tasks: [{template: pizza.tera, output: pizza.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    substitution
    File Should Not Exist    ${dir}/pizza.txt

Array Selector Resolving To An Object Instead Of An Array Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}]
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas[0]", tasks: [{template: pizza.tera, output:
    ...    "pizza-{{ name }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    object.yaml
    Should Contain    ${result.stderr}    not an array

An Item That Is Not A Mapping Is Reported And Skipped While Others Still Generate
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}, "not an object"]
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas", tasks: [{template: pizza.tera, output: "pizza-{{
    ...    name }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    item [1]
    ${output}=    Get File    ${dir}/pizza-Hawaiian.txt
    Should Be Equal    ${output}    Hawaiian

Two Items Colliding On The Same Rendered Output Is Reported And Nothing Is Written
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian, price: 17.99}, {name: Hawaiian, price: 19.99}]
    Create File    ${dir}/pizza.tera    {{ price }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas", tasks: [{template: pizza.tera, output: "pizza-{{
    ...    name }}.txt"}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Two tasks generate the same file
    Should Contain    ${result.stderr}    item [1]
    File Should Not Exist    ${dir}/pizza-Hawaiian.txt
