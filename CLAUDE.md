# Claude Code Instructions for iced_nodegraph

This document provides essential context for Claude Code when working on the iced_nodegraph project.

## Git Commit Message Rules

**Format**: `type(scope): summary` (Conventional Commits)

**Types**: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `style`, `perf`

**Rules**:
- Single line only (no body unless explicitly requested)
- Summary max 60 characters
- Imperative mood: "add", "fix", "remove" (not "added", "fixed")
- Focus on WHY, not WHAT (intention over implementation details)
- No bullet lists, no file listings, no diff dumps

**Examples**:
- `feat(camera): add zoom-at-cursor transformation`
- `fix(wasm): resolve time platform incompatibility`
- `refactor: separate library from demo dependencies`
- `docs: clarify coordinate system formulas`

**Note**: Claude Code automatically adds co-author attribution when creating commits.

## Project Status

**Pre-Release**: This project has not been published to crates.io yet. No backwards compatibility is required - breaking API changes are acceptable.

## Documentation Standards

**CRITICAL**: Use minimal, professional language in all documentation:
- **NO EMOJIS** in code comments, documentation, or console output
- Use clear, technical language without informal expressions
- Status indicators: "VERIFIED", "TESTED", "INCOMPLETE" instead of emoji symbols
- Professional tone in all user-facing text and developer documentation

## Architecture Overview

This workspace contains a node graph editor built on Iced 0.14:

- **`iced_nodegraph`** - Custom node graph widget built on Iced GUI framework *(main project)*
- **`ngwa-rs`** - SpacetimeDB module for backend data persistence (optional)

**Dependencies**: Uses `iced = "0.14"` and `iced_wgpu = "0.14"` from crates.io (upstream).

**Current Status**: Core functionality is complete - node/pin interaction, edge connections, and coordinate transformations are fully functional with type-safe API.

### WASM Browser Compatibility
- **Chrome/Chromium**: Full WebGPU support, recommended browser
- **Firefox**: WebGPU has known buffer-mapping issues (async timing bugs), may crash
- **Safari**: Untested

For WASM demos, Chrome or Chromium-based browsers are recommended.

## Core Architecture Patterns

### Coordinate System Abstraction - VERIFIED & TESTED
The project uses **euclid** crate for type-safe coordinate transformations:
- `WorldPoint`/`ScreenPoint` distinguish coordinate spaces with compile-time type safety
- `Camera2D` handles zoom/pan transformations in `src/node_grapgh/camera.rs`
- Convert between coordinate systems using `IntoIced`/`IntoEuclid` traits in `src/node_grapgh/euclid.rs`

**Critical Transformation Formulas** (mathematically verified):
- **Screen → World**: `world = screen / zoom - position`
  - Implementation: `Transform2D::scale(1/zoom).then_translate(-position)`
- **World → Screen**: `screen = (world + position) * zoom`
  - Applied in rendering pipeline via `draw_with()`
- **Zoom at Cursor**: `new_pos = old_pos + cursor_screen * (1/new_zoom - 1/old_zoom)`
  - Maintains visual stability when zooming

**Test Coverage**: 44 unit tests across camera, state, interaction, and API modules validate all core functionality.

**See `src/node_grapgh/camera.rs` module documentation for complete mathematical derivations and usage patterns.**

### Widget Architecture
- **NodeGraph** (`src/node_grapgh/mod.rs`) - Main container widget managing nodes and edges
- **NodePin** (`src/node_pin/mod.rs`) - Connection points with `PinSide` enum (Left/Right/Top/Bottom/Row)
- **PinReference** (`src/node_pin/mod.rs`) - Type-safe identifier for pin connections (`node_id`, `pin_id`)
- **NodeGraphEvent** (`src/node_grapgh/mod.rs`) - Unified event enum for all graph interactions
- **State Management** (`src/node_grapgh/state.rs`) - Handles dragging states and camera state

### Custom Rendering Pipeline
Uses **WGPU shaders** for high-performance node graph rendering:
- `src/node_grapgh/effects/pipeline/` contains WGPU rendering pipeline
- `shader.wgsl` defines visual appearance of nodes/edges
- Background/Foreground layers for proper rendering order
- GPU-accelerated with custom vertex/fragment shaders

**Edge System**: Fully functional with type-safe API:
- `push_edge(PinReference, PinReference)` adds connections between pins
- Edge dragging and static edge rendering both work
- Shader renders edges in foreground layer with proper bezier curves

**Plug Behavior**: Edge connections behave like physical plugs:
- `EdgeConnected` fires immediately when dragging edge snaps to a compatible pin
- `EdgeDisconnected` fires when moving away from a snapped pin
- Mouse release while snapped keeps the connection; release while not snapped discards the drag
- This provides immediate tactile feedback rather than waiting for mouse release

## Development Workflows

### Building & Testing
```bash
# Build node graph widget
cargo build -p iced_nodegraph

# Run demos
cargo run -p demo_hello_world
cargo run -p demo_styling
cargo run -p demo_500_nodes
cargo run -p demo_shader_editor

# Run tests
cargo test -p iced_nodegraph
```

### SpacetimeDB Integration
```bash
# Install SpacetimeDB CLI first
cd ngwa-rs
spacetime publish  # Deploy backend module
```

## Project-Specific Conventions

### Widget Implementation Pattern
All custom widgets follow Iced's advanced widget pattern:
1. Implement `iced::advanced::Widget` trait
2. Define state type and use `tree::Tag::of::<StateType>()`
3. Handle layout, drawing, and events in separate methods
4. Use `tree::State` for persistent widget state

### Coordinate Transform Pattern
Always use typed coordinates and proper transformation order:
```rust
// Mouse input: Screen → World
let cursor_position: ScreenPoint = cursor.position().into_euclid();
let world_cursor: WorldPoint = camera.screen_to_world().transform_point(cursor_position);

// CRITICAL: Order matters!
// ✅ CORRECT: Transform2D::scale(1/zoom).then_translate(-position)
//    Result: world = screen / zoom - position
// ❌ WRONG: Transform2D::translation(-position).pre_scale(zoom)
//    Result: world = screen * zoom - position (incorrect inverse)
```

**Click Detection Thresholds**:
- `PIN_CLICK_THRESHOLD = 8.0` pixels (in world space)
- `EDGE_CLICK_THRESHOLD = 8.0` pixels (in world space)

### Rendering Effects Pattern
Custom effects use `shader::Primitive` trait:
- Implement `prepare()` for GPU resource setup
- Implement `render()` for actual drawing
- Use `Pipeline` struct to manage WGPU resources

## Key Integration Points

### Iced Framework
- Uses **iced 0.14** from crates.io (upstream)
- Uses advanced renderer features (`iced_wgpu::primitive::Renderer`)
- Requires `features = ["advanced", "wgpu", "tokio"]`

### Cross-Project Dependencies
- SpacetimeDB module (`ngwa-rs`) is independent backend component

## File Organization Logic

```
src/
├── lib.rs                    # Public API exports
├── node_grapgh/              # Main widget (note: typo in original)
│   ├── widget.rs            # Widget trait implementation
│   ├── camera.rs            # 2D camera with zoom/pan
│   ├── euclid.rs           # Coordinate system abstractions
│   ├── effects/            # Custom rendering pipeline
│   │   ├── pipeline/       # WGPU shaders and buffers
│   │   └── primitive/      # Render primitives (nodes, pins, edges)
│   └── state.rs            # Widget state management
└── node_pin/               # Connection point widgets
```

## Public API Reference

### Core Types
```rust
// Type-safe pin reference
pub struct PinReference {
    pub node_id: usize,
    pub pin_id: usize,
}

// Unified event enum
pub enum NodeGraphEvent {
    EdgeConnected { from: PinReference, to: PinReference },
    EdgeDisconnected { from: PinReference, to: PinReference },
    NodeMoved { node_id: usize, position: Point },
    GroupMoved { node_ids: Vec<usize>, delta: Vector },
    SelectionChanged { selected: Vec<usize> },
    CloneRequested { node_ids: Vec<usize> },
    DeleteRequested { node_ids: Vec<usize> },
}
```

### NodeGraph Methods
```rust
// Adding content
ng.push_node(position, element);
ng.push_node_styled(position, element, NodeStyle);
ng.push_edge(PinReference::new(0, 0), PinReference::new(1, 0));
ng.push_edge_styled(from, to, EdgeStyle);

// Event handlers
ng.on_connect(|from_node, from_pin, to_node, to_pin| Message)
ng.on_disconnect(|from_node, from_pin, to_node, to_pin| Message)
ng.on_move(|node_id, position| Message)
ng.on_select(|selected_ids| Message)
ng.on_clone(|node_ids| Message)
ng.on_delete(|node_ids| Message)
ng.on_group_move(|node_ids, delta| Message)

// State queries
ng.node_count() -> usize
ng.edge_count() -> usize
ng.node_position(node_id) -> Option<Point>
ng.edges() -> Iterator<Item = (PinReference, PinReference, Option<&EdgeStyle>)>
```

When adding features, maintain the coordinate system abstractions and follow the effects pipeline pattern for any custom rendering.