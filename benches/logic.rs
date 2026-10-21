// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tier 1: pure in-memory benchmarks for crank's parsing, rendering and
//! pocket logic, and a whole run over a `MemoryWorkspace`. No filesystem
//! access — see `benches/e2e.rs` for the tier that measures real disk
//! I/O.
//!
//! These count CPU instructions under Valgrind (via Gungraun) rather than
//! timing anything: each benchmark runs once, and an unchanged build
//! gives near enough the same count every run, on any machine. That's
//! what lets CI fail a PR over them — see docs/benchmarking.md. Valgrind
//! doesn't run on macOS, so there they need a Linux container.
//!
//! The scaling dimensions are the number of task groups in a `.crank`
//! file, the number of items a template loops over, the number of
//! pockets in a generated file (each at 100, 1,000 and 10,000), and the
//! number of items in an `array` task group for a whole run (10, 100 and
//! 1,000, since each one renders and writes a file).
//!
//! Only the body of a `#[library_benchmark]` function is counted. Its
//! `setup` function builds the input beforehand, uncounted, and leaks it
//! so that freeing it isn't counted either.

// Gungraun's macros expand to code that prints, calls
// `std::process::exit` and (on Linux, in `#[library_benchmark]`) opens a
// `std::fs::File` — banned by `clippy.toml` for the library, not for a
// bench harness.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]
// Gungraun's macros also expand to undocumented modules, constants and
// functions, and reject a doc comment or `#[allow]` on a benchmark
// function, so the missing-docs lints can only be lifted for the whole
// file. Everything written here by hand is still documented.
#![allow(missing_docs, clippy::missing_docs_in_private_items)]

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::{Path, PathBuf};

use crank::crank_file::{self, CrankFile};
use crank::pocket::{OldPocket, OldPockets, PocketMarkers, apply};
use crank::render::render;
use crank::rhai_filter::{CompiledFilter, built_in_filters};
use crank::run::{MemoryWorkspace, Report, run};
use gungraun::prelude::*;

/// A `.crank` file of `n` task groups, each with one task.
fn crank_file_contents(n: usize) -> &'static str {
    let mut contents = String::from("taskGroups:\n");
    for i in 0..n {
        write!(
            contents,
            "  - name: group_{i}\n    object: data_{i}.yaml\n    tasks:\n      - template: t.tera\n        output: out_{i}.txt\n"
        )
        .expect("writing to a String never fails");
    }
    contents.leak()
}

/// What [`render`] takes: a template, its context and the filters.
type RenderInput = (
    &'static str,
    &'static tera::Context,
    &'static HashMap<String, CompiledFilter>,
);

/// A template that loops over `n` items, each run through a built-in
/// filter, the context holding them, and crank's built-in filters.
fn loop_template(n: usize) -> RenderInput {
    let mut data = String::from("items:\n");
    for i in 0..n {
        writeln!(data, "  - name: Item Number {i}\n    size: {i}")
            .expect("writing to a String never fails");
    }
    let value = crank::data::parse(&data).expect("generated data always parses");
    let context = crank::data::to_context(&value).expect("generated data is a mapping");
    let template = "{% for item in items %}{{ item.name | snake_case }}: {{ item.size }}{% if not loop.last %},{% endif %}\n{% endfor %}";
    (
        template,
        Box::leak(Box::new(context)),
        Box::leak(Box::new(built_in_filters())),
    )
}

/// What [`apply`] takes: freshly rendered text, the old pockets and the
/// markers.
type PocketInput = (&'static str, &'static OldPockets, &'static PocketMarkers);

/// A generated file of `n` line-form pockets, and old content for each.
fn pockets(n: usize) -> PocketInput {
    let mut fresh = String::new();
    let mut old = OldPockets::new();
    for i in 0..n {
        writeln!(fresh, "line {i}\n// pocket_{i} [\ndefault\n// ]")
            .expect("writing to a String never fails");
        old.insert(
            format!("pocket_{i}"),
            vec![OldPocket {
                file: PathBuf::from("old.txt"),
                content: format!("hand-written {i}\n"),
            }],
        );
    }
    (
        fresh.leak(),
        Box::leak(Box::new(old)),
        Box::leak(Box::new(PocketMarkers::default_markers())),
    )
}

/// A project with one `array` task group of `n` items, already run once,
/// so the measured run also reads back its outputs and manifest.
fn array_project(n: usize) -> MemoryWorkspace {
    let mut workspace = MemoryWorkspace::new();
    let mut items = String::new();
    for i in 0..n {
        writeln!(items, "- name: item_{i}\n  size: {i}").expect("writing to a String never fails");
    }
    workspace.insert("items.yaml", items);
    workspace.insert(
        "item.txt.tera",
        "{{ name | upper }} is {{ size }} units\n// notes_{{ name }} [\n// ]\n",
    );
    workspace.insert(
        ".crank",
        "taskGroups:\n  - name: items\n    array: items.yaml\n    tasks:\n      - template: item.txt.tera\n        output: \"out/{{ name }}.txt\"\n",
    );
    let report = run(&mut workspace, Path::new("."), false);
    assert!(report.success(), "the generated project always runs");
    workspace
}

#[library_benchmark]
#[bench::n_100(args = (100), setup = crank_file_contents)]
#[bench::n_1000(args = (1_000), setup = crank_file_contents)]
#[bench::n_10000(args = (10_000), setup = crank_file_contents)]
fn parse_crank_file(contents: &str) -> CrankFile {
    crank_file::parse(black_box(contents)).expect("generated .crank contents always parse")
}

#[library_benchmark]
#[bench::n_100(args = (100), setup = loop_template)]
#[bench::n_1000(args = (1_000), setup = loop_template)]
#[bench::n_10000(args = (10_000), setup = loop_template)]
fn render_loop((template, context, filters): RenderInput) -> String {
    render(black_box(template), black_box(context), black_box(filters))
        .expect("the generated template always renders")
}

#[library_benchmark]
#[bench::n_100(args = (100), setup = pockets)]
#[bench::n_1000(args = (1_000), setup = pockets)]
#[bench::n_10000(args = (10_000), setup = pockets)]
fn apply_pockets((fresh, old, markers): PocketInput) -> String {
    let mut seen = HashSet::new();
    apply(
        black_box(fresh),
        black_box(old),
        &mut seen,
        black_box(markers),
    )
    .expect("the generated pockets are always well-formed")
}

// A whole run: parse, load, render every item, carry its pocket over,
// write it and reconcile the manifest. Returns the workspace too, so
// freeing it isn't counted.
#[library_benchmark]
#[bench::n_10(args = (10), setup = array_project)]
#[bench::n_100(args = (100), setup = array_project)]
#[bench::n_1000(args = (1_000), setup = array_project)]
fn run_array(mut workspace: MemoryWorkspace) -> (Report, MemoryWorkspace) {
    let report = run(black_box(&mut workspace), Path::new("."), false);
    (report, workspace)
}

library_benchmark_group!(
    name = logic,
    benchmarks = [parse_crank_file, render_loop, apply_pockets, run_array]
);

main!(library_benchmark_groups = logic);
