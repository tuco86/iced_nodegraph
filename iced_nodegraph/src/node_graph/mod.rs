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

use iced_widget::core::widget::Id as WidgetId;
use iced_widget::core::{Element, Length, Point, Size, Vector};

use self::focus::{FocusOptions, FocusTarget};
use self::widget::edge_path;
use crate::ids::{Ids, Indexed};
use crate::node_pin::{PinDirection, PinEnd, PinInfo};
use crate::style::{
    AnchorStatus, AnchorStyle, AnchorStyleFn, Catalog, CuttingToolStyle, CuttingToolStyleFn,
    DragEdgeStyleFn, EdgeCurve, EdgeStatus, EdgeStyle, EdgeStyleFn, GraphStyle, GraphStyleFn,
    MinimapStyle, MinimapStyleFn, NodeStatus, NodeStyle, NodeStyleFn, PinStatus, PinStyle,
    PinStyleFn, SelectionBoxStyle, SelectionBoxStyleFn,
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

pub(crate) mod camera;
pub(crate) mod euclid;
pub(crate) mod focus;
pub(crate) mod input;
pub(crate) mod orbits;
pub(crate) mod state;
pub(crate) mod widget;

/// A pin resolved for this frame: where it is and which way its side faces.
///
/// The output of a caller's endpoint resolver. The widget's two halves work in
/// different coordinate spaces (layout-absolute for drawing, world for input),
/// and this is where they meet; `direction` is what orients a cable
/// output-first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Station {
    pub point: [f32; 2],
    pub side: u32,
    pub direction: Option<PinDirection>,
}

/// One edge's topology lowered to the hop chain it draws as.
///
/// The single walk from graph topology to cable geometry: drawing, cutting,
/// hit-testing and hover all read this, so what a gesture aims at is what the
/// frame put on screen.
#[derive(Debug)]
pub(super) struct CableGeometry<'e, I: Ids> {
    /// Index into `NodeGraph::edges`.
    pub edge: usize,
    /// The stations to build, output pin first.
    pub hops: Vec<edge_path::Hop>,
    /// Hop index -> the `(anchor index, orbit)` that hop wraps, for every wrap
    /// hop in `hops`. A phantom wrap contributes no entry: it names no anchor
    /// the host owns.
    pub rings: Vec<(usize, (usize, u8))>,
    /// Both endpoint pins, oriented output -> input like `hops`.
    pub ends: (&'e PinRef<I>, &'e PinRef<I>),
}

/// A wrap inserted into one edge's route for the duration of a route drag, so
/// the previewed cable runs where the committed one will.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RoutePhantom {
    /// The edge being re-routed, as an index into `NodeGraph::edges`.
    pub edge: usize,
    /// An anchor dropped from the preview because a detach was published for
    /// it. Held until the host applies that detach, so the cable does not snap
    /// back to the anchor for one frame.
    pub exclude: Option<usize>,
    pub kind: PhantomKind,
}

impl RoutePhantom {
    /// The route edit this preview stands for, as
    /// [`anchor_rings`](NodeGraph::anchor_rings) must count it.
    ///
    /// One derivation for both halves of the widget, so the orbit `draw`
    /// previews and the ring `update` measures against cannot disagree.
    pub fn pending(&self) -> PendingRoute {
        PendingRoute {
            edge: self.edge,
            attach: match self.kind {
                PhantomKind::Snap { anchor } => Some(anchor),
                // A ring at the cursor belongs to no anchor, so it takes no
                // orbit from one.
                PhantomKind::At { .. } => None,
            },
            detach: self.exclude,
        }
    }
}

/// A route drag's edit to the host's routes, before the host has applied it.
///
/// Folded into the occupancy the frame derives so a drag predicts the orbit the
/// round trip will produce rather than the one at the end of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingRoute {
    /// The edge the drag holds, as an index into `NodeGraph::edges`.
    pub edge: usize,
    /// The anchor the drag has attached it to.
    pub attach: Option<usize>,
    /// The anchor the drag has pulled it off.
    pub detach: Option<usize>,
}

/// What the phantom wrap is laid tangent to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PhantomKind {
    /// A ring at the cursor, unattached: the drag has not snapped to anything.
    At { center: [f32; 2], radius: f32 },
    /// The orbit offered by a snapped anchor. Inserted only when the host's own
    /// route does not already carry that anchor, so it self-corrects once the
    /// attach round-trips.
    Snap { anchor: usize },
}

/// One cable resolved as far as it can be before an orbit is chosen: its two
/// pins and the wraps in visiting order.
struct CablePlan<'e, I: Ids> {
    edge: usize,
    head: Station,
    tail: Station,
    ends: (&'e PinRef<I>, &'e PinRef<I>),
    wraps: Vec<WrapPlan>,
}

impl<I: Ids> CablePlan<'_, I> {
    /// The hop chain this cable draws as, given a ring per wrap.
    ///
    /// Called once per candidate arrangement during the orbit search and once
    /// more for the arrangement that wins, so the hop chain and the rings a
    /// candidate was judged on are the ones that ship - identical by
    /// construction, not by agreement between two walks.
    ///
    /// The PATH built from that chain can still differ on the draw side, which
    /// resolves this frame's `EdgeCurve` for the stroke while the search measures
    /// against the curve the last frame published. A host switching a curve
    /// therefore settles the ring assignment one frame later, and the first
    /// frame of all measures against the default.
    ///
    /// The second half is the ring each wrap hop landed on, by hop index, which
    /// only this walk knows: a wrap whose circle does not resolve contributes no
    /// hop and so shifts every index after it.
    fn chain(
        &self,
        orbits: &[u8],
        ring: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
    ) -> (Vec<edge_path::Hop>, Vec<(usize, (usize, u8))>) {
        let mut hops = Vec::with_capacity(self.wraps.len() + 2);
        let mut rings = Vec::with_capacity(self.wraps.len());
        hops.push(edge_path::Hop::Pin {
            point: self.head.point,
            side: self.head.side,
        });
        for (at, wrap) in self.wraps.iter().enumerate() {
            let orbit = orbits.get(at).copied().unwrap_or(0);
            let circle = match (wrap.anchor, wrap.radius) {
                (_, Some(radius)) => Some(edge_path::Orbit {
                    center: wrap.center,
                    radius,
                }),
                (Some(anchor), None) => ring(anchor, orbit),
                (None, None) => None,
            };
            let Some(circle) = circle else { continue };
            if let Some(anchor) = wrap.anchor {
                rings.push((hops.len(), (anchor, orbit)));
            }
            hops.push(edge_path::Hop::Wrap { orbit: circle });
        }
        hops.push(edge_path::Hop::Pin {
            point: self.tail.point,
            side: self.tail.side,
        });
        (hops, rings)
    }

    /// The anchors both cables wrap, in the order this one reaches them.
    ///
    /// Two cables can only be moved relative to one another by a ring choice
    /// where they meet, so this is what decides whether a pair is worth
    /// measuring at all. Visiting order rather than index order, because
    /// CONSECUTIVE entries are the stretches the pair actually flies together:
    /// two anchors with a third shared one between them are not one corridor but
    /// two, and the band spanning them adds nothing the two halves do not
    /// already cover.
    fn shared_anchors(&self, other: &Self) -> Vec<usize> {
        self.wraps
            .iter()
            .filter_map(|wrap| wrap.anchor)
            .filter(|anchor| other.wraps.iter().any(|wrap| wrap.anchor == Some(*anchor)))
            .collect()
    }
}

/// One wrap before its radius is known.
struct WrapPlan {
    /// The anchor it belongs to, or `None` for a ring held at the cursor, which
    /// belongs to no anchor and so takes no orbit from one.
    anchor: Option<usize>,
    center: [f32; 2],
    /// Set only for a cursor ring, whose radius is given rather than assigned.
    radius: Option<f32>,
    /// The angular interval its neighbours subtend at the anchor, the key its
    /// orbit is assigned by.
    span: f32,
}

/// The angle between a wrap's two neighbouring stations, seen from the anchor
/// centre, folded into `[0, PI]`.
///
/// An interval, NOT the arc the cable lays down - the two are anti-correlated.
/// The tangents sit `acos(r / d)` off each neighbour's bearing, so with
/// `A = acos(r / d_prev) + acos(r / d_next)` the realized arc is
/// `folded(delta - A)`: the cable whose neighbours subtend the LEAST goes
/// furthest round the ring.
///
/// The interval is what orders two cables, and it is deliberately what is
/// measured here. Where two cables enter an anchor from the same side and leave
/// to the same side, their intervals NEST, and the contained one belongs inside:
/// seat it outside and its legs have to cut across the other cable twice. That
/// question is settled by the intervals alone, which is why this reads only the
/// centre and the two neighbours - and so is known before any radius is, which
/// is what lets it choose one.
fn wrap_span(center: [f32; 2], prev: [f32; 2], next: [f32; 2]) -> f32 {
    let bearing = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let delta = (bearing(next) - bearing(prev)).rem_euclid(std::f32::consts::TAU);
    delta.min(std::f32::consts::TAU - delta)
}

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
    /// Highest per-fine-tile slot count against the 64-slot cap
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

    /// Snaps a dragged node's origin to a `spacing`-wide world grid.
    ///
    /// The preview and the delta [`on_move`](Self::on_move) reports are the
    /// same number, so a snapped drag lands where it was shown. The delta is
    /// computed on the grabbed node and shared by everything the drag carries,
    /// which keeps a group's relative layout intact - only the grabbed node
    /// ends up exactly on the grid.
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

    /// One edge's [`route`](Edge::route) as anchor indices: ids resolved
    /// through `anchor_lookup`, unknown ids dropped, repeats collapsed to
    /// their first occurrence.
    ///
    /// Order is the host's authoring order, which only matters as the
    /// tie-breaker when the run gives no ordering at all (see
    /// [`edge_hops`](Self::edge_hops)).
    pub(super) fn resolved_route(&self, edge: usize) -> Vec<usize> {
        let Some(edge) = self.edges.get(edge) else {
            return Vec::new();
        };
        let mut out: Vec<usize> = Vec::with_capacity(edge.route.len());
        for id in &edge.route {
            if let Some(index) = self.anchor_index(id)
                && !out.contains(&index)
            {
                out.push(index);
            }
        }
        out
    }

    /// How many rings each anchor shows: the number of edges routed through it,
    /// indexed by anchor index.
    ///
    /// A count, not an order. Which cable rides which ring is decided in
    /// [`orbits::assign`] from the angular intervals its neighbours subtend plus
    /// a measured crossing search; this answers only how many there are, which
    /// is what bounds an anchor's drawn extent and how far a route drag can
    /// reach it.
    ///
    /// `pending` folds in a route drag's edit before the host has applied it, so
    /// the ring the drag measures against is the one the frame draws rather than
    /// the one the graph carried before the gesture started.
    ///
    /// An anchor is capped at `u8::MAX + 1` rings, since an orbit index is a
    /// `u8`; the surplus is not counted and so not drawn.
    ///
    /// Counted off the ROUTES the host authored, not off the cables that get
    /// built: [`edge_hops`](Self::edge_hops) drops an edge whose endpoint pin
    /// this frame's content does not contain, and such an edge still counts a
    /// ring here. An anchor named only by dropped edges therefore draws a ring
    /// no cable rides, and a route drag reaches it one ring wide. The
    /// disagreement is only ever in that direction - the cables seated at an
    /// anchor are a subset of the rings counted for it - so no cable is ever
    /// placed on a ring the hit test does not know about. Closing it needs a
    /// pin resolver at every caller, which `resolve_focus_target` does not
    /// have.
    pub(super) fn anchor_rings(&self, pending: Option<PendingRoute>) -> Vec<usize> {
        const MOST: usize = u8::MAX as usize + 1;
        let mut rings = vec![0usize; self.anchors.len()];
        let dropped = |edge: usize, anchor: usize| {
            pending.is_some_and(|p| p.edge == edge && p.detach == Some(anchor))
        };
        let mut count = |anchor: usize| {
            if let Some(rings) = rings.get_mut(anchor) {
                debug_assert!(
                    *rings < MOST,
                    "anchor {anchor} carries more than {MOST} edges; the surplus is not drawn",
                );
                *rings = rings.saturating_add(1).min(MOST);
            }
        };
        for edge in 0..self.edges.len() {
            let route = self.resolved_route(edge);
            for anchor in &route {
                if !dropped(edge, *anchor) {
                    count(*anchor);
                }
            }
            if let Some(pending) = pending
                && pending.edge == edge
                && let Some(anchor) = pending.attach
                && !route.contains(&anchor)
            {
                count(anchor);
            }
        }
        rings
    }

    /// Every edge lowered to the hop chain it draws as, resolved through the
    /// caller's own coordinate space.
    ///
    /// `pin` resolves an endpoint (returning `None` drops the whole edge, since
    /// a cable with one end missing has nowhere to run); `ring` resolves an
    /// orbit's circle, which the draw path takes from the resolved
    /// [`AnchorStyle`] and the interaction path from the radii the last frame
    /// published; `curve` gives the shape an edge's legs take, which the orbit
    /// search needs because it judges a candidate by building it.
    ///
    /// Two derivations, in this order:
    ///
    /// The visiting order: each anchor's centre is projected onto the straight
    /// run between the two pins and the wraps are taken in ascending
    /// projection, so a cable passes its anchors in the order it actually
    /// reaches them. A run of zero length leaves nothing to project onto, and
    /// the authored order stands.
    ///
    /// The orbit, in [`orbits::assign`]: which ring each cable takes at each
    /// anchor. Cables sharing an anchor nest by how far each wraps it, shortest
    /// innermost, and where a pair also shares a second anchor - so flies the
    /// corridor between them - candidate orders are built and their crossings
    /// counted, keeping whichever measurably crosses least.
    pub(super) fn edge_hops(
        &self,
        pin: &dyn Fn(&PinRef<I>) -> Option<Station>,
        ring: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
        curve: &dyn Fn(usize) -> EdgeCurve,
        phantom: Option<&RoutePhantom>,
    ) -> Vec<CableGeometry<'_, I>> {
        // Resolve every cable's stations and visiting order first. Nothing here
        // needs a radius, which is what lets the orbit be decided afterwards
        // from the whole picture rather than one edge at a time.
        let mut plans: Vec<CablePlan<'_, I>> = Vec::with_capacity(self.edges.len());
        for (index, edge) in self.edges.iter().enumerate() {
            let (Some(a), Some(b)) = (pin(&edge.from), pin(&edge.to)) else {
                continue;
            };
            // Output first, so gradient, arrow and flow follow the data-flow
            // direction however the edge was authored or dragged. Two ends
            // claiming the same direction leave nothing to order by.
            let is_output = |s: &Station| matches!(s.direction, Some(PinDirection::Output));
            let head_is_from = is_output(&a) || !is_output(&b);
            let ((head, head_ref), (tail, tail_ref)) = if head_is_from {
                ((a, &edge.from), (b, &edge.to))
            } else {
                ((b, &edge.to), (a, &edge.from))
            };

            let phantom = phantom.filter(|p| p.edge == index);
            let route = self.resolved_route(index);
            let mut wraps: Vec<WrapPlan> = Vec::with_capacity(route.len() + 1);
            for &anchor in &route {
                if phantom.and_then(|p| p.exclude) == Some(anchor) {
                    continue;
                }
                // Orbit 0 only to read the centre: an anchor's orbits are
                // concentric, so every one of them has it.
                if let Some(circle) = ring(anchor, 0) {
                    wraps.push(WrapPlan {
                        anchor: Some(anchor),
                        center: circle.center,
                        radius: None,
                        span: 0.0,
                    });
                }
            }
            if let Some(phantom) = phantom {
                match phantom.kind {
                    PhantomKind::At { center, radius } => wraps.push(WrapPlan {
                        anchor: None,
                        center,
                        radius: Some(radius),
                        span: 0.0,
                    }),
                    // Already in the host's route: the attach has round-tripped
                    // and the real wrap above is the one to draw.
                    PhantomKind::Snap { anchor } if !route.contains(&anchor) => {
                        if let Some(circle) = ring(anchor, 0) {
                            wraps.push(WrapPlan {
                                anchor: Some(anchor),
                                center: circle.center,
                                radius: None,
                                span: 0.0,
                            });
                        }
                    }
                    PhantomKind::Snap { .. } => {}
                }
            }

            let run = [tail.point[0] - head.point[0], tail.point[1] - head.point[1]];
            let len2 = run[0] * run[0] + run[1] * run[1];
            if len2 >= 1e-6 {
                let projection = |c: &[f32; 2]| {
                    ((c[0] - head.point[0]) * run[0] + (c[1] - head.point[1]) * run[1]) / len2
                };
                wraps.sort_by(|a, b| {
                    projection(&a.center)
                        .total_cmp(&projection(&b.center))
                        .then(
                            a.anchor
                                .unwrap_or(usize::MAX)
                                .cmp(&b.anchor.unwrap_or(usize::MAX)),
                        )
                });
            }
            // Each wrap's span needs its NEIGHBOURS, so it is read once the
            // visiting order is settled.
            for i in 0..wraps.len() {
                let prev = if i == 0 {
                    head.point
                } else {
                    wraps[i - 1].center
                };
                let next = wraps.get(i + 1).map_or(tail.point, |w| w.center);
                wraps[i].span = wrap_span(wraps[i].center, prev, next);
            }

            plans.push(CablePlan {
                edge: index,
                head,
                tail,
                ends: (head_ref, tail_ref),
                wraps,
            });
        }

        // The orbits come from the whole picture at once: an anchor's order is
        // not separable from its neighbours', because two cables flying the same
        // stretch from one anchor to the next stay apart only while their
        // nesting agrees at both ends.
        let wraps: Vec<Vec<orbits::Wrap>> = plans
            .iter()
            .map(|plan| {
                plan.wraps
                    .iter()
                    .map(|wrap| orbits::Wrap {
                        anchor: wrap.anchor,
                        span: wrap.span,
                    })
                    .collect()
            })
            .collect();
        // How many rings each anchor shows, which is how far out its geometry
        // reaches - the bound a crossing has to clear to count as being in the
        // open space between two anchors rather than at a wrap.
        let mut rings_at = vec![0u8; self.anchors.len()];
        for wraps in &wraps {
            for wrap in wraps {
                if let Some(anchor) = wrap.anchor
                    && let Some(count) = rings_at.get_mut(anchor)
                {
                    *count = count.saturating_add(1);
                }
            }
        }
        let contested = orbits::contested(&wraps, self.anchors.len());
        // Only pairs sharing TWO anchors can cross in a corridor, and a corridor
        // crossing is the only thing measured, so those pairs are the whole
        // search space. A pair meeting at one anchor is settled by containment.
        //
        // The bands a pair's crossings are judged against depend on the anchors
        // it shares and how many rings each of those shows, neither of which a
        // candidate can change, so they are built once here rather than per
        // measurement.
        let mut corridors: Vec<(usize, usize, Vec<edge_path::Corridor>)> = Vec::new();
        // No contested anchor means no pair shares two, so the scan below would
        // find nothing. Skipping it keeps a graph of unrouted cables off an
        // all-pairs walk it can never learn anything from.
        let riding: Vec<usize> = if contested.is_empty() {
            Vec::new()
        } else {
            (0..plans.len())
                .filter(|&slot| {
                    wraps[slot]
                        .iter()
                        .filter(|wrap| wrap.anchor.is_some())
                        .count()
                        > 1
                })
                .collect()
        };
        for (at, &one) in riding.iter().enumerate() {
            for &other in &riding[at + 1..] {
                let (plan, partner) = (&plans[one], &plans[other]);
                let shared = plan.shared_anchors(partner);
                if shared.len() < 2 {
                    continue;
                }
                let reach = |anchor: usize| {
                    ring(
                        anchor,
                        rings_at.get(anchor).copied().unwrap_or(1).saturating_sub(1),
                    )
                };
                let mut bands = Vec::new();
                for leg in shared.windows(2) {
                    if let (Some(from), Some(to)) = (reach(leg[0]), reach(leg[1])) {
                        bands.push(edge_path::Corridor { from, to });
                    }
                }
                if !bands.is_empty() {
                    corridors.push((one, other, bands));
                }
            }
        }
        let mut movable: Vec<usize> = corridors
            .iter()
            .flat_map(|&(one, other, _)| [one, other])
            .collect();
        movable.sort_unstable();
        movable.dedup();
        // A candidate costs one build per movable cable plus one crossing count
        // per band, which is what the search's budget is measured in.
        let per_candidate = movable.len()
            + corridors
                .iter()
                .map(|(_, _, bands)| bands.len())
                .sum::<usize>();
        // What a candidate arrangement actually does, built and counted rather
        // than predicted: whether two cables cross along a corridor also depends
        // on which way each wraps each end, and that is chosen by the geometry
        // from the radii, so it is not knowable before the rings are.
        let mut cost = |arrangement: &[Vec<u8>]| {
            let flattened: Vec<(usize, Vec<[f32; 2]>)> = movable
                .iter()
                .map(|&slot| {
                    let (hops, _) = plans[slot].chain(&arrangement[slot], ring);
                    let path = edge_path::build(&hops, &curve(plans[slot].edge)).path;
                    (slot, edge_path::polyline(&path))
                })
                .collect();
            let chords = |slot: usize| {
                flattened
                    .binary_search_by_key(&slot, |&(at, _)| at)
                    .ok()
                    .map(|at| flattened[at].1.as_slice())
            };
            let mut crossings = 0;
            for (one, other, bands) in &corridors {
                let (Some(first), Some(second)) = (chords(*one), chords(*other)) else {
                    continue;
                };
                crossings += edge_path::crossings_between_flattened(first, second, bands);
            }
            crossings
        };
        let assigned = orbits::assign(
            &wraps,
            self.anchors.len(),
            &contested,
            orbits::budget(movable.len(), per_candidate),
            &mut cost,
        );

        plans
            .iter()
            .enumerate()
            .map(|(slot, plan)| {
                let (hops, rings) = plan.chain(&assigned[slot], ring);
                CableGeometry {
                    edge: plan.edge,
                    hops,
                    rings,
                    ends: plan.ends,
                }
            })
            .collect()
    }

    /// The anchors a route drag on `edge` may snap to.
    ///
    /// Every anchor the edge does not already wrap, plus `detached` - the one a
    /// wrap grab just pulled off. That one stays eligible because the detach may
    /// not have round-tripped yet, and a drag that cannot put an anchor back
    /// where it came from would be a trap.
    pub(super) fn route_snap_eligible(&self, edge: usize, detached: Option<usize>) -> Vec<usize> {
        let route = self.resolved_route(edge);
        (0..self.anchors.len())
            .filter(|a| !route.contains(a) || detached == Some(*a))
            .collect()
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
    /// Fires for the drags [`DragInfo`] can name, in addition to the
    /// commit-on-release callbacks. It exists for hosts that mirror an
    /// in-progress drag somewhere else - a collaborative session, an inspector -
    /// and nothing is gated on it: omitting it changes no behaviour.
    ///
    /// It does NOT pair with [`on_drag_end`](Self::on_drag_end) one for one. A
    /// canvas pan reports neither; an anchor drag, a route drag and a pan-button
    /// click on a core or a wrap report only the end, because `DragInfo` has no
    /// variant that names an anchor or a route. A host that brackets work across
    /// a drag must key the opening on `DragInfo` and tolerate a close it never
    /// opened.
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
    /// touch contact, say) and including drags that reported no start - see
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A vocabulary whose ids are all `usize`, so a node, an anchor and an edge
    /// can be told apart by value in a failure message.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct AllUsize;

    impl Ids for AllUsize {
        type NodeId = usize;
        type PinId = usize;
        type EdgeId = usize;
        type AnchorId = usize;
        type Payload = ();
    }

    type Graph<'a> =
        NodeGraph<'a, AllUsize, (), iced_widget::core::Theme, iced_widget::renderer::Renderer>;

    /// Node 0 carries the output at the origin, node 1 the input 400 to the
    /// right, so the run is the positive x axis and a projection is just an x
    /// coordinate.
    fn station(pin: &PinRef<AllUsize>) -> Option<Station> {
        match pin.node_id {
            0 => Some(Station {
                point: [0.0, 0.0],
                side: 1,
                direction: Some(PinDirection::Output),
            }),
            1 => Some(Station {
                point: [400.0, 0.0],
                side: 3,
                direction: Some(PinDirection::Input),
            }),
            _ => None,
        }
    }

    /// Every edge on the default curve, which is what a host that never styles
    /// one gets.
    fn curve(_edge: usize) -> EdgeCurve {
        EdgeCurve::default()
    }

    /// Rings of a fixed radius, so a test reads the ORDER of the wraps rather
    /// than their size.
    fn ring<'g>(graph: &'g Graph<'_>) -> impl Fn(usize, u8) -> Option<edge_path::Orbit> + 'g {
        move |anchor, _orbit| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: 16.0,
            })
        }
    }

    /// Rings whose radius encodes the orbit index, so a test can read WHICH
    /// orbit a wrap was given rather than only where it sits.
    fn indexed_ring<'g>(
        graph: &'g Graph<'_>,
    ) -> impl Fn(usize, u8) -> Option<edge_path::Orbit> + 'g {
        move |anchor, orbit| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: 100.0 + orbit as f32,
            })
        }
    }

    /// The orbit index each wrap was given, read back out of `indexed_ring`.
    fn wrap_orbits(hops: &[edge_path::Hop]) -> Vec<u8> {
        hops.iter()
            .filter_map(|hop| match hop {
                edge_path::Hop::Wrap { orbit } => Some((orbit.radius - 100.0) as u8),
                edge_path::Hop::Pin { .. } => None,
            })
            .collect()
    }

    /// Cables whose geometry gives no reason to prefer one over the other fall
    /// back to edge index, and a snapped drag previews the orbit that tie will
    /// hand it.
    ///
    /// Every cable here runs between the SAME two pins, so every span is equal
    /// and only the index is left to order by. A drag onto an anchor that
    /// already carries a higher-indexed edge is therefore inserted ahead of it
    /// and both cables move. Predicting the free slot past the last instead
    /// would preview the cable on a ring it will not sit on.
    #[test]
    fn a_snap_phantom_takes_the_orbit_the_tie_gives() {
        // The dragged edge is the HIGHEST index: tie order and free slot agree.
        let mut trailing = graph_with_anchors(&[(10, 200.0)]);
        trailing = trailing.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        trailing = trailing.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));
        trailing = trailing.push_edge(edge(2, pin_ref(0), pin_ref(1)));
        let phantom = RoutePhantom {
            edge: 2,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        assert_eq!(trailing.anchor_rings(Some(phantom.pending())), vec![3]);
        let ring = indexed_ring(&trailing);
        let cables = trailing.edge_hops(&station, &ring, &curve, Some(&phantom));
        assert_eq!(wrap_orbits(&cables[0].hops), vec![0]);
        assert_eq!(wrap_orbits(&cables[1].hops), vec![1]);
        assert_eq!(wrap_orbits(&cables[2].hops), vec![2]);

        // The dragged edge is the LOWEST index: it takes orbit 0 and pushes the
        // resident cable outward. This is the case a free-slot prediction gets
        // wrong, and it is the one the drag then measures its unsnap against.
        let mut leading = graph_with_anchors(&[(10, 200.0)]);
        leading = leading.push_edge(edge(0, pin_ref(0), pin_ref(1)));
        leading = leading.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        assert_eq!(
            leading.anchor_rings(Some(phantom.pending())),
            vec![2],
            "the pending attach is counted before the host applies it",
        );
        let ring = indexed_ring(&leading);
        let cables = leading.edge_hops(&station, &ring, &curve, Some(&phantom));
        assert_eq!(
            wrap_orbits(&cables[0].hops),
            vec![0],
            "the dragged cable previews on the orbit the tie earns it",
        );
        assert_eq!(
            wrap_orbits(&cables[1].hops),
            vec![1],
            "the resident cable previews where the attach will push it",
        );
    }

    /// Two cables through one anchor take the ring the geometry asks for: the
    /// one that wraps LESS sits inside.
    ///
    /// Nested angular intervals are the case that matters, and the common one:
    /// two cables that enter an anchor from the same side and leave to the same
    /// side have one interval containing the other. Put the wider wrap on the
    /// inner ring and its legs have to cut across the narrower cable twice, for
    /// no reason - the same two cables nested the other way round do not cross
    /// at all. Push order knows nothing about that, so here it disagrees.
    #[test]
    fn nested_wraps_put_the_narrower_cable_inside() {
        // Both cables pass above the anchor, the second one closer, so the
        // first's angular interval contains the second's: 157 degrees against
        // 90, around the same centre.
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([-100.0, -20.0], 1, PinDirection::Output),
                1 => ([100.0, -20.0], 3, PinDirection::Input),
                2 => ([-100.0, -100.0], 1, PinDirection::Output),
                3 => ([100.0, -100.0], 3, PinDirection::Input),
                _ => return None,
            };
            Some(Station {
                point,
                side,
                direction: Some(direction),
            })
        };

        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(0.0, 0.0)));
        // The WIDER cable is pushed first, so push order asks for the crossing.
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([10]));

        let ring = indexed_ring(&graph);
        let cables = graph.edge_hops(&stations, &ring, &curve, None);
        assert_eq!(
            wrap_orbits(&cables[1].hops),
            vec![0],
            "the narrower wrap belongs on the inner ring",
        );
        assert_eq!(
            wrap_orbits(&cables[0].hops),
            vec![1],
            "the wider wrap goes around the narrower one, not through it",
        );
    }

    /// The centre of every wrap hop, in the order the cable meets them.
    fn wrap_centers(hops: &[edge_path::Hop]) -> Vec<[f32; 2]> {
        hops.iter()
            .filter_map(|hop| match hop {
                edge_path::Hop::Wrap { orbit } => Some(orbit.center),
                edge_path::Hop::Pin { .. } => None,
            })
            .collect()
    }

    fn pin_ref(node: usize) -> PinRef<AllUsize> {
        PinRef::new(node, 0)
    }

    /// Two anchors and an edge routed through both, wired to `station`'s pins.
    fn graph_with_anchors(positions: &[(usize, f32)]) -> Graph<'static> {
        let mut graph = Graph::default();
        for &(id, x) in positions {
            graph = graph.push_anchor(anchor(id, Point::new(x, 0.0)));
        }
        graph
    }

    /// The visiting order is geometry, not authoring order: the host may keep a
    /// route in any order at all, and the cable still passes its anchors in the
    /// order it reaches them.
    #[test]
    fn wraps_are_visited_in_projection_order() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0), (12, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 11, 12]));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(cables.len(), 1);
        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [200.0, 0.0], [300.0, 0.0]],
            "wraps should run along the pin-to-pin run, whatever order they were authored in"
        );
    }

    /// A cable arriving from the input side is turned round before the
    /// projection is taken, so the same scene reads the same either way.
    #[test]
    fn an_input_first_edge_is_oriented_before_ordering() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0)]);
        graph = graph.push_edge(edge(0, pin_ref(1), pin_ref(0)).route([10, 11]));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [300.0, 0.0]]
        );
        assert_eq!(
            (cables[0].ends.0.node_id, cables[0].ends.1.node_id),
            (0, 1),
            "the output pin heads the cable however the edge was authored"
        );
    }

    /// One orbit takes one edge: the edges through an anchor fill its orbits in
    /// push order, so no two cables share a ring.
    #[test]
    fn edges_through_one_anchor_take_successive_orbits() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));

        assert_eq!(graph.anchor_rings(None), vec![2]);

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        let orbits: Vec<u8> = cables
            .iter()
            .flat_map(|cable| cable.rings.iter().map(|(_, (_, orbit))| *orbit))
            .collect();
        assert_eq!(orbits, vec![0, 1]);
    }

    /// A route is a set of anchors, so a repeat is one wrap and an id naming
    /// nothing is no wrap at all. A host mid-edit must not be able to make the
    /// widget draw a cable through the same ring twice.
    #[test]
    fn a_route_dedupes_and_drops_unknown_ids() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 999, 10]));

        assert_eq!(graph.resolved_route(0), vec![0]);
        assert_eq!(graph.anchor_rings(None), vec![1]);
    }

    /// A node id names a node, not an anchor, even though the two share one id
    /// space - so routing through a node id draws nothing rather than wrapping
    /// the node.
    #[test]
    fn a_node_id_in_a_route_resolves_to_no_wrap() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        assert_eq!(graph.anchor_index(&10), Some(0));
        assert_eq!(graph.anchor_index(&0), None);
        assert_eq!(graph.resolved_route(0), vec![0]);
    }

    /// The anchor a detach was just published for stays reachable, so a drag can
    /// put it back where it came from before the host has caught up.
    #[test]
    fn a_detached_anchor_stays_snap_eligible() {
        let mut graph = graph_with_anchors(&[(10, 100.0), (11, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        assert_eq!(graph.route_snap_eligible(0, None), vec![1]);
        assert_eq!(graph.route_snap_eligible(0, Some(0)), vec![0, 1]);
    }

    /// An unrouted edge is the plain two-station cable, which is what keeps a
    /// graph with no anchors drawing exactly as it did without the feature.
    #[test]
    fn an_unrouted_edge_lowers_to_two_pins() {
        let mut graph = Graph::default();
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(cables[0].hops.len(), 2);
        assert!(wrap_centers(&cables[0].hops).is_empty());
        assert!(cables[0].rings.is_empty());
    }

    /// A phantom wrap is ordered by the same projection as a real one, so the
    /// preview runs where the committed cable will.
    #[test]
    fn a_phantom_wrap_is_ordered_like_a_real_one() {
        let mut graph = graph_with_anchors(&[(10, 300.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        let ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::At {
                center: [100.0, 0.0],
                radius: 16.0,
            },
        };
        let cables = graph.edge_hops(&station, &ring, &curve, Some(&phantom));

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [300.0, 0.0]]
        );
        assert_eq!(
            cables[0].rings,
            vec![(2, (0, 0))],
            "only the host's own wrap names an anchor; the phantom names none"
        );
    }

    /// An anchor a detach was published for leaves the preview at once, so the
    /// cable does not snap back to it for the frame the host takes to apply.
    #[test]
    fn an_excluded_anchor_leaves_the_preview() {
        let mut graph = graph_with_anchors(&[(10, 300.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        let ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: Some(0),
            kind: PhantomKind::At {
                center: [100.0, 0.0],
                radius: 16.0,
            },
        };
        let cables = graph.edge_hops(&station, &ring, &curve, Some(&phantom));

        assert_eq!(wrap_centers(&cables[0].hops), vec![[100.0, 0.0]]);
        assert!(cables[0].rings.is_empty());
    }

    /// A snap phantom stands for a real anchor at a real orbit, so it names one
    /// where a ring held at the cursor does not. It stands down once the host's
    /// route carries the anchor itself and the committed wrap takes over.
    #[test]
    fn a_snap_phantom_stands_down_once_the_route_carries_it() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)));

        let offered_ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        let previewed = graph.edge_hops(&station, &offered_ring, &curve, Some(&phantom));
        assert_eq!(wrap_centers(&previewed[0].hops), vec![[200.0, 0.0]]);
        assert_eq!(
            previewed[0].rings,
            vec![(1, (0, 0))],
            "an offered ring still names the anchor it is offered by"
        );

        let mut applied = graph_with_anchors(&[(10, 200.0)]);
        applied = applied.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        let applied_ring = ring(&applied);
        let cables = applied.edge_hops(&station, &applied_ring, &curve, Some(&phantom));
        assert_eq!(wrap_centers(&cables[0].hops), vec![[200.0, 0.0]]);
        assert_eq!(cables[0].rings, vec![(1, (0, 0))]);
    }

    /// A run of no length leaves nothing to project onto, so the authored order
    /// is all there is to go on.
    #[test]
    fn a_degenerate_run_keeps_the_authored_order() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0)]);
        graph = graph.push_edge(edge(0, pin_ref(2), pin_ref(2)).route([10, 11]));

        let collapsed = |_: &PinRef<AllUsize>| {
            Some(Station {
                point: [50.0, 50.0],
                side: 1,
                direction: Some(PinDirection::Output),
            })
        };
        let ring = ring(&graph);
        let cables = graph.edge_hops(&collapsed, &ring, &curve, None);

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[300.0, 0.0], [100.0, 0.0]]
        );
    }

    /// An endpoint the frame cannot resolve drops the whole cable: half a cable
    /// is not a thing to draw.
    #[test]
    fn an_unresolvable_endpoint_drops_the_edge() {
        let mut graph = Graph::default();
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(7)));

        let ring = ring(&graph);
        assert!(graph.edge_hops(&station, &ring, &curve, None).is_empty());
    }

    /// A real corridor is cleared: two cables that fly the stretch between two
    /// anchors come out not crossing along it, where containment alone crosses.
    ///
    /// The scene is the shape the styling demo shows. Both cables wrap both
    /// anchors, and both turn one way at the first and the other at the second -
    /// an S through the corridor, so each rides the CROSSED tangent between the
    /// rings. That is the case where matching ring order is the arrangement that
    /// crosses, and the assignment cannot know it without building: the wrap
    /// direction is chosen by the geometry from the radii. So this asserts on
    /// crossings counted off the built paths, which is also what the search
    /// itself minimises.
    ///
    /// Both halves matter. Clearing the corridor is the feature; containment
    /// crossing is what proves the scene is a genuine conflict rather than one
    /// the seed already happened to solve.
    #[test]
    fn a_crossed_tangent_corridor_comes_out_clear() {
        const A: [f32; 2] = [300.0, 300.0];
        const B: [f32; 2] = [550.0, 300.0];
        // Two sources left of the first anchor, two sinks right of the second,
        // at different heights so the pair has a reason to nest either way.
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([260.0, 193.0], 1, PinDirection::Output),
                1 => ([600.0, 463.0], 3, PinDirection::Input),
                2 => ([260.0, 215.0], 1, PinDirection::Output),
                3 => ([600.0, 323.0], 3, PinDirection::Input),
                _ => return None,
            };
            Some(Station {
                point,
                side,
                direction: Some(direction),
            })
        };

        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(A[0], A[1])));
        graph = graph.push_anchor(anchor(11, Point::new(B[0], B[1])));
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 11]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([10, 11]));

        // The shipped radii, so the rings sit where a host sees them.
        let rings = |anchor: usize, orbit: u8| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING,
            })
        };

        // Crossings along the corridor for one arrangement, measured the way the
        // search measures them.
        let corridor_crossings = |arrangement: &[Vec<u8>]| {
            let cables = graph.edge_hops(&stations, &rings, &curve, None);
            let paths: Vec<edge_path::EdgePath> = cables
                .iter()
                .enumerate()
                .map(|(slot, cable)| {
                    let mut hops = cable.hops.clone();
                    for (&(hop, (_, _)), &orbit) in cable.rings.iter().zip(&arrangement[slot]) {
                        if let edge_path::Hop::Wrap { orbit: circle } = &mut hops[hop] {
                            circle.radius =
                                DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING;
                        }
                    }
                    edge_path::build(&hops, &EdgeCurve::default()).path
                })
                .collect();
            let band = |anchor: usize| rings(anchor, 1).expect("two cables per anchor");
            let bands = [edge_path::Corridor {
                from: band(0),
                to: band(1),
            }];
            edge_path::crossings_between(&paths[0], &paths[1], &bands)
        };

        let chosen: Vec<Vec<u8>> = graph
            .edge_hops(&stations, &rings, &curve, None)
            .iter()
            .map(|cable| cable.rings.iter().map(|&(_, (_, orbit))| orbit).collect())
            .collect();
        assert_eq!(
            corridor_crossings(&chosen),
            0,
            "the corridor still crosses at {chosen:?}",
        );
        assert!(
            corridor_crossings(&[vec![0, 0], vec![1, 1]]) > 0,
            "matching ring order does not cross here, so the scene proves nothing",
        );
    }

    /// The styling demo's own scene comes out clear, and needs more than the
    /// obvious exchange to get there.
    ///
    /// Three cables on each anchor: one pair flies the corridor between them and
    /// a third cable wraps each end on its own way elsewhere. Containment seats
    /// the pair to disagree across the corridor, so it crosses. What makes this
    /// worth a test of its own is that the seed is a PLATEAU: exchanging the two
    /// adjacent rings the pair sits on leaves the count at one, and only
    /// exchanging a non-adjacent pair - moving the uninvolved third cable out of
    /// the way - reaches a clear corridor. A search confined to adjacent rings
    /// stalls here with the crossing still on screen.
    #[test]
    fn a_three_cable_corridor_comes_out_clear() {
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([260.0, 193.3], 1, PinDirection::Output),
                1 => ([350.0, 243.3], 3, PinDirection::Input),
                2 => ([510.0, 243.3], 1, PinDirection::Output),
                3 => ([600.0, 193.3], 3, PinDirection::Input),
                4 => ([600.0, 463.3], 3, PinDirection::Input),
                5 => ([280.0, 103.3], 1, PinDirection::Output),
                6 => ([600.0, 323.3], 3, PinDirection::Input),
                _ => return None,
            };
            Some(Station {
                point,
                side,
                direction: Some(direction),
            })
        };
        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(300.0, 300.0)));
        graph = graph.push_anchor(anchor(11, Point::new(550.0, 300.0)));
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([11]));
        graph = graph.push_edge(edge(2, pin_ref(0), pin_ref(4)).route([10, 11]));
        graph = graph.push_edge(edge(3, pin_ref(5), pin_ref(6)).route([10, 11]));

        let rings = |anchor: usize, orbit: u8| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING,
            })
        };
        let corridor_crossings =
            |arrangement: &[Vec<u8>]| corridor_count(&graph, &stations, &rings, arrangement);

        let chosen: Vec<Vec<u8>> = graph
            .edge_hops(&stations, &rings, &curve, None)
            .iter()
            .map(|cable| cable.rings.iter().map(|&(_, (_, orbit))| orbit).collect())
            .collect();
        assert_eq!(
            corridor_crossings(&chosen),
            0,
            "the corridor still crosses at {chosen:?}",
        );
        // Containment: at the first anchor the pair sits 2 and 1, at the second 1
        // and 2, so it disagrees across the corridor.
        let containment = vec![vec![0], vec![0], vec![2, 1], vec![1, 2]];
        assert!(
            corridor_crossings(&containment) > 0,
            "containment does not cross here, so the scene proves nothing",
        );
        // Exchanging only the pair's own two rings stays on the plateau.
        assert!(
            corridor_crossings(&[vec![0], vec![0], vec![1, 1], vec![2, 2]]) > 0,
            "the adjacent exchange already clears this, so the scene does not \
             need the wider neighbourhood it exists to justify",
        );
    }

    /// Crossings along a corridor for one arrangement, counted the way the
    /// assignment counts them.
    fn corridor_count(
        graph: &Graph<'_>,
        stations: &dyn Fn(&PinRef<AllUsize>) -> Option<Station>,
        rings: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
        arrangement: &[Vec<u8>],
    ) -> usize {
        let cables = graph.edge_hops(stations, rings, &curve, None);
        let paths: Vec<edge_path::EdgePath> = cables
            .iter()
            .enumerate()
            .map(|(slot, cable)| {
                let mut hops = cable.hops.clone();
                for (&(hop, _), &orbit) in cable.rings.iter().zip(&arrangement[slot]) {
                    if let edge_path::Hop::Wrap { orbit: circle } = &mut hops[hop] {
                        circle.radius = DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING;
                    }
                }
                edge_path::build(&hops, &EdgeCurve::default()).path
            })
            .collect();
        let rings_at: Vec<u8> = (0..graph.anchors.len())
            .map(|anchor| {
                u8::try_from(
                    (0..graph.edges.len())
                        .filter(|&edge| graph.resolved_route(edge).contains(&anchor))
                        .count(),
                )
                .unwrap_or(u8::MAX)
            })
            .collect();
        let mut count = 0;
        for (i, first) in paths.iter().enumerate() {
            for (j, second) in paths.iter().enumerate().skip(i + 1) {
                let mut shared: Vec<usize> = graph
                    .resolved_route(i)
                    .into_iter()
                    .filter(|a| graph.resolved_route(j).contains(a))
                    .collect();
                shared.sort_unstable();
                if shared.len() < 2 {
                    continue;
                }
                let reach = |anchor: usize| {
                    rings(
                        anchor,
                        rings_at.get(anchor).copied().unwrap_or(1).saturating_sub(1),
                    )
                };
                let mut bands = Vec::new();
                for (at, &from) in shared.iter().enumerate() {
                    for &to in &shared[at + 1..] {
                        if let (Some(from), Some(to)) = (reach(from), reach(to)) {
                            bands.push(edge_path::Corridor { from, to });
                        }
                    }
                }
                count += edge_path::crossings_between(first, second, &bands);
            }
        }
        count
    }
}
