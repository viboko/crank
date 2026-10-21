# crank playground

A web page for trying crank in the browser: a pretend project on the
left, the open file in the middle, and crank's output below. Every file
is editable, and crank re-runs after every edit, to any file — and what
it writes and deletes is applied to the project, just as crank changes a
real directory. The files it generates are marked "generated"; editing
one re-runs crank too, which puts it straight back, unless the edit is
inside a pocket. Unticking "Run on every edit", above the output,
makes crank wait for its "Run crank" button (or Ctrl+Enter) instead;
until then the output, exit status and "generated", "error" and
"missing" marks are hidden, since they describe the files as they were.
It runs crank itself, compiled to
WebAssembly, through the same library code the CLI runs over a real
disk — so it prints and generates exactly what `crank` would for the
same files. Nothing typed into it leaves the browser; the published page
only counts visits, with [GoatCounter](https://www.goatcounter.com/),
which sets no cookies and never sees a shared link's `#fragment`.

It's published at <https://crank.viboko.dev/>.

It also holds the playground's examples, which double as tests (see
[below](#examples)).

## How it fits together

- crank's whole run — reading `.crank`, rendering every task, writing
  outputs, reconciling `.crank.manifest` — lives in
  [`src/run.rs`](../src/run.rs), over a `Workspace` trait rather than the
  disk. The CLI's `main.rs` implements it for the real filesystem;
  `crank::run::MemoryWorkspace` implements it in memory.
- [`src/lib.rs`](src/lib.rs) loads the page's files into a
  `MemoryWorkspace`, runs `crank::run::run` over it as `crank` would in
  the project's root, and records which files the run wrote and which
  stale outputs it deleted. It returns those with the output lines from
  `crank::output`, styled but uncolored, and the page applies them to its
  tree. The page calls it through two functions exported to
  JavaScript, `run` and `examples`, exchanging plain JSON.
- [`web/`](web) is the page: plain HTML, CSS and JavaScript, with no
  dependencies and no build step of its own, and the same look as
  flint's playground. Its editor is a
  `<textarea>` over a highlighted copy of its own text (YAML, Tera, Rhai
  and pocket markers), so typing, selection and undo are the browser's
  own. Errors are located from crank's own output: the file each one
  names is marked in the tree, a line it gives (Tera's
  `--> template:LINE:COLUMN`, or YAML's `line N`) is marked in the
  editor, and a file crank couldn't find appears as "missing" where it
  should be, for you to create with the New file button.
  Whenever a folder has no files anywhere inside it, a "Remove empty
  folders" button under the tree removes all such folders at once.
- [`web/favicon/`](web/favicon) is the page's icon set: `favicon.svg`,
  which is the logo in [`docs/images/crank-logo.svg`](../docs/images/crank-logo.svg)
  framed square, and the PNG files and `favicon.ico` rendered from it by
  `just playground-favicons` (which needs ImageMagick).
- This crate is its own workspace, so the CLI's dependencies stay what
  the CLI needs. Its [`Cargo.lock`](Cargo.lock) started as a copy of the
  CLI's, so both build the same tera, rhai and YAML versions — keep them
  in step when bumping those.

## Shared links

"Copy link" puts the project's files in the link's fragment, so a link
needs no server and keeps working as long as the site exists:
`#tree=<version>.<data>`. Version 1's `<data>` is the JSON
`{ "tree": [{ "path", "directory"?, "contents"? }], "open" }`, compressed
with raw DEFLATE and encoded as base64url without padding.

A link only holds the files crank needs: the page re-runs crank when
the link opens, so whatever crank would generate the same is left out,
along with folders that leaves empty. A generated file crank wouldn't
generate the same — one with its own contents in a pocket — stays, and
`.crank.manifest` stays with it, since that's how crank finds a file's
old pockets. (Older links may hold every file; they open just the same.)

The version is what keeps old links opening: **never change what an
existing version means.** A new encoding gets the next version number
and its own decoder in `SHARE_DECODERS` in [`web/app.js`](web/app.js),
and the old decoders stay. A link only stores crank's inputs, so an old
link re-runs on whatever crank the site has now.

## Downloading

"Download" saves the same files a link would hold as a zip, inside one
folder, to run `crank` on yourself. With an example loaded, edited or
not, the zip and its folder are named after it
(`crank-example-08-pockets.zip`); otherwise they're `crank-example`.
The page writes the zip itself, compressing with the browser's
`CompressionStream`, so it still needs no dependencies.

## Running it locally

```bash
just playground-serve
```

That builds the site into `target/site/` (`just playground-build` alone just
builds it) and serves it at <http://localhost:8002>. It needs the
`wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` at exactly
the version [`Cargo.lock`](Cargo.lock) pins for the `wasm-bindgen`
crate; `just install-tools` installs both, and `just playground-build` says
which version to install if it finds the wrong one.

## Publishing

[`playground.yml`](../.github/workflows/playground.yml) builds the site
on every PR that could change it, and
[`playground-main.yml`](../.github/workflows/playground-main.yml) builds
it on each such push to `main` and publishes it to the root of the
[GitHub Pages site](../docs/github-pages.md), which also hosts coverage
and benchmark reports. A PR's build is published as a preview at
`/pr-<N>/playground/` (same-repo PRs only), and the PR gets a comment
linking to it; it's removed when the PR closes. Every path in the
playground is relative, so it works there unchanged.

The site's domain, `crank.viboko.dev`, is set by [`web/CNAME`](web/CNAME):
with Pages deploying from a branch, GitHub keeps a custom domain as a
`CNAME` file at the branch's root, and since each publish replaces
everything there but `pr-*/`, `coverage/` and `benchmarks/`, the
playground has to bring its own. The canonical link and Open Graph tags
in [`web/index.html`](web/index.html), [`web/robots.txt`](web/robots.txt)
and [`web/sitemap.xml`](web/sitemap.xml) all name the same domain; `just
seo` checks they agree. `robots.txt` keeps search engines out of the
reports and PR previews.

## Examples

The playground's example buttons load the examples in
[`examples/`](examples). Each is checked by `just test`, which runs
crank over it and fails if its documented output or generated files are
wrong, so they can't go stale.

Each example is one Markdown file in [`examples/`](examples), shown in
file name order — so name them with a two-digit prefix, like
`04-one-file-per-item.md`. It reads as normal Markdown on GitHub, and
follows a fixed shape so it can also be loaded and tested:

````markdown
# A first template

A short explanation of what the example shows. Plain Markdown: this is
everything between the title and the first `##` section.

## `.crank`

```yaml
taskGroups:
  - name: widget
    object: object.yaml
    tasks:
      - template: output.txt.tera
        output: output.txt
```

## `object.yaml`

```yaml
name: widget
```

## `output.txt.tera`

```text
Hello, {{ name }}!
```

## Output

```text
Generated 1 file
  - output.txt (created)
```

## Generated `output.txt`

```text
Hello, widget!
```
````

- **One section per starting file**, headed by its path in backticks.
  Its contents are the code block's lines, each ending in a newline.
  The playground runs crank once as soon as an example loads, so a
  starting file can be one crank will rewrite — a previously generated
  file and the `.crank.manifest` that lists it, to show pockets keeping
  their hand-written content.
- **`## Output`** is exactly what that first run prints, stdout and
  stderr in order, without colors.
- **One ``## Generated `path` `` section per file crank creates or
  changes**, `.crank.manifest` aside, holding what it writes there. These
  aren't loaded into the playground; they're there for readers, and for
  the tests to check.

Each section holds just one fenced code block; use a longer fence
(` ```` `) for a file that itself contains ` ``` `. To fill in or
update an example's output, run the tests (`just test`) and copy the
actual output from the failure — after checking it really is what the
example should show.
