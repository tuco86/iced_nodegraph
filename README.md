# iced_nodegraph

A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework, featuring SDF-based GPU rendering via a custom WGPU pipeline and type-safe coordinate transformations.

**[Live Demo](https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html) | [Documentation](https://tuco86.github.io/iced_nodegraph/iced_nodegraph/)**

## Project Structure

This is a **Cargo workspace** containing:

- **`iced_nodegraph/`** - Core node graph widget library
- **`iced_sdf/`** - SDF rendering engine (signed distance fields on GPU)
- **`demos/`** - Demonstration applications
  - [`hello_world`](demos/hello_world/) - Basic usage and command palette
  - [`styling`](demos/styling/) - Theming and visual customization
  - [`interaction`](demos/interaction/) - Pin rules and connection validation
  - [`500_nodes`](demos/500_nodes/) - Performance benchmark (500 nodes, 640 edges)
  - [`shader_editor`](demos/shader_editor/) - Visual WGSL shader editor

## Development Status

**Pre-release** - The API may change. Not yet published to crates.io.

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
use iced::{Element, Point};

fn view(&self) -> Element<Message> {
    let mut ng = node_graph()
        .on_connect(|from, to| Message::Connected(from, to))
        .on_move(|node_id, position| Message::Moved(node_id, position));

    ng.push_node(0, Point::new(200.0, 150.0), my_node_widget());
    ng.push_node(1, Point::new(525.0, 175.0), another_node());

    ng.push_edge(PinRef::new(0, 0), PinRef::new(1, 0));

    ng.into()
}
```

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
```

## Building

```bash
cargo build -p iced_nodegraph          # Core library
cargo build --workspace                # Everything
cargo test -p iced_nodegraph           # 69 unit tests
cargo test -p iced_sdf                 # 79 unit + doc tests
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

## Architecture

```
iced_nodegraph/                    # Workspace root
├── iced_nodegraph/                # Core widget library
│   └── src/
│       ├── node_graph/            # Main widget + camera + state
│       ├── node_pin/              # Pin widget
│       ├── style/                 # Styling and config types
│       ├── content.rs             # Layout helpers (header, footer, simple_node)
│       ├── helpers.rs             # Clone, delete, selection utilities
│       ├── ids.rs                 # Generic ID system
│       └── prelude.rs             # Convenience re-exports
├── iced_sdf/                      # SDF rendering engine
│   └── src/
│       ├── shape.rs               # SDF primitives and CSG operations
│       ├── eval.rs                # CPU-side SDF evaluation (hit testing)
│       ├── layer.rs               # Rendering layers (fill, stroke, shadow)
│       ├── pattern.rs             # Stroke patterns (dashed, arrowed, dotted)
│       ├── compile.rs             # SDF tree to RPN compiler
│       ├── primitive.rs           # Iced rendering primitive
│       └── pipeline/              # WGPU pipeline, buffers, shader
└── demos/                         # Demo applications
```

### SDF Rendering Pipeline

The renderer uses signed distance fields evaluated on the GPU:

1. **Compile** - SDF shape trees are compiled to RPN (reverse Polish notation)
2. **Upload** - Shapes, ops, and layers are written to GPU storage buffers
3. **Spatial Index** - A compute shader builds a per-tile shape list for culling
4. **Render** - Fragment shader evaluates only relevant shapes per pixel via tile lookup

All geometry (nodes, edges, pins, shadows, outlines) is rendered through this single pipeline. Layers control appearance: fill, gradient, stroke pattern, blur, expand, and outline.

### Coordinate System

Type-safe coordinate spaces using the `euclid` crate:

- **Screen Space** (`ScreenPoint`) - Physical pixel coordinates
- **World Space** (`WorldPoint`) - Virtual canvas coordinates

Transformations are compile-time checked. See [`camera.rs`](iced_nodegraph/src/node_graph/camera.rs) for formulas and tests.

## Interaction

| Action | Input |
|--------|-------|
| Pan | Middle mouse drag |
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
