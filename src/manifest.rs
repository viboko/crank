// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing, serializing, and diffing of `.crank.manifest`: the record of
//! every output path crank wrote on its last fully successful run, grouped
//! by the task group that produced it, used to delete outputs that a
//! `.crank` file no longer produces (e.g. after a rename).
//!
//! The file is a mapping with a format `version` (see [`VERSION`]) and a
//! `taskGroups` mapping of task group name to that group's entries, so a
//! later crank can change the format and still recognize this one.
//!
//! Each entry is usually just a path, but can instead be a `{path,
//! pocketMarkers}` mapping when the template that wrote that file listed
//! its own `pocketMarkers` in its header (see `header.rs`) — see
//! [`ManifestEntry`]. The key matches the header's own `pocketMarkers:`,
//! since it's the same list — this file just records which markers a file
//! was actually written with. This lets old-file pocket scanning
//! (`run.rs::load_old_pockets`) always use exactly the markers a file was
//! generated with, independent of what its template says *now*, without a
//! project that only uses the built-in markers seeing any change to its
//! manifest at all: an entry with nothing to record stays a bare path
//! string.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crank_file::normalize;
use crate::pocket::MarkerPattern;

/// The `.crank.manifest` format version this crank reads and writes. A
/// manifest recording any other version is a [`ParseError`].
pub const VERSION: u32 = 1;

/// The whole of a `.crank.manifest` file, as it's laid out on disk.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    /// The format version the file was written in.
    version: u32,
    /// Each task group's entries, by task group name.
    #[serde(rename = "taskGroups")]
    task_groups: BTreeMap<String, Vec<ManifestEntry>>,
}

/// One `.crank.manifest` entry: either just a path (the common case), or
/// a path plus the pocket markers it was last written with — only present
/// when its template listed its own `pocketMarkers`, replacing crank's
/// built-in ones. `#[serde(untagged)]` is what makes a bare YAML scalar
/// and a `{path, pocketMarkers}` mapping both valid list entries in the
/// same manifest, so a project that only uses the built-in markers gets a
/// manifest of bare paths.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ManifestEntry {
    /// A file written with crank's built-in pocket markers.
    Plain(PathBuf),
    /// A file written with its template's own pocket markers.
    WithPocketMarkers {
        /// The file's path, relative to the `.crank` file's directory.
        path: PathBuf,
        /// The markers the file was written with, so its pockets are
        /// still found on the next run even if its template's change.
        #[serde(rename = "pocketMarkers")]
        pocket_markers: Vec<MarkerPattern>,
    },
}

impl ManifestEntry {
    /// The generated file's path, relative to the `.crank` file's
    /// directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            ManifestEntry::Plain(path) | ManifestEntry::WithPocketMarkers { path, .. } => path,
        }
    }
}

/// A `.crank.manifest` that failed to parse as the expected YAML shape (a
/// `version` and a `taskGroups` mapping of task group name to a list of
/// paths), or that records a format version this crank doesn't read.
#[derive(Debug)]
pub enum ParseError {
    /// The file isn't valid YAML, or isn't the expected shape.
    Yaml(serde_yaml_ng::Error),
    /// The file records a format version other than [`VERSION`].
    UnsupportedVersion(u32),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Yaml(err) => write!(f, "{err}"),
            ParseError::UnsupportedVersion(version) => write!(
                f,
                "it records version {version}, but this crank only reads version {VERSION}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses `source` (the raw contents of a `.crank.manifest` file) into the
/// entries it records, grouped by task group name. Order carries no
/// meaning — only the set of paths (regardless of group) matters to
/// [`stale`].
///
/// # Errors
/// Returns [`ParseError`] if `source` isn't valid YAML, isn't a mapping of
/// `version` and `taskGroups` (task group name to a list of entries), or
/// records a `version` other than [`VERSION`].
pub fn parse(source: &str) -> Result<BTreeMap<String, Vec<ManifestEntry>>, ParseError> {
    let file: ManifestFile = serde_yaml_ng::from_str(source).map_err(ParseError::Yaml)?;
    if file.version != VERSION {
        return Err(ParseError::UnsupportedVersion(file.version));
    }
    Ok(file.task_groups)
}

/// Renders `outputs` back into `.crank.manifest`'s format: a YAML mapping
/// of `version` ([`VERSION`]) and `taskGroups`, a mapping of task group
/// name to a list of entries. A `BTreeMap` already iterates
/// (and so serializes) in sorted-key order, and each group's entries are
/// sorted by path too, so committing the file produces a stable, low-noise
/// diff run to run regardless of task-group/task order in `.crank`. Sorted
/// by [`ManifestEntry::path`] rather than by deriving `Ord` on the whole
/// entry type, which would sort by variant (`Plain` vs `WithPocketMarkers`)
/// before path — not the order this file wants.
///
/// # Panics
/// Never, in practice — a version number and a mapping of strings to lists
/// of entries always have a YAML representation.
#[must_use]
pub fn serialize(outputs: &BTreeMap<String, Vec<ManifestEntry>>) -> String {
    let mut task_groups = outputs.clone();
    for entries in task_groups.values_mut() {
        entries.sort_by(|a, b| a.path().cmp(b.path()));
    }
    serde_yaml_ng::to_string(&ManifestFile {
        version: VERSION,
        task_groups,
    })
    .expect("a version and a mapping of group name to entries always serialize to YAML")
}

/// Returns every entry in `previous` that isn't present in `current` — the
/// outputs a prior run generated that this run didn't, and so should be
/// deleted. Paths are compared once [`normalize`]d, so `./out.txt` last
/// run and `out.txt` this run are the same output, not a stale one.
/// Sorted for deterministic, testable output.
///
/// Deliberately flat: whether a path is stale depends only on whether some
/// current task group still produces it, not on which task group produced
/// it last time — a path that moved from one group to another between runs
/// isn't stale. Callers that need to know which group a stale path used to
/// belong to (e.g. to re-file a failed deletion under the right manifest
/// key) look that up themselves from the grouped manifest.
#[must_use]
pub fn stale(previous: &[PathBuf], current: &[PathBuf]) -> Vec<PathBuf> {
    let current: HashSet<PathBuf> = current.iter().map(|path| normalize(path)).collect();
    let mut stale: Vec<PathBuf> = previous
        .iter()
        .filter(|path| !current.contains(&normalize(path)))
        .cloned()
        .collect();
    stale.sort();
    stale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_mapping_of_group_name_to_paths() {
        let source =
            "version: 1\ntaskGroups:\n  pizzas:\n    - output.py\n    - generated/other.py\n";
        let outputs = parse(source).unwrap();
        assert_eq!(
            outputs.get("pizzas"),
            Some(&vec![
                ManifestEntry::Plain(PathBuf::from("output.py")),
                ManifestEntry::Plain(PathBuf::from("generated/other.py")),
            ])
        );
    }

    #[test]
    fn parses_multiple_groups() {
        let source =
            "version: 1\ntaskGroups:\n  pizzas:\n    - pizza.txt\n  widgets:\n    - widget.py\n";
        let outputs = parse(source).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(
            outputs.get("pizzas"),
            Some(&vec![ManifestEntry::Plain(PathBuf::from("pizza.txt"))])
        );
        assert_eq!(
            outputs.get("widgets"),
            Some(&vec![ManifestEntry::Plain(PathBuf::from("widget.py"))])
        );
    }

    #[test]
    fn a_group_with_an_empty_list_is_valid() {
        let source = "version: 1\ntaskGroups:\n  pizzas: []\n";
        let outputs = parse(source).unwrap();
        assert_eq!(outputs.get("pizzas"), Some(&Vec::new()));
    }

    #[test]
    fn parses_an_empty_task_groups_mapping() {
        let outputs = parse("version: 1\ntaskGroups: {}\n").unwrap();
        assert!(outputs.is_empty());
    }

    #[test]
    fn an_unversioned_grouped_manifest_is_a_parse_error() {
        assert!(parse("pizzas:\n  - output.py\n").is_err());
    }

    #[test]
    fn a_manifest_without_task_groups_is_a_parse_error() {
        assert!(parse("version: 1\n").is_err());
    }

    #[test]
    fn an_unrecognized_top_level_key_is_a_parse_error() {
        assert!(parse("version: 1\ntaskGroups: {}\nextra: true\n").is_err());
    }

    #[test]
    fn an_unsupported_version_is_a_parse_error_naming_both_versions() {
        let err = parse("version: 2\ntaskGroups: {}\n").unwrap_err();
        assert!(matches!(err, ParseError::UnsupportedVersion(2)));
        assert_eq!(
            err.to_string(),
            "it records version 2, but this crank only reads version 1"
        );
    }

    #[test]
    fn a_non_numeric_version_is_a_parse_error() {
        assert!(parse("version: one\ntaskGroups: {}\n").is_err());
    }

    #[test]
    fn malformed_yaml_is_a_parse_error() {
        assert!(parse("[this is not: valid: yaml:").is_err());
    }

    #[test]
    fn parses_a_path_pocket_markers_mapping_entry() {
        let source = "version: 1\ntaskGroups:\n  pizzas:\n    - path: output.py\n      pocketMarkers:\n        - start: '# {name} ['\n          end: '# ]'\n";
        let outputs = parse(source).unwrap();
        let entry = &outputs.get("pizzas").unwrap()[0];
        assert_eq!(entry.path(), Path::new("output.py"));
        let ManifestEntry::WithPocketMarkers { pocket_markers, .. } = entry else {
            panic!("expected a WithPocketMarkers entry");
        };
        assert_eq!(pocket_markers, &[MarkerPattern::new("# {name} [", "# ]")]);
    }

    #[test]
    fn a_mix_of_plain_and_pocket_markers_entries_in_one_group_parses() {
        let source = "version: 1\ntaskGroups:\n  pizzas:\n    - plain.py\n    - path: hashed.py\n      pocketMarkers:\n        - start: '# {name} ['\n          end: '# ]'\n";
        let outputs = parse(source).unwrap();
        let entries = outputs.get("pizzas").unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], ManifestEntry::Plain(_)));
        assert!(matches!(
            entries[1],
            ManifestEntry::WithPocketMarkers { .. }
        ));
    }

    #[test]
    fn an_unrecognized_key_in_a_recorded_marker_is_a_parse_error() {
        let source = "version: 1\ntaskGroups:\n  pizzas:\n    - path: output.py\n      pocketMarkers:\n        - start: 'a {name}'\n          end: 'b'\n          middle: 'c'\n";
        assert!(parse(source).is_err());
    }

    #[test]
    fn a_mapping_entry_without_pocket_markers_is_a_parse_error() {
        assert!(parse("version: 1\ntaskGroups:\n  pizzas:\n    - path: output.py\n").is_err());
    }

    #[test]
    fn serialize_sorts_its_output() {
        let outputs = BTreeMap::from([
            (
                "widgets".to_string(),
                vec![
                    ManifestEntry::Plain(PathBuf::from("output.py")),
                    ManifestEntry::Plain(PathBuf::from("generated/other.py")),
                ],
            ),
            (
                "pizzas".to_string(),
                vec![ManifestEntry::Plain(PathBuf::from("pizza.txt"))],
            ),
        ]);
        let rendered = serialize(&outputs);
        assert_eq!(
            rendered,
            "version: 1\ntaskGroups:\n  pizzas:\n  - pizza.txt\n  widgets:\n  - generated/other.py\n  - output.py\n"
        );
    }

    #[test]
    fn serialize_sorts_paths_within_each_group() {
        let outputs = BTreeMap::from([(
            "widgets".to_string(),
            vec![
                ManifestEntry::Plain(PathBuf::from("b.py")),
                ManifestEntry::Plain(PathBuf::from("a.py")),
            ],
        )]);
        let rendered = serialize(&outputs);
        assert_eq!(
            rendered,
            "version: 1\ntaskGroups:\n  widgets:\n  - a.py\n  - b.py\n"
        );
    }

    #[test]
    fn serialize_round_trips_through_parse() {
        let outputs = BTreeMap::from([(
            "widgets".to_string(),
            vec![
                ManifestEntry::Plain(PathBuf::from("b.py")),
                ManifestEntry::Plain(PathBuf::from("a.py")),
            ],
        )]);
        let rendered = serialize(&outputs);
        let parsed = parse(&rendered).unwrap();
        assert_eq!(
            parsed,
            BTreeMap::from([(
                "widgets".to_string(),
                vec![
                    ManifestEntry::Plain(PathBuf::from("a.py")),
                    ManifestEntry::Plain(PathBuf::from("b.py")),
                ],
            )])
        );
    }

    #[test]
    fn a_plain_entry_serializes_as_a_bare_scalar_not_a_mapping() {
        let outputs = BTreeMap::from([(
            "widgets".to_string(),
            vec![ManifestEntry::Plain(PathBuf::from("a.py"))],
        )]);
        let rendered = serialize(&outputs);
        assert_eq!(rendered, "version: 1\ntaskGroups:\n  widgets:\n  - a.py\n");
        assert!(!rendered.contains("path:"));
    }

    #[test]
    fn a_with_pocket_markers_entry_serializes_as_a_mapping_and_round_trips() {
        let outputs = BTreeMap::from([(
            "widgets".to_string(),
            vec![ManifestEntry::WithPocketMarkers {
                path: PathBuf::from("a.py"),
                pocket_markers: vec![MarkerPattern::new("# {name} [", "# ]")],
            }],
        )]);
        let rendered = serialize(&outputs);
        assert_eq!(
            rendered,
            "version: 1\ntaskGroups:\n  widgets:\n  - path: a.py\n    pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n"
        );
        let parsed = parse(&rendered).unwrap();
        assert_eq!(parsed, outputs);
    }

    #[test]
    fn stale_is_empty_when_current_matches_previous() {
        let previous = vec![PathBuf::from("a.py"), PathBuf::from("b.py")];
        let current = previous.clone();
        assert_eq!(stale(&previous, &current), Vec::<PathBuf>::new());
    }

    #[test]
    fn stale_is_empty_when_previous_is_a_subset_of_current() {
        let previous = vec![PathBuf::from("a.py")];
        let current = vec![PathBuf::from("a.py"), PathBuf::from("b.py")];
        assert_eq!(stale(&previous, &current), Vec::<PathBuf>::new());
    }

    #[test]
    fn stale_returns_every_entry_when_disjoint() {
        let previous = vec![PathBuf::from("a.py"), PathBuf::from("b.py")];
        let current = vec![PathBuf::from("c.py")];
        assert_eq!(
            stale(&previous, &current),
            vec![PathBuf::from("a.py"), PathBuf::from("b.py")]
        );
    }

    #[test]
    fn stale_returns_only_entries_missing_from_current() {
        let previous = vec![
            PathBuf::from("a.py"),
            PathBuf::from("b.py"),
            PathBuf::from("c.py"),
        ];
        let current = vec![PathBuf::from("b.py")];
        assert_eq!(
            stale(&previous, &current),
            vec![PathBuf::from("a.py"), PathBuf::from("c.py")]
        );
    }

    #[test]
    fn stale_compares_paths_once_normalized() {
        let previous = vec![PathBuf::from("./a.py"), PathBuf::from("x/../b.py")];
        let current = vec![PathBuf::from("a.py"), PathBuf::from("./b.py")];
        assert_eq!(stale(&previous, &current), Vec::<PathBuf>::new());
    }

    #[test]
    fn non_ascii_group_names_and_paths_round_trip() {
        let mut outputs = BTreeMap::new();
        for (group, path) in [
            ("ピザ", "出力/マルゲリータ.txt"),
            ("пицца: особая", "вывод/борщ 🍲.txt"),
            ("# 메모", "출력/김치.txt"),
            ("🍕", "żółw/Crème brûlée.txt"),
            ("e\u{301}cole", "e\u{301}cole.txt"),
            ("'quoted' \"too\"", "'a' \"b\".txt"),
        ] {
            outputs.insert(
                group.to_string(),
                vec![ManifestEntry::Plain(PathBuf::from(path))],
            );
        }
        assert_eq!(parse(&serialize(&outputs)).unwrap(), outputs);
    }

    #[test]
    fn non_ascii_paths_sort_stably_within_a_group() {
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "g".to_string(),
            ["ż.txt", "a.txt", "日.txt", "é.txt", "z.txt"]
                .into_iter()
                .map(|path| ManifestEntry::Plain(PathBuf::from(path)))
                .collect(),
        );
        assert_eq!(
            serialize(&outputs),
            "version: 1\ntaskGroups:\n  g:\n  - a.txt\n  - z.txt\n  - é.txt\n  - ż.txt\n  - 日.txt\n"
        );
    }

    #[test]
    fn stale_compares_non_ascii_paths_exactly() {
        // "é" precomposed and "e" + a combining accent are different paths
        // to crank, even though they render the same.
        let previous = vec![PathBuf::from("caf\u{e9}.txt"), PathBuf::from("日本.txt")];
        let current = vec![PathBuf::from("cafe\u{301}.txt"), PathBuf::from("日本.txt")];
        assert_eq!(
            stale(&previous, &current),
            vec![PathBuf::from("caf\u{e9}.txt")]
        );
    }

    #[test]
    fn a_parse_error_shows_the_yaml_parser_message() {
        let source = "- just\n- a list\n";
        let expected = serde_yaml_ng::from_str::<ManifestFile>(source)
            .unwrap_err()
            .to_string();
        assert_eq!(parse(source).unwrap_err().to_string(), expected);
    }
}
