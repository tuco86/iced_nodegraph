//! SDF rendering primitive for Iced.
//!
//! `SdfPrimitive` holds one or more SDF shapes. During `prepare()`, shapes are
//! compiled to RPN ops, uploaded to GPU, and a compute shader builds a spatial
//! index for this primitive's tile region. During `draw()`, a fullscreen triangle
//! reads the spatial index to evaluate only relevant shapes per pixel.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, Device, Queue, TextureFormat,
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

static LAST_STATS: Mutex<types::SdfStats> = Mutex::new(types::SdfStats {
    shape_count: 0, tile_count: 0, prepare_cpu_us: 0, gpu_time_us: None,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().unwrap().clone()
}

// Must match WGSL constants in pipeline/shader.wgsl
const TILE_SIZE: f32 = 16.0;
const MAX_SHAPES_PER_TILE: u32 = 16;

#[derive(Debug, Clone)]
struct ShapeEntry {
    shape: Sdf,
    layers: SmallVec<[Layer; 3]>,
    bounds: [f32; 4],
}

/// SDF rendering primitive holding one or more shapes.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    shapes: Vec<ShapeEntry>,
    pub camera_position: (f32, f32),
    pub camera_zoom: f32,
    pub time: f32,
    pub debug_flags: u32,
}

impl SdfPrimitive {
    pub fn new() -> Self {
        Self { shapes: Vec::new(), camera_position: (0.0, 0.0), camera_zoom: 1.0, time: 0.0, debug_flags: 0 }
    }

    pub fn with_capacity(shapes: usize) -> Self {
        Self { shapes: Vec::with_capacity(shapes), ..Self::new() }
    }

    pub fn single(shape: Sdf) -> Self {
        Self {
            shapes: vec![ShapeEntry {
                shape, layers: smallvec::smallvec![Layer::solid(iced::Color::WHITE)], bounds: [0.0, 0.0, 100.0, 100.0],
            }],
            ..Self::new()
        }
    }

    pub fn push(&mut self, shape: &Sdf, layers: &[Layer], bounds: [f32; 4]) -> &mut Self {
        self.shapes.push(ShapeEntry { shape: shape.clone(), layers: layers.iter().cloned().collect(), bounds });
        self
    }

    pub fn layers(mut self, layers: impl Into<SmallVec<[Layer; 3]>>) -> Self {
        if let Some(e) = self.shapes.first_mut() { e.layers = layers.into(); }
        self
    }

    pub fn screen_bounds(mut self, bounds: [f32; 4]) -> Self {
        if let Some(e) = self.shapes.first_mut() { e.bounds = bounds; }
        self
    }

    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y); self.camera_zoom = zoom; self
    }

    pub fn time(mut self, time: f32) -> Self { self.time = time; self }
    pub fn debug_flags(mut self, flags: u32) -> Self { self.debug_flags = flags; self }
    pub fn debug_tiles(self, enabled: bool) -> Self { self.debug_flags(if enabled { 1 } else { 0 }) }
    pub fn shape_count(&self) -> usize { self.shapes.len() }
    pub fn is_empty(&self) -> bool { self.shapes.is_empty() }

    /// Whether any shape or layer in this primitive has active animations.
    ///
    /// Checks both layer-level animations (flow speed) and node-level
    /// animations (dash/arrow pattern speed). Useful for widgets to decide
    /// whether to request continuous redraws.
    pub fn has_animations(&self) -> bool {
        self.shapes.iter().any(|entry| {
            entry.layers.iter().any(|l| l.is_animated())
                || entry.shape.node().has_animation()
        })
    }
}

impl Default for SdfPrimitive {
    fn default() -> Self { Self::new() }
}

/// Shared pipeline for all SDF primitives.
pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    // Data buffers (group 0)
    draw_data_buffer: buffer::Buffer<types::DrawData>,
    shapes_buffer: buffer::Buffer<types::ShapeInstance>,
    ops_buffer: buffer::Buffer<types::SdfOp>,
    layers_buffer: buffer::Buffer<types::SdfLayer>,
    // Spatial index buffers
    tile_counts_buffer: iced::wgpu::Buffer,
    tile_shapes_buffer: iced::wgpu::Buffer,
    tile_capacity: u32,
    spatial_index_gen: u64,
    // Compute uniforms
    compute_uniform_buffer: iced::wgpu::Buffer,
    compute_uniform_scratch: Vec<u8>,
    // Bind groups
    render_group0: BindGroup,
    compute_group0: BindGroup,
    compute_group1: BindGroup,
    bind_group_gens: (u64, u64, u64, u64, u64),
    // Frame state
    total_tiles: u32,
    draw_index: AtomicU32,
    compile_scratch: Vec<types::SdfOp>,
    frame_stats: types::SdfStats,
}

fn create_spatial_index_buffers(device: &Device, cap: u32) -> (iced::wgpu::Buffer, iced::wgpu::Buffer) {
    let cap = cap.max(1) as u64;
    (
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_counts"), size: cap * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false,
        }),
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_shapes"), size: cap * MAX_SHAPES_PER_TILE as u64 * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false,
        }),
    )
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        let draw_data_buffer = buffer::Buffer::new(device, Some("sdf_draws"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let shapes_buffer = buffer::Buffer::new(device, Some("sdf_shapes"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let ops_buffer = buffer::Buffer::new(device, Some("sdf_ops"), BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let layers_buffer = buffer::Buffer::new(device, Some("sdf_layers"), BufferUsages::STORAGE | BufferUsages::COPY_DST);

        let (tile_counts_buffer, tile_shapes_buffer) = create_spatial_index_buffers(device, 256);

        let compute_uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_compute_uniforms"),
            size: <types::ComputeUniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let render_group0 = create_render_group0(device, &shared, &draw_data_buffer, &shapes_buffer, &ops_buffer, &layers_buffer, &tile_counts_buffer, &tile_shapes_buffer);
        let compute_group0 = create_compute_group0(device, &shared, &shapes_buffer, &ops_buffer, &layers_buffer);
        let compute_group1 = create_compute_group1(device, &shared, &compute_uniform_buffer, &tile_counts_buffer, &tile_shapes_buffer);

        Self {
            shared, draw_data_buffer, shapes_buffer, ops_buffer, layers_buffer,
            tile_counts_buffer, tile_shapes_buffer, tile_capacity: 256, spatial_index_gen: 0,
            compute_uniform_buffer, compute_uniform_scratch: Vec::new(),
            render_group0, compute_group0, compute_group1,
            bind_group_gens: (0, 0, 0, 0, 0),
            total_tiles: 0, draw_index: AtomicU32::new(0),
            compile_scratch: Vec::new(), frame_stats: types::SdfStats::default(),
        }
    }

    fn trim(&mut self) {
        self.frame_stats.tile_count = self.total_tiles;
        if let Ok(mut s) = LAST_STATS.lock() { *s = self.frame_stats.clone(); }
        self.frame_stats = types::SdfStats::default();
        self.draw_data_buffer.clear();
        self.shapes_buffer.clear();
        self.ops_buffer.clear();
        self.layers_buffer.clear();
        self.total_tiles = 0;
        self.draw_index.store(0, Ordering::Relaxed);
    }
}

#[allow(clippy::too_many_arguments)]
fn create_render_group0(
    device: &Device, shared: &SharedSdfResources,
    draws: &buffer::Buffer<types::DrawData>, shapes: &buffer::Buffer<types::ShapeInstance>,
    ops: &buffer::Buffer<types::SdfOp>, layers: &buffer::Buffer<types::SdfLayer>,
    tile_counts: &iced::wgpu::Buffer, tile_shapes: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_render_g0"), layout: &shared.render_group0_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: draws.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: shapes.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: ops.as_entire_binding() },
            BindGroupEntry { binding: 3, resource: layers.as_entire_binding() },
            BindGroupEntry { binding: 4, resource: tile_counts.as_entire_binding() },
            BindGroupEntry { binding: 5, resource: tile_shapes.as_entire_binding() },
        ],
    })
}

fn create_compute_group0(
    device: &Device, shared: &SharedSdfResources,
    shapes: &buffer::Buffer<types::ShapeInstance>, ops: &buffer::Buffer<types::SdfOp>,
    layers: &buffer::Buffer<types::SdfLayer>,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g0"), layout: &shared.compute_group0_layout,
        entries: &[
            BindGroupEntry { binding: 1, resource: shapes.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: ops.as_entire_binding() },
            BindGroupEntry { binding: 3, resource: layers.as_entire_binding() },
        ],
    })
}

fn create_compute_group1(
    device: &Device, shared: &SharedSdfResources,
    uniforms: &iced::wgpu::Buffer, tile_counts: &iced::wgpu::Buffer, tile_shapes: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g1"), layout: &shared.compute_group1_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: tile_counts.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: tile_shapes.as_entire_binding() },
        ],
    })
}

impl Primitive for SdfPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self, pipeline: &mut Self::Pipeline, device: &Device, queue: &Queue,
        bounds: &Rectangle, viewport: &Viewport,
    ) {
        if self.shapes.is_empty() {
            let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData::default());
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let shape_start = pipeline.shapes_buffer.len() as u32;

        // Upload shapes, ops, layers
        for entry in &self.shapes {
            compile_into(entry.shape.node(), &mut pipeline.compile_scratch);
            let ops_offset = pipeline.ops_buffer.len() as u32;
            let ops_count = pipeline.compile_scratch.len() as u32;
            let _ = pipeline.ops_buffer.push_bulk(device, queue, &pipeline.compile_scratch);

            let layers_offset = pipeline.layers_buffer.len() as u32;
            let layers_count = entry.layers.len() as u32;
            let gpu_layers: SmallVec<[types::SdfLayer; 3]> = entry.layers.iter().map(|l| l.to_gpu()).collect();
            let _ = pipeline.layers_buffer.push_bulk(device, queue, &gpu_layers);

            let max_radius = entry.layers.iter().map(|l| l.max_effect_radius()).fold(0.0f32, f32::max);
            let has_fill = entry.layers.iter().any(|l| l.is_fill());

            let _ = pipeline.shapes_buffer.push(device, queue, types::ShapeInstance {
                bounds: types::GpuVec4::from(entry.bounds),
                ops_offset, ops_count, layers_offset, layers_count,
                max_radius, has_fill: u32::from(has_fill), _pad2: 0, _pad3: 0,
            });
        }

        let shape_count = self.shapes.len() as u32;
        let camera_pos = types::GpuVec2::new(self.camera_position.0, self.camera_position.1);
        let grid_origin = types::GpuVec2::new(bounds.x * scale, bounds.y * scale);
        let grid_cols = ((bounds.width * scale / TILE_SIZE).ceil() as u32).max(1);
        let grid_rows = ((bounds.height * scale / TILE_SIZE).ceil() as u32).max(1);

        // Allocate tile region
        let tile_base = pipeline.total_tiles;
        pipeline.total_tiles += grid_cols * grid_rows;

        // Grow spatial index if needed
        if pipeline.total_tiles > pipeline.tile_capacity {
            let new_cap = (pipeline.total_tiles as f32 * 1.5) as u32;
            let (tc, ts) = create_spatial_index_buffers(device, new_cap);
            pipeline.tile_counts_buffer = tc;
            pipeline.tile_shapes_buffer = ts;
            pipeline.tile_capacity = new_cap;
            pipeline.spatial_index_gen += 1;
        }

        // Write compute uniforms
        let cu = types::ComputeUniforms {
            bounds_origin: grid_origin,
            camera_position: camera_pos,
            camera_zoom: self.camera_zoom, scale_factor: scale,
            grid_cols, grid_rows,
            tile_size: TILE_SIZE, tile_base, shape_start, shape_count,
        };
        pipeline.compute_uniform_scratch.clear();
        let mut w = encase::UniformBuffer::new(&mut pipeline.compute_uniform_scratch);
        w.write(&cu).expect("Failed to write compute uniforms");
        queue.write_buffer(&pipeline.compute_uniform_buffer, 0, &pipeline.compute_uniform_scratch);

        // Push DrawData
        let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData {
            bounds_origin: grid_origin,
            camera_position: camera_pos,
            camera_zoom: self.camera_zoom, scale_factor: scale,
            time: self.time, debug_flags: self.debug_flags,
            grid_cols, grid_rows, tile_base,
            shape_start: 0, shape_count: 0, _pad0: 0, _pad1: 0,
        });

        // Recreate bind groups if any buffer changed
        let gens = (
            pipeline.draw_data_buffer.generation(),
            pipeline.shapes_buffer.generation(),
            pipeline.ops_buffer.generation(),
            pipeline.layers_buffer.generation(),
            pipeline.spatial_index_gen,
        );
        if gens != pipeline.bind_group_gens {
            pipeline.render_group0 = create_render_group0(
                device, &pipeline.shared,
                &pipeline.draw_data_buffer, &pipeline.shapes_buffer,
                &pipeline.ops_buffer, &pipeline.layers_buffer,
                &pipeline.tile_counts_buffer, &pipeline.tile_shapes_buffer,
            );
            pipeline.compute_group0 = create_compute_group0(
                device, &pipeline.shared, &pipeline.shapes_buffer, &pipeline.ops_buffer,
                &pipeline.layers_buffer,
            );
            pipeline.compute_group1 = create_compute_group1(
                device, &pipeline.shared, &pipeline.compute_uniform_buffer,
                &pipeline.tile_counts_buffer, &pipeline.tile_shapes_buffer,
            );
            pipeline.bind_group_gens = gens;
        }

        // Dispatch compute
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sdf_compute"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
                label: Some("sdf_spatial_index"), timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline.shared.compute_pipeline);
            pass.set_bind_group(0, &pipeline.compute_group0, &[]);
            pass.set_bind_group(1, &pipeline.compute_group1, &[]);
            pass.dispatch_workgroups(grid_cols.div_ceil(16), grid_rows.div_ceil(16), 1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        pipeline.frame_stats.shape_count += shape_count;
        pipeline.frame_stats.prepare_cpu_us += prepare_start.elapsed().as_micros() as u64;
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut iced::wgpu::RenderPass<'_>) -> bool {
        let draw_idx = pipeline.draw_index.fetch_add(1, Ordering::Relaxed);
        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.render_group0, &[]);
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
        let p = SdfPrimitive::new();
        assert!(p.is_empty());
        assert_eq!(p.shape_count(), 0);
    }

    #[test]
    fn test_primitive_single() {
        let p = SdfPrimitive::single(Sdf::circle([0.0, 0.0], 10.0));
        assert_eq!(p.shape_count(), 1);
    }

    #[test]
    fn test_primitive_push() {
        let mut p = SdfPrimitive::new();
        let c = Sdf::circle([0.0, 0.0], 5.0);
        let l = [Layer::solid(iced::Color::WHITE)];
        p.push(&c, &l, [0.0, 0.0, 10.0, 10.0]);
        p.push(&c, &l, [20.0, 20.0, 10.0, 10.0]);
        assert_eq!(p.shape_count(), 2);
    }

    #[test]
    fn test_tile_base_accumulation() {
        let mut total: u32 = 0;
        let g0 = (160.0f32 / 16.0).ceil() as u32 * (80.0f32 / 16.0).ceil() as u32;
        let b0 = total; total += g0;
        let g1 = (96.0f32 / 16.0).ceil() as u32 * (48.0f32 / 16.0).ceil() as u32;
        let b1 = total; total += g1;
        assert_eq!(b0, 0);
        assert_eq!(b1, g0);
        assert_eq!(b0 + g0, b1);
        assert_eq!(total, g0 + g1);
    }

    #[test]
    fn test_has_animations_empty() {
        assert!(!SdfPrimitive::new().has_animations());
    }

    #[test]
    fn test_has_animations_static() {
        let mut p = SdfPrimitive::new();
        let shape = Sdf::circle([0.0, 0.0], 10.0);
        p.push(&shape, &[Layer::solid(iced::Color::WHITE)], [0.0, 0.0, 100.0, 100.0]);
        assert!(!p.has_animations());
    }

    #[test]
    fn test_has_animations_flow_layer() {
        let mut p = SdfPrimitive::new();
        let shape = Sdf::circle([0.0, 0.0], 10.0);
        let layer = Layer::stroke(iced::Color::WHITE, crate::Pattern::solid(2.0).flow(50.0));
        p.push(&shape, &[layer], [0.0, 0.0, 100.0, 100.0]);
        assert!(p.has_animations());
    }

    #[test]
    fn test_has_animations_animated_shape() {
        let mut p = SdfPrimitive::new();
        let shape = Sdf::circle([0.0, 0.0], 10.0).dash(5.0, 3.0, 1.0, 0.0, 20.0);
        p.push(&shape, &[Layer::solid(iced::Color::WHITE)], [0.0, 0.0, 100.0, 100.0]);
        assert!(p.has_animations());
    }
}
