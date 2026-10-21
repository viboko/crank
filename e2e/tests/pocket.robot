*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests for pocket markers: `// name [` ... `// ]`
...              (whole-line) and `/* name [ */` ... `/* ] */` (short, inline)
...              pairs in a template's rendered output whose content survives a
...              re-render, read back from the task group's previously generated
...              output files (found via `.crank.manifest`) rather than any
...              separate store. Also covers a template's `{# crank: ... #}`
...              header replacing the built-in markers with its own.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Variables ***
# A template header replacing the built-in pocket markers with `#` ones.
${HASH_HEADER}    {#- crank: {pocketMarkers: [{start: '# {name} [', end: '# ]'}]} -#}


*** Test Cases ***
First Run Writes The Fresh Default Pocket Content
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n// notes [\ndefault content\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    widget\n// notes [\ndefault content\n// ]\n

Second Run Preserves Hand Edited Pocket Content While The Rest Still Rerenders
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n// notes [\ndefault content\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    widget\n// notes [\nEDITED CONTENT\n// ]\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    revised\n// notes [\nEDITED CONTENT\n// ]\n

A Pocket Transfers To A Different File When Templates Are Restructured
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/file-a.tera    // notes [\ndefault\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: file-a.tera, output: file-a.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    File Should Exist    ${dir}/file-a.txt
    Create File    ${dir}/file-a.txt    // notes [\nEDITED\n// ]\n
    Create File    ${dir}/file-b.tera    // notes [\ndefault\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: file-b.tera, output: file-b.txt}]}]
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/file-b.txt
    Should Be Equal    ${output}    // notes [\nEDITED\n// ]\n
    File Should Not Exist    ${dir}/file-a.txt

Two Tasks Producing The Same Pocket Name Fails The Run
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template-a.tera    // dup [\na\n// ]\n
    Create File    ${dir}/template-b.tera    // dup [\nb\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template-a.tera, output: a.txt},
    ...    {template: template-b.tera, output: b.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    dup
    File Should Exist    ${dir}/a.txt
    File Should Not Exist    ${dir}/b.txt

An Orphaned Pocket Is Silently Dropped
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n// notes [\ndefault\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    widget\n// notes [\nEDITED\n// ]\n
    Create File    ${dir}/template.tera    {{ name }}\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    Should Not Contain    ${second.stderr}    notes
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    widget\n

An Unclosed Pocket Marker Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // broken [\nno closing marker\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    broken
    Should Contain    ${result.stderr}    never closed
    File Should Not Exist    ${dir}/output.txt

A Stray End Marker Is Reported And Fails
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    no start here\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    no preceding start marker
    File Should Not Exist    ${dir}/output.txt

Whitespace Variations In Markers Round Trip Edited Content
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    //\tspaced\t[\ncontent\n//\t]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    //\tspaced\t[\nEDITED\n//\t]\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    //\tspaced\t[\nEDITED\n//\t]\n

Short Form First Run Writes The Fresh Default Pocket Content
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n/* notes [ */default content/* ] */\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    widget\n/* notes [ */default content/* ] */\n

Short Form Second Run Preserves Hand Edited Pocket Content While The Rest Still Rerenders
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n/* notes [ */default content/* ] */\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    widget\n/* notes [ */EDITED CONTENT/* ] */\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    revised\n/* notes [ */EDITED CONTENT/* ] */\n

Long Form And Short Form Pockets Coexist In The Same File
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File
    ...    ${dir}/template.tera
    ...    {{ name }}\n// notes [\nline default\n// ]\n/* tuning [ */42/* ] */\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File
    ...    ${dir}/output.txt
    ...    widget\n// notes [\nEDITED LINE\n// ]\n/* tuning [ */99/* ] */\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal
    ...    ${output}
    ...    revised\n// notes [\nEDITED LINE\n// ]\n/* tuning [ */99/* ] */\n

A Short Form Pocket Spans Multiple Lines
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n/* notes [ */\nline one\nline two\n/* ] */\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    widget\n/* notes [ */\nline one\nline two\n/* ] */\n
    Create File    ${dir}/output.txt    widget\n/* notes [ */\nEDITED\nMULTI\nLINE\n/* ] */\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    revised\n/* notes [ */\nEDITED\nMULTI\nLINE\n/* ] */\n

Overlapping Pocket Markers From Different Forms Fails The Run
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    // outer [\n/* inner [ */content/* ] */\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    overlap
    File Should Not Exist    ${dir}/output.txt

Short Form Whitespace Variations Round Trip Edited Content
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    /*\tspaced\t[\t*/content/*\t]\t*/\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    /*\tspaced\t[\t*/EDITED/*\t]\t*/\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    /*\tspaced\t[\t*/EDITED/*\t]\t*/\n

A Template Header Replaces The Built In Markers And Keeps Hand Edits
    [Documentation]    A Python template lists `#` markers in its header, so
    ...    floor division (`a // b[0]`) isn't read as a built-in `//` pocket, and
    ...    the header itself never reaches the output.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File
    ...    ${dir}/template.tera
    ...    ${HASH_HEADER}\n{{ name }} = a // b[0]\n# notes [\ndefault\n# ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    widget = a // b[0]\n# notes [\ndefault\n# ]\n
    Create File    ${dir}/output.py    widget = a // b[0]\n# notes [\nEDITED\n# ]\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    revised = a // b[0]\n# notes [\nEDITED\n# ]\n

A Template Headers Markers Are Recorded In The Manifest
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File
    ...    ${dir}/template.tera
    ...    ${HASH_HEADER}\n# notes [\ndefault\n# ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.py}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    - path: output.py
    Should Contain    ${manifest}    pocketMarkers:
    Should Contain    ${manifest}    - start: '# {name} ['

Old Pocket Content Survives After A Template Drops Its Header
    [Documentation]    Proves old-file pocket scanning uses the markers a file
    ...    was actually written with (recorded in .crank.manifest), not what its
    ...    template says *now*.
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File
    ...    ${dir}/template.tera
    ...    ${HASH_HEADER}\n# notes [\ndefault\n# ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.txt    \# notes [\nEDITED VIA HASH\n# ]\n
    # The header goes, and the template switches back to the built-in markers.
    Create File    ${dir}/template.tera    // notes [\nnew default\n// ]\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.txt
    Should Be Equal    ${output}    // notes [\nEDITED VIA HASH\n// ]\n
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest}    version: 1\ntaskGroups:\n${SPACE}${SPACE}group:\n${SPACE}${SPACE}- output.txt\n

A Project Without Template Headers Has A Manifest Of Bare Paths
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}\n// notes [\ndefault\n// ]\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    0
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Be Equal    ${manifest}    version: 1\ntaskGroups:\n${SPACE}${SPACE}group:\n${SPACE}${SPACE}- output.txt\n

A Start Marker Without A Name Fails That Task And Writes Nothing For It
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {# crank: {pocketMarkers: [{start: '# [', end: '# ]'}]} #}\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Invalid crank header in ./template.tera
    Should Contain    ${result.stderr}    must contain `{name}` exactly once
    File Should Not Exist    ${dir}/output.txt

An Unknown Header Setting Fails That Task
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {# crank: {pockets: []} #}\n
    Write Crank File
    ...    ${dir}
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    unknown field `pockets`
    File Should Not Exist    ${dir}/output.txt

Pocket Markers In Crank Apply To Every Template Without Its Own
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }} = a // b[0]\n# notes [\ndefault\n# ]\n
    Write Crank File
    ...    ${dir}
    ...    pocketMarkers: [{start: '# {name} [', end: '# ]'}]\ntaskGroups: [{name: group, object: object.yaml,
    ...    tasks: [{template: template.tera, output: output.py}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.py    widget = a // b[0]\n# notes [\nEDITED\n# ]\n
    Create File    ${dir}/object.yaml    name: revised
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.py
    Should Be Equal    ${output}    revised = a // b[0]\n# notes [\nEDITED\n# ]\n
    ${manifest}=    Get File    ${dir}/.crank.manifest
    Should Contain    ${manifest}    - start: '# {name} ['

A Template Headers Markers Win Over The Ones In Crank
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File
    ...    ${dir}/template.tera
    ...    {#- crank: {pocketMarkers: [{start: '-- {name} [', end: '-- ]'}]} -#}\n-- notes [\ndefault\n-- ]\n
    Write Crank File
    ...    ${dir}
    ...    pocketMarkers: [{start: '# {name} [', end: '# ]'}]\ntaskGroups: [{name: group, object: object.yaml,
    ...    tasks: [{template: template.tera, output: output.sql}]}]
    ${first}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${first.rc}    0
    Create File    ${dir}/output.sql    -- notes [\nEDITED\n-- ]\n
    ${second}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${second.rc}    0
    ${output}=    Get File    ${dir}/output.sql
    Should Be Equal    ${output}    -- notes [\nEDITED\n-- ]\n

Unusable Pocket Markers In Crank Fail The Whole Run Before Any Output Is Written
    ${dir}=    Create Empty Test Directory
    Create File    ${dir}/object.yaml    name: widget
    Create File    ${dir}/template.tera    {{ name }}
    Write Crank File
    ...    ${dir}
    ...    pocketMarkers: [{start: '# [', end: '# ]'}]\ntaskGroups: [{name: group, object: object.yaml,
    ...    tasks: [{template: template.tera, output: output.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to use the pocketMarkers in ./.crank
    File Should Not Exist    ${dir}/output.txt
