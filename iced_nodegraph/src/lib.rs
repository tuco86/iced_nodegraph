// The `on_*` callback fields are `Option<Box<dyn Fn(..) -> Message>>`. Naming each
// one would trade a legible type for a single-use alias, so the lint is off here
// rather than worked around.
#![allow(clippy::type_complexity)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/logo/logo.svg",
    html_favicon_url = "https://raw.githubusercontent.com/tuco86/iced_nodegraph/main/assets/logo/logo.svg"
)]

//! # iced_nodegraph
//!
//! A node graph editor widget for the [iced](https://github.com/iced-rs/iced)
//! GUI framework. Nodes are ordinary iced widgets placed on an infinite zoom/pan
//! canvas and wired together through typed pins; everything on the canvas -
//! node bodies, edges, pins, shadows, background - is drawn by one WGPU pipeline
//! as signed distance fields, so it stays sharp at every zoom level.
//!
//! <figure class="demo-embed compact" data-scene="hello_world">
//!   <div class="demo-frame">
//!     <a href="https://tuco86.github.io/iced_nodegraph/demo_hello_world/index.html">
//!       <img src="https://tuco86.github.io/iced_nodegraph/gallery/hello_world.png" alt="The hello_world demo: an email workflow graph with four connected nodes">
//!     </a>
//!   </div>
//!   <figcaption>Runs live when scrolled into view (WebGPU, Chrome recommended); a still image otherwise. Click the canvas for keyboard input.</figcaption>
//! </figure>
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use iced_nodegraph::{PinRef, edge, node, node_graph};
//! use iced::{Element, Theme, Point, Vector};
//! use iced::widget::text;
//! use iced_wgpu::Renderer;
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     EdgeConnected { from: PinRef, to: PinRef },
//!     NodesMoved { delta: Vector, node_ids: Vec<usize> },
//! }
//!
//! fn view(edges: &[(PinRef, PinRef)]) -> Element<'_, Message, Theme, Renderer> {
//!     node_graph()
//!         .on_connect(|from, to| Message::EdgeConnected { from, to })
//!         .on_move(|delta, node_ids| Message::NodesMoved { delta, node_ids })
//!         .push_node(node(0, Point::new(100.0, 100.0), text("Node A")))
//!         .push_node(node(1, Point::new(300.0, 100.0), text("Node B")))
//!         .edges(edges.iter().map(|(from, to)| edge((), *from, *to)))
//!         .into()
//! }
//! ```
//!
//! ## Ids
//!
//! Nodes, pins, edges and anchors carry your own id types, and a pin can carry
//! a payload. Those five types are named once, on a marker implementing
//! [`Ids`]; [`Indexed`] (`usize` ids, no edge id, no payload) is the default
//! and what [`node_graph`] builds. Any type that is
//! `Clone + Eq + Hash + Debug + Send + Sync + 'static` is an id, so a newtype,
//! an enum or a `uuid::Uuid` needs no impl.
//!
//! ```rust,no_run
//! use iced_nodegraph::{Ids, NodeGraph, PinRef, node};
//! use iced::{Element, Point};
//! use iced::widget::text;
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! struct AppIds;
//!
//! impl Ids for AppIds {
//!     type NodeId = u64;
//!     type PinId = &'static str;
//!     type EdgeId = u64;
//!     type AnchorId = usize;
//!     type Payload = ();
//! }
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Connected(PinRef<AppIds>, PinRef<AppIds>),
//! }
//!
//! fn view() -> Element<'static, Message> {
//!     NodeGraph::<AppIds, _, _, _>::new()
//!         .on_connect(Message::Connected)
//!         .push_node(node(7, Point::ORIGIN, text("seven")))
//!         .into()
//! }
//! ```
//!
//! The marker is the one type the compiler cannot infer, so it is named on the
//! graph and in the messages that carry a [`PinRef`]; every builder and
//! callback infers it from there.
//!
//! ## What the host owns
//!
//! The widget is stateless between frames and never mutates your data model. It
//! renders the nodes and edges you pass in and reports intent through callbacks;
//! your `update` applies the change and the next `view` reflects it. Which makes
//! these yours to uphold:
//!
//! - **Unique node ids.** Lookups resolve by id, so a duplicate push is ignored
//!   (first wins) and debug builds assert on it. Prefer a stable id from your
//!   data - a database key, `uuid::Uuid`, a typed newtype - over a hand-managed
//!   counter.
//! - **Edge dedupe.** [`on_connect`](NodeGraph::on_connect) fires on every snap
//!   during a drag, not on release, so one drag can report several connections.
//!   The default [`can_connect`](NodeGraph::can_connect) already rejects a second
//!   edge into an occupied input; for replace-on-drop instead, drop that rule
//!   (see [`connection`]) and remove the prior edge whose input matches - `to` is
//!   always the input pin.
//! - **Applying moves, deletes and clones.** `on_move` / `on_delete` /
//!   `on_clone` report intent only.
//! - **Applying selection.** Optional: the widget keeps a working selection, so
//!   clicks and the selection box work on their own.
//!   [`on_select`](NodeGraph::on_select) reports it; to own it, mark the matching
//!   nodes with [`Node::selected`] on the next `view` and your value takes over
//!   whenever it changes. Selection is a node property, so there is no ordering
//!   to get right. The camera is the same story through
//!   [`on_camera`](NodeGraph::on_camera) and [`camera`](NodeGraph::camera).
//!
//! ## Core types
//!
//! - [`PinRef`] addresses a pin as `(node_id, pin_id)` over your [`Ids`]. It is
//!   the endpoint type in every connection callback.
//! - [`PinEnd`] is the richer endpoint view (direction, occupancy, payload)
//!   handed to [`can_connect`](NodeGraph::can_connect); [`PinInfo`] is the
//!   per-pin view handed to the style closures.
//! - The camera is not a type you hold: push pan and zoom in through
//!   [`NodeGraph::camera`] and read the user's back out through
//!   [`on_camera`](NodeGraph::on_camera), both as a plain `(Point, f32)`. To
//!   frame content programmatically, give the graph an
//!   [`id`](NodeGraph::id) and run the [`focus`] task, as with
//!   `text_input::focus` or `scrollable::scroll_to`.
//! - [`Keymap`] holds the rebindable key and pointer bindings, with
//!   platform-appropriate defaults.
//! - [`GraphInfo`] carries per-frame diagnostics to
//!   [`on_info`](NodeGraph::on_info): element counts (total / in view / culled),
//!   CPU op timings, and the SDF pipeline's GPU work and memory counters.
//!   Delivered one frame behind, since it is measured during `draw`.
//!
//! ## Styling
//!
//! Node, edge and pin styles are flat concrete structs. Override individual
//! fields with struct-update over a theme-derived default, inside a `.style()`
//! closure that also receives the element's status:
//!
//! ```rust,no_run
//! use iced::{widget::text, Color, Point};
//! use iced_nodegraph::{ColorQuad, Indexed, Node, NodeStyle, default_node_style, node};
//!
//! # #[derive(Debug, Clone)]
//! # enum Message {}
//! # let (pos, body) = (Point::ORIGIN, text("body"));
//! let n: Node<'_, Indexed, Message, iced::Theme, iced::Renderer> = node(0, pos, body).style(|theme, status| NodeStyle {
//!     fill_color: ColorQuad::solid(Color::from_rgb(0.2, 0.3, 0.5)),
//!     ..default_node_style(theme, status)
//! });
//! ```
//!
//! The presets have the same shape as the defaults, so they drop straight into
//! `.style(..)`: [`NodeStyle::input`], [`NodeStyle::process`],
//! [`NodeStyle::output`], [`NodeStyle::comment`]; [`EdgeStyle::error`],
//! [`EdgeStyle::disabled`], [`EdgeStyle::highlighted`], [`EdgeStyle::data_flow`],
//! [`EdgeStyle::debug`]. All derive from the theme's palette, like iced's own
//! `button::success`.
//!
//! [`Pattern`] (re-exported from `iced_nodegraph_sdf`) controls every stroke:
//! `Pattern::solid(width)`, `Pattern::dashed(width, dash, gap)`,
//! `Pattern::dotted(spacing, radius)`, plus `.flow(speed)` to animate it along
//! the stroke. An animated pattern self-drives redraws - no host frame loop
//! needed.
//!
//! The style closure intentionally does not receive the node id: your `view` loop
//! already has it, along with any per-node status. Derive the status there and
//! capture it:
//!
//! ```rust,no_run
//! use iced::{widget::text, Point};
//! use iced_nodegraph::{NodeStyle, Pattern, default_node_style, node, node_graph};
//!
//! # #[derive(Debug, Clone)]
//! # enum Message {}
//! # struct MyNode { id: usize, pos: Point }
//! # let nodes = [MyNode { id: 0, pos: Point::ORIGIN }];
//! # let is_working = |_: usize| true;
//! let ng = node_graph::<Message, iced::Theme, iced::Renderer>().nodes(nodes.iter().map(|n| {
//!     let working = is_working(n.id);
//!     node(n.id, n.pos, text("body")).style(move |theme, status| {
//!         let base = default_node_style(theme, status);
//!         if working {
//!             NodeStyle { border_pattern: Pattern::dashed(2.0, 6.0, 4.0).flow(40.0), ..base }
//!         } else {
//!             base
//!         }
//!     })
//! }));
//! ```
//!
//! The chrome the widget draws itself follows the same closure-plus-default
//! shape, one type per thing: [`GraphStyle`] (canvas background and tiling) via
//! [`graph_style`](NodeGraph::graph_style), [`SelectionBoxStyle`] via
//! [`selection_box_style`](NodeGraph::selection_box_style), [`CuttingToolStyle`]
//! via [`cutting_tool_style`](NodeGraph::cutting_tool_style), and
//! [`MinimapStyle`] via [`minimap_style`](NodeGraph::minimap_style) for the
//! overview [`minimap`](NodeGraph::minimap) puts in a corner. A selected node's
//! look is not chrome - it comes from the node's own closure through
//! [`NodeStatus`].
//!
//! <figure class="demo-embed compact" data-scene="styling">
//!   <div class="demo-frame">
//!     <a href="https://tuco86.github.io/iced_nodegraph/demo_styling/index.html">
//!       <img src="https://tuco86.github.io/iced_nodegraph/gallery/styling.png" alt="The styling demo: four preset-styled nodes with routing anchors and the style control panel">
//!     </a>
//!   </div>
//!   <figcaption>Runs live when scrolled into view (WebGPU, Chrome recommended); a still image otherwise. Click the canvas for keyboard input.</figcaption>
//! </figure>
//!
//! ## Interaction
//!
//! Connections behave like physical plugs: a dragged edge *snaps* to a compatible
//! pin and [`on_connect`](NodeGraph::on_connect) fires immediately; moving away
//! unsnaps and fires [`on_disconnect`](NodeGraph::on_disconnect). Releasing while
//! snapped keeps the connection, releasing while loose discards the drag. Treat
//! these callbacks as live state, not a commit.
//!
//! [`NodeGraph::snap_grid`] puts a dragged node's origin on a world-unit grid.
//! The preview and the delta [`on_move`](NodeGraph::on_move) reports are the
//! same number, and holding [`Keymap::snap_override`] (Alt by default) suspends
//! the snap for as long as it is held. A drag carrying several nodes shares one
//! delta computed on the grabbed node, so a group keeps its internal layout.
//!
//! A node built with [`Node::frame`] is a backdrop: it renders behind every
//! non-frame node, answers a press only where none of them covers the point,
//! and carries the nodes fully inside its bounds along with it. Membership is
//! resolved from the live layout at press time, so a node dropped into a frame
//! is carried by the next drag and the host registers nothing.
//!
//! Rebindable bindings and their platform defaults are documented on [`Keymap`];
//! the full control scheme including mouse and touch gestures is in the
//! [repository README](https://github.com/tuco86/iced_nodegraph#controls).
//!
//! <figure class="demo-embed compact" data-scene="interaction">
//!   <div class="demo-frame">
//!     <a href="https://tuco86.github.io/iced_nodegraph/demo_interaction/index.html">
//!       <img src="https://tuco86.github.io/iced_nodegraph/gallery/interaction.png" alt="The interaction demo: typed pins with valid and rejected connections">
//!     </a>
//!   </div>
//!   <figcaption>Runs live when scrolled into view (WebGPU, Chrome recommended); a still image otherwise. Click the canvas for keyboard input.</figcaption>
//! </figure>
//!
//! ## Coordinates
//!
//! Screen space (pixels from input and the viewport) and world space (the
//! infinite canvas) are distinct [`euclid`](https://docs.rs/euclid) types, so
//! mixing them is a compile error:
//!
//! - **Screen -> world**: `world = screen / zoom - position`
//! - **World -> screen**: `screen = (world + position) * zoom`
//! - **Zoom at cursor**: `new_pos = old_pos + cursor_screen * (1/new_zoom - 1/old_zoom)`
//!
//! The derivations, and the screen/world type discipline they rest on, are in
//! `node_graph/camera.rs`.
//!
//! ## Platform support
//!
//! Native Windows, macOS and Linux via WGPU, and WebAssembly on WebGPU-capable
//! browsers. There is no WebGL and no tiny-skia fallback, so Chrome/Chromium is
//! recommended on the web.
//!
//! Every demo runs in the browser at <https://tuco86.github.io/iced_nodegraph/>.
//!
//! <link rel="stylesheet" href="../gallery/pkg/demo.css"><script type="module" src="../gallery/pkg/demo-loader.js"></script>
pub use connection::{default_can_connect, direction_ok, input_not_occupied, not_same_node};
pub use content::{EdgeRadii, node_footer, node_header};
pub use ids::{Id, Ids, Indexed};
pub use node_graph::{
    Anchor, Corner, Counts, DragInfo, Edge, GraphInfo, Minimap, Node, NodeGraph, OpTiming, PinRef,
    anchor, edge,
    focus::{Easing, FocusAnimation, FocusOptions, FocusTarget, focus, focus_operation},
    input::{ComboKey, KeyAction, KeyCombo, Keymap},
    node,
    widget::node_graph,
};
pub use node_pin::{NodePin, PinDirection, PinEnd, PinInfo, PinSide, node_pin};
pub use style::{
    // Anchor status and style (concrete; override via struct-update over the default)
    AnchorStatus,
    AnchorStyle,
    AnchorStyleFn,
    // The theme-side styling contract and the closure classes `iced::Theme` uses
    Catalog,
    // Unified color type for style fields
    ColorQuad,
    CuttingToolStyle,
    CuttingToolStyleFn,
    DragEdgeStyleFn,
    EdgeCurve,
    // Status enums for widget-side styling
    EdgeStatus,
    EdgeStyle,
    EdgeStyleFn,
    GraphStyle,
    GraphStyleFn,
    MinimapStyle,
    MinimapStyleFn,
    NodeStatus,
    // Node/edge/pin style types (concrete; override via struct-update over defaults)
    NodeStyle,
    NodeStyleFn,
    PinShape,
    PinStatus,
    PinStyle,
    PinStyleFn,
    SelectionBoxStyle,
    SelectionBoxStyleFn,
    // Tiling background (grid/dots/...) for GraphStyle
    TilingBackground,
    TilingKind,
    default_anchor_style,
    default_cutting_tool_style,
    default_edge_style,
    default_graph_style,
    default_minimap_style,
    default_node_style,
    default_pin_style,
    // Built-in status-driven default styles
    default_selection_box_style,
};

// Re-export iced_nodegraph_sdf types downstream crates meet through the widget
pub use iced_nodegraph_sdf::Pattern;
pub use iced_nodegraph_sdf::SdfStats;
pub use iced_nodegraph_sdf::pattern::PatternType as SdfPatternType;

pub mod connection;
pub mod content;
pub mod ids;
mod node_graph;
mod node_pin;
pub mod prelude;
pub mod style;

// Re-exported so downstream crates can name the exact iced types this widget's
// API is built from without risking a version mismatch. The `iced` umbrella
// crate is not re-exported: it is not a dependency of this crate (see Cargo.toml).
pub use iced_wgpu;
pub use iced_widget;
