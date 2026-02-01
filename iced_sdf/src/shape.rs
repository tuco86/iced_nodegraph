//! SDF shape primitives and CSG operations.
//!
//! Provides a builder API for constructing signed distance field shapes
//! that can be combined using boolean operations.

use glam::Vec2;
use std::ops::{BitOr, Sub};

/// An SDF shape node in the CSG tree.
///
/// Each node is either a primitive shape or a boolean operation
/// combining other shapes.
#[derive(Clone, Debug)]
pub enum SdfNode {
    // Primitives (all return distance and arc-length parameter u)
    /// Circle centered at `center` with `radius`.
    Circle { center: Vec2, radius: f32 },
    /// Axis-aligned box centered at `center` with `half_size`.
    Box { center: Vec2, half_size: Vec2 },
    /// Rounded box with corner radius.
    RoundedBox {
        center: Vec2,
        half_size: Vec2,
        corner_radius: f32,
    },
    /// Line segment from `a` to `b`.
    Line { a: Vec2, b: Vec2 },
    /// Cubic bezier curve with 4 control points.
    Bezier {
        p0: Vec2,
        p1: Vec2,
        p2: Vec2,
        p3: Vec2,
    },

    // Boolean operations
    /// Union of two shapes (min distance).
    Union(Box<SdfNode>, Box<SdfNode>),
    /// Subtraction: first shape minus second (max(a, -b)).
    Subtract(Box<SdfNode>, Box<SdfNode>),
    /// Intersection of two shapes (max distance).
    Intersect(Box<SdfNode>, Box<SdfNode>),
    /// Smooth union with blend factor.
    SmoothUnion {
        a: Box<SdfNode>,
        b: Box<SdfNode>,
        k: f32,
    },
    /// Smooth subtraction with blend factor.
    SmoothSubtract {
        a: Box<SdfNode>,
        b: Box<SdfNode>,
        k: f32,
    },

    // Modifiers
    /// Expand/contract shape by offset.
    Round { node: Box<SdfNode>, radius: f32 },
    /// Create outline (annulus) from shape.
    Onion { node: Box<SdfNode>, thickness: f32 },
}

/// Builder for SDF shapes with method chaining.
#[derive(Clone, Debug)]
pub struct Sdf {
    root: SdfNode,
}

impl Sdf {
    // Primitive constructors

    /// Create a circle SDF.
    pub fn circle(center: impl Into<Vec2>, radius: f32) -> Self {
        Self {
            root: SdfNode::Circle {
                center: center.into(),
                radius,
            },
        }
    }

    /// Create a box SDF (axis-aligned rectangle).
    pub fn rect(center: impl Into<Vec2>, half_size: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Box {
                center: center.into(),
                half_size: half_size.into(),
            },
        }
    }

    /// Create a rounded box SDF.
    pub fn rounded_box(
        center: impl Into<Vec2>,
        half_size: impl Into<Vec2>,
        corner_radius: f32,
    ) -> Self {
        Self {
            root: SdfNode::RoundedBox {
                center: center.into(),
                half_size: half_size.into(),
                corner_radius,
            },
        }
    }

    /// Create a line segment SDF.
    pub fn line(a: impl Into<Vec2>, b: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Line {
                a: a.into(),
                b: b.into(),
            },
        }
    }

    /// Create a cubic bezier curve SDF.
    pub fn bezier(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
        p3: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::Bezier {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
                p3: p3.into(),
            },
        }
    }

    // Boolean operations

    /// Union with another shape.
    pub fn union(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Union(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Subtract another shape from this one.
    pub fn subtract(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Subtract(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Intersect with another shape.
    pub fn intersect(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Intersect(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Smooth union with blend factor k.
    pub fn union_smooth(self, other: Sdf, k: f32) -> Self {
        Self {
            root: SdfNode::SmoothUnion {
                a: Box::new(self.root),
                b: Box::new(other.root),
                k,
            },
        }
    }

    /// Smooth subtraction with blend factor k.
    pub fn subtract_smooth(self, other: Sdf, k: f32) -> Self {
        Self {
            root: SdfNode::SmoothSubtract {
                a: Box::new(self.root),
                b: Box::new(other.root),
                k,
            },
        }
    }

    // Modifiers

    /// Round the shape by expanding its boundary.
    pub fn round(self, radius: f32) -> Self {
        Self {
            root: SdfNode::Round {
                node: Box::new(self.root),
                radius,
            },
        }
    }

    /// Create an outline (hollow) version of the shape.
    pub fn onion(self, thickness: f32) -> Self {
        Self {
            root: SdfNode::Onion {
                node: Box::new(self.root),
                thickness,
            },
        }
    }

    /// Consume the builder and return the root node.
    pub fn into_node(self) -> SdfNode {
        self.root
    }

    /// Get a reference to the root node.
    pub fn node(&self) -> &SdfNode {
        &self.root
    }
}

// Operator overloads for ergonomic API

impl BitOr for Sdf {
    type Output = Sdf;

    /// Union operator: `a | b`
    fn bitor(self, rhs: Sdf) -> Sdf {
        self.union(rhs)
    }
}

impl Sub for Sdf {
    type Output = Sdf;

    /// Subtraction operator: `a - b`
    fn sub(self, rhs: Sdf) -> Sdf {
        self.subtract(rhs)
    }
}

impl From<SdfNode> for Sdf {
    fn from(node: SdfNode) -> Self {
        Self { root: node }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_construction() {
        let sdf = Sdf::circle([100.0, 50.0], 25.0);
        match sdf.node() {
            SdfNode::Circle { center, radius } => {
                assert_eq!(*center, Vec2::new(100.0, 50.0));
                assert_eq!(*radius, 25.0);
            }
            _ => panic!("Expected Circle"),
        }
    }

    #[test]
    fn test_union_operator() {
        let a = Sdf::circle([0.0, 0.0], 10.0);
        let b = Sdf::circle([20.0, 0.0], 10.0);
        let combined = a | b;

        match combined.node() {
            SdfNode::Union(_, _) => {}
            _ => panic!("Expected Union"),
        }
    }

    #[test]
    fn test_subtract_operator() {
        let a = Sdf::rect([0.0, 0.0], [50.0, 50.0]);
        let b = Sdf::circle([0.0, 0.0], 25.0);
        let result = a - b;

        match result.node() {
            SdfNode::Subtract(_, _) => {}
            _ => panic!("Expected Subtract"),
        }
    }

    #[test]
    fn test_method_chaining() {
        let shape = Sdf::rounded_box([0.0, 0.0], [100.0, 50.0], 8.0)
            .subtract(Sdf::circle([50.0, 0.0], 10.0))
            .subtract(Sdf::circle([-50.0, 0.0], 10.0))
            .round(2.0);

        // Verify we can build complex shapes
        match shape.node() {
            SdfNode::Round { .. } => {}
            _ => panic!("Expected Round"),
        }
    }
}
