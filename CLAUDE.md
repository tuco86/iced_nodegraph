# Claude Code Instructions for iced_nodegraph

Context for agents working in this repository. It covers workflow, gates, and
conventions. It deliberately does not restate the public API or the control
bindings: those live in rustdoc and the root [`README.md`](README.md), and a
second copy would rot.

## Project Status

Published on crates.io through `v0.4.2`; the working tree is `0.5.0-dev`
(`[workspace.package] version` in the root `Cargo.toml`). Still pre-1.0, so
breaking changes are allowed when justified - but they are no longer free.
Prefer additive, backwards-compatible changes; when a break is genuinely
warranted, make it deliberately, record it in `CHANGELOG.md`, and bump the
version accordingly (under Cargo's 0.x rules a break needs a minor bump).

## Orientation

Two published libraries plus demos, with a strictly one-way dependency
direction: `demos/* -> iced_nodegraph -> iced_nodegraph_sdf`.

- **`iced_nodegraph_sdf`** - the renderer. Shape authoring (`Curve`,
  `ShapeBuilder`, `Shape`, `Tiling`) lowers to `Drawable` segments, which
  `compile.rs` turns into GPU records for `SdfPipeline` and `shader.wgsl`.
  There is one GPU primitive: the circular arc.
- **`iced_nodegraph`** - the widget. `node_graph/mod.rs` holds the builder DSL
  and value types, `node_graph/state.rs` the only state that survives a frame,
  `node_graph/widget.rs` (plus `widget/draw.rs` and `widget/update.rs`) the
  iced `Widget` impl, and `style/*` the flat style structs.
- **`demos/*`** - hello_world, styling, interaction, 500_nodes, shader_editor,
  and the shared `demo_common` crate.

The authority on types and behaviour is the rustdoc: read
`iced_nodegraph/src/lib.rs`'s crate docs and the module docs, and
[`iced_nodegraph_sdf/ARCHITECTURE.md`](iced_nodegraph_sdf/ARCHITECTURE.md) for
the renderer. Regenerate with `cargo doc --workspace --no-deps --open`.

### Invariants not to break

- **The host owns the graph.** The widget never mutates the caller's model. It
  renders the nodes and edges passed to `view` and reports intent through
  `Fn -> Message` callbacks; the application applies the change and feeds the
  result back next frame.
- **The widget is stateless between frames.** Only `NodeGraphState` (camera,
  drag, selection, z-order, touch) survives, and it is keyed by *node index* -
  a transient per-frame identity derived from the host's push order, not by the
  user's node id. `node_lookup` is the single id-to-index map, and the identity
  boundary is the public API: outside it, ids (`N`, `P`, `PinRef`);
  inside it, indices.
- **Screen and world coordinates are distinct types.** `ScreenPoint` and
  `WorldPoint` are separate euclid spaces; convert only through `Camera2D`
  (`screen_to_world` / `world_to_screen`) and the `IntoIced` / `IntoEuclid`
  traits in `node_graph/euclid.rs`. Never coerce one into the other with a raw
  `Point`.
- **The renderer never fails a frame.** When SDF work does not fit, it is
  dropped and counted in `SdfStats` rather than erroring.

### Styling convention

One override convention, everywhere: struct-update over a theme-derived
default.

```rust
node(0, pos, body).style(|theme, status| NodeStyle {
    fill_color: my_color.into(),
    ..default_node_style(theme, status)
});
```

Styles are concrete flat structs - no `Option`/merge config layer, no builder
chains. `Pattern` (re-exported from `iced_nodegraph_sdf`) controls every
stroke.

## Development Workflow

**Phases:**
1. **MVP** - Implement minimal working version of the feature
2. **Fix** - Address all observed errors and issues
3. **Refactor** - Improve code quality, structure, and readability
4. **Commit** - Once code is clean, create a commit
5. **Push** - Only after all checks pass

**Pre-Push Checklist (all must pass):** these mirror the CI `test` job plus the
wasm check CI does not run.

- `cargo fmt --all -- --check`
- `cargo clippy -p iced_nodegraph -p iced_nodegraph_sdf -- -D warnings`
- `cargo test -p iced_nodegraph`
- `cargo test -p iced_nodegraph_sdf -- --test-threads=1` (the pixel tests each
  spin up a wgpu device; parallel runs oversubscribe the GPU)
- `cargo check -p iced_nodegraph --target wasm32-unknown-unknown`

A task is only complete when all checks pass and the code is pushed.

**Git hooks (enforce the above automatically):** versioned hooks live in
`.githooks/`. Enable them once per clone with:

```
git config core.hooksPath .githooks
```

- `pre-commit` runs `cargo fmt --all -- --check` (mirrors the CI format gate).
- `pre-push` runs the full list above, skipping the wasm check when the
  `wasm32-unknown-unknown` target is not installed.

CI (`.github/workflows/ci.yml`) additionally runs `cargo deny check` and the
semver gate below. It is native-only: the wasm check is local.

**Pre-Publish Requirement (before any `cargo publish`):** the CI semver job
runs `cargo semver-checks` for `iced_nodegraph` and `iced_nodegraph_sdf` with
the most recent release tag (`v*`) as the baseline. Tags exist, so the gate is
ACTIVE: any public-API break since the last release fails the build until the
version is bumped to match.

**Release process:** the full step-by-step checklist (version bump, CHANGELOG,
gates, tag, publish order, next dev cycle) lives in
[`RELEASING.md`](RELEASING.md). Follow it for every release.

## Automatic Validation

`.claude/settings.json` registers a `Stop` hook that picks
`.claude/hooks/validate.ps1` on Windows shells and `.claude/hooks/validate.sh`
everywhere else. Both run the same list and must stay in sync:

- `cargo fmt --all --check`
- `cargo check -p iced_nodegraph`
- `cargo check -p iced_nodegraph --target wasm32-unknown-unknown`
- `cargo test -p iced_nodegraph`

They print only on failure, and always exit 0: a non-zero exit would block the
Stop event and re-invoke the agent in a loop.

Clippy and the SDF test suite are not in the hook - run them yourself before
pushing.

## Testing

Unit tests live next to the code they cover (`camera.rs`, `input.rs`,
`state.rs`, `widget.rs`, `content.rs`, `connection.rs`, `style/*`). Everything
that drives the widget through its public API lives in `iced_nodegraph/tests/`:
`clipping.rs`, `coordinates.rs` and `overlay.rs` assert on the arguments the
widget hands its children using the shared recording renderer in
`tests/common/record.rs`; `simulator.rs` drives real events through
`iced_test::Simulator`; `widget_pixel.rs` and `edge_grid_pixel.rs` are pixel
oracles against the headless GPU harness in `tests/common/mod.rs`.
`benches/frame_prep.rs` measures frame-preparation cost. The SDF crate's pixel
tests in `iced_nodegraph_sdf/src/pipeline/pixel_tests.rs` need a real GPU
adapter and serialized execution.

There are no `#[cfg(test)]` modules at the crate root: a test that only touches
the public API belongs in `tests/`, where it also proves the API is reachable
from outside.

Coordinate math, the interaction state machine, style-to-SDF translation, and
the rendered output are all covered. Do not quote a test count in
documentation - it goes stale on the next commit.

## Git Commit Message Rules

**Format**: `type(scope): summary` (Conventional Commits)

**Types**: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `style`, `perf`

**Rules**:
- Single line only (no body unless explicitly requested)
- Summary max 60 characters
- Imperative mood: "add", "fix", "remove" (not "added", "fixed")
- Focus on WHY, not WHAT (intention over implementation details)
- No bullet lists, no file listings, no diff dumps

**Examples**:
- `feat(camera): add zoom-at-cursor transformation`
- `fix(wasm): resolve time platform incompatibility`
- `refactor: separate library from demo dependencies`
- `docs: clarify coordinate system formulas`

**Note**: Claude Code automatically adds co-author attribution when creating
commits.

## Documentation Standards

### Guiding principle: a legible API needs little documentation

Documentation is the second line of defense, not the first. A clear name, a
precise type, and a well-shaped signature carry more than any prose can. When an
item is hard to document because it is confusing, fix the API (rename, retype,
split) instead of papering over it with words. Docs exist to add the knowledge a
signature *cannot* carry - never to restate it.

The failure mode to prevent is slop: doc comments that paraphrase the signature,
assert behavior nobody verified, or pad coverage. Empirically, a comment that
restates the code is noise (it adds no knowledge), and a comment that asserts
unverified behavior is worse than none - it is a "bad comment" that misleads
readers into bugs and is indistinguishable from a correct one. A doc comment must
earn its place; if it would only repeat the signature, leave it off.

### Tone

- NO EMOJIS in code comments, documentation, or console output.
- Clear, technical language. No informal expressions, no marketing.
- Status indicators in prose: "VERIFIED", "TESTED", "INCOMPLETE" (not symbols).
- Describe what the code IS, not what it used to be. No migration breadcrumbs
  ("X no longer exists", "this replaces Y") and no history narration in a doc
  comment - that is what `CHANGELOG.md` and the git log are for.

### When you do document a public item

Apply these when documenting a `fn`, `struct`, `enum`, `trait`, `method`,
`macro`, `type`, or module - especially in a bulk pass.

MUST:
- Open with one short summary sentence, third-person present indicative
  ("Returns the world position.", not "This function returns..."). rustdoc uses
  line 1 as the item summary.
- Add only knowledge the signature cannot convey: intent / the "why", domain
  relationships, how the item combines with others, assumptions, invariants,
  non-obvious cost. This is exactly the content readers need most and get least.
- Document the failure surfaces that apply: `# Errors` for a `Result`-returning
  fn (what each variant means), `# Panics` for a fn that can panic (the
  condition), `# Safety` for every `unsafe fn` (caller invariants).
- Section order, include only what applies:
  summary -> prose -> `# Examples` -> `# Panics` -> `# Errors` -> `# Safety`.
  Always plural `# Examples`.

MUST NOT:
- Restate the signature or types ("Returns a `usize`" on `-> usize` is slop).
- Assert behavior not derivable from the signature without having verified it
  against the body or a known contract.
- Add filler, or a doc comment that exists only to satisfy a coverage rule.

### Examples and doctests

- Add a `# Examples` doctest where an example genuinely helps a reader use the
  item correctly. Do not force trivial examples onto self-evident items
  (getters, `Default`, obvious constructors) - link to a richer example instead.
- Examples must be complete and copy-paste-ready (no `...`, no pseudo-code) and
  must compile: the doctest is the correctness check that keeps the example from
  going stale.
- Where an example would return `Result`, use `?`, never `unwrap`/`try!`
  (users copy examples verbatim).
- For examples that need a renderer/window/event loop (most widget usage), mark
  the fence `no_run` - it still compiles (and stays correct) but is not executed.
  Use `ignore` only when it cannot even compile in a doctest context.

### Uncertainty handling (critical for a bulk pass)

- If behavior is not derivable from the signature, read the function body before
  writing the behavioral claim.
- If it remains unverifiable from the code (external state, runtime config,
  caller contract), state only what is verifiable and omit the speculative part.
  Prefer a doctest that demonstrates verified behavior over prose asserting it.

### Crate / module level

- Each module gets a short overview: purpose, main capabilities, and one
  code-oriented quick-start, so both concept-first and code-first readers land.

## Code Intelligence

For Rust code navigation, prefer a language server over text search: it
resolves through re-exports, generics, and trait impls, which grep cannot.
Whatever symbol-aware tooling is available to you, use these operations:

- **Go to definition** to find where a type or function is declared.
- **Find references** before changing any public item - this workspace has
  five demo crates plus doctests calling into the library, and the compiler
  will not tell you about the doc examples until you run them.
- **Rename symbol** for renames, rather than a find-and-replace pass.
- **Diagnostics** for a fast error/warning read on a file mid-edit, instead of
  a full `cargo check` cycle.

Text search (`grep` / `glob`) is still the right tool for string literals,
comments, non-Rust files (`.toml`, `.md`, `.wgsl`), regex patterns, and when no
language server is running.

## Interaction

The root [`README.md`](README.md#controls) owns the control table (mouse,
keyboard, and touch bindings, plus the web-specific variants). Every binding is
host-rebindable through `NodeGraph::keymap`; see the `Keymap` type.

Two behaviours worth knowing before touching `widget/update.rs`:

- **Plug behaviour.** An edge drag snaps to a compatible pin and fires
  `on_connect` immediately, not on release; moving away fires `on_disconnect`.
  Releasing while snapped keeps the connection, releasing while unsnapped
  discards the drag. So one drag can report several connections.
- **Hit thresholds.** `PIN_CLICK_THRESHOLD` (8.0) and `EDGE_CUT_THRESHOLD`
  (10.0) in `widget/update.rs` are screen pixels, divided by `camera.zoom()` at
  the comparison sites so the on-screen hit target stays constant at any zoom.

## Platform Notes

The libraries build on `iced_widget` / `iced_wgpu` 0.14, never on the `iced`
umbrella crate: Cargo unifies features additively, so an umbrella dependency
would force iced's defaults on every downstream application. `iced` is a
dependency of the demo binaries and of tests/doctests only. See the comment in
the workspace `Cargo.toml`.

### WASM browser compatibility

WebGPU only - there is no WebGL fallback.

- **Chrome/Chromium**: full WebGPU support, the recommended browser.
- **Firefox**: WebGPU has known buffer-mapping issues (async timing bugs), may
  crash.
- **Safari**: untested.

Build the browser demos with `./build_docs.sh` (or `build_docs.ps1`): it
generates rustdoc, then `wasm-pack`s each demo into
`target/doc/<demo>/pkg/` alongside the shared static assets.
