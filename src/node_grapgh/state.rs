use super::euclid::WorldPoint;

use super::camera::Camera2D;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Dragging {
    #[default]
    None,
    Graph(WorldPoint),                   // cursor origin
    Node(usize, WorldPoint),              // node id and cursor origin
    Edge(usize, usize, WorldPoint),       // from_node and from_pin and cursor origin
    EdgeOver(usize, usize, usize, usize), // from_node, from_pin, to_node and to_pin
}

#[derive(Debug)]
pub(super) struct NodeGraphState {
    pub(super) camera: Camera2D,
    pub(super) dragging: Dragging,
    pub(super) time: f32,  // Time in seconds for animations
    pub(super) last_update: Option<std::time::Instant>,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self {
            camera: Camera2D::new(),
            dragging: Default::default(),
            time: 0.0,
            last_update: None,
        }
    }
}
