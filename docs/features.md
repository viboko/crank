# Features

## Overall behavior

- crank runs in the current directory by default, or a directory given
  as a command-line argument, and looks for a `.crank` file directly
  inside it. It doesn't look in subdirectories.
- Without a `.crank` file, crank prints `No crank file found` and exits
  successfully — nothing to do isn't an error.
- The `.crank` file is YAML: a list of **task groups**, each loading one
  data file and rendering one or more **tasks** — a Tera template and
  the path to write the result to. Every path in it is relative to the
  `.crank` file's own directory.
- crank rejects a key it doesn't know anywhere in `.crank`, so a typo
  like `objcet:` is an error rather than silently ignored. Keys
  starting with `x-` are the exception, skipped at any level — room for
  your own notes, or a YAML anchor (`&name`) that aliases (`*name`)
  reuse elsewhere in the file.
- crank never reads, writes or deletes a file outside that directory
  through a path in `.crank` or `.crank.manifest`: an absolute path, or
  one that climbs out with `..` — including an `output` whose data puts
  a `..` in it — is an error. Nor through a symbolic link: a path
  that leads out through one is refused before crank touches it.
- Every task's output is overwritten if it already exists, and its
  parent directories are created as needed. An output that already
  holds exactly what crank generated isn't rewritten, so its
  modification time doesn't change. crank labels each output it lists
  `(created)`, `(modified)` or `(unchanged)`.
- A problem in one task group or task (a missing file, invalid YAML, a
  template error) is reported without stopping other, independent task
  groups and tasks from still running; crank exits non-zero if anything
  failed.
- Two tasks generating the same file — anywhere in `.crank`, including
  two items of one `array` group, and counting paths that differ only
  in letter case, in how an accented letter is encoded, or in `.`/`..`
  components — is an error, and crank
  writes nothing at all that run rather than let one overwrite the
  other.
- Everything crank prints is plain text — no colors or emoji — so it
  reads the same to a screen reader, in a log, or piped elsewhere.

The sections below cover each feature. See
[`crank-file-format.md`](crank-file-format.md) for the complete
reference, with a worked example and unit test for each, and
[`usage.md`](usage.md) for the command line.

## Task groups and data

- `.crank` must have a `taskGroups` key, so an empty or broken file is
  an error rather than a run that deletes every generated file;
  `taskGroups: []` generates nothing on purpose.
- Each task group has a required, unique `name` (used, for example, as
  its key in `.crank.manifest`).
- A task group's `object` names a YAML data file, loaded once and
  applied to every task listed under it; its top-level keys become the
  template's variables.
- `array` is a mutually exclusive alternative to `object`: same syntax,
  but resolving to a sequence, whose tasks run once per item instead.
  An `array` task's `output` must contain a substitution, since every
  item would otherwise be written to the same file.

## Selectors (`#`)

- `object` and `array` can pick part of a data file with a `#selector`
  after the path: keys and `[index]` steps, e.g.
  `object.yaml#pizzas[0]` or `menu.yaml#sections/desserts`.
- A selector that doesn't resolve is an error naming the step that
  failed.

## Templates

- Each task renders its `template` with [Tera](https://keats.github.io/tera/)
  (Jinja-like syntax: variable substitution, `{% for %}` loops with
  `loop.last`/`loop.first`/`loop.index`, conditionals, and filters)
  against its task group's data.
- A task's `output` is itself rendered as a Tera template first, so it
  can include `{{ ... }}` substitutions. One starting with a substitution
  must be quoted (`output: "{{ name }}.txt"`), since YAML reads an
  unquoted leading `{` as a mapping; crank's parse error says so.
- `crank` is a reserved template variable. `{{ crank.filename }}` is the
  name of the file being written (e.g. `widget.rs`), for a header naming
  its own file; a data file can't define a top-level `crank` key.
- A render error names the template and the underlying cause, e.g. the
  missing variable's name.

## Filters

- Tera's own large [built-in filter library](https://keats.github.io/tera/#filters)
  is available with no configuration.
- crank adds two filters of its own, `snake_case` and `wrap`.
- A `.crank` file can declare its own filters under `filters:`, each a
  sandboxed [Rhai](https://rhai.rs) script, with optional named
  parameters (e.g. `{{ value | wrap(width=20) }}`). crank's built-in
  filters are themselves such scripts.
- A filter name that collides with one of Tera's own is an error, not a
  silent override.

## Pockets

- A template's output can mark a region as hand-editable with a pocket
  marker pair — either whole-line (`// name [` ... `// ]`) or inline
  (`/* name [ */` ... `/* ] */`), both active at once. On every run after
  the first, crank reads the task group's previously generated output
  (found via `.crank.manifest`) and keeps a matching pocket's old
  content in place of the freshly rendered default.
- A pocket's name is unique across its whole task group, whichever
  markers it uses, not just its file, so it's still found if it moves to
  a different output file between runs.
- `.crank` can list its own pocket markers under `pocketMarkers` — e.g.
  `# {name} [` ... `# ]` for a Python project — replacing the built-in
  ones for every template. A template can list its own in a
  `{#- crank: ... -#}` header comment at its start, which win over
  `.crank`'s. `.crank.manifest` records the markers each such file was
  written with, so old pockets are still found if the markers later
  change or are removed.

## `.crank.manifest` and stale outputs

- After a fully successful run, crank compares what it just generated
  against `.crank.manifest` — its record, grouped by task group name, of
  what it generated last time — and deletes any output that's no longer
  produced, e.g. after renaming a task's `output`. Respelling a path
  without changing which file it names — `./out.txt` to `out.txt`,
  `Out.txt` to `out.txt` on a case-insensitive filesystem, or an
  accented letter encoded differently on macOS — doesn't make it stale.
- `.crank.manifest` is then rewritten to match (or left alone, keeping
  its modification time, if it already does), and should be committed
  to git.
- `.crank.manifest` records its own format `version`, so a manifest
  written in a format this crank doesn't read fails with a clear error
  rather than being misread.

## Checking for changes (`--check`)

- `crank --check` reports what a run would create, modify, or remove,
  without writing anything, and exits non-zero if it would change
  anything — for CI, to catch generated files that have drifted from
  their sources.

## Names in any language

- Non-ASCII text (any script, emoji, combining accents) works in file
  and directory names, task group names, data, templates and pocket
  content.
- Selector keys and pocket names can use letters and digits in any
  script; filter names are ASCII, since Tera's template syntax requires
  it.
