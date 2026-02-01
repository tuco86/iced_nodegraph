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
    // Primitives (0-15)
    Circle = 0,
    Box = 1,
    RoundedBox = 2,
    Line = 3,
    Bezier = 4,

    // Operations (16-31)
    Union = 16,
    Subtract = 17,
    Intersect = 18,
    SmoothUnion = 19,
    SmoothSubtract = 20,

    // Modifiers (32-47)
    Round = 32,
    Onion = 33,
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
