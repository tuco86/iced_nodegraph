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
    /// Quadratic bezier curve with 3 control points.
    QuadBezier { p0: Vec2, p1: Vec2, p2: Vec2 },
    /// Ellipse with semi-axes.
    Ellipse { ab: Vec2 },
    /// Arbitrary triangle with 3 vertices.
    Triangle { p0: Vec2, p1: Vec2, p2: Vec2 },
    /// Equilateral triangle with circumradius.
    EquilateralTriangle { radius: f32 },
    /// Isosceles triangle.
    IsoscelesTriangle { q: Vec2 },
    /// Rhombus (diamond) with half-diagonals.
    Rhombus { b: Vec2 },
    /// Trapezoid with half-widths and half-height.
    Trapezoid { r1: f32, r2: f32, he: f32 },
    /// Parallelogram with width, height, and skew.
    Parallelogram { wi: f32, he: f32, sk: f32 },
    /// Regular pentagon.
    Pentagon { radius: f32 },
    /// Regular hexagon.
    Hexagon { radius: f32 },
    /// Regular octagon.
    Octagon { radius: f32 },
    /// Six-pointed star (Star of David).
    Hexagram { radius: f32 },
    /// N-pointed star.
    Star { radius: f32, n: u32, m: f32 },
    /// Pie/sector shape.
    Pie { angle: f32, radius: f32 },
    /// Arc shape.
    Arc { angle: f32, ra: f32, rb: f32 },
    /// Disk with horizontal cut.
    CutDisk { radius: f32, h: f32 },
    /// Heart shape (unit-sized, scale with Round/expand).
    Heart,
    /// Egg shape.
    Egg { ra: f32, rb: f32 },
    /// Crescent moon.
    Moon { d: f32, ra: f32, rb: f32 },
    /// Vesica piscis (lens shape).
    Vesica { r: f32, d: f32 },
    /// Capsule with different end radii.
    UnevenCapsule { r1: f32, r2: f32, h: f32 },
    /// Oriented (rotated) box defined by endpoints and thickness.
    OrientedBox { a: Vec2, b: Vec2, thickness: f32 },
    /// Horseshoe shape.
    Horseshoe { angle: f32, radius: f32, w: Vec2 },
    /// Rounded X shape.
    RoundedX { w: f32, r: f32 },
    /// Cross/plus shape.
    Cross { b: Vec2, r: f32 },
    /// Parabola y = k*x^2.
    Parabola { k: f32 },
    /// Cool S shape (unit-sized).
    CoolS,
    /// Blobbycross shape.
    BlobbyCross { he: f32 },

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

    /// Create a quadratic bezier curve SDF.
    pub fn quad_bezier(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::QuadBezier {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
            },
        }
    }

    /// Create an ellipse SDF.
    pub fn ellipse(ab: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Ellipse { ab: ab.into() },
        }
    }

    /// Create an arbitrary triangle SDF from 3 vertices.
    pub fn triangle(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::Triangle {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
            },
        }
    }

    /// Create an equilateral triangle SDF.
    pub fn equilateral_triangle(radius: f32) -> Self {
        Self {
            root: SdfNode::EquilateralTriangle { radius },
        }
    }

    /// Create an isosceles triangle SDF.
    pub fn isosceles_triangle(q: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::IsoscelesTriangle { q: q.into() },
        }
    }

    /// Create a rhombus (diamond) SDF.
    pub fn rhombus(b: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Rhombus { b: b.into() },
        }
    }

    /// Create a trapezoid SDF.
    pub fn trapezoid(r1: f32, r2: f32, he: f32) -> Self {
        Self {
            root: SdfNode::Trapezoid { r1, r2, he },
        }
    }

    /// Create a parallelogram SDF.
    pub fn parallelogram(wi: f32, he: f32, sk: f32) -> Self {
        Self {
            root: SdfNode::Parallelogram { wi, he, sk },
        }
    }

    /// Create a regular pentagon SDF.
    pub fn pentagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Pentagon { radius },
        }
    }

    /// Create a regular hexagon SDF.
    pub fn hexagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Hexagon { radius },
        }
    }

    /// Create a regular octagon SDF.
    pub fn octagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Octagon { radius },
        }
    }

    /// Create a hexagram (Star of David) SDF.
    pub fn hexagram(radius: f32) -> Self {
        Self {
            root: SdfNode::Hexagram { radius },
        }
    }

    /// Create an n-pointed star SDF.
    pub fn star(radius: f32, n: u32, m: f32) -> Self {
        Self {
            root: SdfNode::Star { radius, n, m },
        }
    }

    /// Create a pie/sector SDF. Angle is the half-aperture in radians.
    pub fn pie(angle: f32, radius: f32) -> Self {
        Self {
            root: SdfNode::Pie { angle, radius },
        }
    }

    /// Create an arc SDF. Angle is the half-aperture in radians.
    pub fn arc(angle: f32, ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Arc { angle, ra, rb },
        }
    }

    /// Create a cut disk SDF.
    pub fn cut_disk(radius: f32, h: f32) -> Self {
        Self {
            root: SdfNode::CutDisk { radius, h },
        }
    }

    /// Create a heart shape SDF (unit-sized, use round/expand to scale).
    pub fn heart() -> Self {
        Self {
            root: SdfNode::Heart,
        }
    }

    /// Create an egg shape SDF.
    pub fn egg(ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Egg { ra, rb },
        }
    }

    /// Create a crescent moon SDF.
    pub fn moon(d: f32, ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Moon { d, ra, rb },
        }
    }

    /// Create a vesica piscis (lens) SDF.
    pub fn vesica(r: f32, d: f32) -> Self {
        Self {
            root: SdfNode::Vesica { r, d },
        }
    }

    /// Create an uneven capsule SDF.
    pub fn uneven_capsule(r1: f32, r2: f32, h: f32) -> Self {
        Self {
            root: SdfNode::UnevenCapsule { r1, r2, h },
        }
    }

    /// Create an oriented (rotated) box SDF.
    pub fn oriented_box(a: impl Into<Vec2>, b: impl Into<Vec2>, thickness: f32) -> Self {
        Self {
            root: SdfNode::OrientedBox {
                a: a.into(),
                b: b.into(),
                thickness,
            },
        }
    }

    /// Create a horseshoe SDF. Angle is the half-aperture in radians.
    pub fn horseshoe(angle: f32, radius: f32, w: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Horseshoe {
                angle,
                radius,
                w: w.into(),
            },
        }
    }

    /// Create a rounded X SDF.
    pub fn rounded_x(w: f32, r: f32) -> Self {
        Self {
            root: SdfNode::RoundedX { w, r },
        }
    }

    /// Create a cross/plus SDF.
    pub fn cross(b: impl Into<Vec2>, r: f32) -> Self {
        Self {
            root: SdfNode::Cross { b: b.into(), r },
        }
    }

    /// Create a parabola SDF (y = k*x^2).
    pub fn parabola(k: f32) -> Self {
        Self {
            root: SdfNode::Parabola { k },
        }
    }

    /// Create a Cool S shape SDF (unit-sized).
    pub fn cool_s() -> Self {
        Self {
            root: SdfNode::CoolS,
        }
    }

    /// Create a blobby cross SDF.
    pub fn blobby_cross(he: f32) -> Self {
        Self {
            root: SdfNode::BlobbyCross { he },
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
