# crank documentation

Full documentation for [crank](../README.md), a code generator.

- [**Playground**](../playground/README.md) — trying crank in the
  browser, and the examples it ships with.
- [**Features**](features.md) — what crank does today.
- [**Usage**](usage.md) — running crank, its output, `--check`,
  command-line arguments, `--help` output.
- [**The `.crank` file format**](crank-file-format.md) — task groups
  and tasks, data files and `#selectors`, `array` groups, templates,
  built-in and custom filters, pockets, `.crank.manifest`, and every
  error.
- [**Test coverage**](coverage.md) — how much of the code each test
  suite exercises, how it's reported on PRs, and how to see it locally.
- [**Mutation testing**](mutation-testing.md) — whether the tests
  would notice a change to the code, run weekly, and how to triage what
  it finds.
- [**Benchmarking**](benchmarking.md) — how crank's performance is
  measured and how to run the benchmarks locally.
- [**The GitHub Pages site**](github-pages.md) — what's published at
  crank.viboko.dev (the playground, and coverage and benchmark reports
  for `main` and each PR), and how.
- [**Fuzzing**](fuzzing.md) — how crank is fuzzed for crashes and hangs,
  the seeds and corpus, and how to run the fuzzer locally.
- [**Architecture**](architecture.md) — how the codebase fits together:
  module responsibilities, dependencies, request flow, and which test
  suites and benchmarks cover which parts.
- [**Tooling**](tooling.md) — every recipe in the [`justfile`](../justfile),
  what it wraps, and why — including generating browsable API docs
  (rustdoc) from this codebase's doc comments.

Looking to contribute? See [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the
repo root, and [`AGENTS.md`](../AGENTS.md) if you're working in this repo
with an AI coding agent.
