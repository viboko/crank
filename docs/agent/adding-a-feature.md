# Adding a feature to crank

Follow this sequence for any change to crank's actual behavior. Skipping
steps is how the docs and the tool drift apart — see
[`AGENTS.md`](../../AGENTS.md) for the rest of this project's conventions.

1. **Implement the logic as a pure function** in the relevant `src/`
   module — see [`docs/architecture.md`](../architecture.md) for what
   each one owns (`crank_file.rs` parses `.crank`, `render.rs` renders,
   `pocket.rs` handles pockets, `run.rs` drives a whole run, `output.rs`
   prints, and so on), or add a new module if the feature fits none of
   them. Keep it free of filesystem/process access so it's
   unit-testable.
2. **Touch `main.rs` only for new I/O or CLI flags** — it's the thin
   layer that reads and writes the disk, prints and sets the exit code.
   Most features never need to change it.
3. **Add unit tests** (`#[cfg(test)] mod tests` in the same file)
   covering the new logic's edge cases, following the existing style in
   that module. If this moves lines that docs link to by line range
   (`docs/crank-file-format.md`'s "Examples" table,
   `docs/architecture.md`), run `just fix-doc-links` — see
   [`AGENTS.md`](../../AGENTS.md).
4. **Add an end-to-end Robot Framework test** in `e2e/tests/`, exercising
   the new behavior through the compiled binary (temp dir, real files,
   assert on stdout/stderr/exit code) — see
   `e2e/resources/crank.resource` for the available keywords.
5. **Update docs**: [`docs/crank-file-format.md`](../crank-file-format.md)
   or [`docs/usage.md`](../usage.md) (whichever the change actually
   affects), and [`docs/features.md`](../features.md) if the feature is
   user-visible.
6. **Update or add a playground example** if the feature changes what
   one prints or generates, or deserves one of its own — see
   [`playground/README.md`](../../playground/README.md#examples). A new
   example also makes a good fuzz seed in `fuzz/seeds/run/` (see
   [`docs/fuzzing.md`](../fuzzing.md)).
7. **Update [`CHANGELOG.md`](../../CHANGELOG.md)** — add a bullet under
   `[Unreleased]`. Don't bump the version here; see
   [`release.md`](release.md) for that.
8. **If the feature changes what belongs in this repo's own layout**,
   update the [`.flint`](../../.flint) files too.
9. **Run `just ci`** and confirm it's green.
10. **Open a PR** — follow [`CONTRIBUTING.md`](../../CONTRIBUTING.md).
