// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! crank's command-line entry point: the thin I/O layer over the
//! library. It implements [`Workspace`] over the real filesystem, runs
//! [`crank::run::run`], prints the report's lines and sets the exit code.

// Reading and writing the disk and printing are this file's job —
// `clippy.toml`'s bans are for the library.
#![allow(clippy::disallowed_macros, clippy::disallowed_methods)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use crank::output::Stream;
use crank::run::Workspace;

/// crank's command-line arguments.
//
// `about` (bare) and `version` (bare) pull from Cargo.toml's `description`
// and `version` fields, so the --help/--version text stays in sync with
// the manifest without duplicating it here.
#[derive(Parser)]
#[command(name = "crank", version, about, long_about = None)]
struct Cli {
    /// Directory to run crank in (defaults to the current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Reports what a normal run would create, modify, or remove, without
    /// writing anything; exits non-zero if it would change anything
    #[arg(long)]
    check: bool,
}

/// The real filesystem, as crank's library sees it: every path it's given
/// already starts with the directory crank runs in.
///
/// The library refuses any path whose text leads outside that directory;
/// `Disk` also refuses one that leads outside through a symbolic link —
/// `out/x.txt` where `out` links to `/home/someone`, say — before reading,
/// writing, creating or deleting anything through it.
struct Disk {
    /// The directory crank runs in, with every symbolic link resolved —
    /// or `None` if it can't be resolved (it doesn't exist, say), in which
    /// case there's nothing inside it a link could lead out of.
    root: Option<PathBuf>,
}

impl Disk {
    /// A `Disk` for a run in `root`.
    fn new(root: &Path) -> Self {
        Self {
            root: fs::canonicalize(root).ok(),
        }
    }

    /// Checks that `path` — once every symbolic link in it is followed,
    /// including one at `path` itself — still leads inside the root. A
    /// link that leads nowhere (its target doesn't exist) is refused too,
    /// since writing through it would create its target, wherever that is.
    ///
    /// # Errors
    /// Returns an error of kind [`io::ErrorKind::PermissionDenied`] if
    /// `path` leads outside the root, or any error from resolving it.
    fn check_inside(&self, path: &Path) -> io::Result<()> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        for ancestor in path.ancestors() {
            match fs::canonicalize(ancestor) {
                Ok(resolved) if resolved.starts_with(root) => return Ok(()),
                Ok(_) => return Err(outside(path)),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    if fs::symlink_metadata(ancestor).is_ok() {
                        return Err(outside(path));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Err(outside(path))
    }
}

/// The error for a path that leads outside the directory crank runs in
/// through a symbolic link.
fn outside(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "{} leads outside the directory crank runs in through a symbolic link",
            path.display()
        ),
    )
}

impl Workspace for Disk {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.check_inside(path)?;
        fs::read_to_string(path)
    }

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.check_inside(path)?;
        fs::create_dir_all(path)
    }

    fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        self.check_inside(path)?;
        fs::write(path, contents)
    }

    // Deleting a symbolic link deletes the link, not what it points to, so
    // only the directory holding it has to be inside the root.
    fn remove(&mut self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            self.check_inside(parent)?;
        }
        fs::remove_file(path)
    }

    fn same_file(&self, a: &Path, b: &Path) -> io::Result<bool> {
        self.check_inside(a)?;
        self.check_inside(b)?;
        match same_file::is_same_file(a, b) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            result => result,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let report = crank::run::run(&mut Disk::new(&cli.path), &cli.path, cli.check);

    for line in crank::output::report_lines(&report) {
        match line.stream {
            Stream::Stdout => println!("{}", line.text),
            Stream::Stderr => eprintln!("{}", line.text),
        }
    }

    if report.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
