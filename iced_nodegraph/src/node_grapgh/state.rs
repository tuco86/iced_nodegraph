use super::camera::Camera2D;
use super::canonical::CanonicalState;
use super::effects::pipeline::cache::DirtyFlags;
use super::euclid::{WorldPoint, WorldVector};
use super::physics::{self, EdgePhysicsState, PhysicsConfig};
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
    /// Fruit Ninja edge cutting: trail of cursor positions for visualization
    EdgeCutting(Vec<WorldPoint>),
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

    // Caching and incremental update support
    /// Canonical state storage (authoritative data).
    pub(super) canonical: CanonicalState,
    /// Dirty flags for incremental GPU updates.
    pub(super) dirty: DirtyFlags,
    /// Generation counter for structural changes.
    pub(super) generation: u64,

    // Physics simulation
    /// Physics state for edge wire simulation.
    pub(super) physics: EdgePhysicsState,
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
            canonical: CanonicalState::new(),
            dirty: DirtyFlags::default(),
            generation: 0,
            physics: EdgePhysicsState::new(),
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

    /// Enable or disable physics simulation.
    pub fn set_physics_enabled(&mut self, enabled: bool) {
        self.physics.set_enabled(enabled);
    }

    /// Check if physics is enabled.
    pub fn physics_enabled(&self) -> bool {
        self.physics.enabled
    }

    /// Get mutable access to physics configuration.
    pub fn physics_config_mut(&mut self) -> &mut PhysicsConfig {
        &mut self.physics.config
    }

    /// Step the physics simulation by the given delta time.
    ///
    /// This performs CPU-side physics integration for edge vertices.
    /// Call this each frame with the time delta.
    ///
    /// Returns the number of physics steps that were executed.
    pub fn tick_physics(&mut self, dt: f32) -> u32 {
        let steps = self.physics.accumulate_time(dt);

        if steps == 0 {
            return 0;
        }

        let config = self.physics.config.clone();
        let tension_rest_length = config.rest_length * config.tension_factor;

        // Run physics steps
        for _ in 0..steps {
            // Second pass: integrate all non-anchored vertices
            for v_idx in 0..self.canonical.vertices.len() {
                let vertex = &self.canonical.vertices[v_idx];

                if vertex.is_anchored {
                    continue;
                }

                let edge = &self.canonical.edges[vertex.edge_id];
                let vertex_range = edge.vertex_range.clone();
                let local_idx = vertex.vertex_index;
                let vertex_count = vertex_range.len();

                let mut force = WorldVector::zero();

                // === Spring forces with tension ===
                // Force from previous vertex
                if local_idx > 0 {
                    let prev = &self.canonical.vertices[vertex_range.start + local_idx - 1];
                    force = force
                        + physics::spring_force(
                            vertex.position,
                            prev.position,
                            config.spring_stiffness,
                            tension_rest_length, // Use tensioned rest length
                        );
                }

                // Force from next vertex
                if local_idx < vertex_count - 1 {
                    let next = &self.canonical.vertices[vertex_range.start + local_idx + 1];
                    force = force
                        + physics::spring_force(
                            vertex.position,
                            next.position,
                            config.spring_stiffness,
                            tension_rest_length, // Use tensioned rest length
                        );
                }

                // === Bending stiffness ===
                // Smooths out sharp corners by pulling toward midpoint of neighbors
                if local_idx > 0 && local_idx < vertex_count - 1 {
                    let prev = &self.canonical.vertices[vertex_range.start + local_idx - 1];
                    let next = &self.canonical.vertices[vertex_range.start + local_idx + 1];
                    force = force
                        + physics::bending_force(
                            prev.position,
                            vertex.position,
                            next.position,
                            config.bending_stiffness,
                        );
                }

                // === Gravity (natural cable sag) ===
                force = force + physics::gravity_force(config.gravity);

                // === Node SDF-based repulsion (only) ===
                // Uses spatial culling: skip nodes beyond repulsion range
                for node in &self.canonical.nodes {
                    // Broad-phase culling: skip if vertex is too far from node
                    let node_center_x = node.position.x + node.size.width * 0.5;
                    let node_center_y = node.position.y + node.size.height * 0.5;
                    let dx = vertex.position.x - node_center_x;
                    let dy = vertex.position.y - node_center_y;
                    let dist_sq = dx * dx + dy * dy;
                    // Node half-diagonal + repulsion radius as cutoff
                    let half_diag = (node.size.width * node.size.width
                        + node.size.height * node.size.height)
                        .sqrt()
                        * 0.5;
                    let cutoff = half_diag + config.node_repulsion_radius;
                    if dist_sq > cutoff * cutoff {
                        continue; // Too far - skip
                    }

                    force = force
                        + physics::node_sdf_force(
                            vertex.position,
                            node.position,
                            WorldVector::new(node.size.width, node.size.height),
                            5.0, // corner radius
                            config.inside_node_force,
                            config.node_repulsion,
                            config.node_repulsion_radius,
                        );
                }

                // === Path attraction ===
                // Get start and end pin positions from edge anchors
                if vertex_range.len() >= 2 {
                    let start_vertex = &self.canonical.vertices[vertex_range.start];
                    let end_vertex = &self.canonical.vertices[vertex_range.end - 1];
                    force = force
                        + physics::path_attraction_force(
                            vertex.position,
                            start_vertex.position,
                            end_vertex.position,
                            config.path_attraction,
                        );

                    // === Pin suction ("slurping spaghetti") ===
                    // Pins pull the edge toward themselves, keeping it taut
                    let vertex_t = local_idx as f32 / (vertex_count - 1).max(1) as f32;
                    force = force
                        + physics::pin_suction_force(
                            vertex.position,
                            start_vertex.position,
                            end_vertex.position,
                            vertex_t,
                            config.pin_suction,
                        );
                }

                // === Edge-edge interaction ===
                // Check against vertices from other edges with spatial culling
                let max_range_sq =
                    config.max_interaction_range * config.max_interaction_range;

                for other_edge_idx in 0..self.canonical.edges.len() {
                    if other_edge_idx == vertex.edge_id {
                        continue; // Skip same edge
                    }

                    let other_edge = &self.canonical.edges[other_edge_idx];
                    let other_range = other_edge.vertex_range.clone();

                    // Sample other edge (check every other vertex for performance)
                    for other_v_idx in (other_range.start..other_range.end).step_by(2) {
                        let other_vertex = &self.canonical.vertices[other_v_idx];

                        // Spatial culling: skip if beyond max interaction range
                        let dx = vertex.position.x - other_vertex.position.x;
                        let dy = vertex.position.y - other_vertex.position.y;
                        if dx * dx + dy * dy > max_range_sq {
                            continue; // Too far - skip
                        }

                        force = force
                            + physics::edge_interaction_force(
                                vertex.position,
                                other_vertex.position,
                                config.edge_repulsion,
                                config.edge_equilibrium_distance,
                                config.edge_attraction,
                                config.max_interaction_range,
                            );
                    }
                }

                // Integrate
                let vertex = &mut self.canonical.vertices[v_idx];
                physics::integrate_vertex(
                    vertex,
                    force,
                    config.fixed_dt,
                    config.damping,
                    config.max_velocity,
                );

                // Mark as dirty
                self.dirty.mark_edge_vertex(v_idx);
            }
        }

        // Mark GPU dirty if we did any physics
        if steps > 0 {
            self.physics.gpu_dirty = true;
        }

        steps
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
        let dragging = Dragging::EdgeCutting(trail.clone());

        if let Dragging::EdgeCutting(stored) = dragging {
            assert_eq!(stored.len(), 3);
            assert_eq!(stored[0].x, 0.0);
            assert_eq!(stored[2].x, 20.0);
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
