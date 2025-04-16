use super::euclid::{ScreenPoint, WorldPoint};

use super::camera::Camera2D;

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) enum Dragging {
    #[default]
    None,
    Graph(ScreenPoint),             // cursor origin
    Node(usize, WorldPoint),        // node id and cursor origin
    Edge(usize, usize, WorldPoint), // from_node and from_pin and cursor origin
}

#[derive(Debug)]
pub(super) struct NodeGraphState {
    pub(super) camera: Camera2D,
    pub(super) dragging: Dragging,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self {
            camera: Camera2D::new(),
            dragging: Default::default(),
        }
    }
}
