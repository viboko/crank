// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Running a `.crank` file end to end: reading it and everything it
//! references, rendering every task, writing the outputs and reconciling
//! `.crank.manifest` — over a [`Workspace`] rather than the disk directly,
//! so the CLI (`main.rs`, over the real filesystem) and the browser
//! playground (over a [`MemoryWorkspace`]) run exactly the same logic.
//!
//! Nothing here prints: [`run`] returns a [`Report`], which
//! [`crate::output::report_lines`] turns into what crank prints.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::check::ChangeKind;
use crate::crank_file::{Source, Task, TaskGroup, normalize, stays_inside};
use crate::manifest::ManifestEntry;
use crate::pocket::{MarkerPattern, OldPockets, PocketMarkers};
use crate::rhai_filter::CompiledFilter;
use unicode_normalization::UnicodeNormalization;

/// The task file crank looks for in the directory it runs in.
pub const CRANK_FILE_NAME: &str = ".crank";
/// crank's record of what it generated on its last successful run.
pub const MANIFEST_FILE_NAME: &str = ".crank.manifest";

/// The files crank reads and writes. Every path it's given is the run's
/// root joined with a path from `.crank` (or `.crank.manifest`).
pub trait Workspace {
    /// Reads the file at `path` as UTF-8 text.
    ///
    /// # Errors
    /// Returns an error of kind [`ErrorKind::NotFound`] if there's no file
    /// at `path`, or any other error if it can't be read.
    fn read(&self, path: &Path) -> io::Result<String>;

    /// Creates the directory at `path` and every missing parent.
    ///
    /// # Errors
    /// Returns an error if a directory can't be created.
    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;

    /// Writes `contents` to the file at `path`, replacing any file there.
    ///
    /// # Errors
    /// Returns an error if the file can't be written.
    fn write(&mut self, path: &Path, contents: &str) -> io::Result<()>;

    /// Deletes the file at `path`.
    ///
    /// # Errors
    /// Returns an error of kind [`ErrorKind::NotFound`] if there's no file
    /// at `path`, or any other error if it can't be deleted.
    fn remove(&mut self, path: &Path) -> io::Result<()>;

    /// Whether `a` and `b` are two paths to one existing file — as
    /// `Out.txt` and `out.txt` are on a case-insensitive filesystem.
    /// `false` if either doesn't exist.
    ///
    /// # Errors
    /// Returns an error if it can't tell.
    fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool>;
}

/// A [`Workspace`] held entirely in memory: a map from path to contents.
/// Paths are compared after dropping `.` components and resolving `..`
/// ones lexically, so `./a/../b.txt` and `b.txt` are the same file.
/// Directories aren't tracked — every file's parents exist implicitly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryWorkspace {
    /// Every file's contents, by its normalized path.
    files: BTreeMap<PathBuf, String>,
}

impl MemoryWorkspace {
    /// An empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) the file at `path`.
    pub fn insert(&mut self, path: impl AsRef<Path>, contents: impl Into<String>) {
        self.files.insert(normalize(path.as_ref()), contents.into());
    }

    /// Every file, by its normalized path.
    #[must_use]
    pub fn files(&self) -> &BTreeMap<PathBuf, String> {
        &self.files
    }
}

/// The error for a file that isn't there, worded the way the operating
/// system words it on Linux and macOS — so a run over a
/// [`MemoryWorkspace`] reports a missing file exactly as the CLI does there.
fn not_found() -> io::Error {
    io::Error::new(
        ErrorKind::NotFound,
        "No such file or directory (os error 2)",
    )
}

impl Workspace for MemoryWorkspace {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .get(&normalize(path))
            .cloned()
            .ok_or_else(not_found)
    }

    fn create_dir_all(&mut self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        self.insert(path, contents);
        Ok(())
    }

    fn remove(&mut self, path: &Path) -> io::Result<()> {
        self.files
            .remove(&normalize(path))
            .map(|_| ())
            .ok_or_else(not_found)
    }

    fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
        let a = normalize(a);
        Ok(a == normalize(b) && self.files.contains_key(&a))
    }
}

/// What a run did, for [`crate::output::report_lines`] to describe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// There's no `.crank` file, so there was nothing to do.
    NoCrankFile,
    /// Something went wrong; every problem found, in order. Tasks that
    /// succeeded before or after a failing one have still written their
    /// outputs, but stale outputs weren't removed — unless two tasks
    /// generate the same file, in which case nothing was written at all.
    Failed(Vec<String>),
    /// Every task succeeded: the outputs generated, in task order, and the
    /// stale outputs removed.
    Generated {
        /// Every output generated, in task order, with what generating it
        /// did to the file — an [`ChangeKind::Unchanged`] one was left
        /// alone, not rewritten.
        outputs: Vec<(PathBuf, ChangeKind)>,
        /// Every stale output deleted.
        removed: Vec<PathBuf>,
    },
    /// A `--check` run found no errors: what a normal run would change.
    Checked {
        /// Every output that would be created or modified.
        changes: Vec<(PathBuf, ChangeKind)>,
        /// Every stale output that would be removed.
        stale: Vec<PathBuf>,
    },
}

impl Report {
    /// Whether crank exits successfully after this run.
    #[must_use]
    pub fn success(&self) -> bool {
        match self {
            Self::NoCrankFile | Self::Generated { .. } => true,
            Self::Failed(_) => false,
            Self::Checked { changes, stale } => changes.is_empty() && stale.is_empty(),
        }
    }
}

/// Runs the `.crank` file in `root`, reading and writing through
/// `workspace`. With `check`, nothing is written or deleted: the report
/// says what a normal run would change instead.
pub fn run(workspace: &mut dyn Workspace, root: &Path, check: bool) -> Report {
    let crank_path = root.join(CRANK_FILE_NAME);

    let contents = match workspace.read(&crank_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Report::NoCrankFile,
        Err(err) => {
            return Report::Failed(vec![format!(
                "Failed to read {}: {err}",
                crank_path.display()
            )]);
        }
    };

    let crank_file = match crate::crank_file::parse(&contents) {
        Ok(crank_file) => crank_file,
        Err(err) => {
            return Report::Failed(vec![format!(
                "Failed to parse {}: {err}",
                crank_path.display()
            )]);
        }
    };

    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let previous_outputs = match read_manifest(workspace, &manifest_path) {
        Ok(previous_outputs) => previous_outputs,
        Err(err) => {
            return Report::Failed(vec![format!(
                "Failed to read {}: {err}",
                manifest_path.display()
            )]);
        }
    };
    if let Err(errors) = check_manifest_paths(&previous_outputs) {
        return Report::Failed(errors);
    }

    let filters = match load_filters(workspace, root, &crank_file.filters) {
        Ok(filters) => filters,
        Err(errors) => return Report::Failed(errors),
    };

    let project_markers = match crank_file
        .pocket_markers
        .map(|patterns| PocketMarkers::compile(&patterns).map(|compiled| (patterns, compiled)))
        .transpose()
    {
        Ok(project_markers) => project_markers,
        Err(err) => {
            return Report::Failed(vec![format!(
                "Failed to use the pocketMarkers in {}: {err}",
                crank_path.display()
            )]);
        }
    };

    let default_markers = PocketMarkers::default_markers();
    let run_config = RunConfig {
        filters: &filters,
        previous_outputs: &previous_outputs,
        default_markers: &default_markers,
        project_markers: project_markers.as_ref(),
    };

    let mut pending: Vec<(String, Vec<PendingOutput>)> = Vec::new();
    let mut errors = Vec::new();

    for group in &crank_file.task_groups {
        let mut group_pending: Vec<PendingOutput> = Vec::new();
        let mut accumulators = Accumulators {
            pending: &mut group_pending,
            errors: &mut errors,
        };
        run_task_group(workspace, root, group, &run_config, &mut accumulators);
        pending.push((group.name.clone(), group_pending));
    }

    // Checked before anything is written, so two tasks generating the
    // same file leave every file as it was.
    let collisions = collisions(pending.iter().flat_map(|(_, outputs)| outputs));
    if !collisions.is_empty() {
        errors.extend(collisions);
        return Report::Failed(errors);
    }

    let Written {
        kinds,
        outputs_by_group,
    } = write_outputs(workspace, root, pending, check, &mut errors);
    if !errors.is_empty() {
        return Report::Failed(errors);
    }

    if check {
        let outputs: Vec<PathBuf> = kinds.iter().map(|(path, _)| path.clone()).collect();
        check_reconcile(workspace, root, &previous_outputs, &outputs, kinds)
    } else {
        reconcile_manifest(
            workspace,
            root,
            &manifest_path,
            &previous_outputs,
            kinds,
            &outputs_by_group,
        )
    }
}

/// One output a task generated, rendered and ready to write once every
/// task has run.
struct PendingOutput {
    /// Which task generated it — its group, its item for an `array`
    /// group, and its template — to name in a collision's error.
    origin: String,
    /// Its contents, pockets already applied.
    text: String,
    /// What to record for it in `.crank.manifest`, including its path.
    entry: ManifestEntry,
}

/// What [`collisions`] compares output paths by: the path [`normalize`]d,
/// lowercased and decomposed (Unicode NFD), so any two that a
/// case-insensitive filesystem (macOS's and Windows' by default) or a
/// normalization-insensitive one (macOS's) would take for the same file
/// match — `café.txt` with a precomposed `é` and with `e` plus a
/// combining accent, say.
fn collision_key(path: &Path) -> String {
    normalize(path)
        .to_string_lossy()
        .to_lowercase()
        .nfd()
        .collect()
}

/// Why two paths with the same [`collision_key`] name the same file, for
/// [`collisions`] to add to its error — nothing if it's plain from the
/// paths alone.
fn collision_note(a: &Path, b: &Path) -> &'static str {
    let (a, b) = (normalize(a), normalize(b));
    if a == b {
        ""
    } else if a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() {
        " (paths that differ only in letter case name the same file on macOS and Windows)"
    } else {
        " (paths that differ only in how accented letters are encoded, or in letter case, name the same file on macOS)"
    }
}

/// A message for every output in `outputs` that's the same file as an
/// earlier one — the same path, or one that differs only in letter case,
/// in how accented letters are encoded, or in `.`/`..` components —
/// naming both tasks.
fn collisions<'a>(outputs: impl Iterator<Item = &'a PendingOutput>) -> Vec<String> {
    let mut first_by_key: HashMap<String, &PendingOutput> = HashMap::new();
    let mut errors = Vec::new();
    for output in outputs {
        let path = output.entry.path();
        let Some(first) = first_by_key.get(&collision_key(path)) else {
            first_by_key.insert(collision_key(path), output);
            continue;
        };
        let first_path = first.entry.path();
        let note = collision_note(first_path, path);
        errors.push(format!(
            "Two tasks generate the same file — \"{}\" from {}, and \"{}\" from {}{note} — so nothing was written",
            first_path.display(),
            first.origin,
            path.display(),
            output.origin,
        ));
    }
    errors
}

/// What [`write_outputs`] wrote, or under `--check` would write.
struct Written {
    /// What writing each output did, in task order.
    kinds: Vec<(PathBuf, ChangeKind)>,
    /// The manifest entries of the outputs written, by group.
    outputs_by_group: BTreeMap<String, Vec<ManifestEntry>>,
}

/// Writes every output in `pending` (or, under `check`, works out what
/// writing it would do), group by group, pushing every failure's message
/// to `errors`.
fn write_outputs(
    workspace: &mut dyn Workspace,
    root: &Path,
    pending: Vec<(String, Vec<PendingOutput>)>,
    check: bool,
    errors: &mut Vec<String>,
) -> Written {
    let mut kinds = Vec::new();
    let mut outputs_by_group = BTreeMap::new();
    for (group_name, outputs) in pending {
        let mut entries = Vec::new();
        for output in outputs {
            let path = output.entry.path();
            match write_output(workspace, root, path, &output.text, check) {
                Ok(kind) => {
                    kinds.push((path.to_path_buf(), kind));
                    entries.push(output.entry);
                }
                Err(err) => errors.push(err),
            }
        }
        outputs_by_group.insert(group_name, entries);
    }
    Written {
        kinds,
        outputs_by_group,
    }
}

/// The output in `outputs` that's the same file as `stale` — one of last
/// run's outputs this run doesn't generate by that path — if there is one:
/// `Out.txt` last run and `out.txt` this run, say, on a case-insensitive
/// filesystem. Deleting `stale` would then delete that output. Only the
/// outputs whose path could be — ones with the same [`collision_key`] —
/// are checked with the workspace.
///
/// # Errors
/// Returns a display-ready message if the workspace can't tell.
fn same_file_as_output<'a>(
    workspace: &dyn Workspace,
    root: &Path,
    stale: &Path,
    outputs: &'a [PathBuf],
) -> Result<Option<&'a PathBuf>, String> {
    let key = collision_key(stale);
    for output in outputs.iter().filter(|output| collision_key(output) == key) {
        let same = workspace
            .same_file(&root.join(stale), &root.join(output))
            .map_err(|err| {
                format!(
                    "Failed to check whether stale output {} is the same file as {}: {err}",
                    stale.display(),
                    output.display()
                )
            })?;
        if same {
            return Ok(Some(output));
        }
    }
    Ok(None)
}

/// Loads the entries recorded by `.crank.manifest` at `path`, grouped by
/// task group name. A missing file means there's no prior run to
/// reconcile against, so it's treated the same as an empty manifest rather
/// than an error. One recording a version this crank doesn't read is
/// reported the same as any other malformed manifest.
///
/// # Errors
/// Returns a display-ready message if `path` exists but isn't readable, or
/// isn't valid `.crank.manifest` YAML.
fn read_manifest(
    workspace: &dyn Workspace,
    path: &Path,
) -> Result<BTreeMap<String, Vec<ManifestEntry>>, String> {
    let contents = match workspace.read(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => return Err(err.to_string()),
    };

    crate::manifest::parse(&contents).map_err(|err| err.to_string())
}

/// Checks every path `.crank.manifest` records with [`stays_inside`], so
/// a hand-edited or malicious entry like `../../important` is never read
/// for its pockets or deleted as a stale output.
///
/// # Errors
/// Returns a message for every recorded path that leads outside the
/// `.crank` file's directory.
fn check_manifest_paths(
    previous_outputs: &BTreeMap<String, Vec<ManifestEntry>>,
) -> Result<(), Vec<String>> {
    let errors: Vec<String> = previous_outputs
        .values()
        .flatten()
        .filter(|entry| !stays_inside(entry.path()))
        .map(|entry| {
            format!(
                "{MANIFEST_FILE_NAME} records \"{}\", which leads outside the directory holding {CRANK_FILE_NAME} — crank won't read or delete it; remove that entry from {MANIFEST_FILE_NAME}",
                entry.path().display()
            )
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Deletes every output `.crank` generated last time but not this time,
/// then writes `.crank.manifest` back out reflecting the new state —
/// unless it already holds exactly that, in which case it's left alone. Only
/// called once a run has fully succeeded — see the "skip cleanup entirely
/// on any failure" rule in `docs/crank-file-format.md`, which keeps a broken run from
/// deleting a file based on an incomplete picture of "current" outputs.
///
/// A path that's already gone counts as cleaned up — it's dropped from the
/// manifest without an error — but isn't reported as removed, since this
/// run didn't remove it. So does one that's the same file as one of this
/// run's outputs (see [`same_file_as_output`]), which is left alone. A path that fails to delete stays recorded in the
/// new manifest, under
/// the task group name it had in `previous_outputs` — the only place that
/// still knows which group produced it, since this run doesn't produce it
/// at all (that's why it's stale) — so the next fully successful run
/// retries it instead of losing track of it.
///
/// `outputs` is every output this run generated, with what generating it
/// did to the file.
fn reconcile_manifest(
    workspace: &mut dyn Workspace,
    root: &Path,
    manifest_path: &Path,
    previous_outputs: &BTreeMap<String, Vec<ManifestEntry>>,
    outputs: Vec<(PathBuf, ChangeKind)>,
    outputs_by_group: &BTreeMap<String, Vec<ManifestEntry>>,
) -> Report {
    let previous_flat: Vec<PathBuf> = previous_outputs
        .values()
        .flatten()
        .map(|entry| entry.path().to_path_buf())
        .collect();
    let paths: Vec<PathBuf> = outputs.iter().map(|(path, _)| path.clone()).collect();
    let stale = crate::manifest::stale(&previous_flat, &paths);
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let mut errors = Vec::new();

    for path in &stale {
        match same_file_as_output(workspace, root, path, &paths) {
            // Still generated, by a path that differs only in how it's
            // written, so it isn't stale: dropped from the manifest, not
            // deleted.
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(err) => {
                errors.push(err);
                failed.push(path);
                continue;
            }
        }
        match workspace.remove(&root.join(path)) {
            Ok(()) => removed.push(path.clone()),
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                errors.push(format!(
                    "Failed to remove stale output {}: {err}",
                    path.display()
                ));
                failed.push(path);
            }
        }
    }

    // A retried deletion carries its original manifest entry forward
    // unchanged (not just its bare path), so a stale output's persisted
    // `pocketMarkers` (if it had any) survive until the deletion finally
    // succeeds.
    let mut new_manifest = outputs_by_group.clone();
    for path in failed {
        let (owning_group, entry) = previous_outputs
            .iter()
            .find_map(|(name, entries)| {
                entries
                    .iter()
                    .find(|entry| entry.path() == path.as_path())
                    .map(|entry| (name.clone(), entry.clone()))
            })
            .expect("a stale path always came from some group in the previous manifest");
        new_manifest.entry(owning_group).or_default().push(entry);
    }

    // Left alone when it already says exactly this, like an unchanged
    // output, so its modification time only moves when it changes.
    let serialized = crate::manifest::serialize(&new_manifest);
    let unchanged = workspace
        .read(manifest_path)
        .is_ok_and(|existing| existing == serialized);
    if !unchanged && let Err(err) = workspace.write(manifest_path, &serialized) {
        errors.push(format!(
            "Failed to write {}: {err}",
            manifest_path.display()
        ));
    }

    if errors.is_empty() {
        Report::Generated { outputs, removed }
    } else {
        Report::Failed(errors)
    }
}

/// `--check`'s counterpart to [`reconcile_manifest`]: computes the same
/// stale-path diff but never changes the workspace — no output is deleted,
/// `.crank.manifest` is never written. A stale path with no file there is
/// left out, since a normal run wouldn't report removing it either, as is
/// one that's the same file as one of this run's outputs.
/// `kinds` is what writing each output would do; the unchanged ones are
/// left out of the report's `changes`.
fn check_reconcile(
    workspace: &dyn Workspace,
    root: &Path,
    previous_outputs: &BTreeMap<String, Vec<ManifestEntry>>,
    outputs: &[PathBuf],
    kinds: Vec<(PathBuf, ChangeKind)>,
) -> Report {
    let changes = kinds
        .into_iter()
        .filter(|(_, kind)| *kind != ChangeKind::Unchanged)
        .collect();
    let previous_flat: Vec<PathBuf> = previous_outputs
        .values()
        .flatten()
        .map(|entry| entry.path().to_path_buf())
        .collect();
    let mut stale = Vec::new();
    let mut errors = Vec::new();
    for path in crate::manifest::stale(&previous_flat, outputs) {
        match same_file_as_output(workspace, root, &path, outputs) {
            Ok(Some(_)) => {}
            Ok(None) => {
                if !matches!(
                    workspace.read(&root.join(&path)),
                    Err(err) if err.kind() == ErrorKind::NotFound
                ) {
                    stale.push(path);
                }
            }
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Report::Checked { changes, stale }
    } else {
        Report::Failed(errors)
    }
}

/// Whether `name` is an identifier as Tera's template syntax spells one —
/// ASCII-only, unlike a selector key or a pocket name — and so can be
/// called as a filter (`{{ value | name }}`).
fn is_tera_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Merges crank's built-in filters with every filter declared in the
/// `.crank` file's `filters` key, reading and compiling each one's `.rhai`
/// script. Returns every accumulated problem (a name colliding with a
/// built-in or that a template couldn't call, an unreadable script, a
/// script that fails to compile) instead
/// of stopping at the first one, since filters apply to the whole run and
/// there's no reason to hide a second broken filter behind the first.
///
/// # Errors
/// Returns every accumulated error message if any filter is invalid.
fn load_filters(
    workspace: &dyn Workspace,
    root: &Path,
    declared: &HashMap<String, PathBuf>,
) -> Result<HashMap<String, CompiledFilter>, Vec<String>> {
    let mut filters = crate::rhai_filter::built_in_filters();
    let mut errors = Vec::new();

    for (name, script_path) in declared {
        if filters.contains_key(name) {
            errors.push(format!("Filter `{name}` collides with a built-in filter"));
            continue;
        }
        if !is_tera_identifier(name) {
            errors.push(format!(
                "Filter `{name}` can't be called from a template: a filter name must start with an ASCII letter or `_`, followed by ASCII letters, digits, or `_`"
            ));
            continue;
        }

        let full_path = root.join(script_path);
        let display = full_path.display().to_string();

        let source = match workspace.read(&full_path) {
            Ok(source) => source,
            Err(err) => {
                errors.push(format!("Failed to read filter script {display}: {err}"));
                continue;
            }
        };

        match CompiledFilter::compile(&source) {
            Ok(compiled) => {
                filters.insert(name.clone(), compiled);
            }
            Err(err) => {
                errors.push(format!("Failed to compile filter script {display}: {err}"));
            }
        }
    }

    if errors.is_empty() {
        Ok(filters)
    } else {
        Err(errors)
    }
}

/// Reads every output `previous_outputs` recorded for `group_name` last
/// run (a missing file is fine — nothing to scan) and scans it for
/// pocket markers, merging every file's pockets into one group-wide map
/// — so a pocket can be found regardless of which of the group's
/// previous files it lived in. This has to run for every previous output
/// up front, before this run writes any of the group's new outputs,
/// since a pocket that moved to a different file between runs must still
/// be found — reading it after a new write could find the new content
/// instead of the old.
///
/// Each previous file is scanned with whichever pocket markers it was
/// actually written with — its manifest entry's recorded
/// `pocketMarkers`, if it has any, else `default_markers` for a plain
/// entry — rather than what its template lists now. A file's markers
/// depend on what generated it last time, not on what the *current*
/// template says (which may have since changed, or no longer even be
/// used).
///
/// Returns `None` (having already pushed a message to `errors` for every
/// problem) if any previous output couldn't be read, had recorded markers
/// that can't be used, or had a malformed marker — the
/// caller must skip the whole group in that case, same as an unreadable
/// `object`/`array` data file, since proceeding without a complete picture
/// of the group's old pockets risks silently discarding hand-written
/// content.
fn load_old_pockets(
    workspace: &dyn Workspace,
    root: &Path,
    group_name: &str,
    previous_outputs: &BTreeMap<String, Vec<ManifestEntry>>,
    default_markers: &PocketMarkers,
    errors: &mut Vec<String>,
) -> Option<OldPockets> {
    let previous_entries: &[ManifestEntry] =
        previous_outputs.get(group_name).map_or(&[], Vec::as_slice);

    let mut previous_files: Vec<(PathBuf, String, PocketMarkers)> = Vec::new();
    let mut read_errors = Vec::new();
    for entry in previous_entries {
        match workspace.read(&root.join(entry.path())) {
            Ok(contents) => {
                let markers = match entry {
                    ManifestEntry::Plain(_) => Ok(default_markers.clone()),
                    ManifestEntry::WithPocketMarkers { pocket_markers, .. } => {
                        PocketMarkers::compile(pocket_markers)
                    }
                };
                match markers {
                    Ok(markers) => {
                        previous_files.push((entry.path().to_path_buf(), contents, markers));
                    }
                    Err(err) => read_errors.push(format!(
                        "Failed to use the pocket markers {MANIFEST_FILE_NAME} records for {}: {err}",
                        entry.path().display()
                    )),
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => read_errors.push(format!(
                "Failed to read previous output {} while scanning for pockets: {err}",
                entry.path().display()
            )),
        }
    }
    if !read_errors.is_empty() {
        errors.extend(read_errors);
        return None;
    }

    let files: Vec<(PathBuf, String, &PocketMarkers)> = previous_files
        .iter()
        .map(|(path, contents, markers)| (path.clone(), contents.clone(), markers))
        .collect();

    match crate::pocket::collect_old_pockets(&files) {
        Ok(old_pockets) => Some(old_pockets),
        Err(scan_errors) => {
            for (path, err) in scan_errors {
                errors.push(format!(
                    "Failed to scan previous output {} for pockets: {err}",
                    path.display()
                ));
            }
            None
        }
    }
}

/// Everything a task needs to render and finalize its output that stays
/// constant across every task/item in one task group — bundled into one
/// value passed by reference, so [`run_task`]/[`run_array_task`] don't
/// take an ever-growing list of individually-passed arguments.
struct TaskTools<'a> {
    /// The `.crank` file's directory, which every path is relative to.
    root: &'a Path,
    /// Every custom filter, compiled, by name.
    filters: &'a HashMap<String, CompiledFilter>,
    /// The pockets found in the group's previous outputs.
    old_pockets: &'a OldPockets,
    /// crank's built-in pocket markers, compiled, for every template
    /// without its own when `.crank` lists none either.
    default_markers: &'a PocketMarkers,
    /// The `.crank` file's `pocketMarkers`, if it lists any, for every
    /// template without its own.
    project_markers: Option<&'a ListedMarkers>,
    /// The group's name, to say which task generated an output.
    group: &'a str,
}

/// Everything about a run that stays constant across every task group —
/// bundled the same way [`TaskTools`] bundles per-group state, so
/// [`run_task_group`] doesn't take an ever-growing argument list as this
/// grows.
struct RunConfig<'a> {
    /// Every custom filter, compiled, by name.
    filters: &'a HashMap<String, CompiledFilter>,
    /// `.crank.manifest`'s record of each group's previous outputs.
    previous_outputs: &'a BTreeMap<String, Vec<ManifestEntry>>,
    /// crank's built-in pocket markers, compiled.
    default_markers: &'a PocketMarkers,
    /// The `.crank` file's `pocketMarkers`, if it lists any.
    project_markers: Option<&'a ListedMarkers>,
}

/// What running a task group's tasks adds to: every output rendered,
/// ready to write, and every error.
struct Accumulators<'a> {
    /// Every output rendered, ready to write.
    pending: &'a mut Vec<PendingOutput>,
    /// Every error's message.
    errors: &'a mut Vec<String>,
}

/// Loads `group`'s data file and runs every task listed under it, appending
/// each successfully rendered output to `acc.pending` and every
/// failure's message to `acc.errors`. A failure loading the data file
/// itself skips all of the group's tasks. For an `object` group, a failure
/// in one task doesn't stop the rest from running; for an `array` group, a
/// failure in one item (an item that isn't a mapping, or a render failure)
/// only skips that item, not the rest of the array or the group's other tasks — so one
/// broken group/task/item never hides problems elsewhere in the `.crank`
/// file.
fn run_task_group(
    workspace: &dyn Workspace,
    root: &Path,
    group: &TaskGroup,
    run_config: &RunConfig<'_>,
    acc: &mut Accumulators<'_>,
) {
    let Some(old_pockets) = load_old_pockets(
        workspace,
        root,
        &group.name,
        run_config.previous_outputs,
        run_config.default_markers,
        acc.errors,
    ) else {
        return;
    };
    let tools = TaskTools {
        root,
        filters: run_config.filters,
        old_pockets: &old_pockets,
        default_markers: run_config.default_markers,
        project_markers: run_config.project_markers,
        group: &group.name,
    };
    let mut seen_pocket_names: HashSet<String> = HashSet::new();

    let object_source = group.source.object_source();
    let object_path = root.join(&object_source.path);
    let object_display = object_path.display().to_string();

    let object_contents = match workspace.read(&object_path) {
        Ok(contents) => contents,
        Err(err) => {
            acc.errors
                .push(format!("Failed to read {object_display}: {err}"));
            return;
        }
    };

    let value = match crate::data::parse(&object_contents) {
        Ok(value) => value,
        Err(err) => {
            acc.errors
                .push(format!("Failed to parse {object_display}: {err}"));
            return;
        }
    };

    match &group.source {
        Source::Object(_) => {
            run_object_group(
                workspace,
                group,
                &value,
                &tools,
                &mut seen_pocket_names,
                acc,
            );
        }
        Source::Array(_) => {
            run_array_group(
                workspace,
                group,
                &value,
                &tools,
                &mut seen_pocket_names,
                acc,
            );
        }
    }
}

/// Runs an `object` task group's tasks once, against `value` (narrowed by
/// the group's selector, if it has one) as the shared template context —
/// the `Source::Object` half of [`run_task_group`]'s match, split out to
/// keep both halves under clippy's function-length lint.
fn run_object_group(
    workspace: &dyn Workspace,
    group: &TaskGroup,
    value: &serde_yaml_ng::Value,
    tools: &TaskTools<'_>,
    seen_pocket_names: &mut HashSet<String>,
    acc: &mut Accumulators<'_>,
) {
    let object_source = group.source.object_source();
    let object_display = tools.root.join(&object_source.path).display().to_string();

    let selected = match &object_source.selector {
        Some(selector) => match crate::selector::select_object(value, selector) {
            Ok(selected) => selected,
            Err(err) => {
                acc.errors
                    .push(format!("Failed to select from {object_display}: {err}"));
                return;
            }
        },
        None => value,
    };

    let context = match crate::data::to_context(selected) {
        Ok(context) => context,
        Err(err) => {
            acc.errors.push(format!(
                "Failed to use {object_display} as template data: {err}"
            ));
            return;
        }
    };

    for task in &group.tasks {
        run_task(workspace, task, &context, tools, seen_pocket_names, acc);
    }
}

/// Runs an `array` task group's tasks once per item in `value` (narrowed
/// by the group's selector, if it has one) — the `Source::Array` half of
/// [`run_task_group`]'s match, split out to keep both halves under
/// clippy's function-length lint.
fn run_array_group(
    workspace: &dyn Workspace,
    group: &TaskGroup,
    value: &serde_yaml_ng::Value,
    tools: &TaskTools<'_>,
    seen_pocket_names: &mut HashSet<String>,
    acc: &mut Accumulators<'_>,
) {
    let object_source = group.source.object_source();
    let object_display = tools.root.join(&object_source.path).display().to_string();

    let items = match &object_source.selector {
        Some(selector) => match crate::selector::select_array(value, selector) {
            Ok(items) => items,
            Err(err) => {
                acc.errors
                    .push(format!("Failed to select from {object_display}: {err}"));
                return;
            }
        },
        None => match crate::data::to_items(value) {
            Ok(items) => items,
            Err(err) => {
                acc.errors.push(format!(
                    "Failed to use {object_display} as an array of items: {err}"
                ));
                return;
            }
        },
    };

    for (item_index, item) in items.iter().enumerate() {
        let context = match crate::data::to_context(item) {
            Ok(context) => context,
            Err(err) => {
                acc.errors.push(format!(
                    "Failed to use {object_display} item [{item_index}] as template data: {err}"
                ));
                continue;
            }
        };

        for task in &group.tasks {
            run_array_task(
                workspace,
                task,
                &context,
                tools,
                item_index,
                seen_pocket_names,
                acc,
            );
        }
    }
}

/// Pocket markers listed somewhere other than crank's built-in ones — in
/// a template's header, or in `.crank` — both as written (for
/// `.crank.manifest`) and compiled.
type ListedMarkers = (Vec<MarkerPattern>, PocketMarkers);

/// A task's template, rendered: its text, where it goes, and the pocket
/// markers its header lists, if any.
struct Rendered {
    /// The rendered template body.
    text: String,
    /// The rendered `output` path.
    output_path: PathBuf,
    /// The template's own `pocketMarkers`, or `None` for crank's built-in
    /// ones.
    markers: Option<ListedMarkers>,
}

/// The pocket markers `template_source`'s header lists, if any, both as
/// written and compiled — reported against `template_display` if the
/// header can't be read or a marker can't be used.
fn template_markers(
    template_source: &str,
    template_display: &str,
) -> Result<Option<ListedMarkers>, String> {
    let invalid =
        |err: &dyn std::fmt::Display| format!("Invalid crank header in {template_display}: {err}");
    let header = crate::header::parse(template_source).map_err(|err| invalid(&err))?;
    let compiled = header.compiled_markers().map_err(|err| invalid(&err))?;
    Ok(header.pocket_markers.zip(compiled))
}

/// Renders one task's `output` path (itself a Tera template) and then its
/// template body, against the same context plus the reserved `crank`
/// variable (see [`crate::data::with_crank`]), without writing anything
/// — shared by [`run_task`] and [`run_array_task`], which only differ in
/// what happens with a successful render. Fails if the template's crank
/// header can't be used, or if the rendered path leads outside the
/// `.crank` file's directory (see [`stays_inside`]), which the data alone
/// can make it do.
fn render_task(
    workspace: &dyn Workspace,
    root: &Path,
    task: &Task,
    context: &tera::Context,
    filters: &HashMap<String, CompiledFilter>,
) -> Result<Rendered, String> {
    let template_path = root.join(&task.template);
    let template_display = template_path.display().to_string();

    let template_source = workspace
        .read(&template_path)
        .map_err(|err| format!("Failed to read {template_display}: {err}"))?;
    let markers = template_markers(&template_source, &template_display)?;

    let output_text = task.output.to_string_lossy();
    let rendered_output = crate::render::render(&output_text, context, filters)
        .map_err(|err| format!("Failed to render output path \"{output_text}\": {err}"))?;
    let output_path = PathBuf::from(rendered_output);
    if !stays_inside(&output_path) {
        return Err(format!(
            "Output path \"{}\" leads outside the directory holding {CRANK_FILE_NAME} — not written",
            output_path.display()
        ));
    }

    // The body is rendered after the path, since `crank.filename` comes
    // from it.
    let body_context = crate::data::with_crank(context, &output_path);
    let rendered = crate::render::render(&template_source, &body_context, filters)
        .map_err(|err| format!("Failed to render {template_display}: {err}"))?;

    Ok(Rendered {
        text: rendered,
        output_path,
        markers,
    })
}

/// Writes `rendered` to `output_path` (relative to `root`), creating its
/// parent directory if needed and replacing any file there — unless that
/// file already holds exactly `rendered`, in which case it's left alone
/// (keeping its modification time), or `check` is set, in which case
/// nothing is written at all. Either way, returns what the write did (or
/// would do) to the file, as classified by [`crate::check::classify`].
fn write_output(
    workspace: &mut dyn Workspace,
    root: &Path,
    output_path: &Path,
    rendered: &str,
    check: bool,
) -> Result<ChangeKind, String> {
    let full_path = root.join(output_path);
    let display = output_path.display().to_string();

    let kind = match workspace.read(&full_path) {
        Ok(existing) => crate::check::classify(Some(&existing), rendered),
        Err(err) if err.kind() == ErrorKind::NotFound => crate::check::classify(None, rendered),
        Err(err) if check => return Err(format!("Failed to read {display}: {err}")),
        // A file that's there but can't be read (or isn't UTF-8) can't
        // already hold `rendered`, so it's overwritten as before.
        Err(_) => ChangeKind::Modified,
    };
    if check || kind == ChangeKind::Unchanged {
        return Ok(kind);
    }

    if let Some(parent) = full_path.parent()
        && let Err(err) = workspace.create_dir_all(parent)
    {
        return Err(format!("Failed to create directory for {display}: {err}"));
    }

    workspace
        .write(&full_path, rendered)
        .map_err(|err| format!("Failed to write {display}: {err}"))?;
    Ok(kind)
}

/// Applies pocket-marker substitution to `rendered` (one task's freshly
/// rendered output, before it's written to `output_path`) — a malformed
/// marker, a pocket name reused elsewhere in the group, or a name found
/// in more than one previous output file is reported against
/// `output_path` and fails just this task/item, exactly like a template
/// render failure.
fn apply_pockets(
    output_path: &Path,
    rendered: &str,
    old_pockets: &OldPockets,
    markers: &PocketMarkers,
    seen_pocket_names: &mut HashSet<String>,
) -> Result<String, String> {
    crate::pocket::apply(rendered, old_pockets, seen_pocket_names, markers).map_err(|err| {
        format!(
            "Failed to apply pocket markers to {}: {err}",
            output_path.display()
        )
    })
}

/// Builds the `.crank.manifest` entry to record for a successfully written
/// `output_path`: just the path if it was written with crank's built-in
/// pocket markers (the common case), else the path plus the markers it
/// was written with — its template's, or `.crank`'s — so a later run's
/// old-pocket scan can recognize this exact file's markers regardless of
/// what its template or `.crank` says by then — see `load_old_pockets`.
fn manifest_entry(output_path: PathBuf, patterns: Option<&Vec<MarkerPattern>>) -> ManifestEntry {
    match patterns {
        None => ManifestEntry::Plain(output_path),
        Some(patterns) => ManifestEntry::WithPocketMarkers {
            path: output_path,
            pocket_markers: patterns.clone(),
        },
    }
}

/// Applies pockets to one task's fresh render and adds it to
/// `acc.pending`, to write once every task has run — the part of
/// [`run_task`] and [`run_array_task`] that's the same for both once a
/// render succeeded. `origin` says which task it was, for
/// [`PendingOutput::origin`]. Returns the error message for a failure, for
/// the caller to prefix.
fn finish_task(
    rendered: Rendered,
    origin: String,
    tools: &TaskTools<'_>,
    seen_pocket_names: &mut HashSet<String>,
    acc: &mut Accumulators<'_>,
) -> Result<(), String> {
    let Rendered {
        text,
        output_path,
        markers,
    } = rendered;
    // A template's own markers win over `.crank`'s, which win over the
    // built-in ones.
    let listed = markers.as_ref().or(tools.project_markers);
    let rendered = apply_pockets(
        &output_path,
        &text,
        tools.old_pockets,
        listed.map_or(tools.default_markers, |(_, compiled)| compiled),
        seen_pocket_names,
    )?;
    acc.pending.push(PendingOutput {
        origin,
        text: rendered,
        entry: manifest_entry(output_path, listed.map(|(patterns, _)| patterns)),
    });
    Ok(())
}

/// Runs one `object`-group task: renders its template and `output` path
/// and applies any pocket markers, ready to write.
fn run_task(
    workspace: &dyn Workspace,
    task: &Task,
    context: &tera::Context,
    tools: &TaskTools<'_>,
    seen_pocket_names: &mut HashSet<String>,
    acc: &mut Accumulators<'_>,
) {
    let origin = format!(
        "task group \"{}\" (template {})",
        tools.group,
        task.template.display()
    );
    let result = render_task(workspace, tools.root, task, context, tools.filters)
        .and_then(|rendered| finish_task(rendered, origin, tools, seen_pocket_names, acc));
    if let Err(err) = result {
        acc.errors.push(err);
    }
}

/// Runs one `array`-group task for a single item, the one at
/// `item_index`: renders its template and `output` path against that
/// item's context and applies any pocket markers, ready to write.
fn run_array_task(
    workspace: &dyn Workspace,
    task: &Task,
    context: &tera::Context,
    tools: &TaskTools<'_>,
    item_index: usize,
    seen_pocket_names: &mut HashSet<String>,
    acc: &mut Accumulators<'_>,
) {
    let origin = format!(
        "task group \"{}\" item [{item_index}] (template {})",
        tools.group,
        task.template.display()
    );
    let result = render_task(workspace, tools.root, task, context, tools.filters)
        .and_then(|rendered| finish_task(rendered, origin, tools, seen_pocket_names, acc));
    if let Err(err) = result {
        acc.errors.push(format!("Item [{item_index}]: {err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace holding `files`, each `(path, contents)`.
    fn workspace(files: &[(&str, &str)]) -> MemoryWorkspace {
        let mut workspace = MemoryWorkspace::new();
        for (path, contents) in files {
            workspace.insert(path, *contents);
        }
        workspace
    }

    const CRANK: &str = "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: output.txt.tera\n        output: out/{{ name }}.txt\n";

    #[test]
    fn no_crank_file_is_reported_and_succeeds() {
        let report = run(&mut MemoryWorkspace::new(), Path::new(""), false);
        assert_eq!(report, Report::NoCrankFile);
        assert!(report.success());
    }

    #[test]
    fn a_run_writes_its_outputs_and_the_manifest() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Created)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "Hello widget"
        );
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n"
        );
    }

    #[test]
    fn a_stale_output_is_removed() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - old.txt\n",
            ),
            ("old.txt", "old"),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Created)],
                removed: vec![PathBuf::from("old.txt")],
            }
        );
        assert!(ws.read(Path::new("old.txt")).is_err());
    }

    #[test]
    fn a_stale_output_already_gone_is_dropped_but_not_reported_as_removed() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - gone.txt\n  - old.txt\n",
            ),
            ("old.txt", "old"),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Created)],
                removed: vec![PathBuf::from("old.txt")],
            }
        );
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n"
        );
    }

    /// A [`MemoryWorkspace`] that records every path written to, and can
    /// fail to read one path with something other than
    /// [`ErrorKind::NotFound`].
    struct RecordingWorkspace {
        inner: MemoryWorkspace,
        written: Vec<PathBuf>,
        unreadable: Option<PathBuf>,
    }

    impl Workspace for RecordingWorkspace {
        fn read(&self, path: &Path) -> io::Result<String> {
            if self.unreadable.as_deref() == Some(path) {
                return Err(io::Error::new(ErrorKind::InvalidData, "not UTF-8"));
            }
            self.inner.read(path)
        }

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.inner.create_dir_all(path)
        }

        fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
            self.written.push(path.to_path_buf());
            self.inner.write(path, contents)
        }

        fn remove(&mut self, path: &Path) -> io::Result<()> {
            self.inner.remove(path)
        }

        fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
            self.inner.same_file(a, b)
        }
    }

    fn recording(files: &[(&str, &str)]) -> RecordingWorkspace {
        RecordingWorkspace {
            inner: workspace(files),
            written: Vec::new(),
            unreadable: None,
        }
    }

    #[test]
    fn an_output_already_holding_what_is_generated_is_not_rewritten() {
        let mut ws = recording(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
            ("out/widget.txt", "Hello widget"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Unchanged)],
                removed: vec![],
            }
        );
        assert_eq!(ws.written, [PathBuf::from(".crank.manifest")]);
    }

    #[test]
    fn a_rerun_with_nothing_to_change_writes_nothing() {
        let mut ws = recording(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n",
            ),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
            ("out/widget.txt", "Hello widget"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert!(ws.written.is_empty(), "{:?}", ws.written);
    }

    #[test]
    fn a_manifest_that_would_change_is_rewritten() {
        let mut ws = recording(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - out/old.txt\n",
            ),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
            ("out/widget.txt", "Hello widget"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(ws.written, [PathBuf::from(".crank.manifest")]);
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n"
        );
    }

    #[test]
    fn an_output_holding_something_else_is_rewritten() {
        let mut ws = recording(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
            ("out/widget.txt", "Hello old"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Modified)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.written,
            [
                PathBuf::from("out/widget.txt"),
                PathBuf::from(".crank.manifest")
            ]
        );
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "Hello widget"
        );
    }

    #[test]
    fn an_output_that_cannot_be_read_is_rewritten() {
        let mut ws = recording(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
            ("out/widget.txt", "Hello widget"),
        ]);
        ws.unreadable = Some(PathBuf::from("out/widget.txt"));
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Modified)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.written,
            [
                PathBuf::from("out/widget.txt"),
                PathBuf::from(".crank.manifest")
            ]
        );
    }

    /// A [`MemoryWorkspace`] whose every deletion fails with something
    /// other than [`ErrorKind::NotFound`].
    struct UndeletableWorkspace(MemoryWorkspace);

    impl Workspace for UndeletableWorkspace {
        fn read(&self, path: &Path) -> io::Result<String> {
            self.0.read(path)
        }

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.0.create_dir_all(path)
        }

        fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
            self.0.write(path, contents)
        }

        fn remove(&mut self, _path: &Path) -> io::Result<()> {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Permission denied",
            ))
        }

        fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
            self.0.same_file(a, b)
        }
    }

    #[test]
    fn a_stale_output_that_fails_to_delete_stays_in_the_manifest() {
        let mut ws = UndeletableWorkspace(workspace(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - old.txt\n",
            ),
            ("old.txt", "old"),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello"),
        ]));
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Failed(vec![
                "Failed to remove stale output old.txt: Permission denied".to_string()
            ])
        );
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - old.txt\n  - out/widget.txt\n"
        );
    }

    #[test]
    fn check_leaves_out_a_stale_output_already_gone() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - gone.txt\n  - old.txt\n  - out/widget.txt\n",
            ),
            ("old.txt", "old"),
            ("out/widget.txt", "Hello"),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello"),
        ]);
        let report = run(&mut ws, Path::new(""), true);
        assert_eq!(
            report,
            Report::Checked {
                changes: vec![],
                stale: vec![PathBuf::from("old.txt")],
            }
        );
    }

    #[test]
    fn check_writes_nothing_and_fails_when_something_would_change() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "Hello"),
        ]);
        let before = ws.clone();
        let report = run(&mut ws, Path::new(""), true);
        assert_eq!(
            report,
            Report::Checked {
                changes: vec![(PathBuf::from("out/widget.txt"), ChangeKind::Created)],
                stale: vec![],
            }
        );
        assert!(!report.success());
        assert_eq!(ws, before);
    }

    #[test]
    fn errors_are_collected_while_other_tasks_still_run() {
        let mut ws = workspace(&[
            (
                ".crank",
                "taskGroups:\n  - name: a\n    object: missing.yaml\n    tasks:\n      - template: t.tera\n        output: a.txt\n  - name: b\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: b.txt\n",
            ),
            ("object.yaml", "name: widget\n"),
            ("t.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            matches!(&report, Report::Failed(errors) if errors.len() == 1 && errors[0].starts_with("Failed to read missing.yaml"))
        );
        // The independent group still ran, but nothing was reconciled.
        assert_eq!(ws.read(Path::new("b.txt")).unwrap(), "widget");
        assert!(ws.read(Path::new(".crank.manifest")).is_err());
    }

    #[test]
    fn paths_are_joined_onto_the_root() {
        let mut ws = workspace(&[
            ("project/.crank", CRANK),
            ("project/object.yaml", "name: widget\n"),
            ("project/output.txt.tera", "Hi"),
        ]);
        let report = run(&mut ws, Path::new("project"), false);
        assert!(report.success());
        assert_eq!(ws.read(Path::new("project/out/widget.txt")).unwrap(), "Hi");
    }

    #[test]
    fn memory_workspace_paths_are_normalized() {
        let mut ws = workspace(&[("./a/../b.txt", "b")]);
        assert_eq!(ws.read(Path::new("b.txt")).unwrap(), "b");
        ws.remove(Path::new("./b.txt")).unwrap();
        assert_eq!(
            ws.read(Path::new("b.txt")).unwrap_err().kind(),
            ErrorKind::NotFound
        );
    }

    // -- Non-ASCII text everywhere --------------------------------------
    //
    // File and directory names, task group names, data, templates, and
    // pocket content in a range of scripts (Japanese, Russian, Korean,
    // Polish, French), plus emoji — including multi-code-point ones (a
    // ZWJ sequence, a flag, a skin-tone modifier) and combining marks.

    /// Every flavor of non-ASCII text the tests below throw at crank.
    const SAMPLES: &[&str] = &[
        "日本語のテキスト",
        "Привет, мир",
        "안녕하세요 세계",
        "Zażółć gęślą jaźń",
        "Crème brûlée à la française",
        "🍕🔥",
        "👩‍👩‍👧‍👦 🇵🇱 👍🏽",
        "e\u{301}cole", // "école" with a combining acute accent
        "שלום עולם",    // right-to-left
    ];

    /// `text` as a single-quoted YAML scalar.
    fn yaml_quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    fn generated(report: &Report) -> Vec<PathBuf> {
        match report {
            Report::Generated { outputs, .. } => {
                outputs.iter().map(|(path, _)| path.clone()).collect()
            }
            other => panic!("expected a successful run, got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_file_and_directory_names_work_for_every_referenced_file() {
        // (The filter's *name* stays ASCII: a Tera filter name has to be
        // a Tera identifier — see `render.rs`'s tests.)
        let crank = "\
taskGroups:
  - name: メニュー
    object: データ/ピザ.yaml
    tasks:
      - template: шаблоны/меню.txt.tera
        output: 출력/menü 🍕.txt
filters:
  odwroc: filtry/odwróć.rhai
";
        let mut ws = workspace(&[
            (".crank", crank),
            ("データ/ピザ.yaml", "name: Margherita\n"),
            ("шаблоны/меню.txt.tera", "{{ name | odwroc }}"),
            (
                "filtry/odwróć.rhai",
                "fn filter(value) { let r = \"\"; for c in value { r = c.to_string() + r; } r }",
            ),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(generated(&report), [PathBuf::from("출력/menü 🍕.txt")]);
        assert_eq!(
            ws.read(Path::new("출력/menü 🍕.txt")).unwrap(),
            "atirehgraM"
        );
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  メニュー:\n  - 출력/menü 🍕.txt\n"
        );
    }

    #[test]
    fn non_ascii_data_renders_verbatim() {
        for sample in SAMPLES {
            let data = format!("name: {}\n", yaml_quoted(sample));
            let mut ws = workspace(&[
                (".crank", CRANK),
                ("object.yaml", &data),
                ("output.txt.tera", "«{{ name }}» — {{ name | length }}"),
            ]);
            let report = run(&mut ws, Path::new(""), false);
            let expected_path = PathBuf::from(format!("out/{sample}.txt"));
            assert_eq!(
                generated(&report),
                std::slice::from_ref(&expected_path),
                "{sample}"
            );
            let rendered = ws.read(&expected_path).unwrap();
            assert!(
                rendered.starts_with(&format!("«{sample}» — ")),
                "{rendered}"
            );
        }
    }

    #[test]
    fn non_ascii_data_keys_are_reachable_through_a_loop_and_bracket_lookups() {
        // Tera identifiers are ASCII-only, so `{{ 名前 }}` can't name a key
        // directly — but a key in any script is still reachable.
        let mut ws = workspace(&[
            (".crank", CRANK),
            (
                "object.yaml",
                "name: widget\nラベル:\n  名前: ピザ\n  цена: 12\n",
            ),
            (
                "output.txt.tera",
                "{% for key, value in __tera_context %}{% if key != \"name\" %}{% for k, v in value %}{{ k }}={{ v }};{% endfor %}{% endif %}{% endfor %}",
            ),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(report.success(), "{report:?}");
        let rendered = ws.read(Path::new("out/widget.txt")).unwrap();
        assert!(rendered.contains("名前=ピザ;"), "{rendered}");
        assert!(rendered.contains("цена=12;"), "{rendered}");
    }

    #[test]
    fn an_array_group_fans_out_to_non_ascii_output_paths() {
        let crank = "\
taskGroups:
  - name: 料理
    array: menu.yaml#dishes
    tasks:
      - template: dish.tera
        output: \"dania/{{ name }}.txt\"
";
        let mut items = String::from("dishes:\n");
        items.extend(
            SAMPLES
                .iter()
                .map(|sample| format!("  - name: {}\n", yaml_quoted(sample))),
        );
        let mut ws = workspace(&[
            (".crank", crank),
            ("menu.yaml", &items),
            ("dish.tera", "{{ name | upper }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        let expected: Vec<PathBuf> = SAMPLES
            .iter()
            .map(|sample| PathBuf::from(format!("dania/{sample}.txt")))
            .collect();
        assert_eq!(generated(&report), expected.as_slice());
        assert_eq!(
            ws.read(Path::new("dania/Привет, мир.txt")).unwrap(),
            "ПРИВЕТ, МИР"
        );
        assert_eq!(
            ws.read(Path::new("dania/Zażółć gęślą jaźń.txt")).unwrap(),
            "ZAŻÓŁĆ GĘŚLĄ JAŹŃ"
        );

        // A second run with nothing changed is a no-op under --check, and
        // the manifest it wrote reads back.
        let report = run(&mut ws, Path::new(""), true);
        assert_eq!(
            report,
            Report::Checked {
                changes: vec![],
                stale: vec![],
            }
        );
    }

    #[test]
    fn two_array_items_rendering_the_same_non_ascii_path_collide() {
        let crank = "\
taskGroups:
  - name: dishes
    array: menu.yaml
    tasks:
      - template: dish.tera
        output: \"{{ name }}.txt\"
";
        let mut ws = workspace(&[
            (".crank", crank),
            ("menu.yaml", "- name: 寿司\n- name: 寿司\n"),
            ("dish.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            matches!(&report, Report::Failed(errors) if errors.len() == 1 && errors[0].contains("寿司.txt")),
            "{report:?}"
        );
    }

    #[test]
    fn a_stale_non_ascii_output_is_removed() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  widget:\n  - 古い/ファイル 🗑.txt\n",
            ),
            ("古い/ファイル 🗑.txt", "old"),
            ("object.yaml", "name: новый\n"),
            ("output.txt.tera", "Hello"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out/новый.txt"), ChangeKind::Created)],
                removed: vec![PathBuf::from("古い/ファイル 🗑.txt")],
            }
        );
        assert!(ws.read(Path::new("古い/ファイル 🗑.txt")).is_err());
        assert_eq!(
            ws.read(Path::new(".crank.manifest")).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - out/новый.txt\n"
        );
    }

    #[test]
    fn non_ascii_task_group_names_round_trip_through_the_manifest() {
        // Names that need quoting in YAML (a leading emoji is fine, but a
        // `: ` or a leading `#`/`&`/`*`/`!` isn't) still round-trip.
        let names = [
            "🍕",
            "ピザ: 特製",
            "# заметки",
            "&ancre",
            "*étoile",
            "!wykrzyknik",
            "한국어",
        ];
        let mut crank = String::from("taskGroups:\n");
        crank.extend(names.iter().enumerate().map(|(index, name)| {
            format!(
                "  - name: {}\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: out-{index}.txt\n",
                yaml_quoted(name)
            )
        }));
        let mut ws = workspace(&[
            (".crank", &crank),
            ("object.yaml", "name: x\n"),
            ("t.tera", "{{ name }}"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        // Rerunning reads the manifest back: nothing is stale.
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            matches!(&report, Report::Generated { removed, .. } if removed.is_empty()),
            "{report:?}"
        );
        let manifest =
            crate::manifest::parse(&ws.read(Path::new(".crank.manifest")).unwrap()).unwrap();
        let mut recorded: Vec<&str> = manifest.keys().map(String::as_str).collect();
        recorded.sort_unstable();
        let mut expected = names.to_vec();
        expected.sort_unstable();
        assert_eq!(recorded, expected);
    }

    #[test]
    fn hand_edited_non_ascii_pocket_content_survives_a_rerun() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            (
                "output.txt.tera",
                "ヘッダー {{ name }}\n// notes [\nデフォルト\n// ]\nend: /* inline [ */ 🍕 /* ] */ ✓\n",
            ),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        ws.insert(
            "out/widget.txt",
            "ヘッダー widget\n// notes [\nZażółć 👩‍👩‍👧‍👦 e\u{301}cole\nПривет\n// ]\nend: /* inline [ */ 寿司🍣 /* ] */ ✓\n",
        );
        ws.insert("output.txt.tera", "Заголовок {{ name }}\n// notes [\nデフォルト\n// ]\nend: /* inline [ */ 🍕 /* ] */ ✓\n");
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "Заголовок widget\n// notes [\nZażółć 👩‍👩‍👧‍👦 e\u{301}cole\nПривет\n// ]\nend: /* inline [ */ 寿司🍣 /* ] */ ✓\n"
        );
    }

    #[test]
    fn a_pocket_named_after_non_ascii_data_survives_a_rerun() {
        // The documented `// custom-{{ name }} [` pattern, with names
        // that aren't ASCII.
        let crank = "\
taskGroups:
  - name: dishes
    array: menu.yaml
    tasks:
      - template: dish.tera
        output: \"{{ id }}.txt\"
";
        let mut ws = workspace(&[
            (".crank", crank),
            (
                "menu.yaml",
                "- {id: a, name: 寿司}\n- {id: b, name: Żurek}\n- {id: c, name: Crème_brûlée}\n- {id: d, name: борщ}\n- {id: e, name: 김치}\n- {id: f, name: e\u{301}clair}\n",
            ),
            ("dish.tera", "// notes-{{ name }} [\ndefault\n// ]\n"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        for (id, name) in [
            ("a", "寿司"),
            ("b", "Żurek"),
            ("c", "Crème_brûlée"),
            ("d", "борщ"),
            ("e", "김치"),
            ("f", "e\u{301}clair"),
        ] {
            ws.insert(
                format!("{id}.txt"),
                format!("// notes-{name} [\nedited {name}\n// ]\n"),
            );
        }
        let report = run(&mut ws, Path::new(""), false);
        assert!(report.success(), "{report:?}");
        assert_eq!(
            ws.read(Path::new("a.txt")).unwrap(),
            "// notes-寿司 [\nedited 寿司\n// ]\n"
        );
        assert_eq!(
            ws.read(Path::new("b.txt")).unwrap(),
            "// notes-Żurek [\nedited Żurek\n// ]\n"
        );
        assert_eq!(
            ws.read(Path::new("f.txt")).unwrap(),
            "// notes-e\u{301}clair [\nedited e\u{301}clair\n// ]\n"
        );
    }

    #[test]
    fn a_non_ascii_object_selector_picks_its_sub_object() {
        let crank = "\
taskGroups:
  - name: g
    object: menu.yaml#メニュー/ピザ[1]
    tasks:
      - template: t.tera
        output: out.txt
  - name: h
    array: menu.yaml#меню/блюда
    tasks:
      - template: t.tera
        output: \"{{ name }}.txt\"
";
        let mut ws = workspace(&[
            (".crank", crank),
            (
                "menu.yaml",
                "メニュー:\n  ピザ:\n    - name: マルゲリータ\n    - name: ディアボロ\nменю:\n  блюда:\n    - name: борщ\n    - name: щи\n",
            ),
            ("t.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            generated(&report),
            [
                PathBuf::from("out.txt"),
                PathBuf::from("борщ.txt"),
                PathBuf::from("щи.txt"),
            ]
        );
        assert_eq!(ws.read(Path::new("out.txt")).unwrap(), "ディアボロ");
    }

    #[test]
    fn a_byte_order_mark_at_the_start_of_an_input_is_tolerated() {
        let mut ws = workspace(&[
            (".crank", &format!("\u{feff}{CRANK}")),
            ("object.yaml", "\u{feff}name: widget\n"),
            ("output.txt.tera", "Hello {{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(generated(&report), [PathBuf::from("out/widget.txt")]);
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "Hello widget"
        );
    }

    #[test]
    fn error_messages_name_non_ascii_paths_verbatim() {
        let crank = "\
taskGroups:
  - name: グループ
    object: 存在しない.yaml
    tasks:
      - template: t.tera
        output: a.txt
  - name: друга
    object: object.yaml
    tasks:
      - template: шаблон.tera
        output: b.txt
";
        let mut ws = workspace(&[
            (".crank", crank),
            ("object.yaml", "name: x\n"),
            ("шаблон.tera", "{{ 🍕 }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        let Report::Failed(errors) = &report else {
            panic!("expected a failed run, got {report:?}");
        };
        assert!(
            errors[0].starts_with("Failed to read 存在しない.yaml"),
            "{errors:?}"
        );
        assert!(
            errors[1].starts_with("Failed to render шаблон.tera"),
            "{errors:?}"
        );
        let lines = crate::output::report_lines(&report);
        assert!(
            lines
                .iter()
                .any(|line| line.text.contains("存在しない.yaml"))
        );
    }

    #[test]
    fn a_non_ascii_root_directory_works() {
        let mut ws = workspace(&[
            ("プロジェクト 🚀/.crank", CRANK),
            ("プロジェクト 🚀/object.yaml", "name: widget\n"),
            ("プロジェクト 🚀/output.txt.tera", "Hi"),
        ]);
        let report = run(&mut ws, Path::new("プロジェクト 🚀"), false);
        assert!(report.success(), "{report:?}");
        assert_eq!(
            ws.read(Path::new("プロジェクト 🚀/out/widget.txt"))
                .unwrap(),
            "Hi"
        );
    }

    #[test]
    fn the_success_summary_lists_non_ascii_paths_verbatim() {
        let report = Report::Generated {
            outputs: vec![(PathBuf::from("출력/menü 🍕.txt"), ChangeKind::Created)],
            removed: vec![PathBuf::from("古い.txt")],
        };
        let texts: Vec<String> = crate::output::report_lines(&report)
            .into_iter()
            .map(|line| line.text)
            .collect();
        assert_eq!(
            texts,
            [
                "Generated 1 file",
                "  - 출력/menü 🍕.txt (created)",
                "Removed 1 stale file",
                "  - 古い.txt",
            ]
        );
    }

    #[test]
    fn a_filter_name_a_template_cannot_call_fails_the_run_up_front() {
        // Tera identifiers are ASCII-only, so a template can't call
        // `odwróć` at all — reported before anything runs, rather than as a
        // confusing render error.
        let crank = format!(
            "{CRANK}filters:\n  odwróć: odwróć.rhai\n  🍕: pizza.rhai\n  odwroc: odwróć.rhai\n"
        );
        let mut ws = workspace(&[
            (".crank", &crank),
            ("odwróć.rhai", "fn filter(value) { value }"),
            ("pizza.rhai", "fn filter(value) { value }"),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "{{ name | odwroc }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        let Report::Failed(errors) = &report else {
            panic!("expected a failed run, got {report:?}");
        };
        let mut errors = errors.clone();
        errors.sort();
        assert_eq!(
            errors,
            [
                "Filter `odwróć` can't be called from a template: a filter name must start with an ASCII letter or `_`, followed by ASCII letters, digits, or `_`",
                "Filter `🍕` can't be called from a template: a filter name must start with an ASCII letter or `_`, followed by ASCII letters, digits, or `_`",
            ]
        );
        assert!(ws.read(Path::new("out/widget.txt")).is_err());
    }

    #[test]
    fn tera_identifiers_are_ascii_only() {
        for valid in ["a", "_", "snake_case", "_x9", "Backwards"] {
            assert!(is_tera_identifier(valid), "{valid}");
        }
        for invalid in ["", "9a", "odwróć", "日本", "🍕", "a-b", "a b", "é"] {
            assert!(!is_tera_identifier(invalid), "{invalid}");
        }
    }

    /// A [`MemoryWorkspace`] whose reads of `unreadable` fail with
    /// something other than [`ErrorKind::NotFound`].
    struct UnreadableWorkspace {
        /// The files.
        inner: MemoryWorkspace,
        /// The one path that can't be read.
        unreadable: PathBuf,
    }

    impl Workspace for UnreadableWorkspace {
        fn read(&self, path: &Path) -> io::Result<String> {
            if path == self.unreadable {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "Permission denied",
                ));
            }
            self.inner.read(path)
        }

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.inner.create_dir_all(path)
        }

        fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
            self.inner.write(path, contents)
        }

        fn remove(&mut self, path: &Path) -> io::Result<()> {
            self.inner.remove(path)
        }

        fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
            self.inner.same_file(a, b)
        }
    }

    /// Runs crank over `files`, with `unreadable` failing to read.
    fn run_unreadable(files: &[(&str, &str)], unreadable: &str, check: bool) -> Report {
        let mut ws = UnreadableWorkspace {
            inner: workspace(files),
            unreadable: PathBuf::from(unreadable),
        };
        run(&mut ws, Path::new(""), check)
    }

    /// Whether `report` failed with an error containing `text`.
    fn failed_with(report: &Report, text: &str) -> bool {
        matches!(report, Report::Failed(errors) if errors.iter().any(|e| e.contains(text)))
    }

    #[test]
    fn an_unreadable_crank_file_is_an_error_not_a_missing_one() {
        let report = run_unreadable(&[(".crank", CRANK)], ".crank", false);
        assert_eq!(
            report,
            Report::Failed(vec!["Failed to read .crank: Permission denied".to_string()])
        );
    }

    #[test]
    fn an_unreadable_manifest_is_an_error_not_a_missing_one() {
        let report = run_unreadable(
            &[
                (".crank", CRANK),
                (
                    ".crank.manifest",
                    "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n",
                ),
                ("object.yaml", "name: widget\n"),
                ("output.txt.tera", "Hello"),
            ],
            ".crank.manifest",
            false,
        );
        assert!(failed_with(&report, "Permission denied"), "{report:?}");
    }

    #[test]
    fn an_unreadable_previous_output_is_an_error_not_a_missing_one() {
        let report = run_unreadable(
            &[
                (".crank", CRANK),
                (
                    ".crank.manifest",
                    "version: 1\ntaskGroups:\n  widget:\n  - out/widget.txt\n",
                ),
                ("out/widget.txt", "Hello"),
                ("object.yaml", "name: widget\n"),
                ("output.txt.tera", "Hello"),
            ],
            "out/widget.txt",
            false,
        );
        assert!(
            failed_with(
                &report,
                "Failed to read previous output out/widget.txt while scanning for pockets: Permission denied"
            ),
            "{report:?}"
        );
    }

    #[test]
    fn check_reports_an_output_it_cant_read_as_an_error() {
        let report = run_unreadable(
            &[
                (".crank", CRANK),
                ("out/widget.txt", "Hello"),
                ("object.yaml", "name: widget\n"),
                ("output.txt.tera", "Hello"),
            ],
            "out/widget.txt",
            true,
        );
        assert!(
            failed_with(&report, "Failed to read out/widget.txt: Permission denied"),
            "{report:?}"
        );
    }

    #[test]
    fn a_memory_workspace_lists_the_files_it_holds() {
        let ws = workspace(&[("a.txt", "a"), ("./dir/../b.txt", "b")]);
        assert_eq!(
            ws.files(),
            &BTreeMap::from([
                (PathBuf::from("a.txt"), "a".to_string()),
                (PathBuf::from("b.txt"), "b".to_string()),
            ])
        );
    }

    #[test]
    fn an_output_outside_the_directory_is_a_per_task_error() {
        let mut ws = workspace(&[
            (
                "project/.crank",
                "taskGroups:\n  - name: g\n    object: o.yaml\n    tasks:\n      - template: t.tera\n        output: ../escaped.txt\n      - template: t.tera\n        output: /abs.txt\n      - template: t.tera\n        output: ok.txt\n",
            ),
            ("project/o.yaml", "a: 1\n"),
            ("project/t.tera", "hi"),
        ]);
        let report = run(&mut ws, Path::new("project"), false);
        assert_eq!(
            report,
            Report::Failed(vec![
                "Output path \"../escaped.txt\" leads outside the directory holding .crank — not written".to_string(),
                "Output path \"/abs.txt\" leads outside the directory holding .crank — not written".to_string(),
            ])
        );
        assert!(ws.read(Path::new("escaped.txt")).is_err());
        assert!(ws.read(Path::new("/abs.txt")).is_err());
        assert_eq!(ws.read(Path::new("project/ok.txt")).unwrap(), "hi");
    }

    #[test]
    fn an_array_item_whose_data_leads_its_output_outside_is_a_per_item_error() {
        let mut ws = workspace(&[
            (
                "project/.crank",
                "taskGroups:\n  - name: g\n    array: items.yaml\n    tasks:\n      - template: t.tera\n        output: \"out/{{ name }}.txt\"\n",
            ),
            (
                "project/items.yaml",
                "- name: fine\n- name: ../../escaped\n",
            ),
            ("project/t.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new("project"), false);
        assert_eq!(
            report,
            Report::Failed(vec![
                "Item [1]: Output path \"out/../../escaped.txt\" leads outside the directory holding .crank — not written".to_string(),
            ])
        );
        assert!(ws.read(Path::new("escaped.txt")).is_err());
        assert_eq!(ws.read(Path::new("project/out/fine.txt")).unwrap(), "fine");
    }

    #[test]
    fn a_manifest_entry_outside_the_directory_is_an_error_and_never_deleted() {
        for check in [false, true] {
            let mut ws = workspace(&[
                ("project/.crank", CRANK),
                (
                    "project/.crank.manifest",
                    "version: 1\ntaskGroups:\n  widget:\n  - ../important\n  - /abs\n  - inside.txt\n",
                ),
                ("project/inside.txt", "stale"),
                ("project/object.yaml", "name: widget\n"),
                ("project/output.txt.tera", "Hi"),
                ("important", "keep me"),
                ("/abs", "keep me too"),
            ]);
            let report = run(&mut ws, Path::new("project"), check);
            assert_eq!(
                report,
                Report::Failed(vec![
                    ".crank.manifest records \"../important\", which leads outside the directory holding .crank — crank won't read or delete it; remove that entry from .crank.manifest".to_string(),
                    ".crank.manifest records \"/abs\", which leads outside the directory holding .crank — crank won't read or delete it; remove that entry from .crank.manifest".to_string(),
                ]),
                "check: {check}"
            );
            assert_eq!(ws.read(Path::new("important")).unwrap(), "keep me");
            assert_eq!(ws.read(Path::new("/abs")).unwrap(), "keep me too");
            assert_eq!(ws.read(Path::new("project/inside.txt")).unwrap(), "stale");
            assert!(ws.read(Path::new("project/out/widget.txt")).is_err());
        }
    }

    #[test]
    fn a_crank_file_path_outside_the_directory_fails_the_run_before_reading_it() {
        let mut ws = workspace(&[
            (
                "project/.crank",
                "taskGroups:\n  - name: g\n    object: ../secret.yaml\n    tasks: []\n",
            ),
            ("secret.yaml", "a: 1\n"),
        ]);
        let report = run(&mut ws, Path::new("project"), false);
        assert!(
            failed_with(&report, "`object` path \"../secret.yaml\" leads outside"),
            "{report:?}"
        );
    }

    // -- Pocket markers from a template's header --------------------------

    /// A header replacing the built-in pocket markers with `#` ones.
    const HASH_HEADER: &str =
        "{#- crank:\n  pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n-#}\n";

    #[test]
    fn a_templates_header_markers_keep_hand_edits_and_leave_the_header_out() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            (
                "output.txt.tera",
                &format!("{HASH_HEADER}x = a // b[0]\n# notes [\ndefault\n# ]\n"),
            ),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "x = a // b[0]\n# notes [\ndefault\n# ]\n"
        );
        ws.insert("out/widget.txt", "x = a // b[0]\n# notes [\nmine\n# ]\n");
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "x = a // b[0]\n# notes [\nmine\n# ]\n"
        );
    }

    #[test]
    fn header_markers_are_recorded_in_the_manifest_and_built_in_ones_arent() {
        let crank = "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: hash.tera\n        output: a.py\n      - template: slash.tera\n        output: b.rs\n";
        let mut ws = workspace(&[
            (".crank", crank),
            ("object.yaml", "name: widget\n"),
            ("hash.tera", &format!("{HASH_HEADER}# a [\n# ]\n")),
            ("slash.tera", "// b [\n// ]\n"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - path: a.py\n    pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n  - b.rs\n"
        );
    }

    #[test]
    fn templates_in_one_group_can_use_different_markers_and_move_a_pocket_between_them() {
        let crank = "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: a.py\n";
        let mut ws = workspace(&[
            (".crank", crank),
            ("object.yaml", "name: widget\n"),
            ("t.tera", &format!("{HASH_HEADER}# notes [\ndefault\n# ]\n")),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        ws.insert("a.py", "# notes [\nmine\n# ]\n");
        // The pocket moves to a Rust file using the built-in markers.
        ws.insert(
            ".crank",
            "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: a.rs\n",
        );
        ws.insert("t.tera", "// notes [\ndefault\n// ]\n");
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("a.rs")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  widget:\n  - a.rs\n"
        );
    }

    #[test]
    fn an_old_file_is_scanned_with_the_markers_the_manifest_records_not_its_templates_new_ones() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            (
                "output.txt.tera",
                &format!("{HASH_HEADER}# notes [\ndefault\n# ]\n"),
            ),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        ws.insert("out/widget.txt", "# notes [\nmine\n# ]\n");
        // The header goes: the template now uses the built-in markers.
        ws.insert("output.txt.tera", "// notes [\ndefault\n// ]\n");
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
    }

    #[test]
    fn an_invalid_header_fails_just_that_task() {
        let crank = "taskGroups:\n  - name: widget\n    object: object.yaml\n    tasks:\n      - template: bad.tera\n        output: bad.txt\n      - template: good.tera\n        output: good.txt\n";
        let mut ws = workspace(&[
            (".crank", crank),
            ("object.yaml", "name: widget\n"),
            (
                "bad.tera",
                "{# crank: {pocketMarkers: [{start: '# [', end: '# ]'}]} #}\n",
            ),
            ("good.tera", "fine\n"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "Invalid crank header in bad.tera: the start marker \"# [\" must contain `{name}` exactly once"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new("bad.txt")).is_err());
        assert_eq!(ws.read(Path::new("good.txt")).unwrap(), "fine\n");
    }

    #[test]
    fn an_unknown_header_setting_is_an_error_naming_it() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "{# crank: {pockets: []} #}\n"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "Invalid crank header in output.txt.tera: crank: unknown field `pockets`, expected `pocketMarkers`"
            ),
            "{report:?}"
        );
    }

    #[test]
    fn unusable_markers_recorded_in_the_manifest_fail_the_group() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "x\n"),
            ("out/widget.txt", "x\n"),
            (
                MANIFEST_FILE_NAME,
                "version: 1\ntaskGroups:\n  widget:\n  - path: out/widget.txt\n    pocketMarkers:\n    - start: '# ['\n      end: '# ]'\n",
            ),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "Failed to use the pocket markers .crank.manifest records for out/widget.txt: the start marker"
            ),
            "{report:?}"
        );
    }

    // -- Pocket markers from `.crank` --------------------------------------

    /// A `.crank` file whose `pocketMarkers` replace the built-in ones with
    /// `#` ones, and whose group writes `main.py` from `main.tera` and
    /// `page.html` from `page.tera`.
    const HASH_CRANK: &str = "taskGroups:\n  - name: site\n    object: object.yaml\n    tasks:\n      - template: main.tera\n        output: main.py\n      - template: page.tera\n        output: page.html\npocketMarkers:\n  - start: '# {name} ['\n    end: '# ]'\n";

    #[test]
    fn crank_files_markers_apply_to_a_template_without_its_own() {
        let mut ws = workspace(&[
            (".crank", HASH_CRANK),
            ("object.yaml", "name: widget\n"),
            ("main.tera", "x = a // b[0]\n# notes [\ndefault\n# ]\n"),
            ("page.tera", "<p></p>\n"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        ws.insert("main.py", "x = a // b[0]\n# notes [\nmine\n# ]\n");
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("main.py")).unwrap(),
            "x = a // b[0]\n# notes [\nmine\n# ]\n"
        );
    }

    #[test]
    fn a_templates_header_markers_win_over_the_crank_files() {
        let mut ws = workspace(&[
            (".crank", HASH_CRANK),
            ("object.yaml", "name: widget\n"),
            ("main.tera", "# notes [\n# ]\n"),
            (
                "page.tera",
                "{# crank: {pocketMarkers: [{start: '<!-- {name} [ -->', end: '<!-- ] -->'}]} #}\n<!-- body [ -->default<!-- ] --> # not [ a pocket\n",
            ),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        ws.insert(
            "page.html",
            "\n<!-- body [ -->mine<!-- ] --> # not [ a pocket\n",
        );
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("page.html")).unwrap(),
            "\n<!-- body [ -->mine<!-- ] --> # not [ a pocket\n"
        );
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  site:\n  - path: main.py\n    pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n  - path: page.html\n    pocketMarkers:\n    - start: <!-- {name} [ -->\n      end: <!-- ] -->\n"
        );
    }

    #[test]
    fn unusable_crank_file_markers_fail_the_run_before_anything_is_written() {
        let mut ws = workspace(&[(
            ".crank",
            "taskGroups: []\npocketMarkers: [{start: '# [', end: '# ]'}]\n",
        )]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "Failed to use the pocketMarkers in .crank: the start marker \"# [\" must contain `{name}` exactly once"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new(MANIFEST_FILE_NAME)).is_err());
    }

    #[test]
    fn crank_filename_is_the_rendered_output_files_name() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\n"),
            ("output.txt.tera", "// {{ crank.filename }}\n"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(
            ws.read(Path::new("out/widget.txt")).unwrap(),
            "// widget.txt\n"
        );
    }

    #[test]
    fn crank_filename_follows_each_array_item() {
        let crank = "\
taskGroups:
  - name: dishes
    array: menu.yaml
    tasks:
      - template: dish.tera
        output: \"dishes/{{ id }}.txt\"
";
        let mut ws = workspace(&[
            (".crank", crank),
            ("menu.yaml", "- id: a\n- id: b\n"),
            ("dish.tera", "{{ crank.filename }}"),
        ]);
        assert!(run(&mut ws, Path::new(""), false).success());
        assert_eq!(ws.read(Path::new("dishes/a.txt")).unwrap(), "a.txt");
        assert_eq!(ws.read(Path::new("dishes/b.txt")).unwrap(), "b.txt");
    }

    #[test]
    fn data_defining_crank_fails_the_task() {
        let mut ws = workspace(&[
            (".crank", CRANK),
            ("object.yaml", "name: widget\ncrank: mine\n"),
            ("output.txt.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "Failed to use object.yaml as template data: `crank` is a reserved name"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new("out/widget.txt")).is_err());
    }

    /// Two task groups, `a` and `b`, writing `a_output` and `b_output`
    /// from `a.tera` and `b.tera`, plus a third, `c`, writing `c.txt`.
    fn two_groups(a_output: &str, b_output: &str) -> String {
        format!(
            "\
taskGroups:
  - name: a
    object: object.yaml
    tasks:
      - template: a.tera
        output: \"{a_output}\"
  - name: b
    object: object.yaml
    tasks:
      - template: b.tera
        output: \"{b_output}\"
  - name: c
    object: object.yaml
    tasks:
      - template: c.tera
        output: c.txt
"
        )
    }

    /// A workspace running [`two_groups`], with every template's output
    /// saying which template it came from.
    fn two_groups_workspace(a_output: &str, b_output: &str) -> MemoryWorkspace {
        workspace(&[
            (".crank", &two_groups(a_output, b_output)),
            ("object.yaml", "name: widget\n"),
            ("a.tera", "from a"),
            ("b.tera", "from b"),
            ("c.tera", "from c"),
        ])
    }

    #[test]
    fn two_tasks_generating_the_same_output_write_nothing() {
        let mut ws = two_groups_workspace("o.txt", "o.txt");
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Failed(vec![
                "Two tasks generate the same file — \"o.txt\" from task group \"a\" (template a.tera), and \"o.txt\" from task group \"b\" (template b.tera) — so nothing was written".to_string()
            ])
        );
        assert!(ws.read(Path::new("o.txt")).is_err());
        assert!(ws.read(Path::new("c.txt")).is_err());
        assert!(ws.read(Path::new(MANIFEST_FILE_NAME)).is_err());
    }

    #[test]
    fn two_tasks_generating_the_same_output_fail_a_check() {
        let mut ws = two_groups_workspace("o.txt", "o.txt");
        let report = run(&mut ws, Path::new(""), true);
        assert!(
            failed_with(&report, "Two tasks generate the same file"),
            "{report:?}"
        );
    }

    #[test]
    fn outputs_differing_only_in_dot_components_collide() {
        let mut ws = two_groups_workspace("./out/o.txt", "out/x/../o.txt");
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "\"./out/o.txt\" from task group \"a\" (template a.tera), and \"out/x/../o.txt\" from task group \"b\" (template b.tera) — so nothing was written"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new("c.txt")).is_err());
    }

    #[test]
    fn outputs_differing_only_in_letter_case_collide() {
        let mut ws = two_groups_workspace("O.txt", "o.txt");
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "\"O.txt\" from task group \"a\" (template a.tera), and \"o.txt\" from task group \"b\" (template b.tera) (paths that differ only in letter case name the same file on macOS and Windows) — so nothing was written"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new("c.txt")).is_err());
    }

    #[test]
    fn outputs_differing_only_in_how_accents_are_encoded_collide() {
        let mut ws = two_groups_workspace("Caf\u{e9}.txt", "cafe\u{301}.txt");
        let report = run(&mut ws, Path::new(""), false);
        assert!(
            failed_with(
                &report,
                "(template b.tera) (paths that differ only in how accented letters are encoded, or in letter case, name the same file on macOS) — so nothing was written"
            ),
            "{report:?}"
        );
        assert!(ws.read(Path::new("c.txt")).is_err());
    }

    #[test]
    fn an_output_colliding_within_one_group_names_both_items() {
        let crank = "\
taskGroups:
  - name: dishes
    array: menu.yaml
    tasks:
      - template: dish.tera
        output: \"{{ name }}.txt\"
";
        let mut ws = workspace(&[
            (".crank", crank),
            ("menu.yaml", "- name: a\n- name: b\n- name: a\n"),
            ("dish.tera", "{{ name }}"),
        ]);
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Failed(vec![
                "Two tasks generate the same file — \"a.txt\" from task group \"dishes\" item [0] (template dish.tera), and \"a.txt\" from task group \"dishes\" item [2] (template dish.tera) — so nothing was written".to_string()
            ])
        );
        assert!(ws.read(Path::new("b.txt")).is_err());
    }

    #[test]
    fn a_collision_is_reported_alongside_other_errors_and_still_writes_nothing() {
        let mut ws = two_groups_workspace("o.txt", "o.txt");
        ws.insert("c.tera", "{{ missing }}");
        let report = run(&mut ws, Path::new(""), false);
        let Report::Failed(errors) = &report else {
            panic!("{report:?}");
        };
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("Failed to render c.tera"), "{errors:?}");
        assert!(
            errors[1].contains("Two tasks generate the same file"),
            "{errors:?}"
        );
        assert!(ws.read(Path::new("o.txt")).is_err());
    }

    #[test]
    fn an_output_whose_path_is_respelled_with_dot_components_is_not_stale() {
        let crank = "taskGroups:\n  - name: g\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: out.txt\n";
        let mut ws = workspace(&[
            (".crank", crank),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  g:\n  - ./out.txt\n",
            ),
            ("out.txt", "// notes [\nmine\n// ]\n"),
            ("object.yaml", "name: widget\n"),
            ("t.tera", "// notes [\n// ]\n"),
        ]);
        assert_eq!(
            run(&mut ws, Path::new(""), true),
            Report::Checked {
                changes: vec![],
                stale: vec![]
            }
        );
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out.txt"), ChangeKind::Unchanged)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.read(Path::new("out.txt")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  g:\n  - out.txt\n"
        );
    }

    #[test]
    fn a_memory_workspace_knows_two_spellings_of_one_file() {
        let ws = workspace(&[("out/a.txt", "a"), ("out/b.txt", "b")]);
        let same = |a: &str, b: &str| ws.same_file(Path::new(a), Path::new(b)).unwrap();
        assert!(same("out/a.txt", "./out/x/../a.txt"));
        assert!(!same("out/a.txt", "out/b.txt"));
        assert!(!same("out/c.txt", "out/c.txt"));
    }

    /// A [`MemoryWorkspace`] that ignores letter case and how accented
    /// letters are encoded, as macOS's filesystem does by default.
    struct FoldingWorkspace(MemoryWorkspace);

    /// `path`, lowercased and decomposed.
    fn fold(path: &Path) -> PathBuf {
        PathBuf::from(
            path.to_string_lossy()
                .to_lowercase()
                .nfd()
                .collect::<String>(),
        )
    }

    impl Workspace for FoldingWorkspace {
        fn read(&self, path: &Path) -> io::Result<String> {
            self.0.read(&fold(path))
        }

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.0.create_dir_all(&fold(path))
        }

        fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
            self.0.write(&fold(path), contents)
        }

        fn remove(&mut self, path: &Path) -> io::Result<()> {
            self.0.remove(&fold(path))
        }

        fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
            self.0.same_file(&fold(a), &fold(b))
        }
    }

    /// `.crank` generating `out.txt`, last generated as `Out.txt`, with a
    /// hand-written pocket.
    fn renamed_in_case() -> MemoryWorkspace {
        let crank = "taskGroups:\n  - name: g\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: out.txt\n";
        workspace(&[
            (".crank", crank),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  g:\n  - Out.txt\n",
            ),
            ("out.txt", "// notes [\nmine\n// ]\n"),
            ("object.yaml", "name: widget\n"),
            ("t.tera", "// notes [\n// ]\n"),
        ])
    }

    #[test]
    fn an_output_renamed_in_case_on_a_case_insensitive_filesystem_is_not_stale() {
        let mut ws = FoldingWorkspace(renamed_in_case());
        assert_eq!(
            run(&mut ws, Path::new(""), true),
            Report::Checked {
                changes: vec![],
                stale: vec![]
            }
        );
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out.txt"), ChangeKind::Unchanged)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.read(Path::new("out.txt")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  g:\n  - out.txt\n"
        );
    }

    #[test]
    fn an_output_whose_accents_are_reencoded_on_macos_is_not_stale() {
        let crank = "taskGroups:\n  - name: g\n    object: object.yaml\n    tasks:\n      - template: t.tera\n        output: \"cafe\u{301}.txt\"\n";
        let mut ws = FoldingWorkspace(workspace(&[
            (".crank", crank),
            (
                ".crank.manifest",
                "version: 1\ntaskGroups:\n  g:\n  - caf\u{e9}.txt\n",
            ),
            ("cafe\u{301}.txt", "// notes [\nmine\n// ]\n"),
            ("object.yaml", "name: widget\n"),
            ("t.tera", "// notes [\n// ]\n"),
        ]));
        assert_eq!(
            run(&mut ws, Path::new(""), true),
            Report::Checked {
                changes: vec![],
                stale: vec![]
            }
        );
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("cafe\u{301}.txt"), ChangeKind::Unchanged)],
                removed: vec![],
            }
        );
        assert_eq!(
            ws.read(Path::new("caf\u{e9}.txt")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
    }

    #[test]
    fn an_output_renamed_in_case_on_a_case_sensitive_filesystem_is_stale() {
        let mut ws = renamed_in_case();
        ws.insert("Out.txt", "// notes [\nmine\n// ]\n");
        ws.insert("out.txt", "older");
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(
            report,
            Report::Generated {
                outputs: vec![(PathBuf::from("out.txt"), ChangeKind::Modified)],
                removed: vec![PathBuf::from("Out.txt")],
            }
        );
        assert!(ws.read(Path::new("Out.txt")).is_err());
        assert_eq!(
            ws.read(Path::new("out.txt")).unwrap(),
            "// notes [\nmine\n// ]\n"
        );
    }

    /// A [`MemoryWorkspace`] that can never tell whether two paths are the
    /// same file.
    struct UnsureWorkspace(MemoryWorkspace);

    impl Workspace for UnsureWorkspace {
        fn read(&self, path: &Path) -> io::Result<String> {
            self.0.read(path)
        }

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.0.create_dir_all(path)
        }

        fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
            self.0.write(path, contents)
        }

        fn remove(&mut self, path: &Path) -> io::Result<()> {
            self.0.remove(path)
        }

        fn same_file(&self, _a: &Path, _b: &Path) -> io::Result<bool> {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Permission denied",
            ))
        }
    }

    #[test]
    fn a_stale_output_that_might_be_a_current_one_is_kept() {
        let mut ws = renamed_in_case();
        ws.insert("Out.txt", "old");
        let mut ws = UnsureWorkspace(ws);
        let expected = "Failed to check whether stale output Out.txt is the same file as out.txt: Permission denied";
        let report = run(&mut ws, Path::new(""), true);
        assert_eq!(report, Report::Failed(vec![expected.to_string()]));
        let report = run(&mut ws, Path::new(""), false);
        assert_eq!(report, Report::Failed(vec![expected.to_string()]));
        assert_eq!(ws.read(Path::new("Out.txt")).unwrap(), "old");
        assert_eq!(
            ws.read(Path::new(MANIFEST_FILE_NAME)).unwrap(),
            "version: 1\ntaskGroups:\n  g:\n  - Out.txt\n  - out.txt\n"
        );
    }
}
