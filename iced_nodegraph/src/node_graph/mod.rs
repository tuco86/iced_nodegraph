//! The [`NodeGraph`] widget and the value types its API is built from.
//!
//! # Ownership
//!
//! The host owns the graph. `NodeGraph` is rebuilt every `view` from the host's
//! model and holds no graph state between frames; it reports intent through
//! callbacks and the host applies it. The only state that survives a frame is
//! interaction state (camera, drag, selection, z-order) in
//! [`state`](self::state), keyed by node index rather than node id.
//!
//! ```ignore
//! let mut ng = node_graph()
//!     .on_connect(|from, to| Message::Connected { from, to })
//!     .on_move(|delta, node_ids| Message::NodesMoved { delta, node_ids });
//!
//! ng.push_node(node(0, Point::new(100.0, 100.0), my_node_content));
//! ng.push_edge(edge(PinRef::new(0, 0), PinRef::new(1, 0)));
//! ```
//!
//! # Reporting
//!
//! There is no event enum: each interaction has its own `Fn -> Message` setter
//! (`on_connect`, `on_move`, `on_select`, `on_clone`, `on_delete`, `on_pan`,
//! `on_info`). Selection and camera are additionally *controllable*: feed the
//! reported value back through [`NodeGraph::selection`] / [`NodeGraph::view`] to
//! make the host the source of truth. `on_drag_start`/`on_drag_update`/
//! `on_drag_end` expose a drag while it happens, for hosts that mirror it
//! elsewhere.
//!
//! # Styling
//!
//! Every style entry point is a status-driven closure over a theme:
//! [`Node::style`] and [`Node::pin_style`] for a node and its pins,
//! [`Edge::style`] for an edge, [`NodeGraph::graph_style`] for the canvas and
//! selection chrome, and [`NodeGraph::dragging_edge_style`] for the loose edge
//! during a connect drag.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iced_widget::core::{Element, Length, Point, Size, Vector};

use crate::ids::{NodeId, PinId};
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
    pub(super) id: N,
    pub(super) position: Point,
    pub(super) element: Element<'a, Message, Theme, Renderer>,
    pub(super) style: Option<NodeStyleFn<'a, Theme>>,
    pub(super) pin_style: Option<PinStyleFn<'a, P, UI, Theme>>,
}

/// Creates a [`Node`] with default (theme) styling.
pub fn node<'a, N, P, UI, Message, Theme, Renderer>(
    id: N,
    position: Point,
    element: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Node<'a, N, P, UI, Message, Theme, Renderer> {
    Node {
        id,
        position,
        element: element.into(),
        style: None,
        pin_style: None,
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
        self.style = Some(Box::new(f));
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
        self.pin_style = Some(Box::new(f));
        self
    }
}

/// An edge to push onto the graph: the two pins it connects plus an optional
/// per-edge status-driven style closure. Build with [`edge`] + [`Edge::style`],
/// then add via [`NodeGraph::push_edge`].
///
/// An edge is identified by the pins it joins, so it carries no id of its own -
/// unlike a [`Node`], which needs one to be moved, cloned and deleted by id.
pub struct Edge<'a, N, P, UI, Theme> {
    pub(super) from: PinRef<N, P>,
    pub(super) to: PinRef<N, P>,
    pub(super) style: Option<EdgeStyleFn<'a, P, UI, Theme>>,
}

/// Creates an [`Edge`] with default (theme) styling.
///
/// ```rust
/// use iced_nodegraph::{Edge, PinRef, edge};
///
/// let e: Edge<'_, u32, u32, (), iced::Theme> =
///     edge(PinRef::new(0, 0), PinRef::new(1, 0));
/// ```
pub fn edge<'a, N, P, UI, Theme>(
    from: PinRef<N, P>,
    to: PinRef<N, P>,
) -> Edge<'a, N, P, UI, Theme> {
    Edge {
        from,
        to,
        style: None,
    }
}

impl<'a, N, P, UI, Theme> Edge<'a, N, P, UI, Theme> {
    /// Sets the per-edge style closure: theme, [`EdgeStatus`], and both endpoint
    /// [`PinInfo`]s in draw order (start = output side, end = input side) ->
    /// resolved style.
    pub fn style(
        mut self,
        f: impl Fn(&Theme, EdgeStatus, PinInfo<'_, P, UI>, PinInfo<'_, P, UI>) -> EdgeStyle + 'a,
    ) -> Self {
        self.style = Some(Box::new(f));
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
/// SDF pipeline counters. All timings are CPU-side; no GPU profiling is done -
/// the `sdf_*` byte and work counters below describe GPU resource use and work
/// volume, not GPU time.
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
    /// Bytes uploaded to the GPU this frame (`SdfStats::upload_bytes`).
    pub sdf_upload_bytes: u64,
    /// Live GPU buffer bytes owned by the SDF pipeline (`SdfStats::gpu_bytes`).
    pub sdf_gpu_bytes: u64,
    /// Spatial-index share of `sdf_gpu_bytes` (`SdfStats::index_bytes`).
    pub sdf_index_bytes: u64,
    /// SDF instance draws issued this frame (`SdfStats::sdf_draws`).
    pub sdf_draws: u32,
    /// Physical pixels covered by this frame's SDF draws (`SdfStats::shaded_px`).
    pub sdf_shaded_px: u64,
    /// Fragment-shader `eval_segment` calls this frame; 0 unless the index probe
    /// is enabled (`SdfStats::segment_evals`).
    pub sdf_segment_evals: u64,
    /// Highest per-fine-tile slot count against the 128 cap
    /// (`SdfStats::fine_slots_max`).
    pub sdf_fine_slots_max: u32,
    /// Fine index tiles that dropped a candidate segment at the slot cap
    /// (`SdfStats::fine_evicted_tiles`). Nonzero means some tiles rendered from
    /// an incomplete segment list.
    pub sdf_fine_evicted_tiles: u32,
    /// GPU-side index-build traffic this frame (`SdfStats::index_traffic_bytes`).
    pub sdf_index_traffic_bytes: u64,
    /// True when this frame reused the resident spatial index
    /// (`SdfStats::cull_skipped`).
    pub sdf_cull_skipped: bool,
}

/// Identifies what an in-progress drag is moving. Delivered to the
/// [`on_drag_start`](NodeGraph::on_drag_start) callback so the app can observe a
/// drag live (e.g. to broadcast it), alongside the commit-on-drop callbacks.
///
/// Ids are the user's own node/pin id types (`N`/`P`), matching the rest of the
/// callback API (e.g. [`PinRef`]); both default to `usize`.
#[derive(Debug, Clone, PartialEq)]
pub enum DragInfo<N = usize, P = usize> {
    /// Dragging a single node.
    Node { node_id: N },
    /// Dragging a group of selected nodes.
    Group { node_ids: Vec<N> },
    /// Dragging an edge from a pin (the source node and pin).
    Edge { from_node: N, from_pin: P },
    /// Box selection drag, anchored at this world-space corner.
    BoxSelect { start: Point },
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

/// Node graph widget: a frame-scoped collection of [`Node`]s and [`Edge`]s plus
/// the callbacks and styles that apply to them.
///
/// # Type Parameters
/// - `N`: node id type (defaults to `usize`)
/// - `P`: pin id type (defaults to `usize`)
/// - `UI`: per-pin user payload surfaced to `pin_style`/`can_connect`
///   (defaults to `()`)
/// - `Message`: application message type
/// - `Theme`: iced theme type
/// - `Renderer`: iced renderer type
///
/// Bring your own id types by implementing [`NodeId`] and [`PinId`].
#[allow(missing_debug_implementations)]
pub struct NodeGraph<
    'a,
    N = usize,
    P = usize,
    UI = (),
    Message = (),
    Theme = iced_widget::core::Theme,
    Renderer = iced_widget::renderer::Renderer,
> where
    N: NodeId,
    P: PinId,
{
    pub(super) size: Size<Length>,
    /// Nodes in push order, which is also their initial z-order.
    pub(super) nodes: Vec<Node<'a, N, P, UI, Message, Theme, Renderer>>,
    /// Id -> index map: O(1) `node_index` lookups and deterministic duplicate
    /// detection in `push_node` (first push wins).
    pub(super) node_lookup: HashMap<N, usize>,
    /// Edges in push order. Endpoint pin ids are resolved to positional pin
    /// indices at draw time, since only the laid-out widget tree knows them.
    pub(super) edges: Vec<Edge<'a, N, P, UI, Theme>>,
    pub(super) graph_style: Option<Box<dyn Fn(&Theme) -> GraphStyle + 'a>>,
    pub(super) on_connect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    pub(super) on_disconnect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    pub(super) on_move: Option<Box<dyn Fn(Vector, Vec<N>) -> Message + 'a>>,
    pub(super) on_select: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    pub(super) on_clone: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    pub(super) on_delete: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    /// Host-controlled selection, translated to node indices by
    /// [`selection`](Self::selection).
    pub(super) external_selection: Option<HashSet<usize>>,
    /// Live drag callbacks: fire continuously during a drag, alongside the
    /// commit-on-drop `on_move`. Observing a drag as it happens (to broadcast
    /// it, say) is the app's concern, so the widget only reports it.
    pub(super) on_drag_start: Option<Box<dyn Fn(DragInfo<N, P>) -> Message + 'a>>,
    pub(super) on_drag_update: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    pub(super) on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    /// Commit callback for pan/zoom, the counterpart to [`view`](Self::view).
    pub(super) on_pan: Option<Box<dyn Fn(Point, f32) -> Message + 'a>>,
    /// Per-frame diagnostics callback.
    pub(super) on_info: Option<Box<dyn Fn(GraphInfo) -> Message + 'a>>,
    /// Style for the edge being dragged (theme -> resolved style). The graph
    /// injects the source pin's color for inheriting (TRANSPARENT) stroke ends.
    pub(super) dragging_edge_style: Option<DragEdgeStyleFn<'a, P, UI, Theme>>,
    /// Host-controlled camera (world position + zoom). The widget syncs its
    /// internal camera to this whenever the host changes it, while still running
    /// pan/zoom interaction internally and committing via `on_pan`. Mirrors the
    /// `selection()` / `on_select` controlled pattern.
    pub(super) view: Option<(Point, f32)>,
    /// Connection validation. When set it is authoritative in
    /// `compute_valid_targets`; otherwise
    /// [`default_can_connect`](crate::connection::default_can_connect) applies.
    pub(super) can_connect:
        Option<Box<dyn Fn(PinEnd<'_, N, P, UI>, PinEnd<'_, N, P, UI>) -> bool + 'a>>,
    /// Key and pointer bindings; platform defaults unless overridden via
    /// [`keymap`](Self::keymap).
    pub(super) keymap: input::Keymap,
}

impl<N, P, UI, Message, Theme, Renderer> Default
    for NodeGraph<'_, N, P, UI, Message, Theme, Renderer>
where
    N: NodeId,
    P: PinId,
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
            dragging_edge_style: None,
            view: None,
            can_connect: None,
            keymap: input::Keymap::default(),
        }
    }
}

impl<'a, N, P, UI, Message, Theme, Renderer> NodeGraph<'a, N, P, UI, Message, Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
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

    /// Adds a node, styled by the theme unless the builder overrides it.
    ///
    /// Node ids must be unique: a duplicate push is ignored (the first node with
    /// the id wins) and debug builds assert on it. Prefer a stable id from your
    /// data (a DB key, `uuid::Uuid`, a typed newtype) over a hand-managed
    /// counter.
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
                self.nodes.push(node);
            }
        }
    }

    /// Adds an edge, styled by the theme unless the builder overrides it.
    ///
    /// The widget normalizes orientation when drawing and reporting, so the
    /// output pin is always the edge start (output -> input) regardless of the
    /// order given here.
    pub fn push_edge(&mut self, edge: Edge<'a, N, P, UI, Theme>) {
        self.edges.push(edge);
    }

    /// The user node id at a node index.
    pub(super) fn node_id_at(&self, index: usize) -> Option<&N> {
        self.nodes.get(index).map(|node| &node.id)
    }

    /// The node index of a user node id.
    pub(super) fn node_index(&self, id: &N) -> Option<usize> {
        self.node_lookup.get(id).copied()
    }

    /// The user node ids at the given node indices, skipping unknown indices.
    pub(super) fn node_ids_at(&self, indices: &[usize]) -> Vec<N> {
        indices
            .iter()
            .filter_map(|&index| self.node_id_at(index).cloned())
            .collect()
    }

    /// Sets the graph chrome style (canvas background, tiling, selection
    /// overlay) as a theme-derived closure.
    ///
    /// Every style entry point on the widget is a `Fn(&Theme) -> _`. For a
    /// static style, ignore the theme argument:
    /// `.graph_style(|_| GraphStyle { ..base })`.
    pub fn graph_style(mut self, f: impl Fn(&Theme) -> GraphStyle + 'a) -> Self {
        self.graph_style = Some(Box::new(f));
        self
    }

    /// Sets the style of the edge being dragged (before it connects). Receives
    /// the theme and the source pin, so the closure can derive the stroke from
    /// the pin's info (e.g. a port-typed color) for both ends of the loose edge.
    pub fn dragging_edge_style(
        mut self,
        f: impl Fn(&Theme, PinInfo<'_, P, UI>) -> EdgeStyle + 'a,
    ) -> Self {
        self.dragging_edge_style = Some(Box::new(f));
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

    /// Reports the start of a drag, naming what it moves.
    ///
    /// This and [`on_drag_update`](Self::on_drag_update) /
    /// [`on_drag_end`](Self::on_drag_end) bracket every drag exactly once, and
    /// fire in addition to the commit-on-release callbacks. They exist for hosts
    /// that mirror an in-progress drag somewhere else - a collaborative session,
    /// an inspector - and nothing is gated on them: omitting all three changes no
    /// behaviour.
    pub fn on_drag_start(mut self, f: impl Fn(DragInfo<N, P>) -> Message + 'a) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }

    /// Reports the cursor in world coordinates while a drag is in progress.
    ///
    /// Fires on every cursor move during the drag, so treat it as a stream.
    pub fn on_drag_update(mut self, f: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_drag_update = Some(Box::new(f));
        self
    }

    /// Reports that the drag ended, whether it committed or was discarded.
    ///
    /// Also fires when a drag is cancelled (a second touch contact, say), so it
    /// is the reliable place to clear whatever `on_drag_start` set up.
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

    /// The nodes' world positions paired with their content elements, in push
    /// order. Keeps the `Widget` impl's tree walks independent of `Node`'s shape.
    pub(super) fn elements_iter(
        &self,
    ) -> impl Iterator<Item = (Point, &Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter().map(|node| (node.position, &node.element))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point, &mut Element<'a, Message, Theme, Renderer>)> {
        self.nodes
            .iter_mut()
            .map(|node| (node.position, &mut node.element))
    }
}
