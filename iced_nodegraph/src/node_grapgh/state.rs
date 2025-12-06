use super::euclid::WorldPoint;
use super::camera::Camera2D;
use web_time::Instant;
use iced::animation::Animation;
use iced::keyboard;
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Dragging {
    #[default]
    None,
    Graph(WorldPoint),                    // cursor origin (right mouse button pan)
    Node(usize, WorldPoint),              // node id and cursor origin
    Edge(usize, usize, WorldPoint),       // from_node and from_pin and cursor origin
    EdgeOver(usize, usize, usize, usize), // from_node, from_pin, to_node and to_pin
    BoxSelect(WorldPoint, WorldPoint),    // start point, current point (left mouse on empty space)
    GroupMove(WorldPoint),                // origin point (when dragging a selected node, all move)
    /// Fruit Ninja edge cutting: trail of cursor positions for visualization
    EdgeCutting(Vec<WorldPoint>),
}

#[derive(Debug)]
pub(super) struct NodeGraphState {
    pub(super) camera: Camera2D,
    pub(super) dragging: Dragging,
    pub(super) time: f32,
    pub(super) last_update: Option<Instant>,
    pub(super) fade_in: Animation<bool>,
    pub(super) selected_nodes: HashSet<usize>,
    pub(super) modifiers: keyboard::Modifiers,
    /// Tracks if left mouse button is pressed (for Fruit Ninja edge cutting)
    pub(super) left_mouse_down: bool,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self {
            camera: Camera2D::new(),
            dragging: Default::default(),
            time: 0.0,
            last_update: None,
            fade_in: Animation::new(false)
                .easing(iced::animation::Easing::EaseOut)
                .slow(),
            selected_nodes: HashSet::new(),
            modifiers: keyboard::Modifiers::default(),
            left_mouse_down: false,
        }
    }
}
