<p align="center">
  <img src="assets/logo/logo.svg" alt="iced_nodegraph logo" width="120" height="120">
</p>

<h1 align="center">iced_nodegraph</h1>

A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework, featuring SDF-based GPU rendering via a custom WGPU pipeline and type-safe coordinate transformations.

**[Live Demo](https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html) | [Documentation](https://tuco86.github.io/iced_nodegraph/iced_nodegraph/)**

## Project Structure

This is a **Cargo workspace** containing:

- **`iced_nodegraph/`** - Core node graph widget library
- **`iced_nodegraph_sdf/`** - SDF rendering engine (signed distance fields on GPU)
- **`demos/`** - Demonstration applications
  - [`hello_world`](demos/hello_world/) - Basic usage and command palette
  - [`styling`](demos/styling/) - Theming and visual customization
  - [`interaction`](demos/interaction/) - Pin rules and connection validation
  - [`500_nodes`](demos/500_nodes/) - Performance benchmark (500 nodes, 640 edges)
  - [`shader_editor`](demos/shader_editor/) - Visual WGSL shader editor

## Development Status

**0.1.0** - Initial release. Pre-1.0, so the API may still change between minor versions.

**Target**: Iced 0.14 | **Platforms**: Windows, macOS, Linux, WebAssembly (Chrome)

## Features

- **Nodes** - Draggable containers for custom widgets with configurable styling
- **Pins** - Connection points with type checking, directional flow, and visual feedback
- **Edges** - Bezier curve connections with patterns (solid, dashed, arrowed, dotted)
- **Plug Behavior** - Cable-like connections that snap on hover, not on release
- **Zoom and Pan** - Smooth infinite canvas with zoom-at-cursor
- **Box Selection** - Drag to select multiple nodes, Ctrl+click to toggle
- **SDF Rendering** - All shapes rendered via signed distance fields for crisp edges at any zoom
- **Spatial Index** - Compute shader builds a per-tile index for efficient culling
- **Theme Support** - Integrates with Iced's 22 built-in themes
- **Generic IDs** - Type-safe node, pin, and edge identifiers

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
iced_nodegraph = { git = "https://github.com/tuco86/iced_nodegraph" }
iced = { version = "0.14", features = ["advanced", "wgpu"] }
```

Basic example:

```rust
use iced_nodegraph::prelude::*;
use iced_nodegraph::{edge, node};
use iced::{Element, Point};

fn view(&self) -> Element<Message> {
    let mut ng = node_graph()
        .on_connect(|from, to| Message::Connected(from, to))
        .on_move(|delta, node_ids| Message::Moved(delta, node_ids));

    // Nodes are built with node(id, position, widget) and pushed onto the graph.
    ng.push_node(node(0, Point::new(200.0, 150.0), my_node_widget()));
    ng.push_node(node(1, Point::new(525.0, 175.0), another_node()));

    // An edge connects two pins, addressed by PinRef::new(node_id, pin_id).
    // edge! defaults the edge id to (); use edge(from, to, id) for a custom id.
    ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));

    ng.into()
}
```

`node(..)` and `edge!(..)` return builders, so per-node and per-edge styling can be
chained before pushing: `node(id, pos, w).style(..).pin_style(..)` and
`edge!(from, to).style(..)`.

### Styling presets

Ready-made looks save reinventing them: `NodeStyle::input()` / `process()` /
`output()` and `EdgeStyle::error()` / `disabled()` / `highlighted()` (plus
`data_flow()` and `debug()`). Strokes use `Pattern` (`solid` / `dashed` / `dotted`,
with `.flow(speed)` to animate). A full cookbook - presets, the struct-update
override idiom, and per-node status styling - is in the
[crate docs](https://tuco86.github.io/iced_nodegraph/iced_nodegraph/).

See [`demos/hello_world/`](demos/hello_world/) for a complete working example.

### Running Demos

```bash
git clone https://github.com/tuco86/iced_nodegraph
cd iced_nodegraph

cargo run -p demo_hello_world
cargo run -p demo_styling
cargo run -p demo_interaction
cargo run --release -p demo_500_nodes
cargo run -p demo_shader_editor
cargo run -p sdf_basic                 # iced_nodegraph_sdf example
```

## Building

```bash
cargo build -p iced_nodegraph          # Core library
cargo build --workspace                # Everything
cargo test -p iced_nodegraph           # Core library tests
cargo test -p iced_nodegraph_sdf       # SDF engine tests
cargo clippy --workspace -- -D warnings
```

### WASM Build

```bash
# Windows
.\build_demo_wasm.ps1

# Linux/macOS
./build_demo_wasm.sh
```

Requires `wasm-pack` and a WebGPU-capable browser (Chrome/Chromium recommended).

## Benchmarks

The CPU-side per-frame cost (building node silhouettes, layering, and stroking
edges into one SDF primitive - the work the `on_info()` callback times) is measured
with criterion:

```bash
cargo bench -p iced_nodegraph        # frame_prep: 100 / 500 / 2000 nodes
```

The cost scales roughly linearly with element count. Indicative figures on an
Apple Silicon dev machine: ~1.2 ms at 100 nodes, ~6 ms at 500 nodes (the
`demo_500_nodes` scale), ~22 ms at 2000 nodes. Per-pixel culling runs separately
on the GPU (a compute-shader tile index), so it is not part of this CPU figure.

## Architecture

```
iced_nodegraph/                    # Workspace root
├── iced_nodegraph/                # Core widget library
│   └── src/
│       ├── node_graph/            # Main widget + camera + state
│       ├── node_pin/              # Pin widget
│       ├── style/                 # Styling and config types
│       ├── content.rs             # Layout helpers (node_header, node_footer)
│       ├── ids.rs                 # Generic ID system
│       └── prelude.rs             # Convenience re-exports
├── iced_nodegraph_sdf/                      # SDF rendering engine
│   └── src/
│       ├── drawable.rs            # Segment-based shapes (lines, arcs, beziers)
│       ├── curve.rs               # Shape builders (rect, rounded_rect, circle)
│       ├── boolean.rs             # Boolean ops (union, difference) on contours
│       ├── style.rs               # Distance-stop style chains
│       ├── pattern.rs             # Stroke patterns (dashed, arrowed, dotted)
│       ├── tiling.rs              # Tiling backgrounds (grid, dots, ...)
│       ├── compile.rs             # Drawable + Style -> GPU buffers
│       ├── primitive.rs           # Iced rendering primitive
│       └── pipeline/              # WGPU pipeline, buffers, shader
└── demos/                         # Demo applications
```

### SDF Rendering Pipeline

The renderer uses signed distance fields evaluated on the GPU:

1. **Compile** - shape contours (segments) and styles are compiled to GPU data
2. **Upload** - segments, draw entries, and styles are written to GPU storage buffers
3. **Spatial Index** - a compute shader builds a per-tile segment list for culling
4. **Render** - the fragment shader evaluates only the segments in each pixel's tile

All geometry (nodes, edges, pins, shadows, outlines) is rendered through this single pipeline. A style is a distance-stop chain controlling appearance: fill, gradient, stroke pattern, border, and shadow.

### Coordinate System

Type-safe coordinate spaces using the `euclid` crate:

- **Screen Space** (`ScreenPoint`) - Physical pixel coordinates
- **World Space** (`WorldPoint`) - Virtual canvas coordinates

Transformations are compile-time checked. See [`camera.rs`](iced_nodegraph/src/node_graph/camera.rs) for formulas and tests.

## Interaction

| Action | Input |
|--------|-------|
| Pan | Right mouse drag |
| Zoom | Scroll wheel (at cursor) |
| Connect | Drag from pin to pin |
| Disconnect | Click connected pin to unplug |
| Move node | Drag node header |
| Box select | Left drag on background |
| Toggle select | Ctrl+click |
| Clone | Ctrl+D |
| Delete | Delete key |
| Cut edges | Alt+drag across edges |

## Dependencies

- **iced** 0.14 - GUI framework
- **iced_wgpu** 0.14 - WebGPU renderer
- **euclid** - Type-safe coordinate math
- **glam** - Vector math for SDF evaluation
- **encase** - WGSL buffer layout
- **bytemuck** - Safe transmutation for GPU buffers

## License

See [LICENSE](LICENSE) file for details.
