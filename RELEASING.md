# Releasing

Checklist for cutting a release of `iced_nodegraph` + `iced_nodegraph_sdf`. The
two crates are versioned together from the workspace and published as a pair.

The current development version carries a `-dev` suffix (e.g. `0.5.0-dev`); a
release replaces it with the final number and afterwards a new `-dev` cycle
begins.

## 1. Pick the version

Under Cargo's 0.x semver rules:

- **Public-API break** -> minor bump (`0.4.x` -> `0.5.0`)
- **Additive / bugfix, no API break** -> patch bump (`0.4.0` -> `0.4.1`)

Confirm objectively against the last release tag (this is the CI gate):

```bash
cargo semver-checks --baseline-rev "$(git tag --list 'v*' --sort=-v:refname | head -n1)" \
  -p iced_nodegraph -p iced_nodegraph_sdf
```

"no semver update required" means the change is patch-eligible.

## 2. Bump the version

Version lives in **three** places in the root `Cargo.toml` (the crate manifests
inherit via `version.workspace = true`, so they need no edit):

- `[workspace.package] version = "X.Y.Z"`
- `[workspace.dependencies] iced_nodegraph = { ..., version = "X.Y.Z" }`
- `[workspace.dependencies] iced_nodegraph_sdf = { ..., version = "X.Y.Z" }`

Sync the lockfile to the new version:

```bash
cargo update -p iced_nodegraph -p iced_nodegraph_sdf --precise X.Y.Z
```

Optionally refresh transitive dependencies in the same release
(`cargo update`), then rebuild to confirm nothing breaks.

> The README crates.io badge (`img.shields.io/crates/v/...`) is dynamic and
> updates itself from crates.io — no manual edit.

## 3. Update the CHANGELOG

- Rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`.
- Add a fresh empty `## [Unreleased]` above it.
- Add the link reference at the bottom:
  `[X.Y.Z]: https://github.com/tuco86/iced_nodegraph/releases/tag/vX.Y.Z`

## 4. Run every gate (all must pass)

Mirrors `.github/workflows/ci.yml` plus the wasm check from CLAUDE.md's
pre-push checklist:

```bash
cargo fmt --all -- --check
cargo clippy -p iced_nodegraph -p iced_nodegraph_sdf -- -D warnings
cargo check -p iced_nodegraph --target wasm32-unknown-unknown
cargo test -p iced_nodegraph
cargo test -p iced_nodegraph_sdf -- --test-threads=1   # GPU tests: serialize
cargo deny --log-level error check
# plus the semver-checks command from step 1
```

## 5. Commit, tag, push

```bash
git commit -am "chore(release): X.Y.Z"
git tag -a vX.Y.Z -m "Release X.Y.Z"
git push origin main
git push origin vX.Y.Z
```

Pushing the tag sets the new semver baseline for CI.

## 6. Publish (order matters)

`iced_nodegraph` depends on `iced_nodegraph_sdf = "X.Y.Z"`, so the SDF crate
must be on the crates.io index first:

```bash
cargo publish -p iced_nodegraph_sdf
# wait until it appears on the index (seconds), then:
cargo publish -p iced_nodegraph
```

Publish from the release tree (version = `X.Y.Z`) — do **not** run step 7 first.

## 7. Begin the next dev cycle

```bash
# bump the three version fields in Cargo.toml to the next X.Y.Z-dev, then:
cargo update -p iced_nodegraph -p iced_nodegraph_sdf --precise X.Y.Z-dev
git commit -am "chore: begin X.Y.Z-dev development cycle"
git push origin main
```
