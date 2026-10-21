*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for .crank.manifest: crank's record of
...              previously generated outputs, used to delete files a .crank file no
...              longer produces.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
First Run Creates A Manifest Listing The Generated Output
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    output.py

Renaming A Tasks Output Deletes The Old File And Updates The Manifest
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: old.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    File Should Exist    ${dir}/old.py
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: new.py}]}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    File Should Not Exist    ${dir}/old.py
    ${output}=    Get File    ${dir}/new.py
    Should Be Equal    ${output}    Hello widget
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    new.py
    Should Not Contain    ${manifest}    old.py

A Failed Run Leaves The Manifest And Stale Outputs Untouched
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: ok.py},
    ...    {template: template.tera, output: doomed.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    ${manifest_before}=    Get File    ${dir}/.crank.manifest
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: ok.py},
    ...    {template: missing.tera, output: broken.py}]}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    1
    File Should Exist    ${dir}/doomed.py
    ${manifest_after}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest_before}    ${manifest_after}

A Stale Output Already Deleted By Hand Is Not An Error
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: old.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Remove File    ${dir}/old.py
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: new.py}]}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/new.py
    Should Be Equal    ${output}    Hello widget
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    new.py
    Should Not Contain    ${manifest}    old.py
    Should Not Contain    ${second.stdout}    Removed
    Should Not Contain    ${second.stdout}    old.py

A Manifest Entry For A File That Never Existed Is Dropped Without Being Reported
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    Create File    ${dir}/.crank.manifest    version: 1\ntaskGroups: {group: [fake_filename.txt, output.py]}\n
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    Should Not Contain    ${result.stdout}    Removed
    Should Not Contain    ${result.stdout}    fake_filename.txt
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest}    version: 1\ntaskGroups:\n${SPACE}${SPACE}group:\n${SPACE}${SPACE}- output.py\n

An Array Groups Outputs Are All Tracked And Shrinking It Cleans Up Stale Files
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}, {name: Diabolo}]
    Create File    ${dir}/pizza.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, array: "object.yaml#pizzas", tasks: [{template: pizza.tera, output: "pizza-{{
    ...    name }}.txt"}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    File Should Exist    ${dir}/pizza-Hawaiian.txt
    File Should Exist    ${dir}/pizza-Diabolo.txt
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    pizza-Hawaiian.txt
    Should Contain    ${manifest}    pizza-Diabolo.txt
    Create File    ${dir}/object.yaml    pizzas: [{name: Hawaiian}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    File Should Exist    ${dir}/pizza-Hawaiian.txt
    File Should Not Exist    ${dir}/pizza-Diabolo.txt
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    pizza-Hawaiian.txt
    Should Not Contain    ${manifest}    pizza-Diabolo.txt

Malformed Manifest Is Reported And Fails Before Anything Runs
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    Create File    ${dir}/.crank.manifest    [this is not: valid: yaml:
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank.manifest
    File Should Not Exist    ${dir}/output.py

A Manifest Without A Version Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    Create File    ${dir}/.crank.manifest    group: [output.py]\n
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank.manifest
    File Should Not Exist    ${dir}/output.py

A Manifest With An Unsupported Version Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    Create File    ${dir}/.crank.manifest    version: 2\ntaskGroups: {group: [output.py]}\n
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    it records version 2, but this crank only reads version 1
    File Should Not Exist    ${dir}/output.py

Manifest Groups Outputs By Task Group Name
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/a.yaml    name: alpha
    Create File    ${dir}/b.yaml    name: beta
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: groupA, object: a.yaml, tasks: [{template: template.tera, output: a.py}]}, {name:
    ...    groupB, object: b.yaml, tasks: [{template: template.tera, output: b.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${manifest}=    Get File    ${dir}/.crank.manifest
    ${expected}=    Catenate    SEPARATOR=${EMPTY}
    ...    version: 1\ntaskGroups:\n
    ...    ${SPACE}${SPACE}groupA:\n${SPACE}${SPACE}- a.py\n
    ...    ${SPACE}${SPACE}groupB:\n${SPACE}${SPACE}- b.py\n
    Should Be Equal    ${manifest}    ${expected}

A Rerun With Nothing To Change Leaves The Manifest Alone
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Set Modified Time    ${dir}/.crank.manifest    2001-02-03 04:05:06
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${modified}=    Get Modified Time    ${dir}/.crank.manifest
    Should Be Equal    ${modified}    2001-02-03 04:05:06

Respelling An Output Path With A Dot Keeps The File And Its Pockets
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // notes [\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: ./out.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/out.txt    // notes [\nmine\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    Should Not Contain    ${second.stdout}    Removed
    ${output}=    Get File    ${dir}/out.txt
    Should Be Equal    ${output}    // notes [\nmine\n// ]\n
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest}    version: 1\ntaskGroups:\n${SPACE}${SPACE}group:\n${SPACE}${SPACE}- out.txt\n

Renaming An Output In Letter Case Keeps The File On A Case-Insensitive Filesystem
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/probe.txt    probe
    ${insensitive}=    Run Keyword And Return Status    File Should Exist    ${dir}/PROBE.TXT
    Skip If    not ${insensitive}    the temp directory's filesystem tells letter case apart
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // notes [\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: Out.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/Out.txt    // notes [\nmine\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${check}=    Run Crank Check In Directory    ${dir}
    Should Be Equal As Integers    ${check.rc}    0
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    Should Not Contain    ${second.stdout}    Removed
    ${output}=    Get File    ${dir}/out.txt
    Should Be Equal    ${output}    // notes [\nmine\n// ]\n

Re-encoding An Accent In An Output Path Keeps The File On macOS
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/caf\u00e9-probe.txt    probe
    # Looked for in Python: Robot's own file keywords turn every path into
    # NFC, so they'd find the probe under either encoding.
    ${insensitive}=    Evaluate    os.path.exists(os.path.join($dir, 'cafe\u0301-probe.txt'))    modules=os
    Skip If    not ${insensitive}    the temp directory's filesystem tells accent encodings apart
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // notes [\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera,
    ...    output: "caf\u00e9.txt"}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/caf\u00e9.txt    // notes [\nmine\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera,
    ...    output: "cafe\u0301.txt"}]}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    Should Not Contain    ${second.stdout}    Removed
    ${output}=    Get File    ${dir}/caf\u00e9.txt
    Should Be Equal    ${output}    // notes [\nmine\n// ]\n
