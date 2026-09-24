//! The [`NodeGraph`] widget and the value types its API is built from.
//!
//! # Ownership
//!
//! The host owns the graph. `NodeGraph` is rebuilt every `view` from the host's
//! model and holds no graph state between frames; it reports intent through
//! callbacks and the host applies it. The only state that survives a frame is
//! interaction state (camera, drag, z-order) in
//! [`state`](self::state), keyed by node index rather than node id.
//!
//! The crate-level Quick Start ([`crate`]) shows the whole `view` shape.
//!
//! # Reporting
//!
//! There is no event enum: each interaction has its own `Fn -> Message` setter
//! (`on_connect`, `on_move`, `on_resize`, `on_select`, `on_clone`, `on_delete`,
//! `on_camera`, `on_info`). Nothing is applied locally: selection comes back
//! per node through [`Node::selected`] and the camera through
//! [`NodeGraph::camera`], so the host is always the source of truth.
//! `on_drag_start`/`on_drag_update`/`on_drag_end` expose a drag while it
//! happens, for hosts that mirror it elsewhere.
//!
//! # Styling
//!
//! One shape throughout: a closure over the theme, with a `default_*_style`
//! function as its base. [`Node::style`] and [`Node::pin_style`] for a node and
//! its pins, [`Edge::style`] for an edge, and one entry point per piece of chrome
//! the widget draws itself - [`NodeGraph::graph_style`] (canvas),
//! [`NodeGraph::selection_box_style`], [`NodeGraph::cutting_tool_style`],
//! [`NodeGraph::minimap_style`] and [`NodeGraph::dragging_edge_style`].
//! Per-element closures additionally receive a status, so selection and cut
//! feedback are expressed in the style, not layered on afterwards.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iced_nodegraph_sdf::SdfStats;
use iced_widget::core::widget::Id as WidgetId;
use iced_widget::core::{Element, Length, Point, Size, Vector};
use web_time::Instant;

use self::focus::{FocusOptions, FocusTarget};
use crate::ids::{Ids, Indexed};
use crate::node_pin::{PinEnd, PinInfo};
use crate::style::{
    AnchorStatus, AnchorStyle, AnchorStyleFn, Catalog, CuttingToolStyle, CuttingToolStyleFn,
    DragEdgeStyleFn, EdgeStatus, EdgeStyle, EdgeStyleFn, GraphStyle, GraphStyleFn, MinimapStyle,
    MinimapStyleFn, NodeStatus, NodeStyle, NodeStyleFn, ParticleStyle, ParticleStyleFn, PinStatus,
    PinStyle, PinStyleFn, SelectionBoxStyle, SelectionBoxStyleFn,
};

/// Pin click detection threshold, in screen pixels: divided by zoom before
/// comparing against world-space distances, so the hit target stays constant on
/// screen.
///
/// Also the size the node body opens up for a pin - see
/// `style::defaults::PIN_CUTOUT_RADIUS`.
pub(crate) const PIN_CLICK_THRESHOLD: f32 = 8.0;

/// Edge-cut click distance, in screen pixels, scaled by 1/zoom at the
/// comparison site like [`PIN_CLICK_THRESHOLD`].
pub(crate) const EDGE_CUT_THRESHOLD: f32 = 10.0;

/// Radius of an anchor's orbit 0, in world units.
///
/// The interaction path has no theme, so it cannot resolve an anchor's own
/// `AnchorStyle`; it falls back to this until a frame has been drawn and the
/// resolved radii are available. `style::defaults` builds its default from the
/// same constants, so the fallback and the default agree.
///
/// Sized against the core it encircles rather than against the node: the core
/// is a 6 unit dot, so this clears its edge by 8 units - close enough to read
/// as belonging to the anchor, and clear of the square
/// [`ANCHOR_GRAB_THRESHOLD`] opens around the core (see
/// `the_core_grab_box_never_reaches_orbit_zero`).
pub(crate) const DEFAULT_ORBIT_OFFSET: f32 = 11.0;

/// Additional radius per orbit, in world units. See [`DEFAULT_ORBIT_OFFSET`].
///
/// Wide enough that two wraps read as separate strands at zoom 1 without the
/// outermost orbit of a busy anchor swallowing its surroundings.
pub(crate) const DEFAULT_ORBIT_SPACING: f32 = 6.0;

/// Side length of an anchor's core, in world units.
///
/// The interaction path has no theme, so it cannot resolve an anchor's own
/// `AnchorStyle`; it falls back to this until a frame has been drawn and the
/// resolved size is available, exactly like [`DEFAULT_ORBIT_OFFSET`].
/// `style::defaults` builds its default from this constant, so the fallback and
/// the default agree.
///
/// A 6 unit dot: `Shape::rounded_box` centres on the local origin, so the core
/// reaches 3 units either side of the anchor's position.
pub(crate) const DEFAULT_CORE_SIZE: f32 = 6.0;

/// Anchor-core grab distance, in screen pixels, scaled by 1/zoom at the
/// comparison site like [`PIN_CLICK_THRESHOLD`].
///
/// The hit target, not the drawn size, and clamped from BOTH sides at the
/// comparison site. It is floored at the core's own half-extent, or zooming in
/// would shrink the box inside the dot the user can see and a press within the
/// core would fall through to the canvas. It is capped so the square's corner
/// cannot reach orbit 0, or a press meant for the innermost wrap would grab the
/// core instead. The cap is applied last and wins: a host that styles a core
/// wider than `sqrt(2)` times its orbit 0 has painted a core overlapping its own
/// innermost ring, and the widget will not answer that by making the ring
/// unpressable.
pub(crate) const ANCHOR_GRAB_THRESHOLD: f32 = 7.0;

/// Cable grab distance for the mid-run and end zones, in screen pixels, scaled
/// by 1/zoom at the comparison sites like [`PIN_CLICK_THRESHOLD`].
///
/// The same distance as [`EDGE_CUT_THRESHOLD`], deliberately: both answer "is
/// the cursor on this cable", so a cable you can cut is a cable you can grab.
/// A tighter corridor here makes a press that misses by a pixel or two fall
/// through to the canvas gesture behind it, which reads as the cable refusing
/// to be picked up rather than as a miss.
pub(crate) const EDGE_GRAB_THRESHOLD: f32 = EDGE_CUT_THRESHOLD;

/// Arc length of a cable's end zone, in screen pixels scaled by 1/zoom:
/// pressing the cable within this distance of an endpoint grabs that END
/// (unplugging it) rather than the run in between.
pub(crate) const EDGE_END_GRAB_LENGTH: f32 = 24.0;

/// Side of a resizable node's bottom-right grip, in screen pixels, scaled by
/// 1/zoom at the use sites like [`PIN_CLICK_THRESHOLD`]. The draw path and the
/// hit test derive the zone from this one value, so what is painted is exactly
/// what can be grabbed.
pub(crate) const RESIZE_GRIP_SIDE: f32 = 12.0;

/// Floor for the content size a grip drag reports, in world pixels. A node
/// dragged to nothing would take its own grip with it, leaving the host no way
/// to grab it back.
pub(crate) const MIN_NODE_SIZE: Size = Size::new(32.0, 24.0);

/// A node to push onto the graph: id, position, content element, the class
/// the theme styles it by, and the class it styles all of its pins by.
/// Build with [`node`] + [`Node::style`]/[`Node::pin_style`], then add via
/// [`NodeGraph::push_node`]. Looks like its own widget even though the body and
/// pins are drawn by the graph.
pub struct Node<
    'a,
    I: Ids = Indexed,
    Message = (),
    Theme = iced_widget::core::Theme,
    Renderer = iced_widget::renderer::Renderer,
> where
    Theme: Catalog,
{
    pub(super) id: I::NodeId,
    pub(super) position: Point,
    pub(super) element: Element<'a, Message, Theme, Renderer>,
    pub(super) selected: bool,
    pub(super) resizable: bool,
    pub(super) frame: bool,
    pub(super) class: Theme::NodeClass<'a>,
    pub(super) pin_class: Theme::PinClass<'a, I>,
}

/// Creates a [`Node`] with the theme's default classes.
pub fn node<'a, I: Ids, Message, Theme: Catalog, Renderer>(
    id: I::NodeId,
    position: Point,
    element: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Node<'a, I, Message, Theme, Renderer> {
    Node {
        id,
        position,
        element: element.into(),
        selected: false,
        resizable: false,
        frame: false,
        class: Theme::default_node(),
        pin_class: Theme::default_pin(),
    }
}

impl<'a, I: Ids, Message, Theme: Catalog, Renderer> Node<'a, I, Message, Theme, Renderer> {
    /// Sets the per-node style closure: receives the theme and the node's
    /// [`NodeStatus`], returns the resolved style. Layer over the built-in
    /// default:
    /// ```rust,no_run
    /// use iced::{widget::text, Color, Point};
    /// use iced_nodegraph::{Indexed, Node, NodeStyle, default_node_style, node};
    ///
    /// # #[derive(Debug, Clone)]
    /// # enum Message {}
    /// # let (pos, el) = (Point::ORIGIN, text("body"));
    /// let n: Node<'_, Indexed, Message, iced::Theme, iced::Renderer> = node(0, pos, el)
    ///     .style(|theme, status| NodeStyle {
    ///         fill_color: Color::WHITE.into(),
    ///         ..default_node_style(theme, status)
    ///     });
    /// ```
    pub fn style(mut self, f: impl Fn(&Theme, NodeStatus) -> NodeStyle + 'a) -> Self
    where
        Theme::NodeClass<'a>: From<NodeStyleFn<'a, Theme>>,
    {
        self.class = (Box::new(f) as NodeStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles this node by.
    pub fn class(mut self, class: impl Into<Theme::NodeClass<'a>>) -> Self {
        self.class = class.into();
        self
    }

    /// Marks the node as selected.
    ///
    /// Selection is a property of the node, so the host sets it here from its own
    /// model - typically `.selected(self.selection.contains(&id))`.
    ///
    /// Optional. The widget keeps a working selection of its own driven by clicks
    /// and the selection box, so selection works without any host involvement.
    /// Marking nodes here *overrides* that whenever the marked set changes, which
    /// is what makes the host authoritative: drive selection programmatically,
    /// restore it from a save, or feed back what
    /// [`on_select`](NodeGraph::on_select) reported.
    ///
    /// A selected node draws with [`NodeStatus::Selected`] and sorts above its
    /// unselected siblings.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Gives the node a bottom-right grip the user can drag to resize it.
    ///
    /// The widget owns no node size - the content element's layout does - so a
    /// grip drag is a *report*, not an applied change: the new size travels
    /// through [`NodeGraph::on_resize`] and takes effect only once the host
    /// hands back a content element laid out that big. Same split as
    /// position and [`on_move`](NodeGraph::on_move).
    ///
    /// Both halves must be wired. Without `on_resize` the grip has nowhere to
    /// report, so it is neither drawn nor hit-tested and the corner keeps
    /// dragging the node like any other part of its body.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Turns the node into a frame: a backdrop that carries the nodes it
    /// encloses.
    ///
    /// A frame draws behind every non-frame node and loses any press a node
    /// above it could take, so its body is grabbable only where nothing covers
    /// it. Dragging it moves every node whose bounds lie fully inside the
    /// frame's at the moment of the press - containment is recomputed each
    /// time, so there is no membership to maintain on the host side.
    ///
    /// Frame contents ride along through [`on_move`](NodeGraph::on_move)
    /// alongside the frame itself, which the host applies like any other move.
    pub fn frame(mut self) -> Self {
        self.frame = true;
        self
    }

    /// Sets the closure that styles all of this node's pins: receives the theme,
    /// this pin's [`PinInfo`] view (direction, user info, id), the other
    /// endpoint's info (the drag source during an edge drag, else `None`) and
    /// the pin's [`PinStatus`], returns the resolved pin style.
    /// ```rust,no_run
    /// use iced::{widget::text, Color, Point};
    /// use iced_nodegraph::{Indexed, Node, PinStyle, default_pin_style, node};
    ///
    /// # #[derive(Debug, Clone)]
    /// # enum Message {}
    /// # let (pos, el) = (Point::ORIGIN, text("body"));
    /// # fn color_for(_: &()) -> Color { Color::WHITE }
    /// let n: Node<'_, Indexed, Message, iced::Theme, iced::Renderer> = node(0, pos, el)
    ///     .pin_style(|theme, pin, _other, status| PinStyle {
    ///         color: color_for(pin.info()).into(),
    ///         ..default_pin_style(theme, status)
    ///     });
    /// ```
    pub fn pin_style(
        mut self,
        f: impl Fn(&Theme, &PinInfo<'_, I>, Option<&PinInfo<'_, I>>, PinStatus) -> PinStyle + 'a,
    ) -> Self
    where
        Theme::PinClass<'a, I>: From<PinStyleFn<'a, Theme, I>>,
    {
        self.pin_class = (Box::new(f) as PinStyleFn<'a, Theme, I>).into();
        self
    }

    /// Sets the class the theme styles all of this node's pins by.
    pub fn pin_class(mut self, class: impl Into<Theme::PinClass<'a, I>>) -> Self {
        self.pin_class = class.into();
        self
    }
}

/// An edge to push onto the graph: a user id, endpoint pin references, the
/// anchors it wraps on the way, and the class the theme styles it by. Build
/// with [`edge`] + [`Edge::route`]/[`Edge::style`], then add via
/// [`NodeGraph::push_edge`]. The id is the user's own (e.g. a database key); it
/// travels with the edge, symmetric to [`node`].
pub struct Edge<'a, I: Ids = Indexed, Theme = iced_widget::core::Theme>
where
    Theme: Catalog,
{
    pub(super) id: I::EdgeId,
    pub(super) from: PinRef<I>,
    pub(super) to: PinRef<I>,
    /// The anchors this edge is routed through, as the host authored them. Not
    /// a drawing order: the widget derives the order, the wrap side and the
    /// orbit each frame, so this is a set the host may keep in any order.
    pub(super) route: Vec<I::AnchorId>,
    pub(super) class: Theme::EdgeClass<'a, I>,
    pub(super) particles: Vec<Particle<'a, Theme>>,
}

/// Creates an [`Edge`] with the given id and the theme's default class.
///
/// The id comes first, as in [`node`]. For a vocabulary whose `EdgeId` is `()`
/// that reads `edge((), from, to)`.
///
/// ```rust
/// use iced_nodegraph::{Edge, Indexed, PinRef, edge};
///
/// let e: Edge<'_, Indexed> = edge((), PinRef::new(0, 0), PinRef::new(1, 0));
/// ```
pub fn edge<'a, I: Ids, Theme: Catalog>(
    id: I::EdgeId,
    from: PinRef<I>,
    to: PinRef<I>,
) -> Edge<'a, I, Theme> {
    Edge {
        id,
        from,
        to,
        route: Vec::new(),
        class: Theme::default_edge(),
        particles: Vec::new(),
    }
}

impl<'a, I: Ids, Theme: Catalog> Edge<'a, I, Theme> {
    /// Sets the per-edge style closure: theme, [`EdgeStatus`], and both endpoint
    /// [`PinInfo`]s in draw order (start = output side, end = input side) ->
    /// resolved style.
    pub fn style(
        mut self,
        f: impl Fn(&Theme, EdgeStatus, PinInfo<'_, I>, PinInfo<'_, I>) -> EdgeStyle + 'a,
    ) -> Self
    where
        Theme::EdgeClass<'a, I>: From<EdgeStyleFn<'a, Theme, I>>,
    {
        self.class = (Box::new(f) as EdgeStyleFn<'a, Theme, I>).into();
        self
    }

    /// Sets the class the theme styles this edge by.
    pub fn class(mut self, class: impl Into<Theme::EdgeClass<'a, I>>) -> Self {
        self.class = class.into();
        self
    }

    /// Sets the anchors this edge wraps.
    ///
    /// Order is irrelevant: the widget derives the visiting order from where the
    /// anchors lie along the run between the two pins, the wrap direction from
    /// the arc the cable lays down, and the ring each cable takes at each anchor
    /// from the angular intervals its neighbours subtend there, refined by
    /// counting the crossings a candidate order actually produces. How many
    /// edges share an anchor decides only how many rings it shows, not which
    /// cable rides which. An id naming neither an anchor nor anything at all is
    /// skipped, and a repeated id counts once.
    pub fn route(mut self, anchors: impl IntoIterator<Item = I::AnchorId>) -> Self {
        self.route = anchors.into_iter().collect();
        self
    }

    /// Appends the particles travelling along this edge this frame.
    ///
    /// Each one is drawn where its age and speed put it; one past the input
    /// pin is skipped. Calls accumulate, so several sources may contribute.
    pub fn particles(mut self, particles: impl IntoIterator<Item = Particle<'a, Theme>>) -> Self {
        self.particles.extend(particles);
        self
    }
}

/// A marker travelling along an edge: a packet, a message in the queue the
/// edge stands for. Push one per frame through [`Edge::particles`]; the widget
/// draws it `speed * age` world units along the cable from the output pin and
/// nothing once that is past the input pin. It keeps no particle state and
/// reports nothing, so dropping a particle from the next frame is how a host
/// ends it.
pub struct Particle<'a, Theme = iced_widget::core::Theme>
where
    Theme: Catalog,
{
    pub(super) born: Instant,
    pub(super) speed: f32,
    pub(super) class: Theme::ParticleClass<'a>,
}

/// Creates a [`Particle`] born at `born` that travels `speed` world units per
/// second, with the theme's default class.
///
/// `born` is an `iced::time::Instant` (`web_time` on the web), the same clock
/// `iced::window::frames` reports.
pub fn particle<'a, Theme: Catalog>(born: Instant, speed: f32) -> Particle<'a, Theme> {
    Particle {
        born,
        speed,
        class: Theme::default_particle(),
    }
}

impl<'a, Theme: Catalog> Particle<'a, Theme> {
    /// Sets the style closure: theme -> resolved style.
    pub fn style(mut self, f: impl Fn(&Theme) -> ParticleStyle + 'a) -> Self
    where
        Theme::ParticleClass<'a>: From<ParticleStyleFn<'a, Theme>>,
    {
        self.class = (Box::new(f) as ParticleStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles this particle by.
    pub fn class(mut self, class: impl Into<Theme::ParticleClass<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

/// An anchor to push onto the graph: id, position, and the class the theme
/// styles it by.
///
/// Build with [`anchor`] + [`Anchor::style`], then add via
/// [`NodeGraph::push_anchor`]. Anchors have their own id space, are their own
/// collection and are never a widget-tree element: an edge names the anchors
/// it wraps through [`Edge::route`], and the widget lays the cable tangent to
/// one orbit of each.
#[allow(missing_debug_implementations)]
pub struct Anchor<'a, I: Ids = Indexed, Theme = iced_widget::core::Theme>
where
    Theme: Catalog,
{
    pub(super) id: I::AnchorId,
    pub(super) position: Point,
    pub(super) class: Theme::AnchorClass<'a>,
}

/// Creates an [`Anchor`] with the theme's default class.
pub fn anchor<'a, I: Ids, Theme: Catalog>(
    id: I::AnchorId,
    position: Point,
) -> Anchor<'a, I, Theme> {
    Anchor {
        id,
        position,
        class: Theme::default_anchor(),
    }
}

impl<'a, I: Ids, Theme: Catalog> Anchor<'a, I, Theme> {
    /// Sets the per-anchor style closure: receives the theme and the anchor's
    /// [`AnchorStatus`], returns the resolved style.
    pub fn style(mut self, f: impl Fn(&Theme, AnchorStatus) -> AnchorStyle + 'a) -> Self
    where
        Theme::AnchorClass<'a>: From<AnchorStyleFn<'a, Theme>>,
    {
        self.class = (Box::new(f) as AnchorStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles this anchor by.
    pub fn class(mut self, class: impl Into<Theme::AnchorClass<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

/// Creates an empty graph over the [`Indexed`] vocabulary: `usize` node, pin
/// and anchor ids, no edge ids, no pin payload.
///
/// For any other vocabulary name it once: `NodeGraph::<AppIds, _, _, _>::new()`.
pub fn node_graph<'a, Message, Theme, Renderer>() -> NodeGraph<'a, Indexed, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: iced_widget::core::renderer::Renderer,
{
    NodeGraph::new()
}

pub(crate) mod cable;
pub(crate) mod camera;
pub(crate) mod edge_path;
pub(crate) mod euclid;
pub(crate) mod focus;
pub(crate) mod input;
pub(crate) mod orbits;
pub(crate) mod state;
pub(crate) mod widget;

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
/// `nodes`/`pins`/`edges`/`anchors` are [`Counts`]; `timings` is the CPU cost
/// of each draw operation in stack order (geometry, background, foreground,
/// sdf prepare) and sums to roughly the per-frame CPU time. `sdf` is the SDF
/// pipeline's own counters for the frame. All timings are CPU-side; no GPU
/// profiling is done - the `sdf` byte and work counters describe GPU resource
/// use and work volume, not GPU time.
///
/// Reported one frame behind: the values are measured during `draw` and
/// delivered on the next redraw, mirroring the controlled `on_camera` pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphInfo {
    /// Node counts (total / in view / culled).
    pub nodes: Counts,
    /// Pin counts across all nodes.
    pub pins: Counts,
    /// Edge counts.
    pub edges: Counts,
    /// Anchor counts.
    pub anchors: Counts,
    /// Per-operation CPU timings, in stack order.
    pub timings: Vec<OpTiming>,
    /// The SDF pipeline's counters for the frame (GPU resource use and work
    /// volume, not GPU time).
    pub sdf: SdfStats,
}

/// Identifies what an in-progress drag is moving. Delivered to the
/// [`on_drag_start`](NodeGraph::on_drag_start) callback so the app can observe a
/// drag live (e.g. to broadcast it), alongside the commit-on-drop callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum DragInfo<I: Ids = Indexed> {
    /// Dragging a single node.
    Node { node_id: I::NodeId },
    /// Dragging a group of selected nodes.
    Group { node_ids: Vec<I::NodeId> },
    /// Dragging an edge from a pin (the source node and pin).
    Edge {
        from_node: I::NodeId,
        from_pin: I::PinId,
    },
    /// A selection box, anchored at this world-space corner.
    SelectionBox { start_x: f32, start_y: f32 },
    /// Moving one anchor.
    Anchor { anchor_id: I::AnchorId },
    /// Re-routing one edge with a phantom anchor at the cursor, picked up from
    /// the cable's run or from one of its wraps.
    Route { edge_id: I::EdgeId },
    /// Resizing one node from its corner grip.
    Resize { node_id: I::NodeId },
    /// Slicing across edges with the cutting tool.
    EdgeCut,
}

/// Type-safe reference to a pin: a `node_id` paired with a `pin_id`, over the
/// graph's [`Ids`].
///
/// The fields are public by design. `PinRef` is a transparent id pair with no
/// invariants to uphold: any node/pin id combination is structurally valid, and
/// whether two pins may actually connect is decided elsewhere (e.g. via
/// [`can_connect`](NodeGraph::can_connect)). Build it with a struct literal or
/// [`PinRef::new`], and match or destructure it freely.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinRef<I: Ids = Indexed> {
    /// The node's user id.
    pub node_id: I::NodeId,
    /// The pin's user id within its node.
    pub pin_id: I::PinId,
}

impl<I: Ids> Copy for PinRef<I>
where
    I::NodeId: Copy,
    I::PinId: Copy,
{
}

impl<I: Ids> PinRef<I> {
    /// Creates a pin reference from a node id and a pin id.
    pub fn new(node_id: I::NodeId, pin_id: I::PinId) -> Self {
        Self { node_id, pin_id }
    }
}

/// Which corner of the graph a [`Minimap`] sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// Placement and size of the minimap overlay, enabled by
/// [`NodeGraph::minimap`].
///
/// The map is screen-space chrome: it keeps its size and its corner at every
/// zoom, and it shows the union of the graph's node bounds with what the
/// viewport currently covers, so the viewport rectangle is always inside the
/// map - over an empty graph as well. Its appearance is
/// [`MinimapStyle`](crate::MinimapStyle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Minimap {
    /// Size of the map in screen pixels, capped at the graph's own size minus
    /// the margin on both sides.
    pub size: Size,
    /// The corner of the graph the map is pinned to.
    pub corner: Corner,
    /// Distance from the two edges of that corner, in screen pixels.
    pub margin: f32,
}

impl Default for Minimap {
    fn default() -> Self {
        Self {
            size: Size::new(200.0, 150.0),
            corner: Corner::BottomRight,
            margin: 12.0,
        }
    }
}

/// Node graph widget: a frame-scoped collection of [`Node`]s and [`Edge`]s plus
/// the callbacks and styles that apply to them.
///
/// # Type Parameters
///
/// - `I`: the graph's [`Ids`] - node, pin, edge and anchor id types plus the
///   per-pin payload, named once on a marker type. Defaults to [`Indexed`].
/// - `Message`: application message type
/// - `Theme`: the theme, which styles everything the graph draws through its
///   [`Catalog`] impl. Defaults to [`iced_widget::core::Theme`].
/// - `Renderer`: iced renderer type
///
/// `I` cannot be inferred from the ids pushed into the graph (an associated
/// type does not identify its trait impl), so a graph over custom ids names
/// it once: `NodeGraph::<AppIds, _, _, _>::new()`. `Message`, `Theme` and
/// `Renderer` infer from the callbacks and the `Element` the graph becomes.
#[allow(missing_debug_implementations)]
pub struct NodeGraph<
    'a,
    I: Ids = Indexed,
    Message = (),
    Theme = iced_widget::core::Theme,
    Renderer = iced_widget::renderer::Renderer,
> where
    Theme: Catalog,
{
    pub(super) size: Size<Length>,
    /// Set by [`id`](Self::id); what a [`focus`](crate::focus) task addresses.
    pub(super) id: Option<WidgetId>,
    /// Nodes in push order, which is also their initial z-order.
    pub(super) nodes: Vec<Node<'a, I, Message, Theme, Renderer>>,
    /// Id -> index map: O(1) `node_index` lookups and deterministic duplicate
    /// detection in `push_node` (first push wins).
    pub(super) node_lookup: HashMap<I::NodeId, usize>,
    /// Anchors in push order. Cables wrap them, but they are laid out and drawn
    /// entirely by the graph, so they form their own collection (see
    /// [`Anchor`]).
    pub(super) anchors: Vec<Anchor<'a, I, Theme>>,
    /// Id -> index map, the anchor counterpart of `node_lookup`. Its own id
    /// space: an anchor id never has to avoid a node's.
    pub(super) anchor_lookup: HashMap<I::AnchorId, usize>,
    /// Edges in push order. Endpoint pin ids are resolved to positional pin
    /// indices at draw time, since only the laid-out widget tree knows them.
    pub(super) edges: Vec<Edge<'a, I, Theme>>,
    /// The canvas class the theme resolves through [`Catalog::graph`].
    pub(super) graph_class: Theme::GraphClass<'a>,
    pub(super) on_connect: Option<Box<dyn Fn(PinRef<I>, PinRef<I>) -> Message + 'a>>,
    pub(super) on_disconnect: Option<Box<dyn Fn(PinRef<I>, PinRef<I>) -> Message + 'a>>,
    /// A drop the validation turned down, reported so the host can say why.
    /// The pair is in drag order (source pin first), since a refused pair may
    /// have no output to orient by.
    pub(super) on_connect_refused: Option<Box<dyn Fn(PinRef<I>, PinRef<I>) -> Message + 'a>>,
    pub(super) on_move: Option<Box<dyn Fn(Vector, Vec<I::NodeId>) -> Message + 'a>>,
    /// Grip-resize report, the size counterpart to `on_move`. Only nodes marked
    /// [`Node::resizable`] carry a grip, and only while this is wired.
    pub(super) on_resize: Option<Box<dyn Fn(I::NodeId, Size) -> Message + 'a>>,
    /// Anchor-move report, the anchor counterpart of `on_move`. Reports the new
    /// world position outright rather than a delta, mirroring `on_resize`.
    pub(super) on_anchor_move: Option<Box<dyn Fn(I::AnchorId, Point) -> Message + 'a>>,
    /// An anchor the user asked for by grabbing a cable mid-run, reported with
    /// the edge it belongs on and the world position of the release.
    pub(super) on_anchor_create: Option<Box<dyn Fn(I::EdgeId, Point) -> Message + 'a>>,
    /// An anchor an edge should now wrap.
    pub(super) on_route_attach: Option<Box<dyn Fn(I::EdgeId, I::AnchorId) -> Message + 'a>>,
    /// An anchor an edge should stop wrapping.
    pub(super) on_route_detach: Option<Box<dyn Fn(I::EdgeId, I::AnchorId) -> Message + 'a>>,
    /// An anchor the user asked to remove. The host also owns stripping it out
    /// of every route that named it.
    pub(super) on_anchor_delete: Option<Box<dyn Fn(I::AnchorId) -> Message + 'a>>,
    pub(super) on_select: Option<Box<dyn Fn(Vec<I::NodeId>) -> Message + 'a>>,
    pub(super) on_clone: Option<Box<dyn Fn(Vec<I::NodeId>) -> Message + 'a>>,
    pub(super) on_delete: Option<Box<dyn Fn(Vec<I::NodeId>) -> Message + 'a>>,
    /// Edges destroyed by the cutting tool, named by their user ids. The
    /// id-carrying counterpart to `on_disconnect` for the two paths where the
    /// widget holds a host-supplied edge.
    pub(super) on_edge_delete: Option<Box<dyn Fn(Vec<I::EdgeId>) -> Message + 'a>>,
    /// Live drag callbacks: fire continuously during a drag, alongside the
    /// commit-on-drop `on_move`. Observing a drag as it happens (to broadcast
    /// it, say) is the app's concern, so the widget only reports it.
    pub(super) on_drag_start: Option<Box<dyn Fn(DragInfo<I>) -> Message + 'a>>,
    pub(super) on_drag_update: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    pub(super) on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    /// Commit callback for the camera, the counterpart to
    /// [`camera`](Self::camera).
    pub(super) on_camera: Option<Box<dyn Fn(Point, f32) -> Message + 'a>>,
    /// Per-frame diagnostics callback.
    pub(super) on_info: Option<Box<dyn Fn(GraphInfo) -> Message + 'a>>,
    /// The class of the edge being dragged. The graph injects the source pin's
    /// color for inheriting (TRANSPARENT) stroke ends of the resolved style.
    pub(super) drag_edge_class: Theme::DragEdgeClass<'a, I>,
    /// The class of the phantom anchor a route drag holds at the cursor.
    pub(super) drag_anchor_class: Theme::AnchorClass<'a>,
    /// Box-selection rectangle class.
    pub(super) selection_box_class: Theme::SelectionBoxClass<'a>,
    /// Edge-cutting trail class.
    pub(super) cutting_tool_class: Theme::CuttingToolClass<'a>,
    /// The minimap overlay, when the host asked for one via
    /// [`minimap`](Self::minimap). Absent leaves every draw and input path
    /// untouched.
    pub(super) minimap: Option<Minimap>,
    /// Minimap class.
    pub(super) minimap_class: Theme::MinimapClass<'a>,
    /// Host-controlled camera (world position + zoom). The widget syncs its
    /// internal camera to this whenever the host changes it, while still running
    /// pan/zoom interaction internally and committing via `on_camera`. Mirrors
    /// the [`Node::selected`] / `on_select` pattern for selection.
    pub(super) camera: Option<(Point, f32)>,
    /// Connection validation. When set it is authoritative in
    /// `compute_valid_targets`; otherwise
    /// [`default_can_connect`](crate::connection::default_can_connect) applies.
    pub(super) can_connect: Option<Box<dyn Fn(PinEnd<'_, I>, PinEnd<'_, I>) -> bool + 'a>>,
    /// Key and pointer bindings; platform defaults unless overridden via
    /// [`keymap`](Self::keymap).
    pub(super) keymap: input::Keymap,
    /// World-unit grid a node drag lands on; unset leaves a drag continuous.
    /// Set through [`snap_grid`](Self::snap_grid).
    pub(super) snap_grid: Option<f32>,
}

impl<I: Ids, Message, Theme: Catalog, Renderer> Default
    for NodeGraph<'_, I, Message, Theme, Renderer>
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            id: None,
            nodes: Vec::new(),
            node_lookup: HashMap::new(),
            anchors: Vec::new(),
            anchor_lookup: HashMap::new(),
            edges: Vec::new(),
            graph_class: Theme::default_graph(),
            on_connect: None,
            on_disconnect: None,
            on_connect_refused: None,
            on_move: None,
            on_resize: None,
            on_anchor_move: None,
            on_anchor_create: None,
            on_route_attach: None,
            on_route_detach: None,
            on_anchor_delete: None,
            on_select: None,
            on_clone: None,
            on_delete: None,
            on_edge_delete: None,
            on_drag_start: None,
            on_drag_update: None,
            on_drag_end: None,
            on_camera: None,
            on_info: None,
            drag_edge_class: Theme::default_drag_edge(),
            drag_anchor_class: Theme::default_anchor(),
            selection_box_class: Theme::default_selection_box(),
            cutting_tool_class: Theme::default_cutting_tool(),
            minimap: None,
            minimap_class: Theme::default_minimap(),
            camera: None,
            can_connect: None,
            keymap: input::Keymap::default(),
            snap_grid: None,
        }
    }
}

impl<'a, I: Ids, Message, Theme: Catalog, Renderer> NodeGraph<'a, I, Message, Theme, Renderer> {
    /// Creates an empty graph that fills its container.
    ///
    /// `I` is named here when it is not [`Indexed`]:
    /// `NodeGraph::<AppIds, _, _, _>::new()`. [`node_graph`](crate::node_graph)
    /// is the shorthand for the indexed vocabulary.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the widget id a [`focus`](crate::focus) task addresses.
    pub fn id(mut self, id: impl Into<WidgetId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the host-controlled camera (world position + zoom).
    ///
    /// The widget snaps its camera to this whenever the host changes the value,
    /// while still running pan/zoom interaction internally and committing through
    /// [`on_camera`](Self::on_camera). This is the controlled-component
    /// counterpart to `on_camera`, exactly like [`Node::selected`] is to
    /// `on_select`: feed back what `on_camera` reports and the view stays in
    /// sync; push a new value (e.g. a reset to origin) and the view snaps there.
    pub fn camera(mut self, position: Point, zoom: f32) -> Self {
        self.camera = Some((position, zoom));
        self
    }

    /// Snaps a dragged node's origin, a dragged anchor's position and a
    /// resized node's far corner to a `spacing`-wide world grid.
    ///
    /// The preview and the value [`on_move`](Self::on_move),
    /// [`on_anchor_move`](Self::on_anchor_move) or
    /// [`on_resize`](Self::on_resize) reports are the same number, so a
    /// snapped drag lands where it was shown. A node drag's delta is computed
    /// on the grabbed node and shared by everything the drag carries, which
    /// keeps a group's relative layout intact - only the grabbed node ends up
    /// exactly on the grid.
    ///
    /// Holding [`Keymap::snap_override`](crate::Keymap::snap_override) (Alt by
    /// default) suspends the snap while it is held, mid-drag included.
    pub fn snap_grid(mut self, spacing: f32) -> Self {
        self.snap_grid = Some(spacing);
        self
    }

    /// Adds a node, styled by the theme unless the builder overrides it.
    ///
    /// Node ids must be unique among nodes. Anchors have their own id space, so
    /// a node id never has to avoid one. A duplicate push is ignored (the first
    /// push with the id wins) and debug builds assert on it. Prefer a stable id
    /// from your data (a DB key, `uuid::Uuid`, a typed newtype) over a
    /// hand-managed counter.
    pub fn push_node(mut self, node: Node<'a, I, Message, Theme, Renderer>) -> Self {
        if self.node_lookup.contains_key(&node.id) {
            debug_assert!(
                false,
                "duplicate node id {:?}: the duplicate push is ignored (first wins)",
                node.id,
            );
            return self;
        }
        self.node_lookup.insert(node.id.clone(), self.nodes.len());
        self.nodes.push(node);
        self
    }

    /// Adds every node of an iterator, as [`push_node`](Self::push_node) would
    /// one by one.
    pub fn nodes(
        self,
        nodes: impl IntoIterator<Item = Node<'a, I, Message, Theme, Renderer>>,
    ) -> Self {
        nodes.into_iter().fold(self, Self::push_node)
    }

    /// Adds an anchor: a routing waypoint the cables named in
    /// [`Edge::route`] wrap, never a widget-tree element.
    ///
    /// Anchor ids are their own space, numbered from zero whatever the nodes
    /// use, and follow the same rule as [`push_node`](Self::push_node): unique,
    /// first push wins, debug builds assert on a collision.
    pub fn push_anchor(mut self, anchor: Anchor<'a, I, Theme>) -> Self {
        if self.anchor_lookup.contains_key(&anchor.id) {
            debug_assert!(
                false,
                "duplicate anchor id {:?}: the duplicate push is ignored (first wins)",
                anchor.id,
            );
            return self;
        }
        self.anchor_lookup
            .insert(anchor.id.clone(), self.anchors.len());
        self.anchors.push(anchor);
        self
    }

    /// Adds every anchor of an iterator, as [`push_anchor`](Self::push_anchor)
    /// would one by one.
    pub fn anchors(self, anchors: impl IntoIterator<Item = Anchor<'a, I, Theme>>) -> Self {
        anchors.into_iter().fold(self, Self::push_anchor)
    }

    /// Adds an edge, styled by the theme unless the builder overrides it.
    ///
    /// The widget normalizes orientation when drawing and reporting, so the
    /// output pin is always the edge start (output -> input) regardless of the
    /// order given here.
    pub fn push_edge(mut self, edge: Edge<'a, I, Theme>) -> Self {
        self.edges.push(edge);
        self
    }

    /// Adds every edge of an iterator, as [`push_edge`](Self::push_edge) would
    /// one by one.
    pub fn edges(mut self, edges: impl IntoIterator<Item = Edge<'a, I, Theme>>) -> Self {
        self.edges.extend(edges);
        self
    }

    /// The user node id at a node index.
    pub(super) fn node_id_at(&self, index: usize) -> Option<&I::NodeId> {
        self.nodes.get(index).map(|node| &node.id)
    }

    /// The node index of a user node id.
    pub(super) fn node_index(&self, id: &I::NodeId) -> Option<usize> {
        self.node_lookup.get(id).copied()
    }

    /// The anchor index of a user id, or `None` when the id names a node or
    /// nothing at all.
    pub(super) fn anchor_index(&self, id: &I::AnchorId) -> Option<usize> {
        self.anchor_lookup.get(id).copied()
    }

    /// The selection the host marked on its nodes.
    pub(super) fn host_selection(&self) -> HashSet<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.selected)
            .map(|(index, _)| index)
            .collect()
    }

    /// The selection to render and act on: the widget's pending value while it
    /// waits to be applied, else what the host marked.
    ///
    /// Both halves of the widget read this, so what is highlighted is always what
    /// a delete or a group drag will act on.
    pub(in crate::node_graph) fn resolved_selection(
        &self,
        state: &state::NodeGraphState,
    ) -> HashSet<usize> {
        match &state.pending_selection {
            Some(pending) => pending.clone(),
            None => self.host_selection(),
        }
    }

    /// `selection` as node indices in push order.
    pub(super) fn selection_indices(selection: &HashSet<usize>) -> Vec<usize> {
        let mut indices: Vec<usize> = selection.iter().copied().collect();
        indices.sort_unstable();
        indices
    }

    /// `selection` as user node ids in push order.
    pub(super) fn selection_ids(&self, selection: &HashSet<usize>) -> Vec<I::NodeId> {
        self.node_ids_at(&Self::selection_indices(selection))
    }

    /// The user node ids at the given node indices, skipping unknown indices.
    pub(super) fn node_ids_at(&self, indices: &[usize]) -> Vec<I::NodeId> {
        indices
            .iter()
            .filter_map(|&index| self.node_id_at(index).cloned())
            .collect()
    }

    /// Sets the canvas style: background color and the optional tiling.
    ///
    /// The two interaction overlays have their own entry points
    /// ([`selection_box_style`](Self::selection_box_style),
    /// [`cutting_tool_style`](Self::cutting_tool_style)), so this closure is only
    /// about the canvas itself. For a static style, ignore the theme argument:
    /// `.graph_style(|_| GraphStyle { ..base })`.
    pub fn graph_style(mut self, f: impl Fn(&Theme) -> GraphStyle + 'a) -> Self
    where
        Theme::GraphClass<'a>: From<GraphStyleFn<'a, Theme>>,
    {
        self.graph_class = (Box::new(f) as GraphStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles the canvas by.
    pub fn graph_class(mut self, class: impl Into<Theme::GraphClass<'a>>) -> Self {
        self.graph_class = class.into();
        self
    }

    /// Sets the style of the selection box.
    ///
    /// [`default_selection_box_style`](crate::default_selection_box_style) is the
    /// theme-derived base and applies when this is unset; layer over it with
    /// struct-update, exactly like the per-node and per-edge closures.
    ///
    /// A selected *node's* appearance is not set here: it comes from the node's
    /// own [`style`](Node::style) closure, which receives
    /// [`NodeStatus::Selected`](crate::NodeStatus).
    ///
    /// ```
    /// use iced_nodegraph::{SelectionBoxStyle, default_selection_box_style, node_graph};
    /// use iced::Color;
    /// use iced_wgpu::Renderer;
    ///
    /// let graph = node_graph::<(), iced::Theme, Renderer>().selection_box_style(|theme| {
    ///     SelectionBoxStyle {
    ///         border_width: 2.0,
    ///         ..default_selection_box_style(theme)
    ///     }
    /// });
    /// ```
    pub fn selection_box_style(mut self, f: impl Fn(&Theme) -> SelectionBoxStyle + 'a) -> Self
    where
        Theme::SelectionBoxClass<'a>: From<SelectionBoxStyleFn<'a, Theme>>,
    {
        self.selection_box_class = (Box::new(f) as SelectionBoxStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles the selection box by.
    pub fn selection_box_class(mut self, class: impl Into<Theme::SelectionBoxClass<'a>>) -> Self {
        self.selection_box_class = class.into();
        self
    }

    /// Sets the style of the edge-cutting trail.
    ///
    /// [`default_cutting_tool_style`](crate::default_cutting_tool_style) is the
    /// theme-derived base and applies when this is unset.
    ///
    /// ```
    /// use iced_nodegraph::{CuttingToolStyle, default_cutting_tool_style, node_graph};
    /// use iced_wgpu::Renderer;
    ///
    /// let graph = node_graph::<(), iced::Theme, Renderer>().cutting_tool_style(|theme| {
    ///     CuttingToolStyle {
    ///         width: 5.0,
    ///         ..default_cutting_tool_style(theme)
    ///     }
    /// });
    /// ```
    pub fn cutting_tool_style(mut self, f: impl Fn(&Theme) -> CuttingToolStyle + 'a) -> Self
    where
        Theme::CuttingToolClass<'a>: From<CuttingToolStyleFn<'a, Theme>>,
    {
        self.cutting_tool_class = (Box::new(f) as CuttingToolStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles the edge-cutting trail by.
    pub fn cutting_tool_class(mut self, class: impl Into<Theme::CuttingToolClass<'a>>) -> Self {
        self.cutting_tool_class = class.into();
        self
    }

    /// Shows a minimap overlay in one corner of the graph.
    ///
    /// The map draws every node as a mark inside the union of the graph's node
    /// bounds and the visible world rectangle, plus a rectangle for what the
    /// viewport shows. Clicking it centers the camera on the world point
    /// pressed and dragging keeps centering it, both committed through
    /// [`on_camera`](Self::on_camera) - so a host that does not wire that
    /// callback still pans, it just never learns where to.
    ///
    /// ```
    /// use iced_nodegraph::{Corner, Minimap, node_graph};
    /// use iced::Size;
    /// use iced_wgpu::Renderer;
    ///
    /// let graph = node_graph::<(), iced::Theme, Renderer>().minimap(Minimap {
    ///     size: Size::new(240.0, 160.0),
    ///     corner: Corner::TopRight,
    ///     ..Minimap::default()
    /// });
    /// ```
    pub fn minimap(mut self, minimap: Minimap) -> Self {
        self.minimap = Some(minimap);
        self
    }

    /// Sets the style of the minimap overlay.
    ///
    /// [`default_minimap_style`](crate::default_minimap_style) is the
    /// theme-derived base and applies when this is unset. The style is drawn
    /// only while [`minimap`](Self::minimap) is set.
    ///
    /// ```
    /// use iced_nodegraph::{Minimap, MinimapStyle, default_minimap_style, node_graph};
    /// use iced::Color;
    /// use iced_wgpu::Renderer;
    ///
    /// let graph = node_graph::<(), iced::Theme, Renderer>()
    ///     .minimap(Minimap::default())
    ///     .minimap_style(|theme| MinimapStyle {
    ///         background: Color { a: 1.0, ..default_minimap_style(theme).background },
    ///         ..default_minimap_style(theme)
    ///     });
    /// ```
    pub fn minimap_style(mut self, f: impl Fn(&Theme) -> MinimapStyle + 'a) -> Self
    where
        Theme::MinimapClass<'a>: From<MinimapStyleFn<'a, Theme>>,
    {
        self.minimap_class = (Box::new(f) as MinimapStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles the minimap overlay by.
    pub fn minimap_class(mut self, class: impl Into<Theme::MinimapClass<'a>>) -> Self {
        self.minimap_class = class.into();
        self
    }

    /// Sets the style of the edge being dragged (before it connects). Receives
    /// the theme and the source pin, so the closure can derive the stroke from
    /// the pin's info (e.g. a port-typed color) for both ends of the loose edge.
    pub fn dragging_edge_style(
        mut self,
        f: impl Fn(&Theme, PinInfo<'_, I>) -> EdgeStyle + 'a,
    ) -> Self
    where
        Theme::DragEdgeClass<'a, I>: From<DragEdgeStyleFn<'a, Theme, I>>,
    {
        self.drag_edge_class = (Box::new(f) as DragEdgeStyleFn<'a, Theme, I>).into();
        self
    }

    /// Sets the class the theme styles the edge being dragged by.
    pub fn dragging_edge_class(mut self, class: impl Into<Theme::DragEdgeClass<'a, I>>) -> Self {
        self.drag_edge_class = class.into();
        self
    }

    /// Sets the style of the phantom anchor a route drag holds at the cursor.
    /// Resolved at [`AnchorStatus::Hovered`] for its paint and at
    /// [`AnchorStatus::Idle`] for the orbit radius the previewed cable wraps.
    pub fn dragging_anchor_style(
        mut self,
        f: impl Fn(&Theme, AnchorStatus) -> AnchorStyle + 'a,
    ) -> Self
    where
        Theme::AnchorClass<'a>: From<AnchorStyleFn<'a, Theme>>,
    {
        self.drag_anchor_class = (Box::new(f) as AnchorStyleFn<'a, Theme>).into();
        self
    }

    /// Sets the class the theme styles the phantom anchor by.
    pub fn dragging_anchor_class(mut self, class: impl Into<Theme::AnchorClass<'a>>) -> Self {
        self.drag_anchor_class = class.into();
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
    /// ```rust,no_run
    /// use iced_nodegraph::NodeGraph;
    /// use iced_nodegraph::connection::default_can_connect;
    ///
    /// # #[derive(Debug, Clone)]
    /// # enum Message {}
    /// # let ng: NodeGraph<'_, iced_nodegraph::Indexed, Message> = NodeGraph::new();
    /// let ng = ng
    ///     .can_connect(|from, to| default_can_connect(from, to) && from.info() == to.info());
    /// ```
    ///
    /// Or pick individual predicates ([`direction_ok`](crate::connection::direction_ok),
    /// [`not_same_node`](crate::connection::not_same_node),
    /// [`input_not_occupied`](crate::connection::input_not_occupied)).
    ///
    /// When not set, the widget applies `default_can_connect` (direction, not-same-
    /// node, one-edge-per-input).
    pub fn can_connect(mut self, f: impl Fn(PinEnd<'_, I>, PinEnd<'_, I>) -> bool + 'a) -> Self {
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
    pub fn on_connect(mut self, f: impl Fn(PinRef<I>, PinRef<I>) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when an edge is disconnected between two pins.
    ///
    /// Like [`on_connect`](Self::on_connect), the pair is normalized output-first
    /// (`from` = output, `to` = input).
    pub fn on_disconnect(mut self, f: impl Fn(PinRef<I>, PinRef<I>) -> Message + 'a) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    /// Sets a callback for a drop the connection validation turned down.
    ///
    /// Fires once, on release, when the drag ends over a pin that is reachable
    /// (within the same distance a snap would have taken it) but is not an
    /// accepted target - the one outcome of a drag a host cannot otherwise
    /// observe, since no snap happened and so no
    /// [`on_connect`](Self::on_connect) was published. A release over empty
    /// canvas, over the pin the drag started from, over a pin with
    /// [`disable_interactions`](crate::NodePin::disable_interactions), or over
    /// an accepting pin (which is already connected by then) reports nothing.
    ///
    /// `from` is the pin the drag started on and `to` the pin it was dropped
    /// on, in that order: what a refused pair has in common is nothing, not
    /// even an output, so there is no data flow to normalize to the way
    /// `on_connect` does.
    ///
    /// The refusal carries no reason. Validation is a single predicate -
    /// [`can_connect`](Self::can_connect) when set, otherwise
    /// [`default_can_connect`](crate::connection::default_can_connect) - and a
    /// predicate that answered `false` cannot say which of its rules did. A
    /// host that wants to distinguish "wrong direction" from "wrong type"
    /// re-runs its own rules on the reported pair, where it has its model.
    ///
    /// Only reachable while [`on_connect`](Self::on_connect) is wired: without
    /// it a pin press starts no edge drag at all.
    pub fn on_connect_refused(mut self, f: impl Fn(PinRef<I>, PinRef<I>) -> Message + 'a) -> Self {
        self.on_connect_refused = Some(Box::new(f));
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
    pub fn on_move(mut self, f: impl Fn(Vector, Vec<I::NodeId>) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    /// Sets a callback for a node resized by its corner grip.
    ///
    /// The callback receives the node id and the size the host should give that
    /// node's CONTENT element, in world units. It fires on every cursor move of
    /// the drag, so treat it as a stream, and it reports rather than applies:
    /// node size is the content's layout, which only the host can change, so
    /// the node stays the size it is until the host feeds the new one back.
    ///
    /// Required for resizing, together with [`Node::resizable`]. Without it a
    /// grip could only report into the void, so no grip is drawn or hit-tested
    /// and the corner drags the node like the rest of its body - the same
    /// gating [`on_move`](Self::on_move) has.
    pub fn on_resize(mut self, f: impl Fn(I::NodeId, Size) -> Message + 'a) -> Self {
        self.on_resize = Some(Box::new(f));
        self
    }

    /// Sets a callback for an anchor dragged to a new position.
    ///
    /// Reports the anchor's id and its new world position - the position
    /// outright, not a delta, because an anchor is a single point rather than a
    /// group. Fires once, on release; a motionless press and release is a click
    /// and reports nothing.
    ///
    /// Required for anchor dragging, the same gating [`on_move`](Self::on_move)
    /// has: anchor positions live in the host, so without a handler the drag
    /// would snap back on release. Without it the core is neither grabbable nor
    /// pointed at.
    pub fn on_anchor_move(mut self, f: impl Fn(I::AnchorId, Point) -> Message + 'a) -> Self {
        self.on_anchor_move = Some(Box::new(f));
        self
    }

    /// Sets a callback for an anchor the user created by grabbing a cable.
    ///
    /// Reports the edge the grab started on and the world position the drag
    /// released at. The host mints the anchor's id, pushes it via
    /// [`push_anchor`](Self::push_anchor), and adds that id to the edge's
    /// [`route`](Edge::route) - the widget cannot invent an id in the host's id
    /// space.
    ///
    /// Together with [`on_route_attach`](Self::on_route_attach) and
    /// [`on_route_detach`](Self::on_route_detach) this gates the cable's
    /// mid-run and wrap grab zones: all three must be set before a press on a
    /// cable does anything but fall through.
    ///
    /// Requires the edges to carry ids: with `EdgeId = ()` every report would
    /// name the same edge and the host could not tell which cable was grabbed.
    /// The same applies to `on_route_attach` and `on_route_detach`.
    pub fn on_anchor_create(mut self, f: impl Fn(I::EdgeId, Point) -> Message + 'a) -> Self {
        self.on_anchor_create = Some(Box::new(f));
        self
    }

    /// Sets a callback for an edge that should start wrapping an anchor.
    ///
    /// Fires on SNAP during a route drag, not on release, like
    /// [`on_connect`](Self::on_connect): one drag can attach and detach several
    /// times. The host adds the anchor id to that edge's
    /// [`route`](Edge::route).
    pub fn on_route_attach(mut self, f: impl Fn(I::EdgeId, I::AnchorId) -> Message + 'a) -> Self {
        self.on_route_attach = Some(Box::new(f));
        self
    }

    /// Sets a callback for an edge that should stop wrapping an anchor.
    ///
    /// The counterpart of [`on_route_attach`](Self::on_route_attach), fired
    /// when a route drag leaves the anchor it was snapped to, and by a
    /// pan-button click on the wrap itself. The host removes the anchor id from
    /// that edge's [`route`](Edge::route).
    pub fn on_route_detach(mut self, f: impl Fn(I::EdgeId, I::AnchorId) -> Message + 'a) -> Self {
        self.on_route_detach = Some(Box::new(f));
        self
    }

    /// Sets a callback for an anchor the user asked to remove.
    ///
    /// The host drops the anchor and strips its id out of every
    /// [`route`](Edge::route) that named it; an id left in a route simply
    /// resolves to nothing and is skipped, so a partial application degrades
    /// rather than breaks.
    ///
    /// Required for the delete gesture: without a handler a pan-button press on
    /// an anchor core is an ordinary pan.
    pub fn on_anchor_delete(mut self, f: impl Fn(I::AnchorId) -> Message + 'a) -> Self {
        self.on_anchor_delete = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the selection changes.
    ///
    /// The callback receives the list of currently selected node IDs.
    /// Fires on click-select, selection box, and Shift+click multi-select.
    ///
    /// The widget keeps a working selection, so it stays consistent without the
    /// host doing anything: a burst of clicks composes, and what is highlighted is
    /// what a delete or a group drag acts on.
    ///
    /// To make the host the source of truth, store the reported ids and mark the
    /// matching nodes with [`Node::selected`]. A changed marked set overrides the
    /// widget's working value; an unchanged one leaves it alone, so a host frame
    /// that has not caught up yet cannot undo an interaction.
    pub fn on_select(mut self, f: impl Fn(Vec<I::NodeId>) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the user requests to clone selected nodes (Ctrl+D).
    ///
    /// The callback receives the list of node IDs to clone.
    /// The application is responsible for creating the actual clones.
    pub fn on_clone(mut self, f: impl Fn(Vec<I::NodeId>) -> Message + 'a) -> Self {
        self.on_clone = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the user requests to delete selected nodes (Delete key).
    ///
    /// The callback receives the list of node IDs to delete.
    /// The application is responsible for removing the nodes from its data model.
    pub fn on_delete(mut self, f: impl Fn(Vec<I::NodeId>) -> Message + 'a) -> Self {
        self.on_delete = Some(Box::new(f));
        self
    }

    /// Sets a callback for edges destroyed by the cutting tool, named by the
    /// edge ids the host supplied to [`edge`].
    ///
    /// This is the only place the widget can name an edge: the cut paths hold a
    /// host-supplied [`Edge`], whereas [`on_disconnect`](Self::on_disconnect)
    /// also fires while a drag leaves a snapped pin, where no host edge exists
    /// yet. A cut is reported through *both* callbacks - wire this one when your
    /// edges carry ids, and read `on_disconnect` as live drag feedback.
    ///
    /// Mirrors [`on_delete`](Self::on_delete) for nodes: one batched call per cut
    /// gesture.
    pub fn on_edge_delete(mut self, f: impl Fn(Vec<I::EdgeId>) -> Message + 'a) -> Self {
        self.on_edge_delete = Some(Box::new(f));
        self
    }

    /// Reports the start of a drag, naming what it moves.
    ///
    /// Fires for every drag of graph content - a node, a group, an edge, a
    /// selection box, an anchor, a route, a resize grip, the cutting tool -
    /// in addition to the commit-on-release callbacks, and each of those pairs
    /// with one [`on_drag_end`](Self::on_drag_end). It exists for hosts that
    /// mirror an in-progress drag somewhere else - a collaborative session, an
    /// inspector - and nothing is gated on it: omitting it changes no
    /// behaviour.
    ///
    /// Two gestures report only the end: a travel-free pan-button click on an
    /// anchor core or a cable wrap (a delete or a detach, kept so a host that
    /// collects orphaned anchors on gesture end hears it), and a cancel (a
    /// second touch contact, say). A canvas pan and a minimap drag report
    /// neither. A host that brackets work across a drag must therefore key
    /// the opening on `DragInfo` and tolerate a close it never opened.
    pub fn on_drag_start(mut self, f: impl Fn(DragInfo<I>) -> Message + 'a) -> Self {
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

    /// Reports that a drag ended, whether it committed or was discarded.
    ///
    /// Fires on every transition back to idle, including a cancel (a second
    /// touch contact, say) and the end-only gestures listed at
    /// [`on_drag_start`](Self::on_drag_start). So it is the reliable place to
    /// notice that the widget is no longer dragging, and the wrong place to
    /// assume a matching start.
    pub fn on_drag_end(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_drag_end = Some(Box::new(f));
        self
    }

    /// Sets the commit callback for the camera.
    ///
    /// Fires with the new camera position and zoom when the user finishes a pan
    /// drag, zooms (zoom shifts position too, so both report together), or a
    /// [`focus`](crate::focus) task lands. Store the value and feed it back via
    /// [`camera`](Self::camera) to keep the controlled camera in sync.
    pub fn on_camera(mut self, f: impl Fn(Point, f32) -> Message + 'a) -> Self {
        self.on_camera = Some(Box::new(f));
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
