# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).
While the version stays below `1.0.0`, breaking changes may land in any
minor release.

## [Unreleased]

## [0.1.0] - 2026-10-21

The first release of crank. See [`docs/features.md`](docs/features.md)
for an overview, and [`docs/crank-file-format.md`](docs/crank-file-format.md)
and [`docs/usage.md`](docs/usage.md) for the full reference.

### Added

- The `crank` command: it reads a `.crank` file in the current directory
  (or one given as an argument), renders [Tera](https://keats.github.io/tera/)
  templates against YAML data, and writes each result to its output
  path, creating parent directories as needed. Without a `.crank` file,
  it prints `No crank file found` and exits successfully.
- Task groups, each with a unique `name` and either an `object` data
  file, applied to each of its tasks once, or an `array`, whose tasks
  run once per item. A `#selector` after the path (e.g.
  `menu.yaml#pizzas[0]`) picks part of the file.
- A task's `output` is itself a Tera template, so one task can write a
  file per item (`output: "{{ name }}.txt"`).
- The reserved `crank` template variable: `{{ crank.filename }}` is the
  name of the file being written.
- Tera's built-in filters, plus crank's own `snake_case` and `wrap`, and
  custom filters declared under `filters:` as sandboxed
  [Rhai](https://rhai.rs) scripts, with optional named parameters. A
  filter can't take the name of one of Tera's own.
- Pockets: a template marks a region of its output as hand-editable
  (`// name [` ... `// ]`, or inline `/* name [ */` ... `/* ] */`), and
  crank keeps whatever is in it across runs, even if the pocket moves to
  another file in the same task group. `pocketMarkers`, in `.crank` or a
  template's `{#- crank: ... -#}` header, replaces the built-in markers
  for languages where those aren't comments.
- `.crank.manifest`, a versioned record of what crank generated, meant
  to be committed. After a fully successful run, crank deletes any
  output it no longer generates, e.g. after a rename.
- `--check`: reports what a run would create, modify or remove, without
  writing anything, and exits non-zero if anything would change — for
  CI.
- Careful writes: an output that already holds what crank generates is
  left untouched, two tasks writing the same file is an error, and no
  path in `.crank` or `.crank.manifest` may lead outside its directory,
  directly or through a symbolic link.
- Strict `.crank` parsing: an unknown key is an error, except one
  starting with `x-`, which is free for notes or YAML anchors.
- An error in one task group or task doesn't stop the others; crank
  exits non-zero if anything failed. Errors name the file and the
  cause, such as a template's missing variable.
- Non-ASCII text — any script, emoji, combining accents — in file names,
  task group names, data, templates, selector keys and pocket names.
- Plain-text output, with no colors or emoji, labelling each output
  `(created)`, `(modified)` or `(unchanged)`.
- Prebuilt binaries for Linux (x86_64, arm64), macOS (Apple silicon,
  Intel) and Windows (x86_64), each with a checksum, an SBOM and a
  signed build provenance attestation; a Homebrew formula (`brew install
  viboko/tap/crank-cli`); and the `crank-cli` package on crates.io. The
  minimum supported Rust version is 1.88.
- A browser playground at <https://crank.viboko.dev/>: edit a pretend
  project and watch crank's output update as you type, with worked
  examples, shareable links and a download of the project. It runs
  crank itself, compiled to WebAssembly, and counts visits with
  GoatCounter, which sets no cookies and never sees a shared link's
  `#fragment`.
