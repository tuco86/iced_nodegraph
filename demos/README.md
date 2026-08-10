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
Selection and group move are supported. A toggleable stats overlay reports
per-frame op timings and node/pin/edge counts with cull rates, and the
environment knobs below isolate individual SDF layers.

**Run:** `cargo run -p demo_500_nodes`

### [shader_editor](./shader_editor/)

Visual WGSL shader graph with a category-grouped command palette, typed
sockets, and a compiler that validates the graph and generates WGSL.

**Run:** `cargo run -p demo_shader_editor`

## Diagnosing GPU cost

The `500_nodes` demo doubles as a GPU-cost reporter. `iced_wgpu` 0.14 hardcodes
`required_features: wgpu::Features::empty()`, so a shipped iced app **cannot**
use `TIMESTAMP_QUERY` - there are no GPU timings here. Instead the demo prints
hardware-independent **work and memory counters** plus the frame interval, and
you vary the configuration to find which axis the cost follows.

Run each of these once and paste the report lines:

```bash
NG_REPORT=1                cargo run --release -p demo_500_nodes   # baseline
NG_REPORT=1 NG_SCALE=0.5   cargo run --release -p demo_500_nodes   # quarter the physical pixels
NG_REPORT=1 NG_SCALE=1.5   cargo run --release -p demo_500_nodes   # a 150% Windows desktop
NG_REPORT=1 NG_NO_GRID=1   cargo run --release -p demo_500_nodes   # drop the tiling background layer
NG_REPORT=1 NG_NO_EDGES=1  cargo run --release -p demo_500_nodes   # drop every edge
NG_REPORT=1 NG_NODES=125   cargo run --release -p demo_500_nodes   # quarter the draw count
```

A report line prints every 60 frames:

```
frames  mean ms  p95 ms  draws  shaded Mpx  evals M  fine max  gpu MiB  index MiB  upload KiB  traffic KiB  cull_skipped
```

### Reading it

Frame intervals are **vsync-capped**. The absolute value means nothing while the
renderer keeps up; the signal is *which configuration makes the interval leave
the vsync floor*, and how fast it grows past it.

| Observation | Conclusion |
|---|---|
| interval tracks `NG_SCALE`^2, and `shaded Mpx` with it | fragment/fill-bound - the renderer is paying per physical pixel |
| `NG_NO_GRID` or `NG_NO_EDGES` gives a large win | fragment-bound in that specific layer |
| interval tracks `NG_NODES` at constant `shaded Mpx` | draw-count or index-build bound |
| `upload KiB` well above `draws * 96 B` on an idle graph | the RAM->GPU bandwidth hypothesis has a real target |
| `fine max` sits at 64 | the fine slot cap is saturated: `evals M` under-counts real demand |

`cull_skipped: true` means the frame reused the resident spatial index and ran
no cull dispatch at all - the steady state for a static graph.

### Env knobs

| var | effect |
|---|---|
| `NG_REPORT=1` | force the index probe on and print the report line |
| `NG_SCALE=<f32>` | application scale factor; physical pixels scale with `scale^2` |
| `NG_NODES=<n>` | keep only the first N nodes (and the edges between them) |
| `NG_NO_EDGES=1` | keep the nodes, draw no edges |
| `NG_NO_GRID=1` | remove the tiling background, one full-canvas SDF layer |

The same counters are on `GraphInfo` (`sdf_draws`, `sdf_shaded_px`,
`sdf_segment_evals`, `sdf_gpu_bytes`, `sdf_index_bytes`, `sdf_upload_bytes`,
`sdf_index_traffic_bytes`, `sdf_cull_skipped`), so any application can report
them; `iced_nodegraph_sdf::set_index_probe(true)` arms the ones that need the
per-fine-tile readback.

For the host-side decomposition (resolution / DPI / composition / draw-count /
ALU-amplification sweeps and a dominant-term verdict), run the in-tree probe:

```bash
cargo test -p iced_nodegraph_sdf --release gpu_cost_report -- --ignored --nocapture
```

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

The `interaction` demo owns a `ScreenshotHelper` for the
`--screenshot <path.png>` CLI flag used in documentation captures. Wiring it up
is per demo (state field, `Message` variant, `update` arm, subscription);
`interaction` is the only demo that currently supports the flag, and shows the
full pattern:

```bash
cargo run -p demo_interaction --bin interaction -- --screenshot shot.png
```

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
