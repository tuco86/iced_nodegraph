# Demo: styling

Node style presets and live per-node style editing.

This demo shows how a host application owns node appearance in `iced_nodegraph`:
it keeps a fully resolved `NodeStyle` per node in its own model, hands it back
from the node's `.style()` closure, and edits it from a side panel while the
graph stays interactive.

The whole application lives in `src/lib.rs` (the native `main.rs` and the WASM
entry point both call into it); `src/nodes/mod.rs` builds the node content.

## Features

- **Style presets**: `NodeStyle::input()`, `process()`, `output()`, and
  `comment()` applied to the selected node from the "Apply Preset" buttons.
- **Live style controls**: sliders for corner radius (0-20), opacity
  (0.1-1.0), and border width (0.5-5.0, written as `Pattern::solid(width)`).
  Each change rewrites the stored `NodeStyle` of the selected node.
- **Theme switching**: a `pick_list` over ten iced themes (Dark, Light, both
  Catppuccin variants, Dracula, Nord, both Solarized, both Gruvbox). The
  starting theme is CatppuccinFrappe.
- **Pin styling by direction**: `styling_pin_style` colors outputs orange and
  everything else blue over `default_pin_style(theme, status)`.
- **Content follows style**: `determine_content_style` picks the title-bar
  preset from the node's fill color and reuses the node's own corner radius and
  border thickness, so the header geometry matches the body.
- **Selection feedback**: when a node's status is `NodeStatus::Selected`, the
  style closure copies the accent border, halo ring and opacity from
  `default_node_style(theme, NodeStatus::Selected)` onto the stored style, so a
  hand-styled node highlights like every other node.
- **Grid background**: a `TilingBackground::grid` layer over
  `GraphStyle::from_theme(theme)`.

## Demo Graph

Four nodes, wired Input -> Transform -> Output, with a detached comment node:

| Node | Preset |
|------|--------|
| 0 Input Data | Input |
| 1 Transform | Process |
| 2 Output Result | Output |
| 3 Note: This is a comment | Comment |

## Controls

- **Click a node button** in the right panel to pick the node the sliders and
  presets act on. Its current style is loaded into the sliders.
- **Drag a node** (or a selection) in the canvas to move it.
- **Drag from a pin** to another pin to connect; each node has one input pin on
  the left and one output pin on the right.
- **Scroll** to zoom, **right-drag** to pan. The root
  [README](../../README.md#controls) has the full default control table.

The 280px control panel is wrapped in `opaque`, so pointer events over it do not
reach the graph underneath.

## Running

```bash
cargo run -p demo_styling
```

For the browser build:

```bash
wasm-pack build demos/styling --release --target web --features wasm
```

`build_docs.sh` (or `build_docs.ps1`) does this for every demo and drops the
result next to the rustdoc output in `target/doc/demo_styling/pkg/`.

## Library API Exercised

`NodeGraph` with `on_connect` / `on_disconnect` / `on_move` / `on_select` /
`selection` / `graph_style`, `node(..).style(..).pin_style(..)`, `edge(..)`,
`PinRef`, `NodeStyle`, `PinStyle`, `GraphStyle`,
`TilingBackground`, `Pattern`, `NodeStatus`, `PinStatus`, `PinDirection`,
`PinInfo`, `default_pin_style`, `node_header`, and the `pin!` macro.
