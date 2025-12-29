# iced_nodegraph Workspace Architecture

## Overview

This document describes the organization and structure of the `iced_nodegraph` workspace. The project is organized as a Cargo workspace containing the core library and multiple demonstration projects.

## Workspace Structure

```
iced_nodegraph/                    # Workspace root
├── Cargo.toml                     # Workspace manifest
├── README.md                      # Project documentation
├── LICENSE                        # License information
│
├── iced_nodegraph/                # Core library package
│   ├── Cargo.toml                 # Library dependencies
│   ├── src/
│   │   ├── lib.rs                 # Public API exports
│   │   ├── node_graph/           # Main widget implementation
│   │   │   ├── mod.rs
│   │   │   ├── widget.rs          # Widget trait implementation
│   │   │   ├── camera.rs          # 2D camera transformations
│   │   │   ├── state.rs           # Interaction state management
│   │   │   ├── euclid.rs          # Coordinate system conversions
│   │   │   └── effects/           # WGPU rendering pipeline
│   │   │       ├── pipeline/      # Shader compilation and GPU setup
│   │   │       └── primitive/     # Render primitives
│   │   └── node_pin/              # Pin widget implementation
│   │       └── mod.rs
│   └── target/                    # Build artifacts
│
├── demos/                         # Demonstration projects
│   ├── README.md                  # Demo overview and guidelines
│   │
│   ├── hello_world/               # Basic usage demo
│   │   ├── Cargo.toml
│   │   ├── README.md              # Detailed demo specification
│   │   └── src/
│   │       └── main.rs
│   │
│   ├── styling/                   # Theming and customization demo
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   │       ├── main.rs
│   │       ├── theme.rs           # Theme definitions
│   │       ├── node_styles.rs     # Custom node styles
│   │       └── pin_styles.rs      # Pin customization
│   │
│   └── interaction/               # Pin rules and validation demo
│       ├── Cargo.toml
│       ├── README.md
│       └── src/
│           ├── main.rs
│           ├── pin_types.rs       # Type definitions
│           ├── validation.rs      # Connection validation
│           └── feedback.rs        # User feedback system
│
└── docs/                          # Additional documentation
    └── architecture.md            # This file
```

## Package Organization

### Core Library: `iced_nodegraph`

The core library is located in `iced_nodegraph/` and provides:

- **NodeGraph Widget** - Main container for nodes and edges
- **NodePin Widget** - Connection points with directional constraints
- **Camera System** - Zoom and pan transformations
- **WGPU Rendering Pipeline** - Custom shaders for high-performance rendering
- **Coordinate Systems** - Type-safe screen/world space conversions

**Key Design Principles:**

- Type safety through `euclid` crate coordinate abstractions
- GPU-accelerated rendering with custom WGPU shaders
- Iced 0.14 compatibility using master branch features
- Extensive test coverage (15 camera tests, interaction tests)

### Demo Projects: `demos/*`

Each demo is a standalone binary crate that depends on the core library. Demos are organized by feature focus:

1. **hello_world** - Minimal working example, entry point for new users
2. **styling** - Visual customization and theming capabilities
3. **interaction** - Pin rules, type validation, connection constraints

**Demo Design Philosophy:**

- Self-contained and independently runnable
- READMEs serve as AI initialization specifications
- Focused on specific feature sets
- Educational code with comprehensive comments
- Consistent structure across all demos

## Dependency Graph

```
┌─────────────────────────────────────────┐
│  Workspace Root (Cargo.toml)            │
└──────────────┬──────────────────────────┘
               │
               ├─────────────────────────────────────────┐
               │                                          │
               │                                          │
    ┌──────────▼──────────┐                  ┌───────────▼──────────┐
    │  iced_nodegraph     │                  │  Demo Projects       │
    │  (Library)          │◄─────────────────┤  - hello_world       │
    │                     │                  │  - styling           │
    │  Dependencies:      │                  │  - interaction       │
    │  - iced (master)    │                  │                      │
    │  - iced_wgpu        │                  │  Each depends on:    │
    │  - euclid           │                  │  - iced_nodegraph    │
    │  - wgpu             │                  │  - iced (master)     │
    └─────────────────────┘                  └──────────────────────┘
```

## Build System

### Workspace Configuration

The root `Cargo.toml` defines the workspace:

```toml
[workspace]
members = [
    "iced_nodegraph",
    "demos/hello_world",
    "demos/styling",
    "demos/interaction",
]
resolver = "2"

[workspace.dependencies]
iced = { git = "https://github.com/iced-rs/iced.git", branch = "master" }
iced_nodegraph = { path = "iced_nodegraph" }
```

### Building

```bash
# Build entire workspace
cargo build --workspace

# Build specific package
cargo build -p iced_nodegraph
cargo build -p hello_world_demo

# Run specific demo
cargo run -p styling_demo

# Test core library
cargo test -p iced_nodegraph
```

## Development Workflow

### Adding New Demos

1. Create demo directory: `demos/new_demo/`
2. Write comprehensive `README.md` with:
   - Feature list
   - Implementation requirements
   - Expected behavior
   - Copilot initialization instructions
3. Add to workspace members in root `Cargo.toml`
4. Initialize with Copilot using README as specification
5. Test and verify demo works standalone

### Core Library Development

1. Make changes in `iced_nodegraph/src/`
2. Run tests: `cargo test -p iced_nodegraph`
3. Verify demos still work: `cargo build --workspace`
4. Update documentation if API changes
5. Consider adding demo for new features

## Documentation Strategy

### README Files

- **Root README**: Project overview, quick start, features
- **demos/README.md**: Demo catalog and build instructions
- **Demo READMEs**: Detailed specifications for AI initialization
- **docs/architecture.md**: This file, workspace organization

### Code Documentation

- Inline comments for complex logic
- Module-level docs explaining purpose
- Public API documentation with examples
- Test coverage for critical components

### AI-Assisted Development

Demo READMEs are structured as specifications that enable:

- Copilot to initialize complete demo projects
- Clear requirements and expected outputs
- Consistent structure across demos
- Self-documenting demonstration code

## External Dependencies

### Iced Framework

The project depends on Iced master branch (pre-0.14):

```toml
iced = { git = "https://github.com/iced-rs/iced.git", branch = "master" }
```

**Rationale:** Requires unreleased features for advanced widget implementation.

### Related Workspace

This workspace is part of a larger development environment:

```
c:/workspace/
├── iced/              # Local Iced fork
├── iced_aw/           # Additional widgets library
├── iced_nodegraph/    # This project
└── ngwa-rs/           # SpacetimeDB backend module
```

**Multi-Workspace Configuration:** See `examples/ngwa-rs.code-workspace` for VS Code workspace setup.

## Platform Support

- **Native**: Windows, macOS, Linux with WGPU support
- **WASM**: WebAssembly with WebGPU backend
- **Rendering**: WGPU with custom shaders

## Future Considerations

### Potential New Demos

- **animation** - Node movement, edge flow, transitions
- **serialization** - Save/load graph state
- **undo_redo** - Command pattern implementation
- **minimap** - Overview navigation widget
- **search** - Node search and filtering

### API Evolution

As the library matures toward 0.14 compatibility:

- Stabilize public API surface
- Improve error handling
- Enhance documentation
- Expand test coverage
- Performance optimizations

## References

- [Iced Documentation](https://docs.rs/iced/)
- [WGPU Guide](https://wgpu.rs/)
- [Euclid Crate](https://docs.rs/euclid/)
- Project `.github/copilot-instructions.md` for AI coding guidelines
