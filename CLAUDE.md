# Claude Code Instructions for iced_nodegraph

This document provides essential context for Claude Code when working on the iced_nodegraph project.

## Development Workflow

**Phases:**
1. **MVP** - Implement minimal working version of the feature
2. **Fix** - Address all observed errors and issues
3. **Refactor** - Improve code quality, structure, and readability
4. **Commit** - Once code is clean, create a commit
5. **Push** - Only after all checks pass

**Pre-Push Checklist (all must pass):**
- `cargo test -p iced_nodegraph` - unit tests
- `cargo check -p iced_nodegraph` - native compilation
- `cargo check -p iced_nodegraph --target wasm32-unknown-unknown` - WASM compilation
- `cargo clippy -p iced_nodegraph -- -D warnings` - lints
- `cargo build -p iced_nodegraph` - full build

A task is only complete when all checks pass and code is pushed.

**Pre-Publish Requirement (before any `cargo publish`):**
- The CI semver gate (`.github/workflows/ci.yml`) runs `cargo semver-checks` for
  `iced_nodegraph` and `iced_nodegraph_sdf` against the most recent release tag
  (`v*`). The first release (`v0.1.0`) is tagged, so the gate is now ACTIVE: any
  public-API break since the last release fails the build until the version is
  bumped to match. Under Cargo's 0.x semver rules a breaking change requires a
  minor bump (`0.1.x` -> `0.2.0`); additive, non-breaking changes are a patch
  bump (`0.1.0` -> `0.1.1`). Do not break the public API casually anymore -
  prefer additive changes, and when a break is genuinely warranted, make it
  deliberately and bump the minor version in the same change.

**Release process:** the full step-by-step release checklist (version bump in
all three `Cargo.toml` fields, CHANGELOG, gates, tag, publish order, next dev
cycle) lives in [`RELEASING.md`](RELEASING.md). Follow it for every release.

## Automatic Validation

**Via SubagentStop hook:**
When a subagent/task completes, `.claude/hooks/validate.ps1` runs:
- `cargo check -p iced_nodegraph` - reports native compile errors
- `cargo test -p iced_nodegraph` - reports test failures

The script only outputs on errors to avoid filling context.

**Note:** Run `cargo fmt --all` manually before committing if desired.

Use the `code-reviewer` agent for reviewing significant code changes before committing.

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

**Note**: Claude Code automatically adds co-author attribution when creating commits.

## Project Status

**Released, pre-1.0 (`v0.1.0`)**: The project is published to crates.io. It is
still pre-1.0 (beta), so the API is not frozen and breaking changes are allowed
when justified - but they are no longer free. Treat the public API as
stabilizing: prefer additive, backwards-compatible changes; reserve breaks for
cases that genuinely warrant them. Every public-API break must be intentional
and paired with the appropriate version bump (a minor bump under 0.x semver, see
the Pre-Publish Requirement above). The semver gate enforces this against the
last release tag.

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

## Tool Usage Preferences

**ALWAYS use LSP (cclsp MCP) for Rust code navigation - it's faster and more accurate than grep:**

| Task | Tool | Example |
|------|------|---------|
| Find definition | `mcp__cclsp__find_definition` | `symbol_name: "NodeGraph"` |
| Find all usages | `mcp__cclsp__find_references` | `symbol_name: "push_edge"` |
| Rename symbol | `mcp__cclsp__rename_symbol` | `symbol_name: "old", new_name: "new"` |
| Get diagnostics | `mcp__cclsp__get_diagnostics` | `file_path: "src/lib.rs"` |

**Parameters:**
- `file_path`: File where symbol is defined (for context)
- `symbol_name`: Name of the symbol to find
- `symbol_kind`: Optional - "function", "struct", "method", "field", etc.

**When to use Grep/Glob instead:**
- Searching in string literals or comments
- Non-Rust files (toml, md, wgsl)
- Regex pattern searches
- LSP server unavailable

**Common patterns to follow:**
- When adding a new style field, use `find_references` on `default_node_style` to see the pattern
- When modifying NodeGraph API, check usages in demos with `find_references`

## Architecture Overview

This workspace contains a node graph editor built on Iced 0.14:

- **`iced_nodegraph`** - Custom node graph widget built on Iced GUI framework *(main project)*
- **`iced_nodegraph_sdf`** - Segment-based SDF renderer providing exact distance fields for nodes, edges, pins, and pin cutouts
- **`demos/*`** - hello_world, styling, interaction, 500_nodes, shader_editor, plus a shared `common` crate

`ngwa-rs` (a SpacetimeDB backend module) is an optional, separate sibling workspace at `../ngwa-rs`. It is NOT a member of this workspace's `Cargo.toml` and is not required to build or run the widget or demos.

**Dependencies**: Uses `iced = "0.14"` from crates.io and the in-tree `iced_nodegraph_sdf` crate for SDF-based rendering.

**Current Status**: Core functionality is complete - node/pin interaction, edge connections, and coordinate transformations are fully functional with type-safe API.

### WASM Browser Compatibility
- **Chrome/Chromium**: Full WebGPU support, recommended browser
- **Firefox**: WebGPU has known buffer-mapping issues (async timing bugs), may crash
- **Safari**: Untested

For WASM demos, Chrome or Chromium-based browsers are recommended.

## Core Architecture Patterns

### Coordinate System Abstraction - VERIFIED & TESTED
The project uses **euclid** crate for type-safe coordinate transformations:
- `WorldPoint`/`ScreenPoint` distinguish coordinate spaces with compile-time type safety
- `Camera2D` handles zoom/pan transformations in `src/node_graph/camera.rs`
- Convert between coordinate systems using `IntoIced`/`IntoEuclid` traits in `src/node_graph/euclid.rs`

**Critical Transformation Formulas** (mathematically verified):
- **Screen → World**: `world = screen / zoom - position`
  - Implementation: `Transform2D::scale(1/zoom).then_translate(-position)`
- **World → Screen**: `screen = (world + position) * zoom`
  - Applied in rendering pipeline via `draw_with()`
- **Zoom at Cursor**: `new_pos = old_pos + cursor_screen * (1/new_zoom - 1/old_zoom)`
  - Maintains visual stability when zooming

**Test Coverage**: 44 unit tests across camera, state, interaction, and API modules validate all core functionality.

**See `src/node_graph/camera.rs` module documentation for complete mathematical derivations and usage patterns.**

### Widget Architecture
- **NodeGraph** (`src/node_graph/mod.rs`) - Main container widget managing nodes and edges
- **NodePin** (`src/node_pin/mod.rs`) - Connection points with `PinSide` enum (Left/Right/Top/Bottom/Row)
- **PinRef** (`src/node_graph/mod.rs`) - Type-safe identifier for pin connections (`node_id`, `pin_id`), generic over `N`/`P`
- **PinEnd** (`src/node_pin/mod.rs`) - Endpoint view passed to `can_connect` (ids, direction, user info)
- **Callbacks** (`src/node_graph/mod.rs`) - `Fn -> Message` handlers (`on_connect`, `on_move`, ...) instead of an event enum
- **State Management** (`src/node_graph/state.rs`) - Handles dragging states and camera state

### SDF-Based Rendering
Uses **iced_nodegraph_sdf** for high-performance node graph rendering:
- Nodes, edges, pins, and overlays rendered via SDF `Layer` + `Pattern` API
- `Pattern` controls stroke appearance (solid, dashed, dotted, arrowed, etc.)
- `Layer` composites fill, gradient, outline, blur, and expand effects
- Background is theme-driven via `GraphStyle`, with optional `TilingBackground` (`TilingKind`: grid/dots/triangle/hex)

**Edge System**: Fully functional with type-safe API:
- `push_edge(edge!(from, to))` adds connections between pins (endpoints are `PinRef`)
- Edge dragging and static edge rendering both work
- SDF renders edges with bezier curves and configurable patterns

**Plug Behavior**: Edge connections behave like physical plugs:
- `EdgeConnected` fires immediately when dragging edge snaps to a compatible pin
- `EdgeDisconnected` fires when moving away from a snapped pin
- Mouse release while snapped keeps the connection; release while not snapped discards the drag
- This provides immediate tactile feedback rather than waiting for mouse release

## Development Workflows

### Building & Testing
```bash
# Build node graph widget
cargo build -p iced_nodegraph

# Run demos
cargo run -p demo_hello_world
cargo run -p demo_styling
cargo run -p demo_500_nodes
cargo run -p demo_shader_editor

# Run tests
cargo test -p iced_nodegraph
```

### SpacetimeDB Integration
```bash
# Install SpacetimeDB CLI first
cd ngwa-rs
spacetime publish  # Deploy backend module
```

## Project-Specific Conventions

### Widget Implementation Pattern
All custom widgets follow Iced's advanced widget pattern:
1. Implement `iced::advanced::Widget` trait
2. Define state type and use `tree::Tag::of::<StateType>()`
3. Handle layout, drawing, and events in separate methods
4. Use `tree::State` for persistent widget state

### Coordinate Transform Pattern
Always use typed coordinates and proper transformation order:
```rust
// Mouse input: Screen → World
let cursor_position: ScreenPoint = cursor.position().into_euclid();
let world_cursor: WorldPoint = camera.screen_to_world().transform_point(cursor_position);

// CRITICAL: Order matters!
// ✅ CORRECT: Transform2D::scale(1/zoom).then_translate(-position)
//    Result: world = screen / zoom - position
// ❌ WRONG: Transform2D::translation(-position).pre_scale(zoom)
//    Result: world = screen * zoom - position (incorrect inverse)
```

**Click Detection Thresholds**:
- `PIN_CLICK_THRESHOLD = 8.0` screen px (divided by `camera.zoom()` at the
  comparison sites, so the on-screen hit target is constant at any zoom)
- `EDGE_CUT_THRESHOLD = 10.0` screen px (same 1/zoom scaling)

### Style System Pattern
Styles are concrete, flat structs (no `Option`/`merge` config layer):
- `NodeStyle` has flat `fill_color`, `border_color`, `border_pattern: Pattern`,
  `border_outline_*`, and `shadow_*` fields (colors are `ColorQuad`)
- `EdgeStyle` has `pattern: Pattern` for the stroke plus flat `border_*` and
  `shadow_*` fields; `PinStyle` mirrors the same shape
- Override via struct-update over the theme default inside a `.style()` closure:
  `NodeStyle { fill_color, ..default_node_style(theme, status) }`
- `Pattern::solid(width)`, `Pattern::dashed(w, dash, gap)`, etc. for stroke patterns

## Key Integration Points

### Iced Framework
- Uses **iced 0.14** from crates.io (upstream)
- Uses advanced renderer features via `iced_nodegraph_sdf`
- Requires `features = ["advanced", "wgpu", "tokio"]`

### Cross-Project Dependencies
- SpacetimeDB module (`ngwa-rs`) is independent backend component

## Module Architecture

### Library Entry Point
- `iced_nodegraph/src/lib.rs` - Public API exports, re-exports all public types

### Core Modules (iced_nodegraph/src/)

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `node_graph/mod.rs` | Main widget, builders | `NodeGraph`, `Node`, `Edge`, `PinRef`, `DragInfo` |
| `node_graph/widget.rs` | Widget trait impl (trunk) | `node_graph()` constructor |
| `node_graph/widget/draw.rs` | Render path | `draw_impl`, SDF layer batching |
| `node_graph/widget/update.rs` | Event path | `update_impl`, `Dragging` state machine |
| `node_graph/widget/camera_overlay.rs` | Pop-out overlay wrapper | `CameraOverlay` |
| `node_graph/camera.rs` | Zoom/pan transforms | `Camera2D`, coordinate math |
| `node_graph/input.rs` | Rebindable keymap | `Keymap`, `KeyCombo`, `KeyAction` |
| `node_graph/euclid.rs` | Type-safe coords | `WorldPoint`, `ScreenPoint`, `IntoIced` |
| `node_graph/state.rs` | Interaction state | `State`, `DragState` |
| `node_pin/mod.rs` | Connection points | `NodePin`, `PinEnd`, `PinInfo`, `PinSide` |
| `style/{node,edge,pin}.rs` | Theming | `NodeStyle`, `EdgeStyle`, `PinStyle`, `GraphStyle` |
| `style/defaults.rs` | Theme defaults | `default_node_style()`, `default_edge_style()`, `default_pin_style()` |
| `content.rs` | Layout helpers | `node_header()`, `node_footer()` |

### Demo Applications (demos/)

| Demo | Purpose | Key Patterns |
|------|---------|--------------|
| `hello_world/` | Basic usage | Node creation, connections |
| `styling/` | Customization | `.style()` closures, `NodeStyle`/`EdgeStyle` |
| `500_nodes/` | Performance | Procedural generation |
| `shader_editor/` | Complex app | Compiler, live preview |

### Dependency Flow
```
lib.rs (public API)
  ├── node_graph/ (widget)
  │     ├── widget.rs (iced Widget trait; draw/update/camera_overlay submodules)
  │     ├── state.rs (interaction)
  │     └── camera.rs (transforms)
  ├── node_pin/ (pin widget)
  ├── style/ (theming)
  └── content.rs (layout helpers)
```

## Public API Reference

### Core Types
```rust
// Type-safe pin reference, generic over node id N and pin id P.
// PinReference (the old usize/usize struct) no longer exists - use PinRef.
pub struct PinRef<N = usize, P = usize> {
    pub node_id: N,
    pub pin_id: P,
}
```

There is no `NodeGraphEvent` enum. The widget reports through `Fn -> Message`
callbacks (`on_connect`, `on_move`, ...); the host defines its own `Message`
enum and maps each callback to one of its variants.

### NodeGraph Methods
The widget is generic over node id `N` and pin id `P` (both default to `usize`).
A connection endpoint is a `PinRef<N, P> { node_id, pin_id }`. Nodes and edges
are built with the `node(...)` / `edge!(...)` constructors and pushed as whole
builder values; styling is attached to the builder via a `.style()` closure
(there is no `push_node_styled` / `push_edge_styled`, and no `NodeConfig` /
`EdgeConfig`).
```rust
// Adding content: build with node()/edge!, then push the builder
ng.push_node(node(node_id, position, element));
ng.push_node(node(node_id, position, element)
    .style(|theme, status| NodeStyle { ..default_node_style(theme, status) })
    .pin_style(|theme, status, info| PinStyle { ..default_pin_style(theme, status) }));
ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));
ng.push_edge(edge!(from, to)
    .style(|theme, status, from_pin, to_pin| EdgeStyle { ..default_edge_style(theme, status) }));

// Event handlers (callbacks return the host's own Message)
ng.on_connect(|from, to| Message)        // from, to: PinRef<N, P>
ng.on_disconnect(|from, to| Message)
ng.on_move(|delta, node_ids| Message)    // delta: Vector, node_ids: Vec<N> (single or group)
ng.on_select(|selected_ids| Message)     // selected_ids: Vec<N>
ng.on_clone(|node_ids| Message)
ng.on_delete(|node_ids| Message)
ng.on_pan(|pos, zoom| Message)           // commit-on-release for pan AND zoom
ng.on_info(|info| Message)               // per-frame GraphInfo metrics
ng.on_drag_start(|drag| Message)         // low-level drag hooks (DragInfo<N, P>)
ng.on_drag_update(|pos| Message)
ng.on_drag_end(|| Message)
ng.can_connect(|from, to| bool)          // from, to: PinEnd<'_, N, P, UI> (not PinRef)
ng.selection(&selected_set)              // highlight + z-order selected nodes
ng.view(pos, zoom)                        // controlled camera; host owns pos/zoom (pairs with on_pan)
```

The camera is a controlled value just like selection: the host keeps `pos`/`zoom`
in its model, feeds them via `view()`, and updates them from `on_pan`. Pushing a
new `view()` (e.g. resetting to origin) snaps the widget camera there.

When adding features, maintain the coordinate system abstractions and use `iced_nodegraph_sdf` Layer/Pattern API for custom rendering.