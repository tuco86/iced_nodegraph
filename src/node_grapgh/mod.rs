use iced::{Length, Point, Size};

mod widget;

/// A container that distributes its contents according to their coordinates.
///
/// The number of columns is determined by the row with the most elements.
#[allow(missing_debug_implementations)]
pub struct NodeGraph<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    size: Size<Length>,
    nodes: Vec<(Point, iced::Element<'a, Message, Theme, Renderer>)>, // (node_id, pin_id) -> node
    edges: Vec<((usize, usize), (usize, usize))>, // (from_node, from_pin) -> (to_node, to_pin)
    dragging_node: Option<(usize, Point)>,        // draging node with offset
    dragging_edge: Option<(usize, usize)>,        // connecting (from_node, from_pin)
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
            dragging_node: None,
            dragging_edge: None,
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
}
