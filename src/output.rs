// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plain-text console output: no colors, no emoji, so every line reads
//! the same to a screen reader, in a log, or piped to another tool.
//!
//! [`report_lines`] describes a [`Report`] as the lines crank prints, for
//! the CLI to print and the browser playground to show.

use std::path::{Path, PathBuf};

use crate::check::ChangeKind;
use crate::run::Report;

/// Where a line is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output: what crank did.
    Stdout,
    /// Standard error: what went wrong.
    Stderr,
}

/// One line crank prints, without its newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Which stream it's printed to.
    pub stream: Stream,
    /// The line itself.
    pub text: String,
}

impl Line {
    /// A stdout line of `text`.
    fn stdout(text: impl Into<String>) -> Self {
        Self {
            stream: Stream::Stdout,
            text: text.into(),
        }
    }
}

/// Everything crank prints for `report`, in order.
#[must_use]
pub fn report_lines(report: &Report) -> Vec<Line> {
    match report {
        // An exact string per spec.
        Report::NoCrankFile => vec![Line::stdout("No crank file found")],
        Report::Failed(errors) => error_lines(errors),
        Report::Generated { outputs, removed } => {
            let mut lines = success_lines(outputs);
            lines.extend(removed_lines(removed));
            lines
        }
        Report::Checked { changes, stale } => check_summary_lines(changes, stale),
    }
}

/// A `  - path` list item.
fn item(path: &Path, suffix: &str) -> Line {
    Line::stdout(format!("  - {}{suffix}", path.display()))
}

/// The success summary shown once every task has run without error,
/// listing each output file generated (in task order), labelled with
/// what generating it did to the file — see [`generated_label`].
fn success_lines(outputs: &[(PathBuf, ChangeKind)]) -> Vec<Line> {
    if outputs.is_empty() {
        return vec![Line::stdout("No files to generate")];
    }
    let mut lines = vec![Line::stdout(success_header(outputs.len()))];
    lines.extend(
        outputs
            .iter()
            .map(|(output, kind)| item(output, &format!(" ({})", generated_label(*kind)))),
    );
    lines
}

/// The label shown next to a generated path in [`success_lines`]: what
/// generating it did to the file. An `unchanged` file already held
/// exactly what was generated, so crank left it alone.
fn generated_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Created => "created",
        ChangeKind::Modified => "modified",
        ChangeKind::Unchanged => "unchanged",
    }
}

/// The summary of stale outputs deleted because `.crank` no longer
/// produces them (per `.crank.manifest`'s record of the prior run). Empty
/// when nothing was removed, so a run that doesn't touch any stale files
/// stays silent about this step.
fn removed_lines(removed: &[PathBuf]) -> Vec<Line> {
    if removed.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::stdout(removed_header(removed.len()))];
    lines.extend(removed.iter().map(|output| item(output, "")));
    lines
}

/// Every error accumulated while running task groups/tasks, one per line,
/// on stderr — each message is already fully formatted (naming the
/// offending group/task) by the caller. A message spanning several lines
/// (a render error's chain of causes) is several lines here too.
fn error_lines(errors: &[String]) -> Vec<Line> {
    errors
        .iter()
        .flat_map(|error| error.split('\n'))
        .map(|text| Line {
            stream: Stream::Stderr,
            text: text.to_string(),
        })
        .collect()
}

/// The `--check` summary of what a normal run would do differently on
/// disk, without doing any of it. `changes` lists every task output that
/// would be created or modified — an unchanged output is never included,
/// see [`ChangeKind`]; `stale` lists every previously generated path that
/// would be deleted as no longer produced.
fn check_summary_lines(changes: &[(PathBuf, ChangeKind)], stale: &[PathBuf]) -> Vec<Line> {
    if changes.is_empty() && stale.is_empty() {
        return vec![Line::stdout("Nothing would change")];
    }
    let mut lines = Vec::new();
    if !changes.is_empty() {
        lines.push(Line::stdout(check_changes_header(changes.len())));
        lines.extend(
            changes
                .iter()
                .map(|(path, kind)| item(path, &format!(" ({})", change_kind_label(*kind)))),
        );
    }
    if !stale.is_empty() {
        lines.push(Line::stdout(check_removed_header(stale.len())));
        lines.extend(stale.iter().map(|path| item(path, "")));
    }
    lines
}

/// The header line for [`success_lines`], pluralized on `count`.
fn success_header(count: usize) -> String {
    let noun = if count == 1 { "file" } else { "files" };
    format!("Generated {count} {noun}")
}

/// The header line for [`removed_lines`], pluralized on `count`.
fn removed_header(count: usize) -> String {
    let noun = if count == 1 { "file" } else { "files" };
    format!("Removed {count} stale {noun}")
}

/// The label shown next to a changed path in [`check_summary_lines`].
/// `Unchanged` never reaches here — the caller filters it out before
/// building the `changes` list, since only created/modified outputs are
/// reportable changes.
fn change_kind_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Created => "would create",
        ChangeKind::Modified => "would modify",
        ChangeKind::Unchanged => {
            unreachable!("check_summary_lines is never given an Unchanged entry")
        }
    }
}

/// The header line for [`check_summary_lines`]'s changes section,
/// pluralized on `count`.
fn check_changes_header(count: usize) -> String {
    let noun = if count == 1 { "file" } else { "files" };
    format!("{count} {noun} would be created or modified")
}

/// The header line for [`check_summary_lines`]'s stale section,
/// pluralized on `count`.
fn check_removed_header(count: usize) -> String {
    let noun = if count == 1 { "file" } else { "files" };
    format!("{count} stale {noun} would be removed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_header_is_singular_for_one_file() {
        assert_eq!(success_header(1), "Generated 1 file");
    }

    #[test]
    fn success_header_is_plural_for_multiple_files() {
        assert_eq!(success_header(3), "Generated 3 files");
    }

    #[test]
    fn success_lines_label_every_output_with_what_happened_to_it() {
        let outputs = [
            (PathBuf::from("models/users.js"), ChangeKind::Unchanged),
            (PathBuf::from("models/orders.js"), ChangeKind::Modified),
            (PathBuf::from("models/items.js"), ChangeKind::Created),
        ];
        let texts: Vec<String> = success_lines(&outputs)
            .into_iter()
            .map(|line| line.text)
            .collect();
        assert_eq!(
            texts,
            [
                "Generated 3 files",
                "  - models/users.js (unchanged)",
                "  - models/orders.js (modified)",
                "  - models/items.js (created)",
            ]
        );
    }

    #[test]
    fn removed_header_is_singular_for_one_file() {
        assert_eq!(removed_header(1), "Removed 1 stale file");
    }

    #[test]
    fn removed_header_is_plural_for_multiple_files() {
        assert_eq!(removed_header(3), "Removed 3 stale files");
    }

    #[test]
    fn check_changes_header_is_singular_for_one_file() {
        assert_eq!(
            check_changes_header(1),
            "1 file would be created or modified"
        );
    }

    #[test]
    fn check_changes_header_is_plural_for_multiple_files() {
        assert_eq!(
            check_changes_header(3),
            "3 files would be created or modified"
        );
    }

    #[test]
    fn check_removed_header_is_singular_for_one_file() {
        assert_eq!(check_removed_header(1), "1 stale file would be removed");
    }

    #[test]
    fn check_removed_header_is_plural_for_multiple_files() {
        assert_eq!(check_removed_header(3), "3 stale files would be removed");
    }
}
