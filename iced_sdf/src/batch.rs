//! Batched SDF rendering.
//!
//! Collects multiple SDF shapes into a single draw call using instanced
//! rendering. Each shape gets a screen-space quad; the fragment shader
//! evaluates only that shape's RPN ops within the quad.

use crate::compile::compile_into;
use crate::layer::Layer;
use crate::pipeline::types::{ShapeInstance, SdfLayer, SdfOp};
use crate::shape::Sdf;

/// A collected batch of SDF shapes ready for GPU submission.
///
/// Shapes are compiled into flat ops/layers buffers with per-shape offsets.
/// Call [`push`] to add shapes, then pass the batch to an [`SdfBatchPrimitive`]
/// for rendering.
#[derive(Debug, Clone)]
pub struct SdfBatch {
    /// Per-shape instance data (bounds + buffer offsets).
    shapes: Vec<ShapeInstance>,
    /// Flat array of all compiled SDF operations.
    ops: Vec<SdfOp>,
    /// Flat array of all GPU layers.
    layers: Vec<SdfLayer>,
    /// Scratch buffer reused across push() calls for compilation.
    compile_scratch: Vec<SdfOp>,
}

impl SdfBatch {
    /// Create an empty batch.
    pub fn new() -> Self {
        Self {
            shapes: Vec::new(),
            ops: Vec::new(),
            layers: Vec::new(),
            compile_scratch: Vec::new(),
        }
    }

    /// Create a batch with pre-allocated capacity.
    pub fn with_capacity(shapes: usize, ops: usize, layers: usize) -> Self {
        Self {
            shapes: Vec::with_capacity(shapes),
            ops: Vec::with_capacity(ops),
            layers: Vec::with_capacity(layers),
            compile_scratch: Vec::new(),
        }
    }

    /// Add a shape to the batch.
    ///
    /// `bounds` is the screen-space bounding box `[x, y, width, height]`.
    /// The shape's SDF tree is compiled to RPN and appended to the flat buffers.
    ///
    /// Returns the shape index within this batch.
    pub fn push(&mut self, shape: &Sdf, shape_layers: &[Layer], bounds: [f32; 4]) -> usize {
        let ops_offset = self.ops.len() as u32;
        let layers_offset = self.layers.len() as u32;

        // Compile SDF tree to RPN ops (reuses scratch buffer)
        compile_into(shape.node(), &mut self.compile_scratch);
        let ops_count = self.compile_scratch.len() as u32;
        self.ops.extend_from_slice(&self.compile_scratch);

        // Convert layers to GPU format
        let layers_count = shape_layers.len() as u32;
        for layer in shape_layers {
            self.layers.push(layer.to_gpu());
        }

        let index = self.shapes.len();
        self.shapes.push(ShapeInstance {
            bounds: glam::Vec4::new(bounds[0], bounds[1], bounds[2], bounds[3]),
            ops_offset,
            ops_count,
            layers_offset,
            layers_count,
        });

        index
    }

    /// Clear all shapes for reuse next frame.
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.ops.clear();
        self.layers.clear();
    }

    /// Number of shapes in the batch.
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// Number of compiled ops across all shapes.
    pub fn ops_count(&self) -> usize {
        self.ops.len()
    }

    /// Number of layers across all shapes.
    pub fn layers_count(&self) -> usize {
        self.layers.len()
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Access the shape instances.
    pub fn shapes(&self) -> &[ShapeInstance] {
        &self.shapes
    }

    /// Access the flat ops buffer.
    pub fn ops(&self) -> &[SdfOp] {
        &self.ops
    }

    /// Access the flat layers buffer.
    pub fn gpu_layers(&self) -> &[SdfLayer] {
        &self.layers
    }

    /// Upload the batch contents to GPU pipeline buffers using bulk writes.
    ///
    /// Adjusts shape offsets to account for existing data in the pipeline
    /// buffers, so batches can be uploaded to non-empty pipelines shared
    /// with other primitives.
    pub fn upload(
        &self,
        shapes_buffer: &mut crate::pipeline::Buffer<ShapeInstance>,
        ops_buffer: &mut crate::pipeline::Buffer<SdfOp>,
        layers_buffer: &mut crate::pipeline::Buffer<SdfLayer>,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
    ) {
        if self.is_empty() {
            return;
        }

        // Record base offsets before pushing
        let ops_base = ops_buffer.len() as u32;
        let layers_base = layers_buffer.len() as u32;

        let _ = ops_buffer.push_bulk(device, queue, &self.ops);
        let _ = layers_buffer.push_bulk(device, queue, &self.layers);

        // Adjust shape offsets to pipeline-global positions
        let adjusted: Vec<ShapeInstance> = self
            .shapes
            .iter()
            .map(|s| ShapeInstance {
                bounds: s.bounds,
                ops_offset: s.ops_offset + ops_base,
                ops_count: s.ops_count,
                layers_offset: s.layers_offset + layers_base,
                layers_count: s.layers_count,
            })
            .collect();
        let _ = shapes_buffer.push_bulk(device, queue, &adjusted);
    }
}

impl Default for SdfBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Layer;
    use crate::shape::Sdf;
    use iced::Color;

    #[test]
    fn test_empty_batch() {
        let batch = SdfBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.shape_count(), 0);
    }

    #[test]
    fn test_push_single_shape() {
        let mut batch = SdfBatch::new();
        let shape = Sdf::circle([100.0, 50.0], 25.0);
        let layers = [Layer::solid(Color::WHITE)];
        let idx = batch.push(&shape, &layers, [75.0, 25.0, 50.0, 50.0]);

        assert_eq!(idx, 0);
        assert_eq!(batch.shape_count(), 1);
        assert_eq!(batch.ops_count(), 1); // circle = 1 op
        assert_eq!(batch.layers_count(), 1);

        let inst = &batch.shapes()[0];
        assert_eq!(inst.ops_offset, 0);
        assert_eq!(inst.ops_count, 1);
        assert_eq!(inst.layers_offset, 0);
        assert_eq!(inst.layers_count, 1);
    }

    #[test]
    fn test_push_multiple_shapes() {
        let mut batch = SdfBatch::new();

        // Shape 0: circle (1 op, 1 layer)
        let s0 = Sdf::circle([0.0, 0.0], 10.0);
        let l0 = [Layer::solid(Color::WHITE)];
        batch.push(&s0, &l0, [0.0, 0.0, 20.0, 20.0]);

        // Shape 1: union of two circles (3 ops, 2 layers)
        let s1 = Sdf::circle([0.0, 0.0], 5.0) | Sdf::circle([10.0, 0.0], 5.0);
        let l1 = [
            Layer::solid(Color::BLACK).expand(4.0).blur(2.0),
            Layer::solid(Color::WHITE),
        ];
        batch.push(&s1, &l1, [0.0, 0.0, 30.0, 20.0]);

        assert_eq!(batch.shape_count(), 2);
        assert_eq!(batch.ops_count(), 4); // 1 + 3
        assert_eq!(batch.layers_count(), 3); // 1 + 2

        // Verify offsets
        let inst1 = &batch.shapes()[1];
        assert_eq!(inst1.ops_offset, 1); // after shape 0's 1 op
        assert_eq!(inst1.ops_count, 3);
        assert_eq!(inst1.layers_offset, 1); // after shape 0's 1 layer
        assert_eq!(inst1.layers_count, 2);
    }

    #[test]
    fn test_clear() {
        let mut batch = SdfBatch::new();
        let shape = Sdf::circle([0.0, 0.0], 10.0);
        batch.push(&shape, &[Layer::solid(Color::WHITE)], [0.0; 4]);
        assert!(!batch.is_empty());

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.ops_count(), 0);
        assert_eq!(batch.layers_count(), 0);
    }
}
