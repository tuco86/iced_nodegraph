use iced::{Length, Point, Size};

use crate::style::{EdgeStyle, GraphStyle, NodeStyle};

pub(crate) mod camera;
pub(crate) mod effects;
pub(crate) mod euclid;
pub(crate) mod state;
pub(crate) mod widget;

#[cfg(test)]
mod interaction_tests;

/// A container that distributes its contents according to their coordinates.
///
/// The number of columns is determined by the row with the most elements.
#[allow(missing_debug_implementations)]
pub struct NodeGraph<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    size: Size<Length>,
    /// Nodes with position, element, and optional per-node style
    nodes: Vec<(Point, iced::Element<'a, Message, Theme, Renderer>, Option<NodeStyle>)>,
    /// Edges with connectivity and optional per-edge style
    edges: Vec<(((usize, usize), (usize, usize)), Option<EdgeStyle>)>,
    /// Global graph style (background, drag colors)
    graph_style: Option<GraphStyle>,
    on_connect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(usize, Point) -> Message + 'a>>,
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
            on_connect: None,
            on_disconnect: None,
            on_move: None,
        }
    }
}

impl<'a, Message, Theme, Renderer> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    /// Adds a node at the given position with default styling.
    pub fn push_node(
        &mut self,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
    ) {
        self.nodes.push((position, element.into(), None));
    }

    /// Adds a node at the given position with custom styling.
    pub fn push_node_styled(
        &mut self,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
        style: NodeStyle,
    ) {
        self.nodes.push((position, element.into(), Some(style)));
    }

    /// Adds an edge between two pins with default styling.
    pub fn push_edge(&mut self, from_node: usize, from_pin: usize, to_node: usize, to_pin: usize) {
        self.edges.push((((from_node, from_pin), (to_node, to_pin)), None));
    }

    /// Adds an edge between two pins with custom styling.
    pub fn push_edge_styled(
        &mut self,
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
        style: EdgeStyle,
    ) {
        self.edges.push((((from_node, from_pin), (to_node, to_pin)), Some(style)));
    }

    /// Sets the global graph style (background, drag colors).
    pub fn graph_style(mut self, style: GraphStyle) -> Self {
        self.graph_style = Some(style);
        self
    }

    /// Sets the message that will be produced when an edge connection is completed.
    ///
    /// The closure receives (from_node, from_pin, to_node, to_pin) indices.
    pub fn on_connect(mut self, f: impl Fn(usize, usize, usize, usize) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets the message that will be produced when an edge is disconnected.
    ///
    /// The closure receives (from_node, from_pin, to_node, to_pin) indices.
    pub fn on_disconnect(mut self, f: impl Fn(usize, usize, usize, usize) -> Message + 'a) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    /// Sets the message that will be produced when a node is moved.
    ///
    /// The closure receives (node_index, new_position).
    pub fn on_move(mut self, f: impl Fn(usize, Point) -> Message + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }

    /// Sets the width of the [`NodeGraph`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.size.width = width.into();
        self
    }

    /// Sets the height of the [`NodeGraph`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.size.height = height.into();
        self
    }

    pub(super) fn elements_iter(
        &self,
    ) -> impl Iterator<Item = (Point, &iced::Element<'a, Message, Theme, Renderer>, Option<&NodeStyle>)> {
        self.nodes.iter().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point, &mut iced::Element<'a, Message, Theme, Renderer>, Option<&NodeStyle>)> {
        self.nodes.iter_mut().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    /// Returns the graph style if set.
    pub(super) fn get_graph_style(&self) -> Option<&GraphStyle> {
        self.graph_style.as_ref()
    }

    /// Returns the edges with their optional styles.
    #[allow(dead_code)] // Will be used when static edge rendering is implemented
    pub(super) fn edges_iter(&self) -> impl Iterator<Item = (((usize, usize), (usize, usize)), Option<&EdgeStyle>)> {
        self.edges.iter().map(|(conn, style)| (*conn, style.as_ref()))
    }

    pub(super) fn on_connect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> {
        self.on_connect.as_ref()
    }

    pub(super) fn on_disconnect_handler(
        &self,
    ) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> {
        self.on_disconnect.as_ref()
    }

    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(usize, Point) -> Message + 'a>> {
        self.on_move.as_ref()
    }

    /// Checks if the NodeGraph currently needs continuous animation updates
    pub fn needs_animation(&self) -> bool {
        // The widget itself will determine this based on its internal state
        // This is a placeholder - the actual implementation is in the widget
        false
    }
}
