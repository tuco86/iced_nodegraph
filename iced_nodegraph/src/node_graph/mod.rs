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
//!     .on_move(|node_id, pos| Message::NodeMoved { node_id, pos });
//!
//! ng.push_node(node(0, Point::new(100.0, 100.0), my_node_content));
//! ng.push_edge(edge(PinRef::new(0, 0), PinRef::new(1, 0)));
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
//! `on_move()`, `on_select()`, `on_clone()`, `on_delete()`, `on_group_move()`.
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

use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

use iced::{Length, Point, Size, Vector};

use crate::ids::{IdMap, NodeId, PinId};
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

/// An edge to push onto the graph: endpoint pin references and an optional
/// per-edge status-driven style closure. Build with [`edge`] + [`Edge::style`],
/// then add via [`NodeGraph::push_edge`].
pub struct Edge<'a, N, P, UI, Theme> {
    from: PinRef<N, P>,
    to: PinRef<N, P>,
    style_fn: Option<EdgeStyleFn<'a, P, UI, Theme>>,
}

/// Creates an [`Edge`] with default (theme) styling.
pub fn edge<'a, N, P, UI, Theme>(
    from: PinRef<N, P>,
    to: PinRef<N, P>,
) -> Edge<'a, N, P, UI, Theme> {
    Edge {
        from,
        to,
        style_fn: None,
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
        self.style_fn = Some(Box::new(f));
        self
    }
}

pub mod camera;
pub(crate) mod euclid;
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

/// Per-layer SDF tile debug toggles.
#[derive(Debug, Clone, Copy, Default)]
pub struct SdfDebug {
    pub edges: bool,
    pub shadows: bool,
    pub node_fill: bool,
    pub node_foreground: bool,
}

/// Identifies what an in-progress drag is moving. Delivered to the
/// [`on_drag_start`](NodeGraph::on_drag_start) callback so the app can observe a
/// drag live (e.g. to broadcast it), alongside the commit-on-drop callbacks.
#[derive(Debug, Clone)]
pub enum DragInfo {
    /// Dragging a single node
    Node { node_id: usize },
    /// Dragging a group of selected nodes
    Group { node_ids: Vec<usize> },
    /// Dragging an edge from a pin
    Edge { from_node: usize, from_pin: usize },
    /// Box selection drag
    BoxSelect { start_x: f32, start_y: f32 },
}

/// Generic pin reference with user-defined ID types.
///
/// This is the generic version of [`PinReference`] that uses your own ID types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinRef<N, P> {
    pub node_id: N,
    pub pin_id: P,
}

impl<N: Clone, P: Clone> PinRef<N, P> {
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
/// Users can provide their own ID types by implementing [`NodeId`] and [`PinId`].
#[allow(missing_debug_implementations)]
pub struct NodeGraph<
    'a,
    N = usize,
    P = usize,
    UI = (),
    Message = (),
    Theme = iced::Theme,
    Renderer = iced::Renderer,
> where
    N: NodeId,
    P: PinId,
{
    pub(super) size: Size<Length>,
    /// Nodes with position, element, and config overrides.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `default_node_style()` values at render time.
    pub(super) nodes: Vec<(
        Point,
        iced::Element<'a, Message, Theme, Renderer>,
        Option<NodeStyleFn<'a, Theme>>,
        Option<PinStyleFn<'a, P, UI, Theme>>,
    )>,
    /// Edges with user-defined pin references and config overrides.
    /// Pin IDs are resolved to local indices at render time.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `default_edge_style()` values at render time.
    pub(super) edges: Vec<(
        PinRef<N, P>,
        PinRef<N, P>,
        Option<EdgeStyleFn<'a, P, UI, Theme>>,
    )>,
    /// Maps user node IDs to internal indices for translation at render/event time.
    pub(super) node_ids: IdMap<N>,
    graph_style: Option<GraphStyle>,
    on_connect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(N, Point) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_clone: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_delete: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_group_move: Option<Box<dyn Fn(Vec<N>, Vector) -> Message + 'a>>,
    /// External selection using internal indices.
    /// Populated by `selection()` method which converts user IDs to indices.
    external_selection: Option<HashSet<usize>>,
    // Live drag callbacks: fire continuously during a drag (start/update/end),
    // in addition to the commit-on-drop on_move/on_group_move. They make live
    // observation of an in-progress drag possible (e.g. collaborative broadcast),
    // which is the app's concern, not the widget's.
    on_drag_start: Option<Box<dyn Fn(DragInfo) -> Message + 'a>>,
    on_drag_update: Option<Box<dyn Fn(f32, f32) -> Message + 'a>>,
    on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    /// Callback for camera state changes (position, zoom).
    /// Used for tracking viewport state in application for features like spawn-at-center.
    on_camera_change: Option<Box<dyn Fn(Point, f32) -> Message + 'a>>,
    /// Style callback for box selection overlay.
    /// Returns (fill_color, border_color).
    pub(super) box_select_style_fn: Option<Box<dyn Fn(&Theme) -> (iced::Color, iced::Color) + 'a>>,
    /// Style callback for edge cutting tool overlay.
    /// Returns the line color.
    pub(super) cutting_tool_style_fn: Option<Box<dyn Fn(&Theme) -> iced::Color + 'a>>,
    /// Style for the edge being dragged (theme -> resolved style). The graph
    /// injects the source pin's color for inheriting (TRANSPARENT) stroke ends.
    pub(super) dragging_edge_style_fn: Option<DragEdgeStyleFn<'a, P, UI, Theme>>,
    /// Initial camera position and zoom to restore on first render.
    /// Applied once when the widget state is created, then controlled by user interaction.
    pub(super) initial_camera: Option<(Point, f32)>,
    /// Custom validation callback for pin connection compatibility.
    /// When set, it is authoritative in `compute_valid_targets` (the built-in
    /// direction check only applies as the default when this is unset).
    pub(super) can_connect:
        Option<Box<dyn Fn(PinEnd<'_, N, P, UI>, PinEnd<'_, N, P, UI>) -> bool + 'a>>,
    /// Per-layer SDF tile debug visualization.
    pub(super) sdf_debug: SdfDebug,
}

impl<N, P, UI, Message, Theme, Renderer> Default
    for NodeGraph<'_, N, P, UI, Message, Theme, Renderer>
where
    N: NodeId,
    P: PinId,
    Renderer: iced_widget::core::renderer::Renderer,
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            nodes: Vec::new(),
            edges: Vec::new(),
            node_ids: IdMap::new(),
            graph_style: None,
            on_connect: None,
            on_disconnect: None,
            on_move: None,
            on_select: None,
            on_clone: None,
            on_delete: None,
            on_group_move: None,
            external_selection: None,
            on_drag_start: None,
            on_drag_update: None,
            on_drag_end: None,
            on_camera_change: None,
            box_select_style_fn: None,
            cutting_tool_style_fn: None,
            dragging_edge_style_fn: None,
            initial_camera: None,
            can_connect: None,
            sdf_debug: SdfDebug::default(),
        }
    }
}

impl<'a, N, P, UI, Message, Theme, Renderer> NodeGraph<'a, N, P, UI, Message, Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    Renderer: iced_widget::core::renderer::Renderer,
{
    /// Sets the initial camera position and zoom level.
    ///
    /// This is used to restore camera state from persistence.
    /// Applied once when the widget state is created, then controlled by user interaction.
    pub fn initial_camera(mut self, position: Point, zoom: f32) -> Self {
        self.initial_camera = Some((position, zoom));
        self
    }

    /// Adds a node with the given ID and default styling.
    ///
    /// The node will use theme defaults from `default_node_style()`.
    pub fn push_node(&mut self, node: Node<'a, N, P, UI, Message, Theme, Renderer>) {
        self.node_ids.register(node.id);
        self.nodes.push((
            node.position,
            node.element,
            node.style_fn,
            node.pin_style_fn,
        ));
    }

    /// Adds an edge to the graph.
    ///
    /// Pin IDs are resolved to local indices at render time; the widget
    /// normalizes orientation so the output pin is the edge start (output ->
    /// input).
    pub fn push_edge(&mut self, edge: Edge<'a, N, P, UI, Theme>) {
        self.edges.push((edge.from, edge.to, edge.style_fn));
    }

    /// Translates internal node index to user's node ID.
    pub(super) fn index_to_node_id(&self, index: usize) -> Option<N> {
        self.node_ids.id(index).cloned()
    }

    pub fn graph_style(mut self, style: GraphStyle) -> Self {
        self.graph_style = Some(style);
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

    /// Sets a style callback for the edge cutting tool overlay.
    ///
    /// The callback receives the theme and returns the line color.
    ///
    /// # Example
    /// ```ignore
    /// node_graph()
    ///     .cutting_tool_style(|theme| Color::from_rgb(1.0, 0.3, 0.3))
    /// ```
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

    pub fn cutting_tool_style(mut self, f: impl Fn(&Theme) -> iced::Color + 'a) -> Self {
        self.cutting_tool_style_fn = Some(Box::new(f));
        self
    }

    /// Sets a validation callback for pin connection compatibility.
    ///
    /// When set, this callback is authoritative: it receives both endpoints as
    /// [`PinEnd`] views (node id, pin id, direction, user info) and returns
    /// `true` if they can connect. No implicit direction filtering is applied;
    /// inspect [`PinEnd::direction`] yourself if you need it.
    ///
    /// When not set, the built-in direction check applies (Output<->Input,
    /// `Both` connects to anything).
    pub fn can_connect(
        mut self,
        f: impl Fn(PinEnd<'_, N, P, UI>, PinEnd<'_, N, P, UI>) -> bool + 'a,
    ) -> Self {
        self.can_connect = Some(Box::new(f));
        self
    }

    /// Enables SDF tile debug visualization per primitive layer.
    pub fn sdf_debug(mut self, debug: SdfDebug) -> Self {
        self.sdf_debug = debug;
        self
    }

    /// Sets a callback for when an edge is connected between two pins.
    pub fn on_connect(mut self, f: impl Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when an edge is disconnected between two pins.
    pub fn on_disconnect(mut self, f: impl Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when a node is moved to a new position.
    ///
    /// The callback receives the node ID and its new position in world coordinates.
    pub fn on_move(mut self, f: impl Fn(N, Point) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the selection changes.
    ///
    /// The callback receives the list of currently selected node IDs.
    /// Fires on click-select, box-select, and Shift+click multi-select.
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

    /// Sets a callback for when multiple selected nodes are moved together.
    ///
    /// The callback receives the list of moved node IDs and the movement delta vector.
    /// This fires instead of individual `on_move` callbacks when dragging a selection.
    pub fn on_group_move(mut self, f: impl Fn(Vec<N>, Vector) -> Message + 'a) -> Self {
        self.on_group_move = Some(Box::new(f));
        self
    }

    /// Sets a callback for when a drag operation starts.
    /// Used for real-time collaboration to broadcast drag state to other users.
    pub fn on_drag_start(mut self, f: impl Fn(DragInfo) -> Message + 'a) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }

    /// Sets a callback for drag position updates.
    /// Called frequently during drag operations with current cursor position (world coordinates).
    pub fn on_drag_update(mut self, f: impl Fn(f32, f32) -> Message + 'a) -> Self {
        self.on_drag_update = Some(Box::new(f));
        self
    }

    /// Sets a callback for when a drag operation ends.
    pub fn on_drag_end(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_drag_end = Some(Box::new(f));
        self
    }

    /// Sets a callback for when the camera state changes (pan or zoom).
    ///
    /// The callback receives the current camera position and zoom level.
    /// Useful for tracking viewport state for features like spawn-at-screen-center.
    pub fn on_camera_change(mut self, f: impl Fn(Point, f32) -> Message + 'a) -> Self {
        self.on_camera_change = Some(Box::new(f));
        self
    }

    /// Sets the external selection using user node IDs.
    ///
    /// The IDs are converted to internal indices. Unknown IDs are ignored.
    /// This allows controlling which nodes are selected from outside the widget.
    pub fn selection<'b>(mut self, selection: impl IntoIterator<Item = &'b N>) -> Self
    where
        N: 'b,
    {
        let indices: HashSet<usize> = selection
            .into_iter()
            .filter_map(|id| self.node_ids.index(id))
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
        self.nodes.iter().map(|(p, e, _, _)| (*p, e))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point, &mut iced::Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter_mut().map(|(p, e, _, _)| (*p, e))
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
    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(N, Point) -> Message + 'a>> {
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
    pub(super) fn on_group_move_handler(
        &self,
    ) -> Option<&Box<dyn Fn(Vec<N>, Vector) -> Message + 'a>> {
        self.on_group_move.as_ref()
    }
    pub(super) fn on_drag_start_handler(&self) -> Option<&Box<dyn Fn(DragInfo) -> Message + 'a>> {
        self.on_drag_start.as_ref()
    }
    pub(super) fn on_drag_update_handler(&self) -> Option<&Box<dyn Fn(f32, f32) -> Message + 'a>> {
        self.on_drag_update.as_ref()
    }
    pub(super) fn on_drag_end_handler(&self) -> Option<&Box<dyn Fn() -> Message + 'a>> {
        self.on_drag_end.as_ref()
    }
    pub(super) fn get_external_selection(&self) -> Option<&HashSet<usize>> {
        self.external_selection.as_ref()
    }

    pub(super) fn on_camera_change_handler(
        &self,
    ) -> Option<&Box<dyn Fn(Point, f32) -> Message + 'a>> {
        self.on_camera_change.as_ref()
    }

    /// Translates a list of internal node indices to user IDs.
    /// Returns empty vec if any translation fails.
    pub(super) fn translate_node_ids(&self, indices: &[usize]) -> Vec<N> {
        indices
            .iter()
            .filter_map(|&idx| self.node_ids.id(idx).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::PinReference;

    #[test]
    fn test_pin_reference_equality() {
        let a = PinReference::new(1, 2);
        let b = PinReference::new(1, 2);
        let c = PinReference::new(1, 3);
        let d = PinReference::new(2, 2);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn test_pin_reference_copy() {
        let a = PinReference::new(5, 10);
        let b = a; // Copy
        assert_eq!(a.node_id, b.node_id);
        assert_eq!(a.pin_id, b.pin_id);
    }

    #[test]
    fn test_pin_reference_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(PinReference::new(0, 0));
        set.insert(PinReference::new(0, 1));
        set.insert(PinReference::new(1, 0));
        set.insert(PinReference::new(0, 0)); // duplicate

        assert_eq!(set.len(), 3);
        assert!(set.contains(&PinReference::new(0, 0)));
        assert!(set.contains(&PinReference::new(0, 1)));
        assert!(set.contains(&PinReference::new(1, 0)));
    }
}
