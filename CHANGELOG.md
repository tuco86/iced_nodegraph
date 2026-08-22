# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- **`iced_nodegraph_sdf` authors geometry through `Shape` alone.** `Curve`,
  `ShapeBuilder` and the `boolean` module are no longer public, and
  `Segment`, `DrawableType` and `TilingType` are crate-private. They were an
  authoring layer with no sink: `SdfPrimitive::push` accepts only a `&Shape`,
  so a `Drawable` built through them could never be submitted. `Shape`'s
  primitives plus the `-` / `|` / `&` operators express the same set algebra,
  and `Drawable` stays public as `Shape::evaluate`'s return type.

- **`PinShape` carries only the two shapes the renderer draws**, `Circle` and
  `Square`. `Diamond` and `Triangle` were declared but never drawn - the pin
  draw path matched `Square` and sent everything else to a circle - so a host
  that selected one got a circle and no diagnostic. The `#[repr(u32)]` and the
  explicit discriminants are gone with them: nothing casts a `PinShape` to its
  discriminant.

- **The `Theme` type parameter is gone** from `NodeGraph`, `Node`, `Edge`,
  `NodePin` and the style-fn types; all of them are now concrete on
  `iced::Theme`. For `NodeGraph` the parameter was already a fiction: its
  `Widget` and `Element` impls spelled the concrete `iced::Theme`, not a
  generic, so no other theme could reach an `Element` - and `node_graph()`
  declared its own `Theme` parameter that shadowed the concrete import, so
  `node_graph::<Msg, MyTheme, R>()` compiled and handed back a graph with no
  `Widget` impl. `NodePin` is the one type that was honestly polymorphic: its
  `Widget`, `From` and constructor were generic over `Theme`, and that
  capability is deliberately dropped, because a `NodePin` only ever renders
  inside a `NodeGraph`, which is monomorphic. Drop the parameter from every
  turbofish and type annotation; `node_graph::<Msg, Renderer>()` is the new
  spelling.

- **`Camera2D` is no longer public.** Eight of its fourteen methods - every
  one that sets or reads pan/zoom - name a type from the crate-private `euclid`
  module. Those were reachable only out of band, by taking a matching `euclid`
  dependency and letting the unit parameter infer; the remaining six (`new`,
  `zoom`, `layer_transformation`, `draw_with`, `update_with`,
  `cursor_screen_to_layout`) were plainly callable. What no caller could ever
  obtain is the camera the widget actually uses: it lives in private widget
  state. The host's camera API is, and was, `NodeGraph::view` in and `on_pan`
  out, both plain `(Point, f32)`.

### Added

- **Resizable nodes.** `Node::resizable(true)` gives a node a bottom-right
  grip; dragging it reports the size its content should have through
  `NodeGraph::on_resize(|node_id, size| ...)`. Both halves are required - the
  widget owns no node size, the host's content layout does, so a grip drag is
  a report the host applies on the next frame, exactly like `on_move` for
  position. Every report is absolute (size at press plus cursor delta, floored
  at 32x24 world px), so applying none, some or all of them still tracks the
  cursor. The grip is 12 screen px across at any zoom, hover reports
  `mouse::Interaction::ResizingDiagonallyDown`, and it is drawn only where it
  is live - no grip without `on_resize`, and the corner of a node that is not
  `resizable` drags it as before.
- **GPU work and memory counters.** `SdfStats` gains `upload_bytes`,
  `gpu_bytes`, `index_bytes`, `sdf_draws`, `shaded_px`, `segment_evals`,
  `fine_slots_max`, `fine_live_tiles`, `fine_evicted_tiles`,
  `fine_evicted_slots` and `index_traffic_bytes`, so any machine can
  self-report SDF GPU resource use. These are work and byte counts, not
  timings: `iced_wgpu` 0.14 hardcodes `Features::empty()`, so a shipped iced
  app cannot use `TIMESTAMP_QUERY`.
- `iced_nodegraph_sdf::set_index_probe` / `index_probe_enabled` arm a second,
  opt-in async readback of the per-fine-tile slot counts. `Sum(fine_counts) * 64`
  is the `eval_segment` call count the fragment shader performs per frame - the
  hardware-independent fragment-cost metric. Off by default: it copies 4 bytes
  per fine tile per culled frame (~480 KiB on a 500-node 1280x768 frame).
- **Fine-slot eviction telemetry.** `fine_counts` now packs the live slot count
  in its low 16 bits and the count of candidates DROPPED at the
  `MAX_FINE_SLOTS` cap in its high 16 bits. Previously a full tile silently
  discarded a candidate - either evicting the resident slot furthest from the
  tile centre or rejecting the newcomer - and `fine_slots_max == 128` could not
  be told apart from "exactly full". A dropped segment that would have been the
  per-pixel nearest renders a wrong distance, so this is a correctness signal.
  Currently 0 on every measured configuration.
- Three non-ignored GPU budget tests on the canonical 500-node scene:
  `gpu_memory_budget_500_nodes` (pipeline and spatial-index bytes),
  `idle_frame_uploads_nothing` (a static graph re-uploads only the per-frame
  draw table) and `fragment_work_budget_500_nodes` (`segment_evals`, plus
  `fine_evicted_tiles == 0` as a hard correctness gate).
- `gpu_cost_report`, an ignored cost-decomposition probe
  (`cargo test -p iced_nodegraph_sdf --release gpu_cost_report -- --ignored --nocapture`).
  It sweeps resolution, DPI scale, scene composition, draw count at near-zero
  fragment work and ALU amplification, then prices the fragment, bandwidth,
  fill and per-draw terms at a configurable target GPU and names the dominant
  one. Tunable through `SDF_PROBE_*` environment variables.
- `GraphInfo` mirrors the SDF counters as `sdf_upload_bytes`, `sdf_gpu_bytes`,
  `sdf_index_bytes`, `sdf_draws`, `sdf_shaded_px`, `sdf_segment_evals`,
  `sdf_fine_slots_max`, `sdf_fine_evicted_tiles`, `sdf_index_traffic_bytes`
  and `sdf_cull_skipped`, so an application can report GPU resource use
  through the existing per-frame diagnostics channel.
- `demos/500_nodes` reporter knobs `NG_REPORT`, `NG_SCALE`, `NG_NODES`,
  `NG_NO_EDGES` and `NG_NO_GRID`, plus the new counters in its stats panel and
  a periodic report line. See "Diagnosing GPU cost" in `demos/README.md`.
- `SdfStats::gpu_dropped_items`: geometry never uploaded because a buffer hit
  the device's `max_storage_buffer_binding_size`. Nonzero means part of the
  scene is absent from the frame.
- `gpu_cost_report` times the scatter and sort halves of the index build
  separately (six timestamps instead of four), because they scale on different
  axes. Measured on the canonical scene: scatter 45.5 us and CONSTANT across
  0.5x-2x resolution (it tracks geometry), sort 254.2 us and rising with
  resolution (it tracks tiles) - so the scatter is 15% of index-build time.

- `NodeGraph::on_edge_delete(Vec<E>)` reports the edges the cutting tool
  destroyed, named by the ids the host passed to `edge!`. Until now the edge id
  went in and never came back out, so a host keyed by edge id had to recover it
  by matching the endpoint pair - `demos/hello_world` did exactly that. This is
  the only path where the widget holds a host-supplied edge: `on_disconnect`
  also fires while a drag leaves a snapped pin, where no host edge exists yet.
  Mirrors `on_delete(Vec<N>)` for nodes: one batched call per cut gesture.
- `SelectionBoxStyle` + `default_selection_box_style(theme)` and
  `CuttingToolStyle` + `default_cutting_tool_style(theme)`: the two interaction
  overlays now follow the crate's one styling shape - a closure over the theme
  with a `default_*_style` base - like nodes, pins and edges already did.
  Both structs expose the stroke width the widget previously kept in a private
  constant, so a host closure can reach everything the default reaches.
- `DragInfo` derives `PartialEq`, like the other public diagnostic types.

### Changed

- **The theme defaults are one mapping instead of six.** Every color the widget
  picks for itself now comes from a single internal role table derived from the
  iced palette, so the same relationships hold in all 22 built-in themes:

  - SURFACES (canvas, grid, node body, node border) are perceptual lightness
    steps away from the window background in Oklab, holding its hue and chroma.
    The previous mapping used `background.weak`/`background.strong`, whose
    `deviate` helper multiplies chroma as it moves lightness - on a dark,
    saturated canvas one step also doubles saturation, which a node graph shows
    as a colored slab under a colored lattice rather than as elevation.
  - MARKS (edge stroke, pin indicator) sit at a fixed fraction of the distance
    from the canvas to the theme's own foreground, so each theme places them
    inside its own contrast range. An edge was `secondary.base`, which is
    derived from background and text alone and therefore looked the same in
    every theme; a pin was `primary.base`, the selection accent.
  - ACCENTS keep the theme's authored hue but are floored to a minimum
    separation from the canvas. `Theme::KanagawaDragon` pairs a `#181616`
    background with a `#223249` primary, which previously selected a node in a
    color 0.09 in lightness from its own background.

  Cross-theme tests pin the contract: the ladder is ordered and equally spaced
  in every theme, marks resolve against canvas and body, accents separate from
  both, a pin never wears the selection accent, and a node body keeps at least
  three quarters of the canvas text contrast.
- **Node bodies are opaque.** `default_node_style` returned `opacity` 0.75
  (dark) / 0.85 (light), so the grid and any edge behind a node showed through
  its content. Selection therefore no longer signals through opacity: a selected
  body is tinted toward the accent, which also identifies it when its border is
  scrolled off screen.
- The canvas grid is opaque one elevation step above the background, rather
  than `background.strong` at 35% alpha. Node shadows are straight down
  (`offset (0, 3)`) and carry the elevation the fill no longer does.
- **`PinStyle::cutout_radius` sets the well a pin opens in the node body.** The
  hole used to be `(radius * 0.4 + border_width) * 2.0`, so it could only be
  resized by resizing the pin, and neither number was reachable from a style
  closure. It is now its own field, defaulted in `default_pin_style` to
  `PIN_CLICK_THRESHOLD` - how far the body steps aside is how far the pin's hit
  area reaches, which is a property of the interaction rather than of how big
  the mark happens to be drawn.
- **`PinStyle::radius` is the drawn radius.** It was scaled by 0.4 before
  reaching the renderer, so the documented "indicator radius in world-space
  pixels" rendered at 40% of its value. The default is 5.0, which draws the
  10-pixel dot the old default's 6.0 never did. A `PinShape::Square` takes the
  area of the circle of the same radius, so changing a pin's shape changes its
  outline and not its visual weight.
- **`PinStatus::ValidTarget` has static feedback again.** Since the valid-target
  pulse was withdrawn - animation invalidates too much of the SDF renderer's
  resident shape cache to be worth it yet - `default_pin_style` ignored its
  status argument entirely, so every pin looked identical during an edge drag. A
  valid target now takes the theme's `success` color with a translucent halo
  filling its cutout: the third and last accent, next to `primary` for selection
  and `danger` for destruction, each still meaning exactly one thing. Both pin
  states resolve to the same indicator recipe and the same node silhouette
  (`valid_target_feedback_costs_no_geometry`), so the feedback repaints resident
  shapes and cannot churn the cache - the property an animated pulse could not
  offer. `node_pin`'s module docs no longer describe the withdrawn pulse.
- **Removed the `PinStyle::data`/`execution`/`control`/`event` presets.** Four
  unused constructors with hard-coded colors that ignored the theme, teaching
  the opposite of the one styling convention; two of them also advertised
  `PinShape::Triangle`/`Diamond`, which the renderer draws as circles. Every
  demo already types its pins the supported way, with
  `PinStyle { color, ..default_pin_style(theme, status) }`.
- **Removed `GraphStyle::new`, `GraphStyle::dark` and `GraphStyle::light`.**
  Two were aliases for `GraphStyle::default` and the third was a hand-picked
  pale canvas, so together they were a second, theme-blind mapping standing
  beside `GraphStyle::from_theme`. A fixed canvas is a struct literal over
  `Default::default()`; a themed one is `from_theme`.
- **Fine tiles are 8x8 pixels instead of 16x16** (`TILE_SIZE 16 -> 8`,
  `COARSE_FACTOR 4 -> 8`; coarse tiles stay 64px), and `MAX_FINE_SLOTS` drops
  128 -> 64. The index keeps a segment when its lower distance bound to the tile
  is below `kbound`, the smallest upper bound any segment achieves over that
  tile - a ceiling that scales with the TILE DIAGONAL, not with the geometry.
  Halving the tile halves the ceiling, so on the canonical 1280x768 scene:
  node `slot/live` 15.30 -> 8.25, edge `slot/live` 12.34 -> 4.72,
  `segment_evals` 10.62 M -> 4.38 M (mean 8.60 -> 3.55 slots/pixel), fragment
  365.2 -> 224.1 us and index sort 172.3 -> 125.2 us. The whole GPU frame goes
  583 -> 393 us, and the 1080p iGPU verdict 11.0 -> 7.2 ms of a 16.7 ms budget
  (66% -> 43% of one 60 Hz frame).
  The sort got FASTER rather than paying for the extra tiles: `cs_sort_fine`
  gave its fine phase `lindex >= 16u` of a 64-thread workgroup, so 48 threads
  idled there; at `COARSE_FACTOR = 8` the 64 fine tiles per coarse tile use all
  64. Cost is index memory, 25.04 -> 37.78 MiB on the budget scene: 4x the fine
  tiles at a quarter the slot capacity each is 2x the fine storage, while the
  coarse level - now half the index - is untouched. The budgets are re-baselined
  accordingly. `MAX_FINE_SLOTS = 64` keeps ~3.4x headroom over the deepest tile
  measured (19, live demo 18) with `fine_evicted_tiles == 0` everywhere; 32
  would have been memory-neutral but leaves too little margin on a cap whose
  overflow silently renders wrong distances.
- `SdfStats::segment_evals` now multiplies the fine-slot sum by `FINE_TILE_PX`
  (`TILE_SIZE^2`) instead of a hardcoded 256, so the fragment-work figure no
  longer silently rescales by 4x with the tile geometry. `index_pruning_ceiling`
  is likewise driven off `TILE_SIZE` rather than a literal 16.
- Bezier arc-spline tolerance `CUBIC_ARC_TOL` raised from 0.05 to 0.1 world
  units. Screen error is `tol * zoom`, so this is <= 1.0 px at the widget's
  maximum zoom of 10 and invisible everywhere below it - at overview zoom it is
  0.024 px. Curved edges drop from 12.0 to 8.0 arc segments, which multiplies
  through the whole pipeline: on the canonical 1280x768 scene the edge layer's
  slots per live tile fall 16.92 -> 12.34, sort+fine 192.8 -> 145.7 us and
  fragment 238.6 -> 183.0 us; the whole GPU frame goes 688.7 -> 576.9 us
  (-16%). Node bodies are unaffected - they are boxes and circles, not beziers.
  0.1 captures about two thirds of the total available saving at a fifth of the
  error: 0.5 would buy only another 9% while flipping pixels outright when
  zoomed in. `Curve::bezier` also stopped hardcoding its own copy of the value.
- `cs_sort_fine` no longer evaluates the segment field at the fine tile centre
  for OPEN entries. That value feeds exactly one consumer, the closed-contour
  interior test, so every stroke and edge computed it and threw it away - once
  per (slot, fine tile) pair, on arcs, which are the expensive segment kind.
  Sort+fine pass at 1280x768: edges-only 231.7 -> 192.8 us (-17%), all layers
  254.2 -> 225.6 us (-11%); nodes are closed contours and are unchanged, which
  is the control. Output stays pixel-identical.
- The `bench_scene` test fixture generated its 640 bezier edges from
  `(i % 25, i % 20)`, a pair with period `lcm(25, 20) = 100` — so 640 edges
  collapsed onto 100 distinct curves stacked ~6.4 deep. Per-tile entry counts,
  and with them the fine-slot demand and the measured fragment cost of the edge
  layer, were inflated far past any real graph. Now `32 x 20 = 640` on the same
  24px pitch, one distinct start per edge. Measured effect on the edge layer at
  1280x768: live fine tiles 406 -> 1614, slots per live tile 67.2 -> 16.9, peak
  slots per tile 128 (the cap) -> 41, fragment cost 398 -> 241 us. Every
  `gpu_cost_report` figure recorded before this change understates the index's
  discrimination and overstates edge cost.
- SDF cull grid is now **world-anchored** instead of screen-anchored. A new
  `DrawData.grid_offset` folds the camera pan into the tile lattice, so tile
  membership depends only on world position, zoom and a tile-quantized window
  base — not the continuous camera. Panning therefore reuses the resident
  spatial index and skips the Stage-2 cull dispatch (`SdfStats::cull_skipped`)
  for every frame that does not cross a 64px coarse-tile boundary (measured: a
  256px pan runs the cull ~4× instead of ~256×). Output is pixel-identical; the
  only cost is a one-coarse-tile apron per axis (a small, constant increase in
  index size). Zoom still reculls as before.
- **Breaking:** neither library depends on the `iced` umbrella crate anymore.
  `iced_nodegraph` now uses `iced_widget` (which re-exports `iced_core` as
  `core` and `iced_renderer` as `renderer`) and `iced_nodegraph_sdf` uses
  `iced_wgpu` (which re-exports `iced_core` as `core` and `wgpu`). All public
  types are unchanged - they were always these crates' types, reached through
  the umbrella's re-exports.

  The umbrella dependency was declared as `iced = { features = ["wgpu"] }`
  without `default-features = false`. Because Cargo unifies features
  additively, that silently switched iced's whole default set back on for every
  downstream application, even one that carefully wrote
  `default-features = false` itself. Two consequences, both now gone:
  `tiny-skia` compiled a second, unused software renderer (`iced_tiny_skia`,
  `softbuffer`, `tiny-xlib`, `kurbo`) into the binary, and `web-colors` forced
  colors to be blended in sRGB rather than linear space.

  Dropping the umbrella also drops the windowing shell (`iced_winit`, `winit`,
  `window_clipboard`, x11/wayland, `mundy`/`zbus`) from the widget's dependency
  tree, where it never belonged: `cargo tree -p iced_nodegraph -e normal` goes
  from 221 to 117 crates.
- **Breaking:** `pub use iced;` is replaced by `pub use iced_widget;` and
  `pub use iced_wgpu;`. Downstream code that reached for
  `iced_nodegraph::iced::*` should depend on `iced` directly, or use
  `iced_nodegraph::iced_widget::core::*`.
- Demos require `iced_palette` 0.1.1, the first release that also drops the
  `iced` umbrella crate. Older versions depended on `iced` with default
  features, which re-enabled `tiny-skia` and `web-colors` for the whole demo
  graph regardless of what the demos themselves asked for. No demo pulls the
  software renderer any more.

**BREAKING.** Selection is a property of the node: `Node::selected(bool)`
replaces `NodeGraph::selection(..)`.

```rust
ng.push_node(node(id, pos, body).selected(self.selection.contains(&id)));
```

`selection()` resolved the given ids against the already-pushed nodes, so calling
it before `push_node` - which is where a builder chain naturally puts it - matched
nothing and was silently dropped. Every demo in this repository did exactly that,
so host-controlled selection had never actually worked. A flag on the node has no
resolution step and no ordering to get wrong.

Selection behaviour itself is unchanged: the widget still keeps a working
selection driven by clicks and the selection box, so a host that only reads
`on_select` needs no changes at all. Marking nodes overrides that working value
whenever the marked set changes - which is what `selection()` was for, minus the
ordering trap. An unchanged marked set leaves the working value alone, so a host
frame that has not caught up cannot undo an interaction.

One visible difference: pressing empty canvas no longer clears the highlight on
press. The selection is replaced when the box closes, so the previous one stays
visible while rubber-banding.

**BREAKING.** The selection rectangle is called the *selection box* everywhere
now, so `DragInfo::BoxSelect` is `DragInfo::SelectionBox`. Previously the type
said `SelectionBoxStyle` while the drag variant said `BoxSelect`.

**BREAKING.** `NodeGraph::box_select_style` is now `selection_box_style`, and
both overlay closures return a named struct instead of a bare `(Color, Color)` /
`Color`:

```rust
ng.selection_box_style(|theme| SelectionBoxStyle {
    border_width: 2.0,
    ..default_selection_box_style(theme)
})
```

A selected node's appearance is no longer a border tweak.
`default_node_style(theme, NodeStatus::Selected)` now expresses it in full - an
accent border, an accent halo ring, full opacity and a deepened shadow - so a
host that wants the theme's selection feedback copies those fields instead of
reconstructing them from two numbers. Every channel is style-level, so switching
selection does not rebuild node geometry.

The edge-cutting trail's stroke width is now specified in screen pixels and
divided by the zoom, matching the selection box and the hit thresholds; it
previously stayed at a fixed world width and thinned out when zoomed away.

### Removed

**BREAKING.** `SelectionStyle` and `GraphStyle::selection_style`. The struct
mixed two scopes: three colors for the transient overlays, which the renderer
read from the configured `GraphStyle`, and `selected_border_color` /
`selected_border_width`, which it never read at all - `default_node_style` built
its own `SelectionStyle::from_theme`, so setting those two through `graph_style`
silently did nothing. The overlays moved to their own style types (above) and the
selected-node look moved into `default_node_style`, leaving `GraphStyle` as what
its name says: the canvas.

### Fixed

- **Shapes went missing after a draw-set change.** A primitive skipped
  re-uploading its cull lists when a per-draw-slot record matched on resident
  block plus list cursors. That predicate cannot express what the skip needs.
  The record is keyed by draw slot while the bytes are keyed by cursor, and the
  cull buffers are one flat arena whose ranges a per-frame cursor hands out, so a
  frame in which a slot is absent lets a LOWER slot write at the same cursor
  while the record survives untouched. Every list element embeds its own draw
  slot, so honouring the stale record scattered the primitive's geometry into a
  foreign draw's tiles and dropped it from its own: a node lost its border, a pin
  its indicator, another pin half of one, all with straight tile-aligned edges.
  It took a particular sequence of moves to hit, which is why it survived since
  the index reuse landed.

  The reuse decision is now CONTENT-based: `Buffer::write_or_skip` takes what the
  caller believes the buffer should hold, compares it against the mirror and
  uploads only on a difference. Ownership of a range is no longer something a key
  has to encode, so the failure is unrepresentable rather than merely detected.
  `Buffer::skip`, the scatter-slot records and the block generation they keyed on
  are gone. Costs nothing: an idle frame's `upload_bytes` and `cull_skipped` are
  unchanged, since what the skip saves is the GPU upload, not the CPU rebuild.

- **Every SDF-drawn color was a gamma step too bright.** `compile::c2v` uploaded
  a `Color`'s sRGB-encoded components as if they were already linear, while
  iced's own quad and text pipelines pack through `graphics::color::pack`. The
  same `Color` therefore rendered as two different colors depending on which
  pipeline drew it - the canvas quad correct, every node body, border, pin, edge
  and shadow lifted (`#3A3C40` came out as `#83858A`). Because the error
  compresses the dark end hardest, it flattened the distinctions between
  palettes: 22 built-in themes rendered node bodies in nearly the same mid grey.
  Colors now route through `graphics::color::pack`, so an SDF surface and an
  iced quad of one `Color` match, gradients interpolate in the target's space,
  and the pixel-test fill predicates derive their expected bytes from the style
  instead of transcribing them.
- **The widget pixel-oracle harness never started a frame.** `edge_grid_pixel`
  drew through `graph.draw` + `Renderer::screenshot` without calling
  `Renderer::reset`, which the iced runtime does per frame and which
  `screenshot()` - unlike `present()` - does not. Every render therefore piled
  another copy of the scene onto the previous layers: the Nth screenshot
  prepared N scenes, so draws, entries and index tiles grew linearly
  (1001 -> 6006 draws over six renders). `edge_grid_stable_across_frames` was
  measuring accumulation, not stability, and passed only because the tile total
  happened to stay under the device-limit fallback within six frames. Edge
  coverage is now byte-identical across frames (52456 six times, previously
  drifting 52494 -> 58035 once the fallback engaged). The same missing reset was
  the real cause behind two tests ignored as "shared-renderer cross-test
  pollution"; both are un-ignored and pass in the full suite.
- **The geometry arenas could exceed the device's storage-binding limit.** The
  tile index has always clamped against `max_storage_buffer_binding_size` and
  degraded to `grid_cols = 0`; `Buffer<T>` grew 1.5x unconditionally until wgpu
  rejected the allocation - a hard failure with no fallback, and the only
  unbounded resource in the pipeline. At the wgpu default of 128 MiB that is
  2_097_152 `GpuSegment`s, with the first overshooting grow at ~1_398_101 live
  segments. Growth now clamps, and every write path checks capacity BEFORE
  mutating, so a refusal needs no rollback: the slot never becomes live and
  consumers bounded by `len()` cannot read it. Refused items are counted in
  `SdfStats::gpu_dropped_items` instead of being silent.
- The bezier tessellation tolerance is now actually tested.
  `bezier_tessellation_matches_a_finer_reference` renders a production-fitted
  curve against a 20x finer reference spline at the widget's maximum zoom, and
  `cubic_tolerance_stays_within_one_screen_pixel_at_max_zoom` pins the stated
  `tol * zoom <= 1.0 px` contract. Every existing bezier test either compared
  like against like (`tiled` vs `untiled` uses the same tolerance for both, so
  it cannot see a tolerance change) or asserted a structural invariant, so a
  tenfold tolerance increase - plainly visible when zoomed in - left all 155
  tests green.

- The doc examples for the `pin!` macro and for the `node_pin` module were
  `ignore`d pseudo-code fragments that did not compile (macro calls in item
  position, undeclared types). They are now real, compile-checked examples, so
  `cargo test --doc` covers the macro surface.

### Internal

- `NodeGraph` stores `Node` and `Edge` values directly instead of decomposing
  them into anonymous tuples on push, so the builders are the single
  representation of a node and an edge.
- The recording-renderer widget tests moved from `src/{clipping,coordinate,
  overlay}_tests.rs` into `tests/{clipping,coordinates,overlay}.rs` and now share
  one fake renderer in `tests/common/record.rs`, replacing three near-identical
  copies. The crate has no `#[cfg(test)]` modules at its root.
- `demos/shader_editor` reports compilation failures through `Display` on
  `CompileError`/`ValidationError` rather than a `{:?}` dump, and refuses to
  generate WGSL for an unhandled node type instead of emitting a stub function
  with a `TODO` comment.
- The criterion benchmark moved from `iced_nodegraph/benches/` into its own
  `iced_nodegraph_bench` workspace member (`cargo bench -p
  iced_nodegraph_bench`). Cargo scopes `dev-dependencies` to the package, not to
  the target that uses them, so every `cargo test -p iced_nodegraph` was
  compiling criterion's 38-crate tree (~59 CPU-seconds) for test binaries that
  never link it. The bench only ever used `iced_nodegraph_sdf`.
- The `iced` dev-dependency drops the `wayland` feature. `winit` refuses to
  compile on Linux without a display backend, so one is required, but the tests
  are headless and never open a window: `x11` costs 7 crates where `wayland`
  costs 34 (the smithay stack plus tiny-skia, pulled in by winit's adwaita
  client-side decorations).

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

[0.4.2]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.2
[0.4.1]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.1
[0.4.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.4.0
[0.3.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.1.0
