# AI Coding Instructions for iced_nodegraph

## Documentation Standards

**CRITICAL**: Use minimal, professional language in all documentation:
- **NO EMOJIS** in code comments, documentation, or console output
- Use clear, technical language without informal expressions
- Status indicators: "VERIFIED", "TESTED", "INCOMPLETE" instead of emoji symbols
- Professional tone in all user-facing text and developer documentation

## Architecture Overview

This workspace contains **4 interdependent Rust projects** forming a node graph editor ecosystem:

- **`iced_nodegraph`** - Custom node graph widget built on Iced GUI framework *(main project)*
- **`iced`** - Fork/local version of the Iced GUI library with advanced rendering features
- **`iced_aw`** - Additional widgets library extending Iced's capabilities
- **`ngwa-rs`** - SpacetimeDB module for backend data persistence

**Current Status**: Core functionality is working - node/pin interaction, edge dragging, and coordinate transformations are fully functional. **Edge rendering between nodes is incomplete** (static edge rendering needs implementation).

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

**Test Coverage**: 15 comprehensive tests in `src/node_grapgh/camera.rs` validate all transformations.

**See `src/node_grapgh/camera.rs` module documentation for complete mathematical derivations and usage patterns.**

### Widget Architecture
- **NodeGraph** (`src/node_grapgh/mod.rs`) - Main container widget managing nodes and edges
- **NodePin** (`src/node_pin/mod.rs`) - Connection points with `PinSide` enum (Left/Right/Top/Bottom/Row)
- **State Management** (`src/node_grapgh/state.rs`) - Handles dragging states and camera state

### Custom Rendering Pipeline
Uses **WGPU shaders** for high-performance node graph rendering:
- `src/node_grapgh/effects/pipeline/` contains WGPU rendering pipeline
- `shader.wgsl` defines visual appearance of nodes/edges
- Background/Foreground layers for proper rendering order
- GPU-accelerated with custom vertex/fragment shaders

**WARNING - Known Issue**: Edge rendering is partially implemented but incomplete:
- Edge dragging works (temporary edge while connecting pins)
- Static edge rendering is missing - `edges: vec![]` hardcoded in widget.rs:122
- Missing API: No `push_edge()` method in NodeGraph struct
- Shader has edge rendering logic but it's only used during drag operations

## Development Workflows

### Building & Testing
```bash
# Build node graph widget
cd iced_nodegraph && cargo build

# Run example
cargo run --example hello_world

# Test with different rendering backends
ICED_TEST_BACKEND=tiny-skia cargo test
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

### Iced Framework Coupling
- Depends on local `../iced` and `../iced/wgpu` paths
- Uses advanced renderer features (`iced_wgpu::primitive::Renderer`)
- Requires `features = ["advanced", "wgpu", "tokio"]`

### Cross-Project Dependencies
- **Workspace file**: `examples/ngwa-rs.code-workspace` configures all 4 projects
- Build order matters: iced → iced_aw → iced_nodegraph
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

## Critical Implementation Gaps

**Edge Rendering System Needs Completion**:
1. **Missing API**: Add `push_edge()` method to NodeGraph in `src/node_grapgh/mod.rs`
2. **Widget Integration**: Replace `edges: vec![]` with `self.edges.clone()` in `src/node_grapgh/widget.rs:122`
3. **Shader Extension**: Extend `fs_foreground()` in `shader.wgsl` to render static edges (currently only renders dragging edges)
4. **Edge Management**: Implement edge creation/deletion logic in user interaction handlers

When adding features, maintain the coordinate system abstractions and follow the effects pipeline pattern for any custom rendering.