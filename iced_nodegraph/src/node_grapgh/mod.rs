use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use iced::{Color, Length, Point, Size, Vector};

use crate::ids::{EdgeId, IdMaps, NodeId, PinId};
use crate::node_pin::PinReference;
use crate::style::{EdgeConfig, GraphStyle, NodeConfig, PinConfig};

pub mod camera;
pub(crate) mod effects;
pub(crate) mod euclid;
pub(crate) mod state;
pub(crate) mod widget;

#[cfg(test)]
mod interaction_tests;

/// Information about a drag operation, used for real-time collaboration.
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

/// State of a remote user for collaborative editing.
#[derive(Debug, Clone)]
pub struct RemoteUserState {
    /// Display nickname
    pub nickname: String,
    /// User's assigned color
    pub color: Color,
    /// Current cursor position in world space (None if not visible)
    pub cursor: Option<Point>,
    /// Node IDs this user has selected
    pub selected_nodes: Vec<usize>,
    /// Current drag operation (if any)
    pub drag: Option<RemoteDrag>,
}

/// Remote user's current drag operation.
#[derive(Debug, Clone)]
pub enum RemoteDrag {
    /// Dragging a single node
    Node { node_id: usize, current: Point },
    /// Dragging a group of nodes
    Group { node_ids: Vec<usize>, delta: Vector },
    /// Dragging an edge from a pin
    Edge {
        from_node: usize,
        from_pin: usize,
        current: Point,
    },
    /// Box selection in progress
    BoxSelect { start: Point, current: Point },
}

/// Events emitted by the NodeGraph widget.
#[derive(Debug, Clone)]
pub enum NodeGraphEvent {
    EdgeConnected {
        from: PinReference,
        to: PinReference,
    },
    EdgeDisconnected {
        from: PinReference,
        to: PinReference,
    },
    NodeMoved {
        node_id: usize,
        position: Point,
    },
    GroupMoved {
        node_ids: Vec<usize>,
        delta: Vector,
    },
    SelectionChanged {
        selected: Vec<usize>,
    },
    CloneRequested {
        node_ids: Vec<usize>,
    },
    DeleteRequested {
        node_ids: Vec<usize>,
    },
}

/// Generic message enum for graph interactions with user-defined ID types.
///
/// This is the generic version of [`NodeGraphEvent`] that uses your own ID types
/// instead of `usize`. Wrap this in your application's message enum:
///
/// ```rust,ignore
/// use iced_nodegraph::{NodeGraphMessage, PinRef};
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum MyNodeId { Input, Process, Output }
/// impl iced_nodegraph::NodeId for MyNodeId {}
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum MyPinId { DataIn, DataOut }
/// impl iced_nodegraph::PinId for MyPinId {}
///
/// #[derive(Clone, Debug)]
/// enum AppMessage {
///     Graph(NodeGraphMessage<MyNodeId, MyPinId>),
///     // ... other messages
/// }
/// ```
#[derive(Debug, Clone)]
pub enum NodeGraphMessage<N = usize, P = usize, E = usize>
where
    N: NodeId,
    P: PinId,
    E: EdgeId,
{
    /// An edge was connected between two pins.
    EdgeConnected {
        edge_id: E,
        from: PinRef<N, P>,
        to: PinRef<N, P>,
    },
    /// An edge was disconnected.
    EdgeDisconnected {
        edge_id: E,
        from: PinRef<N, P>,
        to: PinRef<N, P>,
    },
    /// A node was moved to a new position.
    NodeMoved { node_id: N, position: Point },
    /// Multiple nodes were moved together.
    GroupMoved { node_ids: Vec<N>, delta: Vector },
    /// The selection changed.
    SelectionChanged { selected: Vec<N> },
    /// User requested to clone selected nodes.
    CloneRequested { node_ids: Vec<N> },
    /// User requested to delete selected nodes.
    DeleteRequested { node_ids: Vec<N> },
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
/// - `E`: Edge ID type (defaults to `usize`)
/// - `Message`: Application message type
/// - `Theme`: Iced theme type (defaults to `iced::Theme`)
/// - `Renderer`: Iced renderer type (defaults to `iced::Renderer`)
///
/// Users can provide their own ID types by implementing [`NodeId`], [`PinId`], [`EdgeId`].
#[allow(missing_debug_implementations)]
pub struct NodeGraph<
    'a,
    N = usize,
    P = usize,
    E = usize,
    Message = (),
    Theme = iced::Theme,
    Renderer = iced::Renderer,
> where
    N: NodeId,
    P: PinId,
    E: EdgeId,
{
    pub(super) size: Size<Length>,
    /// Nodes with position, element, and config overrides.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `NodeStyle::from_theme()` values at render time.
    pub(super) nodes: Vec<(
        Point,
        iced::Element<'a, Message, Theme, Renderer>,
        NodeConfig,
    )>,
    /// Edges with user-defined pin references and config overrides.
    /// Pin IDs are resolved to local indices at render time.
    /// Config fields set to Some() override theme defaults.
    /// None fields use `EdgeStyle::from_theme()` values at render time.
    pub(super) edges: Vec<(PinRef<N, P>, PinRef<N, P>, EdgeConfig)>,
    /// Bidirectional maps for ID translation.
    pub(super) id_maps: IdMaps<N, P, E>,
    graph_style: Option<GraphStyle>,
    on_connect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(PinRef<N, P>, PinRef<N, P>) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(N, Point) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_clone: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_delete: Option<Box<dyn Fn(Vec<N>) -> Message + 'a>>,
    on_group_move: Option<Box<dyn Fn(Vec<N>, Vector) -> Message + 'a>>,
    external_selection: Option<&'a HashSet<usize>>,
    // Drag event callbacks for real-time collaboration
    on_drag_start: Option<Box<dyn Fn(DragInfo) -> Message + 'a>>,
    on_drag_update: Option<Box<dyn Fn(f32, f32) -> Message + 'a>>,
    on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    // Remote users for collaborative rendering
    remote_users: Option<&'a [RemoteUserState]>,
    /// Unified event callback for all graph interactions.
    /// Alternative to individual callbacks (on_connect, on_move, etc.)
    on_event: Option<Box<dyn Fn(NodeGraphMessage<N, P, E>) -> Message + 'a>>,
    /// Callback for camera state changes (position, zoom).
    /// Used for tracking viewport state in application for features like spawn-at-center.
    on_camera_change: Option<Box<dyn Fn(Point, f32) -> Message + 'a>>,
    /// Global pin style overrides applied to all pins.
    /// Individual pin colors from widgets take precedence over this.
    pub(super) pin_defaults: Option<PinConfig>,
    /// Phantom data for unused type parameter (E is only used in callbacks)
    _phantom: PhantomData<E>,
}

impl<N, P, E, Message, Theme, Renderer> Default for NodeGraph<'_, N, P, E, Message, Theme, Renderer>
where
    N: NodeId,
    P: PinId,
    E: EdgeId,
    Renderer: iced_widget::core::renderer::Renderer,
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            nodes: Vec::new(),
            edges: Vec::new(),
            id_maps: IdMaps::new(),
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
            remote_users: None,
            on_event: None,
            on_camera_change: None,
            pin_defaults: None,
            _phantom: PhantomData,
        }
    }
}

impl<'a, N, P, E, Message, Theme, Renderer> NodeGraph<'a, N, P, E, Message, Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    Renderer: iced_widget::core::renderer::Renderer,
{
    /// Adds a node with the given ID and default styling.
    ///
    /// The node will use theme defaults from `NodeStyle::from_theme()`.
    pub fn push_node(
        &mut self,
        node_id: N,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
    ) {
        self.id_maps.register_node(node_id);
        self.nodes
            .push((position, element.into(), NodeConfig::default()));
    }

    /// Adds a node with specific style overrides.
    ///
    /// Only the properties set in `config` will override theme defaults.
    /// Unset (None) properties will use `NodeStyle::from_theme()` values.
    ///
    /// Use `NodeConfig::merge()` to combine multiple configs for inheritance.
    pub fn push_node_styled(
        &mut self,
        node_id: N,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
        config: NodeConfig,
    ) {
        self.id_maps.register_node(node_id);
        self.nodes.push((position, element.into(), config));
    }

    /// Adds an edge with default styling.
    ///
    /// The edge will use theme defaults from `EdgeStyle::from_theme()`.
    /// Pin IDs are resolved to local indices at render time.
    pub fn push_edge(&mut self, from: PinRef<N, P>, to: PinRef<N, P>) {
        self.edges.push((from, to, EdgeConfig::default()));
    }

    /// Adds an edge with specific style overrides.
    ///
    /// Only the properties set in `config` will override theme defaults.
    /// Unset (None) properties will use `EdgeStyle::from_theme()` values.
    /// Pin IDs are resolved to local indices at render time.
    pub fn push_edge_styled(&mut self, from: PinRef<N, P>, to: PinRef<N, P>, config: EdgeConfig) {
        self.edges.push((from, to, config));
    }

    /// Translates internal node index to user's node ID.
    pub(super) fn index_to_node_id(&self, index: usize) -> Option<N> {
        self.id_maps.node_id(index).cloned()
    }

    pub fn graph_style(mut self, style: GraphStyle) -> Self {
        self.graph_style = Some(style);
        self
    }

    /// Sets global pin style defaults.
    /// These override theme defaults but individual pin colors from widgets still take precedence.
    pub fn pin_defaults(mut self, config: PinConfig) -> Self {
        self.pin_defaults = Some(config);
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

    pub fn on_move(mut self, f: impl Fn(N, Point) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    pub fn on_clone(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_clone = Some(Box::new(f));
        self
    }

    pub fn on_delete(mut self, f: impl Fn(Vec<N>) -> Message + 'a) -> Self {
        self.on_delete = Some(Box::new(f));
        self
    }

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

    /// Sets remote user states for collaborative rendering.
    /// Remote users' cursors, selections, and drags will be rendered on the canvas.
    pub fn remote_users(mut self, users: &'a [RemoteUserState]) -> Self {
        self.remote_users = Some(users);
        self
    }

    /// Sets a unified event callback for all graph interactions.
    ///
    /// This is an alternative to using individual callbacks (on_connect, on_move, etc.).
    /// When set, this callback fires for all graph events, allowing centralized event handling.
    pub fn on_event(mut self, f: impl Fn(NodeGraphMessage<N, P, E>) -> Message + 'a) -> Self {
        self.on_event = Some(Box::new(f));
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

    pub fn selection(mut self, selection: &'a HashSet<usize>) -> Self {
        self.external_selection = Some(selection);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.size.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.size.height = height.into();
        self
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns an iterator over all edges with their configs.
    pub fn edges(&self) -> impl Iterator<Item = (&PinRef<N, P>, &PinRef<N, P>, &EdgeConfig)> {
        self.edges
            .iter()
            .map(|(from, to, config)| (from, to, config))
    }

    /// Returns the position of a node by its user ID.
    pub fn node_position(&self, node_id: &N) -> Option<Point> {
        let idx = self.id_maps.node_index(node_id)?;
        self.nodes.get(idx).map(|(pos, _, _)| *pos)
    }

    /// Returns the position of a node by its internal index.
    pub fn node_position_by_index(&self, index: usize) -> Option<Point> {
        self.nodes.get(index).map(|(pos, _, _)| *pos)
    }

    pub(super) fn elements_iter(
        &self,
    ) -> impl Iterator<
        Item = (
            Point,
            &iced::Element<'a, Message, Theme, Renderer>,
            &NodeConfig,
        ),
    > {
        self.nodes.iter().map(|(p, e, c)| (*p, e, c))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<
        Item = (
            Point,
            &mut iced::Element<'a, Message, Theme, Renderer>,
            &NodeConfig,
        ),
    > {
        self.nodes
            .iter_mut()
            .map(|(p, e, c)| (*p, e, c as &NodeConfig))
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
        self.external_selection
    }

    pub(super) fn get_on_event(
        &self,
    ) -> Option<&(dyn Fn(NodeGraphMessage<N, P, E>) -> Message + 'a)> {
        self.on_event.as_deref()
    }

    pub(super) fn on_camera_change_handler(
        &self,
    ) -> Option<&Box<dyn Fn(Point, f32) -> Message + 'a>> {
        self.on_camera_change.as_ref()
    }

    pub fn needs_animation(&self) -> bool {
        false
    }

    /// Translates a list of internal node indices to user IDs.
    /// Returns empty vec if any translation fails.
    pub(super) fn translate_node_ids(&self, indices: &[usize]) -> Vec<N> {
        indices
            .iter()
            .filter_map(|&idx| self.id_maps.node_id(idx).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_graph_event_debug() {
        let event = NodeGraphEvent::EdgeConnected {
            from: PinReference::new(0, 1),
            to: PinReference::new(2, 3),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("EdgeConnected"));
    }

    #[test]
    fn test_node_graph_event_clone() {
        let event = NodeGraphEvent::NodeMoved {
            node_id: 5,
            position: Point::new(100.0, 200.0),
        };
        let cloned = event.clone();
        if let NodeGraphEvent::NodeMoved { node_id, position } = cloned {
            assert_eq!(node_id, 5);
            assert_eq!(position.x, 100.0);
            assert_eq!(position.y, 200.0);
        } else {
            panic!("Expected NodeMoved");
        }
    }

    #[test]
    fn test_node_graph_event_variants() {
        // Test all event variants can be created
        let _ = NodeGraphEvent::EdgeConnected {
            from: PinReference::new(0, 0),
            to: PinReference::new(1, 0),
        };
        let _ = NodeGraphEvent::EdgeDisconnected {
            from: PinReference::new(0, 0),
            to: PinReference::new(1, 0),
        };
        let _ = NodeGraphEvent::NodeMoved {
            node_id: 0,
            position: Point::ORIGIN,
        };
        let _ = NodeGraphEvent::GroupMoved {
            node_ids: vec![0, 1, 2],
            delta: Vector::new(10.0, 20.0),
        };
        let _ = NodeGraphEvent::SelectionChanged {
            selected: vec![0, 1],
        };
        let _ = NodeGraphEvent::CloneRequested { node_ids: vec![0] };
        let _ = NodeGraphEvent::DeleteRequested {
            node_ids: vec![0, 1, 2],
        };
    }

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
