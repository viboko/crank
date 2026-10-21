*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Black-box end-to-end tests that crank never reads, writes or deletes
...              through a path leading outside the directory holding .crank.

Resource         ../resources/crank.resource

Test Teardown    Cleanup Test Directories


*** Test Cases ***
An Output Leading Outside The Directory Is Refused While Other Tasks Still Run
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: ../escaped.txt},
    ...    {template: template.tera, output: inside.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Output path "../escaped.txt" leads outside the directory holding .crank
    File Should Not Exist    ${dir}/escaped.txt
    ${inside}=    Get File    ${dir}/project/inside.txt
    Should Be Equal    ${inside}    Hello widget

A Template Leading Outside The Directory Is Refused Before Anything Runs
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: ../template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    `template` path "../template.tera" leads outside the directory holding .crank
    File Should Not Exist    ${dir}/project/out.txt

A Manifest Entry Leading Outside The Directory Is An Error And Never Deleted
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create File    ${dir}/important    keep me
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Create File    ${dir}/project/.crank.manifest    version: 1\ntaskGroups: {group: [../important]}\n
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    .crank.manifest records "../important", which leads outside
    ${important}=    Get File    ${dir}/important
    Should Be Equal    ${important}    keep me
    ${check}=    Run Crank Check In Directory    ${dir}/project
    Should Be Equal As Integers    ${check.rc}    1
    File Should Exist    ${dir}/important

An Output Through A Symbolic Link Leading Outside Is Refused
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create Directory    ${dir}/elsewhere
    Evaluate    os.symlink($dir + '/elsewhere', $dir + '/project/out')    modules=os
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out/escaped.txt},
    ...    {template: template.tera, output: out/sub/escaped.txt}, {template: template.tera, output: inside.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    Failed to create directory for out/escaped.txt
    Should Contain    ${result.stderr}    Failed to create directory for out/sub/escaped.txt
    Should Contain    ${result.stderr}    out leads outside the directory crank runs in through a symbolic link
    File Should Not Exist    ${dir}/elsewhere/escaped.txt
    Directory Should Not Exist    ${dir}/elsewhere/sub
    File Should Exist    ${dir}/project/inside.txt

An Output That Is A Dangling Symbolic Link Is Refused
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Evaluate    os.symlink($dir + '/created.txt', $dir + '/project/out.txt')    modules=os
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    leads outside the directory crank runs in through a symbolic link
    File Should Not Exist    ${dir}/created.txt

A Template Through A Symbolic Link Leading Outside Is Not Read
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create File    ${dir}/secret.txt    top secret
    Evaluate    os.symlink($dir + '/secret.txt', $dir + '/project/template.tera')    modules=os
    Create File    ${dir}/project/object.yaml    name: widget
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain
    ...    ${result.stderr}
    ...    template.tera leads outside the directory crank runs in through a symbolic link
    File Should Not Exist    ${dir}/project/out.txt

A Symbolic Link Pointing Inside The Directory Still Works
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project/real
    Evaluate    os.symlink($dir + '/project/real', $dir + '/project/out')    modules=os
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: out/x.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    0
    ${output}=    Get File    ${dir}/project/real/x.txt
    Should Be Equal    ${output}    Hello widget

A Stale Output Through A Symbolic Link Leading Outside Is Not Deleted
    ${dir}=    Create Empty Test Directory
    Create Directory    ${dir}/project
    Create Directory    ${dir}/elsewhere
    Create File    ${dir}/elsewhere/important    keep me
    Evaluate    os.symlink($dir + '/elsewhere', $dir + '/project/out')    modules=os
    Create File    ${dir}/project/object.yaml    name: widget
    Create File    ${dir}/project/template.tera    Hello {{ name }}
    Create File    ${dir}/project/.crank.manifest    version: 1\ntaskGroups: {group: [out/important]}\n
    Write Crank File
    ...    ${dir}/project
    ...    taskGroups: [{name: group, object: object.yaml, tasks: [{template: template.tera, output: new.txt}]}]
    ${result}=    Run Crank In Directory    ${dir}/project
    Should Be Equal As Integers    ${result.rc}    1
    Should Contain    ${result.stderr}    leads outside the directory crank runs in through a symbolic link
    ${important}=    Get File    ${dir}/elsewhere/important
    Should Be Equal    ${important}    keep me
