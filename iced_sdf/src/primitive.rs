//! SDF rendering primitive for Iced.
//!
//! `SdfPrimitive` holds one or more SDF shapes. During `prepare()`, shapes are
//! compiled to RPN ops and uploaded to GPU buffers. During `draw()`, a fullscreen
//! triangle renders all shapes via per-pixel AABB filtering and SDF evaluation.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferUsages, Device, Queue, TextureFormat,
};
use iced::Rectangle;
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use smallvec::SmallVec;

use crate::compile::compile_into;
use crate::layer::Layer;
use crate::pipeline::{buffer, types};
use crate::shape::Sdf;
use crate::shared::SharedSdfResources;

/// Global stats from the last completed frame.
static LAST_STATS: Mutex<types::SdfStats> = Mutex::new(types::SdfStats {
    shape_count: 0,
    tile_count: 0,
    prepare_cpu_us: 0,
    gpu_time_us: None,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().unwrap().clone()
}

/// A single shape entry within an SdfPrimitive.
#[derive(Debug, Clone)]
struct ShapeEntry {
    shape: Sdf,
    layers: SmallVec<[Layer; 3]>,
    bounds: [f32; 4],
}

/// SDF rendering primitive holding one or more shapes.
///
/// Shapes are compiled to RPN ops during `prepare()`. The fragment shader
/// loops over each shape, applies AABB filtering per pixel, and evaluates
/// only overlapping shapes.
///
/// # Single shape (builder pattern)
/// ```ignore
/// SdfPrimitive::single(shape)
///     .layers(vec![Layer::solid(Color::WHITE)])
///     .screen_bounds([0.0, 0.0, 100.0, 100.0])
///     .camera(cam_x, cam_y, zoom)
///     .time(t)
/// ```
///
/// # Multiple shapes
/// ```ignore
/// let mut prim = SdfPrimitive::new();
/// prim.push(&shape1, &layers1, bounds1);
/// prim.push(&shape2, &layers2, bounds2);
/// let prim = prim.camera(cam_x, cam_y, zoom).time(t);
/// ```
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    shapes: Vec<ShapeEntry>,
    /// Camera position (world origin offset).
    pub camera_position: (f32, f32),
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Animation time in seconds.
    pub time: f32,
    /// Debug visualization flags.
    pub debug_flags: u32,
}

impl SdfPrimitive {
    /// Create an empty primitive. Use [`push`] to add shapes.
    pub fn new() -> Self {
        Self {
            shapes: Vec::new(),
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug_flags: 0,
        }
    }

    /// Create a primitive with pre-allocated capacity.
    pub fn with_capacity(shapes: usize) -> Self {
        Self {
            shapes: Vec::with_capacity(shapes),
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug_flags: 0,
        }
    }

    /// Create a primitive with a single shape (convenience builder).
    pub fn single(shape: Sdf) -> Self {
        Self {
            shapes: vec![ShapeEntry {
                shape,
                layers: smallvec::smallvec![Layer::solid(iced::Color::WHITE)],
                bounds: [0.0, 0.0, 100.0, 100.0],
            }],
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug_flags: 0,
        }
    }

    /// Add a shape to the primitive.
    pub fn push(&mut self, shape: &Sdf, layers: &[Layer], bounds: [f32; 4]) -> &mut Self {
        self.shapes.push(ShapeEntry {
            shape: shape.clone(),
            layers: layers.iter().cloned().collect(),
            bounds,
        });
        self
    }

    /// Set rendering layers (applies to the first shape, for single-shape builder).
    pub fn layers(mut self, layers: impl Into<SmallVec<[Layer; 3]>>) -> Self {
        if let Some(entry) = self.shapes.first_mut() {
            entry.layers = layers.into();
        }
        self
    }

    /// Set the screen-space bounding box (applies to the first shape).
    pub fn screen_bounds(mut self, bounds: [f32; 4]) -> Self {
        if let Some(entry) = self.shapes.first_mut() {
            entry.bounds = bounds;
        }
        self
    }

    /// Set camera position and zoom.
    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    /// Set animation time.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }

    /// Set debug visualization flags.
    pub fn debug_flags(mut self, flags: u32) -> Self {
        self.debug_flags = flags;
        self
    }

    /// Enable or disable tile culling debug overlay.
    pub fn debug_tiles(self, enabled: bool) -> Self {
        self.debug_flags(if enabled { 1 } else { 0 })
    }

    /// Number of shapes in this primitive.
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// Whether this primitive has no shapes.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

impl Default for SdfPrimitive {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared pipeline for all SDF primitives.
///
/// Accumulates shape data across `prepare()` calls. Each `draw()` renders
/// a fullscreen triangle that loops over this draw's shape range.
pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    shapes_buffer: buffer::Buffer<types::ShapeInstance>,
    ops_buffer: buffer::Buffer<types::SdfOp>,
    layers_buffer: buffer::Buffer<types::SdfLayer>,
    draw_data_buffer: buffer::Buffer<types::DrawData>,
    bind_group: BindGroup,
    bind_group_generations: (u64, u64, u64, u64),
    draw_index: AtomicU32,
    compile_scratch: Vec<types::SdfOp>,
    frame_stats: types::SdfStats,
}

fn create_bind_group(
    device: &Device,
    shared: &SharedSdfResources,
    draw_data_buffer: &buffer::Buffer<types::DrawData>,
    shapes_buffer: &buffer::Buffer<types::ShapeInstance>,
    ops_buffer: &buffer::Buffer<types::SdfOp>,
    layers_buffer: &buffer::Buffer<types::SdfLayer>,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_bind_group"),
        layout: &shared.bind_group_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: draw_data_buffer.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: shapes_buffer.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: ops_buffer.as_entire_binding() },
            BindGroupEntry { binding: 3, resource: layers_buffer.as_entire_binding() },
        ],
    })
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        let shapes_buffer = buffer::Buffer::new(device, Some("sdf_shapes"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let ops_buffer = buffer::Buffer::new(device, Some("sdf_ops"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let layers_buffer = buffer::Buffer::new(device, Some("sdf_layers"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let draw_data_buffer = buffer::Buffer::new(device, Some("sdf_draw_data"), BufferUsages::STORAGE | BufferUsages::COPY_DST);

        let bind_group = create_bind_group(
            device, &shared,
            &draw_data_buffer, &shapes_buffer, &ops_buffer, &layers_buffer,
        );

        Self {
            shared,
            shapes_buffer,
            ops_buffer,
            layers_buffer,
            draw_data_buffer,
            bind_group,
            bind_group_generations: (0, 0, 0, 0),
            draw_index: AtomicU32::new(0),
            compile_scratch: Vec::new(),
            frame_stats: types::SdfStats::default(),
        }
    }

    fn trim(&mut self) {
        self.frame_stats.tile_count = self.shapes_buffer.len() as u32;
        if let Ok(mut stats) = LAST_STATS.lock() {
            *stats = self.frame_stats.clone();
        }
        self.frame_stats = types::SdfStats::default();
        self.shapes_buffer.clear();
        self.ops_buffer.clear();
        self.layers_buffer.clear();
        self.draw_data_buffer.clear();
        self.draw_index.store(0, Ordering::Relaxed);
    }
}

impl Primitive for SdfPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        if self.shapes.is_empty() {
            let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData::default());
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let shape_start = pipeline.shapes_buffer.len() as u32;

        for entry in &self.shapes {
            compile_into(entry.shape.node(), &mut pipeline.compile_scratch);
            let ops_offset = pipeline.ops_buffer.len() as u32;
            let ops_count = pipeline.compile_scratch.len() as u32;
            let _ = pipeline.ops_buffer.push_bulk(device, queue, &pipeline.compile_scratch);

            let layers_offset = pipeline.layers_buffer.len() as u32;
            let layers_count = entry.layers.len() as u32;
            let gpu_layers: SmallVec<[types::SdfLayer; 3]> =
                entry.layers.iter().map(|l| l.to_gpu()).collect();
            let _ = pipeline.layers_buffer.push_bulk(device, queue, &gpu_layers);

            let max_radius = entry.layers.iter().map(|l| l.max_effect_radius()).fold(0.0f32, f32::max);
            let has_fill = entry.layers.iter().any(|l| l.is_fill());

            let _ = pipeline.shapes_buffer.push(device, queue, types::ShapeInstance {
                bounds: glam::Vec4::new(entry.bounds[0], entry.bounds[1], entry.bounds[2], entry.bounds[3]),
                ops_offset, ops_count, layers_offset, layers_count,
                max_radius,
                has_fill: u32::from(has_fill),
                _pad2: 0, _pad3: 0,
            });
        }

        let shape_count = self.shapes.len() as u32;

        // Push DrawData for this draw
        let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData {
            camera_position: glam::Vec2::new(self.camera_position.0, self.camera_position.1),
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            debug_flags: self.debug_flags,
            shape_start,
            shape_count,
        });

        // Recreate bind group if any buffer was resized
        let gens = (
            pipeline.shapes_buffer.generation(),
            pipeline.ops_buffer.generation(),
            pipeline.layers_buffer.generation(),
            pipeline.draw_data_buffer.generation(),
        );
        if gens != pipeline.bind_group_generations {
            pipeline.bind_group = create_bind_group(
                device, &pipeline.shared,
                &pipeline.draw_data_buffer, &pipeline.shapes_buffer,
                &pipeline.ops_buffer, &pipeline.layers_buffer,
            );
            pipeline.bind_group_generations = gens;
        }

        pipeline.frame_stats.shape_count += shape_count;
        pipeline.frame_stats.prepare_cpu_us += prepare_start.elapsed().as_micros() as u64;
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        let draw_idx = pipeline.draw_index.fetch_add(1, Ordering::Relaxed);
        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, draw_idx..draw_idx + 1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Sdf;

    #[test]
    fn test_primitive_new_is_empty() {
        let prim = SdfPrimitive::new();
        assert!(prim.is_empty());
        assert_eq!(prim.shape_count(), 0);
    }

    #[test]
    fn test_primitive_single() {
        let prim = SdfPrimitive::single(Sdf::circle([0.0, 0.0], 10.0));
        assert_eq!(prim.shape_count(), 1);
        assert!(!prim.is_empty());
    }

    #[test]
    fn test_primitive_push() {
        let mut prim = SdfPrimitive::new();
        let circle = Sdf::circle([0.0, 0.0], 5.0);
        let layers = [Layer::solid(iced::Color::WHITE)];
        prim.push(&circle, &layers, [0.0, 0.0, 10.0, 10.0]);
        prim.push(&circle, &layers, [20.0, 20.0, 10.0, 10.0]);
        assert_eq!(prim.shape_count(), 2);
    }

    #[test]
    fn test_primitive_builder_chain() {
        let prim = SdfPrimitive::single(Sdf::circle([0.0, 0.0], 10.0))
            .layers(vec![Layer::solid(iced::Color::BLACK)])
            .screen_bounds([10.0, 20.0, 100.0, 50.0])
            .camera(1.0, 2.0, 3.0)
            .time(0.5)
            .debug_flags(1);

        assert_eq!(prim.camera_position, (1.0, 2.0));
        assert_eq!(prim.camera_zoom, 3.0);
        assert_eq!(prim.time, 0.5);
        assert_eq!(prim.debug_flags, 1);
    }

    #[test]
    fn test_primitive_with_capacity() {
        let prim = SdfPrimitive::with_capacity(100);
        assert!(prim.is_empty());
    }
}
