//! Physics simulation for edge vertices.
//!
//! This module provides a hybrid CPU/GPU physics simulation for edge wires.
//! Edges are represented as chains of vertices connected by springs,
//! with magnetic repulsion from nodes and other edges.

use super::canonical::CanonicalVertex;
use super::euclid::{WorldPoint, WorldVector};

/// Configuration for the physics simulation.
///
/// These parameters control the behavior of edge wires:
/// - Spring stiffness determines how quickly wires return to rest
/// - Damping controls energy loss (prevents infinite oscillation)
/// - Repulsion keeps wires from overlapping with nodes and each other
#[derive(Debug, Clone)]
pub struct PhysicsConfig {
    /// Spring stiffness between adjacent vertices.
    /// Higher values make wires more rigid.
    pub spring_stiffness: f32,

    /// Damping coefficient (energy loss per frame).
    /// 0.0 = no damping, 1.0 = fully damped.
    /// Typical values: 0.90-0.98
    pub damping: f32,

    /// Rest length between adjacent vertices in world units.
    /// This determines how many vertices an edge has.
    pub rest_length: f32,

    /// Repulsion strength from nodes.
    /// Higher values push wires further from nodes.
    pub node_repulsion: f32,

    /// Repulsion strength between edge vertices.
    /// Prevents wires from overlapping each other.
    pub edge_repulsion: f32,

    /// Radius within which repulsion forces apply.
    pub repulsion_radius: f32,

    /// Maximum velocity for any vertex.
    /// Prevents simulation explosion.
    pub max_velocity: f32,

    /// Number of physics substeps per frame.
    /// More substeps = more stable but slower.
    pub substeps: u32,

    /// Fixed timestep for physics integration (seconds).
    pub fixed_dt: f32,

    // === New parameters for enhanced physics ===
    /// Tension factor: multiplier on rest length to create constant tension.
    /// < 1.0 means edges are under compression (want to be shorter).
    /// Default 0.8 means edges try to be 80% of their natural length.
    pub tension_factor: f32,

    /// Strength of force pushing vertices toward the direct path between pins.
    pub path_attraction: f32,

    /// Node padding: minimum distance from node edges.
    pub node_padding: f32,

    /// Force strength when inside a node (very strong push out).
    pub inside_node_force: f32,

    /// Long-range attraction between edges (weak).
    pub edge_attraction: f32,

    /// Distance beyond which edge attraction starts.
    pub edge_attraction_range: f32,

    /// Gravity strength (positive = downward sag).
    /// Creates natural cable droop.
    pub gravity: f32,

    /// Bending stiffness - resistance to sharp bends.
    /// Higher values make the cable smoother/stiffer.
    pub bending_stiffness: f32,

    /// Long-range attraction strength toward nodes.
    /// Pulls distant edges gently toward nearby nodes.
    pub node_attraction: f32,

    /// Distance beyond which node attraction starts.
    pub node_attraction_range: f32,

    /// Pin suction strength - how strongly pins "slurp up" the edge.
    /// Higher values keep edges taut and prevent vertices from leaking past pins.
    pub pin_suction: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            spring_stiffness: 800.0, // Very stiff springs - taut cables
            damping: 0.92,           // Higher damping for stability
            rest_length: 10.0,       // Short segments = many vertices
            node_repulsion: 600.0,   // Moderate repulsion at close range
            edge_repulsion: 300.0,   // Edge-edge repulsion
            repulsion_radius: 15.0,  // Small repulsion radius (15 pixels)
            max_velocity: 200.0,     // Lower max velocity for stability
            substeps: 4,
            fixed_dt: 1.0 / 120.0, // 120 Hz physics
            // Enhanced parameters
            tension_factor: 0.75, // Edges want to be 75% of natural length (very taut)
            path_attraction: 2.0, // Very weak path attraction
            node_padding: 15.0,   // Keep 15 units from node edges
            inside_node_force: 3000.0, // Strong push when inside nodes
            edge_attraction: 10.0, // Weak long-range attraction between edges
            edge_attraction_range: 80.0, // Start edge attraction beyond 80 units
            gravity: 8.0,         // Very light gravity - mostly horizontal tension
            bending_stiffness: 100.0, // High bending stiffness - smooth curves
            node_attraction: 8.0, // Weak long-range attraction toward nodes
            node_attraction_range: 100.0, // Start node attraction beyond 100 units
            pin_suction: 150.0,   // Strong pin suction - "slurping spaghetti"
        }
    }
}

impl PhysicsConfig {
    /// Create a new physics config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set spring stiffness.
    pub fn with_spring_stiffness(mut self, value: f32) -> Self {
        self.spring_stiffness = value;
        self
    }

    /// Set damping.
    pub fn with_damping(mut self, value: f32) -> Self {
        self.damping = value.clamp(0.0, 1.0);
        self
    }

    /// Set rest length.
    pub fn with_rest_length(mut self, value: f32) -> Self {
        self.rest_length = value.max(1.0);
        self
    }

    /// Set node repulsion.
    pub fn with_node_repulsion(mut self, value: f32) -> Self {
        self.node_repulsion = value;
        self
    }

    /// Set edge repulsion.
    pub fn with_edge_repulsion(mut self, value: f32) -> Self {
        self.edge_repulsion = value;
        self
    }
}

/// State of the physics simulation.
#[derive(Debug, Default)]
pub struct EdgePhysicsState {
    /// Whether physics simulation is enabled.
    pub enabled: bool,

    /// Physics configuration.
    pub config: PhysicsConfig,

    /// Accumulated time for fixed timestep integration.
    pub accumulated_time: f32,

    /// Whether GPU buffers need to be updated.
    pub gpu_dirty: bool,
}

impl EdgePhysicsState {
    /// Create a new physics state.
    pub fn new() -> Self {
        Self {
            enabled: true,
            config: PhysicsConfig::default(),
            accumulated_time: 0.0,
            gpu_dirty: true,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: PhysicsConfig) -> Self {
        Self {
            enabled: true,
            config,
            accumulated_time: 0.0,
            gpu_dirty: true,
        }
    }

    /// Enable or disable physics.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Accumulate time and return number of physics steps to run.
    pub fn accumulate_time(&mut self, dt: f32) -> u32 {
        if !self.enabled {
            return 0;
        }

        self.accumulated_time += dt;
        let mut steps = 0;

        while self.accumulated_time >= self.config.fixed_dt {
            self.accumulated_time -= self.config.fixed_dt;
            steps += 1;
        }

        // Cap to prevent spiral of death
        steps.min(self.config.substeps * 2)
    }
}

/// Signed distance to a rounded box (node shape).
/// Returns negative when inside, positive when outside.
fn sd_rounded_box(
    point: WorldPoint,
    center: WorldPoint,
    half_size: WorldVector,
    radius: f32,
) -> f32 {
    let p = WorldVector::new(
        (point.x - center.x).abs() - half_size.x + radius,
        (point.y - center.y).abs() - half_size.y + radius,
    );
    let outside = (p.x.max(0.0) * p.x.max(0.0) + p.y.max(0.0) * p.y.max(0.0)).sqrt();
    let inside = p.x.max(p.y).min(0.0);
    outside + inside - radius
}

/// Calculate gradient of SDF at a point (normalized direction away from surface).
fn sd_rounded_box_gradient(
    point: WorldPoint,
    center: WorldPoint,
    half_size: WorldVector,
    radius: f32,
) -> WorldVector {
    let epsilon = 0.01;
    let dx = sd_rounded_box(
        WorldPoint::new(point.x + epsilon, point.y),
        center,
        half_size,
        radius,
    ) - sd_rounded_box(
        WorldPoint::new(point.x - epsilon, point.y),
        center,
        half_size,
        radius,
    );
    let dy = sd_rounded_box(
        WorldPoint::new(point.x, point.y + epsilon),
        center,
        half_size,
        radius,
    ) - sd_rounded_box(
        WorldPoint::new(point.x, point.y - epsilon),
        center,
        half_size,
        radius,
    );
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        WorldVector::zero()
    } else {
        WorldVector::new(dx / len, dy / len)
    }
}

/// Calculate force from node SDF.
/// Strong push when inside, medium push when close outside,
/// weak attraction when far (to guide edges around nodes nicely).
pub fn node_sdf_force(
    vertex: WorldPoint,
    node_pos: WorldVector,
    node_size: WorldVector,
    corner_radius: f32,
    padding: f32,
    inside_force: f32,
    outside_repulsion: f32,
    repulsion_radius: f32,
    attraction_strength: f32,
    attraction_range: f32,
) -> WorldVector {
    let center = WorldPoint::new(
        node_pos.x + node_size.x * 0.5,
        node_pos.y + node_size.y * 0.5,
    );
    let half_size = WorldVector::new(node_size.x * 0.5, node_size.y * 0.5);

    let dist = sd_rounded_box(vertex, center, half_size, corner_radius);
    let gradient = sd_rounded_box_gradient(vertex, center, half_size, corner_radius);

    if dist < 0.0 {
        // Inside node: strong push outward
        let force_magnitude = inside_force * (-dist / 10.0).min(1.0);
        WorldVector::new(gradient.x * force_magnitude, gradient.y * force_magnitude)
    } else if dist < padding + repulsion_radius {
        // Near node (within padding + repulsion_radius): push outward with falloff
        let t = dist / (padding + repulsion_radius);
        let force_magnitude = outside_repulsion * (1.0 - t) * (1.0 - t); // Quadratic falloff
        WorldVector::new(gradient.x * force_magnitude, gradient.y * force_magnitude)
    } else if dist > attraction_range {
        // Far from node: weak attraction (pull toward node)
        let t = ((dist - attraction_range) / attraction_range).min(1.0);
        let force_magnitude = -attraction_strength * t; // Negative = toward node
        WorldVector::new(gradient.x * force_magnitude, gradient.y * force_magnitude)
    } else {
        WorldVector::zero()
    }
}

/// Calculate force pulling vertex toward the direct line between two pins.
/// This creates a tendency for edges to take the shortest path.
pub fn path_attraction_force(
    vertex: WorldPoint,
    start_pin: WorldPoint,
    end_pin: WorldPoint,
    strength: f32,
) -> WorldVector {
    // Project vertex onto line segment
    let line = WorldVector::new(end_pin.x - start_pin.x, end_pin.y - start_pin.y);
    let line_len_sq = line.x * line.x + line.y * line.y;

    if line_len_sq < 0.001 {
        return WorldVector::zero();
    }

    let t = ((vertex.x - start_pin.x) * line.x + (vertex.y - start_pin.y) * line.y) / line_len_sq;
    let t = t.clamp(0.0, 1.0);

    // Closest point on line
    let closest = WorldPoint::new(start_pin.x + t * line.x, start_pin.y + t * line.y);

    // Force toward closest point
    let dx = closest.x - vertex.x;
    let dy = closest.y - vertex.y;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < 0.001 {
        WorldVector::zero()
    } else {
        // Quadratic falloff - stronger when far from path
        let force_magnitude = strength * (dist / 100.0).min(1.0);
        WorldVector::new(dx / dist * force_magnitude, dy / dist * force_magnitude)
    }
}

/// Calculate edge-edge interaction force.
/// Short range: repulsion. Long range: weak attraction.
pub fn edge_interaction_force(
    vertex: WorldPoint,
    other_vertex: WorldPoint,
    repulsion_strength: f32,
    repulsion_radius: f32,
    attraction_strength: f32,
    attraction_range: f32,
) -> WorldVector {
    let dx = vertex.x - other_vertex.x;
    let dy = vertex.y - other_vertex.y;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < 0.001 {
        return WorldVector::zero();
    }

    let dir_x = dx / dist;
    let dir_y = dy / dist;

    if dist < repulsion_radius {
        // Short range repulsion
        let t = 1.0 - dist / repulsion_radius;
        let force_magnitude = repulsion_strength * t * t;
        WorldVector::new(dir_x * force_magnitude, dir_y * force_magnitude)
    } else if dist > attraction_range {
        // Long range attraction (pull toward other)
        let t = ((dist - attraction_range) / attraction_range).min(1.0);
        let force_magnitude = -attraction_strength * t;
        WorldVector::new(dir_x * force_magnitude, dir_y * force_magnitude)
    } else {
        WorldVector::zero()
    }
}

/// Calculate spring force between two positions.
///
/// Returns force vector applied to `from` position.
pub fn spring_force(
    from: WorldPoint,
    to: WorldPoint,
    stiffness: f32,
    rest_length: f32,
) -> WorldVector {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let distance = (dx * dx + dy * dy).sqrt();

    if distance < 0.001 {
        return WorldVector::zero();
    }

    let direction_x = dx / distance;
    let direction_y = dy / distance;
    let displacement = distance - rest_length;
    let force_magnitude = stiffness * displacement;

    WorldVector::new(direction_x * force_magnitude, direction_y * force_magnitude)
}

/// Calculate bending stiffness force.
///
/// This force resists sharp bends by pulling the middle vertex toward
/// the line between prev and next vertices. Creates smooth cable curves.
pub fn bending_force(
    prev: WorldPoint,
    current: WorldPoint,
    next: WorldPoint,
    stiffness: f32,
) -> WorldVector {
    // Target position is midpoint between prev and next
    let target_x = (prev.x + next.x) * 0.5;
    let target_y = (prev.y + next.y) * 0.5;

    // Force toward the midpoint (straightening force)
    let dx = target_x - current.x;
    let dy = target_y - current.y;

    WorldVector::new(dx * stiffness, dy * stiffness)
}

/// Calculate gravity force (downward).
pub fn gravity_force(strength: f32) -> WorldVector {
    WorldVector::new(0.0, strength)
}

/// Calculate pin suction force.
/// Pulls vertex toward the nearest anchor point (start or end pin).
/// This simulates the pin "slurping up" the edge like spaghetti.
/// The force is stronger for vertices closer to the anchor.
pub fn pin_suction_force(
    vertex: WorldPoint,
    start_anchor: WorldPoint,
    end_anchor: WorldPoint,
    vertex_t: f32, // 0.0 = at start, 1.0 = at end
    strength: f32,
) -> WorldVector {
    // Determine which anchor to pull toward based on position along edge
    let (target, pull_strength) = if vertex_t < 0.5 {
        // Closer to start - pull toward start with strength based on proximity
        let t = 1.0 - vertex_t * 2.0; // 1.0 at start, 0.0 at middle
        (start_anchor, strength * t * t)
    } else {
        // Closer to end - pull toward end
        let t = (vertex_t - 0.5) * 2.0; // 0.0 at middle, 1.0 at end
        (end_anchor, strength * t * t)
    };

    let dx = target.x - vertex.x;
    let dy = target.y - vertex.y;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < 0.001 {
        WorldVector::zero()
    } else {
        WorldVector::new(dx / dist * pull_strength, dy / dist * pull_strength)
    }
}

/// Calculate repulsion force from a point.
///
/// Returns force vector pushing `from` away from `repulsor`.
pub fn repulsion_force(
    from: WorldPoint,
    repulsor: WorldPoint,
    strength: f32,
    radius: f32,
) -> WorldVector {
    let dx = from.x - repulsor.x;
    let dy = from.y - repulsor.y;
    let distance_sq = dx * dx + dy * dy;
    let distance = distance_sq.sqrt();

    if distance > radius || distance < 0.001 {
        return WorldVector::zero();
    }

    let direction_x = dx / distance;
    let direction_y = dy / distance;
    let force_magnitude = strength / (distance_sq + 1.0);

    WorldVector::new(direction_x * force_magnitude, direction_y * force_magnitude)
}

/// Apply physics step to a vertex.
///
/// Updates position and velocity based on accumulated forces.
pub fn integrate_vertex(
    vertex: &mut CanonicalVertex,
    force: WorldVector,
    dt: f32,
    damping: f32,
    max_velocity: f32,
) {
    if vertex.is_anchored {
        return;
    }

    // Semi-implicit Euler integration
    let acceleration = WorldVector::new(force.x / vertex.mass, force.y / vertex.mass);

    // Update velocity
    vertex.velocity.x = (vertex.velocity.x + acceleration.x * dt) * damping;
    vertex.velocity.y = (vertex.velocity.y + acceleration.y * dt) * damping;

    // Clamp velocity
    let speed =
        (vertex.velocity.x * vertex.velocity.x + vertex.velocity.y * vertex.velocity.y).sqrt();
    if speed > max_velocity {
        let scale = max_velocity / speed;
        vertex.velocity.x *= scale;
        vertex.velocity.y *= scale;
    }

    // Update position
    vertex.position.x += vertex.velocity.x * dt;
    vertex.position.y += vertex.velocity.y * dt;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_config_default() {
        let config = PhysicsConfig::default();
        assert_eq!(config.spring_stiffness, 800.0);
        assert_eq!(config.damping, 0.92);
        assert_eq!(config.rest_length, 10.0);
        assert_eq!(config.pin_suction, 150.0);
    }

    #[test]
    fn test_physics_config_builder() {
        let config = PhysicsConfig::new()
            .with_spring_stiffness(1000.0)
            .with_damping(0.9)
            .with_rest_length(50.0);

        assert_eq!(config.spring_stiffness, 1000.0);
        assert_eq!(config.damping, 0.9);
        assert_eq!(config.rest_length, 50.0);
    }

    #[test]
    fn test_spring_force_at_rest() {
        let from = WorldPoint::new(0.0, 0.0);
        let to = WorldPoint::new(30.0, 0.0);
        let force = spring_force(from, to, 500.0, 30.0);

        // At rest length, force should be zero
        assert!(force.x.abs() < 0.001);
        assert!(force.y.abs() < 0.001);
    }

    #[test]
    fn test_spring_force_stretched() {
        let from = WorldPoint::new(0.0, 0.0);
        let to = WorldPoint::new(60.0, 0.0);
        let force = spring_force(from, to, 500.0, 30.0);

        // Stretched beyond rest, force should pull toward to
        assert!(force.x > 0.0);
        assert!(force.y.abs() < 0.001);
    }

    #[test]
    fn test_spring_force_compressed() {
        let from = WorldPoint::new(0.0, 0.0);
        let to = WorldPoint::new(15.0, 0.0);
        let force = spring_force(from, to, 500.0, 30.0);

        // Compressed below rest, force should push away
        assert!(force.x < 0.0);
    }

    #[test]
    fn test_repulsion_force_within_radius() {
        let from = WorldPoint::new(0.0, 0.0);
        let repulsor = WorldPoint::new(10.0, 0.0);
        let force = repulsion_force(from, repulsor, 1000.0, 50.0);

        // Should push away from repulsor (negative x)
        assert!(force.x < 0.0);
    }

    #[test]
    fn test_repulsion_force_outside_radius() {
        let from = WorldPoint::new(0.0, 0.0);
        let repulsor = WorldPoint::new(100.0, 0.0);
        let force = repulsion_force(from, repulsor, 1000.0, 50.0);

        // Outside radius, no force
        assert!(force.x.abs() < 0.001);
        assert!(force.y.abs() < 0.001);
    }

    #[test]
    fn test_accumulate_time() {
        let mut state = EdgePhysicsState::new();
        state.config.fixed_dt = 1.0 / 60.0; // 60 Hz

        // Accumulate one frame at 60 fps
        let steps = state.accumulate_time(1.0 / 60.0);
        assert_eq!(steps, 1);

        // Accumulate half a frame
        let steps = state.accumulate_time(1.0 / 120.0);
        assert_eq!(steps, 0);

        // Accumulate another half (should trigger)
        let steps = state.accumulate_time(1.0 / 120.0);
        assert_eq!(steps, 1);
    }

    #[test]
    fn test_accumulate_time_disabled() {
        let mut state = EdgePhysicsState::new();
        state.set_enabled(false);

        let steps = state.accumulate_time(1.0);
        assert_eq!(steps, 0);
    }

    #[test]
    fn test_integrate_vertex_anchored() {
        let mut vertex = CanonicalVertex::anchored(WorldPoint::new(0.0, 0.0), 0, 0);
        let original_pos = vertex.position;

        integrate_vertex(
            &mut vertex,
            WorldVector::new(1000.0, 1000.0),
            0.1,
            0.95,
            500.0,
        );

        // Anchored vertex should not move
        assert_eq!(vertex.position.x, original_pos.x);
        assert_eq!(vertex.position.y, original_pos.y);
    }

    #[test]
    fn test_integrate_vertex_free() {
        let mut vertex = CanonicalVertex::free(WorldPoint::new(0.0, 0.0), 0, 1);

        integrate_vertex(
            &mut vertex,
            WorldVector::new(100.0, 0.0),
            0.1,
            1.0, // No damping for predictable test
            500.0,
        );

        // Should have moved in x direction
        assert!(vertex.position.x > 0.0);
        assert!(vertex.velocity.x > 0.0);
    }

    #[test]
    fn test_velocity_clamping() {
        let mut vertex = CanonicalVertex::free(WorldPoint::new(0.0, 0.0), 0, 1);

        // Apply huge force
        integrate_vertex(
            &mut vertex,
            WorldVector::new(100000.0, 0.0),
            1.0,
            1.0,
            100.0, // Low max velocity
        );

        let speed =
            (vertex.velocity.x * vertex.velocity.x + vertex.velocity.y * vertex.velocity.y).sqrt();
        assert!(speed <= 100.0 + 0.001);
    }
}
