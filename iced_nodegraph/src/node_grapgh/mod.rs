use std::collections::HashSet;

use iced::{Length, Point, Size, Vector};

use crate::node_pin::PinReference;
use crate::style::{EdgeStyle, GraphStyle, NodeStyle};

pub mod camera;
pub(crate) mod effects;
pub(crate) mod euclid;
pub(crate) mod state;
pub(crate) mod widget;

#[cfg(test)]
mod interaction_tests;

/// Events emitted by the NodeGraph widget.
#[derive(Debug, Clone)]
pub enum NodeGraphEvent {
    EdgeConnected { from: PinReference, to: PinReference },
    EdgeDisconnected { from: PinReference, to: PinReference },
    NodeMoved { node_id: usize, position: Point },
    GroupMoved { node_ids: Vec<usize>, delta: Vector },
    SelectionChanged { selected: Vec<usize> },
    CloneRequested { node_ids: Vec<usize> },
    DeleteRequested { node_ids: Vec<usize> },
}

#[allow(missing_debug_implementations)]
pub struct NodeGraph<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    pub(super) size: Size<Length>,
    pub(super) nodes: Vec<(Point, iced::Element<'a, Message, Theme, Renderer>, Option<NodeStyle>)>,
    pub(super) edges: Vec<(PinReference, PinReference, Option<EdgeStyle>)>,
    graph_style: Option<GraphStyle>,
    on_connect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_disconnect: Option<Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>>,
    on_move: Option<Box<dyn Fn(usize, Point) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_clone: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_delete: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
    on_group_move: Option<Box<dyn Fn(Vec<usize>, Vector) -> Message + 'a>>,
    external_selection: Option<&'a HashSet<usize>>,
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
            on_select: None,
            on_clone: None,
            on_delete: None,
            on_group_move: None,
            external_selection: None,
        }
    }
}

impl<'a, Message, Theme, Renderer> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    pub fn push_node(&mut self, position: Point, element: impl Into<iced::Element<'a, Message, Theme, Renderer>>) {
        self.nodes.push((position, element.into(), None));
    }

    pub fn push_node_styled(&mut self, position: Point, element: impl Into<iced::Element<'a, Message, Theme, Renderer>>, style: NodeStyle) {
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

    pub fn on_connect(mut self, f: impl Fn(usize, usize, usize, usize) -> Message + 'a) -> Self {
        self.on_connect = Some(Box::new(f));
        self
    }

    pub fn on_disconnect(mut self, f: impl Fn(usize, usize, usize, usize) -> Message + 'a) -> Self {
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

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }

    pub fn edges(&self) -> impl Iterator<Item = (PinReference, PinReference, Option<&EdgeStyle>)> {
        self.edges.iter().map(|(from, to, style)| (*from, *to, style.as_ref()))
    }

    pub fn node_position(&self, node_id: usize) -> Option<Point> {
        self.nodes.get(node_id).map(|(pos, _, _)| *pos)
    }

    pub(super) fn elements_iter(&self) -> impl Iterator<Item = (Point, &iced::Element<'a, Message, Theme, Renderer>, Option<&NodeStyle>)> {
        self.nodes.iter().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    pub(super) fn elements_iter_mut(&mut self) -> impl Iterator<Item = (Point, &mut iced::Element<'a, Message, Theme, Renderer>, Option<&NodeStyle>)> {
        self.nodes.iter_mut().map(|(p, e, s)| (*p, e, s.as_ref()))
    }

    pub(super) fn get_graph_style(&self) -> Option<&GraphStyle> { self.graph_style.as_ref() }

    #[allow(dead_code)]
    pub(super) fn edges_iter(&self) -> impl Iterator<Item = (PinReference, PinReference, Option<&EdgeStyle>)> {
        self.edges.iter().map(|(from, to, style)| (*from, *to, style.as_ref()))
    }

    pub(super) fn on_connect_handler(&self) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> { self.on_connect.as_ref() }
    pub(super) fn on_disconnect_handler(&self) -> Option<&Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>> { self.on_disconnect.as_ref() }
    pub(super) fn on_move_handler(&self) -> Option<&Box<dyn Fn(usize, Point) -> Message + 'a>> { self.on_move.as_ref() }
    pub(super) fn on_select_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> { self.on_select.as_ref() }
    pub(super) fn on_clone_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> { self.on_clone.as_ref() }
    pub(super) fn on_delete_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>) -> Message + 'a>> { self.on_delete.as_ref() }
    pub(super) fn on_group_move_handler(&self) -> Option<&Box<dyn Fn(Vec<usize>, Vector) -> Message + 'a>> { self.on_group_move.as_ref() }
    pub(super) fn get_external_selection(&self) -> Option<&HashSet<usize>> { self.external_selection }

    pub fn needs_animation(&self) -> bool { false }
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
        let _ = NodeGraphEvent::CloneRequested {
            node_ids: vec![0],
        };
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
