# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-09-24

### Migrating from 0.4

- Ids: declare a marker implementing `Ids` (or keep the default `Indexed`) and
  name it once, as `NodeGraph::<AppIds, _, _, _>::new()` and in messages as
  `PinRef<AppIds>`. The marker traits `NodeId`, `PinId`, `EdgeId` are gone.
- Type parameters: `NodeGraph<'a, I, Message, Theme, Renderer>`; `Node`,
  `Edge`, `Anchor`, `PinRef` and `DragInfo` take `I`; `Theme` must implement
  `Catalog` (`iced::Theme` does).
- Builders: `push_node` / `push_edge` return the graph (chain them, or
  `graph = graph.push_node(..)`); `edge(from, to, id)` -> `edge(id, from, to)`;
  `edge!(from, to)` -> `edge((), from, to)`.
- Renames: `view` -> `camera`, `on_pan` -> `on_camera`, `box_select_style` ->
  `selection_box_style`, `DragInfo::BoxSelect` -> `DragInfo::SelectionBox`,
  `SdfPatternType` -> `PatternType`, `GraphStyle::from_theme` ->
  `default_graph_style`.
- Selection: `NodeGraph::selection(..)` -> `Node::selected(bool)` per node.
- Styles: `NodeStyle` / `EdgeStyle` presets take `(theme, status)`; the overlay
  closures return `SelectionBoxStyle` / `CuttingToolStyle`; a `PinStyle::radius`
  times 0.4 keeps 0.4.2's size; struct literals name the new fields, or use
  struct-update over the `default_*_style`.
- Removed, with replacement: `Camera2D` (use `camera` / `on_camera`),
  `GraphStyle::new` / `dark` / `light` / `Default` (use `default_graph_style`),
  `SelectionStyle` (use `NodeStatus::Selected` in the node style), the
  `PinStyle` presets (use `default_pin_style`), `PinShape::Diamond` /
  `Triangle`, `NodePin`'s public fields (use its builder), `Curve` /
  `ShapeBuilder` / `boolean` (use `Shape` and its operators), `pub use iced`
  (depend on `iced`).

### Breaking

- **The id types are one `Ids` marker.** 0.4.2's
  `NodeGraph<'a, N, P, UI, Message, Theme, Renderer, E>` is
  `NodeGraph<'a, I, Message, Theme, Renderer>` with `I: Ids`, whose associated
  types name the node, pin, edge and anchor ids and the per-pin payload once:

  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  struct AppIds;
  impl Ids for AppIds {
      type NodeId = u64; type PinId = &'static str; type EdgeId = u64;
      type AnchorId = usize; type Payload = ();
  }
  ```

  `Node`, `Edge`, `Anchor`, `PinRef`, `PinEnd`, `PinInfo`, `DragInfo` and
  `FocusTarget` take that one parameter; `Indexed` is the default and what
  `node_graph()` builds. The marker traits `NodeId`, `PinId` and `EdgeId` are
  replaced by `Id`, blanket-implemented for every `Clone + Eq + Hash + Debug +
  Send + Sync + 'static` type. A `NodePin` whose id or payload type does not
  match the graph's is a debug-build assertion at the first layout.

- **The theme is resolved through a `Catalog`.** 0.4.2 implemented `Widget`
  for `iced::Theme` only; `NodeGraph`, `Node`, `Edge`, `Anchor` and `NodePin`
  now work with any `Theme: Catalog`. The trait has a class type per element
  (`NodeClass`, `PinClass`, `EdgeClass`, `DragEdgeClass`, `AnchorClass`,
  `GraphClass`, `SelectionBoxClass`, `CuttingToolClass`, `ParticleClass`,
  `MinimapClass`), a `default_*` per class and a resolver per class.
  `iced::Theme` implements it through the exported boxed closures (`NodeStyleFn`
  and siblings), so every `.style(..)` closure keeps compiling, and each gains a
  `.class(..)` sibling (`Node::class`, `Node::pin_class`, `Edge::class`,
  `NodeGraph::graph_class`, `selection_box_class`, ...).

- **`edge(id, from, to)`.** The id comes first, as in `node`, and the `edge!`
  macro is removed; the no-id case is `edge((), from, to)`.

- **`push_node` and `push_edge` consume and return the graph**, like
  `Column::push`; `push_anchor` and the bulk adders `nodes`, `edges` and
  `anchors` follow the same shape, so a `view` is one expression.
  `NodeGraph::new()` is added beside `Default`.

- **`NodeGraph::view` is `camera` and `on_pan` is `on_camera`.** The
  signatures are unchanged.

- **Selection is a property of the node.** `NodeGraph::selection(..)` is
  removed; mark nodes with `Node::selected(bool)`. The widget still keeps a
  working selection from clicks and the selection box and reports it through
  `on_select`; a changed marked set overrides it. Pressing empty canvas keeps
  the old highlight until the selection box closes.

- **`GraphInfo` carries the frame's `SdfStats` whole.** `sdf_entries` and
  `sdf_tiles` are replaced by `sdf: SdfStats` (every pipeline counter,
  re-exported), and `anchors: Counts` is new. A struct literal must name both.

- **`DragInfo` names every drag of graph content.** It is generic over
  `I: Ids`, `BoxSelect` is `SelectionBox`, and the new variants `Anchor`,
  `Route`, `Resize` and `EdgeCut` need match arms. Every `on_drag_start` pairs
  with one `on_drag_end`. `DragInfo` derives `PartialEq`.

- **Style structs and `Keymap` gained fields.** `EdgeStyle::glow_color` /
  `glow_width` (the cable hover glow), `PinStyle::cutout_radius` (the well the
  pin opens in the node body) and `Keymap::frame_all` / `frame_selection` /
  `snap_override`. Full struct literals must name them; struct-update over a
  `default_*_style` or `Keymap::default()` is unaffected.

- **`PinStyle::radius` is the drawn radius.** 0.4.2 scaled it by 0.4, so the
  same value now draws 2.5x larger; multiply an old value by 0.4 to keep the
  size. The default is 5.0. A `PinShape::Square` takes the area of the circle
  of the same radius.

- **The style presets take `(theme, status)` and derive from the palette.**
  `NodeStyle::input`, `process`, `output`, `comment` and `EdgeStyle::data_flow`,
  `error`, `disabled`, `highlighted`, `debug` have the shape of the
  `default_*_style` functions and drop into `.style(NodeStyle::input)`. They
  keep the selected and pending-cut feedback.

- **The `PinStyle::data` / `execution` / `control` / `event` presets are
  removed.** Type pins with `PinStyle { color, ..default_pin_style(theme,
  status) }`.

- **`GraphStyle` follows the one styling convention.** `GraphStyle::from_theme`
  is `default_graph_style(theme)`. `GraphStyle::new`, `dark`, `light`, the
  `Default` impl and the `background_color` / `tiling` / `selection_style`
  builders are removed: override the fields with struct-update over the
  default. `SelectionStyle` and the `GraphStyle::selection_style` field are
  removed; the overlays have their own styles and the selected-node look is
  `default_node_style(theme, NodeStatus::Selected)`.

- **The overlay styles are named structs.** `box_select_style` is
  `selection_box_style` and returns `SelectionBoxStyle` (base
  `default_selection_box_style`) instead of a `(Color, Color)`;
  `cutting_tool_style` returns `CuttingToolStyle` (base
  `default_cutting_tool_style`) instead of a `Color`.

- **`NodePin`'s fields are private.** `side`, `direction`, `pin_id`,
  `user_info` and `content` are set through its builder methods.

- **`PinShape` has only `Circle` and `Square`.** `Diamond` and `Triangle` were
  drawn as circles. The `#[repr(u32)]` and explicit discriminants are removed.

- **`PinSide` has no integer encoding.** The `#[repr(u32)]`, the explicit
  discriminants and `impl From<PinSide> for u32` are removed.

- **`Camera2D` is no longer public.** The widget's camera lives in private
  state; the host's camera API is `NodeGraph::camera` in and `on_camera` out,
  both plain `(Point, f32)`.

- **`SdfPatternType` is `PatternType`.** It is exported under its own name as
  `iced_nodegraph::PatternType` and `iced_nodegraph_sdf::PatternType`.

- **No `iced` umbrella dependency.** `iced_nodegraph` builds on `iced_widget`
  and `iced_nodegraph_sdf` on `iced_wgpu`, so the libraries no longer switch
  iced's default features (`tiny-skia`, `web-colors`) back on for the host.
  `pub use iced` is replaced by `pub use iced_widget` and `pub use iced_wgpu`;
  depend on `iced` directly. The public types are unchanged.

- **`iced_nodegraph_sdf` authors geometry through `Shape` alone.** `Curve`,
  `ShapeBuilder` and the `boolean` module are no longer public: nothing they
  built could be submitted, since `SdfPrimitive::push` takes a `&Shape`. Use
  `Shape`'s primitives and the `-` / `|` / `&` operators.

### Added

- **Routing anchors.** `anchor(id, position)`, `NodeGraph::push_anchor` /
  `anchors` and `Edge::route(anchors)` route a cable around anchors, with ids
  from `Ids::AnchorId`; the widget derives visiting order, wrap direction and
  ring per cable every frame. Gestures report through `on_anchor_create`,
  `on_anchor_move`, `on_anchor_delete`, `on_route_attach`, `on_route_detach`.
  Styled by `AnchorStyle` (incl. `offered_ring_color`) / `AnchorStatus` /
  `default_anchor_style`; `dragging_anchor_style` / `dragging_anchor_class`.

- **Fit-to-view focus.** `iced_nodegraph::focus(id, target, opts)` returns a
  `Task` that frames a `FocusTarget` (`All`, `Selection`, nodes, anchors,
  edges or a world `Rect`) in the graph carrying `NodeGraph::id`;
  `focus_operation` exposes the underlying operation. `FocusOptions` carries
  padding, zoom bounds and a `FocusAnimation` with an `Easing`. The keymap
  gains `frame_all` (`Home`) and `frame_selection` (`F`).

- **Edge particles.** `Edge::particles` takes `particle(born, speed)` values
  that travel `speed * age` world units along the cable, styled by
  `ParticleStyle` through `Particle::style` / `Particle::class` over
  `default_particle_style`. The widget keeps them animating; the host pushes
  each particle every frame and ends it by leaving it out.

- **Cable hover glow.** The stretch of cable under the cursor gets a glow,
  styled by `EdgeStyle::glow_color` / `glow_width` (`glow_width` 0 turns it
  off).

- **Grid snap while dragging.** `NodeGraph::snap_grid(spacing)` snaps dragged
  nodes, anchor moves and grip resizes to a world-unit grid; a group keeps its
  layout. Holding `Keymap::snap_override` (default Alt) suspends it.

- **Frames.** `Node::frame()` makes a node a backdrop that renders behind the
  other nodes and carries the nodes lying fully inside it when dragged.

- **Minimap overlay.** `NodeGraph::minimap(Minimap { size, corner, margin })`
  pins an overview of nodes and anchor rings to a corner, styled through
  `minimap_style` over `default_minimap_style`. Clicking or dragging on it
  moves the camera through `on_camera`.

- **Resizable nodes.** `Node::resizable(true)` gives a node a bottom-right
  grip whose drag reports the new absolute content size through
  `NodeGraph::on_resize`; the host applies it.

- **`NodeGraph::on_connect_refused`** reports the pin pair of a drop the
  connection validation turned down, once, on release.

- **`NodeGraph::on_edge_delete`** reports the ids of the edges one cutting
  gesture destroyed, as `on_delete` does for nodes.

- **Node content can set the mouse cursor.** `mouse_interaction` forwards to
  the topmost node under the cursor; the graph claims the cursor only while
  panning, dragging, cutting, box-selecting or over a resize grip.

- **`Shape::path`** builds one open multi-segment stroke from a start point and
  `PathSeg::{Line, Arc, Bezier}`; a dash or flow pattern phases once over the
  whole path.

- **GPU work and memory counters.** `SdfStats` gains `upload_bytes`,
  `gpu_bytes`, `index_bytes`, `sdf_draws`, `shaded_px`, `segment_evals`,
  `fine_slots_max`, `fine_live_tiles`, `index_traffic_bytes`,
  `fine_evicted_tiles`, `fine_evicted_slots` and `gpu_dropped_items` (geometry
  dropped at the device's buffer limit), and derives `PartialEq`.
  `set_index_probe` / `index_probe_enabled` arm an opt-in readback that feeds
  `segment_evals`.

- **`SdfPrimitive::mark_animated`** declares geometry the caller recomputes
  every frame, so `has_animations` keeps the redraw loop running for it.

- **`SdfPrimitive::layout_bounds`** lets a primitive drawn inside a scaled
  parent fold that scale into its camera zoom.

- `iced_nodegraph_sdf::color::transparent`, the zero-alpha helper.

- `PatternType` is exported at the `iced_nodegraph_sdf` crate root.

- The prelude includes `Particle`, `particle`, `ParticleStyle` and
  `default_particle_style`.

### Changed

- **The theme defaults are one mapping.** Every color the widget picks comes
  from one role table derived from the iced palette: surfaces are lightness
  steps from the background, marks sit between canvas and foreground, and
  accents are floored to a minimum separation, in all 22 built-in themes.

- **The default look is opaque.** Node bodies are opaque (0.4.2 used 0.75 /
  0.85 opacity), the canvas grid is opaque one elevation step above the
  background, and node shadows fall straight down.

- **Selected nodes are styled in full.** `default_node_style(theme,
  NodeStatus::Selected)` gives an accent border, an accent halo, a tinted body
  and a deeper shadow.

- **Valid drop targets are highlighted.** `default_pin_style` shows
  `PinStatus::ValidTarget` statically in the theme's `success` color with a
  halo filling the pin's cutout.

- **The edge-cutting trail width is in screen pixels**, divided by the zoom
  like the selection box, instead of a fixed world width.

- **The SDF renderer does less work per frame.**
  - Fine tiles are 8px instead of 16px: the GPU frame on the 500-node
    benchmark scene goes 583 -> 393 us, at +13 MiB of index memory.
  - The tile grid is world-anchored: panning reuses the tile index and skips
    the cull pass until it crosses a 64px tile boundary.
  - Beziers fit to 0.1 world units (<= 1 px at maximum zoom), 8 instead of 12
    arcs per curve (-16% GPU frame); the tile sort is 11% cheaper.

### Fixed

- **A pop-out inside a pin opens.** A `pick_list`, `combo_box` or `tooltip`
  wrapped in a `NodePin` drew its trigger but never its menu; the pin now
  forwards `overlay` to its content.

- **Pop-outs of nodes away from the world origin and at zoom.** A menu inside a
  node far from the origin collapsed to nothing, and at zoom one was laid out
  against the wrong region and flipped or clamped against a phantom edge.

- **A `NodeGraph` works as a node body.** The outer graph no longer adopts the
  inner graph's pins, wheel and shortcuts go to the innermost graph first, and
  the inner graph's pop-outs and SDF layers follow the outer pan and zoom.

- **A `PinSide::Row` pin attaches on the border nearer the far end.** Cables
  always took the left border and pointed back through the node; each end now
  leaves outward on the border facing its other end, and both borders show an
  indicator.

- **A cable is hit-tested against the curve it is drawn with.** An edge styled
  `EdgeCurve::Line` was cut and pressed as if it were a bezier.

- **A touch pan drifted by the widget's screen offset** when the graph was not
  at the window origin.

- **Every SDF-drawn color was a gamma step too bright.** SDF surfaces now match
  an iced quad of the same `Color`, and gradients interpolate in the target's
  color space.

- **Shapes went missing after a draw-set change.** A sequence of moves could
  scatter one primitive's geometry into another draw's tiles, leaving nodes
  and pins with straight, tile-aligned gaps.

- **A `Shape::rounded_box` whose corner radius fills its half-extent** (a
  circle or a pill) no longer paints a spur off the shape.

- **The shared SDF resources are keyed by device and surface format**, so a
  second wgpu device in one process (a rebuilt browser embed, two headless
  renderers) no longer has its submits rejected.

- **The geometry buffers stop at the device's storage-binding limit** instead
  of failing the allocation; what does not fit is counted in
  `SdfStats::gpu_dropped_items`.

- **An infinite node body is a debug assertion at layout**, naming the node,
  instead of NaN geometry or a silently misrendered node.

### Internal

- `demos/hello_world` has one config node per `Catalog` class and status and
  boots a complete styling rig.
- The criterion bench lives in the `iced_nodegraph_bench` member as
  `benches/shape_eval.rs`, so `cargo test -p iced_nodegraph` no longer builds
  criterion.
- GPU budget tests on the 500-node scene (memory, idle-frame uploads, fragment
  work) and the ignored `gpu_cost_report` probe, which also times the index
  scatter and sort separately.
- `demos/500_nodes` reporter knobs (`NG_REPORT`, `NG_SCALE`, `NG_NODES`,
  `NG_NO_EDGES`, `NG_NO_GRID`); see "Diagnosing GPU cost" in
  `demos/README.md`.
- The `bench_scene` fixture draws 640 distinct edges instead of 100 stacked
  ones.
- The widget pixel-oracle harness resets the renderer per frame; two tests
  ignored for cross-test pollution run again.
- A test pins the bezier tessellation tolerance against a finer reference.
- The `pin!` and `node_pin` doc examples compile.
- Demos require `iced_palette` 0.1.1, which drops the `iced` umbrella crate.
- `NodeGraph` stores `Node` and `Edge` values directly.
- The recording-renderer tests live in `tests/` and share
  `tests/common/record.rs`.
- `demos/shader_editor` reports compile errors through `Display` and refuses
  unhandled node types.
- The `iced` dev-dependency uses `x11` instead of `wayland`.
- Cable topology lives in `node_graph/cable.rs`, path geometry in
  `node_graph/edge_path.rs`.
- The animation clock advances in one place.
- `iced_nodegraph_sdf` docs present `Shape` as the one public authoring API.

## [0.4.2] - 2026-07-23

### Fixed

- SDF render bind-group layout declared the `draws` storage buffer visible to
  the vertex stage, though only the fragment shader reads it. That required the
  `VERTEX_STORAGE` downlevel flag and failed `create_bind_group_layout` on
  backends/devices without it (e.g. OpenGL). Binding 0 is now fragment-only.
- Removed the WebGL wasm fallback. The renderer reads storage buffers in the
  fragment stage, which WebGL2 does not provide (its
  `max_storage_buffers_per_shader_stage` is 0), so the fallback crashed at
  bind-group-layout creation. The SDF crate no longer enables `iced_wgpu`'s
  `webgl` feature, matching the documented WebGPU-only browser support; without
  WebGPU the app now fails to acquire an adapter instead of crashing mid-frame.

## [0.4.1] - 2026-07-23

### Added

- `docs/scatter.svg`: a diagram of the gather-to-scatter index-build flip
  (bbox walk + exact interval append, the three cull kernels), embedded in
  the SDF README's Part 5, which previously covered scatter in prose only.

### Fixed

- Stale SDF docs: `tiles.svg` still showed the pre-doubling 256 coarse slots
  (2KB/tile) and 8-bit fine packing; README/ARCHITECTURE still described the
  removed z-axis cull dispatch and the cursor-based slot reuse that arena
  residency replaced. All now match the shipped constants and kernels.
- Zero corner radius panic: a `Shape::rounded_box(_, [0.0; 4])` (the selection
  box and toggle indicator, drawn when clicking near a node corner) emitted a
  degenerate zero-radius arc per corner, tripping `from_center_arc`'s
  positive-radius debug assert. A non-positive-radius arc is now a sharp turn
  (heading rotates, no segment), so a zero-radius box evaluates to a plain
  rectangle.

## [0.4.0] - 2026-07-11

### Added

- Touch support: a single finger emulates the left mouse button (tap selects,
  drag moves nodes or drags edges), a one-finger drag on empty canvas pans
  (instead of box-selecting; a quick tap there clears the selection), and two
  fingers pinch-zoom and pan the camera. Embedded node content receives the
  synthesized mouse events, so sliders and inputs stay operable by touch.
- Host-configurable, platform-aware keymap: `NodeGraph::keymap` takes a
  `Keymap` (re-exported with `KeyCombo`, `ComboKey`, `KeyAction`) whose key
  bindings can be rebound or disabled individually and whose pointer fields
  (`pan_button`, `edge_cut_modifiers`, `multi_select_modifiers`) replace the
  hardcoded Right-button/Cmd/Shift gates. Key combos match layout-independently
  (physical key via `Key::to_latin`) and with exact modifier state. The wasm32
  default rebinds clone to `Alt+D` (browsers reserve `Cmd/Ctrl+D` for
  bookmarking at chrome level) and drops the `Backspace` delete alternative
  (legacy back-navigation).
- Scripted GPU profiling: `gpu_trace.py` drives the Nsight Graphics CLI
  headlessly and prints per-pass GPU times plus hardware counters (SM/L2/DRAM
  throughput, warp-stall breakdown) for the SDF pipeline, via the new ignored
  `gpu_probe_loop` test; `--demo <name>` traces a demo binary for whole-frame
  GPU times instead. The headless test renderer now honors `WGPU_*` env vars
  (`WGPU_DEBUG=1` on release builds emits pass labels without validation
  overhead). The probe splits the shade pass into per-category markers
  (background / edges / node fills).

### Changed

- Keyboard shortcuts now require the exact modifier state of their combo:
  `Cmd+Shift+A` no longer triggers Select All (previously any superset of the
  required modifiers matched). Shortcut letters resolve from the physical key
  position on non-Latin layouts instead of the logical character.
- The `Widget::update` event path was restructured into per-`Dragging`-variant
  handlers with a shared `UpdateCtx`; the mirrored unplug-FROM/unplug-TO blocks
  collapsed into one parameterized path. No behavior change intended beyond the
  keymap items above; the drag/selection test suites cover the move.
- The README was rebuilt around what a first-time visitor needs: a hero
  screenshot of the live WASM demo (`assets/hero.png`, linked to the hosted
  demo), a per-demo live-run table, and a controls table corrected against the
  widget source (Shift+click adds to selection, Ctrl+A selects all, Ctrl+drag
  cuts edges, Shift+drag forks an edge). Internal sections (dependency list,
  project tree, architecture duplicate) moved out or dropped.
- The sort/fine cull kernel dispatches one workgroup per LIVE coarse tile
  (1D-flat; the kernel binary-searches its draw over the `coarse_base`
  prefix sums, fed by a small uniform since `arrayLength` reports capacity).
  The old (largest grid) x (draw count) dispatch launched ~120k workgroups
  on the 500-node scene with 99% dead on arrival; their launch overhead was
  77% of the cull pass and read as DRAM/L2 saturation. Cull GPU time drops
  3.8x (2.7 ms -> 0.72 ms at base clocks; interaction-frame GPU total
  3.4 ms -> 1.45 ms), output pixel-identical.
- Test mock renderers use real `iced_graphics` paragraph/editor types instead
  of `()` (whose iced_core impls are debug_assertions-gated), so
  `cargo test --release` compiles across the workspace. Demo style-overlay
  setters take `f32` directly, resolving Rust's deprecated
  `f32: From<f64>` literal fallback (rust-lang/rust#154024) ahead of it
  becoming a hard error.
- `Camera2D` clamps zoom at every entry point (`ZOOM_MIN`/`ZOOM_MAX`, non-finite
  input falls back to 1.0): a zero/NaN zoom restored from persistence can no
  longer panic the inverted camera transform.
- The WGSL/Rust layout constants (tile strides, slot caps, flags) are guarded
  by a consistency test; the test-side duplicates now import the production
  constants.

### Fixed

- The GPU frame probe (`gpu_frame_times`) now mirrors iced_wgpu's
  per-primitive viewport/scissor clipping. Previously every instance
  rasterized the full canvas, inflating the production-faithful fragment
  measurement ~10x on the 500-node scene (6.4 ms -> 0.6 ms); the node clips
  also sit at their real screen positions instead of stacked at the origin.
- A pan-button press during a node/edge/box drag (or a left press during a
  pan) no longer hijacks the drag state machine mid-drag: the in-progress
  drag would be silently discarded without `on_drag_end` or a committed
  move/camera. Entry transitions now require an idle drag state.
- `Tiling::grid`/`triangles`/`hex` line `thickness` now takes effect in the
  SDF shader (previously packed but never read; only `Dots` consumed its
  parameter). The widget's style-side `expand` workaround was removed.
- Command+Click edge cut now hit-tests the rendered bezier instead of the
  straight chord between pins, so clicking the visible curve cuts it and
  clicking empty space near the invisible chord does not.
- Pin-click, edge-cut and snap/unsnap thresholds are screen-space (divided
  by zoom at each comparison), keeping hit targets a constant on-screen size
  across the 0.1x-10x zoom range instead of shrinking to sub-pixel when
  zoomed out.
- `push_node` ignores a duplicate node id deterministically in release
  builds (first push wins; debug builds still assert) and node-id lookups
  are O(1) via an id-to-index map instead of a linear scan.
- `Pattern::dashed_angle`/`arrowed_angle` clamp the cap angle to +-1.2 rad;
  values near +-pi/2 degenerated the shader's `tan`/`cos` dash math into
  NaN or invisible strokes.
- The draw path builds the per-node pin table once per frame instead of
  re-walking the widget tree (`find_pins`) per edge endpoint, drag preview,
  foreground and diagnostics pass.
- The shader_editor demo removes the matching shader-graph connection when
  an edge is unplugged (visual pin indices were compared against socket
  indices, so disconnects never matched and stale connections accumulated).

## [0.3.0] - 2026-07-10

### Added

- Coarse-slot overflow telemetry: `SdfStats::coarse_demand_max` /
  `coarse_overflow_tiles` report the true per-tile demand of the scatter cull
  via a non-blocking async readback (one frame delayed), making first-come
  slot drops in pathologically dense tiles observable instead of silent. Zero
  cost when nothing overflows.

### Changed

- SDF geometry buffers (segments/entries/styles) are persistent arenas with
  content-keyed, refcounted residency: reuse survives any draw reorder, so a
  selection z-resort or node add/remove re-evaluates only the primitives that
  actually changed (was: everything after the first change, a ~2-3 ms hitch on
  500 nodes). Shape residency also skips the biarc fit for unmoved edges on a
  background rebuild; cold prepare on the 500-node scene drops ~7-9 ms ->
  ~5 ms. Unused blocks age out after 8 frames; a rare compaction
  (`SdfStats::arena_compactions`) resets the arenas when fragmented. New
  per-frame counters: `SdfStats::resident_hits` / `geometry_rebuilds`.
- Rebuilt the SDF tile cull as a scatter pipeline (per-segment/per-entry
  scatter + deterministic per-tile sort): index-build GPU time drops ~4.4x on
  a 500-node scene, output pixel-identical. Coarse tiles grow to 512 slots
  (16-bit fine references), removing overflow drops in dense overviews. Each
  compute pipeline stays within the WebGPU spec-default 8 storage buffers per
  stage, keeping wasm/WebGPU supported.
- The spatial index is reused across frames when camera, viewport and geometry
  are unchanged: idle redraws and animation-only frames skip the cull dispatch
  (`SdfStats::cull_skipped`).
- `Shape` recipe hashes are computed once at construction (head struct) instead
  of two tree walks per entry per frame.
- Node shadows push in stable node order instead of selection z-order, so a
  selection click no longer rebuilds the whole background layer (all edge
  biarcs included). Overlapping identical shadows blend identically; differing
  custom shadow styles may shift marginally in the overlap.

### Removed

- Write-only `bounds` field of the GPU draw entry (80 -> 64 bytes per entry).

### Fixed

- Two latent slot-reuse hazards (pre-existing, found in the release review,
  now regression-tested): a primitive rebuilding in place with unchanged
  buffer counts (e.g. a recolor) no longer leaks its new bytes into later
  primitives that reference its segment/style slots; a primitive that goes
  empty for a frame invalidates its slot record instead of stale-matching
  overwritten buffer ranges on revival.
- Fine-tile reference lists are re-sorted after keep-nearest eviction, so an
  overflowing 16px tile can no longer split one entry into multiple runs
  (double compositing).

## [0.2.0] - 2026-06-29

### Added

- Composable `can_connect` helpers and a richer default connection rule.
- Debug-assert that node ids are unique on push.
- `GraphInfo` + `info()` callback exposing per-frame counts and CPU op timings.
- Theme-driven tiling background on `GraphStyle` (`TilingBackground`/`TilingKind`:
  grid, dots, triangle, hex).

### Changed

- Rewritten arc-only SDF v3 renderer with substantial performance gains.
- Interactions are gated on whether their handler is set.
- Style system maps theme colors through the palette instead of hand-mixing.
- Demos self-drive redraws; the external frame clock was dropped.

### Removed

- Legacy SDF v2 renderer.

### Fixed

- All animated primitives are reported for redraw, fixing idle-animation updates.

## [0.1.0] - 2026-06-16

Initial release.

### Added

- Node graph editor widget for Iced 0.14 with type-safe coordinate transforms
  (`WorldPoint`/`ScreenPoint`, `Camera2D` zoom/pan, zoom-at-cursor).
- Type-safe `PinRef` connection endpoints and `pin!()` macro.
- Interactive node dragging, single- and multi-select, box selection, group move.
- Clone (Ctrl+D) and delete (Delete) with selection sync across all demos.
- Controlled camera and selection via `view()`/`selection()`, with `on_pan`,
  `on_connect`, `on_disconnect`, `on_move`, `on_select`, `on_clone`, `on_delete`
  and `can_connect` callbacks.
- Plug-style edge connections: connect/disconnect fire on snap during drag.
- `iced_nodegraph_sdf`: segment-based SDF renderer with exact distance fields
  (lines, arcs, cubic beziers) and boolean contour operations (union, difference,
  intersection, merge) for clean pin cutouts.
- SDF `Layer`/`Pattern` API for fill, gradient, outline, border, shadow, blur,
  and expand effects on nodes, edges, and pins.
- Z-ordering by last-moved with selected nodes drawn on top.
- Demos: `hello_world`, `styling`, `interaction`, `500_nodes`, `shader_editor`.

[0.5.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.5.0
[0.4.2]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.2
[0.4.1]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.1
[0.4.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.0
[0.3.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.1.0
