# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[0.1.0]: https://github.com/tuco86/iced_nodegraph/releases/tag/v0.1.0
