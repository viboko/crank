// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! crank's browser playground: runs crank over files held in memory,
//! using the same `crank::run` and `crank::output` the CLI uses over a
//! real disk — so the playground shows exactly what the CLI would print
//! and generate for the same files. Also home to the playground's
//! examples (see [`example`]).
//!
//! The page calls the three `#[wasm_bindgen]` functions, [`run_json`],
//! [`pocket_markers_json`] and [`examples_json`], exchanging plain JSON.

pub mod example;

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path};

use crank::manifest::ManifestEntry;
use crank::output::Stream;
use crank::pocket::MarkerPattern;
use crank::run::{MANIFEST_FILE_NAME, MemoryWorkspace, Report, Workspace};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

/// One file or directory in the tree crank runs over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// `/`-joined path relative to the tree's root.
    pub path: String,
    /// Whether this is an (empty) directory rather than a file. crank
    /// never reads a directory; the page keeps them so a folder can exist
    /// before anything's in it.
    #[serde(default)]
    pub directory: bool,
    /// A file's contents.
    #[serde(default)]
    pub contents: String,
}

impl TreeEntry {
    /// A file at `path` holding `contents`.
    #[must_use]
    pub fn file(path: &str, contents: &str) -> Self {
        Self {
            path: path.to_string(),
            directory: false,
            contents: contents.to_string(),
        }
    }
}

/// A file crank wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Written {
    /// `/`-joined path relative to the tree's root.
    pub path: String,
    /// What crank wrote there.
    pub contents: String,
}

/// One line crank prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// `stdout` or `stderr`.
    pub stream: &'static str,
    /// The line's text.
    pub text: String,
}

/// What crank makes of a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// Everything crank prints, stdout and stderr interleaved in order.
    pub lines: Vec<Line>,
    /// Whether crank would exit successfully.
    pub passed: bool,
    /// Every file crank wrote, sorted by path. crank writes as it goes,
    /// so a failed run can still have written some. A file that already
    /// held what crank generates isn't written, so isn't here.
    pub written: Vec<Written>,
    /// Every file crank generated, sorted by path, whether or not it
    /// wrote it this time: after a successful run, every output and
    /// `.crank.manifest`; after a failed one, only what it wrote, since
    /// that's all a failed run reports.
    pub generated: Vec<String>,
    /// Every stale output crank deleted, sorted by path.
    pub removed: Vec<String>,
}

impl Outcome {
    /// Everything crank prints, one line per line.
    #[must_use]
    pub fn output(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A [`MemoryWorkspace`] that remembers which files were written and
/// deleted, so a run's changes can be told apart from the files it only
/// read.
struct Recorder {
    /// The files themselves.
    workspace: MemoryWorkspace,
    /// The last thing that happened to each path crank touched, by its
    /// [`slash_path`]: `true` for a write, `false` for a delete.
    touched: BTreeMap<String, bool>,
}

impl Workspace for Recorder {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.workspace.read(path)
    }

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.workspace.create_dir_all(path)
    }

    fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        self.workspace.write(path, contents)?;
        self.touched.insert(slash_path(path), true);
        Ok(())
    }

    fn remove(&mut self, path: &Path) -> io::Result<()> {
        self.workspace.remove(path)?;
        self.touched.insert(slash_path(path), false);
        Ok(())
    }
    fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
        self.workspace.same_file(a, b)
    }
}

/// `path` as the page writes paths: `/`-joined, without `.` components.
fn slash_path(path: &Path) -> String {
    path.components()
        .filter(|component| *component != Component::CurDir)
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Runs crank over `tree`, as `crank` would in the tree's root — so, like
/// the CLI run with no arguments, it names files as `./path`. The page
/// applies what it wrote and removed to its tree, just as crank changes a
/// real directory.
///
/// # Panics
/// Never: a file crank wrote is always there to read back.
#[must_use]
pub fn run(tree: &[TreeEntry]) -> Outcome {
    let mut workspace = MemoryWorkspace::new();
    for entry in tree.iter().filter(|entry| !entry.directory) {
        workspace.insert(&entry.path, entry.contents.as_str());
    }
    let mut recorder = Recorder {
        workspace,
        touched: BTreeMap::new(),
    };
    let report = crank::run::run(&mut recorder, Path::new("."), false);

    let mut generated: Vec<String> = match &report {
        Report::Generated { outputs, .. } => outputs
            .iter()
            .map(|(path, _)| slash_path(path))
            .chain(
                recorder
                    .workspace
                    .read(Path::new(MANIFEST_FILE_NAME))
                    .is_ok()
                    .then(|| MANIFEST_FILE_NAME.to_string()),
            )
            .collect(),
        _ => Vec::new(),
    };
    let mut written = Vec::new();
    let mut removed = Vec::new();
    for (path, was_written) in recorder.touched {
        if was_written {
            let contents = recorder
                .workspace
                .read(Path::new(&path))
                .expect("a file crank just wrote is still there");
            generated.push(path.clone());
            written.push(Written { path, contents });
        } else {
            removed.push(path);
        }
    }

    generated.sort();
    generated.dedup();

    let lines = crank::output::report_lines(&report)
        .into_iter()
        .map(|line| Line {
            stream: match line.stream {
                Stream::Stdout => "stdout",
                Stream::Stderr => "stderr",
            },
            text: line.text,
        })
        .collect();

    Outcome {
        lines,
        passed: report.success(),
        written,
        generated,
        removed,
    }
}

/// [`run`] for the page: takes the tree as a JSON array of [`TreeEntry`]
/// objects, and returns the [`Outcome`] as JSON — or `{"error": "..."}`
/// if the tree isn't valid JSON.
///
/// # Panics
/// Never: plain structs and strings always serialize.
#[wasm_bindgen(js_name = run)]
#[must_use]
pub fn run_json(tree: &str) -> String {
    match serde_json::from_str::<Vec<TreeEntry>>(tree) {
        Ok(tree) => serde_json::to_string(&run(&tree)),
        Err(error) => serde_json::to_string(&serde_json::json!({ "error": error.to_string() })),
    }
    .expect("plain structs and strings always serialize")
}

/// The `.crank.manifest` entry `manifest` (the tree's, if it has one)
/// records for `path`, if any.
fn manifest_entry(path: &str, manifest: Option<&str>) -> Option<ManifestEntry> {
    crank::manifest::parse(manifest?)
        .ok()?
        .into_values()
        .flatten()
        .find(|entry| slash_path(entry.path()) == path)
}

/// The pocket markers the file at `path` (holding `contents`) uses, as
/// far as the page can tell, for highlighting them. A template's are the
/// ones its crank header lists, else the ones `crank` (the tree's
/// `.crank`, if it has one) lists. Any other file's are the ones
/// `manifest` (the tree's `.crank.manifest`, if it has one) records for
/// it — or, if it records nothing about it, the ones `crank` lists. Each
/// falls back to crank's built-in markers — also when a file can't be
/// read, since this only steers highlighting and the run itself reports
/// those problems.
#[must_use]
pub fn pocket_markers(
    path: &str,
    contents: &str,
    manifest: Option<&str>,
    crank: Option<&str>,
) -> Vec<MarkerPattern> {
    let project = || {
        crank
            .and_then(|crank| crank::crank_file::parse(crank).ok())
            .and_then(|crank_file| crank_file.pocket_markers)
    };
    let listed = if Path::new(path).extension().is_some_and(|ext| ext == "tera") {
        crank::header::parse(contents)
            .ok()
            .and_then(|header| header.pocket_markers)
            .or_else(project)
    } else {
        match manifest_entry(path, manifest) {
            Some(ManifestEntry::WithPocketMarkers { pocket_markers, .. }) => Some(pocket_markers),
            Some(ManifestEntry::Plain(_)) => None,
            None => project(),
        }
    };
    listed.unwrap_or_else(crank::pocket::default_patterns)
}

/// [`pocket_markers`] for the page, as a JSON array of `{start, end}`
/// objects.
///
/// # Panics
/// Never: plain strings always serialize.
#[wasm_bindgen(js_name = pocketMarkers)]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "wasm-bindgen hands over an optional string from JavaScript owned"
)]
pub fn pocket_markers_json(
    path: &str,
    contents: &str,
    manifest: Option<String>,
    crank: Option<String>,
) -> String {
    serde_json::to_string(&pocket_markers(
        path,
        contents,
        manifest.as_deref(),
        crank.as_deref(),
    ))
    .expect("plain strings always serialize")
}

/// Every example (see [`example`]), as a JSON array of
/// [`example::Example`] objects, in order.
///
/// # Panics
/// If an example doesn't parse — which the tests rule out before it can
/// ship.
#[wasm_bindgen(js_name = examples)]
#[must_use]
pub fn examples_json() -> String {
    let examples: Vec<example::Example> = EXAMPLE_SOURCES
        .iter()
        .map(|(id, source)| example::parse(id, source).expect("examples are tested to parse"))
        .collect();
    serde_json::to_string(&examples).expect("plain structs and strings always serialize")
}

/// Every example in `examples/`, as `(id, Markdown source)`, sorted by
/// id — embedded at build time by `build.rs`.
pub const EXAMPLE_SOURCES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/examples.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    const CRANK: &str = "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: out.txt\n";

    /// The paths in `written`.
    fn paths(written: &[Written]) -> Vec<&str> {
        written.iter().map(|file| file.path.as_str()).collect()
    }

    #[test]
    fn a_run_reports_its_output_and_the_files_it_wrote() {
        let outcome = run(&[
            TreeEntry::file(".crank", CRANK),
            TreeEntry::file("object.yaml", "name: widget\n"),
            TreeEntry::file("t.tera", "Hello {{ name }}\n"),
        ]);
        assert_eq!(outcome.output(), "Generated 1 file\n  - out.txt (created)");
        assert!(outcome.passed);
        assert_eq!(
            outcome.written,
            vec![
                Written {
                    path: ".crank.manifest".to_string(),
                    contents: "version: 1\ntaskGroups:\n  widget:\n  - out.txt\n".to_string(),
                },
                Written {
                    path: "out.txt".to_string(),
                    contents: "Hello widget\n".to_string(),
                },
            ]
        );
        assert_eq!(outcome.removed, Vec::<String>::new());
    }

    #[test]
    fn a_stale_output_is_reported_removed() {
        let outcome = run(&[
            TreeEntry::file(".crank", CRANK),
            TreeEntry::file(
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - old.txt\n  - out.txt\n",
            ),
            TreeEntry::file("object.yaml", "name: widget\n"),
            TreeEntry::file("t.tera", "Hello\n"),
            TreeEntry::file("out.txt", "Hello\n"),
            TreeEntry::file("old.txt", "old\n"),
        ]);
        // `out.txt` already holds what's generated, so it isn't rewritten.
        assert_eq!(paths(&outcome.written), vec![".crank.manifest"]);
        assert_eq!(outcome.removed, vec!["old.txt"]);
    }

    #[test]
    fn a_file_crank_left_alone_is_still_generated() {
        let outcome = run(&[
            TreeEntry::file(".crank", CRANK),
            TreeEntry::file(
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - out.txt\n",
            ),
            TreeEntry::file("object.yaml", "name: widget\n"),
            TreeEntry::file("t.tera", "Hello\n"),
            TreeEntry::file("out.txt", "Hello\n"),
        ]);
        assert_eq!(paths(&outcome.written), Vec::<&str>::new());
        assert_eq!(outcome.generated, vec![".crank.manifest", "out.txt"]);
    }

    #[test]
    fn a_failed_run_counts_only_what_it_wrote_as_generated() {
        let outcome = run(&[
            TreeEntry::file(
                ".crank",
                "taskGroups:\n  - name: a\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: a.txt\n      - template: missing.tera\n        output: b.txt\n",
            ),
            TreeEntry::file("object.yaml", "name: widget\n"),
            TreeEntry::file("t.tera", "{{ name }}\n"),
        ]);
        assert!(!outcome.passed);
        assert_eq!(outcome.generated, vec!["a.txt"]);
    }

    #[test]
    fn a_failed_run_still_reports_what_it_wrote() {
        let outcome = run(&[
            TreeEntry::file(
                ".crank",
                "taskGroups:\n  - name: a\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: a.txt\n      - template: missing.tera\n        output: b.txt\n",
            ),
            TreeEntry::file("object.yaml", "name: widget\n"),
            TreeEntry::file("t.tera", "{{ name }}\n"),
        ]);
        assert!(!outcome.passed);
        assert_eq!(outcome.lines[0].stream, "stderr");
        assert_eq!(paths(&outcome.written), vec!["a.txt"]);
    }

    #[test]
    fn every_example_prints_and_generates_exactly_what_it_documents() {
        assert_ne!(EXAMPLE_SOURCES.len(), 0, "no examples were embedded");
        for (id, source) in EXAMPLE_SOURCES {
            let example = example::parse(id, source).unwrap_or_else(|err| panic!("{id}: {err}"));
            let outcome = run(&example.tree);
            let actual = outcome.output();
            assert!(
                actual == example.output,
                "{id}.md's `## Output` doesn't match what crank prints for it:\n\
                 --- documented ---\n{}\n--- actual ---\n{actual}\n---",
                example.output
            );
            // Every file crank created or changed, `.crank.manifest` aside.
            let before = |path: &str| {
                example
                    .tree
                    .iter()
                    .find(|entry| entry.path == path)
                    .map(|entry| entry.contents.as_str())
            };
            let actual: Vec<(String, String)> = outcome
                .written
                .iter()
                .filter(|file| {
                    file.path != ".crank.manifest"
                        && before(&file.path) != Some(file.contents.as_str())
                })
                .map(|file| (file.path.clone(), file.contents.clone()))
                .collect();
            let mut documented = example.generated.clone();
            documented.sort();
            assert!(
                actual == documented,
                "{id}.md's `## Generated` sections don't match what crank writes for it:\n\
                 --- documented ---\n{documented:#?}\n--- actual ---\n{actual:#?}\n---",
            );
        }
    }

    #[test]
    fn run_json_takes_and_returns_json() {
        let outcome: serde_json::Value = serde_json::from_str(&run_json(
            r#"[{"path": "src", "directory": true},
                {"path": ".crank", "contents": "taskGroups: []\n"}]"#,
        ))
        .unwrap();
        assert_eq!(outcome["passed"], true);
        assert_eq!(outcome["written"][0]["path"], ".crank.manifest");
        assert_eq!(outcome["removed"], serde_json::json!([]));
        assert_eq!(
            outcome["lines"][0],
            serde_json::json!({
                "stream": "stdout",
                "text": "No files to generate",
            })
        );
    }

    #[test]
    fn a_templates_pocket_markers_come_from_its_header() {
        let template = "{# crank: {pocketMarkers: [{start: '# {name} [', end: '# ]'}]} #}\n";
        assert_eq!(
            pocket_markers("t.tera", template, None, None),
            vec![MarkerPattern::new("# {name} [", "# ]")]
        );
        assert_eq!(
            pocket_markers("t.tera", "no header\n", None, None),
            crank::pocket::default_patterns()
        );
        assert_eq!(
            pocket_markers("t.tera", "{# crank: [broken #}", None, None),
            crank::pocket::default_patterns()
        );
    }

    #[test]
    fn another_files_pocket_markers_come_from_the_manifest() {
        let manifest = "version: 1\ntaskGroups:\n  g:\n  - path: ./a.py\n    pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n  - b.rs\n";
        assert_eq!(
            pocket_markers("a.py", "", Some(manifest), None),
            vec![MarkerPattern::new("# {name} [", "# ]")]
        );
        assert_eq!(
            pocket_markers("b.rs", "", Some(manifest), None),
            crank::pocket::default_patterns()
        );
        assert_eq!(
            pocket_markers("a.py", "", Some("not: [valid"), None),
            crank::pocket::default_patterns()
        );
        assert_eq!(
            pocket_markers("a.py", "", None, None),
            crank::pocket::default_patterns()
        );
    }

    #[test]
    fn the_crank_files_markers_apply_where_nothing_closer_lists_any() {
        let crank = "taskGroups: []\npocketMarkers: [{start: '# {name} [', end: '# ]'}]\n";
        let hash = vec![MarkerPattern::new("# {name} [", "# ]")];
        let manifest = "version: 1\ntaskGroups:\n  g:\n  - plain.py\n";
        // A template without a header, and a file the manifest doesn't list.
        assert_eq!(pocket_markers("t.tera", "x\n", None, Some(crank)), hash);
        assert_eq!(
            pocket_markers("new.py", "", Some(manifest), Some(crank)),
            hash
        );
        // A header wins over `.crank`...
        let header = "{# crank: {pocketMarkers: []} #}";
        assert_eq!(pocket_markers("t.tera", header, None, Some(crank)), vec![]);
        // ...and a file the manifest says used the built-in markers did.
        assert_eq!(
            pocket_markers("plain.py", "", Some(manifest), Some(crank)),
            crank::pocket::default_patterns()
        );
    }

    #[test]
    fn pocket_markers_json_is_a_list_of_start_end_pairs() {
        let markers: serde_json::Value =
            serde_json::from_str(&pocket_markers_json("a.txt", "", None, None)).unwrap();
        assert_eq!(
            markers[0],
            serde_json::json!({"start": "// {name} [", "end": "// ]"})
        );
    }

    #[test]
    fn run_json_reports_bad_input_as_an_error() {
        let not_json: serde_json::Value = serde_json::from_str(&run_json("nope")).unwrap();
        assert!(not_json["error"].as_str().unwrap().starts_with("expected"));
    }

    #[test]
    fn examples_json_lists_every_example_in_order() {
        let examples: serde_json::Value = serde_json::from_str(&examples_json()).unwrap();
        let ids: Vec<&str> = examples
            .as_array()
            .unwrap()
            .iter()
            .map(|example| example["id"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = EXAMPLE_SOURCES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, expected);
    }
}
