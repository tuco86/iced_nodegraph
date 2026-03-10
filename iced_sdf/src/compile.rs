//! Compiles SDF tree to RPN (Reverse Polish Notation) for GPU evaluation.
//!
//! The GPU shader evaluates SDFs using a stack-based approach. This module
//! converts the tree structure into a linear array of operations.

use crate::pipeline::types::SdfOp;
use crate::shape::SdfNode;
use glam::Vec4;

/// Operation types for the shader.
/// Must match the constants in shader.wgsl.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
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

/// Compile an SDF tree into RPN format for GPU evaluation.
///
/// Returns a vector of `SdfOp` structs that can be uploaded to a storage buffer.
pub fn compile(node: &SdfNode) -> Vec<SdfOp> {
    let mut ops = Vec::new();
    compile_node(node, &mut ops);
    ops
}

/// Recursively compile a node in postfix order.
fn compile_node(node: &SdfNode, ops: &mut Vec<SdfOp>) {
    match node {
        // Primitives push themselves to the stack
        SdfNode::Circle { center, radius } => {
            ops.push(SdfOp {
                op_type: OpType::Circle as u32,
                flags: 0,
                param0: Vec4::new(center.x, center.y, *radius, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Box { center, half_size } => {
            ops.push(SdfOp {
                op_type: OpType::Box as u32,
                flags: 0,
                param0: Vec4::new(center.x, center.y, half_size.x, half_size.y),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::RoundedBox {
            center,
            half_size,
            corner_radius,
        } => {
            ops.push(SdfOp {
                op_type: OpType::RoundedBox as u32,
                flags: 0,
                param0: Vec4::new(center.x, center.y, half_size.x, half_size.y),
                param1: Vec4::new(*corner_radius, 0.0, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Line { a, b } => {
            ops.push(SdfOp {
                op_type: OpType::Line as u32,
                flags: 0,
                param0: Vec4::new(a.x, a.y, b.x, b.y),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Bezier { p0, p1, p2, p3 } => {
            ops.push(SdfOp {
                op_type: OpType::Bezier as u32,
                flags: 0,
                param0: Vec4::new(p0.x, p0.y, p1.x, p1.y),
                param1: Vec4::new(p2.x, p2.y, p3.x, p3.y),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::QuadBezier { p0, p1, p2 } => {
            ops.push(SdfOp {
                op_type: OpType::QuadBezier as u32,
                flags: 0,
                param0: Vec4::new(p0.x, p0.y, p1.x, p1.y),
                param1: Vec4::new(p2.x, p2.y, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Ellipse { ab } => {
            ops.push(SdfOp {
                op_type: OpType::Ellipse as u32,
                flags: 0,
                param0: Vec4::new(ab.x, ab.y, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Triangle { p0, p1, p2 } => {
            ops.push(SdfOp {
                op_type: OpType::Triangle as u32,
                flags: 0,
                param0: Vec4::new(p0.x, p0.y, p1.x, p1.y),
                param1: Vec4::new(p2.x, p2.y, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::EquilateralTriangle { radius } => {
            ops.push(SdfOp {
                op_type: OpType::EquilateralTriangle as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::IsoscelesTriangle { q } => {
            ops.push(SdfOp {
                op_type: OpType::IsoscelesTriangle as u32,
                flags: 0,
                param0: Vec4::new(q.x, q.y, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Rhombus { b } => {
            ops.push(SdfOp {
                op_type: OpType::Rhombus as u32,
                flags: 0,
                param0: Vec4::new(b.x, b.y, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Trapezoid { r1, r2, he } => {
            ops.push(SdfOp {
                op_type: OpType::Trapezoid as u32,
                flags: 0,
                param0: Vec4::new(*r1, *r2, *he, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Parallelogram { wi, he, sk } => {
            ops.push(SdfOp {
                op_type: OpType::Parallelogram as u32,
                flags: 0,
                param0: Vec4::new(*wi, *he, *sk, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Pentagon { radius } => {
            ops.push(SdfOp {
                op_type: OpType::Pentagon as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Hexagon { radius } => {
            ops.push(SdfOp {
                op_type: OpType::Hexagon as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Octagon { radius } => {
            ops.push(SdfOp {
                op_type: OpType::Octagon as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Hexagram { radius } => {
            ops.push(SdfOp {
                op_type: OpType::Hexagram as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Star { radius, n, m } => {
            ops.push(SdfOp {
                op_type: OpType::Star as u32,
                flags: 0,
                param0: Vec4::new(*radius, *n as f32, *m, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Pie { angle, radius } => {
            ops.push(SdfOp {
                op_type: OpType::Pie as u32,
                flags: 0,
                param0: Vec4::new(angle.sin(), angle.cos(), *radius, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Arc { angle, ra, rb } => {
            ops.push(SdfOp {
                op_type: OpType::Arc as u32,
                flags: 0,
                param0: Vec4::new(angle.sin(), angle.cos(), *ra, *rb),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::CutDisk { radius, h } => {
            ops.push(SdfOp {
                op_type: OpType::CutDisk as u32,
                flags: 0,
                param0: Vec4::new(*radius, *h, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Heart => {
            ops.push(SdfOp {
                op_type: OpType::Heart as u32,
                flags: 0,
                param0: Vec4::ZERO,
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Egg { ra, rb } => {
            ops.push(SdfOp {
                op_type: OpType::Egg as u32,
                flags: 0,
                param0: Vec4::new(*ra, *rb, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Moon { d, ra, rb } => {
            ops.push(SdfOp {
                op_type: OpType::Moon as u32,
                flags: 0,
                param0: Vec4::new(*d, *ra, *rb, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Vesica { r, d } => {
            ops.push(SdfOp {
                op_type: OpType::Vesica as u32,
                flags: 0,
                param0: Vec4::new(*r, *d, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::UnevenCapsule { r1, r2, h } => {
            ops.push(SdfOp {
                op_type: OpType::UnevenCapsule as u32,
                flags: 0,
                param0: Vec4::new(*r1, *r2, *h, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::OrientedBox { a, b, thickness } => {
            ops.push(SdfOp {
                op_type: OpType::OrientedBox as u32,
                flags: 0,
                param0: Vec4::new(a.x, a.y, b.x, b.y),
                param1: Vec4::new(*thickness, 0.0, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Horseshoe { angle, radius, w } => {
            ops.push(SdfOp {
                op_type: OpType::Horseshoe as u32,
                flags: 0,
                param0: Vec4::new(angle.sin(), angle.cos(), *radius, 0.0),
                param1: Vec4::new(w.x, w.y, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::RoundedX { w, r } => {
            ops.push(SdfOp {
                op_type: OpType::RoundedX as u32,
                flags: 0,
                param0: Vec4::new(*w, *r, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Cross { b, r } => {
            ops.push(SdfOp {
                op_type: OpType::Cross as u32,
                flags: 0,
                param0: Vec4::new(b.x, b.y, *r, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Parabola { k } => {
            ops.push(SdfOp {
                op_type: OpType::Parabola as u32,
                flags: 0,
                param0: Vec4::new(*k, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::CoolS => {
            ops.push(SdfOp {
                op_type: OpType::CoolS as u32,
                flags: 0,
                param0: Vec4::ZERO,
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::BlobbyCross { he } => {
            ops.push(SdfOp {
                op_type: OpType::BlobbyCross as u32,
                flags: 0,
                param0: Vec4::new(*he, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        // Binary operations: evaluate children first (postfix), then operation
        SdfNode::Union(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            ops.push(SdfOp {
                op_type: OpType::Union as u32,
                flags: 0,
                param0: Vec4::ZERO,
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Subtract(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            ops.push(SdfOp {
                op_type: OpType::Subtract as u32,
                flags: 0,
                param0: Vec4::ZERO,
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Intersect(a, b) => {
            compile_node(a, ops);
            compile_node(b, ops);
            ops.push(SdfOp {
                op_type: OpType::Intersect as u32,
                flags: 0,
                param0: Vec4::ZERO,
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::SmoothUnion { a, b, k } => {
            compile_node(a, ops);
            compile_node(b, ops);
            ops.push(SdfOp {
                op_type: OpType::SmoothUnion as u32,
                flags: 0,
                param0: Vec4::new(*k, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::SmoothSubtract { a, b, k } => {
            compile_node(a, ops);
            compile_node(b, ops);
            ops.push(SdfOp {
                op_type: OpType::SmoothSubtract as u32,
                flags: 0,
                param0: Vec4::new(*k, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        // Unary modifiers: evaluate child first, then modifier
        SdfNode::Round { node, radius } => {
            compile_node(node, ops);
            ops.push(SdfOp {
                op_type: OpType::Round as u32,
                flags: 0,
                param0: Vec4::new(*radius, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Onion { node, thickness } => {
            compile_node(node, ops);
            ops.push(SdfOp {
                op_type: OpType::Onion as u32,
                flags: 0,
                param0: Vec4::new(*thickness, 0.0, 0.0, 0.0),
                param1: Vec4::ZERO,
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Dash {
            node,
            dash,
            gap,
            thickness,
            angle,
            speed,
        } => {
            let perimeter = node.perimeter().unwrap_or(0.0);
            compile_node(node, ops);
            ops.push(SdfOp {
                op_type: OpType::Dash as u32,
                flags: 0,
                param0: Vec4::new(*dash, *gap, *thickness, *angle),
                param1: Vec4::new(*speed, perimeter, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
        }

        SdfNode::Arrow {
            node,
            segment,
            gap,
            thickness,
            angle,
            speed,
        } => {
            let perimeter = node.perimeter().unwrap_or(0.0);
            compile_node(node, ops);
            ops.push(SdfOp {
                op_type: OpType::Arrow as u32,
                flags: 0,
                param0: Vec4::new(*segment, *gap, *thickness, *angle),
                param1: Vec4::new(*speed, perimeter, 0.0, 0.0),
                param2: Vec4::ZERO,
                ..Default::default()
            });
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
        assert_eq!(ops[0].param0.x, 10.0);
        assert_eq!(ops[0].param0.y, 20.0);
        assert_eq!(ops[0].param0.z, 5.0);
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
