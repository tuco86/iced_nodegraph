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
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            spring_stiffness: 500.0,
            damping: 0.95,
            rest_length: 30.0,
            node_repulsion: 1000.0,
            edge_repulsion: 200.0,
            repulsion_radius: 50.0,
            max_velocity: 500.0,
            substeps: 4,
            fixed_dt: 1.0 / 120.0, // 120 Hz physics
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

/// Calculate spring force between two positions.
///
/// Returns force vector applied to `from` position.
pub fn spring_force(from: WorldPoint, to: WorldPoint, stiffness: f32, rest_length: f32) -> WorldVector {
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
    let speed = (vertex.velocity.x * vertex.velocity.x + vertex.velocity.y * vertex.velocity.y).sqrt();
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
        assert_eq!(config.spring_stiffness, 500.0);
        assert_eq!(config.damping, 0.95);
        assert_eq!(config.rest_length, 30.0);
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
