# Benchmarking crank

This is dev tooling for catching accidental performance regressions, not
a behavior change — it doesn't need the `README.md`/`docs/usage.md`/
`CHANGELOG.md` updates described in [`AGENTS.md`](../AGENTS.md)'s
docs-honesty rule. If a change here ever *does* change crank's observable
behavior, that change needs those updates as normal.

## What's measured, and what isn't

The benchmarks exercise the dimensions a crank project actually grows
along: how many task groups its `.crank` file has, how much data a
template loops over, how many pockets a generated file carries, and how
many files a run generates.

There are two tiers, both under `benches/`. They measure different
things with different tools, and CI treats them differently (see "CI"
below):

- **`benches/logic.rs`** — instruction counts, with
  [Gungraun](https://github.com/gungraun/gungraun). Calls crank's
  library directly with synthetic in-memory data — `crank_file::parse`,
  `render::render`, `pocket::apply`, and a whole `run::run` over a
  `MemoryWorkspace` — under [Valgrind](https://valgrind.org/), which
  counts the CPU instructions each call executes. Each benchmark runs
  once, and an unchanged build gives nearly the same count on every run,
  on any machine — so a PR that raises a count by more than 1% fails
  its check. This is where crank's actual logic lives, so it's the
  primary regression signal. What it can't see is time itself: a change
  can execute the same number of instructions and still be slower (by
  missing the CPU's cache more often, say), and nothing the operating
  system does on crank's behalf is counted. Valgrind doesn't run on
  macOS, so there these need Docker (see "Running locally").
- **`benches/e2e.rs`** — wall-clock time, with
  [Criterion](https://github.com/bheisler/criterion.rs). Spawns the real
  compiled `crank` binary (`env!("CARGO_BIN_EXE_crank")`, not the
  `assert_cmd` crate — this is a benchmark, not a Rust-level integration
  test, so it doesn't blur `AGENTS.md`'s testing-split rule) against
  real generated projects that write 10 / 100 / 1,000 files. This exists
  because in-memory benchmarks alone can't prove the actual disk path is
  efficient — a regression there (an accidental re-read of every file,
  non-linear scaling in writing outputs, etc.) would be invisible to
  `benches/logic.rs`, and most of its cost is inside the operating
  system, where Valgrind can't count. Timings on a shared CI runner are
  noisy, so this tier is reported on each PR but never fails it.

Deliberately **not** built: an automated Big-O classifier. Fitting a
runtime series to a complexity class is a known but noisy technique —
useful for a human eyeballing a graph, unreliable as an automated CI gate
on shared-runner data. Use the graph (below) for that judgment call.

## What each benchmark covers

A crank run reads the `.crank` file (`crank_file::parse`), renders each
task's template against its data (`render::render`), carries over any
pockets from the previous output (`pocket::apply`), and writes the
results. The benchmarks below cover the first three steps in isolation,
a whole run in memory, and the whole program end to end. What the size
in each name counts is noted in its description.

The first four are the instruction-count benchmarks, named as Gungraun
prints them (after its `logic::logic::` prefix, and with the size in
place of `<n>`).

### `parse_crank_file` `n_<n>`

Reads a `.crank` file of `n` task groups, each with one task, into its
parsed form. Sizes 100, 1,000 and 10,000. Nothing is rendered.

### `render_loop` `n_<n>`

Renders a template that loops over `n` items of a data file, running
each item's name through the built-in `snake_case` filter (a Rhai
script) and using `loop.last`. Sizes 100, 1,000 and 10,000. Measures
Tera's rendering plus crank's filter calls, which dominate as data
grows.

### `apply_pockets` `n_<n>`

Carries old content over into a freshly rendered file holding `n`
line-form pockets, every one of which has old content to keep. Sizes
100, 1,000 and 10,000. Measures finding the markers and splicing the
old content in.

### `run_array` `n_<n>`

A whole run, in memory, of a project with one `array` task group of `n`
items: parse `.crank`, load the data, render and write one file per
item (each with a pocket), and reconcile `.crank.manifest`. The project
has already been run once, so this run also reads back every previous
output for its pockets. Sizes 10, 100 and 1,000.

### `crank binary (real directory)`

The wall-clock benchmark. This heading must match its Criterion ID
exactly: `.github/scripts/bench-report.py` reads this section to let
readers expand a benchmark's description in the PR comment, and flags
any Criterion benchmark without one. Add a heading here whenever you
add one.

Runs the real compiled `crank` program, exactly as a user would, on a
real temporary project like `run_array`'s, generating one file per item.
Size = number of files, at 10, 100 and 1,000. The project is run once
before timing starts, so each timed run finds its previous outputs on
disk. Covers what the in-memory benchmarks skip — process startup,
reading files from disk and writing them back — so it's the slowest and
noisiest of the five.

## Running locally

### Instruction counts

```bash
just bench-logic          # Linux
just bench-logic-docker   # macOS, or anywhere else with Docker
```

`bench-logic` needs Valgrind, and `gungraun-runner` at exactly the
version of the `gungraun` crate in `Cargo.lock` — the recipe checks, and
prints the `cargo install` command if it's missing or out of date.
`bench-logic-docker` runs the same thing in a Linux container that it
builds with both installed; its build output and saved baselines live
in the `crank-bench-logic` Docker volume rather than `target/`. A run
takes a few minutes, most of it in the largest `render_loop` and
`run_array` benchmarks.

To compare your branch against `main`:

```bash
git checkout main
just bench-logic -- --save-baseline=main
git checkout <your-branch>
just bench-logic -- --baseline=main
```

Gungraun prints each benchmark's counts beside the baseline's, with the
change as a percentage. `Instructions` is the figure CI checks. The
others come from Valgrind's simulation of a CPU cache: `Estimated
Cycles` weights cache misses to approximate time, and is worth a look
when a change moves data around without changing how much work is done.
To fail on a regression the way CI does, add
`--callgrind-limits='ir=1%'`.

Counts are only comparable between runs in the same environment: a
different Rust, Valgrind or C library version changes them, so a
baseline saved by `bench-logic` means nothing to `bench-logic-docker`,
or after the Rust version in
[`rust-toolchain.toml`](../rust-toolchain.toml) changes.

### Wall-clock time

```bash
just bench
```

To compare two points in history (e.g. your branch against `main`):

```bash
git checkout main
just bench -- --save-baseline main
git checkout <your-branch>
just bench -- --baseline main
```

`--baseline <name>` (unlike `--save-baseline`, see the CI section below)
compares against that named baseline directly without overwriting it, so
this two-step sequence already gives you a real, complete comparison —
Criterion prints a percentage change per benchmark, using its own
statistical model (bootstrapped confidence intervals) to distinguish a
real change from run-to-run noise, and `target/criterion/report/index.html`
renders the same comparison as an HTML report (see "How to read the
wall-clock report" below).

Neither recipe is part of `just ci`, `just lint`, or `just test-all`:
the wall-clock benchmark is slower and noisier than the checks that
gate merges, and the instruction counts can't run everywhere `just ci`
does. CI runs both separately — see "CI" below.

## How to read the wall-clock report

**The HTML report** (`target/criterion/report/index.html`, or
per-benchmark under `target/criterion/<group>/<id>/report/index.html`):

- The violin plot shows the distribution of sample timings — a tighter,
  more symmetric shape means a cleaner measurement; a wide or
  multi-lobed shape means more noise (background load, thermal
  throttling, etc.) and the number should be trusted less.
- Once a real comparison is in play (a `--baseline` run, or CI's
  regenerated comparison — see below), the report adds a "Change: +X%"
  or "-X%" line and a regression/improvement line plot overlaying the
  new run against the baseline. Criterion only calls this out as an
  actual regression/improvement (as opposed to "Change within noise
  threshold") when the statistical test clears its significance
  threshold — a small percentage change with that caveat is expected
  noise, not a real signal.

**The PR comment** (built by `.github/scripts/bench-report.py`): one
row per benchmark, one column per size, each cell the percentage change
in mean time from the PR's base to the PR. The marks are Criterion's own
verdict, read from the same report page the number links to:

- 🔴 **regressed** / 🟢 **improved** — the change passed Criterion's
  statistical test *and* its whole confidence interval cleared the
  noise threshold ("Performance has regressed/improved"). CI raises
  that threshold from Criterion's default ±1% to ±10%
  (`NOISE_THRESHOLD` in `.github/workflows/bench.yml`), for the reason
  below.
- no mark — "No change in performance detected" or "Change within noise
  threshold". The number shown is Criterion's best estimate, so it can
  be past the threshold and still unmarked, when the interval reaches
  back inside it (e.g. +12.3%, with an interval of +8.9% to +16.3%).
  Hover over any number to see its interval and Criterion's verdict.
- `new` — nothing on the base branch to compare against (e.g. a
  benchmark this PR adds).

Shared GitHub runners vary by several percent between identical runs.
Rebuilding also shifts benchmarks whose code didn't change: editing one
module can move where unrelated code lands in memory, which can speed
it up or slow it down by a few percent, consistently. (flint, whose
setup this is, saw one benchmark 7–9% slower at every size from a change
to an unrelated module; at ±1% that marked 13 of 18 cells 🔴, and ±10%
left exactly the 5 real regressions.) The cost is that a real
slowdown under 10% isn't marked — its number is still shown, so a
consistent few-percent change across sizes is still worth a look. A
change that grows with size (as a regression from O(n) to O(n²)
would) is a real signal; re-running the workflow settles a borderline
one. Click a benchmark's name for what
it covers (from "What each benchmark covers" above), or a number for
its full Criterion page.

## Where baseline data lives

`--save-baseline <name>` / `--baseline <name>` write to
`target/criterion/<group>/<benchmark-id>/<baseline-name>/` (`estimates.json`,
`sample.json`, etc.) — inside `target/`, so it's gitignored. This is why
the local before/after workflow above works: a plain `git checkout`
never touches gitignored/untracked paths (it only touches files that
differ between the two commits' trees), so `target/criterion` survives a
branch switch on its own. It does **not** survive `cargo clean` or
`git clean` — those explicitly remove untracked files, including it.

Gungraun's `--save-baseline=<name>` / `--baseline=<name>` work the same
way, under `target/gungraun/` — or, for `just bench-logic-docker`, in
the `crank-bench-logic` Docker volume (`docker volume rm
crank-bench-logic` clears it).

## CI

### Instruction counts: a gate

**`.github/workflows/bench-logic.yml`** runs on every PR. Its `gate`
job runs `.github/scripts/bench-gate.sh` (described below) over `src/`,
`Cargo.toml`, `Cargo.lock`, `benches/logic.rs` and the workflow itself,
and skips the rest if none of those changed beyond unit tests.
Otherwise the `instruction-counts` job installs Valgrind and
`gungraun-runner`, runs `just bench-logic` on the PR's base commit
(`--save-baseline=base`), then on the PR (`--baseline=base`) with a
limit of 1% on each benchmark's instruction count (`INSTRUCTION_LIMIT`
in the workflow). Any benchmark over the limit fails the job, and the
job's log names it. Both runs use the same runner and toolchain, so
the counts are comparable.

Why 1%: an unchanged build's counts vary a little between runs, wherever
a hash map is involved — crank keys pockets, filters and task groups in
maps whose hash is seeded randomly on each run — and otherwise by a few
instructions in millions. 1% clears that comfortably. A change can also
shift counts in code it didn't touch, by changing what the compiler inlines — rarely by this
much, but if the check fails on code the PR doesn't go near, that's the
likely cause. There is no way to override the check from within a PR
except by editing the limit, which shows up in the diff: if a slowdown
is the price of a change worth making, raise the limit in that PR, say
why, and lower it again afterwards.

The job is skipped, not failed, on a PR that can't have changed
performance, and GitHub counts a skipped job as passing. That's what
makes it usable as a **required status check**, which is how it should
be set up (`instruction-counts`, in the ruleset for `main`). It passes
without comparing anything when the base commit's benchmarks don't run
— a PR that updates Gungraun, for instance, since the runner installed
is the PR's version. The job's log and a warning annotation say so.

### Wall-clock time: a report

Two workflows for PRs, split so that PRs from forks are handled
safely, plus one for `main`:

- **`.github/workflows/bench.yml`** runs on every PR that touches
  `src/` or `Cargo.lock` (a dependency update can change performance
  too). A `gate` job runs `.github/scripts/bench-gate.sh`, which
  compares each changed file with its trailing `#[cfg(test)]` module
  removed, and skips benchmarking if the only `src/` changes were unit
  tests — those can't change performance. Otherwise the `bench` job runs
  `just bench` on the PR's base commit (`--save-baseline base`), then on
  the PR (`--save-baseline pr`), on the same runner. Since
  `--save-baseline <name>` only ever compares against a previous run
  saved under the *same* name, a final pass reloads both
  (`--load-baseline pr --baseline base`, no re-measurement) to rewrite
  the HTML reports as a genuine base-vs-PR comparison.
  `.github/scripts/bench-report.py` turns that into the comment
  described above, and the job uploads the comment plus the HTML
  reports as the `bench-result` artifact. It also writes the table to
  the run's step summary.
- **`.github/workflows/bench-publish.yml`** runs when `bench.yml`
  finishes. It publishes the reports to `/pr-<N>/benchmarks/` on the
  [GitHub Pages site](github-pages.md), fills the comment's links in
  with their URLs, and posts the comment, or updates its earlier one
  (via `.github/scripts/pr-comment.sh`). When the PR closes,
  `pages-cleanup.yml` deletes `/pr-<N>/`. Its runs don't show up in the
  PR's checks (no `workflow_run` run does), so if publishing fails it
  replaces the comment with a "couldn't publish results" notice linking
  to the failed run — otherwise the failure would only be visible in
  the Actions tab. It can't do this for a fork's PR whose artifact it
  couldn't validate: GitHub doesn't say which PR such a run was for.

Why two workflows: a PR's workflows run that PR's code, and for a PR
from a fork GitHub only grants a read-only token, so `bench.yml` can't
comment or push. `bench-publish.yml` is triggered by `workflow_run`,
which always runs the workflow as it is on `main` with a write token —
safe only because it never executes PR code. It treats the artifact as
untrusted: it checks that the PR number in it belongs to a PR whose head is the commit that was benchmarked, and only
hosts reports on Pages for same-repo PRs (a fork's benchmark code also
writes its report pages, so hosting those would let anyone publish
arbitrary HTML under this repo's Pages domain). A fork's PR still gets
the comment, with links to the workflow run instead. A consequence:
changes to `bench-publish.yml` or the scripts it uses
(`gh-pages.sh`) only take effect once merged to `main`.

A third workflow, **`.github/workflows/bench-main.yml`**, benchmarks
`main` on each push that changes implementation code and publishes the
reports to `/benchmarks/`, with no comparison.

GitHub Pages must be enabled once — see "Setup" in
[`github-pages.md`](github-pages.md). Until it is, the comment's links
point at the workflow run, whose `bench-result` artifact holds the
reports.

This check is **informational, not a gate**: don't make it a
required status check. A slowdown never fails it — only a benchmark
that fails to build or run on the PR does. A base commit whose
benchmarks fail (e.g. this PR restructures the suite) is tolerated, and
the comment shows `new` instead of a comparison.

Nothing is persisted beyond each PR's comment, its `/pr-<N>/benchmarks/`
reports (removed on close), the 14-day artifact and `main`'s latest
reports at `/benchmarks/` — no historical dashboard, no timing data
committed to the repo.
