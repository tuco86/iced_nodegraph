//! SDF rendering primitive for Iced.
//!
//! Each SdfPrimitive compiles its drawables into GPU buffers, dispatches
//! a compute shader to build the tile spatial index, then renders via
//! a fullscreen triangle that reads the index for per-tile evaluation.

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

use crate::compile::compile_drawable;
use crate::drawable::Drawable;
use crate::pipeline::{buffer, types};
use crate::shared::SharedSdfResources;
use crate::style::Style;

static LAST_STATS: Mutex<types::SdfStats> = Mutex::new(types::SdfStats {
    entry_count: 0, tile_count: 0, prepare_cpu_us: 0,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().unwrap().clone()
}

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 32;
// Each slot = 2 u32s (segment_idx, style_idx)
const SLOT_STRIDE: u32 = MAX_SLOTS_PER_TILE * 2;

#[derive(Debug, Clone)]
struct DrawEntry {
    drawable: Drawable,
    style: Style,
    #[allow(dead_code)]
    screen_bounds: [f32; 4],
}

/// SDF rendering primitive holding drawables with styles.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    entries: Vec<DrawEntry>,
    pub camera_position: (f32, f32),
    pub camera_zoom: f32,
    pub time: f32,
    pub debug_flags: u32,
}

impl SdfPrimitive {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug_flags: 0,
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self { entries: Vec::with_capacity(n), ..Self::new() }
    }

    pub fn push(
        &mut self, drawable: &Drawable, style: &Style, screen_bounds: [f32; 4],
    ) -> &mut Self {
        self.entries.push(DrawEntry {
            drawable: drawable.clone(),
            style: style.clone(),
            screen_bounds,
        });
        self
    }

    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    pub fn time(mut self, time: f32) -> Self { self.time = time; self }
    pub fn debug_flags(mut self, flags: u32) -> Self { self.debug_flags = flags; self }
    pub fn debug_tiles(self, enabled: bool) -> Self {
        self.debug_flags(if enabled { 1 } else { 0 })
    }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
    pub fn entry_count(&self) -> usize { self.entries.len() }

    pub fn has_animations(&self) -> bool {
        self.entries.iter().any(|e| e.style.is_animated())
    }
}

impl Default for SdfPrimitive {
    fn default() -> Self { Self::new() }
}

// --- Pipeline ---

pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    // Data buffers
    draw_data_buffer: buffer::Buffer<types::DrawData>,
    entries_buffer: buffer::Buffer<types::GpuDrawEntry>,
    segments_buffer: buffer::Buffer<types::GpuSegment>,
    styles_buffer: buffer::Buffer<types::GpuStyle>,
    // Spatial index
    tile_counts_buffer: iced::wgpu::Buffer,
    tile_entries_buffer: iced::wgpu::Buffer,
    tile_capacity: u32,
    spatial_index_gen: u64,
    // Compute
    compute_uniform_buffer: iced::wgpu::Buffer,
    compute_uniform_scratch: Vec<u8>,
    // Bind groups
    render_group0: BindGroup,
    compute_group0: BindGroup,
    compute_group1: BindGroup,
    bind_group_gens: (u64, u64, u64, u64, u64), // draws, entries, segments, styles, spatial
    // Frame state
    total_tiles: u32,
    draw_index: AtomicU32,
    segment_scratch: Vec<types::GpuSegment>,
    frame_stats: types::SdfStats,
}

fn create_tile_buffers(device: &Device, cap: u32) -> (iced::wgpu::Buffer, iced::wgpu::Buffer) {
    let cap = cap.max(1) as u64;
    (
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_counts"), size: cap * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false,
        }),
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_slots"), size: cap * SLOT_STRIDE as u64 * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn create_render_group0(
    device: &Device, shared: &SharedSdfResources,
    draws: &buffer::Buffer<types::DrawData>,
    entries: &buffer::Buffer<types::GpuDrawEntry>,
    segments: &buffer::Buffer<types::GpuSegment>,
    styles: &buffer::Buffer<types::GpuStyle>,
    tile_counts: &iced::wgpu::Buffer,
    tile_entries: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_render_g0"), layout: &shared.render_group0_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: draws.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: entries.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: segments.as_entire_binding() },
            BindGroupEntry { binding: 3, resource: styles.as_entire_binding() },
            BindGroupEntry { binding: 4, resource: tile_counts.as_entire_binding() },
            BindGroupEntry { binding: 5, resource: tile_entries.as_entire_binding() },
        ],
    })
}

fn create_compute_group0(
    device: &Device, shared: &SharedSdfResources,
    draws: &buffer::Buffer<types::DrawData>,
    entries: &buffer::Buffer<types::GpuDrawEntry>,
    segments: &buffer::Buffer<types::GpuSegment>,
    styles: &buffer::Buffer<types::GpuStyle>,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g0"), layout: &shared.compute_group0_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: draws.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: entries.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: segments.as_entire_binding() },
            BindGroupEntry { binding: 3, resource: styles.as_entire_binding() },
        ],
    })
}

fn create_compute_group1(
    device: &Device, shared: &SharedSdfResources,
    uniforms: &iced::wgpu::Buffer,
    tile_counts: &iced::wgpu::Buffer,
    tile_entries: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g1"), layout: &shared.compute_group1_layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: tile_counts.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: tile_entries.as_entire_binding() },
        ],
    })
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        let usage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let draw_data_buffer = buffer::Buffer::new(device, Some("sdf_draws"), usage);
        let entries_buffer = buffer::Buffer::new(device, Some("sdf_entries"), usage);
        let segments_buffer = buffer::Buffer::new(device, Some("sdf_segments"), usage);
        let styles_buffer = buffer::Buffer::new(device, Some("sdf_styles"), usage);

        let (tile_counts_buffer, tile_entries_buffer) = create_tile_buffers(device, 256);

        let compute_uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_compute_uniforms"),
            size: <types::ComputeUniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let render_group0 = create_render_group0(
            device, &shared, &draw_data_buffer, &entries_buffer,
            &segments_buffer, &styles_buffer, &tile_counts_buffer, &tile_entries_buffer,
        );
        let compute_group0 = create_compute_group0(
            device, &shared, &draw_data_buffer, &entries_buffer, &segments_buffer, &styles_buffer,
        );
        let compute_group1 = create_compute_group1(
            device, &shared, &compute_uniform_buffer, &tile_counts_buffer, &tile_entries_buffer,
        );

        Self {
            shared, draw_data_buffer, entries_buffer, segments_buffer, styles_buffer,
            tile_counts_buffer, tile_entries_buffer, tile_capacity: 256, spatial_index_gen: 0,
            compute_uniform_buffer, compute_uniform_scratch: Vec::new(),
            render_group0, compute_group0, compute_group1,
            bind_group_gens: (0, 0, 0, 0, 0),
            total_tiles: 0, draw_index: AtomicU32::new(0),
            segment_scratch: Vec::new(), frame_stats: types::SdfStats::default(),
        }
    }

    fn trim(&mut self) {
        self.frame_stats.tile_count = self.total_tiles;
        if let Ok(mut s) = LAST_STATS.lock() { *s = self.frame_stats.clone(); }
        self.frame_stats = types::SdfStats::default();
        self.draw_data_buffer.clear();
        self.entries_buffer.clear();
        self.segments_buffer.clear();
        self.styles_buffer.clear();
        self.total_tiles = 0;
        self.draw_index.store(0, Ordering::Relaxed);
    }
}

impl Primitive for SdfPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self, pipeline: &mut Self::Pipeline, device: &Device, queue: &Queue,
        bounds: &Rectangle, viewport: &Viewport,
    ) {
        if self.entries.is_empty() {
            let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData::default());
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let entry_start = pipeline.entries_buffer.len() as u32;

        for (i, entry) in self.entries.iter().enumerate() {
            pipeline.segment_scratch.clear();

            let segment_offset = pipeline.segments_buffer.len() as u32;
            let (mut gpu_entry, gpu_style) = compile_drawable(
                &entry.drawable,
                &entry.style,
                i as u32,
                segment_offset,
                &mut pipeline.segment_scratch,
            );

            if !pipeline.segment_scratch.is_empty() {
                let _ = pipeline.segments_buffer.push_bulk(
                    device, queue, &pipeline.segment_scratch,
                );
            }

            let style_idx = pipeline.styles_buffer.push(device, queue, gpu_style);
            gpu_entry.style_idx = style_idx as u32;

            let _ = pipeline.entries_buffer.push(device, queue, gpu_entry);
        }

        let entry_count = self.entries.len() as u32;
        let camera_pos = types::GpuVec2::new(self.camera_position.0, self.camera_position.1);
        let grid_origin = types::GpuVec2::new(bounds.x * scale, bounds.y * scale);
        let grid_cols = ((bounds.width * scale / TILE_SIZE).ceil() as u32).max(1);
        let grid_rows = ((bounds.height * scale / TILE_SIZE).ceil() as u32).max(1);

        // Allocate tile region
        let tile_base = pipeline.total_tiles;
        pipeline.total_tiles += grid_cols * grid_rows;

        // Grow spatial index buffers if needed
        if pipeline.total_tiles > pipeline.tile_capacity {
            let new_cap = (pipeline.total_tiles as f32 * 1.5) as u32;
            let (tc, te) = create_tile_buffers(device, new_cap);
            pipeline.tile_counts_buffer = tc;
            pipeline.tile_entries_buffer = te;
            pipeline.tile_capacity = new_cap;
            pipeline.spatial_index_gen += 1;
        }

        // Write compute uniform: just the index into DrawData
        let draw_index = pipeline.draw_data_buffer.len() as u32; // will be this index after push
        let cu = types::ComputeUniforms {
            draw_index,
            _pad0: 0, _pad1: 0, _pad2: 0,
        };
        pipeline.compute_uniform_scratch.clear();
        let mut w = encase::UniformBuffer::new(&mut pipeline.compute_uniform_scratch);
        w.write(&cu).expect("Failed to write compute uniforms");
        queue.write_buffer(&pipeline.compute_uniform_buffer, 0, &pipeline.compute_uniform_scratch);

        // Push DrawData
        let _ = pipeline.draw_data_buffer.push(device, queue, types::DrawData {
            bounds_origin: grid_origin,
            camera_position: camera_pos,
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            debug_flags: self.debug_flags,
            entry_count,
            entry_start,
            grid_cols,
            grid_rows,
            tile_base,
            _pad0: 0, _pad1: 0, _pad2: 0,
        });

        // Recreate bind groups if any buffer generation changed
        let gens = (
            pipeline.draw_data_buffer.generation(),
            pipeline.entries_buffer.generation(),
            pipeline.segments_buffer.generation(),
            pipeline.styles_buffer.generation(),
            pipeline.spatial_index_gen,
        );
        if gens != pipeline.bind_group_gens {
            pipeline.render_group0 = create_render_group0(
                device, &pipeline.shared,
                &pipeline.draw_data_buffer, &pipeline.entries_buffer,
                &pipeline.segments_buffer, &pipeline.styles_buffer,
                &pipeline.tile_counts_buffer, &pipeline.tile_entries_buffer,
            );
            pipeline.compute_group0 = create_compute_group0(
                device, &pipeline.shared,
                &pipeline.draw_data_buffer, &pipeline.entries_buffer,
                &pipeline.segments_buffer, &pipeline.styles_buffer,
            );
            pipeline.compute_group1 = create_compute_group1(
                device, &pipeline.shared,
                &pipeline.compute_uniform_buffer,
                &pipeline.tile_counts_buffer, &pipeline.tile_entries_buffer,
            );
            pipeline.bind_group_gens = gens;
        }

        // Dispatch compute shader to build spatial index
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

        pipeline.frame_stats.entry_count += entry_count;
        pipeline.frame_stats.prepare_cpu_us += prepare_start.elapsed().as_micros() as u64;
    }

    fn draw(
        &self, pipeline: &Self::Pipeline, render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
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

    #[test]
    fn test_primitive_empty() {
        let p = SdfPrimitive::new();
        assert!(p.is_empty());
        assert_eq!(p.entry_count(), 0);
        assert!(!p.has_animations());
    }

    #[test]
    fn test_primitive_push() {
        let mut p = SdfPrimitive::new();
        let d = crate::curve::Curve::line([0.0, 0.0], [10.0, 0.0]);
        let s = Style::stroke(iced::Color::WHITE, crate::pattern::Pattern::solid(2.0));
        p.push(&d, &s, [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(p.entry_count(), 1);
    }
}
