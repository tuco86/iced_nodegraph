# Demo: hello_world

An interactive node graph application built on `iced_nodegraph`. Despite the
name, this is the most feature-complete demo: it combines a command palette,
theme switching, a live style-configuration node system, selection and grouping,
and persistent state.

The graph opens with a small example workflow (email trigger, parser, filter,
calendar) wired together, or restores the last saved session on native targets.

## Features

- Command palette (Cmd/Ctrl+Space) for adding nodes and switching themes,
  with fuzzy filtering, keyboard navigation, and nested submenus.
- Multiple node families:
  - Workflow nodes: email trigger, email parser, filter, calendar.
  - Input nodes: float slider, integer slider, boolean toggle, RGB color
    picker, color presets, and enum selectors for edge curve, pin shape, and
    edge pattern type.
  - Math nodes: Add, Subtract, Multiply, Divide. Math nodes can be chained;
    results propagate iteratively through the graph.
  - Config nodes: Node Config, Edge Config, Shadow Config, Pin Config, plus
    Apply to Graph and Apply to Node.
- Live style configuration: connect input nodes (sliders, color pickers) to
  config nodes, then route those into an Apply node to drive the graph's
  appearance (corner radius, opacity, colors, borders, shadows, edge stroke
  and pattern, pin shape and size). Changes apply immediately as values flow.
- Theme switching across 22 built-in Iced themes, with live preview while the
  theme submenu is open.
- Selection, clone, delete, and group-move for nodes, with box selection and an
  edge cutting tool.
- Pan and zoom with cursor-anchored zoom.
- State persistence (native only): nodes, edges, theme, camera, window
  geometry, and config-section expansion are saved to disk and restored on
  launch.

## Controls

- Cmd/Ctrl+Space - Open or close the command palette.
- Cmd/Ctrl+N - Jump straight to the Add Node submenu.
- Cmd/Ctrl+T - Jump straight to the Change Theme submenu.
- Cmd/Ctrl+E - Export the current graph state to a file (native only).
- Arrow Up / Arrow Down - Navigate palette entries.
- Enter - Confirm the selected palette entry.
- Escape - Cancel the palette (reverts any theme preview).
- Drag a node - Move it; group selections move together.
- Drag from a pin - Create a connection to a compatible pin.
- Click an edge - Cut the connection.
- Scroll - Zoom in or out at the cursor.
- Middle-drag - Pan the canvas.

## Running

```bash
cargo run -p demo_hello_world
```

## Notes

- Persistence is native only. On WASM the graph lives in memory for the
  session, and state export is disabled.
- Saved state is written to the OS data directory, for example
  `%APPDATA%\iced_nodegraph\demo\state.json` on Windows.
- Node and edge identifiers use NanoID strings; pins are identified by stable
  string labels.
- WebGPU is required for the browser build; Chromium-based browsers are
  recommended.
