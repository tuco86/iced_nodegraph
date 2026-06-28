# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
