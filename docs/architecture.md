# iced_nodegraph Workspace Architecture

## Overview

This document describes the organization of the `iced_nodegraph` workspace. The
project is a Cargo workspace containing two library crates (the node graph
widget and the SDF renderer it draws with) and a set of demonstration binaries.

## Workspace Structure

```
iced_nodegraph/                    # Workspace root
|-- Cargo.toml                     # Workspace manifest
|-- README.md                      # Project documentation
|-- CHANGELOG.md
|-- LICENSE
|-- CLAUDE.md                      # Contributor / agent instructions
|-- docs/architecture.md           # This file
|-- plan/                          # Historical design records
|
|-- iced_nodegraph/                # Core widget library
|   |-- Cargo.toml
|   |-- NODE_STYLE_GUIDE.md
|   `-- src/
|       |-- lib.rs                 # Public API exports
|       |-- prelude.rs             # Convenience re-exports
|       |-- ids.rs                 # Generic node/pin/edge id traits
|       |-- content.rs             # Node content layout helpers
|       |-- helpers.rs             # Clone/delete/selection utilities
|       |-- clipping_tests.rs
|       |-- node_graph/
|       |   |-- mod.rs             # NodeGraph, NodeGraphEvent, builder API
|       |   |-- widget.rs          # iced Widget trait: layout, events, draw
|       |   |-- camera.rs          # 2D zoom/pan transforms
|       |   |-- euclid.rs          # Type-safe World/Screen coordinates
|       |   `-- state.rs           # Interaction and drag state
|       |-- node_pin/mod.rs        # NodePin widget, PinReference, PinSide
|       `-- style/
|           |-- mod.rs             # NodeStyle, EdgeStyle, PinStyle, GraphStyle
|           `-- config.rs          # Partial-override config types (merge)
|
|-- iced_nodegraph_sdf/                      # Segment-based SDF renderer
|   |-- ARCHITECTURE.md            # Authoritative renderer design doc
|   `-- src/
|       |-- curve.rs               # Curve / ShapeBuilder contour API
|       |-- drawable.rs            # Compiled Drawable + Segment data
|       |-- compile.rs             # CPU -> GPU buffer compilation
|       |-- boolean.rs             # Union/difference/intersection on contours
|       |-- pattern.rs             # Stroke patterns (solid, dashed, arrowed...)
|       |-- style.rs               # Fill/gradient/outline/blur style
|       |-- tiling.rs              # Repeating grid/dots/triangle/hex patterns
|       |-- primitive.rs           # iced Primitive integration
|       |-- shared.rs              # Lazy shared GPU resources
|       `-- pipeline/              # WGSL shader, GPU types, buffers, tests
|
`-- demos/                         # Demonstration binaries
    |-- README.md
    |-- lib.rs                     # Rustdoc overview of all demos
    |-- common/                    # Shared ScreenshotHelper
    |-- hello_world/
    |-- styling/
    |-- interaction/
    |-- 500_nodes/
    `-- shader_editor/
```

## Package Organization

### Core Library: `iced_nodegraph`

Provides:

- **NodeGraph widget** - container managing nodes, edges, camera, and interaction
- **NodePin widget** - connection points with directional and type metadata
- **Camera system** - zoom and pan with type-safe screen/world transforms
- **Style system** - `Style` types plus `Config` partial-override types that merge

Key design principles:

- Type safety through `euclid` coordinate abstractions (`WorldPoint`, `ScreenPoint`)
- Rendering delegated to `iced_nodegraph_sdf` for exact distance fields at any zoom
- Generic over node id `N` and pin id `P` (both default to `usize`)

### SDF Renderer: `iced_nodegraph_sdf`

A standalone crate that renders contours, strokes, and tilings from exact
signed distance fields. Node bodies with pin cutouts are produced by boolean
contour operations (`difference_many`). See `iced_nodegraph_sdf/ARCHITECTURE.md` for the
full CPU-compile / GPU-compute / fragment pipeline.

### Demo Projects: `demos/*`

Each demo is a standalone binary crate (with a `lib.rs` shared by the native
`main.rs` and the WASM entry point) that depends on the core library. See
`demos/README.md` for the catalog.

## Dependency Graph

```
+-------------------------------+
|  Workspace Root (Cargo.toml)  |
+---------------+---------------+
                |
        +-------+--------+----------------------+
        |                |                      |
+-------v------+  +------v-------+      +--------v---------+
| iced_nodegraph_sdf     |  | iced_nodegraph|     | demos/*          |
| (renderer)   |<-| (widget)      |<----| each depends on  |
|              |  |               |     | iced_nodegraph   |
| deps: iced,  |  | deps: iced,   |     | (+ demo_common)  |
| wgpu, glam,  |  | iced_nodegraph_sdf,     |     |                  |
| encase,      |  | euclid        |     |                  |
| bytemuck     |  |               |     |                  |
+--------------+  +---------------+     +------------------+
```

## Build System

### Workspace Configuration

The root `Cargo.toml` declares the members and shared dependency versions:

```toml
[workspace]
members = [
    "iced_nodegraph",
    "iced_nodegraph_sdf",
    "iced_nodegraph_sdf/examples/basic",
    "demos/common",
    "demos/hello_world",
    "demos/styling",
    "demos/interaction",
    "demos/500_nodes",
    "demos/shader_editor",
]
resolver = "2"

[workspace.dependencies]
iced = "0.14"
iced_nodegraph = { path = "iced_nodegraph" }
```

The project uses the released `iced = "0.14"` from crates.io.

### Common Commands

```bash
# Build the whole workspace
cargo build --workspace

# Library checks (native and WASM)
cargo check -p iced_nodegraph
cargo check -p iced_nodegraph --target wasm32-unknown-unknown

# Tests and lints
cargo test -p iced_nodegraph
cargo test -p iced_nodegraph_sdf
cargo clippy -p iced_nodegraph -- -D warnings

# Run a demo
cargo run -p demo_hello_world
```

## Documentation Strategy

- **Root README**: project overview and quick start
- **demos/README.md**: demo catalog and run commands
- **Demo READMEs**: describe what each implemented demo does
- **docs/architecture.md**: this file, workspace organization
- **iced_nodegraph_sdf/ARCHITECTURE.md**: authoritative SDF renderer design
- **iced_nodegraph/NODE_STYLE_GUIDE.md**: visual node-design guidelines

## External Dependencies

`ngwa-rs` (a SpacetimeDB backend module) is an optional, separate sibling
workspace located at `../ngwa-rs`. It is not a member of this workspace and is
not required to build or run the widget or any demo.

## Platform Support

- **Native**: Windows, macOS, Linux with a WGPU-capable driver
- **WASM**: WebAssembly with the WebGPU backend (Chrome / Chromium recommended;
  Firefox has known WebGPU buffer-mapping issues)

## References

- [Iced Documentation](https://docs.rs/iced/)
- [WGPU Guide](https://wgpu.rs/)
- [Euclid Crate](https://docs.rs/euclid/)
