//! Node graph widget and core types.
//!
//! This module provides the main [`NodeGraph`] widget for building interactive
//! node-based editors. It handles rendering, user interaction, and event dispatch.
//!
//! ## Quick Start
//!
//! ```ignore
//! use iced_nodegraph::{node_graph, PinRef};
//!
//! let mut ng = node_graph()
//!     .on_connect(|from, to| Message::Connected { from, to })
//!     .on_move(|delta, node_ids| Message::NodesMoved { delta, node_ids });
//!
//! ng.push_node(node(0, Point::new(100.0, 100.0), my_node_content));
//! ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));
//! ```
//!
//! ## Architecture
//!
//! - [`NodeGraph`] - The main widget container
//! - [`PinRef`] - Type-safe reference to a pin (generic over ID types)
//! - [`Camera2D`](camera::Camera2D) - Zoom and pan state management
//!
//! ## Event Handling
//!
//! Interaction is reported through individual callbacks: `on_connect()`,
//! `on_move()`, `on_select()`, `on_clone()`, `on_delete()`.
//! Move and select work without the app keeping its own model; the app receives
//! data on commit. Live drag callbacks (`on_drag_start/update/end`) additionally
//! report an in-progress drag so it can be observed as it happens.
//!
//! ## Styling
//!
//! Visual appearance is controlled per element through status-driven closures:
//! - [`Node::style`] - per-node body style; [`Node::pin_style`] - the node's pins
//! - [`Edge::style`] - per-edge style
//! - [`NodeGraph::graph_style`] / [`NodeGraph::dragging_edge_style`] - graph chrome

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;

use iced::{Length, Point, Size, Vector};

use crate::ids::{EdgeId, NodeId, PinId};
use crate::node_pin::{PinEnd, PinInfo};
use crate::style::{EdgeStatus, EdgeStyle, GraphStyle, NodeStatus, NodeStyle, PinStatus, PinStyle};

/// Per-node style callback: theme + status -> resolved style. Used by [`Node`].
pub(crate) type NodeStyleFn<'a, Theme> = Box<dyn Fn(&Theme, NodeStatus) -> NodeStyle + 'a>;
/// Per-edge style callback: theme + status + both endpoint pin infos (in draw
/// order: start = output side, end = input side) -> resolved style. Used by
/// [`Edge`].
pub(crate) type EdgeStyleFn<'a, P, UI, Theme> =
    Box<dyn Fn(&Theme, EdgeStatus, PinInfo<'_, P, UI>, PinInfo<'_, P, UI>) -> EdgeStyle + 'a>;
/// Per-node pin style callback: theme + this pin's info + the other endpoint's
/// info (the drag source during an edge drag, else `None`) + status -> resolved
/// pin style. The node styles all of its pins (pins carry no style of their
/// own). Used by [`Node::pin_style`].
pub(crate) type PinStyleFn<'a, P, UI, Theme> = Box<
    dyn Fn(&Theme, &PinInfo<'_, P, UI>, Option<&PinInfo<'_, P, UI>>, PinStatus) -> PinStyle + 'a,
>;
/// Drag-edge style callback: theme + the source pin's info -> resolved style. A
/// freshly dragged edge has no status. Used by [`NodeGraph::dragging_edge_style`].
pub(crate) type DragEdgeStyleFn<'a, P, UI, Theme> =
    Box<dyn Fn(&Theme, PinInfo<'_, P, UI>) -> EdgeStyle + 'a>;

/// A node to push onto the graph: id, position, content element, an optional
/// per-node style closure, and an optional closure styling all of its pins.
/// Build with [`node`] + [`Node::style`]/[`Node::pin_style`], then add via
/// [`NodeGraph::push_node`]. Looks like its own widget even though the body and
/// pins are drawn by the graph.
pub struct Node<'a, N, P, UI, Message, Theme, Renderer> {
    id: N,
    position: Point,
    element: iced::Element<'a, Message, Theme, Renderer>,
    style_fn: Option<NodeStyleFn<'a, Theme>>,
    pin_style_fn: Option<PinStyleFn<'a, P, UI, Theme>>,
}

/// Creates a [`Node`] with default (theme) styling.
pub fn node<'a, N, P, UI, Message, Theme, Renderer>(
    id: N,
    position: Point,
    element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
) -> Node<'a, N, P, UI, Message, Theme, Renderer> {
    Node {
        id,
        position,
        element: element.into(),
        style_fn: None,
        pin_style_fn: None,
    }
}

impl<'a, N, P, UI, Message, Theme, Renderer> Node<'a, N, P, UI, Message, Theme, Renderer> {
    /// Sets the per-node style closure: receives the theme and the node's
    /// [`NodeStatus`], returns the resolved style. Layer over the built-in
    /// default:
    /// ```ignore
    /// node(0, pos, el).style(|theme, status| NodeStyle {
    ///     fill_color: Color::WHITE.into(),
    ///     ..default_node_style(theme, status)
    /// })
    /// ```
    pub fn style(mut self, f: impl Fn(&Theme, NodeStatus) -> NodeStyle + 'a) -> Self {
        self.style_fn = Some(Box::new(f));
        self
    }

    /// Sets the closure that styles all of this node's pins: receives the theme,
    /// this pin's [`PinInfo`] view (direction, user info, id), the other
    /// endpoint's info (the drag source during an edge drag, else `None`) and
    /// the pin's [`PinStatus`], returns the resolved pin style.
    /// ```ignore
    /// node(0, pos, el).pin_style(|theme, pin, other, status| PinStyle {
    ///     color: color_for(pin.info()).into(),
    ///     ..default_pin_style(theme, status)
    /// })
    /// ```
    pub fn pin_style(
        mut self,
        f: impl Fn(&Theme, &PinInfo<'_, P, UI>, Option<&PinInfo<'_, P, UI>>, PinStatus) -> PinStyle + 'a,
    ) -> Self {
        self.pin_style_fn = Some(Box::new(f));
        self
    }
}

/// An edge to push onto the graph: a user id, endpoint pin references, and an
/// optional per-edge status-driven style closure. Build with [`edge`] +
/// [`Edge::style`], then add via [`NodeGraph::push_edge`]. The id is the user's
/// own (e.g. a DB key); it travels with the edge, symmetric to [`node`].
pub struct Edge<'a, N, P, E, UI, Theme> {
    id: E,
    from: PinRef<N, P>,
    to: PinRef<N, P>,
    style_fn: Option<EdgeStyleFn<'a, P, UI, Theme>>,
}

/// Creates an [`Edge`] with the given id and default (theme) styling.
///
/// The id comes last so the common no-id case reads cleanly via the `edge!`
/// macro: `edge!(from, to)` expands to `edge(from, to, ())`.
pub fn edge<'a, N, P, E, UI, Theme>(
    from: PinRef<N, P>,
    to: PinRef<N, P>,
    id: E,
) -> Edge<'a, N, P, E, UI, Theme> {
    Edge {
        id,
        from,
        to,
        style_fn: None,
    }
}

/// Builds an [`Edge`], defaulting the id to `()` when omitted.
///
/// ```ignore
/// edge!(PinRef::new(0, 0), PinRef::new(1, 0))       // id = ()
/// edge!(PinRef::new(0, 0), PinRef::new(1, 0), my_id) // id = my_id
/// ```
#[macro_export]
macro_rules! edge {
    ($from:expr, $to:expr $(,)?) => {
        $crate::edge($from, $to, ())
    };
    ($from:expr, $to:expr, $id:expr $(,)?) => {
        $crate::edge($from, $to, $id)
    };
}

impl<'a, N, P, E, UI, Theme> Edge<'a, N, P, E, UI, Theme> {
    /// Sets the per-edge style closure: theme, [`EdgeStatus`], and both endpoint
    /// [`PinInfo`]s in draw order (start = output side, end = input side) ->
    /// resolved style.
    pub fn style(
        mut self,
        f: impl Fn(&Theme, EdgeStatus, PinInfo<'_, P, UI>, PinInfo<'_, P, UI>) -> EdgeStyle + 'a,
    ) -> Self {
        self.style_fn = Some(Box::new(f));
        self
    }
}

pub mod camera;
pub(crate) mod euclid;
pub(crate) mod input;
pub(crate) mod state;
pub(crate) mod widget;

/// Shared per-frame rendering context for all primitives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderContext {
    pub camera_zoom: f32,
    pub camera_position: euclid::WorldPoint,
    /// Screen-space top-left of the widget within the window. SDF screen
    /// mapping must offset by this so layers align with Iced content when the
    /// graph is not at the window origin (e.g. below a toolbar).
    pub viewport_origin: euclid::ScreenVector,
    pub time: f32,
}

/// Counts for one element kind in a frame: how many exist, how many are in view,
/// and how many were culled (off-screen). `total == in_view + culled`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Total elements of this kind in the graph.
    pub total: usize,
    /// Elements whose screen bounds intersect the viewport.
    pub in_view: usize,
    /// Elements fully off-screen.
    pub culled: usize,
}

/// One timed slice of the per-frame CPU work, in the order it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpTiming {
    /// Stable label of the operation (e.g. `"geometry"`, `"edges"`).
    pub label: &'static str,
    /// CPU time the operation took this frame.
    pub duration: Duration,
}

/// Per-frame diagnostics for the graph, delivered to [`NodeGraph::on_info`].
///
/// `nodes`/`pins`/`edges` are [`Counts`]; `timings` is the CPU cost of each draw
/// operation in stack order (geometry, background, foreground, sdf prepare) and
/// sums to roughly the per-frame CPU time. `sdf_entries`/`sdf_tiles` are the
/// SDF pipeline counters. All timings are CPU-side; no GPU profiling is done.
///
/// Reported one frame behind: the values are measured during `draw` and
/// delivered on the next redraw, mirroring the controlled `on_pan` pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphInfo {
    /// Node counts (total / in view / culled).
    pub nodes: Counts,
    /// Pin counts across all nodes.
    pub pins: Counts,
    /// Edge counts.
    pub edges: Counts,
    /// Per-operation CPU timings, in stack order.
    pub timings: Vec<OpTiming>,
    /// SDF draw entries submitted this frame.
    pub sdf_entries: u32,
    /// SDF tiles the index covered this frame.
    pub sdf_tiles: u32,
}

/// Identifies what an in-progress drag is moving. Delivered to the
/// [`on_drag_start`](NodeGraph::on_drag_start) callback so the app can observe a
/// drag live (e.g. to broadcast it), alongside the commit-on-drop callbacks.
///
/// Ids are the user's own node/pin id types (`N`/`P`), matching the rest of the
/// callback API (e.g. [`PinRef`]); both default to `usize`.
#[derive(Debug, Clone)]
pub enum DragInfo<N = usize, P = usize> {
    /// Dragging a single node.
    Node { node_id: N },
    /// Dragging a group of selected nodes.
    Group { node_ids: Vec<N> },
    /// Dragging an edge from a pin (the source node and pin).
    Edge { from_node: N, from_pin: P },
    /// Box selection drag, anchored at this world-space corner.
    BoxSelect { start_x: f32, start_y: f32 },
}

/// Type-safe reference to a pin: a `node_id` paired with a `pin_id`, generic over
/// your id types.
///
/// The fields are public by design. `PinRef` is a transparent id pair with no
/// invariants to uphold: any node/pin id combination is structurally valid, and
/// whether two pins may actually connect is decided elsewhere (e.g. via
/// [`can_connect`](NodeGraph::can_connect)). Build it with a struct literal or
/// [`PinRef::new`], and match or destructure it freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinRef<N, P> {
    /// The node's user id.
    pub node_id: N,
    /// The pin's user id within its node.
    pub pin_id: P,
}

impl<N: Clone, P: Clone> PinRef<N, P> {
    /// Creates a pin reference from a node id and a pin id.
    pub fn new(node_id: N, pin_id: P) -> Self {
        Self { node_id, pin_id }
    }
}

/// Node graph widget with generic ID types.
///
/// # Type Parameters
/// - `N`: Node ID type (defaults to `usize`)
/// - `P`: Pin ID type (defaults to `usize`)
/// - `Message`: Application message type
/// - `Theme`: Iced theme type (defaults to `iced::Theme`)
/// - `Renderer`: Iced renderer type (defaults to `iced::Renderer`)
///
/// Users can provide their own ID types by implementing [`NodeId`], [`PinId`]
/// and [`EdgeId`].
#[allow(missing_debug_implementations)]
pub struct NodeGraph<
    'a,
    N = usize,
    P = usize,
    UI = (),
    Message = (),
    Theme = iced::Theme,
    Renderer = iced::Renderer,
    E = (),
> where
    N: NodeId,
    P: PinId,
    E: EdgeId,
{
    pub(super) size: Size<Length>,
    /// Nodes with position, element, and config overrides.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `default_node_style()` values at render time.
    pub(super) nodes: Vec<(
        N,
        Point,
        iced::Element<'a, Message, Theme, Renderer>,
        Option<NodeStyleFn<'a, Theme>>,
        Option<PinStyleFn<'a, P, UI, Theme>>,
    )>,
    /// Id -> index map backing `node_index`: O(1) lookups and deterministic
    /// duplicate detection in `push_node` (first push wins).
    node_lookup: HashMap<N, usize>,
    /// Edges with user-defined pin references and config overrides.
    /// Pin IDs are resolved to local indices at render time.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `default_edge_style()` values at render time.
    pub(super) edges: Vec<(
        E,
        PinRef<N, P>,
        PinRef<N, P>,
        Option<EdgeStyleFn<'a, P, UI, Theme>>,
    )>,
    graph_style: Option<Box<dyn Fn(&Theme) -> GraphStyle + 'a>>,
    on_connect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(Vector, Vec<N>) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_clone: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_delete: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    /// External selection using internal indices.
    /// Populated by `selection()` method which converts user IDs to indices.
    external_selection: Option<HashSet<usize>>,
    // Live drag callbacks: fire continuously during a drag (start/update/end),
    // in addition to the commit-on-drop on_move. They make live
    // observation of an in-progress drag possible (e.g. collaborative broadcast),
    // which is the app's concern, not the widget's.
    on_drag_start: Option<Box<dyn Fn(DragInfo<N, P>) -> Message + 'a>>,
    on_drag_update: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    /// Commit callback for pan/zoom: fires with the new camera (position, zoom)
    /// when the user finishes a pan drag or zooms. The host stores it and feeds
    /// it back via `view()`, mirroring `on_move` / `selection`.
    on_pan: Option<Box<dyn Fn(Point, f32) -> Message + 'a>>,
    /// Per-frame diagnostics callback (element counts + CPU op timings).
    on_info: Option<Box<dyn Fn(GraphInfo) -> Message + 'a>>,
    /// Style callback for box selection overlay.
    /// Returns (fill_color, border_color).
    pub(super) box_select_style_fn: Option<Box<dyn Fn(&Theme) -> (iced::Color, iced::Color) + 'a>>,
    /// Style callback for edge cutting tool overlay.
    /// Returns the line color.
    pub(super) cutting_tool_style_fn: Option<Box<dyn Fn(&Theme) -> iced::Color + 'a>>,
    /// Style for the edge being dragged (theme -> resolved style). The graph
    /// injects the source pin's color for inheriting (TRANSPARENT) stroke ends.
    pub(super) dragging_edge_style_fn: Option<DragEdgeStyleFn<'a, P, UI, Theme>>,
    /// Host-controlled camera (world position + zoom). The widget syncs its
    /// internal camera to this whenever the host changes it, while still running
    /// pan/zoom interaction internally and committing via `on_pan`. Mirrors the
    /// `selection()` / `on_select` controlled pattern.
    pub(super) view: Option<(Point, f32)>,
    /// Custom validation callback for pin connection compatibility.
    /// When set, it is authoritative in `compute_valid_targets` (the built-in
    /// direction check only applies as the default when this is unset).
    pub(super) can_connect:
        Option<Box<dyn Fn(PinEnd<'_, N, P, UI>, PinEnd<'_, N, P, UI>) -> bool + 'a>>,
    /// Key and pointer bindings; platform defaults unless overridden via
    /// [`keymap`](Self::keymap).
    pub(super) keymap: input::Keymap,
}

impl<N, P, E, UI, Message, Theme, Renderer> Default
    for NodeGraph<'_, N, P, UI, Message, Theme, Renderer, E>
where
    N: NodeId,
    P: PinId,
    E: EdgeId,
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            nodes: Vec::new(),
            node_lookup: HashMap::new(),
            edges: Vec::new(),
            graph_style: None,
            on_connect: None,
            on_disconnect: None,
            on_move: None,
            on_select: None,
            on_clone: None,
            on_delete: None,
            external_selection: None,
            on_drag_start: None,
            on_drag_update: None,
            on_drag_end: None,
            on_pan: None,
            on_info: None,
            box_select_style_fn: None,
            cutting_tool_style_fn: None,
            dragging_edge_style_fn: None,
            view: None,
            can_connect: None,
            keymap: input::Keymap::default(),
        }
    }
}

impl<'a, N, P, E, UI, Message, Theme, Renderer> NodeGraph<'a, N, P, UI, Message, Theme, Renderer, E>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    /// Sets the host-controlled camera (world position + zoom).
    ///
    /// The widget snaps its camera to this whenever the host changes the value,
    /// while still running pan/zoom interaction internally and committing through
    /// [`on_pan`](Self::on_pan). This is the controlled-component counterpart to
    /// `on_pan`, exactly like `selection()` is to `on_select`: feed back what
    /// `on_pan` reports and the view stays in sync; push a new value (e.g. a reset
    /// to origin) and the view snaps there.
    pub fn view(mut self, position: Point, zoom: f32) -> Self {
        self.view = Some((position, zoom));
        self
    }

    /// Adds a node with the given ID and default styling.
    ///
    /// The node will use theme defaults from `default_node_style()`.
    ///
    /// Node IDs must be unique: a duplicate push is ignored (the first node
    /// with the id wins), and debug builds assert on it. Prefer a stable id
    /// from your data (a DB key, `uuid::Uuid`, a typed newtype) over a
    /// hand-managed counter.
    pub fn push_node(&mut self, node: Node<'a, N, P, UI, Message, Theme, Renderer>) {
        match self.node_lookup.entry(node.id.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                debug_assert!(
                    false,
                    "duplicate node id {:?}: node ids must be unique; \
                     the duplicate push is ignored (first wins)",
                    node.id,
                );
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(self.nodes.len());
                self.nodes.push((
                    node.id,
                    node.position,
                    node.element,
                    node.style_fn,
                    node.pin_style_fn,
                ));
            }
        }
    }

    /// Adds an edge to the graph.
    ///
    /// Pin IDs are resolved to local indices at render time; the widget
    /// normalizes orientation so the output pin is the edge start (output ->
    /// input).
    pub fn push_edge(&mut self, edge: Edge<'a, N, P, E, UI, Theme>) {
        self.edges
            .push((edge.id, edge.from, edge.to, edge.style_fn));
    }

    /// The user node id stored at an internal index.
    pub(super) fn node_id_at(&self, index: usize) -> Option<&N> {
        self.nodes.get(index).map(|(id, ..)| id)
    }

    /// Clones the user node id stored at an internal index.
    pub(super) fn index_to_node_id(&self, index: usize) -> Option<N> {
        self.node_id_at(index).cloned()
    }

    /// The internal index of a node by its user id. Linear scan: the node Vec is
    /// the single source of truth, the index is a transient render-time detail.
    pub(super) fn node_index(&self, id: &N) -> Option<usize> {
        self.node_lookup.get(id).copied()
    }

    /// Sets the graph chrome style (background, etc.) as a theme-derived closure.
    ///
    /// Mirrors the other style setters (`box_select_style`, `dragging_edge_style`,
    /// `cutting_tool_style`) and the per-node/edge/pin `.style()` closures: every
    /// style entry point on the widget is a `Fn(&Theme) -> _`. For a static style,
    /// ignore the theme argument: `.graph_style(|_| GraphStyle { ..base })`.
    pub fn graph_style(mut self, f: impl Fn(&Theme) -> GraphStyle + 'a) -> Self {
        self.graph_style = Some(Box::new(f));
        self
    }

    /// Sets a style callback for the box selection overlay.
    ///
    /// The callback receives the theme and returns (fill_color, border_color).
    ///
    /// # Example
    /// ```ignore
    /// node_graph()
    ///     .box_select_style(|theme| {
    ///         (Color::from_rgba(0.3, 0.6, 1.0, 0.2), Color::from_rgb(0.3, 0.6, 1.0))
    ///     })
    /// ```
    pub fn box_select_style(
        mut self,
        f: impl Fn(&Theme) -> (iced::Color, iced::Color) + 'a,
    ) -> Self {
        self.box_select_style_fn = Some(Box::new(f));
        self
    }

    /// Sets the style of the edge being dragged (before it connects). Receives
    /// the theme and the source pin, so the closure can derive the stroke from
    /// the pin's info (e.g. a port-typed color) for both ends of the loose edge.
    pub fn dragging_edge_style(
        mut self,
        f: impl Fn(&Theme, PinInfo<'_, P, UI>) -> EdgeStyle + 'a,
    ) -> Self {
        self.dragging_edge_style_fn = Some(Box::new(f));
        self
    }

    /// Sets a style callback for the edge cutting tool overlay.
    ///
    /// The callback receives the theme and returns the line color.
    ///
    /// # Example
    /// ```ignore
    /// node_graph()
    ///     .cutting_tool_style(|theme| Color::from_rgb(1.0, 0.3, 0.3))
    /// ```
    pub fn cutting_tool_style(mut self, f: impl Fn(&Theme) -> iced::Color + 'a) -> Self {
        self.cutting_tool_style_fn = Some(Box::new(f));
        self
    }

    /// Sets a validation callback for pin connection compatibility.
    ///
    /// When set, this callback is authoritative: it receives both endpoints as
    /// [`PinEnd`] views (node id, pin id, direction, occupancy, user info) and
    /// returns `true` if they can connect.
    ///
    /// # Warning
    ///
    /// Setting this REPLACES the built-in checks; they do not auto-compose, and
    /// there is no opt-out flag. A closure that only inspects payloads would re-allow
    /// same-direction, self-node, and double-booked-input connections. Re-include the
    /// built-in rules with
    /// [`default_can_connect`](crate::connection::default_can_connect):
    ///
    /// ```ignore
    /// use iced_nodegraph::connection::default_can_connect;
    /// ng.can_connect(|from, to| default_can_connect(from, to) && from.info() == to.info());
    /// ```
    ///
    /// Or pick individual predicates ([`direction_ok`](crate::connection::direction_ok),
    /// [`not_same_node`](crate::connection::not_same_node),
    /// [`input_not_occupied`](crate::connection::input_not_occupied)).
    ///
    /// When not set, the widget applies `default_can_connect` (direction, not-same-
    /// node, one-edge-per-input).
    pub fn can_connect(
        mut self,
        f: impl Fn(PinEnd<'_, N, P, UI>, PinEnd<'_, N, P, UI>) -> bool + 'a,
    ) -> Self {
        self.can_connect = Some(Box::new(f));
        self
    }

    /// Overrides the key and pointer bindings.
    ///
    /// The default [`Keymap`](crate::Keymap) is platform-aware (e.g. clone is
    /// `Alt+D` on the web because browsers reserve `Cmd/Ctrl+D`); pass a
    /// modified copy to rebind or disable individual actions:
    ///
    /// ```
    /// use iced_nodegraph::{Keymap, node_graph};
    /// use iced_wgpu::Renderer;
    ///
    /// let keymap = Keymap {
    ///     select_all: None, // disable Select All
    ///     ..Keymap::default()
    /// };
    /// let graph = node_graph::<(), iced::Theme, Renderer>().keymap(keymap);
    /// ```
    pub fn keymap(mut self, keymap: input::Keymap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Sets a callback for when an edge is connected between two pins.
    ///
    /// `from` is always the OUTPUT pin and `to` always the INPUT pin, whichever way
    /// the user dragged: the widget normalizes orientation to the rendered data
    /// flow. So `to` is the key when enforcing one edge per input (see the
    /// crate-level "What the host owns").
    ///
    /// Fires on SNAP during a drag, not on release - a single drag can emit several
    /// connect/disconnect pairs as the edge snaps and unsnaps. Treat it as live
    /// state, not a commit.
    ///
    /// Required to start an edge drag: without this callback, pressing a pin selects
    /// its node instead (a dropped edge could not be persisted anyway).
    pub fn on_connect(mut self, f: impl Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when an edge is disconnected between two pins.
    ///
    /// Like [`on_connect`](Self::on_connect), the pair is normalized output-first
    /// (`from` = output, `to` = input).
    pub fn on_disconnect(mut self, f: impl Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when one or more nodes are dragged to a new position.
    ///
    /// The callback receives the movement delta in world coordinates and the list
    /// of moved node IDs. Dragging a single node reports that one node; dragging a
    /// selection reports the whole group. In both cases the app applies the same
    /// delta to every listed node.
    ///
    /// Required for node dragging: node positions live in the host, so without this
    /// callback a drag has nowhere to land and the widget keeps nodes stationary
    /// (selection still works).
    pub fn on_move(mut self, f: impl Fn(Vector, Vec<N>) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the selection changes.
    ///
    /// The callback receives the list of currently selected node IDs.
    /// Fires on click-select, box-select, and Shift+click multi-select.
    ///
    /// The widget keeps its own selection regardless; to make the host the source
    /// of truth, feed the reported value back via [`selection`](Self::selection).
    pub fn on_select(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the user requests to clone selected nodes (Ctrl+D).
    ///
    /// The callback receives the list of node IDs to clone.
    /// The application is responsible for creating the actual clones.
    pub fn on_clone(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_clone = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the user requests to delete selected nodes (Delete key).
    ///
    /// The callback receives the list of node IDs to delete.
    /// The application is responsible for removing the nodes from its data model.
    pub fn on_delete(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_delete = Some(Box::new(f));
        self
    }

    /// Sets a callback for when a drag operation starts.
    /// Used for real-time collaboration to broadcast drag state to other users.
    pub fn on_drag_start(mut self, f: impl Fn(DragInfo<N, P>) -> Message + 'a) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }

    /// Sets a callback for drag position updates.
    ///
    /// Called frequently during a drag with the current cursor position in world
    /// coordinates as a [`Point`] (a semantic type, matching `on_move`'s
    /// `Vector`, rather than a bare `(f32, f32)` tuple).
    pub fn on_drag_update(mut self, f: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_drag_update = Some(Box::new(f));
        self
    }

    /// Sets a callback for when a drag operation ends.
    pub fn on_drag_end(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_drag_end = Some(Box::new(f));
        self
    }

    /// Sets the commit callback for pan/zoom.
    ///
    /// Fires with the new camera position and zoom when the user finishes a pan
    /// drag or zooms (zoom shifts position too, so both report together). Store
    /// the value and feed it back via [`view`](Self::view) to keep the controlled
    /// camera in sync.
    pub fn on_pan(mut self, f: impl Fn(Point, f32) -> Message + 'a) -> Self {
        self.on_pan = Some(Box::new(f));
        self
    }

    /// Sets the per-frame diagnostics callback.
    ///
    /// Fires once per redraw with a [`GraphInfo`]: element counts (total / in
    /// view / culled) and the CPU time of each draw operation, in stack order.
    /// Values are measured during `draw` and delivered on the next redraw (one
    /// frame behind), so a live readout should keep requesting redraws. CPU-side
    /// only; no GPU profiling.
    pub fn on_info(mut self, f: impl Fn(GraphInfo) -> Message + 'a) -> Self {
        self.on_info = Some(Box::new(f));
        self
    }

    /// Sets the host-controlled selection using user node IDs.
    ///
    /// The IDs are converted to internal indices; unknown IDs are ignored.
    ///
    /// Optional: the widget tracks selection internally and reports it through
    /// [`on_select`](Self::on_select), so an uncontrolled graph works without this.
    /// Feed it only when the host is the source of truth - to drive selection
    /// programmatically (select-all, clear, restore from a save). This is the
    /// controlled-component counterpart to `on_select`, exactly like
    /// [`view`](Self::view) is to `on_pan`.
    pub fn selection<'b>(mut self, selection: impl IntoIterator<Item = &'b N>) -> Self
    where
        N: 'b,
    {
        let indices: HashSet<usize> = selection
            .into_iter()
            .filter_map(|id| self.node_index(id))
            .collect();
        self.external_selection = Some(indices);
        self
    }

    /// Sets the width of the node graph widget.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.size.width = width.into();
        self
    }

    /// Sets the height of the node graph widget.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.size.height = height.into();
        self
    }

    pub(super) fn elements_iter(
        &self,
    ) -> impl Iterator<Item = (Point, &iced::Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter().map(|(_, p, e, _, _)| (*p, e))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point, &mut iced::Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter_mut().map(|(_, p, e, _, _)| (*p, e))
    }

    pub(super) fn on_connect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>> {
        self.on_connect.as_ref()
    }
    pub(super) fn on_disconnect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>> {
        self.on_disconnect.as_ref()
    }
    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(Vector, Vec<N>) -> Message + 'a>> {
        self.on_move.as_ref()
    }
    pub(super) fn on_select_handler(&self) -> Option<&Box<dyn Fn(Vec<N>) -> Message + 'a>> {
        self.on_select.as_ref()
    }
    pub(super) fn on_clone_handler(&self) -> Option<&Box<dyn Fn(Vec<N>) -> Message + 'a>> {
        self.on_clone.as_ref()
    }
    pub(super) fn on_delete_handler(&self) -> Option<&Box<dyn Fn(Vec<N>) -> Message + 'a>> {
        self.on_delete.as_ref()
    }
    pub(super) fn on_drag_start_handler(
        &self,
    ) -> Option<&Box<dyn Fn(DragInfo<N, P>) -> Message + 'a>> {
        self.on_drag_start.as_ref()
    }
    pub(super) fn on_drag_update_handler(&self) -> Option<&Box<dyn Fn(Point) -> Message + 'a>> {
        self.on_drag_update.as_ref()
    }
    pub(super) fn on_drag_end_handler(&self) -> Option<&Box<dyn Fn() -> Message + 'a>> {
        self.on_drag_end.as_ref()
    }
    pub(super) fn get_external_selection(&self) -> Option<&HashSet<usize>> {
        self.external_selection.as_ref()
    }

    pub(super) fn on_pan_handler(&self) -> Option<&Box<dyn Fn(Point, f32) -> Message + 'a>> {
        self.on_pan.as_ref()
    }
    pub(super) fn on_info_handler(&self) -> Option<&Box<dyn Fn(GraphInfo) -> Message + 'a>> {
        self.on_info.as_ref()
    }
    pub(super) fn view_value(&self) -> Option<(Point, f32)> {
        self.view
    }

    /// Translates a list of internal node indices to user IDs.
    /// Returns empty vec if any translation fails.
    pub(super) fn translate_node_ids(&self, indices: &[usize]) -> Vec<N> {
        indices
            .iter()
            .filter_map(|&idx| self.index_to_node_id(idx))
            .collect()
    }
}
