// Pre-existing warnings allowed at crate level (not part of current refactoring)
#![allow(clippy::type_complexity)]
#![allow(clippy::borrowed_box)]
#![allow(clippy::should_implement_trait)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/logo/logo.svg",
    html_favicon_url = "https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/logo/logo.svg"
)]

//! # iced_nodegraph
//!
//! A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework,
//! featuring SDF-based rendering and type-safe coordinate transformations.
//!
//! ## Features
//!
//! - **Nodes** - Draggable containers for your custom widgets
//! - **Pins** - Connection points on nodes with type checking and visual feedback
//! - **Edges** - Connect pins to build data flow graphs with type-safe [`PinRef`]
//! - **Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
//! - **Selection** - Multi-select with box selection, clone (Ctrl+D), delete (Delete key)
//! - **Zoom & Pan** - Smooth infinite canvas navigation with [`Camera2D`]
//! - **SDF Rendering** - High-performance visualization via signed-distance fields (`iced_nodegraph_sdf`)
//! - **Spatial Index** - A GPU tile index culls geometry per pixel, scaling to large graphs
//! - **Pin Feedback** - Valid drop targets pulse while dragging an edge
//! - **Theme Support** - Integrates with Iced's theming system
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use iced_nodegraph::{NodeGraph, PinRef, edge, node, node_graph};
//! use iced::{Element, Theme, Point, Vector};
//! use iced::widget::text;
//! use iced_wgpu::Renderer;
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     EdgeConnected { from: PinRef<usize, usize>, to: PinRef<usize, usize> },
//!     NodesMoved { delta: Vector, node_ids: Vec<usize> },
//! }
//!
//! fn view(edges: &[(PinRef<usize, usize>, PinRef<usize, usize>)]) -> Element<'_, Message, Theme, Renderer> {
//!     let mut ng = node_graph()
//!         .on_connect(|from, to| Message::EdgeConnected { from, to })
//!         .on_move(|delta, node_ids| Message::NodesMoved { delta, node_ids });
//!
//!     // Add nodes with IDs
//!     ng.push_node(node(0, Point::new(100.0, 100.0), text("Node A")));
//!     ng.push_node(node(1, Point::new(300.0, 100.0), text("Node B")));
//!
//!     // Add edges with type-safe PinRef; edge! defaults the id to ()
//!     for (from, to) in edges {
//!         ng.push_edge(edge!(*from, *to));
//!     }
//!
//!     ng.into()
//! }
//! ```
//!
//! ## Core Types
//!
//! ### [`PinRef`]
//! Type-safe identifier for a pin connection, generic over your node/pin id types:
//! ```rust
//! use iced_nodegraph::PinRef;
//!
//! let pin = PinRef::new(0, 1); // node 0, pin 1
//! assert_eq!(pin.node_id, 0);
//! assert_eq!(pin.pin_id, 1);
//! ```
//!
//! ### [`Camera2D`]
//! Programmatic access to zoom and pan state.
//!
//! ## Styling
//!
//! Node, edge and pin styles are concrete flat structs. Override fields with
//! struct-update over a theme-derived default inside a `.style()` closure:
//!
//! ```ignore
//! node(0, pos, body).style(|theme, status| NodeStyle {
//!     fill_color: ColorQuad::solid(Color::from_rgb(0.2, 0.3, 0.5)),
//!     ..default_node_style(theme, status)
//! });
//! ```
//!
//! ### Ready-made presets
//!
//! Reach for these before hand-rolling a look:
//! - Nodes: [`NodeStyle::input`] (blue), [`NodeStyle::process`] (green),
//!   [`NodeStyle::output`] (orange).
//! - Edges: [`EdgeStyle::error`] (red animated marching-ants with a border ring),
//!   [`EdgeStyle::disabled`] (gray dashed), [`EdgeStyle::highlighted`] (yellow with
//!   a soft ring), [`EdgeStyle::data_flow`] (blue), [`EdgeStyle::debug`] (dotted
//!   cyan straight line).
//!
//! ```ignore
//! edge!(from, to).style(|_theme, _status, _from, _to| EdgeStyle::error());
//! ```
//!
//! ### Stroke patterns
//!
//! [`Pattern`] (re-exported from `iced_nodegraph_sdf`) controls every stroke:
//! `Pattern::solid(width)`, `Pattern::dashed(width, dash, gap)`,
//! `Pattern::dotted(spacing, radius)`, plus `.flow(speed)` to animate it along the
//! stroke. An animated pattern self-drives redraws - no host frame loop needed.
//!
//! ### Per-node status
//!
//! The style closure intentionally does not receive the node id: your `view` loop
//! already has it (and any per-node status). Derive the status there and pass it in;
//! a shared function keeps it DRY across nodes:
//!
//! ```ignore
//! for n in &self.nodes {
//!     let working = self.is_working(n.id);
//!     ng.push_node(node(n.id, n.pos, body).style(move |theme, status| {
//!         let base = default_node_style(theme, status);
//!         if working {
//!             NodeStyle { border_pattern: Pattern::dashed(2.0, 6.0, 4.0).flow(40.0), ..base }
//!         } else {
//!             base
//!         }
//!     }));
//! }
//! ```
//!
//! ## Demonstration Projects
//!
//! ### [hello_world](https://github.com/tuco86/iced_nodegraph/tree/main/demos/hello_world)
//! Basic node graph with command palette:
//! - Node creation and positioning
//! - Pin connections with type colors
//! - Camera controls (pan/zoom)
//! - Theme switching with live preview
//! - Email processing workflow example
//!
//! ```bash
//! cargo run -p demo_hello_world
//! ```
//!
//! ### [styling](https://github.com/tuco86/iced_nodegraph/tree/main/demos/styling)
//! Visual customization showcase:
//! - Custom node styles (colors, borders, opacity)
//! - Live style controls with sliders
//! - Preset styles (Input, Process, Output, Comment)
//! - Theme switching
//!
//! ```bash
//! cargo run -p demo_styling
//! ```
//!
//! ### [500_nodes](https://github.com/tuco86/iced_nodegraph/tree/main/demos/500_nodes)
//! Performance benchmark with 500+ nodes:
//! - Procedural shader graph generation
//! - Stress tests GPU rendering pipeline
//! - Multiple node types and connection patterns
//!
//! ```bash
//! cargo run -p demo_500_nodes
//! ```
//!
//! ### [shader_editor](https://github.com/tuco86/iced_nodegraph/tree/main/demos/shader_editor)
//! Visual WGSL shader editor:
//! - Math, Vector, Color, Texture nodes
//! - Real-time shader compilation
//! - Command palette for node spawning
//!
//! ```bash
//! cargo run -p demo_shader_editor
//! ```
//!
//! ## Platform Support
//!
//! ### Native (Windows, macOS, Linux)
//! Full WGPU rendering via signed-distance fields (`iced_nodegraph_sdf`).
//!
//! ### WebAssembly (Browser)
//! WebGPU only - there is no WebGL fallback. Chrome/Chromium is recommended.
//!
//! ## Architecture
//!
//! ### Coordinate System
//!
//! The widget uses two distinct coordinate spaces with compile-time type safety via the
//! [`euclid`](https://docs.rs/euclid) crate:
//!
//! - **Screen Space** - Pixel coordinates from user input (mouse, viewport)
//! - **World Space** - Virtual infinite canvas where nodes exist
//!
//! Transformations use mathematically consistent formulas:
//!
//! - **Screen -> World**: `world = screen / zoom - position`
//! - **World -> Screen**: `screen = (world + position) * zoom`
//! - **Zoom at Cursor**: `new_pos = old_pos + cursor_screen * (1/new_zoom - 1/old_zoom)`
//!
//! See [`Camera2D`] for implementation details and comprehensive test coverage.
//!
//! ### SDF Rendering
//!
//! Nodes, edges, pins and overlays are drawn with signed-distance fields via the
//! in-tree `iced_nodegraph_sdf` crate - there are no hand-written vertex or
//! fragment shaders in this crate:
//! - Layered compositing for correct draw order (shadows, edges, node bodies)
//! - A GPU tile index culls geometry per pixel, scaling to large graphs
//! - Cross-platform (native WGPU + WebGPU)
//!
//! ## Interaction
//!
//! | Action | Input |
//! |--------|-------|
//! | Pan canvas | Right mouse drag |
//! | Zoom | Mouse wheel (maintains cursor position) |
//! | Connect pins | Left-click source pin, drag to target |
//! | Re-route edge | Click on edge endpoint to unplug |
//! | Move node | Left-click and drag node |
//! | Box select | Left-click on empty space, drag |
//! | Clone selection | Ctrl+D |
//! | Delete selection | Delete key |
//! | Add to selection | Shift+click |
//!
//! ### Plug Behavior
//!
//! Edge connections behave like physical plugs:
//! - **Snap**: When dragging an edge close to a compatible pin, it "snaps" and
//!   `EdgeConnected` fires immediately (not on mouse release)
//! - **Unsnap**: Moving away from the snapped pin fires `EdgeDisconnected`
//! - **Release**: Releasing the mouse while snapped keeps the connection;
//!   releasing while not snapped discards the drag
//!
//! ### What the host owns
//!
//! The widget is stateless between frames and never mutates your data model, so a
//! few invariants are yours to enforce:
//!
//! - **Edge dedupe.** `on_connect` fires on every snap during a drag (not on
//!   release), so one drag can report several connections. The default
//!   [`can_connect`](NodeGraph::can_connect) already rejects a second edge into an
//!   occupied input; for replace-on-drop instead, drop that rule (see
//!   [`connection`]) and remove the prior edge whose input matches in your
//!   `on_connect` handler - `to` is always the input pin.
//! - **Unique node ids.** Lookups resolve to the first match, so reuse renders a
//!   node doubled. Prefer a stable id from your data - a database key, `uuid::Uuid`,
//!   or a typed newtype - over a hand-managed counter (collision-proof, and it
//!   survives multi-client collaboration); debug builds assert uniqueness.
//! - **Applying moves/deletes/clones.** `on_move` / `on_delete` / `on_clone` report
//!   intent; your model applies it and feeds the result back on the next `view`.
//!
//! ## Diagnostics
//!
//! The widget is stateless between frames - the host owns nodes, edges and
//! selection - so there are no `node_count()` / `edges()` query methods. For
//! per-frame metrics (element counts total/in-view/culled and CPU op timings),
//! register a callback with [`NodeGraph::on_info`]; it delivers a [`GraphInfo`]
//! each redraw.

pub use connection::{default_can_connect, direction_ok, input_not_occupied, not_same_node};
pub use content::{EdgeRadii, node_footer, node_header};
pub use ids::{EdgeId, NodeId, PinId};
pub use node_graph::{
    Counts, DragInfo, Easing, Edge, FocusAnimation, FocusOptions, FocusTarget, GraphInfo, Node,
    NodeGraph, OpTiming, PinRef,
    camera::Camera2D,
    edge,
    input::{ComboKey, KeyAction, KeyCombo, Keymap},
    node,
    widget::node_graph,
};
pub use node_pin::{NodePin, PinDirection, PinEnd, PinInfo, PinSide, node_pin};
pub use style::{
    // Unified color type for style fields
    ColorQuad,
    EdgeCurve,
    // Status enums for widget-side styling
    EdgeStatus,
    EdgeStyle,
    GraphStyle,
    NodeStatus,
    // Node/edge/pin style types (concrete; override via struct-update over defaults)
    NodeStyle,
    PinShape,
    PinStatus,
    PinStyle,
    SelectionStyle,
    // Tiling background (grid/dots/...) for GraphStyle
    TilingBackground,
    TilingKind,
    // Built-in status-driven default styles
    default_edge_style,
    default_node_style,
    default_pin_style,
};

// Re-export iced_nodegraph_sdf pattern types for downstream crates
pub use iced_nodegraph_sdf::Pattern;
pub use iced_nodegraph_sdf::pattern::PatternType as SdfPatternType;

pub mod connection;
pub mod content;
pub mod ids;
mod node_graph;
mod node_pin;
pub mod prelude;
pub mod style;

#[cfg(test)]
mod clipping_tests;
#[cfg(test)]
mod coordinate_tests;
#[cfg(test)]
mod overlay_tests;

// Re-export iced for downstream crates
pub use iced;
