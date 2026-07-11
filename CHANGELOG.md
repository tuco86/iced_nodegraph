# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Scripted GPU profiling: `gpu_trace.py` drives the Nsight Graphics CLI
  headlessly and prints per-pass GPU times plus hardware counters (SM/L2/DRAM
  throughput, warp-stall breakdown) for the SDF pipeline, via the new ignored
  `gpu_probe_loop` test; `--demo <name>` traces a demo binary for whole-frame
  GPU times instead. The headless test renderer now honors `WGPU_*` env vars
  (`WGPU_DEBUG=1` on release builds emits pass labels without validation
  overhead). The probe splits the shade pass into per-category markers
  (background / edges / node fills).

### Changed

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

[0.2.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.2.0

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

[0.3.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.1.0
