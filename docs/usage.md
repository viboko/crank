# Usage

```console
$ crank --help
Code generation CLI: renders Tera templates against YAML data, driven by a .crank task file.

Usage: crank [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to run crank in (defaults to the current directory) [default: .]

Options:
      --check    Reports what a normal run would create, modify, or remove, without writing anything; exits non-zero if it would change anything
  -h, --help     Print help
  -V, --version  Print version
```

## Running crank

```bash
crank
crank path/to/project
```

crank looks for a `.crank` file directly inside the target directory (the
current directory by default, or the directory named by an optional `PATH`
argument), and runs it exactly as it would if invoked from inside that
directory: every path in `.crank` is relative to the `.crank` file's own
directory. If none exists, crank prints `No crank file found` and exits
successfully. See [`crank-file-format.md`](crank-file-format.md) for
what a `.crank` file can say. crank doesn't recurse into subdirectories
looking for nested `.crank` files — see [`AGENTS.md`](../AGENTS.md)'s
"Scope discipline" section.

Everything crank prints is plain text: no colors, ANSI escape codes, or
emoji. Each line means the same whether it's read on screen, by a screen
reader, in a CI log, or piped to another tool.

## Output

```console
$ crank
Generated 3 files
  - client.js (modified)
  - server.js (unchanged)
  - shared.js (created)
Removed 1 stale file
  - old-client.js
```

A successful run lists every file crank generated, and any stale
outputs it deleted (files `.crank.manifest` says it generated last time, but that
nothing generates now). Anything that went wrong goes to standard error
instead, one problem per line, naming the file involved — see "Errors"
in [`crank-file-format.md`](crank-file-format.md#errors) for every one.
Each file is labelled with what generating it did: `created` (it
didn't exist), `modified` (crank overwrote different content), or
`unchanged` (it already held exactly what crank generated, so crank left
it alone rather than rewriting it — its modification time stays the
same, and a build tool watching it won't see it change).
A problem in one task group or task doesn't stop the others from
running.

crank exits with status `0` when everything succeeded (including when
there's no `.crank` file at all), and `1` when anything failed. Under
`--check`, it also exits `1` when anything would change — see below.

## Checking for pending changes (`--check`)

```sh
crank --check
```

`crank --check` runs the same generation and `.crank.manifest`
reconciliation logic as a normal run, but never writes anything: no output
file is created or overwritten, no stale file is deleted, and
`.crank.manifest` itself is never rewritten. Instead it prints a summary of
what a normal run *would* do — every output that would be created or
modified, and every stale output (per `.crank.manifest`) that would be
removed — and exits non-zero if that list is non-empty, or if any of the
errors below occurred. If nothing would change, it exits `0`.

This is meant for CI: run `crank --check` to fail a build when checked-in
generated output has drifted from its `.crank`/data/template sources,
without needing a second checkout to diff against.

Unlike a normal run — whose exit code reflects only whether an error
occurred, not what changed — `--check`'s exit code also reflects whether
anything would change. A clean, error-free run that would still create,
modify, or remove a file still exits `1` under `--check`.

## Command-line arguments

```text
crank [--check] [PATH]
```

`PATH` is optional and defaults to `.`, the current directory. When
given, crank runs in `PATH` instead — exactly as if it had been invoked
from inside that directory (see "Running crank" above). `--check` is
described above. The standard `--help` and `--version` flags are also
available. See [`CHANGELOG.md`](../CHANGELOG.md) for what's shipped so
far.
