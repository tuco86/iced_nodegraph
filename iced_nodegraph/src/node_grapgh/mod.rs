use std::collections::HashSet;

use iced::{Color, Length, Point, Size, Vector};

use crate::node_pin::PinReference;
use crate::style::{EdgeStyle, GraphDefaults, GraphStyle, NodeStyle};

pub mod camera;
pub(crate) mod canonical;
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

#[allow(missing_debug_implementations)]
pub struct NodeGraph<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    pub(super) size: Size<Length>,
    pub(super) nodes: Vec<(
        Point,
        iced::Element<'a, Message, Theme, Renderer>,
        Option<NodeStyle>,
    )>,
    pub(super) edges: Vec<(PinReference, PinReference, Option<EdgeStyle>)>,
    graph_style: Option<GraphStyle>,
    /// Graph-wide style defaults for the cascading style system.
    /// Applied after theme defaults but before per-item styles.
    pub(super) graph_defaults: Option<GraphDefaults>,
    on_connect: Option<Box<dyn Fn(PinReference, PinReference) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(PinReference, PinReference) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(usize, Point) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_clone: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_delete: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_group_move: Option<Box<dyn Fn(Vec<usize>, Vector) -> Message + 'a>>,
    external_selection: Option<&'a HashSet<usize>>,
    // Drag event callbacks for real-time collaboration
    on_drag_start: Option<Box<dyn Fn(DragInfo) -> Message + 'a>>,
    on_drag_update: Option<Box<dyn Fn(f32, f32) -> Message + 'a>>,
    on_drag_end: Option<Box<dyn Fn() -> Message + 'a>>,
    // Remote users for collaborative rendering
    remote_users: Option<&'a [RemoteUserState]>,
    /// Unified event callback for all graph interactions.
    /// Alternative to individual callbacks (on_connect, on_move, etc.)
    on_event: Option<Box<dyn Fn(NodeGraphEvent) -> Message + 'a>>,
}

impl<Message, Theme, Renderer> Default for NodeGraph<'_, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            nodes: Vec::new(),
            edges: Vec::new(),
            graph_style: None,
            graph_defaults: None,
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
        }
    }
}

impl<'a, Message, Theme, Renderer> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    pub fn push_node(
        &mut self,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
    ) {
        self.nodes.push((position, element.into(), None));
    }

    pub fn push_node_styled(
        &mut self,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
        style: NodeStyle,
    ) {
        self.nodes.push((position, element.into(), Some(style)));
    }

    pub fn push_edge(&mut self, from: PinReference, to: PinReference) {
        self.edges.push((from, to, None));
    }

    pub fn push_edge_styled(&mut self, from: PinReference, to: PinReference, style: EdgeStyle) {
        self.edges.push((from, to, Some(style)));
    }

    pub fn graph_style(mut self, style: GraphStyle) -> Self {
        self.graph_style = Some(style);
        self
    }

    /// Sets graph-wide style defaults for the cascading style system.
    ///
    /// These defaults are applied after theme defaults but before per-item styles.
    /// Use this to configure consistent styling across all nodes, edges, and pins
    /// in this graph without overriding individual item styles.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use iced_nodegraph::style::{GraphDefaults, NodeConfig, EdgeConfig};
    ///
    /// let defaults = GraphDefaults::new()
    ///     .node(NodeConfig::new()
    ///         .corner_radius(10.0)
    ///         .opacity(0.8))
    ///     .edge(EdgeConfig::new()
    ///         .thickness(3.0));
    ///
    /// node_graph()
    ///     .defaults(defaults)
    ///     // nodes will inherit corner_radius=10.0 and opacity=0.8
    /// ```
    pub fn defaults(mut self, defaults: GraphDefaults) -> Self {
        self.graph_defaults = Some(defaults);
        self
    }

    /// Sets a callback for when an edge is connected between two pins.
    pub fn on_connect(mut self, f: impl Fn(PinReference, PinReference) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets a callback for when an edge is disconnected between two pins.
    pub fn on_disconnect(mut self, f: impl Fn(PinReference, PinReference) -> Message + 'a) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    pub fn on_move(mut self, f: impl Fn(usize, Point) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(Vec<usize>) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    pub fn on_clone(mut self, f: impl Fn(Vec<usize>) -> Message + 'a) -> Self {
        self.on_clone = Some(Box::new(f));
        self
    }

    pub fn on_delete(mut self, f: impl Fn(Vec<usize>) -> Message + 'a) -> Self {
        self.on_delete = Some(Box::new(f));
        self
    }

    pub fn on_group_move(mut self, f: impl Fn(Vec<usize>, Vector) -> Message + 'a) -> Self {
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// node_graph()
    ///     .on_event(|event| match event {
    ///         NodeGraphEvent::EdgeConnected { from, to } => Message::Connected { from, to },
    ///         NodeGraphEvent::NodeMoved { node_id, position } => Message::Moved { node_id, position },
    ///         NodeGraphEvent::SelectionChanged { selected } => Message::Selected(selected),
    ///         _ => Message::Noop,
    ///     })
    /// ```
    pub fn on_event(mut self, f: impl Fn(NodeGraphEvent) -> Message + 'a) -> Self {
        self.on_event = Some(Box::new(f));
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

    pub fn edges(&self) -> impl Iterator<Item = (PinReference, PinReference, Option<&EdgeStyle>)> {
        self.edges
            .iter()
            .map(|(from, to, style)| (*from, *to, style.as_ref()))
    }

    pub fn node_position(&self, node_id: usize) -> Option<Point> {
        self.nodes.get(node_id).map(|(pos, _, _)| *pos)
    }

    pub(super) fn elements_iter(
        &self,
    ) -> impl Iterator<
        Item = (
            Point,
            &iced::Element<'a, Message, Theme, Renderer>,
            Option<&NodeStyle>,
        ),
    > {
        self.nodes.iter().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<
        Item = (
            Point,
            &mut iced::Element<'a, Message, Theme, Renderer>,
            Option<&NodeStyle>,
        ),
    > {
        self.nodes.iter_mut().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    pub(super) fn get_graph_style(&self) -> Option<&GraphStyle> {
        self.graph_style.as_ref()
    }

    #[allow(dead_code)]
    pub(super) fn edges_iter(
        &self,
    ) -> impl Iterator<Item = (PinReference, PinReference, Option<&EdgeStyle>)> {
        self.edges
            .iter()
            .map(|(from, to, style)| (*from, *to, style.as_ref()))
    }

    pub(super) fn on_connect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(PinReference, PinReference) -> Message + 'a>> {
        self.on_connect.as_ref()
    }
    pub(super) fn on_disconnect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(PinReference, PinReference) -> Message + 'a>> {
        self.on_disconnect.as_ref()
    }
    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(usize, Point) -> Message + 'a>> {
        self.on_move.as_ref()
    }
    pub(super) fn on_select_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> {
        self.on_select.as_ref()
    }
    pub(super) fn on_clone_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> {
        self.on_clone.as_ref()
    }
    pub(super) fn on_delete_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> {
        self.on_delete.as_ref()
    }
    pub(super) fn on_group_move_handler(
        &self,
    ) -> Option<&Box<dyn Fn(Vec<usize>, Vector) -> Message + 'a>> {
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
    pub(super) fn get_remote_users(&self) -> Option<&[RemoteUserState]> {
        self.remote_users
    }

    pub(super) fn get_on_event(&self) -> Option<&(dyn Fn(NodeGraphEvent) -> Message + 'a)> {
        self.on_event.as_deref()
    }

    pub fn needs_animation(&self) -> bool {
        false
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
