use iced::{Length, Point, Size};

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
    nodes: Vec<(Point, iced::Element<'a, Message, Theme, Renderer>)>, // (node_id, pin_id) -> node
    edges: Vec<((usize, usize), (usize, usize))>, // (from_node, from_pin) -> (to_node, to_pin)
    on_connect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(usize, Point) -> Message + 'a>>,
}

impl<Message, Theme, Renderer> Default for NodeGraph<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    fn default() -> Self {
        Self {
            size: Size::new(Length::Fill, Length::Fill),
            nodes: Vec::new(),
            edges: Vec::new(),
            on_connect: None,
            on_disconnect: None,
            on_move: None,
        }
    }
}

impl<'a, Message, Theme, Renderer> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    pub fn push_node(
        &mut self,
        position: Point,
        element: impl Into<iced::Element<'a, Message, Theme, Renderer>>,
    ) {
        self.nodes.push((position, element.into()));
    }

    pub fn push_edge(
        &mut self,
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    ) {
        self.edges.push(((from_node, from_pin), (to_node, to_pin)));
    }

    /// Sets the message that will be produced when an edge connection is completed.
    /// 
    /// The closure receives (from_node, from_pin, to_node, to_pin) indices.
    pub fn on_connect(
        mut self,
        f: impl Fn(usize, usize, usize, usize) -> Message + 'a,
    ) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    /// Sets the message that will be produced when an edge is disconnected.
    /// 
    /// The closure receives (from_node, from_pin, to_node, to_pin) indices.
    pub fn on_disconnect(
        mut self,
        f: impl Fn(usize, usize, usize, usize) -> Message + 'a,
    ) -> Self {
        self.on_disconnect = Some(Box::new(f));
        self
    }

    /// Sets the message that will be produced when a node is moved.
    /// 
    /// The closure receives (node_index, new_position).
    pub fn on_move(
        mut self,
        f: impl Fn(usize, Point) -> Message + 'a,
    ) -> Self {
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
    ) -> impl Iterator<Item = (Point, &iced::Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter().map(|(p, e)| (*p, e))
    }

    pub(super) fn elements_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point, &mut iced::Element<'a, Message, Theme, Renderer>)> {
        self.nodes.iter_mut().map(|(p, e)| (*p, e))
    }

    pub(super) fn on_connect_handler(&self) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> {
        self.on_connect.as_ref()
    }

    pub(super) fn on_disconnect_handler(&self) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> {
        self.on_disconnect.as_ref()
    }

    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(usize, Point) -> Message + 'a>> {
        self.on_move.as_ref()
    }
}
