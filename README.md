![crank](docs/images/crank-logo.svg)

# crank

**crank** is a code generator: a command-line tool that feeds YAML data
into [Tera](https://keats.github.io/tera/) templates and writes the
results to disk, as described by a `.crank` file. Run it again whenever
the data or templates change, and it regenerates everything — keeping
any hand-written code you've put in marked *pockets* of the generated
files, and deleting files it generated before but no longer does.

The idea is to keep repetitive code (API clients, config files, enums,
docs tables) generated from one source of truth and checked into the
repo, with `crank --check` failing CI when the generated files have
drifted from their sources.

**[Try it in your browser](https://crank.viboko.dev/)**: the
playground runs crank as you type, with examples of each feature.

See the [full documentation](docs/README.md) for the `.crank` file
format, the command-line reference, and everything else crank does.
Contributors can browse the internal API reference, built from the code's
doc comments, on [docs.rs](https://docs.rs/crank-cli) (or locally with
`just docs`).

## Example

Given this layout:

```text
object.yaml
output.py.tera
.crank
```

Template files use the `.tera` extension — Tera is only *inspired* by
Jinja2, not an implementation of it, so `.j2`/`.jinja2` would be
misleading. `output.py.tera` follows the target filename (`output.py`)
plus the templating language, so it's unambiguous which output a template
produces even next to other templates.

`object.yaml` (the data):

```yaml
name: widget
tags:
  - alpha
  - beta
```

`output.py.tera` (the template, using Tera's Jinja-style syntax):

```jinja
class {{ name | upper }}:
    tags = [
{% for tag in tags %}
        "{{ tag }}"{% if not loop.last %},{% endif %}
{% endfor %}
    ]
```

`.crank` (which data feeds which template, and where the output goes):

```yaml
taskGroups:
  - name: widget
    object: object.yaml
    tasks:
      - template: output.py.tera
        output: output.py
```

Running `crank` renders `output.py.tera` against `object.yaml` and writes
the result to `output.py`:

```console
$ crank
Generated 1 file
  - output.py (created)
```

Every path in `.crank` is resolved relative to the `.crank` file's own
directory.

See [`docs/crank-file-format.md`](docs/crank-file-format.md) for the
full `.crank` file syntax, including `array` groups that generate a file
per item, `#selectors`, custom filters, and pockets.

## Why not X?

crank isn't the first tool to generate code from templates. Here's how
it compares with the ones you're most likely to have come across:

- **[Cookiecutter](https://github.com/cookiecutter/cookiecutter)** and
  **[Copier](https://github.com/copier-org/copier)** scaffold a new
  project from a template, once; Copier can later update it by merging
  in changes to the template. crank is for code that's regenerated
  continuously from data inside a project, with the hand-written parts
  marked as pockets rather than merged.
- **[Hygen](https://github.com/jondot/hygen)** and
  **[Plop](https://github.com/plopjs/plop)** generate new files from
  templates and answers to prompts, as you work. crank is driven by
  data files instead, so it can regenerate everything at any time, and
  is a single binary with no Node.js needed.
- **Template one-liners** such as
  [j2cli](https://github.com/kolypto/j2cli) render one template with one
  data file. crank runs many templates over many data files, writes a
  file per item of a list, carries hand-written pockets over, deletes
  outputs that are no longer generated, and can check it's all up to
  date.

## Installation

On macOS or Linux, with [Homebrew](https://brew.sh/):

```bash
brew install viboko/tap/crank-cli
```

With Rust 1.88 or newer, from [crates.io](https://crates.io/crates/crank-cli):

```bash
cargo install --locked crank-cli
```

The crate is called `crank-cli` because `crank` was already taken on
crates.io, and the Homebrew formula takes the same name; the command
each installs is still `crank`.

Or download a prebuilt binary from the
[latest release](https://github.com/viboko/crank/releases/latest), for
Linux (x86_64 and arm64), macOS (Apple silicon and Intel) or Windows
(x86_64). Each archive has a `.sha256` checksum beside it, and a signed
build provenance attestation you can check with the
[GitHub CLI](https://cli.github.com/):

```bash
shasum -a 256 -c crank-v0.1.0-aarch64-apple-darwin.tar.gz.sha256
gh attestation verify crank-v0.1.0-aarch64-apple-darwin.tar.gz --repo viboko/crank
```

To build from a checkout of this repository instead, with
[rustup](https://rustup.rs/)'s `cargo`, which fetches the Rust version
the repo pins:

```bash
cargo install --locked --path .
```

## Reporting a bug or asking for a feature

[Open an issue](https://github.com/viboko/crank/issues/new) on GitHub.
For a bug, say what you expected crank to generate or print and what it
did instead, and include the output of `crank --version`. The quickest
way to show the problem is often a
[playground](https://crank.viboko.dev/) link: recreate it there and use
"Copy link", which puts the whole pretend project in the link.

To report a security vulnerability, don't open a public issue; see
[`SECURITY.md`](SECURITY.md) instead. To propose a change yourself, see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## Development

This project uses [`just`](https://github.com/casey/just) as a task
runner. See the [`justfile`](justfile) for available recipes, or run:

```bash
just
```

`just ci` runs everything CI runs; see
[`docs/tooling.md`](docs/tooling.md) for what each check does.
`just coverage` shows how much of the code the tests reach — see
[`docs/coverage.md`](docs/coverage.md) — and `just mutants` whether they'd
notice it changing ([`docs/mutation-testing.md`](docs/mutation-testing.md)).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
