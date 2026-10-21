*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for `crank --check`: reports what a normal
...              run would create, modify, or remove without writing anything.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
Check Reports Nothing To Change When Everything Is Up To Date
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    ${manifest_before}=    Get File    ${dir}/.crank.manifest
    ${output_before}=    Get File    ${dir}/output.py
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    Should Contain    ${check.stdout}    Nothing would change
    ${manifest_after}=    Get File    ${dir}/.crank.manifest
    ${output_after}=    Get File    ${dir}/output.py
    Should Be Equal    ${manifest_before}    ${manifest_after}
    Should Be Equal    ${output_before}    ${output_after}

Check Reports A File That Would Be Created Without Creating It
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stdout}    output.py
    Should Contain    ${check.stdout}    would create
    File Should Not Exist    ${dir}/output.py
    File Should Not Exist    ${dir}/.crank.manifest

Check Reports A File That Would Be Modified Without Modifying It
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/object.yaml    name: gadget
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stdout}    output.py
    Should Contain    ${check.stdout}    would modify
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    Hello widget

Check Reports A Stale File That Would Be Removed Without Removing It
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: old.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    ${manifest_before}=    Get File    ${dir}/.crank.manifest
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: new.py}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stdout}    old.py
    Should Contain    ${check.stdout}    stale
    File Should Exist    ${dir}/old.py
    File Should Not Exist    ${dir}/new.py
    ${manifest_after}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest_before}    ${manifest_after}

Check Does Not Report A Stale File That Is Already Gone
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/.crank.manifest    version: 1\ntaskGroups: {group: [fake_filename.txt, output.py]}\n
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    Should Contain    ${check.stdout}    Nothing would change
    Should Not Contain    ${check.stdout}    fake_filename.txt

Check In A Directory With No Crank File Behaves Like A Normal Run
    ${dir}=    Create Empty Test Directory
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    Should Contain    ${check.stdout}    No crank file found

Check Still Fails On An Error Without Printing A Change Summary
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: missing.tera, output: output.py}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stderr}    missing.tera
    Should Not Contain    ${check.stdout}    would create
    Should Not Contain    ${check.stdout}    would modify
    File Should Not Exist    ${dir}/output.py

Check Reports Creates Modifies And Removals Together
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: kept.py},
    ...    {template: template.tera, output: old.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/object.yaml    name: gadget
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: kept.py},
    ...    {template: template.tera, output: new.py}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    1
    Should Contain    ${check.stdout}    kept.py
    Should Contain    ${check.stdout}    would modify
    Should Contain    ${check.stdout}    new.py
    Should Contain    ${check.stdout}    would create
    Should Contain    ${check.stdout}    old.py
    Should Contain    ${check.stdout}    stale
