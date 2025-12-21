use super::camera::Camera2D;
use super::canonical::CanonicalState;
use super::effects::pipeline::cache::DirtyFlags;
use super::euclid::WorldPoint;
use iced::animation::Animation;
use iced::keyboard;
use std::collections::HashSet;
use web_time::Instant;

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
    /// Fruit Ninja edge cutting: trail of cursor positions and pending edges to cut
    EdgeCutting {
        trail: Vec<WorldPoint>,
        pending_cuts: HashSet<usize>,
    },
    /// Dragging an edge vertex (for physics wire simulation)
    EdgeVertex {
        edge_index: usize,
        vertex_index: usize,
        origin: WorldPoint,
    },
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
    /// Currently hovered node index (for hover effects)
    pub(super) hovered_node: Option<usize>,

    // Caching and incremental update support
    /// Canonical state storage (authoritative data).
    pub(super) canonical: CanonicalState,
    /// Dirty flags for incremental GPU updates.
    pub(super) dirty: DirtyFlags,
    /// Generation counter for structural changes.
    pub(super) generation: u64,
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
            hovered_node: None,
            canonical: CanonicalState::new(),
            dirty: DirtyFlags::default(),
            generation: 0,
        }
    }
}

impl NodeGraphState {
    /// Mark a node's position as dirty.
    pub fn mark_node_position_dirty(&mut self, node_id: usize) {
        self.dirty.mark_node_position(node_id);
    }

    /// Mark a node's style as dirty.
    pub fn mark_node_style_dirty(&mut self, node_id: usize) {
        self.dirty.mark_node_style(node_id);
    }

    /// Mark an edge as dirty.
    pub fn mark_edge_dirty(&mut self, edge_id: usize) {
        self.dirty.mark_edge(edge_id);
    }

    /// Mark structural change (node/edge added/removed).
    pub fn mark_structure_changed(&mut self) {
        self.dirty.mark_structure_changed();
        self.generation += 1;
    }

    /// Clear dirty flags after GPU sync.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Check if any changes need to be synced to GPU.
    pub fn needs_sync(&self) -> bool {
        !self.dirty.is_clean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::Point2D;

    #[test]
    fn test_dragging_default_is_none() {
        let dragging: Dragging = Default::default();
        assert_eq!(dragging, Dragging::None);
    }

    #[test]
    fn test_dragging_states_not_equal() {
        let origin = Point2D::new(10.0, 20.0);

        assert_ne!(Dragging::None, Dragging::Graph(origin));
        assert_ne!(Dragging::Graph(origin), Dragging::Node(0, origin));
        assert_ne!(Dragging::Node(0, origin), Dragging::Edge(0, 0, origin));
    }

    #[test]
    fn test_dragging_graph_stores_origin() {
        let origin = Point2D::new(100.0, 200.0);
        let dragging = Dragging::Graph(origin);

        if let Dragging::Graph(stored) = dragging {
            assert_eq!(stored.x, 100.0);
            assert_eq!(stored.y, 200.0);
        } else {
            panic!("Expected Dragging::Graph");
        }
    }

    #[test]
    fn test_dragging_node_stores_index_and_origin() {
        let origin = Point2D::new(50.0, 75.0);
        let dragging = Dragging::Node(5, origin);

        if let Dragging::Node(idx, stored) = dragging {
            assert_eq!(idx, 5);
            assert_eq!(stored.x, 50.0);
            assert_eq!(stored.y, 75.0);
        } else {
            panic!("Expected Dragging::Node");
        }
    }

    #[test]
    fn test_dragging_edge_stores_node_pin_and_cursor() {
        let cursor = Point2D::new(300.0, 400.0);
        let dragging = Dragging::Edge(2, 1, cursor);

        if let Dragging::Edge(node, pin, stored) = dragging {
            assert_eq!(node, 2);
            assert_eq!(pin, 1);
            assert_eq!(stored.x, 300.0);
            assert_eq!(stored.y, 400.0);
        } else {
            panic!("Expected Dragging::Edge");
        }
    }

    #[test]
    fn test_box_select_stores_two_points() {
        let start = Point2D::new(0.0, 0.0);
        let current = Point2D::new(100.0, 100.0);
        let dragging = Dragging::BoxSelect(start, current);

        if let Dragging::BoxSelect(s, c) = dragging {
            assert_eq!(s.x, 0.0);
            assert_eq!(s.y, 0.0);
            assert_eq!(c.x, 100.0);
            assert_eq!(c.y, 100.0);
        } else {
            panic!("Expected Dragging::BoxSelect");
        }
    }

    #[test]
    fn test_group_move_stores_origin() {
        let origin = Point2D::new(250.0, 350.0);
        let dragging = Dragging::GroupMove(origin);

        if let Dragging::GroupMove(stored) = dragging {
            assert_eq!(stored.x, 250.0);
            assert_eq!(stored.y, 350.0);
        } else {
            panic!("Expected Dragging::GroupMove");
        }
    }

    #[test]
    fn test_edge_cutting_trail() {
        let trail = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(20.0, 20.0),
        ];
        let mut pending_cuts = HashSet::new();
        pending_cuts.insert(1);
        pending_cuts.insert(3);
        let dragging = Dragging::EdgeCutting {
            trail: trail.clone(),
            pending_cuts: pending_cuts.clone(),
        };

        if let Dragging::EdgeCutting { trail: stored, pending_cuts: cuts } = dragging {
            assert_eq!(stored.len(), 3);
            assert_eq!(stored[0].x, 0.0);
            assert_eq!(stored[2].x, 20.0);
            assert!(cuts.contains(&1));
            assert!(cuts.contains(&3));
            assert!(!cuts.contains(&2));
        } else {
            panic!("Expected Dragging::EdgeCutting");
        }
    }

    #[test]
    fn test_selection_set_operations() {
        let mut state = NodeGraphState::default();

        // Start empty
        assert!(state.selected_nodes.is_empty());

        // Add nodes
        state.selected_nodes.insert(0);
        state.selected_nodes.insert(2);
        state.selected_nodes.insert(5);

        assert_eq!(state.selected_nodes.len(), 3);
        assert!(state.selected_nodes.contains(&0));
        assert!(state.selected_nodes.contains(&2));
        assert!(state.selected_nodes.contains(&5));
        assert!(!state.selected_nodes.contains(&1));

        // Remove node
        state.selected_nodes.remove(&2);
        assert_eq!(state.selected_nodes.len(), 2);
        assert!(!state.selected_nodes.contains(&2));

        // Clear all
        state.selected_nodes.clear();
        assert!(state.selected_nodes.is_empty());
    }

    #[test]
    fn test_node_graph_state_default() {
        let state = NodeGraphState::default();

        assert_eq!(state.dragging, Dragging::None);
        assert_eq!(state.time, 0.0);
        assert!(state.last_update.is_none());
        assert!(state.selected_nodes.is_empty());
        assert!(!state.left_mouse_down);
    }
}
