# Hard rules for iced_nodegraph

The full conventions live in `AGENTS.md`. These few must stay visible even after
a long conversation has pushed that context out of view.

- **The gate is non-negotiable.** Before calling anything done:
  `cargo fmt --all -- --check`,
  `cargo clippy -p iced_nodegraph -p iced_nodegraph_sdf --all-targets -- -D warnings`,
  `ICED_TEST_BACKEND=tiny-skia cargo test -p iced_nodegraph`,
  `cargo test -p iced_nodegraph_sdf -- --test-threads=1`,
  `cargo check --workspace`,
  `cargo check -p iced_nodegraph_bench --benches`,
  `cargo check -p iced_nodegraph --target wasm32-unknown-unknown`.
  Never weaken a test or widen an `allow` to make it pass.

- **No emojis** in code, comments, documentation or console output.

- **Describe what the code IS.** No history narration in comments or docs, no
  migration breadcrumbs ("X no longer exists", "previously", "this replaces Y").
  That is what `CHANGELOG.md` and the git log are for.

- **A doc comment that restates the signature is noise; one that asserts
  unverified behaviour is worse than none.** Read the body before making a
  behavioural claim, or omit the claim.

- **The host owns the graph.** The widget never mutates the caller's model and
  keeps no graph state between frames. Screen and world coordinates are distinct
  euclid types - convert only through `Camera2D`.
