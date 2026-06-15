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
//! ## State Query Methods
//!
//! ```rust,ignore
//! let graph: NodeGraph = ...;
//!
//! // Query graph state
//! let count = graph.node_count();
//! let edges = graph.edge_count();
//! let pos = graph.node_position(0); // Option<Point>
//!
//! // Iterate edges
//! for (from, to, style) in graph.edges() {
//!     println!("Edge: {:?} -> {:?}", from, to);
//! }
//! ```

pub use content::{EdgeRadii, node_footer, node_header};
pub use ids::{EdgeId, NodeId, PinId};
pub use node_graph::{
    DragInfo, Edge, Node, NodeGraph, PinRef, SdfDebug, camera::Camera2D, edge, node,
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
    // Built-in status-driven default styles
    default_edge_style,
    default_node_style,
    default_pin_style,
};

// Re-export iced_nodegraph_sdf pattern types for downstream crates
pub use iced_nodegraph_sdf::Pattern;
pub use iced_nodegraph_sdf::pattern::PatternType as SdfPatternType;

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
