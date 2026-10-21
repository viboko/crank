// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tier 2: real filesystem, real binary. Complements `benches/logic.rs`'s
//! in-memory benchmarks by measuring the actual `crank` binary against a
//! real on-disk project, so a regression in the I/O path (e.g. an
//! accidental re-read of every file, or non-linear scaling in how outputs
//! are written) shows up even though the library never touches the disk
//! itself.
//!
//! Each project has one `array` task group of `n` items, so a run reads
//! one data file and one template and writes `n` files. Real file writes
//! and process spawns are inherently slower per sample than Tier 1's
//! in-memory calls, so expect noisier numbers here (see
//! docs/benchmarking.md). Each size's project is created, and run once,
//! outside the timed portion — so every timed run finds the previous
//! run's outputs and `.crank.manifest` in place, as a re-run in a real
//! project would.

// Measuring real disk I/O and real process spawns is this bench's job —
// `clippy.toml`'s bans are for the library.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// The numbers of generated files each run is timed with.
const SIZES: [usize; 3] = [10, 100, 1_000];

/// A throwaway project generating `n` files, cleaned up on drop.
struct TempProject {
    /// Where it is.
    dir: PathBuf,
}

impl TempProject {
    /// Creates a fresh one, generating `n` files.
    fn with_item_count(n: usize) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("crank-bench-{}-{unique}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp project dir");

        let mut items = String::new();
        for i in 0..n {
            writeln!(items, "- name: item_{i}\n  size: {i}")
                .expect("writing to a String never fails");
        }
        fs::write(dir.join("items.yaml"), items).expect("write data file");
        fs::write(
            dir.join("item.txt.tera"),
            "{{ name | upper }} is {{ size }} units\n// notes_{{ name }} [\n// ]\n",
        )
        .expect("write template");
        fs::write(
            dir.join(".crank"),
            "taskGroups:\n  - name: items\n    array: items.yaml\n    tasks:\n      - template: item.txt.tera\n        output: \"out/{{ name }}.txt\"\n",
        )
        .expect("write .crank");

        TempProject { dir }
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Runs `crank` in `dir`, failing the benchmark if it fails.
fn run_crank(dir: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_crank"))
        .current_dir(dir)
        .output()
        .expect("run crank");
    assert!(
        output.status.success(),
        "crank failed on its own generated project: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Times `crank` generating each of [`SIZES`]' file counts.
fn bench_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("crank binary (real directory)");
    group.sample_size(20);
    for &n in &SIZES {
        let project = TempProject::with_item_count(n);
        run_crank(&project.dir);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| run_crank(&project.dir));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_e2e);
criterion_main!(benches);
