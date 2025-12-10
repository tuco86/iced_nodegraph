//! Force-directed graph layout using Fruchterman-Reingold algorithm.

use iced::Point;
use iced_nodegraph::PinReference;

/// Physics simulation parameters
const REPULSION: f32 = 8000.0;
const ATTRACTION: f32 = 0.005;
const IDEAL_EDGE_LENGTH: f32 = 120.0;
const DAMPING: f32 = 0.85;
const MAX_VELOCITY: f32 = 30.0;
const MIN_DISTANCE: f32 = 1.0;
const CONVERGENCE_THRESHOLD: f32 = 0.5;
const MAX_ITERATIONS: usize = 200;

/// Node with physics state for force-directed layout.
struct PhysicsNode {
    position: Point,
    velocity: (f32, f32),
}

/// Force-directed layout calculator.
pub struct ForceDirectedLayout {
    nodes: Vec<PhysicsNode>,
    edges: Vec<(usize, usize)>,
}

impl ForceDirectedLayout {
    /// Creates a new layout from initial positions and edges.
    pub fn new(positions: Vec<Point>, edges: &[(PinReference, PinReference)]) -> Self {
        let nodes = positions
            .into_iter()
            .map(|pos| PhysicsNode {
                position: pos,
                velocity: (0.0, 0.0),
            })
            .collect();

        let edges = edges
            .iter()
            .map(|(from, to)| (from.node_id, to.node_id))
            .collect();

        Self { nodes, edges }
    }

    /// Runs simulation until convergence or max iterations.
    pub fn simulate(&mut self) -> Vec<Point> {
        for iteration in 0..MAX_ITERATIONS {
            let max_displacement = self.step();

            if max_displacement < CONVERGENCE_THRESHOLD {
                println!(
                    "Force-directed layout converged after {} iterations",
                    iteration + 1
                );
                break;
            }
        }

        self.nodes.iter().map(|n| n.position).collect()
    }

    /// Performs a single simulation step, returns max displacement.
    fn step(&mut self) -> f32 {
        let n = self.nodes.len();
        let mut forces = vec![(0.0f32, 0.0f32); n];

        // Repulsion between all node pairs
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.nodes[j].position.x - self.nodes[i].position.x;
                let dy = self.nodes[j].position.y - self.nodes[i].position.y;
                let dist_sq = dx * dx + dy * dy;
                let dist = dist_sq.sqrt().max(MIN_DISTANCE);

                // Repulsion force: F = k / d^2
                let force = REPULSION / dist_sq.max(MIN_DISTANCE * MIN_DISTANCE);
                let fx = force * dx / dist;
                let fy = force * dy / dist;

                forces[i].0 -= fx;
                forces[i].1 -= fy;
                forces[j].0 += fx;
                forces[j].1 += fy;
            }
        }

        // Attraction along edges
        for (a, b) in &self.edges {
            let dx = self.nodes[*b].position.x - self.nodes[*a].position.x;
            let dy = self.nodes[*b].position.y - self.nodes[*a].position.y;
            let dist = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);

            // Attraction force: F = k * (d - ideal)
            let force = ATTRACTION * (dist - IDEAL_EDGE_LENGTH);
            let fx = force * dx / dist;
            let fy = force * dy / dist;

            forces[*a].0 += fx;
            forces[*a].1 += fy;
            forces[*b].0 -= fx;
            forces[*b].1 -= fy;
        }

        // Apply forces with velocity and damping
        let mut max_displacement: f32 = 0.0;

        for (i, node) in self.nodes.iter_mut().enumerate() {
            // Update velocity
            node.velocity.0 = (node.velocity.0 + forces[i].0) * DAMPING;
            node.velocity.1 = (node.velocity.1 + forces[i].1) * DAMPING;

            // Clamp velocity
            let speed =
                (node.velocity.0 * node.velocity.0 + node.velocity.1 * node.velocity.1).sqrt();
            if speed > MAX_VELOCITY {
                node.velocity.0 *= MAX_VELOCITY / speed;
                node.velocity.1 *= MAX_VELOCITY / speed;
            }

            // Update position
            node.position.x += node.velocity.0;
            node.position.y += node.velocity.1;

            // Track max displacement
            let displacement =
                (node.velocity.0 * node.velocity.0 + node.velocity.1 * node.velocity.1).sqrt();
            max_displacement = max_displacement.max(displacement);
        }

        max_displacement
    }
}
