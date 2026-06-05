# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `iced_nodegraph_sdf`: segment-based SDF renderer with exact distance fields (lines, arcs, cubic beziers)
- `iced_nodegraph_sdf`: boolean contour operations (union, difference, intersection, merge) for clean pin cutouts
- Closed-loop validation guard on boolean operands (debug assertion plus release auto-close)
- Pin cutouts: node bodies are punctured by per-pin circular cutouts via a single boolean difference
- Z-ordering: nodes are ordered by last-moved, with selected nodes drawn on top
- Edge styling: border, shadow, and outline configuration with the `Pattern` API
- Per-layer SDF tile debug toggles and runtime SDF stats
- Group move and selection sync wired through all demos

### Changed

- Migrated rendering from the legacy CSG SDF path to the segment-based `iced_nodegraph_sdf` crate
- Fullscreen-quad pipeline with per-draw AABB filtering and tight clip bounds

### Fixed

- Tile-boundary seam from `fwidth` derivative undefined behavior, resolved with analytic edge AA
- Stack overlays now wrapped in opaque containers to block input passthrough

## [0.1.0] - 2025-12-09

### Added

- Node graph editor widget with GPU-accelerated WGPU rendering
- Type-safe `PinReference` for edge connections
- `pin!()` macro for concise pin creation
- Interactive node dragging, selection, and multi-select
- Box selection with Shift+click for adding to selection
- Clone nodes with Ctrl+D, delete with Delete key
- Camera zoom/pan with mathematically consistent transformations
- Zoom-at-cursor maintains visual stability
- Custom WGPU shaders for high-performance rendering
- 5-pass instanced rendering pipeline (background, edges, nodes, pins, dragging)
- SDF-based shapes for crisp edges at any zoom level
- Smooth pin pulsing animations for valid drop targets
- Full Iced 0.14 compatibility
- Theme integration with Iced's theming system

### Demos

- `hello_world` - Basic node graph with command palette
- `styling` - Visual customization and theming
- `500_nodes` - Performance benchmark with procedural graph
- `shader_editor` - Visual WGSL shader editor

### Testing

- 44 unit tests covering camera, state, interaction, and API

[Unreleased]: https://github.com/iced-rs/iced_nodegraph/compare/v0.1.0...HEAD
