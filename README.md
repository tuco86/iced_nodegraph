<p align="center">
  <img src="https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/logo/logo.svg" alt="iced_nodegraph logo" width="120" height="120">
</p>

<h1 align="center">iced_nodegraph</h1>

<p align="center">A node graph editor widget for <a href="https://github.com/iced-rs/iced">iced</a>.</p>

<p align="center">
  <a href="https://crates.io/crates/iced_nodegraph"><img src="https://img.shields.io/crates/v/iced_nodegraph.svg" alt="crates.io"></a>
  <a href="https://docs.rs/iced_nodegraph"><img src="https://img.shields.io/docsrs/iced_nodegraph" alt="docs.rs"></a>
  <a href="https://github.com/tuco86/iced_nodegraph/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html">
    <img src="https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/hero.png" alt="The hello_world demo: an email workflow graph with four connected nodes" width="800">
  </a>
  <br>
  <em><a href="https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html">Run this demo in your browser</a> (WebGPU required, Chrome recommended)</em>
</p>

Nodes are ordinary iced widgets - sliders, text inputs, whatever your `view`
builds - placed on an infinite zoom/pan canvas and wired together through typed
pins. The widget holds no graph state: your application owns the data model,
and connections, moves, and deletes arrive as messages, like any other iced
widget. Everything on the canvas is drawn by a single WGPU pipeline as signed
distance fields, so it stays sharp at every zoom level and handles graphs with
hundreds of nodes.

## Getting started

```bash
cargo add iced_nodegraph
cargo add iced --features wgpu
```

```rust
use iced_nodegraph::prelude::*;
use iced_nodegraph::{edge, node};
use iced::{Element, Point};

fn view(&self) -> Element<Message> {
    let mut ng = node_graph()
        .on_connect(|from, to| Message::Connected(from, to))
        .on_move(|delta, node_ids| Message::Moved(delta, node_ids));

    // A node is an id, a position, and any iced widget as content.
    ng.push_node(node(0, Point::new(200.0, 150.0), my_node_widget()));
    ng.push_node(node(1, Point::new(525.0, 175.0), another_node()));

    // An edge connects two pins, addressed as (node id, pin id).
    ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));

    ng.into()
}
```

`node(..)` and `edge!(..)` are builders: chain `.style(..)` for per-node and
per-edge looks, starting from presets like `NodeStyle::input()` or
`EdgeStyle::error()`. The [crate docs](https://docs.rs/iced_nodegraph) cover
styling, connection validation, and the callback contract in detail.

## Demos

Every demo runs natively and in the browser.

| Demo | Live | Shows |
|------|------|-------|
| [`hello_world`](https://github.com/tuco86/iced_nodegraph/tree/main/demos/hello_world) | [run](https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html) | Command palette, theme switching, selection, clone/delete, persistence |
| [`styling`](https://github.com/tuco86/iced_nodegraph/tree/main/demos/styling) | [run](https://tuco86.github.io/iced_nodegraph/demo_styling/index.html) | Style presets and live node styling controls |
| [`interaction`](https://github.com/tuco86/iced_nodegraph/tree/main/demos/interaction) | [run](https://tuco86.github.io/iced_nodegraph/demo_interaction/index.html) | Pin direction and type rules, connection validation, snap feedback |
| [`500_nodes`](https://github.com/tuco86/iced_nodegraph/tree/main/demos/500_nodes) | [run](https://tuco86.github.io/iced_nodegraph/demo_500_nodes/index.html) | 500 nodes / 640 edges stress test with a runtime stats overlay |
| [`shader_editor`](https://github.com/tuco86/iced_nodegraph/tree/main/demos/shader_editor) | [run](https://tuco86.github.io/iced_nodegraph/demo_shader_editor/index.html) | Visual WGSL editor: the graph compiles to a running shader |

```bash
cargo run -p demo_hello_world             # also: demo_styling, demo_interaction, demo_shader_editor
cargo run --release -p demo_500_nodes
```

## Controls

| Action | Mouse / Keyboard | Touch |
|--------|------------------|-------|
| Pan | Right mouse drag | One-finger drag on empty canvas, or two-finger drag |
| Zoom | Scroll wheel (zooms at cursor) | Two-finger pinch |
| Connect | Drag from pin to pin | Drag from pin to pin |
| Disconnect | Click a connected pin to unplug | Tap a connected pin to unplug |
| Fork edge | Shift+drag from a connected pin | - |
| Move node | Drag node | Drag node |
| Box select | Left drag on empty canvas | - (empty-canvas drag pans) |
| Add to selection | Shift+click | - |
| Select all | Ctrl+A | - |
| Clone selection | Ctrl+D (web: Alt+D) | - |
| Delete selection | Delete / Backspace (web: Delete) | - |
| Cut edges | Ctrl+drag across edges | - |

Ctrl is Cmd on macOS. On the web, clone avoids `Ctrl/Cmd+D` (the browser's
bookmark shortcut) and delete drops the `Backspace` alternative (legacy
back-navigation). Every binding is host-rebindable through
`NodeGraph::keymap` - see the `Keymap` type. Connections snap while dragging
near a compatible pin - like plugging in a cable - rather than on mouse
release, and compatible targets pulse during the drag.

## How it works

**Your app owns the graph.** The widget is stateless between frames: it renders
the nodes and edges you pass in and reports intent (`on_connect`, `on_move`,
`on_delete`, ...) through callbacks. Your update logic applies the change and
the next `view` reflects it - the plain iced loop, no hidden state to sync.

**One GPU pipeline.** Node bodies, edges, pins, shadows, and the background
grid are compiled into a single signed-distance-field scene and rendered by
the in-tree [`iced_nodegraph_sdf`](https://github.com/tuco86/iced_nodegraph/tree/main/iced_nodegraph_sdf)
crate. A compute-shader tile index culls per pixel, and animated stroke
patterns drive their own redraws. Details in
[ARCHITECTURE.md](https://github.com/tuco86/iced_nodegraph/blob/main/iced_nodegraph_sdf/ARCHITECTURE.md).

**Typed coordinates.** Screen space and world space are distinct
[`euclid`](https://docs.rs/euclid) types, so mixing them up is a compile error.

## Compatibility

- Targets iced **0.14** with the wgpu renderer (the SDF pipeline has no
  tiny-skia fallback).
- Native: Windows, macOS, Linux. Web: WebAssembly on WebGPU-capable browsers -
  there is no WebGL fallback, so Chrome/Chromium is recommended.
- Pre-1.0: minor releases may change the API. See the
  [CHANGELOG](https://github.com/tuco86/iced_nodegraph/blob/main/CHANGELOG.md).

## Development

The workspace contains the widget (`iced_nodegraph`), the SDF renderer
(`iced_nodegraph_sdf`), and the demos.

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo bench -p iced_nodegraph      # CPU frame-prep cost at 100/500/2000 nodes
./build_docs.sh                    # rustdoc + all demos as WASM (needs wasm-pack)
```

## License

[MIT](https://github.com/tuco86/iced_nodegraph/blob/main/LICENSE)
