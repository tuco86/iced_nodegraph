# iced_nodegraph

A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework, featuring GPU-accelerated rendering and type-safe coordinate transformations.

## Features

- **Type-Safe Coordinate System** - Uses [euclid](https://docs.rs/euclid/) for compile-time coordinate space checking
- **GPU-Accelerated Rendering** - Custom WGPU shaders for high-performance node/edge visualization
- **Camera System** - Smooth zoom and pan with mathematically verified transformations
- **Interactive Node Graph** - Drag nodes, connect pins, re-route edges with intuitive "cable unplugging" behavior
- **Extensible Architecture** - Built on Iced's advanced widget system

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
iced_nodegraph = { path = "../iced_nodegraph" }
iced = { version = "0.13", features = ["advanced", "wgpu"] }
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

All transformations between these spaces are mathematically verified. See [`src/node_grapgh/camera.rs`](src/node_grapgh/camera.rs) for detailed documentation.

**Key Formulas:**
```text
Screen → World: world = screen / zoom - camera_position
World → Screen: screen = (world + camera_position) * zoom
```

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

The coordinate transformation system has comprehensive test coverage:

```bash
# Run camera transformation tests
cargo test --lib camera

# All tests (15 camera tests + integration tests)
cargo test
```

Tests verify:
- Screen ↔ World transformation consistency
- Zoom-at-cursor position stability
- Multiple zoom step accuracy
- Pan + zoom combinations
- Real-world bug scenarios

## Known Limitations

- **Static Edge Rendering**: While edge dragging works perfectly, rendering of persistent edges between nodes is not yet implemented. The WGPU shader has the rendering logic, but the API for managing edge state needs completion.

## Performance

- GPU-accelerated rendering via WGPU
- Efficient coordinate transformations with euclid
- Minimal CPU overhead for interaction handling
- Scales well with large node graphs

## Dependencies

This widget requires a local fork of Iced with WGPU rendering support:

- `iced` (local path: `../iced`) - Core GUI framework
- `iced_wgpu` - GPU rendering backend
- `euclid` - Type-safe 2D coordinate transformations
- `wgpu` - Cross-platform GPU API

## Contributing

This project is part of a larger node-graph editor ecosystem. When contributing:

1. Maintain the type-safe coordinate system abstractions
2. Follow the Iced advanced widget pattern
3. Add tests for coordinate transformations
4. Use the WGPU effects pipeline for custom rendering

## License

See [LICENSE](LICENSE) file for details.

## Related Projects

This workspace includes several interdependent projects:

- **iced_nodegraph** - This node graph widget (current project)
- **iced** - Custom fork of the Iced GUI framework
- **iced_aw** - Additional widgets extending Iced
- **ngwa-rs** - SpacetimeDB backend module for persistence

See [`examples/ngwa-rs.code-workspace`](examples/ngwa-rs.code-workspace) for the complete workspace configuration.
