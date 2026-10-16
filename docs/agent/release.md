# Releasing a new flint version

flint follows [Semantic Versioning](https://semver.org/). While the
version stays below `1.0.0`, breaking changes are allowed in minor
releases (`0.x.0`), but must be called out clearly in the changelog.
Raising the minimum supported Rust version (`rust-version` in
`Cargo.toml`) counts as a minor-version change — see "Minimum supported
Rust version" in [`docs/tooling.md`](../tooling.md).

1. **Decide the bump** with the user if it isn't obvious:
   - Patch (`0.1.x`): bug fixes, no behavior change to documented features.
   - Minor (`0.x.0`): new features, or a documented breaking change while
     pre-1.0.
   - `1.0.0`: only when the user explicitly decides flint's core behavior
     is stable enough to commit to — don't bump to 1.0 unilaterally.
2. **Update `Cargo.toml`** `version` field.
3. **Update [`CHANGELOG.md`](../../CHANGELOG.md)**: move the
   `[Unreleased]` entries under a new `## [x.y.z] - YYYY-MM-DD` heading
   (today's date), leaving a fresh empty `[Unreleased]` section above it.
4. **Run `just ci`** to confirm everything is green at the release point.
5. **Commit** the version bump (e.g. `Release vX.Y.Z`).
6. **Tag** the commit (`git tag vX.Y.Z`) — but confirm with the user
   before pushing the tag or the commit anywhere; tagging/pushing
   releases is a visible, hard-to-reverse action. Pushing the tag
   triggers [`.github/workflows/release.yml`](../../.github/workflows/release.yml),
   which builds release binaries for each supported target and publishes
   a GitHub Release with them attached — each with a checksum and an
   SBOM, plus a signed build provenance attestation — with no manual
   steps of its own. Every PR already runs the same build as a dry run (see
   "Release dry run" in [`docs/tooling.md`](../tooling.md)), so a broken
   release build shows up there rather than on the tag.
7. **Publish to crates.io** once the GitHub Release exists: `cargo
   publish` from the tagged commit (the user runs it — it needs their
   crates.io token). `just package-check` has already checked what goes
   into the crate.
8. **Bump the Homebrew formula** in
   [`viboko/homebrew-tap`](https://github.com/viboko/homebrew-tap)'s
   `Formula/flint-cli.rb`: its `version`, and for each of its four
   targets (macOS arm64 and x86_64, Linux arm64 and x86_64) the archive's
   `url` and the `sha256` from the release's `.sha256` file beside it.
   Then `brew install --build-from-source viboko/tap/flint-cli`, `brew
   test` and `brew audit --strict --online` on it, as a PR to the tap.
9. **Update the version in [`README.md`](../../README.md)'s**
   Installation example (the `shasum` and `gh attestation verify`
   lines), so it names a file the latest release has.
