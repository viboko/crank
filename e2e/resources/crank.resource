*** Comments ***
# SPDX-FileCopyrightText: The crank contributors
# SPDX-License-Identifier: MIT OR Apache-2.0


*** Settings ***
Documentation    Shared keywords for driving the compiled crank binary as a black box.

Library          OperatingSystem
Library          Process
Library          String


*** Variables ***
${CRANK_BINARY}    %{CRANK_BINARY}
# Every directory the current test created, for its teardown to remove.
@{TEST_DIRS}       @{EMPTY}


*** Keywords ***
Create Empty Test Directory
    [Documentation]    Creates a fresh, empty temp directory and returns its path.
    ...    Cleanup Test Directories removes it again in the test's teardown.
    ${suffix}=    Generate Random String    8    [LETTERS][NUMBERS]
    ${dir}=    Join Path    ${TEMPDIR}    crank-e2e-${suffix}
    Create Directory    ${dir}
    # Test scope is the point: it's how the teardown finds this test's
    # directories, and it resets to empty for the next test.
    VAR    @{TEST_DIRS}=    @{TEST_DIRS}    ${dir}    scope=TEST    # robocop: off=VAR06
    RETURN    ${dir}

Cleanup Test Directories
    [Documentation]    Test Teardown: removes every directory the test created with
    ...    Create Empty Test Directory.
    FOR    ${dir}    IN    @{TEST_DIRS}
        Remove Directory    ${dir}    recursive=True
    END

Write Crank File
    [Documentation]    Writes the .crank file inside ${dir}: each of ${parts},
    ...    joined by single spaces, so a long one-line file can be split
    ...    across several lines of a test at any space.
    [Arguments]    ${dir}    @{parts}
    ${content}=    Catenate    @{parts}
    Create File    ${dir}/.crank    ${content}

Run Crank In Directory
    [Documentation]    Runs the crank binary with ${dir} as its working directory.
    [Arguments]    ${dir}
    ${result}=    Run Process    ${CRANK_BINARY}    cwd=${dir}
    RETURN    ${result}

Run Crank Against Path
    [Documentation]    Runs the crank binary from ${TEMPDIR}, passing ${path} as its
    ...    PATH argument, so the run happens against ${path} without it being the cwd.
    [Arguments]    ${path}
    ${result}=    Run Process    ${CRANK_BINARY}    ${path}    cwd=${TEMPDIR}
    RETURN    ${result}

Run Crank Check In Directory
    [Documentation]    Runs the crank binary with --check and ${dir} as its working
    ...    directory.
    [Arguments]    ${dir}
    ${result}=    Run Process    ${CRANK_BINARY}    --check    cwd=${dir}
    RETURN    ${result}
