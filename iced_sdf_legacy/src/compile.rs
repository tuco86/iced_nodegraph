//! Compiles SDF tree to RPN (Reverse Polish Notation) for GPU evaluation.
//!
//! The GPU shader evaluates SDFs using a stack-based approach. This module
//! converts the tree structure into a linear array of operations.

use crate::pipeline::types::SdfOp;
use crate::shape::SdfNode;
use crate::pipeline::types::GpuVec4;

#[inline]
fn v4(x: f32, y: f32, z: f32, w: f32) -> GpuVec4 {
    GpuVec4::new(x, y, z, w)
}

const V4_ZERO: GpuVec4 = GpuVec4::ZERO;

/// Operation types for the shader.
/// Must match the constants in shader.wgsl.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpType {
    // Primitives
    Circle = 0,
    Box = 1,
    RoundedBox = 2,
    Line = 3,
    Bezier = 4,
    Ellipse = 5,
    Triangle = 6,
    EquilateralTriangle = 7,
    IsoscelesTriangle = 8,
    Rhombus = 9,
    Trapezoid = 10,
    Parallelogram = 11,
    Pentagon = 12,
    Hexagon = 13,
    Octagon = 14,
    Hexagram = 15,

    // Operations (16-31)
    Union = 16,
    Subtract = 17,
    Intersect = 18,
    SmoothUnion = 19,
    SmoothSubtract = 20,

    // More primitives (21-31)
    Star = 21,
    Pie = 22,
    Arc = 23,
    CutDisk = 24,
    Heart = 25,
    Egg = 26,
    Moon = 27,
    Vesica = 28,
    UnevenCapsule = 29,
    OrientedBox = 30,
    Horseshoe = 31,

    // Modifiers (32-33)
    Round = 32,
    Onion = 33,

    // More primitives (34+)
    RoundedX = 34,
    Cross = 35,
    QuadBezier = 36,
    Parabola = 37,
    CoolS = 38,
    BlobbyCross = 39,

    // Pattern modifiers (40+)
    Dash = 40,
    Arrow = 41,
}

#[cfg(test)]
fn compile(node: &SdfNode) -> Vec<SdfOp> {
    let mut ops = Vec::new();
    compile_into(node, &mut ops);
    ops
}

/// Compile an SDF tree into RPN format, reusing the provided Vec.
///
/// Clears `ops` before compiling. Use this to avoid per-frame allocations.
pub(crate) fn compile_into(node: &SdfNode, ops: &mut Vec<SdfOp>) {
    ops.clear();
    compile_node(node, ops);
}

/// Push a primitive SdfOp with the given type and parameters.
macro_rules! push_prim {
    ($ops:expr, $op:expr, $p0:expr) => {
        $ops.push(SdfOp {
            op_type: $op as u32,
            param0: $p0,
            ..Default::default()
        })
    };
    ($ops:expr, $op:expr, $p0:expr, $p1:expr) => {
        $ops.push(SdfOp {
            op_type: $op as u32,
            param0: $p0,
            param1: $p1,
            ..Default::default()
        })
    };
}

/// Recursively compile a node in postfix order.
fn compile_node(node: &SdfNode, ops: &mut Vec<SdfOp>) {
    match node {
        // Primitives
        SdfNode::Circle { center, radius } => {
            push_prim!(ops, OpType::Circle, v4(center.x, center.y, *radius, 0.0));
        }
        SdfNode::Box { center, half_size } => {
            push_prim!(ops, OpType::Box, v4(center.x, center.y, half_size.x, half_size.y));
        }
        SdfNode::RoundedBox { center, half_size, corner_radius } => {
            push_prim!(ops, OpType::RoundedBox,
                v4(center.x, center.y, half_size.x, half_size.y),
                v4(*corner_radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Line { a, b } => {
            push_prim!(ops, OpType::Line, v4(a.x, a.y, b.x, b.y));
        }
        SdfNode::Bezier { p0, p1, p2, p3 } => {
            push_prim!(ops, OpType::Bezier,
                v4(p0.x, p0.y, p1.x, p1.y),
                v4(p2.x, p2.y, p3.x, p3.y));
        }
        SdfNode::QuadBezier { p0, p1, p2 } => {
            push_prim!(ops, OpType::QuadBezier,
                v4(p0.x, p0.y, p1.x, p1.y),
                v4(p2.x, p2.y, 0.0, 0.0));
        }
        SdfNode::Ellipse { ab } => {
            push_prim!(ops, OpType::Ellipse, v4(ab.x, ab.y, 0.0, 0.0));
        }
        SdfNode::Triangle { p0, p1, p2 } => {
            push_prim!(ops, OpType::Triangle,
                v4(p0.x, p0.y, p1.x, p1.y),
                v4(p2.x, p2.y, 0.0, 0.0));
        }
        SdfNode::EquilateralTriangle { radius } => {
            push_prim!(ops, OpType::EquilateralTriangle, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::IsoscelesTriangle { q } => {
            push_prim!(ops, OpType::IsoscelesTriangle, v4(q.x, q.y, 0.0, 0.0));
        }
        SdfNode::Rhombus { b } => {
            push_prim!(ops, OpType::Rhombus, v4(b.x, b.y, 0.0, 0.0));
        }
        SdfNode::Trapezoid { r1, r2, he } => {
            push_prim!(ops, OpType::Trapezoid, v4(*r1, *r2, *he, 0.0));
        }
        SdfNode::Parallelogram { wi, he, sk } => {
            push_prim!(ops, OpType::Parallelogram, v4(*wi, *he, *sk, 0.0));
        }
        SdfNode::Pentagon { radius } => {
            push_prim!(ops, OpType::Pentagon, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Hexagon { radius } => {
            push_prim!(ops, OpType::Hexagon, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Octagon { radius } => {
            push_prim!(ops, OpType::Octagon, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Hexagram { radius } => {
            push_prim!(ops, OpType::Hexagram, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Star { radius, n, m } => {
            push_prim!(ops, OpType::Star, v4(*radius, *n as f32, *m, 0.0));
        }
        SdfNode::Pie { angle, radius } => {
            push_prim!(ops, OpType::Pie, v4(angle.sin(), angle.cos(), *radius, 0.0));
        }
        SdfNode::Arc { angle, ra, rb } => {
            push_prim!(ops, OpType::Arc, v4(angle.sin(), angle.cos(), *ra, *rb));
        }
        SdfNode::CutDisk { radius, h } => {
            push_prim!(ops, OpType::CutDisk, v4(*radius, *h, 0.0, 0.0));
        }
        SdfNode::Heart => {
            push_prim!(ops, OpType::Heart, V4_ZERO);
        }
        SdfNode::Egg { ra, rb } => {
            push_prim!(ops, OpType::Egg, v4(*ra, *rb, 0.0, 0.0));
        }
        SdfNode::Moon { d, ra, rb } => {
            push_prim!(ops, OpType::Moon, v4(*d, *ra, *rb, 0.0));
        }
        SdfNode::Vesica { r, d } => {
            push_prim!(ops, OpType::Vesica, v4(*r, *d, 0.0, 0.0));
        }
        SdfNode::UnevenCapsule { r1, r2, h } => {
            push_prim!(ops, OpType::UnevenCapsule, v4(*r1, *r2, *h, 0.0));
        }
        SdfNode::OrientedBox { a, b, thickness } => {
            push_prim!(ops, OpType::OrientedBox,
                v4(a.x, a.y, b.x, b.y),
                v4(*thickness, 0.0, 0.0, 0.0));
        }
        SdfNode::Horseshoe { angle, radius, w } => {
            push_prim!(ops, OpType::Horseshoe,
                v4(angle.sin(), angle.cos(), *radius, 0.0),
                v4(w.x, w.y, 0.0, 0.0));
        }
        SdfNode::RoundedX { w, r } => {
            push_prim!(ops, OpType::RoundedX, v4(*w, *r, 0.0, 0.0));
        }
        SdfNode::Cross { b, r } => {
            push_prim!(ops, OpType::Cross, v4(b.x, b.y, *r, 0.0));
        }
        SdfNode::Parabola { k } => {
            push_prim!(ops, OpType::Parabola, v4(*k, 0.0, 0.0, 0.0));
        }
        SdfNode::CoolS => {
            push_prim!(ops, OpType::CoolS, V4_ZERO);
        }
        SdfNode::BlobbyCross { he } => {
            push_prim!(ops, OpType::BlobbyCross, v4(*he, 0.0, 0.0, 0.0));
        }

        // Binary operations: evaluate children first (postfix), then operation
        SdfNode::Union(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            push_prim!(ops, OpType::Union, V4_ZERO);
        }
        SdfNode::Subtract(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            push_prim!(ops, OpType::Subtract, V4_ZERO);
        }
        SdfNode::Intersect(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            push_prim!(ops, OpType::Intersect, V4_ZERO);
        }
        SdfNode::SmoothUnion { a, b, k } => {
            compile_node(a, ops);
            compile_node(b, ops);
            push_prim!(ops, OpType::SmoothUnion, v4(*k, 0.0, 0.0, 0.0));
        }
        SdfNode::SmoothSubtract { a, b, k } => {
            compile_node(a, ops);
            compile_node(b, ops);
            push_prim!(ops, OpType::SmoothSubtract, v4(*k, 0.0, 0.0, 0.0));
        }

        // Unary modifiers: evaluate child first, then modifier
        SdfNode::Round { node, radius } => {
            compile_node(node, ops);
            push_prim!(ops, OpType::Round, v4(*radius, 0.0, 0.0, 0.0));
        }
        SdfNode::Onion { node, thickness } => {
            compile_node(node, ops);
            push_prim!(ops, OpType::Onion, v4(*thickness, 0.0, 0.0, 0.0));
        }

        SdfNode::Dash { node, dash, gap, thickness, angle, speed } => {
            let perimeter = node.perimeter().unwrap_or(0.0);
            compile_node(node, ops);
            push_prim!(ops, OpType::Dash,
                v4(*dash, *gap, *thickness, *angle),
                v4(*speed, perimeter, 0.0, 0.0));
        }
        SdfNode::Arrow { node, segment, gap, thickness, angle, speed } => {
            let perimeter = node.perimeter().unwrap_or(0.0);
            compile_node(node, ops);
            push_prim!(ops, OpType::Arrow,
                v4(*segment, *gap, *thickness, *angle),
                v4(*speed, perimeter, 0.0, 0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Sdf;

    #[test]
    fn test_compile_circle() {
        let sdf = Sdf::circle([10.0, 20.0], 5.0);
        let ops = compile(sdf.node());

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].op_type, OpType::Circle as u32);
        assert_eq!(ops[0].param0.0[0], 10.0);
        assert_eq!(ops[0].param0.0[1], 20.0);
        assert_eq!(ops[0].param0.0[2], 5.0);
    }

    #[test]
    fn test_compile_union() {
        let a = Sdf::circle([0.0, 0.0], 10.0);
        let b = Sdf::circle([20.0, 0.0], 10.0);
        let combined = a | b;
        let ops = compile(combined.node());

        // Postfix: [Circle A, Circle B, Union]
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].op_type, OpType::Circle as u32);
        assert_eq!(ops[1].op_type, OpType::Circle as u32);
        assert_eq!(ops[2].op_type, OpType::Union as u32);
    }

    #[test]
    fn test_compile_subtract() {
        let a = Sdf::rect([0.0, 0.0], [50.0, 50.0]);
        let b = Sdf::circle([0.0, 0.0], 25.0);
        let result = a - b;
        let ops = compile(result.node());

        // Postfix: [Box, Circle, Subtract]
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].op_type, OpType::Box as u32);
        assert_eq!(ops[1].op_type, OpType::Circle as u32);
        assert_eq!(ops[2].op_type, OpType::Subtract as u32);
    }

    #[test]
    fn test_compile_nested() {
        // (A - B) | C
        let a = Sdf::rect([0.0, 0.0], [100.0, 100.0]);
        let b = Sdf::circle([50.0, 50.0], 30.0);
        let c = Sdf::circle([-50.0, -50.0], 20.0);
        let result = (a - b) | c;
        let ops = compile(result.node());

        // Postfix: [Box, Circle B, Subtract, Circle C, Union]
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[0].op_type, OpType::Box as u32);
        assert_eq!(ops[1].op_type, OpType::Circle as u32);
        assert_eq!(ops[2].op_type, OpType::Subtract as u32);
        assert_eq!(ops[3].op_type, OpType::Circle as u32);
        assert_eq!(ops[4].op_type, OpType::Union as u32);
    }
}
