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

    /// Maximum velocity for any vertex.
    /// Prevents simulation explosion.
    pub max_velocity: f32,

    /// Number of physics substeps per frame.
    /// More substeps = more stable but slower.
    pub substeps: u32,

    /// Fixed timestep for physics integration (seconds).
    pub fixed_dt: f32,

    /// Tension factor: multiplier on rest length to create constant tension.
    /// < 1.0 means edges are under compression (want to be shorter).
    /// Default 0.8 means edges try to be 80% of their natural length.
    pub tension_factor: f32,

    /// Strength of force pushing vertices toward the direct path between pins.
    pub path_attraction: f32,

    /// Force strength when inside a node (very strong push out).
    pub inside_node_force: f32,

    /// Attraction strength between edges in the medium range (10-100px).
    /// Causes edges to bundle together naturally.
    pub edge_attraction: f32,

    /// Gravity strength (positive = downward sag).
    /// Creates natural cable droop.
    pub gravity: f32,

    /// Bending stiffness - resistance to sharp bends.
    /// Higher values make the cable smoother/stiffer.
    pub bending_stiffness: f32,

    /// Pin suction strength - how strongly pins "slurp up" the edge.
    /// Higher values keep edges taut and prevent vertices from leaking past pins.
    pub pin_suction: f32,

    // === SDF-based force model parameters ===

    /// Maximum range for any force interaction (optimization cutoff).
    /// Beyond this distance, no forces are calculated.
    pub max_interaction_range: f32,

    /// Node repulsion zone: distance from node surface where repulsion applies.
    /// Inside this zone, edges are pushed away. Beyond, no node force.
    pub node_repulsion_radius: f32,

    /// Edge-edge equilibrium distance: transition point between repulsion and attraction.
    /// Below this distance: repulsion. Above: attraction (up to max_interaction_range).
    pub edge_equilibrium_distance: f32,

    // === NEW: Improved segment model ===

    /// Number of segments per edge (constant, not length-dependent).
    /// More segments = smoother curves but more computation.
    /// Default: 32
    pub segments_per_edge: u32,

    /// Minimum segment length to prevent collapse.
    /// Segments won't contract below this length.
    /// Default: 2.0 pixels
    pub min_segment_length: f32,

    /// Contraction strength - how strongly segments try to become shorter.
    /// Higher values = tighter cables.
    /// Default: 50.0
    pub contraction_strength: f32,

    /// Extra contraction in curved regions.
    /// Creates more detail in curves, less in straight sections.
    /// Default: 30.0
    pub curvature_contraction: f32,

    /// Hard collision distance from node surface.
    /// Vertices cannot get closer than this to any node.
    /// Default: 10.0 pixels
    pub node_wrap_distance: f32,

    /// Target distance between parallel edges (bundling).
    /// Edges will repel if closer, attract if further (up to edge_attraction_range).
    /// Default: 5.0 pixels
    pub edge_bundle_distance: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            spring_stiffness: 1500.0, // Very stiff springs - snappy response
            damping: 0.75,            // Lower damping - more responsive
            rest_length: 10.0,        // Short segments = many vertices
            node_repulsion: 3000.0,   // Strong repulsion near node surface
            edge_repulsion: 400.0,    // Edge-edge repulsion
            max_velocity: 2000.0,     // Much higher max velocity for snappy feel
            substeps: 8,              // More substeps for stability at higher speeds
            fixed_dt: 1.0 / 60.0,     // 60 Hz physics - larger timesteps
            tension_factor: 0.6,      // Edges want to be 60% of natural length (very taut)
            path_attraction: 5.0,     // Moderate path attraction
            inside_node_force: 20000.0, // Extreme push when inside nodes
            edge_attraction: 15.0,    // Moderate attraction between edges
            gravity: 15.0,            // Noticeable gravity for cable droop
            bending_stiffness: 150.0, // High bending stiffness - smooth curves
            pin_suction: 250.0,       // Strong pin suction - "slurping spaghetti"
            // SDF-based force model
            max_interaction_range: 100.0,    // No forces beyond 100px
            node_repulsion_radius: 20.0,     // Repel edges up to 20px from node surface
            edge_equilibrium_distance: 10.0, // <10px repel, >10px attract
            // Improved segment model
            segments_per_edge: 32,           // Constant 32 segments per edge
            min_segment_length: 2.0,         // Minimum 2px per segment
            contraction_strength: 300.0,     // HIGH contraction - snappy cable snap-back
            curvature_contraction: 100.0,    // Strong extra contraction in curves
            node_wrap_distance: 10.0,        // 10px hard collision boundary
            edge_bundle_distance: 5.0,       // 5px between bundled edges
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

    /// Whether to use GPU compute shader for physics (much faster).
    /// If false, falls back to CPU simulation.
    pub use_gpu: bool,

    /// Number of physics steps pending for GPU dispatch.
    /// Accumulated in tick_physics(), consumed in prepare().
    /// Uses Cell for interior mutability (draw() has immutable ref).
    pub pending_steps: std::cell::Cell<u32>,
}

impl EdgePhysicsState {
    /// Create a new physics state.
    pub fn new() -> Self {
        Self {
            enabled: true,
            config: PhysicsConfig::default(),
            accumulated_time: 0.0,
            gpu_dirty: true,
            use_gpu: true, // Default to GPU physics for performance
            pending_steps: std::cell::Cell::new(0),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: PhysicsConfig) -> Self {
        Self {
            enabled: true,
            config,
            accumulated_time: 0.0,
            gpu_dirty: true,
            use_gpu: true,
            pending_steps: std::cell::Cell::new(0),
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

/// Calculate repulsion force from node using SDF.
///
/// Force profile:
/// - Inside node (dist < 0): Strong exponential repulsion outward
/// - 0 to repulsion_radius: Quadratic falloff to zero
/// - Beyond repulsion_radius: No force
///
/// This simplified model only pushes edges away from nodes,
/// with no attraction component.
pub fn node_sdf_force(
    vertex: WorldPoint,
    node_pos: WorldVector,
    node_size: WorldVector,
    corner_radius: f32,
    inside_force: f32,
    outside_repulsion: f32,
    repulsion_radius: f32,
) -> WorldVector {
    let center = WorldPoint::new(
        node_pos.x + node_size.x * 0.5,
        node_pos.y + node_size.y * 0.5,
    );
    let half_size = WorldVector::new(node_size.x * 0.5, node_size.y * 0.5);

    let dist = sd_rounded_box(vertex, center, half_size, corner_radius);
    let gradient = sd_rounded_box_gradient(vertex, center, half_size, corner_radius);

    if dist < 0.0 {
        // Inside node: extremely strong push outward
        // Force scales quadratically with depth - edges MUST escape
        let depth = -dist;
        // Quadratic scaling: deeper = much stronger force
        // At 10px deep: 1 + 10 = 11x
        // At 50px deep: 1 + 250 = 251x
        let scale = 1.0 + (depth * depth) / 10.0;
        let force_magnitude = inside_force * scale;
        WorldVector::new(gradient.x * force_magnitude, gradient.y * force_magnitude)
    } else if dist < repulsion_radius {
        // 0 to repulsion_radius: quadratic falloff to zero
        let t = dist / repulsion_radius;
        let force_magnitude = outside_repulsion * (1.0 - t) * (1.0 - t);
        WorldVector::new(gradient.x * force_magnitude, gradient.y * force_magnitude)
    } else {
        // Beyond repulsion_radius: no force
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

/// Calculate edge-edge interaction force using SDF-like model.
///
/// Force profile:
/// - 0 to equilibrium_distance: Quadratic repulsion (prevents overlap)
/// - equilibrium_distance: Force = 0 (transition point)
/// - equilibrium_distance to max_range: Quadratic attraction falloff to zero (bundling)
/// - Beyond max_range: No force (optimization cutoff)
///
/// This creates natural edge bundling behavior where edges
/// maintain minimum spacing but are gently attracted together.
pub fn edge_interaction_force(
    vertex: WorldPoint,
    other_vertex: WorldPoint,
    repulsion_strength: f32,
    equilibrium_distance: f32,
    attraction_strength: f32,
    max_range: f32,
) -> WorldVector {
    let dx = vertex.x - other_vertex.x;
    let dy = vertex.y - other_vertex.y;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < 0.001 {
        return WorldVector::zero();
    }

    let dir_x = dx / dist;
    let dir_y = dy / dist;

    if dist < equilibrium_distance {
        // Short range repulsion (quadratic falloff toward equilibrium)
        let t = 1.0 - dist / equilibrium_distance;
        let force_magnitude = repulsion_strength * t * t;
        WorldVector::new(dir_x * force_magnitude, dir_y * force_magnitude)
    } else if dist < max_range {
        // Medium range attraction (quadratic falloff toward max_range)
        // At equilibrium_distance: force = attraction_strength
        // At max_range: force = 0
        let t = (dist - equilibrium_distance) / (max_range - equilibrium_distance);
        let force_magnitude = -attraction_strength * (1.0 - t) * (1.0 - t);
        WorldVector::new(dir_x * force_magnitude, dir_y * force_magnitude)
    } else {
        // Beyond max_range: no force
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

// ============================================================================
// IMPROVED SEGMENT MODEL FORCES
// ============================================================================

/// Contraction force - segments try to become shorter.
/// Force is proportional to current segment length (longer = stronger pull).
pub fn contraction_force(
    current: WorldPoint,
    neighbor: WorldPoint,
    contraction_strength: f32,
    rest_length: f32,
    min_segment_length: f32,
) -> WorldVector {
    let dx = neighbor.x - current.x;
    let dy = neighbor.y - current.y;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < min_segment_length || dist < 0.001 {
        return WorldVector::zero();
    }

    let direction_x = dx / dist;
    let direction_y = dy / dist;

    // Force proportional to length (longer segments pull harder)
    let length_factor = dist / rest_length;
    let force_magnitude = contraction_strength * length_factor;

    WorldVector::new(direction_x * force_magnitude, direction_y * force_magnitude)
}

/// Calculate curvature at a vertex (how bent the edge is here).
/// Returns 0.0 for straight, 1.0 for 90 degree bend, 2.0 for hairpin.
fn calculate_curvature(prev: WorldPoint, current: WorldPoint, next: WorldPoint) -> f32 {
    let v1_x = current.x - prev.x;
    let v1_y = current.y - prev.y;
    let v2_x = next.x - current.x;
    let v2_y = next.y - current.y;

    let len1 = (v1_x * v1_x + v1_y * v1_y).sqrt();
    let len2 = (v2_x * v2_x + v2_y * v2_y).sqrt();

    if len1 < 0.001 || len2 < 0.001 {
        return 0.0;
    }

    // Normalize and compute dot product
    let d = (v1_x / len1) * (v2_x / len2) + (v1_y / len1) * (v2_y / len2);

    // curvature = 1 - cos(angle)
    1.0 - d
}

/// Extra contraction force in curved regions.
/// Creates more detail in curves, less vertices in straight sections.
pub fn curvature_contraction_force(
    prev: WorldPoint,
    current: WorldPoint,
    next: WorldPoint,
    curvature_contraction: f32,
) -> WorldVector {
    let curvature = calculate_curvature(prev, current, next);
    let midpoint_x = (prev.x + next.x) * 0.5;
    let midpoint_y = (prev.y + next.y) * 0.5;
    let to_midpoint_x = midpoint_x - current.x;
    let to_midpoint_y = midpoint_y - current.y;
    let dist = (to_midpoint_x * to_midpoint_x + to_midpoint_y * to_midpoint_y).sqrt();

    if dist < 0.001 {
        return WorldVector::zero();
    }

    // Stronger pull toward midpoint at curves
    let force_magnitude = curvature_contraction * curvature;
    WorldVector::new(
        (to_midpoint_x / dist) * force_magnitude,
        (to_midpoint_y / dist) * force_magnitude,
    )
}

/// Apply hard node collision constraint.
/// Returns corrected position that maintains minimum distance from a node.
pub fn apply_node_collision(
    pos: WorldPoint,
    node_position: WorldPoint,
    node_size: WorldVector,
    corner_radius: f32,
    wrap_distance: f32,
) -> WorldPoint {
    let node_center_x = node_position.x + node_size.x * 0.5;
    let node_center_y = node_position.y + node_size.y * 0.5;
    let local_x = pos.x - node_center_x;
    let local_y = pos.y - node_center_y;
    let half_w = node_size.x * 0.5 - corner_radius;
    let half_h = node_size.y * 0.5 - corner_radius;

    // SDF calculation
    let q_x = local_x.abs() - half_w;
    let q_y = local_y.abs() - half_h;

    let outside_dist = (q_x.max(0.0) * q_x.max(0.0) + q_y.max(0.0) * q_y.max(0.0)).sqrt();
    let inside_dist = q_x.max(q_y).min(0.0);
    let sdf = outside_dist + inside_dist - corner_radius;

    if sdf >= wrap_distance {
        return pos; // Far enough away
    }

    // Calculate gradient (direction pointing outward)
    let gradient_x;
    let gradient_y;

    if q_x > 0.0 && q_y > 0.0 {
        // Corner region
        let corner_dist = (q_x * q_x + q_y * q_y).sqrt();
        if corner_dist < 0.001 {
            gradient_x = local_x.signum();
            gradient_y = local_y.signum();
        } else {
            gradient_x = (local_x.signum() * q_x) / corner_dist;
            gradient_y = (local_y.signum() * q_y) / corner_dist;
        }
    } else if q_x > q_y {
        // Left/right edge
        gradient_x = local_x.signum();
        gradient_y = 0.0;
    } else {
        // Top/bottom edge
        gradient_x = 0.0;
        gradient_y = local_y.signum();
    }

    // Push out by the penetration amount
    let penetration = wrap_distance - sdf;
    WorldPoint::new(pos.x + gradient_x * penetration, pos.y + gradient_y * penetration)
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
        // New SDF-based force model parameters
        assert_eq!(config.max_interaction_range, 100.0);
        assert_eq!(config.node_repulsion_radius, 15.0);
        assert_eq!(config.edge_equilibrium_distance, 10.0);
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

    // === Node SDF Force Tests ===

    #[test]
    fn test_node_sdf_force_inside() {
        // Vertex inside node should have strong repulsion outward
        // Note: Use off-center position, as gradient is zero at exact center
        let vertex = WorldPoint::new(70.0, 50.0); // Inside a 100x100 node at (0,0), off-center
        let node_pos = WorldVector::new(0.0, 0.0);
        let node_size = WorldVector::new(100.0, 100.0);

        let force = node_sdf_force(
            vertex,
            node_pos,
            node_size,
            5.0,     // corner radius
            3000.0,  // inside_force
            600.0,   // outside_repulsion
            15.0,    // repulsion_radius
        );

        // Should have non-zero force pushing outward (toward positive x since vertex is right of center)
        let magnitude = (force.x * force.x + force.y * force.y).sqrt();
        assert!(magnitude > 0.0, "Inside node should have repulsion force");
        assert!(force.x > 0.0, "Force should push toward nearest edge (right)");
    }

    #[test]
    fn test_node_sdf_force_near_surface() {
        // Vertex just outside node surface (within repulsion_radius)
        let vertex = WorldPoint::new(55.0, 50.0); // 5px outside right edge of 100x100 node
        let node_pos = WorldVector::new(0.0, 0.0);
        let node_size = WorldVector::new(100.0, 100.0);

        let force = node_sdf_force(
            vertex,
            node_pos,
            node_size,
            5.0,     // corner radius
            3000.0,  // inside_force
            600.0,   // outside_repulsion
            15.0,    // repulsion_radius
        );

        // Should have repulsion pushing away (positive x)
        assert!(force.x > 0.0, "Near node surface should push away");
    }

    #[test]
    fn test_node_sdf_force_beyond_range() {
        // Vertex far from node (beyond repulsion_radius) should have no force
        let vertex = WorldPoint::new(200.0, 50.0); // Far from node
        let node_pos = WorldVector::new(0.0, 0.0);
        let node_size = WorldVector::new(100.0, 100.0);

        let force = node_sdf_force(
            vertex,
            node_pos,
            node_size,
            5.0,     // corner radius
            3000.0,  // inside_force
            600.0,   // outside_repulsion
            15.0,    // repulsion_radius
        );

        // Should be zero
        assert!(force.x.abs() < 0.001, "Beyond range should have no force");
        assert!(force.y.abs() < 0.001, "Beyond range should have no force");
    }

    // === Edge-Edge Interaction Force Tests ===

    #[test]
    fn test_edge_interaction_close_repulsion() {
        // Vertices closer than equilibrium distance should repel
        let v1 = WorldPoint::new(0.0, 0.0);
        let v2 = WorldPoint::new(5.0, 0.0); // 5px apart (< 10px equilibrium)

        let force = edge_interaction_force(
            v1,
            v2,
            300.0,  // repulsion_strength
            10.0,   // equilibrium_distance
            10.0,   // attraction_strength
            100.0,  // max_range
        );

        // Should repel (negative x, pushing v1 away from v2)
        assert!(force.x < 0.0, "Close edges should repel");
    }

    #[test]
    fn test_edge_interaction_at_equilibrium() {
        // At exactly equilibrium distance, force should be ~0
        let v1 = WorldPoint::new(0.0, 0.0);
        let v2 = WorldPoint::new(10.0, 0.0); // Exactly at equilibrium

        let force = edge_interaction_force(
            v1,
            v2,
            300.0,  // repulsion_strength
            10.0,   // equilibrium_distance
            10.0,   // attraction_strength
            100.0,  // max_range
        );

        // Force should be very small (transitioning from repel to attract)
        let magnitude = (force.x * force.x + force.y * force.y).sqrt();
        assert!(magnitude < 11.0, "At equilibrium force should be small");
    }

    #[test]
    fn test_edge_interaction_medium_attraction() {
        // Vertices between equilibrium and max_range should attract
        let v1 = WorldPoint::new(0.0, 0.0);
        let v2 = WorldPoint::new(50.0, 0.0); // 50px apart (10-100px range)

        let force = edge_interaction_force(
            v1,
            v2,
            300.0,  // repulsion_strength
            10.0,   // equilibrium_distance
            10.0,   // attraction_strength
            100.0,  // max_range
        );

        // Should attract (positive x, pulling v1 toward v2)
        assert!(force.x > 0.0, "Medium range edges should attract");
    }

    #[test]
    fn test_edge_interaction_far_no_force() {
        // Vertices beyond max_range should have no interaction
        let v1 = WorldPoint::new(0.0, 0.0);
        let v2 = WorldPoint::new(150.0, 0.0); // 150px apart (> 100px max_range)

        let force = edge_interaction_force(
            v1,
            v2,
            300.0,  // repulsion_strength
            10.0,   // equilibrium_distance
            10.0,   // attraction_strength
            100.0,  // max_range
        );

        // Should be zero
        assert!(force.x.abs() < 0.001, "Beyond max_range should have no force");
        assert!(force.y.abs() < 0.001, "Beyond max_range should have no force");
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
