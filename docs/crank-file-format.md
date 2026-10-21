# The `.crank` file format

A `.crank` file tells crank which YAML data to feed into which Tera
templates, and where to write the results. This page is the full
reference for the file and everything it refers to: data files and
selectors, templates and filters, pockets of hand-written content, and
`.crank.manifest`. See [`usage.md`](usage.md) for running crank itself.

## The `.crank` file

A `.crank` file is YAML, and describes one or more **task groups**:

```yaml
taskGroups:
  - name: widget
    object: object.yaml
    tasks:
      - template: output.py.tera
        output: output.py
```

`taskGroups` is required. Since crank deletes every output it generated
last time that this run doesn't generate, a `.crank` file that's empty,
or lost its `taskGroups` to a bad edit, would otherwise delete them all.
To generate nothing on purpose, write `taskGroups: []`.

- `name` — identifies the task group, e.g. as its key in
  `.crank.manifest`. Required, and must be unique across every task group
  in a `.crank` file — a duplicate `name` is a parse error.
- `object` — path to a YAML data file, relative to the `.crank` file's own
  directory, optionally followed by a `#selector` picking a sub-object out
  of it (e.g. `object.yaml#pizzas[0]`) — see "Object selectors" below.
  Loaded once per task group and applied to every task listed under it.
- `array` — a mutually exclusive alternative to `object`: same
  path-plus-optional-`#selector` syntax, but resolving to a sequence
  instead of a single object. A task group must specify exactly one of
  `object`/`array` — specifying both, or neither, is a `.crank` parse
  error. See "Array task groups" below.
- `tasks` — a list of `{ template, output }` pairs. `template` and `output`
  are both paths relative to the `.crank` file's own directory. `output` is
  itself rendered as a Tera template (against the same data as `template`)
  before being used as a path, so it can include `{{ ... }}` substitutions
  — see "Array task groups" below for why this matters most for `array`
  groups. YAML reads a value starting with `{` as a mapping, so an
  `output` (or `template`) that *starts* with a substitution must be
  quoted — `output: "{{ name }}.txt"`, not `output: {{ name }}.txt`.
  Unquoted, it's a `.crank` parse error saying so.
- `filters` — optional; a mapping of filter name to the path of a `.rhai`
  script defining it, relative to the `.crank` file's own directory:

  ```yaml
  filters:
    backwards: backwards.rhai
  ```

  See "Custom filters" below.
- `pocketMarkers` — optional; a list of pocket marker pairs replacing
  crank's built-in `//` and `/* */` ones for every template that doesn't
  list its own:

  ```yaml
  pocketMarkers:
    - start: '# {name} ['
      end: '# ]'
  ```

  See "Your own pocket markers" below.

Any other key, at any level — the top, a task group, a task or a
`pocketMarkers` entry — is a `.crank` parse error naming it and where
it is, so a misspelled or misremembered key (`objcet:`, `tmpl:`)
can't go unnoticed. The exception is a key starting with `x-`, which
crank skips, along with everything under it. That leaves room for your
own notes, or for a YAML anchor that the rest of the file reuses with
aliases:

```yaml
x-dish-tasks: &dish-tasks
  - template: page.html.tera
    output: "site/{{ id }}.html"
  - template: card.txt.tera
    output: "print/{{ id }}.txt"

taskGroups:
  - name: starters
    array: menu.yaml#starters
    tasks: *dish-tasks
  - name: mains
    array: menu.yaml#mains
    tasks: *dish-tasks
```

An alias (`*dish-tasks`) repeats the anchored value whole. YAML's `<<`
merge key, which splices one mapping into another, isn't supported:
crank reads `<<` as an unknown key, and says so. (`filters`' keys are
filter names, not `.crank` keys, so neither rule applies to them.)

A `.crank` file may list any number of task groups, and each task group any
number of tasks. Every task group and every task runs independently: a
problem in one (a missing file, invalid YAML, a template error) is reported
without stopping any other task group or task from still running. crank
exits non-zero if anything failed.

Every task's `output` is overwritten if it already exists — unless it
already holds exactly what crank generated, in which case it's left
alone, keeping its modification time — and its parent directory is
created if it doesn't exist yet.

No two tasks may generate the same file. crank renders every task
before writing any output, and if two of them — in different task
groups, or two items of one `array` group — render `output` paths naming
the same file, it reports each pair and writes nothing at all that run
(see "Errors" below). Paths count as the same once `.` and `..`
components are resolved (`./o.txt`, `x/../o.txt` and `o.txt`), letter
case is ignored (`o.txt` and `O.txt`, which are one file on macOS and
Windows), and so is how an accented letter is encoded (`café.txt` with
a single `é` character and with `e` plus a combining accent, which are
one file on macOS), so a `.crank` file that works on one system doesn't
overwrite its own output on another.

Every path — `object`, `array`, `template`, a filter script, each
rendered `output`, and each `.crank.manifest` entry — must stay inside
the `.crank` file's directory: an absolute path, or one whose `..`
components climb above that directory (`../x.txt`, `a/../../x.txt`), is
an error (see "Errors" below), so crank never reads, writes or deletes
a file outside it through one. `a/../b.txt` is fine. The CLI also
follows symbolic links before touching a file: a path that leads outside
the directory through one — `out/x.txt` where `out` links to
`/home/someone`, or a `template.tera` linking to a file elsewhere — is
refused the same way, as is a link whose target doesn't exist (writing
through it would create that target). A link that stays inside the
directory works as usual, and deleting a stale output that is itself a
link deletes just the link.

## `.crank.manifest`

Alongside `.crank`, crank maintains `.crank.manifest`: a record of every
output path it wrote on its last fully successful run. **Commit this file
to git** — it's what lets crank clean up outputs that are no longer
produced (e.g. after renaming a task's `output`, or removing a task) in any
checkout, not just the working copy that happened to generate them.

On each run, crank:

1. Reads `.crank.manifest` (if it exists) before generating anything.
2. Generates as usual.
3. If every task group and task succeeded, compares the two lists and
   deletes any output `.crank.manifest` recorded that this run didn't
   produce — comparing paths with `.` and `..` resolved, and never
   deleting one that's the same file as an output this run produced
   (`Out.txt` recorded, `out.txt` produced, on a case-insensitive
   filesystem, or `café.txt` re-encoded on macOS) — then rewrites
   `.crank.manifest` to match what was just generated — unless it already matches exactly, in which case it's
   left alone.

If **any** task group or task fails, this reconciliation is skipped
entirely for that run — nothing is deleted and `.crank.manifest` is left
untouched, so a broken run never deletes a still-wanted file based on an
incomplete picture of "current" outputs. Reconciliation happens on the next
fully successful run instead.

`.crank.manifest` is a YAML mapping with two keys: `version`, the
manifest format's version (currently `1`), and `taskGroups`, a mapping of
task group `name` to the list of output paths that group produced, each
path relative to the `.crank` file's own directory. Both the group names
and each group's list of paths are sorted for a stable diff. An entry is
usually just a path, like every one of `each-pizza`'s below — except `pizzas`' one output here, which
was written with pocket markers other than the built-in ones (see "Your
own pocket markers" below), so crank recorded those markers alongside
its path:

```yaml
version: 1
taskGroups:
  pizzas:
    - path: output/menu.py
      pocketMarkers:
        - start: '# {name} ['
          end: '# ]'
  each-pizza:
    - output/pizza-Diabolo.txt
    - output/pizza-Hawaiian.txt
```

This `pocketMarkers` key mirrors the one in `.crank` and in a template's
header (same shape) but isn't something you write — crank writes it, as
its own record of what actually generated `output/menu.py`, so it can
find that file's pockets on a later run using the markers it was
actually written with, regardless of what `.crank` or its template says
by then (either may since have changed, or the template may no longer be
used at all). A project that only uses the built-in markers never sees
this shape — every `.crank.manifest` entry stays a bare path.

Its format is internal to crank — treat it as generated, not something to
hand-edit, aside from resolving the occasional merge conflict.

crank only reads a manifest whose `version` is one it understands —
currently just `1`. One recording any other version (e.g. written by a
newer crank) fails with an error naming both versions. Delete it and let
the next successful run regenerate it in the current format.

## The data file

Any valid YAML document. When `object` has no `#selector`, the file's top
level must be a mapping, since its keys become the variables available to
templates — a list or scalar at the top level is a data-file error, since
there'd be nothing to bind template variables to. A top-level key named
`crank` is an error too, since that name is reserved for crank's own
variable (see "The `crank` variable" below); a `crank` key nested deeper
is fine. When `array` has no
`#selector`, the file's top level must instead be a sequence — see "Array
task groups" below.

## Object selectors

An `object` field can pick a sub-object out of a larger data file with a
`#selector` suffix: `path#segment/segment...`, where each segment is a
mapping key (a bare identifier), optionally followed by one or more `[N]`
sequence-index suffixes (`pizzas[0]`), and a selector may open with a bare
`[N]` (no leading key) if the file's top level is itself a list.

Given:

```yaml
# object.yaml
pizzas:
  - name: Hawaiian
    price: 17.99
  - name: Diabolo
    price: 19.99
```

```yaml
taskGroups:
  - name: hawaiian
    object: object.yaml#pizzas[0]
    tasks:
      - template: template.tera
        output: output.py
```

`object.yaml#pizzas[0]` resolves to `{name: Hawaiian, price: 17.99}`, so the
template sees `name` and `price` directly.

Whatever an `object` selector resolves to **must be a mapping** — selecting
an array (e.g. `object.yaml#pizzas`, with no index) or a scalar (e.g.
`object.yaml#pizzas[0]/name`) is an error, since crank needs a single
object to hand to a template as its context; a template can already loop
over a list *value* (`{% for pizza in pizzas %}` works whenever
`pizzas` is a key in the object in scope), but there's no way to pass a
bare array as an `object` group's whole template context — see "Array task
groups" below for running a group's tasks once per item in a sequence
instead.

A selector's key segments are bare identifiers only: a letter or
underscore, then any letters, digits, underscores, or combining marks —
letters and digits in any script, so `object.yaml#メニュー[0]` or
`object.yaml#меню/блюда` works. There's no quoting/escaping, so a real
YAML key containing `/`, `[`, `.`, spaces, or emoji can't be selected.

If a `.crank` file writes `taskGroups` in YAML's compact flow style (all on
one line, e.g. `taskGroups: [{object: object.yaml#pizzas[0], ...}]`), the
selector must be quoted (`"object.yaml#pizzas[0]"`) since `[`/`]` are
structural characters in flow-style YAML. In the block style shown above
(each field on its own line), no quoting is needed. The same rule applies
to an `array` field's selector.

## Array task groups

An `array` field is a mutually exclusive alternative to `object`, using the
exact same `path` / `path#selector` syntax described above — the only
difference is what the resolved value must be: a **sequence**, instead of
a mapping. A task group must specify exactly one of `object`/`array`;
specifying both, or neither, is a `.crank` parse error.

Where an `object` group loads one object and runs its tasks once, an
`array` group runs its tasks once *per item* in the sequence, with that
item becoming the template context for that run — so one task can fan out
into many generated files:

```yaml
taskGroups:
  - name: pizzas
    array: object.yaml#pizzas
    tasks:
      - template: pizza.txt.tera
        output: pizza-{{ name|lower }}.txt
```

Given the `pizzas` data file from "Object selectors" above, this generates
`pizza-hawaiian.txt` and `pizza-diabolo.txt`, each rendered against its own
pizza's `name`/`price`. Each item, just like an `object` group's resolved
value, **must be a mapping** — an item that isn't is a per-item error (that
one item's tasks are skipped; every other item, task, and task group still
runs).

An `array` field with no `#selector` uses the data file's whole top level
as the sequence of items, which must therefore already be a YAML sequence
(mirroring how an `object` field with no selector requires the whole file
to be a mapping).

Because the same task runs once per item, a literal `output` (e.g.
`output: pizza.txt`) would have every item overwrite the same file. So for
`array` groups specifically, `output` **must** contain a `{{ ... }}`
substitution — checked when the `.crank` file is parsed, before any data
file is read. This is on top of `output` always being rendered as a Tera
template (see "The `.crank` file" above), which applies to `object` groups
too, just without the mandatory-substitution rule, since there's only one
item to render for.

Even with a substitution present, two items can still render to the same
`output` (e.g. two pizzas with the same `name`) — that's two tasks
generating the same file, which fails the run without writing anything
(see "The `.crank` file" above).

## The template file

Templates use [Tera](https://keats.github.io/tera/) syntax (a Jinja2/Django
dialect). crank doesn't require a particular extension, but `.tera` is the
recommended convention — e.g. `output.py.tera` for a template that produces
`output.py` — since Tera is only inspired by Jinja2, not an implementation
of it, so `.j2`/`.jinja2` would be misleading:

- `{{ variable }}` — substitutes a value from the data file.
- `{% for item in items %}...{% endfor %}` — loops over a list; `{% for
  key, value in mapping %}...{% endfor %}` loops over a mapping.
  `loop.index`, `loop.first`, and `loop.last` are available inside a loop
  body.
- `{% if condition %}...{% endif %}` — conditionals.
- `{{ variable | filter }}` — filters. Every filter available to a template
  — crank's built-ins and any declared under `filters` in the `.crank` file
  — is backed by a sandboxed [Rhai](https://rhai.rs) script. See "Built-in
  filters" and "Custom filters" below.
- Data, and the template's own text, can be in any language or include
  emoji, and is written out exactly as is. A *variable name* in a template
  is ASCII-only, though (Tera's syntax), so a data key like `名前` can't be
  written as `{{ 名前 }}` — reach it by looping over a mapping instead
  (`{% for key, value in labels %}`).
- Referencing a variable that isn't in the data file is a render error, not
  a silent empty string — this is deliberate, so a data/template mismatch
  is caught immediately rather than producing quietly-wrong output.
- `{{ crank.filename }}` — the name of the file being written, from the
  reserved `crank` variable. See "The `crank` variable" below.

See [Tera's own documentation](https://keats.github.io/tera/) for the
full template syntax (crank doesn't restrict or extend it beyond its
filters).

Before writing a custom filter, check
[Tera's own built-in filter list](https://keats.github.io/tera/#filters) —
it already covers most common cases (`upper`, `lower`, `trim`, `truncate`,
`join`, `sort`, `default`, and many more), and every one of them is
available in a crank template with no configuration. A custom filter is
for what Tera doesn't already provide.

### The `crank` variable

`crank` is a reserved template variable: crank fills it in with what it
knows about the file being rendered, and a data file can't define a
top-level key of that name (see "The data file" above). It has one
property:

- `crank.filename` — the last component of the task's rendered `output`
  path: `widget.rs` for `output: "src/{{ name }}.rs"` with `name: widget`.
  Each item of an `array` group gets its own.

That's handy for a header naming the file it's in:

```text
// {{ crank.filename }} — generated by crank, do not edit.
```

`crank` is only available in a template's body, not in `output` itself,
since `crank.filename` comes from the rendered `output`; using it there is
an undefined-variable render error. More properties may be added to
`crank` later.

### Built-in filters

| Filter | Description | Script |
| --- | --- | --- |
| `snake_case` | Converts a string to snake_case, keeping letters and digits in any script and splitting on a camelCase boundary as well as spaces/punctuation/emoji | [`src/filters/snake_case.rhai`](../src/filters/snake_case.rhai) |
| `wrap` | Word-wraps a string at a given column width | [`src/filters/wrap.rhai`](../src/filters/wrap.rhai) |

These also double as working reference examples of the two filter
conventions below, without duplicating anything Tera already ships.

Built-in filter names are reserved: declaring a custom filter under one of
these names in `filters` is a `.crank` error (see "Errors" below). A name
that instead collides with one of *Tera's* own built-in filters isn't
caught this early — Tera's list isn't something crank tracks — but fails
loudly the first time that filter set is used to render anything, rather
than silently shadowing Tera's version (see "Errors" below).

### Custom filters

A `.crank` file can declare its own filters under a root-level `filters`
key, each backed by a `.rhai` script (path relative to the `.crank` file's
own directory):

```yaml
filters:
  backwards: backwards.rhai
```

A filter's name must start with an ASCII letter or `_`, followed by ASCII
letters, digits, or `_` — Tera's template syntax can't call anything
else, so any other name (e.g. `odwróć`) is a `.crank` error (see "Errors"
below). The script's own path can use any characters.

The script must define a function named exactly `filter`, in one of two
forms:

- `fn filter(value)` — a plain filter taking one value and returning one
  value. The value can be any shape a Tera context can hold — string,
  number, bool, array, map, or null. `backwards.rhai` (deliberately not
  called `reverse` — Tera already has a built-in filter under that name,
  see above):

  ```rhai
  fn filter(value) {
      let result = "";
      for ch in value {
          result = ch.to_string() + result;
      }
      result
  }
  ```

  Used in a template exactly like a built-in filter: `{{ name | backwards }}`.

- `fn filter(value, args)` — a filter that also accepts Tera's named
  filter parameters (e.g. `{{ value | truncate(length=10) }}` in Tera's own
  syntax), passed as `args`, a Rhai object map from parameter name to
  value. A missing key doesn't error on its own — `args.get("key")` just
  returns `()` — so check with `args.contains("key")` up front: `throw` a
  specific message for a required parameter, or fall back to a default for
  an optional one. crank's own [`wrap.rhai`](../src/filters/wrap.rhai) does
  both, and is a working reference example:

  ```rhai
  fn filter(value, args) {
      if !args.contains("width") {
          throw "wrap filter requires a `width` named parameter, e.g. `wrap(width=20)`";
      }
      let width = args.get("width");
      let indent = if args.contains("indent") { args.get("indent") } else { "" };
      // ...
  }
  ```

  Used with parameters exactly like a Tera built-in: `{{ text |
  wrap(width=20, indent="// ") }}`.

Rhai has no Unicode character classes of its own, so crank adds three
methods on a character to every filter script: `ch.is_alphanumeric()` (a
letter or digit in any script), `ch.is_uppercase()`, and `ch.is_mark()`
(a combining mark, such as the accent in a decomposed "é"). Iterating a
string (`for ch in value`) visits each Unicode code point, and `len()`
counts them, not bytes — so a multi-code-point emoji like 👩‍👩‍👧‍👦 is
several characters to a script.

Which form a script defines is detected automatically (Rhai dispatches
functions by name and parameter count), so a plain filter never needs to
know about `args` at all. crank's own
[`snake_case`](../src/filters/snake_case.rhai) script is a working
reference example of the plain form.

Each script runs in its own isolated Rhai engine, with no file or network
access and fixed resource limits (operation count, call depth, string/
array/map size) to stop a runaway script — there are no configuration
knobs for these limits.

Every filter declared in `.crank` is read and compiled once, right after
the `.crank` file itself is parsed and before any task group runs. A
missing/unreadable script, a script that fails to compile, or a name that
collides with a built-in filter fails the entire run (see "Errors" below).
A script erroring once a template actually calls it — no `filter` function,
wrong arity, a runtime error, or a return value with no Tera
representation — is an ordinary per-task render error, exactly like an
undefined template variable.

## Pocket markers

A template's output can mark a region as user-editable with a pair of
markers. Two pairs are built in and active at the same time:

- `// name [` ... `// ]`, meant to occupy whole lines:

  ```text
  // name [
  default content
  // ]
  ```

- `/* name [ */` ... `/* ] */`, meant to sit inline within a single line
  (its content can still span multiple lines, though):

  ```text
  /* name [ */ content /* ] */
  ```

A pocket's name is one or more letters, digits, combining marks,
underscores, or hyphens, in any script — `// notes-Żurek [` and
`// メモ [` are both pockets (an emoji isn't a letter, so `// 🍕 [` isn't
one). Any amount of whitespace is allowed between each marker's tokens (`//`,
`name`, and `[` on a `//` start marker, `//` and `]` on its end
marker; `/*`, `name`, `[`, and `*/` on a `/*` start marker, `/*`,
`]`, and `*/` on its end marker). On the first run, whatever's between a
pair of markers is rendered and written like the rest of the file,
markers included. On every later run, before writing a file, crank looks
up each of the task group's previous output paths (per
`.crank.manifest`), reads whichever of those files still exist, and scans
them for pockets by name, across every marker pair. If a pocket found in the
freshly rendered output shares a name with one found in an old file, the
old file's content is kept in place of the freshly rendered default — the
marker lines themselves are always the fresh render's. If no old pocket
shares its name (e.g. the very first run), the freshly rendered default
is used as-is.

There's no separate file or database backing this — a pocket's content is
*only* ever read back from whatever's already on disk in a task group's
previously generated output, scanned fresh on every run. A pocket found
in an old file that no longer appears anywhere in the current run's
output for that group (e.g. its marker was removed from the template) is
simply dropped: no warning, no error.

A pocket's name is unique across its **whole task group**, not just the
file it's in, and whichever markers it uses — a `//` pocket and a `/*`
pocket sharing a name collide exactly like two `//` pockets would. This
matters when templates get restructured: if a
pocket moves from one output file to another between runs (same name,
different file), crank still finds and transfers its content, because
every one of the group's previous output files is read and scanned up
front, before any of the group's new outputs are written this run — not
interleaved file-by-file, since an old file could otherwise be
overwritten before a later task claims its pocket. Reusing a pocket name
for two different markers within the same task group's current output —
whether in the same file or two different ones, and whether with the
same markers or different ones — is an error (see "Errors" below), not a
silent collision.

Pockets can't nest or overlap, including across different marker pairs —
e.g. a `/*` marker sitting inside a `//` pocket's default body is an
error, not a silently-ignored coincidence.

A pocket's name is itself literal text inside the template, so it can
use `{{ ... }}` substitutions to vary per file (e.g. for an `array`
group, `// custom-{{ name }} [`) — this "just works" through the same
Tera render that produces the rest of the template's output, with no
special handling needed.

### Your own pocket markers

`//` and `/* */` aren't comments in every language — not in Python, SQL,
YAML or shell, say. You can list your own markers instead, in two
places:

- **In `.crank`**, under a top-level `pocketMarkers` key, for every
  template in the project:

  ```yaml
  pocketMarkers:
    - start: '# {name} ['
      end: '# ]'
  taskGroups:
    - ...
  ```

- **In a template's crank header**, for just that template: a Tera
  comment at the very start of the template whose text starts with
  `crank:`, holding YAML:

  ```text
  {#- crank:
    pocketMarkers:
      - start: '-- {name} ['
        end: '-- ]'
  -#}
  -- Generated by crank.
  CREATE TABLE users (id TEXT);

  -- indexes [
  -- ]
  ```

  Tera skips comments, so the header never reaches the output (`{#-` and
  `-#}` also trim the whitespace around it, so it doesn't leave a blank
  line either). Only a comment that opens the file counts as a header —
  a `{# crank: ... #}` anywhere else is just a comment — and only
  `pocketMarkers` may appear under `crank:`.

A template's output uses the markers its header lists, if it lists any;
else the ones `.crank` lists, if it lists any; else the built-in ones.
So a mostly-Python project can set `#` markers once in `.crank`, and give
its one SQL template `--` markers in its own header.

Each item in `pocketMarkers` is a `start` and `end` marker, written as
literal text: characters like `[`, `*` or `.` mean themselves, any run of
whitespace matches any amount of whitespace (including none), and
whitespace at either end is ignored. `start` must contain `{name}`
exactly once, where the pocket's name goes, plus some other text; `end`
can't contain `{name}`. What a name can be — letters, digits, combining
marks, underscores and hyphens — is fixed; a marker only decides where it
goes.

A list of markers **replaces** whatever it overrides entirely — the
built-in markers, or `.crank`'s — rather than adding to it. With `#`
markers listed, neither `//` nor `/* */` is recognized any more. That's
usually what you want: in Python, `rows // PAGE_SIZES[0]` is floor
division, but with the built-in markers active it would read as the start
of a pocket called `PAGE_SIZES`. List more than one pair to use several
styles in the same file, or an empty list (`pocketMarkers: []`) for no
pockets at all.

Different templates in the same task group can use different markers, and
a pocket's content can still move between their outputs: it's the content
between the markers that moves, never the markers themselves.

Because a previously generated file must be read back using the markers
it was actually written with — not necessarily what `.crank` or its
template lists *now* — `.crank.manifest` records each output's markers
whenever they weren't the built-in ones (see above). So a file's
hand-edited pockets are still found after its markers change, or are
removed entirely.

## Examples

| Behavior | Test |
| --- | --- |
| A task group with one task | [`parses_a_single_group_with_a_single_task`](../src/crank_file.rs#L494-L520) |
| A data file's nested maps/lists/scalars | [`parses_nested_maps_lists_and_scalars`](../src/data.rs#L129-L145) |
| A `#selector` picks a sub-object out of a data file | [`selects_a_key_then_index`](../src/selector.rs#L593-L600) |
| An `array` field parses, with a `#selector` picking a sub-array out of a data file | [`parses_an_array_group_with_a_selector`](../src/crank_file.rs#L755-L771) |
| A `#selector` picks a sub-array out of a data file | [`selects_a_nested_array_via_a_key`](../src/selector.rs#L660-L668) |
| A data file's whole top-level sequence converts to an `array` group's items | [`a_top_level_sequence_converts_to_items`](../src/data.rs#L166-L172) |
| An `array` group task's `output` with no `{{ ... }}` substitution is a `.crank` parse error | [`an_array_group_task_output_with_no_substitution_is_a_parse_error`](../src/crank_file.rs#L790-L801) |
| An `output` starting with an unquoted `{{ ... }}` is a `.crank` parse error saying to quote it | [`an_output_starting_with_an_unquoted_substitution_is_a_parse_error_saying_to_quote_it`](../src/crank_file.rs#L816-L828) |
| A key other than `.crank`'s own, at any level, is a `.crank` parse error | [`an_unknown_key_at_any_level_is_a_parse_error`](../src/crank_file.rs#L1018-L1042) |
| Keys starting with `x-` are skipped, at any level | [`keys_starting_with_x_are_ignored_at_every_level`](../src/crank_file.rs#L1051-L1074) |
| An `x-` key holds an anchor that aliases reuse | [`an_x_key_can_hold_an_anchor_that_aliases_reuse`](../src/crank_file.rs#L1076-L1096) |
| A `{{ variable }}` substitution | [`substitutes_a_plain_variable`](../src/render.rs#L169-L174) |
| `crank.filename` is the rendered `output` file's name | [`crank_filename_is_the_rendered_output_files_name`](../src/run.rs#L2582-L2594) |
| A data file defining a top-level `crank` key is an error | [`a_top_level_crank_key_is_reserved`](../src/data.rs#L232-L240) |
| A loop using `loop.last` | [`loops_over_an_array_using_loop_last`](../src/render.rs#L176-L187) |
| A loop over a mapping | [`loops_over_a_map`](../src/render.rs#L189-L203) |
| The `snake_case` filter | [`applies_the_snake_case_filter_via_the_builtin_pipeline`](../src/render.rs#L205-L215) |
| The `wrap` filter with a named parameter | [`applies_the_wrap_filter_with_a_named_parameter_via_the_builtin_pipeline`](../src/render.rs#L217-L227) |
| A filter colliding with a Tera built-in of the same name is a render error | [`a_filter_colliding_with_a_tera_built_in_is_a_render_error`](../src/render.rs#L229-L238) |
| A custom Rhai filter | [`applies_a_custom_rhai_filter`](../src/render.rs#L270-L276) |
| A custom Rhai filter with a named parameter | [`a_custom_filter_can_use_a_named_parameter`](../src/render.rs#L302-L311) |
| A render error's message includes the underlying cause (e.g. the missing variable's name), not just Tera's generic top-level message | [`a_render_errors_message_includes_the_underlying_cause`](../src/render.rs#L253-L268) |
| Determining which previously generated outputs are now stale | [`stale_returns_only_entries_missing_from_current`](../src/manifest.rs#L407-L419) |
| Old content is substituted for a pocket's fresh default | [`old_content_is_substituted_for_the_fresh_default`](../src/pocket.rs#L782-L799) |
| A pocket found in a different old file than the one it appears in now still substitutes (the moved-between-files case) | [`a_pocket_found_in_a_different_old_file_than_the_fresh_one_still_substitutes`](../src/pocket.rs#L801-L818) |
| A pocket in an old file that's absent from the fresh render is dropped silently | [`an_orphaned_old_pocket_is_silently_dropped`](../src/pocket.rs#L820-L834) |
| A `/* name [ */` ... `/* ] */` pocket round-trips its fresh default content | [`short_form_first_run_with_no_old_pockets_keeps_the_fresh_default_content`](../src/pocket.rs#L941-L947) |
| A `//` pocket and a `/*` pocket coexist in the same rendered text, each substituting independently | [`long_form_and_short_form_coexist_in_the_same_text`](../src/pocket.rs#L973-L994) |
| A `/*` marker overlapping a `//` pocket's body is a per-task error | [`overlapping_markers_from_different_forms_is_an_error`](../src/pocket.rs#L1006-L1015) |
| `.crank` lists pocket markers for every template without its own | [`crank_files_markers_apply_to_a_template_without_its_own`](../src/run.rs#L2521-L2536) |
| A template's header markers win over `.crank`'s | [`a_templates_header_markers_win_over_the_crank_files`](../src/run.rs#L2538-L2563) |
| A template's header lists its own pocket markers | [`a_header_lists_pocket_markers`](../src/header.rs#L141-L148) |
| A marker is literal text, with any run of whitespace matching any amount of it | [`a_pattern_compiles_to_its_text_with_whitespace_as_any_amount`](../src/pocket.rs#L574-L582) |
| Listed markers replace the built-in ones entirely | [`listed_markers_replace_the_built_in_ones_entirely`](../src/pocket.rs#L615-L620) |
| Python's floor division isn't read as a pocket once other markers are listed | [`floor_division_in_python_isnt_a_pocket_once_the_built_in_markers_are_replaced`](../src/pocket.rs#L622-L633) |
| A `.crank.manifest` entry records the pocket markers a file was written with | [`parses_a_path_pocket_markers_mapping_entry`](../src/manifest.rs#L257-L267) |
| An old file is scanned with the markers `.crank.manifest` records for it, not its template's current ones | [`an_old_file_is_scanned_with_the_markers_the_manifest_records_not_its_templates_new_ones`](../src/run.rs#L2430-L2449) |
| An output already holding exactly what is generated isn't rewritten | [`an_output_already_holding_what_is_generated_is_not_rewritten`](../src/run.rs#L1388-L1405) |
| A rerun with nothing to change writes nothing, not even `.crank.manifest` | [`a_rerun_with_nothing_to_change_writes_nothing`](../src/run.rs#L1407-L1421) |

## Errors

| Situation | Result |
| --- | --- |
| No `.crank` file in the target directory | Prints `No crank file found`, exits `0` |
| `.crank` file isn't readable, or isn't valid YAML | Error naming the `.crank` file, exits `1` |
| A `filters` entry's script isn't readable, fails to compile, or its name collides with a built-in filter or isn't one a template can call (not an ASCII identifier) | Error naming the filter, exits `1` before any task group runs (checked together — every broken filter is reported, not just the first) |
| An `object`, `array`, `template` or filter script path is absolute or climbs out of the `.crank` file's directory with `..` | Error naming the field and the path, exits `1` before any task group runs (checked as part of parsing `.crank`) ([`a_template_outside_the_directory_is_a_parse_error`](../src/crank_file.rs#L985-L995)) |
| `.crank` has a key it doesn't define, at any level, that doesn't start with `x-` — including YAML's `<<` merge key | Error naming the key and where it is (e.g. `taskGroups[0].tasks[1]`), exits `1` before any task group runs (checked as part of parsing `.crank`, and reported ahead of the missing field a misspelled key leaves) ([`a_wrongly_named_key_is_reported_rather_than_the_field_it_leaves_missing`](../src/crank_file.rs#L1118-L1126)) |
| `.crank` has no `taskGroups` key — including an empty file, or one holding only comments | Error saying `taskGroups` is missing, exits `1` before any task group runs; nothing is written or deleted ([`a_missing_task_groups_key_is_a_parse_error`](../src/crank_file.rs#L675-L684)) |
| A task group is missing `name` | Error naming the `.crank` file, exits `1` before any task group runs (checked as part of parsing `.crank`) |
| Two task groups share the same `name` | Error naming the duplicate `name`, exits `1` before any task group runs (checked as part of parsing `.crank`) |
| A task group specifies both `object` and `array`, or neither | Error naming the task group, exits `1` before any task group runs (checked as part of parsing `.crank`) |
| An `array` group's task `output` has no `{{ ... }}` substitution | Error naming the task's `output`, exits `1` before any task group runs (checked as part of parsing `.crank`, alongside the check above) |
| A task's `output` or `template` starts with an unquoted `{{ ... }}`, so YAML reads it as a mapping | Error naming the field and saying to quote the value, exits `1` before any task group runs (checked as part of parsing `.crank`) |
| `.crank`'s `pocketMarkers` can't be used (the same rules as a template's markers, below) | Error naming `.crank` and the problem, exits `1` before any task group runs |
| A template's crank header isn't valid YAML, is never closed with `#}`, or has a key other than `crank:` at its top or `pocketMarkers` under it — or a marker in it lacks `start` or `end`, has a `start` without exactly one `{name}` (or with nothing else), or has an empty `end` or one containing `{name}` | Error naming the template and the problem; that task (or, for an `array` group, that item's task) is skipped |
| A task group's previous output file (per `.crank.manifest`) isn't readable while scanning for pocket content | Error naming the file; that group's tasks are skipped |
| A `.crank.manifest` entry's recorded `pocketMarkers` can't be used (the same rules as a template's) | Error naming the file; that group's tasks are skipped (same treatment as an unreadable previous output) |
| A task group's previous output file has a malformed pocket marker (unclosed, a stray end marker, or two markers overlapping — same or different marker pairs) while scanning for old content | Error naming the file and the problem; that group's tasks are skipped |
| A task group's `object`/`array` file isn't readable or isn't valid YAML | Error naming the data file; that group's tasks are skipped |
| A task group's `object` selector doesn't resolve (a missing key, an out-of-range index, a step applied to the wrong shape of value, or the final result isn't a mapping) | Error naming the data file and the selector; that group's tasks are skipped ([`index_out_of_range_is_a_select_error`](../src/selector.rs#L623-L628)) |
| A task group's `array` selector doesn't resolve, or resolves to something other than a sequence (e.g. a mapping) | Error naming the data file and the selector; that group's tasks are skipped |
| An `array` field with no `#selector` whose data file's top level isn't a sequence | Error naming the data file; that group's tasks are skipped |
| An `array` group's item isn't a mapping | Error naming the data file and the item's index; that one item's tasks are skipped, every other item/task/group still runs |
| A task group's data (its `object`, or an `array` group's item) has a top-level key named `crank`, which is reserved | Error naming the data file (and, for an `array` group, the item's index) and saying `crank` is reserved; that group's tasks (or that item's tasks) are skipped |
| A task's `template` file isn't readable, has malformed Tera syntax, or fails to render (e.g. an undefined variable, a custom filter erroring on its input, or a declared filter's name colliding with one of Tera's own built-ins) | Error naming the template file, followed by the full chain of underlying causes (e.g. the specific variable name that's missing) — Tera's own error type only exposes that detail via its `source()` chain, not its top-level message, so crank walks and prints it; that task (or, for an `array` group, that item's task) is skipped |
| A task's `output` fails to render as a Tera template (e.g. an undefined variable) | Error naming the raw `output` text, same treatment as a template body render failure; that task (or item's task) is skipped |
| A task's freshly rendered output has a malformed pocket marker (unclosed, a stray end marker, or two markers overlapping — same or different marker pairs) | Error naming the output path and the problem; that task (or item's task) is skipped |
| A task's freshly rendered output reuses a pocket name already produced elsewhere in the same task group | Error naming the output path and the pocket name; that task (or item's task) is skipped |
| A pocket name matches more than one location among the task group's previous output files | Error naming the output path, the pocket name, and every old file it was found in; that task (or item's task) is skipped |
| A pocket found in an old output file no longer appears anywhere in this run's fresh output for that group | Silently dropped — no error, no warning |
| A task's rendered `output` is absolute or climbs out of the `.crank` file's directory with `..` — from the `.crank` file itself or from the data it substitutes | Error naming the rendered path; nothing is written for that task (or item's task), every other task/item still runs ([`an_output_outside_the_directory_is_a_per_task_error`](../src/run.rs#L2264-L2285)) |
| A file crank is about to read, write, create a directory for, or delete leads outside the `.crank` file's directory through a symbolic link (or is a link whose target doesn't exist) | Treated as that file being unreadable/unwritable/undeletable: the error names the path and says it leads outside through a symbolic link, with the same result as the matching row here (e.g. that task is skipped; a stale output stays recorded in `.crank.manifest`) |
| Two tasks render `output` paths naming the same file — the same path, or one differing only in `.`/`..` components, letter case, or how an accented letter is encoded — whether in different task groups, or two items of one `array` group | Error naming both paths and the task (group, item index for an `array` group, and template) each came from; nothing is written and nothing deleted, not even the other tasks' outputs or `.crank.manifest` ([`two_tasks_generating_the_same_output_write_nothing`](../src/run.rs#L2671-L2684)) |
| A stale output (recorded in `.crank.manifest` but no longer produced by that path) is the same file as one this run produced — `./out.txt` last run and `out.txt` now, `Out.txt` and `out.txt` on a case-insensitive filesystem, or an accented letter encoded differently on macOS | Not stale: it's left alone and dropped from `.crank.manifest`, which records the new path ([`an_output_renamed_in_case_on_a_case_insensitive_filesystem_is_not_stale`](../src/run.rs#L2879-L2905)) |
| crank can't tell whether a stale output is the same file as one this run produced | Error naming both paths; that entry stays recorded in `.crank.manifest`, and nothing is deleted for it |
| A task's `output` can't be written | Error naming the output path |
| `.crank.manifest` exists but isn't readable, isn't valid YAML for its `version`/`taskGroups` shape (e.g. with no `version`), or records a `version` other than `1` | Error naming `.crank.manifest`, exits `1` before any task group runs |
| A `.crank.manifest` entry is absolute or climbs out of the `.crank` file's directory with `..` | Error naming every such entry, exits `1` before any task group runs; the file it names is never read or deleted, and `.crank.manifest` is left as it is, for the entry to be removed by hand ([`a_manifest_entry_outside_the_directory_is_an_error_and_never_deleted`](../src/run.rs#L2311-L2340)) |
| A stale output (recorded in `.crank.manifest` but no longer produced) is already gone, e.g. deleted by hand | Not an error: it's dropped from `.crank.manifest`, but not listed as removed (and `--check` doesn't list it as something that would be removed) |
| A stale output (recorded in `.crank.manifest` but no longer produced) can't be deleted | Error naming the output path; that entry stays recorded in `.crank.manifest` so the next successful run retries it |

Whenever at least one error occurred, crank prints every accumulated error
and exits `1`. Otherwise it prints a success summary listing every file it
wrote (and, if any stale outputs were deleted this run, a second summary
listing them) and exits `0`.

Under `--check`, the same accumulated-error rules above still apply — the
run is aborted on any error before the created/modified/stale-removal
summary is produced, exactly as it would be without `--check`. See
"Checking for pending changes" in [`usage.md`](usage.md#checking-for-pending-changes---check) for `--check`'s own exit code rules
when no error occurred.
