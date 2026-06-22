//! SDF rendering primitive for Iced.
//!
//! Each SdfPrimitive compiles its drawables into GPU buffers, dispatches
//! a compute shader to build the tile spatial index, then renders via
//! a fullscreen triangle that reads the index for per-tile evaluation.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

use bitflags::bitflags;
use encase::ShaderSize;
use iced::Rectangle;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, Device, Queue, TextureFormat,
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use std::collections::HashMap;

use crate::compile::{compile_drawable, compile_local_at, entry_referencing};
use crate::drawable::Drawable;
use crate::pipeline::{buffer, types};
use crate::recipe::{ShapeCache, ShapeExpr};
use crate::shared::SharedSdfResources;
use crate::style::Style;

static LAST_STATS: Mutex<types::SdfStats> = Mutex::new(types::SdfStats {
    entry_count: 0,
    tile_count: 0,
    prepare_cpu_us: 0,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 32;
// Each slot = 2 u32s (segment_idx, style_idx)
const SLOT_STRIDE: u32 = MAX_SLOTS_PER_TILE * 2;

bitflags! {
    /// Per-draw SDF debug visualization modes. The bit values are mirrored by
    /// matching `DEBUG_*` constants in `shader.wgsl`; keep them in sync.
    ///
    /// Modes are independent and may be combined, but [`DebugFlags::DISTANCE_FIELD`]
    /// and [`DebugFlags::HOVERED_TILE`] both replace normal band rendering, so the
    /// last one evaluated wins where they overlap.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct DebugFlags: u32 {
        /// Overlay tile borders, tinted by slot occupancy (green to red on a
        /// log scale), and mark empty tiles. Diagnoses the spatial index.
        const TILE_HEATMAP = 1 << 0;
        /// Replace band rendering with the raw IQ distance field of each entry.
        const DISTANCE_FIELD = 1 << 1;
        /// Show the IQ distance field built from only the segments held by the
        /// tile under the cursor, plus an occupancy readout bar. Visualizes what
        /// a single tile's 32-slot buffer actually contains (and whether it
        /// overflowed). Requires a mouse position (see [`SdfPrimitive::mouse`]).
        const HOVERED_TILE = 1 << 2;
    }
}

/// Where a draw entry's geometry comes from. `Drawable` is the world-baked v2
/// path; `Recipe` is the v3 dedup path - a position-free shape evaluated once by
/// the pipeline's frame-surviving `ShapeCache` and placed by `translate`.
#[derive(Debug, Clone)]
enum EntrySource {
    Drawable(Drawable),
    Recipe {
        expr: ShapeExpr,
        translate: [f32; 2],
    },
}

#[derive(Debug, Clone)]
struct DrawEntry {
    source: EntrySource,
    style: Style,
}

/// SDF rendering primitive holding drawables with styles.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    entries: Vec<DrawEntry>,
    pub camera_position: (f32, f32),
    pub camera_zoom: f32,
    pub time: f32,
    pub debug: DebugFlags,
    /// Cursor position in window-logical coordinates (the same space as the
    /// widget bounds passed to `prepare`). `prepare` maps it to tile-local
    /// physical pixels for [`DebugFlags::HOVERED_TILE`]. Off-widget by default.
    pub mouse: (f32, f32),
}

impl SdfPrimitive {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug: DebugFlags::empty(),
            mouse: (f32::MIN, f32::MIN),
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            entries: Vec::with_capacity(n),
            ..Self::new()
        }
    }

    /// Append a drawable with its style. The pipeline derives a per-draw AABB
    /// on the GPU from the drawable's world-space bounds, so callers do not
    /// supply a screen rectangle.
    pub fn push(&mut self, drawable: &Drawable, style: &Style) -> &mut Self {
        self.entries.push(DrawEntry {
            source: EntrySource::Drawable(drawable.clone()),
            style: style.clone(),
        });
        self
    }

    /// Append a cacheable shape recipe placed at world `translate` (the v3 dedup
    /// path). The pipeline evaluates the recipe once via its frame-surviving
    /// `ShapeCache` and reuses the geometry for every identical shape, so N
    /// identical nodes pay for ONE boolean. Geometry is stored local with the
    /// placement carried as a per-instance translate; it renders identically to
    /// the equivalent world-baked [`push`].
    pub fn push_recipe(
        &mut self,
        expr: ShapeExpr,
        translate: [f32; 2],
        style: &Style,
    ) -> &mut Self {
        self.entries.push(DrawEntry {
            source: EntrySource::Recipe { expr, translate },
            style: style.clone(),
        });
        self
    }

    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }
    pub fn debug(mut self, flags: DebugFlags) -> Self {
        self.debug = flags;
        self
    }
    /// Sets the cursor position (window-logical coordinates) used by
    /// [`DebugFlags::HOVERED_TILE`]. No effect unless that flag is set.
    pub fn mouse(mut self, x: f32, y: f32) -> Self {
        self.mouse = (x, y);
        self
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn has_animations(&self) -> bool {
        self.entries.iter().any(|e| e.style.is_animated())
    }
}

impl Default for SdfPrimitive {
    fn default() -> Self {
        Self::new()
    }
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
    /// Frame-surviving cache of evaluated shape recipes (v3 dedup). Lives on the
    /// persistent pipeline, NOT the per-frame primitive, and is deliberately not
    /// cleared by `trim` so a unique shape's boolean runs once across frames.
    shape_cache: ShapeCache,
    /// Per-FRAME map recipe-hash -> segment_start in this frame's segment buffer.
    /// The first instance of a shape uploads its segments; every later identical
    /// instance is a command referencing that range (GPU instancing). Cleared by
    /// `trim` because the segment buffer is rebuilt each frame.
    frame_shape_slots: HashMap<u64, u32>,
    /// GPU timestamp pair around the cull compute pass (R3), when supported.
    compute_timer: Option<ComputeTimer>,
}

/// GPU timestamp pair around the cull compute pass (R3). Present only when the
/// device has `TIMESTAMP_QUERY`; lets a measurement isolate cull GPU time from
/// the CPU/submit/fence overhead that dominates a synchronous wall-clock frame.
struct ComputeTimer {
    query_set: iced::wgpu::QuerySet,
    resolve: iced::wgpu::Buffer,
    readback: iced::wgpu::Buffer,
    period_ns: f32,
}

impl ComputeTimer {
    fn new(device: &Device, queue: &Queue) -> Self {
        let query_set = device.create_query_set(&iced::wgpu::QuerySetDescriptor {
            label: Some("sdf_compute_ts"),
            ty: iced::wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 16,
            usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 16,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            query_set,
            resolve,
            readback,
            period_ns: queue.get_timestamp_period(),
        }
    }
}

impl SdfPipeline {
    /// Number of `GpuSegment`s currently uploaded (this frame). With GPU
    /// instancing, identical shapes contribute their segments ONCE, so this
    /// tracks unique-shape geometry, not draw count.
    pub fn segment_count(&self) -> usize {
        self.segments_buffer.len()
    }

    /// Cull-compute GPU time of the last `prepare`, in milliseconds, when the
    /// device supports timestamps (R3). Blocks to map the readback, so use it in
    /// measurements, not the render loop. `None` without `TIMESTAMP_QUERY`.
    pub fn last_compute_ms(&self, device: &Device) -> Option<f64> {
        let t = self.compute_timer.as_ref()?;
        let slice = t.readback.slice(..);
        slice.map_async(iced::wgpu::MapMode::Read, |_| {});
        device
            .poll(iced::wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .ok()?;
        let data = slice.get_mapped_range();
        let ts: [u64; 2] = [
            u64::from_le_bytes(data[0..8].try_into().unwrap()),
            u64::from_le_bytes(data[8..16].try_into().unwrap()),
        ];
        let ms = ts[1].wrapping_sub(ts[0]) as f64 * t.period_ns as f64 / 1.0e6;
        drop(data);
        t.readback.unmap();
        Some(ms)
    }
}

fn create_tile_buffers(device: &Device, cap: u32) -> (iced::wgpu::Buffer, iced::wgpu::Buffer) {
    let cap = cap.max(1) as u64;
    let usage = BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC;
    (
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_counts"),
            size: cap * 4,
            usage,
            mapped_at_creation: false,
        }),
        device.create_buffer(&BufferDescriptor {
            label: Some("sdf_tile_slots"),
            size: cap * SLOT_STRIDE as u64 * 4,
            usage,
            mapped_at_creation: false,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn create_render_group0(
    device: &Device,
    shared: &SharedSdfResources,
    draws: &buffer::Buffer<types::DrawData>,
    entries: &buffer::Buffer<types::GpuDrawEntry>,
    segments: &buffer::Buffer<types::GpuSegment>,
    styles: &buffer::Buffer<types::GpuStyle>,
    tile_counts: &iced::wgpu::Buffer,
    tile_entries: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_render_g0"),
        layout: &shared.render_group0_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: draws.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: entries.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: segments.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: styles.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: tile_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: tile_entries.as_entire_binding(),
            },
        ],
    })
}

fn create_compute_group0(
    device: &Device,
    shared: &SharedSdfResources,
    draws: &buffer::Buffer<types::DrawData>,
    entries: &buffer::Buffer<types::GpuDrawEntry>,
    segments: &buffer::Buffer<types::GpuSegment>,
    styles: &buffer::Buffer<types::GpuStyle>,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g0"),
        layout: &shared.compute_group0_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: draws.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: entries.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: segments.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: styles.as_entire_binding(),
            },
        ],
    })
}

fn create_compute_group1(
    device: &Device,
    shared: &SharedSdfResources,
    uniforms: &iced::wgpu::Buffer,
    tile_counts: &iced::wgpu::Buffer,
    tile_entries: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g1"),
        layout: &shared.compute_group1_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: tile_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: tile_entries.as_entire_binding(),
            },
        ],
    })
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
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
            device,
            &shared,
            &draw_data_buffer,
            &entries_buffer,
            &segments_buffer,
            &styles_buffer,
            &tile_counts_buffer,
            &tile_entries_buffer,
        );
        let compute_group0 = create_compute_group0(
            device,
            &shared,
            &draw_data_buffer,
            &entries_buffer,
            &segments_buffer,
            &styles_buffer,
        );
        let compute_group1 = create_compute_group1(
            device,
            &shared,
            &compute_uniform_buffer,
            &tile_counts_buffer,
            &tile_entries_buffer,
        );

        Self {
            shared,
            draw_data_buffer,
            entries_buffer,
            segments_buffer,
            styles_buffer,
            tile_counts_buffer,
            tile_entries_buffer,
            tile_capacity: 256,
            spatial_index_gen: 0,
            compute_uniform_buffer,
            compute_uniform_scratch: Vec::new(),
            render_group0,
            compute_group0,
            compute_group1,
            bind_group_gens: (0, 0, 0, 0, 0),
            total_tiles: 0,
            draw_index: AtomicU32::new(0),
            segment_scratch: Vec::new(),
            frame_stats: types::SdfStats::default(),
            shape_cache: ShapeCache::new(4096),
            frame_shape_slots: HashMap::new(),
            compute_timer: if device
                .features()
                .contains(iced::wgpu::Features::TIMESTAMP_QUERY)
            {
                Some(ComputeTimer::new(device, queue))
            } else {
                None
            },
        }
    }

    fn trim(&mut self) {
        self.frame_stats.tile_count = self.total_tiles;
        if let Ok(mut s) = LAST_STATS.lock() {
            *s = self.frame_stats.clone();
        }
        self.frame_stats = types::SdfStats::default();
        self.draw_data_buffer.clear();
        self.entries_buffer.clear();
        self.segments_buffer.clear();
        self.styles_buffer.clear();
        self.frame_shape_slots.clear();
        self.total_tiles = 0;
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
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        if self.entries.is_empty() {
            let _ = pipeline
                .draw_data_buffer
                .push(device, queue, types::DrawData::default());
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let entry_start = pipeline.entries_buffer.len() as u32;

        // Accumulate this primitive's GPU data, then upload each buffer in ONE
        // bulk write. The old per-entry pushes issued ~3 `queue.write_buffer`
        // calls per entry (1500+ per frame for 500 nodes) - that submission
        // overhead, not the boolean, was what remained of v3's per-frame cost.
        let seg_base = pipeline.segments_buffer.len() as u32;
        let style_base = pipeline.styles_buffer.len() as u32;
        let mut seg_batch = std::mem::take(&mut pipeline.segment_scratch);
        seg_batch.clear();
        let mut style_batch: Vec<types::GpuStyle> = Vec::with_capacity(self.entries.len());
        let mut entry_batch: Vec<types::GpuDrawEntry> = Vec::with_capacity(self.entries.len());

        for (i, entry) in self.entries.iter().enumerate() {
            let segment_offset = seg_base + seg_batch.len() as u32;
            let (mut gpu_entry, gpu_style) = match &entry.source {
                EntrySource::Drawable(drawable) => compile_drawable(
                    drawable,
                    &entry.style,
                    i as u32,
                    segment_offset,
                    &mut seg_batch,
                ),
                EntrySource::Recipe { expr, translate } => {
                    // Evaluate once (cached across frames), then place by
                    // translate. The clone breaks the cache borrow before the
                    // batch is touched; it copies arcs, not the boolean.
                    let local = pipeline.shape_cache.get_or_eval(expr).clone();
                    let hash = expr.recipe_hash();
                    if let Some(&shared_start) = pipeline.frame_shape_slots.get(&hash) {
                        // Segments already in the batch this frame: one tiny
                        // command referencing the shared range, NO new segments.
                        entry_referencing(&local, &entry.style, i as u32, *translate, shared_start)
                    } else {
                        pipeline.frame_shape_slots.insert(hash, segment_offset);
                        compile_local_at(
                            &local,
                            &entry.style,
                            i as u32,
                            *translate,
                            segment_offset,
                            &mut seg_batch,
                        )
                    }
                }
            };
            gpu_entry.style_idx = style_base + style_batch.len() as u32;
            style_batch.push(gpu_style);
            entry_batch.push(gpu_entry);
        }

        let _ = pipeline
            .segments_buffer
            .push_bulk(device, queue, &seg_batch);
        let _ = pipeline
            .styles_buffer
            .push_bulk(device, queue, &style_batch);
        let _ = pipeline
            .entries_buffer
            .push_bulk(device, queue, &entry_batch);
        seg_batch.clear();
        pipeline.segment_scratch = seg_batch; // restore the reused allocation

        let entry_count = self.entries.len() as u32;
        let camera_pos = types::GpuVec2::new(self.camera_position.0, self.camera_position.1);
        let grid_origin = types::GpuVec2::new(bounds.x * scale, bounds.y * scale);
        let grid_cols = ((bounds.width * scale / TILE_SIZE).ceil() as u32).max(1);
        let grid_rows = ((bounds.height * scale / TILE_SIZE).ceil() as u32).max(1);

        // Allocate tile region
        let tile_base = pipeline.total_tiles;
        pipeline.total_tiles += grid_cols * grid_rows;

        // Grow spatial index buffers if needed. Earlier primitives in this
        // frame have already written their tile data to the current buffers
        // and submitted compute dispatches against them; subsequent renders
        // read tile data from the *current* buffer state. We must therefore
        // copy the populated range into the new buffer before swapping —
        // otherwise prior primitives render as empty tiles.
        if pipeline.total_tiles > pipeline.tile_capacity {
            let new_cap = (pipeline.total_tiles as f32 * 1.5) as u32;
            let (tc, te) = create_tile_buffers(device, new_cap);
            let preserved_tiles = tile_base as u64;
            if preserved_tiles > 0 {
                let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("sdf_tile_grow_copy"),
                });
                encoder.copy_buffer_to_buffer(
                    &pipeline.tile_counts_buffer,
                    0,
                    &tc,
                    0,
                    preserved_tiles * 4,
                );
                encoder.copy_buffer_to_buffer(
                    &pipeline.tile_entries_buffer,
                    0,
                    &te,
                    0,
                    preserved_tiles * SLOT_STRIDE as u64 * 4,
                );
                queue.submit(std::iter::once(encoder.finish()));
            }
            pipeline.tile_counts_buffer = tc;
            pipeline.tile_entries_buffer = te;
            pipeline.tile_capacity = new_cap;
            pipeline.spatial_index_gen += 1;
        }

        // Write compute uniform: just the index into DrawData
        let draw_index = pipeline.draw_data_buffer.len() as u32; // will be this index after push
        let cu = types::ComputeUniforms {
            draw_index,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        pipeline.compute_uniform_scratch.clear();
        let mut w = encase::UniformBuffer::new(&mut pipeline.compute_uniform_scratch);
        w.write(&cu).expect("Failed to write compute uniforms");
        queue.write_buffer(
            &pipeline.compute_uniform_buffer,
            0,
            &pipeline.compute_uniform_scratch,
        );

        // Cursor in tile-local physical pixels (matches the shader's `local_px`).
        let mouse_px = types::GpuVec2::new(
            (self.mouse.0 - bounds.x) * scale,
            (self.mouse.1 - bounds.y) * scale,
        );

        // Push DrawData
        let _ = pipeline.draw_data_buffer.push(
            device,
            queue,
            types::DrawData {
                bounds_origin: grid_origin,
                camera_position: camera_pos,
                camera_zoom: self.camera_zoom,
                scale_factor: scale,
                time: self.time,
                debug_flags: self.debug.bits(),
                entry_count,
                entry_start,
                grid_cols,
                grid_rows,
                tile_base,
                _pad0: 0,
                mouse_px,
            },
        );

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
                device,
                &pipeline.shared,
                &pipeline.draw_data_buffer,
                &pipeline.entries_buffer,
                &pipeline.segments_buffer,
                &pipeline.styles_buffer,
                &pipeline.tile_counts_buffer,
                &pipeline.tile_entries_buffer,
            );
            pipeline.compute_group0 = create_compute_group0(
                device,
                &pipeline.shared,
                &pipeline.draw_data_buffer,
                &pipeline.entries_buffer,
                &pipeline.segments_buffer,
                &pipeline.styles_buffer,
            );
            pipeline.compute_group1 = create_compute_group1(
                device,
                &pipeline.shared,
                &pipeline.compute_uniform_buffer,
                &pipeline.tile_counts_buffer,
                &pipeline.tile_entries_buffer,
            );
            pipeline.bind_group_gens = gens;
        }

        // Dispatch compute shader to build spatial index
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sdf_compute"),
        });
        let ts_writes =
            pipeline
                .compute_timer
                .as_ref()
                .map(|t| iced::wgpu::ComputePassTimestampWrites {
                    query_set: &t.query_set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                });
        {
            let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
                label: Some("sdf_spatial_index"),
                timestamp_writes: ts_writes,
            });
            pass.set_pipeline(&pipeline.shared.compute_pipeline);
            pass.set_bind_group(0, &pipeline.compute_group0, &[]);
            pass.set_bind_group(1, &pipeline.compute_group1, &[]);
            pass.dispatch_workgroups(grid_cols.div_ceil(16), grid_rows.div_ceil(16), 1);
        }
        // Resolve the timestamp pair so `last_compute_ms` can read the cull time.
        if let Some(t) = &pipeline.compute_timer {
            encoder.resolve_query_set(&t.query_set, 0..2, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, 16);
        }
        queue.submit(std::iter::once(encoder.finish()));

        pipeline.frame_stats.entry_count += entry_count;
        pipeline.frame_stats.prepare_cpu_us += prepare_start.elapsed().as_micros() as u64;
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
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
        p.push(&d, &s);
        assert_eq!(p.entry_count(), 1);
    }
}
