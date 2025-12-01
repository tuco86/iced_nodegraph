# iced_nodegraph

A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework, featuring GPU-accelerated rendering with custom WGPU shaders and type-safe coordinate transformations.

## Iced 0.14 Ready!

**Fully updated and tested with Iced 0.14**  
**All warnings fixed and dependencies optimized**  
**Smooth animations restored (droppable pins pulsing)**  
**Cross-platform support (Windows, macOS, Linux)**  

**[Live Demo](https://tuco86.github.io/iced_nodegraph/iced_nodegraph/) | [Documentation](https://github.com/tuco86/iced_nodegraph/tree/main/docs) | [Demos](https://github.com/tuco86/iced_nodegraph/tree/main/demos)**

## Project Structure

This is a **Cargo workspace** containing:

- **`iced_nodegraph/`** - Core library (the node graph widget)
- **`demos/`** - Demonstration projects showcasing features
  - [`hello_world`](demos/hello_world/) - Basic usage and command palette
  - [`styling`](demos/styling/) - Theming and visual customization
  - [`interaction`](demos/interaction/) - Pin rules and connection validation

See [`docs/architecture.md`](docs/architecture.md) for detailed workspace documentation.

## Development Status

**This project is actively being developed with AI assistance (Claude Sonnet 4.5) and is in a state of flux.** Many features are still being refactored and the API may change significantly. Use at your own risk.

**Target**: Iced 0.14 - This library uses features from Iced master branch (post 0.13.1) and has an unstable API.

## Features

- **Nodes** - Draggable containers for your custom widgets
- **Pins** - Connection points on nodes with type checking and visual feedback
- **Edges** - Connect pins to build data flow graphs
- **Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
- **Zoom & Pan** - Smooth infinite canvas navigation
- **GPU Rendering** - High-performance visualization with custom WGPU shaders
- **Smooth Animations** - Monitor-synchronized pin pulsing and transitions
- **Theme Support** - Integrates with Iced's theming system

## Quick Start

**Note:** Requires Iced from master branch (targeting 0.14 release)

### As a Library User

Add to your `Cargo.toml`:

```toml
[dependencies]
iced_nodegraph = { git = "https://github.com/tuco86/iced_nodegraph" }
iced = { git = "https://github.com/iced-rs/iced", features = ["advanced", "wgpu"] }
```

Basic example:

```rust
use iced_nodegraph::{node_graph, node_pin, PinSide, PinDirection};
use iced::{Color, Element, Point};

fn view(&self) -> Element<Message> {
    let mut ng = node_graph()
        .on_connect(|from_node, from_pin, to_node, to_pin| {
            Message::EdgeConnected { from_node, from_pin, to_node, to_pin }
        });

    // Add nodes with pins
    ng.push_node(Point::new(200.0, 150.0), my_node_widget());
    ng.push_node(Point::new(525.0, 175.0), another_node());
    
    // Add edges between nodes
    ng.push_edge(0, 0, 1, 0);  // Connect node 0 pin 0 to node 1 pin 0

    ng.into()
}
```

See [`demos/hello_world/`](demos/hello_world/) for a complete working example.

### Running Demos

```bash
# Clone and navigate to workspace
git clone https://github.com/tuco86/iced_nodegraph
cd iced_nodegraph

# Run hello_world demo
cargo run -p iced_nodegraph_demo_hello_world

# Run styling demo
cargo run -p iced_nodegraph_demo_styling

# Run interaction demo  
cargo run -p iced_nodegraph_demo_interaction
```

## Building

### Workspace Build

```bash
# Build entire workspace (library + all demos)
cargo build --workspace

# Build only the core library
cargo build -p iced_nodegraph

# Build specific demo
cargo build -p iced_nodegraph_demo_hello_world

# Run tests
cargo test -p iced_nodegraph
```

### Documentation with WASM Demo

```bash
# Build WASM demo and generate docs (Windows)
.\build_demo_wasm.ps1
cargo doc --workspace --no-deps --open

# Build WASM demo and generate docs (Linux/macOS)
chmod +x build_demo_wasm.sh
./build_demo_wasm.sh
cargo doc --workspace --no-deps --open
```

**Requirements:**
- `wasm-pack` (`cargo install wasm-pack`)
- WebGPU-capable browser (Chrome/Edge 113+, Firefox 119+)

### Demo-Specific Commands

```bash
# Run from workspace root
cargo run -p iced_nodegraph_demo_hello_world

# Or navigate to demo directory
cd demos/hello_world
cargo run
```

## Architecture

### Workspace Structure

```
iced_nodegraph/                    # Workspace root
├── Cargo.toml                     # Workspace manifest
├── iced_nodegraph/                # Core library
│   ├── Cargo.toml
│   └── src/
│       ├── node_grapgh/           # Main widget
│       │   ├── camera.rs          # Coordinate transformations (15 tests)
│       │   ├── widget.rs          # Widget implementation
│       │   ├── state.rs           # Interaction state
│       │   ├── euclid.rs          # Type-safe coordinates
│       │   └── effects/           # WGPU rendering
│       │       ├── pipeline/      # GPU rendering pipeline
│       │       │   ├── mod.rs     # 5-pass instanced rendering
│       │       │   ├── shader.wgsl # SDF-based vertex/fragment shaders
│       │       │   ├── buffer.rs  # Dynamic GPU buffer management
│       │       │   └── types.rs   # Shader uniform structures
│       │       └── primitive/     # Rendering primitive
│       └── node_pin/              # Pin widgets
├── demos/                         # Demo applications
│   ├── hello_world/               # Basic usage
│   ├── styling/                   # Theming
│   ├── interaction/               # Pin rules
│   └── demo_500_nodes/            # Performance benchmark (500 nodes)
└── docs/                          # Documentation
    └── architecture.md            # Detailed architecture
```

See [`docs/architecture.md`](docs/architecture.md) for comprehensive workspace documentation.

### GPU Rendering Architecture

The node graph uses a **custom WGPU rendering pipeline** with **instanced rendering** for scalable performance:

#### Rendering Pipeline (5 Passes)
1. **Background Pass** - Fullscreen grid rendering
2. **Edge Pass** - Instanced quad rendering for Bezier curve edges
3. **Node Pass** - Instanced rounded rectangles with SDF-based pin cutouts
4. **Pin Pass** - Instanced circles with animated pulsing effects
5. **Dragging Pass** - Foreground layer for edge drag preview

#### Key Features
- **SDF (Signed Distance Functions)** - All shapes rendered using mathematical distance fields for crisp edges at any zoom level
- **Instanced Rendering** - Single draw call per entity type (nodes/pins/edges), not per-pixel iteration
- **GPU Storage Buffers** - Node/pin/edge data stored in GPU-accessible buffers
- **Dynamic Buffer Resizing** - Automatic capacity growth as graph complexity increases
- **Animation Support** - Time-based uniforms for smooth pin pulsing on valid drop targets

#### Performance Characteristics
- **Complexity**: O(visible_pixels × primitives_in_viewport) instead of O(screen_pixels × total_nodes)
- **Scalability**: Tested with 500+ nodes and 600+ edges
- **Bottleneck**: Currently all nodes/edges rendered regardless of visibility (Phase 3 frustum culling planned)

**Implementation**: See [`iced_nodegraph/src/node_grapgh/effects/pipeline/`](iced_nodegraph/src/node_grapgh/effects/pipeline/) for shader code and rendering logic.

### Coordinate System

The widget uses two distinct coordinate spaces with compile-time type safety:

- **Screen Space** - Pixel coordinates from user input (mouse, viewport)
- **World Space** - Virtual infinite canvas where nodes exist

Transformations use mathematically consistent formulas. See [`iced_nodegraph/src/node_grapgh/camera.rs`](iced_nodegraph/src/node_grapgh/camera.rs) for implementation details and comprehensive test coverage.

## Interaction

- **Pan**: Middle mouse button drag
- **Zoom**: Mouse wheel (maintains cursor position)
- **Connect Pins**: Left-click on source pin, drag to target pin
- **Disconnect Edges**: Click on pin connection point to unplug (cable-like interaction)
- **Move Nodes**: Left-click and drag node header

## Testing

```bash
# Run all library tests (24 camera + interaction tests)
cargo test -p iced_nodegraph

# Run specific test suite
cargo test -p iced_nodegraph --lib camera

# Run all workspace tests
cargo test --workspace
```

Test coverage includes coordinate transformations, zoom stability, pin detection, and edge click handling.

## Demos

### [hello_world](demos/hello_world/)
Basic node graph with command palette (Cmd+K) for adding nodes and changing themes. Demonstrates fundamental usage patterns.

**Features:**
- Node creation and positioning
- Pin connections
- Camera controls (pan/zoom)
- Command palette with live theme preview

### [demo_500_nodes](demos/demo_500_nodes/)
Performance benchmark demonstrating instanced rendering with 500 nodes and 640 edges. Simulates a realistic procedural shader graph.

**Features:**
- 500-node graph with 7 processing stages (inputs → noise → vectors → math → textures → blending → outputs)
- Real-time FPS monitoring
- Tests rendering scalability at various zoom levels
- Demonstrates bottleneck (zoom performance) before frustum culling implementation

**Run:** `cargo run --release -p demo_500_nodes`

### [styling](demos/styling/) *(Planned)*
Visual customization and theming system. Shows how to create custom node appearances and integrate with Iced's theme system.

### [interaction](demos/interaction/) *(Planned)*
Pin rules and connection validation. Demonstrates input/output directionality, type checking, and visual feedback for valid/invalid connections.

## Known Limitations

- **Edge Rendering**: Static edge rendering between nodes is not fully implemented. Edge dragging works, but persistent edge display needs completion.
- **API Stability**: Expect breaking changes as the library evolves toward 0.14 compatibility.
- **Documentation**: Many areas need better documentation as refactoring stabilizes.
- **Demo Status**: Only hello_world demo is currently implemented. Others are documented and ready for implementation.

## Dependencies

- **iced** (master branch) - Core GUI framework, requires unreleased 0.14 features
- **euclid** - Type-safe 2D coordinate transformations
- **wgpu** - Cross-platform GPU API for custom shaders

## License

See [LICENSE](LICENSE) file for details.
