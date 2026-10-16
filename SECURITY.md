# Security policy

## Reporting a vulnerability

Please **don't** report security issues in public GitHub issues or pull
requests. Instead, use GitHub's private vulnerability reporting: go to
the repository's **Security** tab and choose **Report a vulnerability**.
That opens a private advisory visible only to you and the maintainer.

Include what you can of:

- the flint version (`flint --version`) and your OS;
- a `.flint` file and/or directory layout that reproduces the problem;
- what happened, and what you expected to happen.

flint has a single maintainer, so responses are best-effort. Once a
report is confirmed, the fix is released and the advisory published
together, crediting you unless you'd rather not be named.

## Supported versions

Only the latest release gets security fixes. While flint is below
`1.0.0`, fixes aren't backported to older versions.

## What counts as a vulnerability

flint is often run in CI against repositories — and pull requests — that
its user didn't write, so both the `.flint` files and the directory tree
it checks should be treated as untrusted input. Examples of things worth
reporting:

- a `.flint` file or directory tree that makes flint crash, hang, or use
  unbounded memory;
- flint reading, following, or reporting on anything outside the
  directory it was asked to check (e.g. via symlinks);
- file names that, when flint prints them, can inject terminal escape
  sequences or forge lines in CI logs;
- a vulnerable dependency that flint's use actually exposes (advisories
  are also checked automatically by `cargo deny` in CI).

A bug that only gives a wrong lint result — flint allowing a file it
should flag, or the reverse — is a normal bug; please open a regular
issue for those.
