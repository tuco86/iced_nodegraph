# iced_nodegraph

A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework, featuring GPU-accelerated rendering with custom WGPU shaders and type-safe coordinate transformations.

## ⚠️ Development Status

**This project is actively being developed with AI assistance (Claude Sonnet 4.5) and is in a state of flux.** Many features are still being refactored and the API may change significantly. Use at your own risk.

**Target**: Iced 0.14 (unreleased) - This library requires features from the upcoming 0.14 release and currently builds against the master branch of Iced.

## Features

- **Nodes** - Draggable containers for your custom widgets
- **Pins** - Connection points on nodes with type checking and visual feedback
- **Edges** - Connect pins to build data flow graphs
- **Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
- **Zoom & Pan** - Smooth infinite canvas navigation
- **GPU Rendering** - High-performance visualization with custom WGPU shaders

## Quick Start

**Note:** Requires Iced from master branch (targeting 0.14 release)

Add to your `Cargo.toml`:

```toml
[dependencies]
iced_nodegraph = { path = "../iced_nodegraph" }
iced = { git = "https://github.com/iced-rs/iced", features = ["advanced", "wgpu"] }
```

Basic example:

```rust
use iced_nodegraph::NodeGraph;
use iced::{Element, Point};

let mut node_graph = NodeGraph::new();

// Add nodes at world coordinates
node_graph.push(Point::new(200.0, 150.0), my_node_widget);
node_graph.push(Point::new(525.0, 175.0), another_node);

// Create edges between pins
node_graph.on_connect(|from_node, from_pin, to_node, to_pin| {
    println!("Connected: node {} pin {} -> node {} pin {}", 
             from_node, from_pin, to_node, to_pin);
});

// Convert to Iced Element
let element: Element<Message> = node_graph.into();
```

See [`examples/hello_world.rs`](examples/hello_world.rs) for a complete working example.

## Building

```bash
# Build the library
cargo build

# Run the example
cargo run --example hello_world

# Run tests
cargo test
```

## Architecture

### Coordinate System

The widget uses two distinct coordinate spaces with compile-time type safety:

- **Screen Space** - Pixel coordinates from user input (mouse, viewport)
- **World Space** - Virtual infinite canvas where nodes exist

Transformations use mathematically consistent formulas. See [`src/node_grapgh/camera.rs`](src/node_grapgh/camera.rs) for implementation details and comprehensive test coverage.

### Widget Structure

```
src/
├── node_grapgh/          # Main node graph widget
│   ├── camera.rs        # Camera transformations (15 tests)
│   ├── widget.rs        # Widget trait implementation
│   ├── state.rs         # Interaction state management
│   ├── euclid.rs        # Type-safe coordinate conversions
│   └── effects/         # WGPU rendering pipeline
│       ├── pipeline/    # Shader compilation and GPU setup
│       └── primitive/   # Render primitives (nodes, pins, edges)
└── node_pin/            # Pin widget for node connections
```

## Interaction

- **Pan**: Middle mouse button drag
- **Zoom**: Mouse wheel (maintains cursor position)
- **Connect Pins**: Left-click on source pin, drag to target pin
- **Re-route Edges**: Click on existing edge connection point - the clicked end unplugs like a physical cable
- **Move Nodes**: Left-click and drag node header

## Testing

```bash
# Run all tests (24 camera + interaction tests)
cargo test

# Run specific test suite
cargo test --lib camera
```

Test coverage includes coordinate transformations, zoom stability, pin detection, and edge click handling.

## Known Limitations

- **Edge Rendering**: Static edge rendering between nodes is not fully implemented. Edge dragging works, but persistent edge display needs completion.
- **API Stability**: Expect breaking changes as the library evolves toward 0.14 compatibility.
- **Documentation**: Many areas need better documentation as refactoring stabilizes.

## Dependencies

- **iced** (master branch) - Core GUI framework, requires unreleased 0.14 features
- **euclid** - Type-safe 2D coordinate transformations
- **wgpu** - Cross-platform GPU API for custom shaders

## License

See [LICENSE](LICENSE) file for details.
