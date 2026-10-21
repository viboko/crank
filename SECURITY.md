# Security policy

## Reporting a vulnerability

Please **don't** report security issues in public GitHub issues or pull
requests. Instead, use GitHub's private vulnerability reporting: go to
the repository's **Security** tab and choose **Report a vulnerability**.
That opens a private advisory visible only to you and the maintainer.

Include what you can of:

- the crank version (`crank --version`) and your OS;
- a `.crank` file and the files it uses that reproduce the problem;
- what happened, and what you expected to happen.

crank has a single maintainer, so responses are best-effort. Once a
report is confirmed, the fix is released and the advisory published
together, crediting you unless you'd rather not be named.

## Supported versions

Only the latest release gets security fixes. While crank is below
`1.0.0`, fixes aren't backported to older versions.

## What counts as a vulnerability

crank is often run in CI against repositories — and pull requests — that
its user didn't write, so the `.crank` file and everything it refers to
(data files, templates, filter scripts, previous outputs and
`.crank.manifest`) should be treated as untrusted input. Examples of
things worth reporting:

- input that makes crank crash, hang, or use unbounded memory — a
  filter script getting past the Rhai sandbox's limits, say;
- crank reading, writing or deleting a file outside the directory
  holding `.crank`. It refuses every path in `.crank` and
  `.crank.manifest` that's absolute or climbs out with `..` (including
  an `output` the data puts a `..` into), and every path that leads out
  through a symbolic link, so it never writes or deletes outside that
  directory;
- crank reading, writing or deleting a file other than the ones its
  input names: the files a `.crank` file refers to, the outputs it
  generates, and the stale outputs `.crank.manifest` records;
- a filter script reaching anything beyond the value it's given — the
  filesystem, the network, the environment, other processes;
- file names or content that, when crank prints them, can inject
  terminal escape sequences or forge lines in CI logs;
- a vulnerable dependency that crank's use actually exposes (advisories
  are also checked automatically by `cargo deny` in CI).

A bug that only generates the wrong output — a template rendering
incorrectly, or a pocket's content not being carried over — is a normal
bug; please open a regular issue for those.
