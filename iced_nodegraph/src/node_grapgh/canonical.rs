//! Canonical state structures for the node graph.
//!
//! These structures represent the authoritative state of the graph.
//! GPU buffers are views into this state, updated incrementally via dirty tracking.

use super::euclid::{WorldPoint, WorldSize, WorldVector};
use crate::style::{EdgeStyle, NodeStyle};
use crate::PinReference;
use iced::Color;
use std::ops::Range;

/// Canonical state for a single node.
///
/// This structure contains all data needed to render a node,
/// stored in a format that allows efficient dirty tracking.
#[derive(Debug, Clone)]
pub struct CanonicalNode {
    /// Position of the node's top-left corner in world coordinates.
    pub position: WorldVector,
    /// Size of the node.
    pub size: WorldSize,
    /// Visual style (colors, border, corner radius).
    pub style: NodeStyle,
    /// Range of pin indices in the canonical pins array.
    pub pin_range: Range<usize>,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
}

impl CanonicalNode {
    /// Create a new canonical node.
    pub fn new(position: WorldVector, size: WorldSize, style: NodeStyle, pin_range: Range<usize>) -> Self {
        Self {
            position,
            size,
            style,
            pin_range,
            opacity: 1.0,
        }
    }

    /// Get the center point of the node.
    pub fn center(&self) -> WorldPoint {
        WorldPoint::new(
            self.position.x + self.size.width / 2.0,
            self.position.y + self.size.height / 2.0,
        )
    }
}

/// Canonical state for a single pin.
#[derive(Debug, Clone, Copy)]
pub struct CanonicalPin {
    /// Index of the node this pin belongs to.
    pub node_id: usize,
    /// Offset from node's top-left corner.
    pub offset: WorldVector,
    /// Which side of the node (0=Left, 1=Right, 2=Top, 3=Bottom, 4=Row).
    pub side: u32,
    /// Pin radius for hit testing and rendering.
    pub radius: f32,
    /// Pin color.
    pub color: Color,
    /// Direction (0=Input, 1=Output, 2=Both).
    pub direction: u32,
    /// Pin type identifier (for connection validation).
    pub pin_type: Option<&'static str>,
}

/// Canonical state for a single edge.
#[derive(Debug, Clone)]
pub struct CanonicalEdge {
    /// Source pin reference.
    pub from: PinReference,
    /// Target pin reference.
    pub to: PinReference,
    /// Range of vertex indices in the canonical vertices array.
    pub vertex_range: Range<usize>,
    /// Visual style.
    pub style: Option<EdgeStyle>,
}

impl CanonicalEdge {
    /// Create a new canonical edge.
    pub fn new(from: PinReference, to: PinReference, vertex_range: Range<usize>) -> Self {
        Self {
            from,
            to,
            vertex_range,
            style: None,
        }
    }

    /// Create with style.
    pub fn with_style(mut self, style: EdgeStyle) -> Self {
        self.style = Some(style);
        self
    }
}

/// Canonical state for a single edge vertex (for physics simulation).
#[derive(Debug, Clone, Copy)]
pub struct CanonicalVertex {
    /// Position in world coordinates.
    pub position: WorldPoint,
    /// Velocity for physics simulation.
    pub velocity: WorldVector,
    /// Mass for physics simulation.
    pub mass: f32,
    /// Whether this vertex is anchored (fixed to a pin).
    pub is_anchored: bool,
    /// Index of the edge this vertex belongs to.
    pub edge_id: usize,
    /// Index within the edge's vertex array.
    pub vertex_index: usize,
}

impl CanonicalVertex {
    /// Create a new anchored vertex (for edge endpoints).
    pub fn anchored(position: WorldPoint, edge_id: usize, vertex_index: usize) -> Self {
        Self {
            position,
            velocity: WorldVector::zero(),
            mass: 1.0,
            is_anchored: true,
            edge_id,
            vertex_index,
        }
    }

    /// Create a new free vertex (for physics simulation).
    pub fn free(position: WorldPoint, edge_id: usize, vertex_index: usize) -> Self {
        Self {
            position,
            velocity: WorldVector::zero(),
            mass: 1.0,
            is_anchored: false,
            edge_id,
            vertex_index,
        }
    }
}

/// Complete canonical state for the node graph.
///
/// This is the authoritative data store. GPU buffers are derived from this
/// and updated incrementally based on dirty flags.
#[derive(Debug, Clone, Default)]
pub struct CanonicalState {
    /// All nodes in the graph.
    pub nodes: Vec<CanonicalNode>,
    /// All pins (flattened from all nodes).
    pub pins: Vec<CanonicalPin>,
    /// All edges in the graph.
    pub edges: Vec<CanonicalEdge>,
    /// All edge vertices (for physics simulation).
    pub vertices: Vec<CanonicalVertex>,
}

impl CanonicalState {
    /// Create an empty canonical state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get the number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get a node by ID.
    pub fn node(&self, id: usize) -> Option<&CanonicalNode> {
        self.nodes.get(id)
    }

    /// Get a mutable node by ID.
    pub fn node_mut(&mut self, id: usize) -> Option<&mut CanonicalNode> {
        self.nodes.get_mut(id)
    }

    /// Get an edge by ID.
    pub fn edge(&self, id: usize) -> Option<&CanonicalEdge> {
        self.edges.get(id)
    }

    /// Get a mutable edge by ID.
    pub fn edge_mut(&mut self, id: usize) -> Option<&mut CanonicalEdge> {
        self.edges.get_mut(id)
    }

    /// Calculate vertex count for an edge based on its length.
    ///
    /// Uses dynamic vertex allocation based on distance between endpoints.
    pub fn calculate_vertex_count(start: WorldPoint, end: WorldPoint, rest_length: f32) -> usize {
        let length = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
        let count = (length / rest_length).ceil() as usize;
        count.clamp(2, 32) // Minimum 2 (start/end), maximum 32
    }

    /// Initialize vertices for an edge with physics-based positions.
    ///
    /// Creates vertices evenly distributed between start and end pins.
    pub fn init_edge_vertices(
        &mut self,
        edge_id: usize,
        start_pos: WorldPoint,
        end_pos: WorldPoint,
        rest_length: f32,
    ) {
        let vertex_count = Self::calculate_vertex_count(start_pos, end_pos, rest_length);
        let vertex_start = self.vertices.len();

        for i in 0..vertex_count {
            let t = i as f32 / (vertex_count - 1).max(1) as f32;
            let pos = WorldPoint::new(
                start_pos.x + (end_pos.x - start_pos.x) * t,
                start_pos.y + (end_pos.y - start_pos.y) * t,
            );

            let is_anchored = i == 0 || i == vertex_count - 1;
            let vertex = if is_anchored {
                CanonicalVertex::anchored(pos, edge_id, i)
            } else {
                CanonicalVertex::free(pos, edge_id, i)
            };

            self.vertices.push(vertex);
        }

        // Update edge's vertex range
        if let Some(edge) = self.edges.get_mut(edge_id) {
            edge.vertex_range = vertex_start..self.vertices.len();
        }
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.pins.clear();
        self.edges.clear();
        self.vertices.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_vertex_count_short_edge() {
        let start = WorldPoint::new(0.0, 0.0);
        let end = WorldPoint::new(30.0, 0.0);
        let count = CanonicalState::calculate_vertex_count(start, end, 30.0);
        assert_eq!(count, 2); // Minimum is 2
    }

    #[test]
    fn test_calculate_vertex_count_long_edge() {
        let start = WorldPoint::new(0.0, 0.0);
        let end = WorldPoint::new(300.0, 0.0);
        let count = CanonicalState::calculate_vertex_count(start, end, 30.0);
        assert_eq!(count, 10);
    }

    #[test]
    fn test_calculate_vertex_count_max() {
        let start = WorldPoint::new(0.0, 0.0);
        let end = WorldPoint::new(10000.0, 0.0);
        let count = CanonicalState::calculate_vertex_count(start, end, 30.0);
        assert_eq!(count, 32); // Clamped to max
    }

    #[test]
    fn test_canonical_vertex_anchored() {
        let v = CanonicalVertex::anchored(WorldPoint::new(100.0, 200.0), 5, 0);
        assert!(v.is_anchored);
        assert_eq!(v.edge_id, 5);
        assert_eq!(v.vertex_index, 0);
    }

    #[test]
    fn test_canonical_vertex_free() {
        let v = CanonicalVertex::free(WorldPoint::new(100.0, 200.0), 5, 3);
        assert!(!v.is_anchored);
        assert_eq!(v.edge_id, 5);
        assert_eq!(v.vertex_index, 3);
    }

    #[test]
    fn test_init_edge_vertices() {
        let mut state = CanonicalState::new();
        state.edges.push(CanonicalEdge::new(
            PinReference::new(0, 0),
            PinReference::new(1, 0),
            0..0, // Will be updated
        ));

        let start = WorldPoint::new(0.0, 0.0);
        let end = WorldPoint::new(90.0, 0.0);
        state.init_edge_vertices(0, start, end, 30.0);

        // 90/30 = 3, ceil = 3 -> 3 vertices (start, middle, end)
        assert_eq!(state.vertices.len(), 3);
        assert!(state.vertices[0].is_anchored); // start
        assert!(!state.vertices[1].is_anchored); // middle
        assert!(state.vertices[2].is_anchored); // end

        // Check edge vertex range was updated
        assert_eq!(state.edges[0].vertex_range, 0..3);
    }
}
