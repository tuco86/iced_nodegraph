//! # iced_nodegraph
//!
//! A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework,
//! featuring GPU-accelerated rendering with custom WGPU shaders and type-safe coordinate transformations.
//!
//! ## Features
//!
//! - **Nodes** - Draggable containers for your custom widgets
//! - **Pins** - Connection points on nodes with type checking and visual feedback
//! - **Edges** - Connect pins to build data flow graphs with type-safe [`PinReference`]
//! - **Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
//! - **Selection** - Multi-select with box selection, clone (Ctrl+D), delete (Delete key)
//! - **Zoom & Pan** - Smooth infinite canvas navigation with [`Camera2D`]
//! - **GPU Rendering** - High-performance visualization with custom WGPU shaders
//! - **Smooth Animations** - Monitor-synchronized pin pulsing and transitions
//! - **Theme Support** - Integrates with Iced's theming system
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use iced_nodegraph::{NodeGraph, PinReference, node_graph};
//! use iced::{Element, Theme, Point};
//! use iced::widget::text;
//! use iced_wgpu::Renderer;
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     EdgeConnected { from_node: usize, from_pin: usize, to_node: usize, to_pin: usize },
//!     NodeMoved { node_id: usize, position: Point },
//! }
//!
//! fn view(edges: &[(PinReference, PinReference)]) -> Element<'_, Message, Theme, Renderer> {
//!     let mut ng = node_graph()
//!         .on_connect(|from_node, from_pin, to_node, to_pin| {
//!             Message::EdgeConnected { from_node, from_pin, to_node, to_pin }
//!         })
//!         .on_move(|node_id, position| Message::NodeMoved { node_id, position });
//!
//!     // Add nodes
//!     ng.push_node(Point::new(100.0, 100.0), text("Node A"));
//!     ng.push_node(Point::new(300.0, 100.0), text("Node B"));
//!
//!     // Add edges using type-safe PinReference
//!     for (from, to) in edges {
//!         ng.push_edge(*from, *to);
//!     }
//!
//!     ng.into()
//! }
//! ```
//!
//! ## Core Types
//!
//! ### [`PinReference`]
//! Type-safe identifier for pin connections:
//! ```rust
//! use iced_nodegraph::PinReference;
//!
//! let pin = PinReference::new(0, 1); // node 0, pin 1
//! assert_eq!(pin.node_id, 0);
//! assert_eq!(pin.pin_id, 1);
//! ```
//!
//! ### [`NodeGraphEvent`]
//! Unified event enum for all graph interactions:
//! - `EdgeConnected` / `EdgeDisconnected` - Connection changes
//! - `NodeMoved` / `GroupMoved` - Position changes
//! - `SelectionChanged` - Selection updates
//! - `CloneRequested` / `DeleteRequested` - Edit operations
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
//! Full WGPU with custom shaders for high-performance rendering.
//!
//! ### WebAssembly (Browser)
//! WebGPU acceleration with fallback to WebGL where needed.
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
//! See [`camera`](node_grapgh::camera) module for implementation details and comprehensive test coverage.
//!
//! ### Custom Rendering
//!
//! Uses WGPU shaders for high-performance rendering:
//! - Background/Foreground layers for proper rendering order
//! - GPU-accelerated with custom vertex/fragment shaders
//! - Cross-platform support (native WGPU + WebGPU)
//!
//! ## Interaction
//!
//! | Action | Input |
//! |--------|-------|
//! | Pan canvas | Middle mouse drag |
//! | Zoom | Mouse wheel (maintains cursor position) |
//! | Connect pins | Left-click source pin, drag to target |
//! | Re-route edge | Click on edge endpoint to unplug |
//! | Move node | Left-click and drag node |
//! | Box select | Left-click on empty space, drag |
//! | Clone selection | Ctrl+D |
//! | Delete selection | Delete key |
//! | Add to selection | Shift+click |
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

pub use content::{
    NodeContentStyle, get_text_color, is_theme_dark, node_label, node_separator, node_title_bar,
    simple_node,
};
pub use node_grapgh::{
    DragInfo, NodeGraph, NodeGraphEvent, RemoteDrag, RemoteUserState, camera::Camera2D,
    widget::node_graph,
};
pub use node_pin::{NodePin, PinDirection, PinReference, PinSide, node_pin};
pub use style::{
    DashPattern, EdgeAnimation, EdgeStyle, EdgeType, GraphStyle, NodeStyle, SelectionStyle,
    is_dark_theme, relative_luminance,
};

pub mod content;
mod node;
mod node_grapgh;
mod node_pin;
pub mod style;
