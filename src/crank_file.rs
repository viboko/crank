// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of `.crank` file contents into task groups and tasks.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::pocket::MarkerPattern;
use crate::selector::ObjectSource;

/// The parsed contents of a `.crank` file: an ordered list of task groups,
/// each run independently against its own data file, plus any custom
/// filters made available to every template and any pocket markers
/// replacing crank's built-in ones.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawCrankFile")]
pub struct CrankFile {
    /// The task groups, in the order the file lists them.
    pub task_groups: Vec<TaskGroup>,
    /// Custom Tera filters: filter name -> path to a `.rhai` script
    /// (relative to the `.crank` file's own directory) defining it.
    pub filters: HashMap<String, PathBuf>,
    /// `pocketMarkers:` — the marker pairs that replace crank's built-in
    /// ones for every template that doesn't list its own in its header,
    /// if given. Only their shape is checked here; `run.rs` compiles them.
    pub pocket_markers: Option<Vec<MarkerPattern>>,
}

/// The literal YAML shape of a `.crank` file, deserialized as-is before
/// [`CrankFile`]'s `TryFrom` validates it — kept separate only so a
/// duplicate task-group `name` (a check that needs to see every group at
/// once, unlike the per-group checks in [`TaskGroup`]'s own `TryFrom`) can
/// become a [`CrankFileError`] rather than an opaque serde message.
#[derive(Debug, Deserialize)]
struct RawCrankFile {
    /// See [`CrankFile::task_groups`].
    #[serde(rename = "taskGroups")]
    task_groups: Vec<TaskGroup>,
    /// See [`CrankFile::filters`].
    #[serde(default)]
    filters: HashMap<String, PathBuf>,
    /// See [`CrankFile::pocket_markers`].
    #[serde(rename = "pocketMarkers", default)]
    pocket_markers: Option<Vec<RawMarkerPattern>>,
}

/// One `pocketMarkers` entry as `.crank` writes it. [`MarkerPattern`]
/// itself denies unknown keys, as a template header and `.crank.manifest`
/// need; this shape leaves them to [`parse`], which lets `x-` keys
/// through, as everywhere else in `.crank`.
#[derive(Debug, Deserialize)]
struct RawMarkerPattern {
    /// See [`MarkerPattern::start`].
    start: String,
    /// See [`MarkerPattern::end`].
    end: String,
}

/// A `.crank` file that failed a check spanning its whole list of task
/// groups: a repeated `name`, or a path leading outside the `.crank`
/// file's directory.
#[derive(Debug)]
pub enum CrankFileError {
    /// Two or more task groups share a `name`.
    DuplicateTaskGroupName {
        /// The repeated name.
        name: String,
    },
    /// A `template`, `object`/`array` or filter script path leads outside
    /// the `.crank` file's directory — see [`stays_inside`].
    PathOutsideDirectory {
        /// Which field gave the path, e.g. `` `template` ``.
        field: String,
        /// The path as written.
        path: PathBuf,
    },
}

impl fmt::Display for CrankFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CrankFileError::DuplicateTaskGroupName { name } => write!(
                f,
                "task group name \"{name}\" is used more than once — every task group's `name` must be unique"
            ),
            CrankFileError::PathOutsideDirectory { field, path } => write!(
                f,
                "{field} path \"{}\" leads outside the directory holding .crank — every path must stay inside it",
                path.display()
            ),
        }
    }
}

impl std::error::Error for CrankFileError {}

impl TryFrom<RawCrankFile> for CrankFile {
    type Error = CrankFileError;

    fn try_from(raw: RawCrankFile) -> Result<Self, Self::Error> {
        let mut seen = HashSet::new();
        for group in &raw.task_groups {
            if !seen.insert(group.name.as_str()) {
                return Err(CrankFileError::DuplicateTaskGroupName {
                    name: group.name.clone(),
                });
            }
        }
        check_paths_stay_inside(&raw)?;

        Ok(CrankFile {
            task_groups: raw.task_groups,
            filters: raw.filters,
            pocket_markers: raw.pocket_markers.map(|patterns| {
                patterns
                    .into_iter()
                    .map(|pattern| MarkerPattern::new(&pattern.start, &pattern.end))
                    .collect()
            }),
        })
    }
}

/// Whether `path`, taken relative to the `.crank` file's directory, stays
/// inside that directory: it isn't absolute, and no `..` component climbs
/// above where it started (`a/../b` stays inside, `a/../../b` doesn't).
/// Decided from the path's text alone — a symbolic link inside the
/// directory that points out of it isn't detected.
#[must_use]
pub fn stays_inside(path: &Path) -> bool {
    let mut depth: usize = 0;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return false,
            Component::CurDir => {}
            Component::ParentDir => match depth.checked_sub(1) {
                Some(parent) => depth = parent,
                None => return false,
            },
            Component::Normal(_) => depth = depth.saturating_add(1),
        }
    }
    true
}

/// `path` with `.` components dropped and `..` components resolved
/// lexically against the components before them, so `./a/../b.txt` and
/// `b.txt` come out the same.
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// Checks every path `raw` reads from — filter scripts, data files and
/// templates — with [`stays_inside`]. Filters are checked in name order,
/// so the error reported is the same from one run to the next.
///
/// # Errors
/// Returns [`CrankFileError::PathOutsideDirectory`] for the first path
/// that leads outside.
fn check_paths_stay_inside(raw: &RawCrankFile) -> Result<(), CrankFileError> {
    let mut filters: Vec<(&String, &PathBuf)> = raw.filters.iter().collect();
    filters.sort();
    let filter_paths = filters
        .into_iter()
        .map(|(name, path)| (format!("filter `{name}`'s script"), path));
    let group_paths = raw.task_groups.iter().flat_map(|group| {
        let field = match group.source {
            Source::Object(_) => "`object`",
            Source::Array(_) => "`array`",
        };
        std::iter::once((field.to_string(), &group.source.object_source().path)).chain(
            group
                .tasks
                .iter()
                .map(|task| ("`template`".to_string(), &task.template)),
        )
    });

    match filter_paths
        .chain(group_paths)
        .find(|(_, path)| !stays_inside(path))
    {
        Some((field, path)) => Err(CrankFileError::PathOutsideDirectory {
            field,
            path: path.clone(),
        }),
        None => Ok(()),
    }
}

/// A task group's data source: either a single `object`, loaded once and
/// applied to every task, or an `array`, whose tasks each run once per
/// item in the sequence. A `.crank` file must specify exactly one of the
/// two per task group — see [`TaskGroupError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `object:` — one value, applied to every task once.
    Object(ObjectSource),
    /// `array:` — a sequence, with every task run once per item.
    Array(ObjectSource),
}

impl Source {
    /// The wrapped [`ObjectSource`] (a data file path plus an optional
    /// `#selector`) regardless of variant. Both variants share this exact
    /// shape — only how the selected value's shape is checked once loaded
    /// differs (a mapping for `Object`, a sequence for `Array`), which is
    /// the caller's job to handle by matching on the variant itself.
    #[must_use]
    pub fn object_source(&self) -> &ObjectSource {
        match self {
            Source::Object(object) | Source::Array(object) => object,
        }
    }
}

/// One task group: a [`Source`], applied to every [`Task`] listed under it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawTaskGroup")]
pub struct TaskGroup {
    /// Identifies this task group, e.g. as a key in `.crank.manifest`.
    /// Required, and must be unique across every task group in a `.crank`
    /// file — see [`CrankFileError::DuplicateTaskGroupName`].
    pub name: String,
    /// Where the group's data comes from.
    pub source: Source,
    /// The group's tasks, in the order the file lists them.
    pub tasks: Vec<Task>,
}

/// The literal YAML shape of a task group, deserialized as-is before
/// [`TaskGroup`]'s `TryFrom` validates it — `object`/`array` are both
/// optional here only so their absence/co-presence can be turned into a
/// [`TaskGroupError`] rather than an opaque serde "missing field" message.
/// `name` is a plain required field instead, like [`Task`]'s fields — a
/// missing `name` surfaces as serde's ordinary "missing field" error.
#[derive(Debug, Deserialize)]
struct RawTaskGroup {
    /// See [`TaskGroup::name`].
    name: String,
    /// `object:`, if given — see [`Source::Object`].
    object: Option<ObjectSource>,
    /// `array:`, if given — see [`Source::Array`].
    array: Option<ObjectSource>,
    /// See [`TaskGroup::tasks`].
    #[serde(default)]
    tasks: Vec<Task>,
}

/// A task group that failed the checks a plain `#[derive(Deserialize)]`
/// can't express: specifying both/neither of `object`/`array`, or an
/// `array` group task whose `output` has no substitution.
#[derive(Debug)]
pub enum TaskGroupError {
    /// The group gives both `object` and `array`.
    BothObjectAndArray,
    /// The group gives neither `object` nor `array`.
    NeitherObjectNorArray,
    /// An `array` group's task writes every item to the same `output`.
    ArrayOutputMissingSubstitution {
        /// The task's `output`, which has no `{{ ... }}` in it.
        output: PathBuf,
    },
}

impl fmt::Display for TaskGroupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskGroupError::BothObjectAndArray => write!(
                f,
                "a task group can't specify both `object` and `array` — pick one"
            ),
            TaskGroupError::NeitherObjectNorArray => {
                write!(f, "a task group must specify either `object` or `array`")
            }
            TaskGroupError::ArrayOutputMissingSubstitution { output } => write!(
                f,
                "an `array` task group's task `output` (\"{}\") must include a `{{{{ ... }}}}` substitution, since every item would otherwise be written to the same file",
                output.display()
            ),
        }
    }
}

impl std::error::Error for TaskGroupError {}

impl TryFrom<RawTaskGroup> for TaskGroup {
    type Error = TaskGroupError;

    fn try_from(raw: RawTaskGroup) -> Result<Self, Self::Error> {
        let source = match (raw.object, raw.array) {
            (Some(_), Some(_)) => return Err(TaskGroupError::BothObjectAndArray),
            (None, None) => return Err(TaskGroupError::NeitherObjectNorArray),
            (Some(object), None) => Source::Object(object),
            (None, Some(array)) => {
                if let Some(task) = raw
                    .tasks
                    .iter()
                    .find(|task| !task.output.to_string_lossy().contains("{{"))
                {
                    return Err(TaskGroupError::ArrayOutputMissingSubstitution {
                        output: task.output.clone(),
                    });
                }
                Source::Array(array)
            }
        };

        Ok(TaskGroup {
            name: raw.name,
            source,
            tasks: raw.tasks,
        })
    }
}

/// One task: a template rendered with the enclosing [`TaskGroup`]'s data,
/// written to `output`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Task {
    /// Path to the template file, relative to the `.crank` file's own
    /// directory.
    #[serde(deserialize_with = "path_string")]
    pub template: PathBuf,
    /// Path to write the rendered output to, relative to the `.crank`
    /// file's own directory.
    #[serde(deserialize_with = "path_string")]
    pub output: PathBuf,
}

/// Deserializes a task's `template`/`output` path. YAML reads a plain
/// value starting with `{` as a flow mapping, so an unquoted
/// `output: {{ name }}.txt` arrives as a map rather than a string; that
/// case gets an error saying to quote it, instead of serde's bare "invalid
/// type: map".
fn path_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    struct PathVisitor;

    impl<'de> serde::de::Visitor<'de> for PathVisitor {
        type Value = PathBuf;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("path string")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<PathBuf, E> {
            Ok(PathBuf::from(value))
        }

        // A plain scalar YAML reads as a number or boolean (`output: 42`)
        // is still a path, as it was when this deserialized straight into
        // a `PathBuf`.
        fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<PathBuf, E> {
            Ok(PathBuf::from(value.to_string()))
        }

        fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<PathBuf, E> {
            Ok(PathBuf::from(value.to_string()))
        }

        fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<PathBuf, E> {
            Ok(PathBuf::from(value.to_string()))
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<PathBuf, E> {
            Ok(PathBuf::from(value.to_string()))
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(self, _: A) -> Result<PathBuf, A::Error> {
            Err(serde::de::Error::custom(
                "expected a path string, but YAML read a mapping: a value starting with `{{ ... }}` must be quoted, e.g. `output: \"{{ name }}.txt\"`",
            ))
        }
    }

    deserializer.deserialize_any(PathVisitor)
}

/// The prefix marking a `.crank` key as the user's own, for crank to
/// ignore — e.g. `x-defaults:`, holding a YAML anchor for other entries to
/// reuse.
const EXTENSION_PREFIX: &str = "x-";

/// A `.crank` file that failed to parse as the expected YAML shape.
#[derive(Debug)]
pub enum ParseError {
    /// Not valid YAML, or not the shape `.crank` expects.
    Yaml(serde_yaml_ng::Error),
    /// A key crank doesn't know, and that doesn't start with `x-`.
    UnknownKey {
        /// The key as written.
        key: String,
        /// Where it is, e.g. `taskGroups[0].tasks[1]`, or `the top level`.
        at: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Yaml(err) => write!(f, "{err}"),
            ParseError::UnknownKey { key, at } if key == "<<" => write!(
                f,
                "YAML merge keys (`<<`) aren't supported, at {at} — reuse an anchored value whole with an alias (`*name`) instead"
            ),
            ParseError::UnknownKey { key, at } => write!(
                f,
                "unknown key \"{key}\" at {at} — crank ignores only keys starting with \"{EXTENSION_PREFIX}\""
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses `source` (the raw contents of a `.crank` file) into a
/// [`CrankFile`].
///
/// # Errors
/// Returns [`ParseError`] if `source` isn't valid YAML, doesn't match
/// the expected `.crank` shape, or has a key crank doesn't know that
/// doesn't start with `x-`. An unknown key is reported ahead of a shape
/// error — the first one, in the order written.
pub fn parse(source: &str) -> Result<CrankFile, ParseError> {
    let mut unknown = None;
    let result =
        serde_ignored::deserialize(serde_yaml_ng::Deserializer::from_str(source), |path| {
            if unknown.is_none()
                && let serde_ignored::Path::Map { parent, key } = path
                && !key.starts_with(EXTENSION_PREFIX)
            {
                unknown = Some(ParseError::UnknownKey {
                    key: key.clone(),
                    at: location(parent),
                });
            }
        });
    // An unknown key found before a shape error is usually its cause: a
    // wrongly named `tmpl:`, or a `<<:` merge crank didn't apply, leaves
    // its field missing.
    match unknown {
        Some(err) => Err(err),
        None => result.map_err(ParseError::Yaml),
    }
}

/// Where in `.crank` `path` leads, written the way serde's own errors
/// write it — `taskGroups[0].tasks[1]` — or `the top level` for the root.
fn location(path: &serde_ignored::Path<'_>) -> String {
    let mut steps = Vec::new();
    let mut current = path;
    loop {
        current = match current {
            serde_ignored::Path::Root => break,
            serde_ignored::Path::Seq { parent, index } => {
                steps.push(format!("[{index}]"));
                parent
            }
            serde_ignored::Path::Map { parent, key } => {
                steps.push(format!(".{key}"));
                parent
            }
            serde_ignored::Path::Some { parent }
            | serde_ignored::Path::NewtypeStruct { parent }
            | serde_ignored::Path::NewtypeVariant { parent } => parent,
        };
    }
    if steps.is_empty() {
        return "the top level".to_string();
    }
    let joined: String = steps.into_iter().rev().collect();
    joined.strip_prefix('.').unwrap_or(&joined).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_group_with_a_single_task() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    tasks:
      - template: template.tera
        output: output.py
";
        let crank_file = parse(source).unwrap();
        assert_eq!(
            crank_file,
            CrankFile {
                task_groups: vec![TaskGroup {
                    name: "widgets".to_string(),
                    source: Source::Object(ObjectSource::from_path("object.yaml")),
                    tasks: vec![Task {
                        template: PathBuf::from("template.tera"),
                        output: PathBuf::from("output.py"),
                    }],
                }],
                filters: HashMap::new(),
                pocket_markers: None,
            }
        );
    }

    #[test]
    fn parses_multiple_groups() {
        let source = "
taskGroups:
  - name: a
    object: a.yaml
    tasks:
      - template: a.tera
        output: a.py
  - name: b
    object: b.yaml
    tasks:
      - template: b.tera
        output: b.py
";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.task_groups.len(), 2);
        assert_eq!(
            crank_file.task_groups[0].source.object_source(),
            &ObjectSource::from_path("a.yaml")
        );
        assert_eq!(
            crank_file.task_groups[1].source.object_source(),
            &ObjectSource::from_path("b.yaml")
        );
    }

    #[test]
    fn parses_multiple_tasks_within_one_group() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    tasks:
      - template: one.tera
        output: one.py
      - template: two.tera
        output: two.py
";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.task_groups[0].tasks.len(), 2);
    }

    #[test]
    fn neither_object_nor_array_field_is_a_parse_error() {
        let source = "
taskGroups:
  - name: widgets
    tasks:
      - template: template.tera
        output: output.py
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn both_object_and_array_fields_is_a_parse_error() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    array: object.yaml#pizzas
    tasks:
      - template: template.tera
        output: pizza-{{ name }}.txt
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn missing_template_field_is_a_parse_error() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    tasks:
      - output: output.py
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn missing_name_field_is_a_parse_error() {
        let source = "
taskGroups:
  - object: object.yaml
    tasks:
      - template: template.tera
        output: output.py
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn duplicate_task_group_names_is_a_parse_error() {
        let source = "
taskGroups:
  - name: widgets
    object: a.yaml
    tasks:
      - template: template.tera
        output: a.py
  - name: widgets
    object: b.yaml
    tasks:
      - template: template.tera
        output: b.py
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn an_object_field_with_a_selector_parses() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml#pizzas[0]
    tasks:
      - template: template.tera
        output: output.py
";
        let crank_file = parse(source).unwrap();
        let object = crank_file.task_groups[0].source.object_source();
        assert_eq!(object.path, PathBuf::from("object.yaml"));
        assert!(object.selector.is_some());
    }

    #[test]
    fn malformed_selector_syntax_is_a_parse_error() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml#pizzas[
    tasks:
      - template: template.tera
        output: output.py
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn malformed_yaml_is_a_parse_error() {
        let source = "taskGroups: [this is not: valid: yaml:";
        assert!(parse(source).is_err());
    }

    #[test]
    fn empty_task_groups_list_is_valid() {
        let source = "taskGroups: []";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.task_groups, Vec::<TaskGroup>::new());
    }

    #[test]
    fn a_missing_task_groups_key_is_a_parse_error() {
        for source in ["filters: {}\n", "x-notes: none yet\n", "{}"] {
            let err = parse(source).unwrap_err().to_string();
            assert!(
                err.contains("missing field `taskGroups`"),
                "{source:?}: {err}"
            );
        }
    }

    #[test]
    fn a_group_with_no_tasks_is_valid() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    tasks: []
";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.task_groups[0].tasks, Vec::<Task>::new());
    }

    #[test]
    fn filters_field_defaults_to_empty_when_absent() {
        let source = "taskGroups: []";
        let crank_file = parse(source).unwrap();
        assert!(crank_file.filters.is_empty());
    }

    #[test]
    fn empty_filters_map_is_valid() {
        let source = "
taskGroups: []
filters: {}
";
        let crank_file = parse(source).unwrap();
        assert!(crank_file.filters.is_empty());
    }

    #[test]
    fn parses_a_filters_mapping() {
        let source = "
taskGroups: []
filters:
  reverse: reverse.rhai
";
        let crank_file = parse(source).unwrap();
        assert_eq!(
            crank_file.filters.get("reverse"),
            Some(&PathBuf::from("reverse.rhai"))
        );
    }

    #[test]
    fn parses_multiple_filters() {
        let source = "
taskGroups: []
filters:
  reverse: reverse.rhai
  shout: shout.rhai
";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.filters.len(), 2);
        assert_eq!(
            crank_file.filters.get("shout"),
            Some(&PathBuf::from("shout.rhai"))
        );
    }

    #[test]
    fn filters_as_a_list_is_a_parse_error() {
        let source = "
taskGroups: []
filters:
  - reverse: reverse.rhai
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn parses_an_array_group_with_a_selector() {
        let source = "
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.tera
        output: pizza-{{ name }}.txt
";
        let crank_file = parse(source).unwrap();
        let Source::Array(object) = &crank_file.task_groups[0].source else {
            panic!("expected an array source");
        };
        assert_eq!(object.path, PathBuf::from("object.yaml"));
        assert!(object.selector.is_some());
    }

    #[test]
    fn parses_an_array_group_with_no_selector() {
        let source = "
taskGroups:
  - name: pizzas
    array: pizzas.yaml
    tasks:
      - template: pizza.tera
        output: pizza-{{ name }}.txt
";
        let crank_file = parse(source).unwrap();
        assert_eq!(
            crank_file.task_groups[0].source,
            Source::Array(ObjectSource::from_path("pizzas.yaml"))
        );
    }

    #[test]
    fn an_array_group_task_output_with_no_substitution_is_a_parse_error() {
        let source = "
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.tera
        output: pizza.txt
";
        assert!(parse(source).is_err());
    }

    #[test]
    fn an_array_group_task_output_with_a_substitution_parses() {
        let source = "
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.tera
        output: pizza-{{ name }}.txt
";
        assert!(parse(source).is_ok());
    }

    #[test]
    fn an_output_starting_with_an_unquoted_substitution_is_a_parse_error_saying_to_quote_it() {
        let source = "
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.tera
        output: {{ name }}.txt
";
        let error = parse(source).unwrap_err().to_string();
        assert!(error.contains("must be quoted"), "{error}");
    }

    #[test]
    fn an_output_starting_with_a_quoted_substitution_parses() {
        let source = r#"
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.tera
        output: "{{ name }}.txt"
"#;
        let crank_file = parse(source).unwrap();
        assert_eq!(
            crank_file.task_groups[0].tasks[0].output,
            PathBuf::from("{{ name }}.txt")
        );
    }

    #[test]
    fn an_object_group_task_output_needs_no_substitution() {
        let source = "
taskGroups:
  - name: widgets
    object: object.yaml
    tasks:
      - template: template.tera
        output: output.py
";
        assert!(parse(source).is_ok());
    }

    #[test]
    fn root_level_pocket_markers_parse() {
        let source = "
taskGroups: []
pocketMarkers:
  - start: '# {name} ['
    end: '# ]'
";
        let crank_file = parse(source).unwrap();
        assert_eq!(
            crank_file.pocket_markers,
            Some(vec![MarkerPattern::new("# {name} [", "# ]")])
        );
    }

    #[test]
    fn absent_pocket_markers_are_none() {
        assert_eq!(parse("taskGroups: []\n").unwrap().pocket_markers, None);
    }

    #[test]
    fn a_pocket_marker_missing_its_end_is_a_parse_error() {
        assert!(parse("pocketMarkers: [{start: '# {name} ['}]\n").is_err());
    }

    #[test]
    fn non_ascii_names_and_paths_parse() {
        let source = "
taskGroups:
  - name: メニュー 🍕
    object: データ/ピザ.yaml#メニュー[0]
    tasks:
      - template: шаблоны/меню.tera
        output: 출력/{{ name }}.txt
filters:
  odwroc: filtry/odwróć.rhai
";
        let crank_file = parse(source).unwrap();
        let group = &crank_file.task_groups[0];
        assert_eq!(group.name, "メニュー 🍕");
        assert_eq!(
            group.source.object_source().path,
            PathBuf::from("データ/ピザ.yaml")
        );
        assert_eq!(group.tasks[0].template, PathBuf::from("шаблоны/меню.tera"));
        assert_eq!(group.tasks[0].output, PathBuf::from("출력/{{ name }}.txt"));
        assert_eq!(
            crank_file.filters.get("odwroc"),
            Some(&PathBuf::from("filtry/odwróć.rhai"))
        );
    }

    #[test]
    fn duplicate_non_ascii_group_names_are_reported_by_name() {
        let source = "
taskGroups:
  - name: ピザ
    object: a.yaml
    tasks: []
  - name: ピザ
    object: b.yaml
    tasks: []
";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("ピザ"), "{err}");
    }

    #[test]
    fn a_leading_byte_order_mark_is_ignored() {
        let source = "\u{feff}taskGroups:\n  - name: g\n    object: o.yaml\n    tasks: []\n";
        assert_eq!(parse(source).unwrap().task_groups[0].name, "g");
    }

    #[test]
    fn a_path_yaml_reads_as_a_number_or_boolean_is_still_a_path() {
        let source = "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - template: 42\n        output: true\n      - template: -7\n        output: 1.5\n";
        let crank_file = parse(source).unwrap();
        let paths: Vec<_> = crank_file.task_groups[0]
            .tasks
            .iter()
            .map(|task| (task.template.clone(), task.output.clone()))
            .collect();
        assert_eq!(
            paths,
            vec![
                (PathBuf::from("42"), PathBuf::from("true")),
                (PathBuf::from("-7"), PathBuf::from("1.5")),
            ]
        );
    }

    #[test]
    fn a_path_given_as_a_sequence_is_a_parse_error_saying_a_path_was_expected() {
        let source = "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - template: [a, b]\n        output: out.txt\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("expected path string"), "{err}");
    }

    #[test]
    fn stays_inside_accepts_paths_that_never_climb_above_the_directory() {
        for path in [
            "out.txt",
            "a/b/c.txt",
            "./a.txt",
            "a/../b.txt",
            "a/b/../../c.txt",
            "",
        ] {
            assert!(stays_inside(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn stays_inside_rejects_absolute_paths_and_climbs_above_the_directory() {
        for path in [
            "../x.txt",
            "a/../../x.txt",
            "./../x",
            "/etc/passwd",
            "a/../../a/x.txt",
        ] {
            assert!(!stays_inside(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_template_outside_the_directory_is_a_parse_error() {
        let source = "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - template: ../t.tera\n        output: out.txt\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(
            err.contains(
                "`template` path \"../t.tera\" leads outside the directory holding .crank"
            ),
            "{err}"
        );
    }

    #[test]
    fn a_data_file_outside_the_directory_is_a_parse_error() {
        let object = "taskGroups:\n  - name: g\n    object: /etc/o.yaml\n    tasks: []\n";
        let err = parse(object).unwrap_err().to_string();
        assert!(err.contains("`object` path \"/etc/o.yaml\""), "{err}");

        let array = "taskGroups:\n  - name: g\n    array: ../o.yaml#items\n    tasks: []\n";
        let err = parse(array).unwrap_err().to_string();
        assert!(err.contains("`array` path \"../o.yaml\""), "{err}");
    }

    #[test]
    fn a_filter_script_outside_the_directory_is_a_parse_error() {
        let source = "filters:\n  b: ../b.rhai\n  a: ../a.rhai\ntaskGroups: []\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(
            err.contains("filter `a`'s script path \"../a.rhai\""),
            "the first filter by name is reported: {err}"
        );
    }

    #[test]
    fn an_unknown_key_at_any_level_is_a_parse_error() {
        let cases = [
            ("taskgroups: []\n", "\"taskgroups\" at the top level"),
            (
                "taskGroups:\n  - name: g\n    objcet: o.yaml\n    object: o.yaml\n",
                "\"objcet\" at taskGroups[0]",
            ),
            (
                "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - template: t.tera\n        output: o.txt\n      - template: t.tera\n        output: p.txt\n        mode: 644\n",
                "\"mode\" at taskGroups[0].tasks[1]",
            ),
            (
                "pocketMarkers:\n  - start: '# {name} ['\n    end: '# ]'\n    middle: x\ntaskGroups: []\n",
                "\"middle\" at pocketMarkers[0]",
            ),
        ];
        for (source, expected) in cases {
            let err = parse(source).unwrap_err().to_string();
            assert!(
                err.contains(&format!("unknown key {expected}")),
                "{source}: {err}"
            );
        }
    }

    #[test]
    fn the_first_unknown_key_in_the_file_is_reported() {
        let source = "first: 1\ntaskGroups: []\nsecond: 2\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("\"first\""), "{err}");
    }

    #[test]
    fn keys_starting_with_x_are_ignored_at_every_level() {
        let source = "
x-notes: anything at all
pocketMarkers:
  - start: '# {name} ['
    end: '# ]'
    x-why: Python
taskGroups:
  - name: widgets
    x-owner: me
    object: object.yaml
    tasks:
      - template: template.tera
        output: output.py
        x-reviewed: true
";
        let crank_file = parse(source).unwrap();
        assert_eq!(crank_file.task_groups.len(), 1);
        assert_eq!(
            crank_file.pocket_markers,
            Some(vec![MarkerPattern::new("# {name} [", "# ]")])
        );
    }

    #[test]
    fn an_x_key_can_hold_an_anchor_that_aliases_reuse() {
        let source = "
x-tasks: &tasks
  - template: model.py.tera
    output: \"{{ name }}.py\"
taskGroups:
  - name: pizzas
    array: pizzas.yaml
    tasks: *tasks
  - name: pastas
    array: pastas.yaml
    tasks: *tasks
";
        let crank_file = parse(source).unwrap();
        let [pizzas, pastas] = crank_file.task_groups.as_slice() else {
            panic!("two task groups: {crank_file:?}");
        };
        assert_eq!(pizzas.tasks, pastas.tasks);
        assert_eq!(pizzas.tasks[0].template, PathBuf::from("model.py.tera"));
    }

    #[test]
    fn a_merge_key_is_a_parse_error_suggesting_an_alias() {
        let source = "
x-task: &task
  template: t.tera
taskGroups:
  - name: g
    object: o.yaml
    tasks:
      - <<: *task
        output: o.txt
";
        let err = parse(source).unwrap_err().to_string();
        assert!(
            err.contains("YAML merge keys (`<<`) aren't supported, at taskGroups[0].tasks[0]"),
            "{err}"
        );
        assert!(err.contains("alias (`*name`)"), "{err}");
    }

    #[test]
    fn a_wrongly_named_key_is_reported_rather_than_the_field_it_leaves_missing() {
        let source = "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - tmpl: t.tera\n        output: o.txt\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(
            err.contains("unknown key \"tmpl\" at taskGroups[0].tasks[0]"),
            "{err}"
        );
    }

    #[test]
    fn a_filter_name_is_not_an_unknown_key() {
        let source = "filters:\n  backwards: backwards.rhai\ntaskGroups: []\n";
        assert!(parse(source).is_ok());
    }
}
