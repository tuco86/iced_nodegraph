# iced_nodegraph Demos

This directory contains demonstration projects showcasing different aspects of the `iced_nodegraph` library.

## Demo Projects

### [hello_world](./hello_world/)

**Basic Node Graph Usage**

The simplest possible node graph application. Shows fundamental concepts:

- Creating and displaying a NodeGraph widget
- Adding nodes to the canvas
- Basic camera controls (pan, zoom)
- Connecting pins between nodes
- Handling connection events

**Run:** `cargo run -p hello_world_demo`

### [styling](./styling/)

**Visual Customization and Theming**

Demonstrates theming and visual customization capabilities:

- Custom node styles (colors, borders, shadows)
- Pin appearance customization per type
- Theme switching (light/dark modes)
- Edge styling variations
- Visual feedback states (hover, selected, dragging)

**Run:** `cargo run -p styling_demo`

### [interaction](./interaction/)

**Input/Output Pin Rules and Connection Validation**

Shows how to implement directional data flow and enforce connection constraints:

- Input-only pins (left side) - can only receive edges
- Output-only pins (right side) - can only send edges  
- Bidirectional pins (top/bottom)
- Pin type validation (prevent incompatible connections)
- Multiple edges per pin vs. single connection enforcement
- Visual feedback for valid/invalid connection attempts

**Run:** `cargo run -p interaction_demo`

## Building Demos

### Build All Demos

```bash
cargo build --workspace
```

### Run Specific Demo

```bash
# From workspace root
cargo run -p hello_world_demo
cargo run -p styling_demo
cargo run -p interaction_demo
```

### Run from Demo Directory

```bash
cd demos/hello_world
cargo run
```

## Demo Structure

Each demo project follows this structure:

```
demos/<demo_name>/
├── Cargo.toml           # Demo-specific dependencies
├── README.md            # Detailed documentation and requirements
└── src/
    ├── main.rs          # Application entry point
    └── ...              # Demo-specific modules
```

## Development Guidelines

- Each demo should be self-contained and runnable independently
- READMEs serve as specification documents for AI-assisted initialization
- Keep demos focused on specific feature sets
- Use consistent code style across all demos
- Include comprehensive comments explaining key concepts

## Contributing New Demos

When adding a new demo:

1. Create directory under `demos/`
2. Write comprehensive README.md with:
   - Features demonstrated
   - Implementation requirements
   - Expected output
   - Copilot initialization instructions
3. Add workspace member to root `Cargo.toml`
4. Ensure demo builds and runs correctly
5. Update this README with demo description

## Requirements

All demos require:

- Rust 1.75+ (edition 2024)
- Iced master branch (targeting 0.14 release)
- WGPU-capable graphics driver
- See individual demo READMEs for specific dependencies
