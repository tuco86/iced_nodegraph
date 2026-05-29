# iced_nodegraph Demos

This directory contains demonstration projects showcasing different aspects of
the `iced_nodegraph` library. All demos are implemented and runnable.

## Demo Projects

### [hello_world](./hello_world/)

The most feature-complete demo. A pre-built workflow graph with a command
palette (Cmd/Ctrl+Space), 22 theme presets, live style-config nodes,
selection, clone, delete, group move, edge cutting, and native persistence.

**Run:** `cargo run -p demo_hello_world`

### [styling](./styling/)

Visual customization and theming. Node presets, theme switching, and live
style controls (corner radius, opacity, border width) applied to the selection.

**Run:** `cargo run -p demo_styling`

### [interaction](./interaction/)

Typed pin connection validation: input-only, output-only, and bidirectional
pins; type compatibility; single-connection and duplicate rules; self-loop
rejection; and live snap feedback via `can_connect`.

**Run:** `cargo run -p demo_interaction`

### [500_nodes](./500_nodes/)

Performance benchmark with a procedurally generated graph of 500+ nodes.
Selection and group move are supported, with per-layer SDF debug toggles and a
runtime stats overlay.

**Run:** `cargo run -p demo_500_nodes`

### [shader_editor](./shader_editor/)

Visual WGSL shader graph with a category-grouped command palette, typed
sockets, and a compiler that validates the graph and generates WGSL.

**Run:** `cargo run -p demo_shader_editor`

## Building Demos

```bash
# Build the whole workspace
cargo build --workspace

# Run a specific demo from the workspace root
cargo run -p demo_hello_world

# Or run from the demo directory
cd demos/hello_world
cargo run
```

The shared `demos/common` crate provides a `ScreenshotHelper` for the
`--screenshot <path.png>` CLI flag used in documentation captures.

## Demo Structure

```
demos/<demo_name>/
|-- Cargo.toml           # Demo-specific dependencies
|-- README.md            # Demo documentation
`-- src/
    |-- main.rs          # Native entry point
    |-- lib.rs           # Application logic (shared with the WASM target)
    `-- ...              # Demo-specific modules
```

## Requirements

- Rust (edition 2024)
- `iced = "0.14"` from crates.io
- A WGPU-capable graphics driver (WebGPU for the WASM builds; Chrome or another
  Chromium-based browser is recommended)
