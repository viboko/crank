// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzz target: runs crank over an arbitrary in-memory project — the same
//! `crank::run::run` the CLI uses on a real disk, then renders its output
//! — so parsing the `.crank` file, data files, selectors, templates,
//! filter scripts and pockets, writing outputs and reconciling the
//! manifest are all exercised together.
//!
//! An input is the `.crank` file's contents, then any number of further
//! files, each starting with a line `--- <path>` and holding everything up
//! to the next such line. crank runs twice, so the second run reads back
//! the first one's outputs and manifest (pockets, stale files), and then
//! once more with `--check`. See docs/fuzzing.md.

#![no_main]
// `fuzz_target!` expands to code that can write its input to a file
// (`RUST_LIBFUZZER_DEBUG_PATH`), which `clippy.toml` bans for the library,
// not for the fuzz harness.
#![allow(clippy::disallowed_types)]

use std::path::Path;

use crank::output::report_lines;
use crank::run::{MemoryWorkspace, run};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // `crank` reads every file with `fs::read_to_string`, so it never sees
    // invalid UTF-8.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let mut workspace = MemoryWorkspace::new();
    for (path, contents) in split_input(text) {
        workspace.insert(path, contents);
    }

    let root = Path::new(".");
    for check in [false, false, true] {
        let report = run(&mut workspace, root, check);
        let _ = report_lines(&report);
    }
});

/// Splits an input into `(path, contents)` pairs: `.crank` first, with
/// everything before the first `--- <path>` line, then one per such line.
fn split_input(text: &str) -> Vec<(&str, &str)> {
    let mut files = Vec::new();
    let mut path = ".crank";
    let mut start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(next) = line.trim_end_matches('\n').strip_prefix("--- ") {
            files.push((path, text.get(start..offset).unwrap_or_default()));
            path = next;
            start = offset + line.len();
        }
        offset += line.len();
    }
    files.push((path, text.get(start..).unwrap_or_default()));
    files
}
