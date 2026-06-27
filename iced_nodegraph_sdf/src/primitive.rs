//! SDF rendering primitive for Iced.
//!
//! Each SdfPrimitive compiles its drawables into GPU buffers, dispatches
//! a compute shader to build the tile spatial index, then renders via
//! a fullscreen triangle that reads the index for per-tile evaluation.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

use bitflags::bitflags;
use iced::Rectangle;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, Device, Queue, TextureFormat,
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use std::collections::HashMap;

use crate::compile::{compile_local_at, entry_referencing};
use crate::pattern::PatternType;
use crate::pipeline::{buffer, types};
use crate::shape::{Shape, ShapeCache};
use crate::shared::SharedSdfResources;
use crate::style::{Style, Transfer};

static LAST_STATS: Mutex<types::SdfStats> = Mutex::new(types::SdfStats {
    entry_count: 0,
    tile_count: 0,
    prepare_cpu_us: 0,
    unique_shapes: 0,
    segment_count: 0,
    unique_styles: 0,
    cache_hits: 0,
    cache_misses: 0,
    cache_hit_rate: 0.0,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 128;
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
        /// a single tile's slot buffer actually contains (and whether it
        /// overflowed). Requires a mouse position (see [`SdfPrimitive::mouse`]).
        const HOVERED_TILE = 1 << 2;
    }
}

/// One queued draw: a position-free [`Shape`] (evaluated once by the pipeline's
/// frame-surviving `ShapeCache` when cacheable) placed at world `placement` (the
/// per-instance translate, excluded from the cache key), with its band `style`.
#[derive(Debug, Clone)]
struct DrawEntry {
    shape: Shape,
    placement: [f32; 2],
    style: Style,
}

/// SDF rendering primitive holding drawables with styles.
#[derive(Debug)]
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
    /// Hint that this primitive is the static, full-coverage background (the
    /// bottom tiling). When set, the pipeline may cache its rendered output to a
    /// texture and blit it on frames whose camera and content are unchanged,
    /// cutting the one fullscreen fragment pass the tile cull cannot prune. Inert
    /// for any non-background primitive.
    pub cache_background: bool,
    /// The `DrawData` slot this primitive was assigned in `prepare`, stored on the
    /// primitive itself rather than derived from draw order. iced PREPARES every
    /// queued instance but SKIPS drawing those whose bounds snap empty or fall off
    /// the viewport; a draw-order counter would then hand every later primitive the
    /// wrong slot (wrong camera/tiles -> misrendered fill, missing border/pins).
    /// Interior-mutable because `Primitive::prepare` takes `&self`.
    draw_slot: AtomicU32,
}

impl Clone for SdfPrimitive {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            camera_position: self.camera_position,
            camera_zoom: self.camera_zoom,
            time: self.time,
            debug: self.debug,
            mouse: self.mouse,
            cache_background: self.cache_background,
            draw_slot: AtomicU32::new(self.draw_slot.load(Ordering::Relaxed)),
        }
    }
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
            cache_background: false,
            draw_slot: AtomicU32::new(0),
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            entries: Vec::with_capacity(n),
            ..Self::new()
        }
    }

    /// Append a [`Shape`] with its `style`, placed at world `placement`. This is
    /// the single geometry input. The pipeline evaluates the shape once via its
    /// frame-surviving `ShapeCache` (for cacheable booleans) and reuses the
    /// geometry for every identical shape, so N identical nodes pay for ONE
    /// boolean; `placement` is carried as a per-instance translate, kept OUT of
    /// the cache key so two identical shapes at different positions share a slot.
    /// The per-draw AABB is derived on the GPU from the evaluated geometry, so
    /// callers do not supply a screen rectangle.
    pub fn push(&mut self, shape: &Shape, style: &Style, placement: [f32; 2]) -> &mut Self {
        self.entries.push(DrawEntry {
            shape: shape.clone(),
            placement,
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
    /// Marks this primitive as the cacheable static background (see
    /// [`SdfPrimitive::cache_background`]).
    pub fn background(mut self) -> Self {
        self.cache_background = true;
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

    /// Hashes everything that determines this primitive's COMPILED geometry buffers
    /// (each entry's shape, world placement and style) - but NOT the camera, time or
    /// debug flags, which live in `DrawData` and never touch the segment/entry/style
    /// buffers. Two frames with an equal hash produce byte-identical geometry, so
    /// `prepare` can skip the re-evaluation and re-upload (see the slot reuse path).
    fn geometry_hash(&self) -> u64 {
        let mut h = KeyHasher::new();
        h.u32(self.entries.len() as u32);
        for e in &self.entries {
            h.u64(e.shape.hash());
            h.f32(e.placement[0]);
            h.f32(e.placement[1]);
            hash_style_into(&mut h, &e.style);
        }
        h.0
    }
}

/// FNV-1a hasher for the background cache key (deterministic, native==wasm).
struct KeyHasher(u64);

impl KeyHasher {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn u32(&mut self, x: u32) {
        self.0 ^= x as u64;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn f32(&mut self, x: f32) {
        // Canonicalize so -0.0 == 0.0 and all NaNs collapse (key stability).
        let b = if x == 0.0 {
            0
        } else if x.is_nan() {
            0x7fc0_0000
        } else {
            x.to_bits()
        };
        self.u32(b);
    }
    fn color(&mut self, c: iced::Color) {
        self.f32(c.r);
        self.f32(c.g);
        self.f32(c.b);
        self.f32(c.a);
    }
    fn u64(&mut self, x: u64) {
        self.u32(x as u32);
        self.u32((x >> 32) as u32);
    }
}

/// Folds a [`Style`]'s geometry-relevant recipe (stops, pattern, transfer, df) into
/// `h`. Used by [`SdfPrimitive::geometry_hash`] to detect whether a primitive's
/// compiled output would differ from last frame - so the camera/time are NOT here.
fn hash_style_into(h: &mut KeyHasher, s: &Style) {
    h.u32(s.stops.len() as u32);
    for st in &s.stops {
        h.f32(st.dist);
        h.color(st.start);
        h.color(st.end);
    }
    h.u32(s.distance_field as u32);
    match s.transfer {
        Transfer::Linear => h.u32(0),
        Transfer::Smoothstep => h.u32(1),
        Transfer::Gamma(g) => {
            h.u32(2);
            h.f32(g);
        }
    }
    match &s.pattern {
        None => h.u32(0),
        Some(p) => {
            h.u32(1);
            h.f32(p.thickness);
            h.f32(p.flow_speed);
            match p.pattern_type {
                PatternType::Solid => h.u32(0),
                PatternType::Dashed { dash, gap, angle } => {
                    h.u32(1);
                    h.f32(dash);
                    h.f32(gap);
                    h.f32(angle);
                }
                PatternType::Arrowed {
                    segment,
                    gap,
                    angle,
                } => {
                    h.u32(2);
                    h.f32(segment);
                    h.f32(gap);
                    h.f32(angle);
                }
                PatternType::Dotted { spacing, radius } => {
                    h.u32(3);
                    h.f32(spacing);
                    h.f32(radius);
                }
                PatternType::DashDotted {
                    dash,
                    gap,
                    dot_radius,
                } => {
                    h.u32(4);
                    h.f32(dash);
                    h.f32(gap);
                    h.f32(dot_radius);
                }
                PatternType::ArrowDotted {
                    segment,
                    gap,
                    dot_radius,
                } => {
                    h.u32(5);
                    h.f32(segment);
                    h.f32(gap);
                    h.f32(dot_radius);
                }
            }
        }
    }
}

/// Hash a compiled [`types::GpuStyle`] for per-frame style deduplication. Folds
/// every field that reaches the GPU (stop colours/distances + pattern + transfer)
/// so byte-identical styles collide and share one slot. Padding is excluded; it
/// is always zero and carries no rendered state.
fn hash_gpu_style(s: &types::GpuStyle) -> u64 {
    let mut h = KeyHasher::new();
    for v in s
        .stop_start
        .iter()
        .chain(s.stop_end.iter())
        .chain(s.stop_dist.iter())
    {
        for &c in v.as_ref() {
            h.f32(c);
        }
    }
    h.u32(s.stop_count);
    h.u32(s.flags);
    h.u32(s.pattern_type);
    h.f32(s.pattern_thickness);
    h.f32(s.pattern_param0);
    h.f32(s.pattern_param1);
    h.f32(s.pattern_param2);
    h.f32(s.flow_speed);
    h.u32(s.transfer_type);
    h.f32(s.transfer_param);
    h.0
}

impl SdfPrimitive {
    /// Content key for the static-background cache, or `None` when this primitive
    /// is not a cacheable background. Only PURE, non-animated, un-patterned
    /// tilings cache; anything else returns `None` so it renders direct and is
    /// never served stale. Captures everything that changes the rendered pixels -
    /// camera, grid placement, scale, tiling type/params, and stop colours - but
    /// NOT `time` (a cacheable background does not read it).
    fn background_key(
        &self,
        grid_origin: (f32, f32),
        grid_cols: u32,
        grid_rows: u32,
        scale: f32,
    ) -> Option<u64> {
        if self.entries.is_empty() {
            return None;
        }
        let mut h = KeyHasher::new();
        h.f32(self.camera_position.0);
        h.f32(self.camera_position.1);
        h.f32(self.camera_zoom);
        h.f32(scale);
        h.f32(grid_origin.0);
        h.f32(grid_origin.1);
        h.u32(grid_cols);
        h.u32(grid_rows);
        for e in &self.entries {
            let Shape::Tiling(t) = &e.shape else {
                return None;
            };
            if e.style.is_animated() || e.style.pattern.is_some() {
                return None;
            }
            let (tt, params) = t.to_gpu();
            h.u32(tt as u32);
            for p in params {
                h.f32(p);
            }
            h.u32(e.style.stops.len() as u32);
            for s in &e.style.stops {
                h.f32(s.dist);
                h.color(s.start);
                h.color(s.end);
            }
            h.u32(e.style.distance_field as u32);
            // Transfer affects the blend; mix its discriminant + param.
            match e.style.transfer {
                crate::style::Transfer::Linear => h.u32(0),
                crate::style::Transfer::Smoothstep => h.u32(1),
                crate::style::Transfer::Gamma(g) => {
                    h.u32(2);
                    h.f32(g);
                }
            }
        }
        Some(h.0)
    }
}

impl Default for SdfPrimitive {
    fn default() -> Self {
        Self::new()
    }
}

// --- Pipeline ---

/// Per draw-slot record of what a primitive wrote into the persistent buffers last
/// frame: its geometry hash plus the exact ranges it occupies. A primitive at the
/// same slot whose hash matches AND whose buffer cursors line up with these starts
/// reuses the resident data (no eval/upload) by skipping over the ranges.
#[derive(Clone, Copy)]
struct SlotState {
    geom_hash: u64,
    seg_start: u32,
    seg_count: u32,
    entry_start: u32,
    entry_count: u32,
    style_start: u32,
    style_count: u32,
}

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
    // Bind groups
    render_group0: BindGroup,
    compute_group0: BindGroup,
    compute_group1: BindGroup,
    bind_group_gens: (u64, u64, u64, u64, u64), // draws, entries, segments, styles, spatial
    // Frame state
    total_tiles: u32,
    // Deferred cull-compute: each prepare only RECORDS its dispatch params here;
    // the FIRST draw runs ALL culls in one encoder + one `queue.submit` (was one
    // submit per primitive - ~70us each, the dominant prepare cost). Because the
    // dispatch is recorded after every prepare, the tile buffer can grow freely
    // during prepares with NO copy (nothing is computed until the end). Mutex/
    // AtomicBool (not RefCell/Cell) because the Pipeline must be Sync per iced's
    // `Primitive` bound. `frame_device`/`frame_queue` are cloned in prepare so the
    // immutable `draw` can build + submit the batched compute.
    pending_dispatches: Mutex<Vec<(u32, u32)>>, // (grid_cols, grid_rows) per culled draw
    pending_bg_populate: Mutex<Option<u32>>,    // draw_index of a bg to cache
    compute_submitted: std::sync::atomic::AtomicBool,
    frame_device: Option<Device>,
    frame_queue: Option<Queue>,
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
    /// Per-FRAME map compiled-style-hash -> `style_idx` in this frame's style
    /// buffer. Mirrors `frame_shape_slots` for styles: the first entry with a
    /// given look uploads one `GpuStyle`; identical entries (e.g. every node with
    /// the same fill) reuse that slot instead of duplicating ~336 bytes each.
    /// Cleared by `trim` because the style buffer is rebuilt each frame.
    frame_style_slots: HashMap<u64, u32>,
    /// Per draw-slot geometry record from the LAST frame, indexed by draw slot.
    /// Survives `trim` (unlike the per-frame maps): it is what this frame's
    /// primitives compare against to skip re-evaluating unchanged geometry. A slot
    /// holds `None` until first written; structural changes overwrite it.
    slots: Vec<Option<SlotState>>,
    /// GPU timestamp pair around the cull compute pass (R3), when supported.
    compute_timer: Option<ComputeTimer>,
    /// Static-background texture cache (Phase C). Survives frames; populated only
    /// for the background primitive (the widget marks it).
    bg_cache: crate::pipeline::bg_cache::BgCache,
    /// Draw-call index of the background this frame when it should be blitted
    /// from the cache instead of rendered (cache populate/hit); `None` = render
    /// the background normally (direct).
    bg_blit_index: Option<u32>,
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

    /// Shape-cache hit rate over the pipeline's lifetime (Improvement A). ~1.0 on
    /// a static graph is the R4 contract.
    pub fn cache_hit_rate(&self) -> f32 {
        self.shape_cache.hit_rate()
    }

    /// Shape-cache misses (boolean->arcs evaluations) over the lifetime.
    pub fn cache_misses(&self) -> u64 {
        self.shape_cache.misses()
    }

    /// Whether the most recently prepared frame served its background from the
    /// texture cache (blit) instead of rendering it directly. Diagnostic hook for
    /// the static-background cache gate.
    pub fn bg_cache_blitted(&self) -> bool {
        self.bg_blit_index.is_some()
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
    tile_counts: &iced::wgpu::Buffer,
    tile_entries: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_compute_g1"),
        layout: &shared.compute_group1_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: tile_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
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
        let compute_group1 =
            create_compute_group1(device, &shared, &tile_counts_buffer, &tile_entries_buffer);

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
            render_group0,
            compute_group0,
            compute_group1,
            bind_group_gens: (0, 0, 0, 0, 0),
            total_tiles: 0,
            pending_dispatches: Mutex::new(Vec::new()),
            pending_bg_populate: Mutex::new(None),
            compute_submitted: std::sync::atomic::AtomicBool::new(false),
            frame_device: None,
            frame_queue: None,
            segment_scratch: Vec::new(),
            frame_stats: types::SdfStats::default(),
            shape_cache: ShapeCache::new(4096),
            frame_shape_slots: HashMap::new(),
            frame_style_slots: HashMap::new(),
            slots: Vec::new(),
            compute_timer: if device
                .features()
                .contains(iced::wgpu::Features::TIMESTAMP_QUERY)
            {
                Some(ComputeTimer::new(device, queue))
            } else {
                None
            },
            bg_cache: crate::pipeline::bg_cache::BgCache::new(device, format),
            bg_blit_index: None,
        }
    }

    fn trim(&mut self) {
        // Capture frame metrics from the buffers/cache BEFORE clearing them.
        self.frame_stats.tile_count = self.total_tiles;
        self.frame_stats.segment_count = self.segments_buffer.len() as u32;
        self.frame_stats.unique_shapes = self.frame_shape_slots.len() as u32;
        self.frame_stats.unique_styles = self.frame_style_slots.len() as u32;
        self.frame_stats.cache_hits = self.shape_cache.hits();
        self.frame_stats.cache_misses = self.shape_cache.misses();
        self.frame_stats.cache_hit_rate = self.shape_cache.hit_rate();
        if let Ok(mut s) = LAST_STATS.lock() {
            *s = self.frame_stats.clone();
        }
        self.frame_stats = types::SdfStats::default();
        self.draw_data_buffer.clear();
        self.entries_buffer.clear();
        self.segments_buffer.clear();
        self.styles_buffer.clear();
        self.frame_shape_slots.clear();
        self.frame_style_slots.clear();
        self.total_tiles = 0;
        self.bg_blit_index = None;
        self.pending_dispatches.get_mut().unwrap().clear();
        *self.pending_bg_populate.get_mut().unwrap() = None;
        self.compute_submitted
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl SdfPipeline {
    /// Records the fullscreen-triangle SDF instance draw for `draw_index` (set
    /// render pipeline + group0 + draw `0..3`). Shared by the live `draw` and the
    /// deferred background-cache populate so the two cannot drift.
    fn record_sdf_instance(&self, pass: &mut iced::wgpu::RenderPass<'_>, draw_index: u32) {
        pass.set_pipeline(&self.shared.render_pipeline);
        pass.set_bind_group(0, &self.render_group0, &[]);
        pass.draw(0..3, draw_index..draw_index + 1);
    }

    /// Runs every cull dispatch recorded this frame in ONE encoder + ONE submit,
    /// then any deferred background-cache populate. Called once, from the first
    /// `draw` (all prepares are complete, so the buffers are final). Replaces the
    /// former one-submit-per-primitive path, whose `queue.submit` overhead was the
    /// dominant `prepare` cost.
    fn run_deferred_compute(&self) {
        let (Some(device), Some(queue)) = (self.frame_device.as_ref(), self.frame_queue.as_ref())
        else {
            return;
        };
        let dispatches = self.pending_dispatches.lock().unwrap();
        if !dispatches.is_empty() {
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("sdf_compute_batch"),
            });
            let ts_writes =
                self.compute_timer
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
                pass.set_pipeline(&self.shared.compute_pipeline);
                pass.set_bind_group(0, &self.compute_group0, &[]);
                pass.set_bind_group(1, &self.compute_group1, &[]);
                // One dispatch for the whole frame: the z-axis is the draw index
                // (read as workgroup_id.z), so each draw selects its own DrawData
                // without a per-draw uniform. x/y are sized to the LARGEST draw
                // grid; smaller draws' surplus workgroups self-abort at the grid
                // bound (`col/row >= grid_cols/rows`). z spans every draw, so the
                // few non-culled slots (empty/blit) get a layer that returns at
                // once. z is bounded by maxComputeWorkgroupsPerDimension (65535).
                let max_cols = dispatches.iter().map(|&(c, _)| c).max().unwrap_or(0);
                let max_rows = dispatches.iter().map(|&(_, r)| r).max().unwrap_or(0);
                let draw_count = self.draw_data_buffer.len() as u32;
                pass.dispatch_workgroups(max_cols.div_ceil(16), max_rows.div_ceil(16), draw_count);
            }
            if let Some(t) = &self.compute_timer {
                encoder.resolve_query_set(&t.query_set, 0..2, &t.resolve, 0);
                encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, 16);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
        drop(dispatches);

        // Deferred background-cache populate: render the bg into the cache texture
        // now that its tiles are computed (the cull above ran first in the queue).
        if let Some(draw_index) = *self.pending_bg_populate.lock().unwrap() {
            let mut enc = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("sdf_bg_populate"),
            });
            {
                let mut pass = enc.begin_render_pass(&iced::wgpu::RenderPassDescriptor {
                    label: Some("sdf_bg_populate_pass"),
                    color_attachments: &[Some(iced::wgpu::RenderPassColorAttachment {
                        view: self.bg_cache.target_view(),
                        resolve_target: None,
                        ops: iced::wgpu::Operations {
                            load: iced::wgpu::LoadOp::Clear(iced::wgpu::Color::TRANSPARENT),
                            store: iced::wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                self.record_sdf_instance(&mut pass, draw_index);
            }
            queue.submit(std::iter::once(enc.finish()));
        }
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
            self.draw_slot
                .store(pipeline.draw_data_buffer.len() as u32, Ordering::Relaxed);
            let _ = pipeline
                .draw_data_buffer
                .push(device, queue, types::DrawData::default());
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let entry_start = pipeline.entries_buffer.len() as u32;
        let seg_start = pipeline.segments_buffer.len() as u32;
        let style_start = pipeline.styles_buffer.len() as u32;
        let draw_slot = pipeline.draw_data_buffer.len();

        // Skip the whole geometry rebuild when this slot's primitive is byte-for-
        // byte identical to last frame AND the buffer cursors line up with where it
        // wrote then (so no earlier primitive shifted the packed offsets). The
        // resident segments/entries/styles are then reused in place - no eval, no
        // upload - by advancing the cursors over them.
        let geom_hash = self.geometry_hash();
        let reuse = pipeline
            .slots
            .get(draw_slot)
            .copied()
            .flatten()
            .filter(|s| {
                s.geom_hash == geom_hash
                    && s.seg_start == seg_start
                    && s.entry_start == entry_start
                    && s.style_start == style_start
            });

        if let Some(s) = reuse {
            pipeline.segments_buffer.skip(s.seg_count as usize);
            pipeline.entries_buffer.skip(s.entry_count as usize);
            pipeline.styles_buffer.skip(s.style_count as usize);
        } else {
            // Accumulate this primitive's GPU data, then upload each buffer in ONE
            // bulk write. The old per-entry pushes issued ~3 `queue.write_buffer`
            // calls per entry (1500+ per frame for 500 nodes) - that submission
            // overhead, not the boolean, was what remained of v3's per-frame cost.
            let seg_base = seg_start;
            let style_base = style_start;
            let mut seg_batch = std::mem::take(&mut pipeline.segment_scratch);
            seg_batch.clear();
            let mut style_batch: Vec<types::GpuStyle> = Vec::with_capacity(self.entries.len());
            let mut entry_batch: Vec<types::GpuDrawEntry> = Vec::with_capacity(self.entries.len());

            for (i, entry) in self.entries.iter().enumerate() {
                let segment_offset = seg_base + seg_batch.len() as u32;
                // Evaluate the shape to LOCAL geometry: cacheable booleans come from
                // the frame-surviving cache (one boolean for all identical shapes);
                // cheap primitives and ephemeral strokes (edges) evaluate fresh. The
                // clone breaks the cache borrow before the batch is touched; it
                // copies arcs, not the boolean.
                let local = if entry.shape.is_cacheable() {
                    pipeline.shape_cache.get_or_eval(&entry.shape).clone()
                } else {
                    entry.shape.evaluate()
                };
                let hash = entry.shape.hash();
                let (mut gpu_entry, gpu_style) =
                    if let Some(&shared_start) = pipeline.frame_shape_slots.get(&hash) {
                        // Segments already in the batch this frame: one tiny command
                        // referencing the shared range, NO new segments uploaded.
                        entry_referencing(
                            &local,
                            &entry.style,
                            i as u32,
                            entry.placement,
                            shared_start,
                        )
                    } else {
                        pipeline.frame_shape_slots.insert(hash, segment_offset);
                        // Pass `seg_base` (the buffer base), NOT `segment_offset`:
                        // `compile_local_at` adds the batch position (`seg_batch.len()`)
                        // itself, so it lands at `seg_base + seg_batch.len()` =
                        // `segment_offset`. Passing the already-offset value would
                        // double-count the batch length, indexing every entry after the
                        // first past its real segments.
                        compile_local_at(
                            &local,
                            &entry.style,
                            i as u32,
                            entry.placement,
                            seg_base,
                            &mut seg_batch,
                        )
                    };
                // Deduplicate styles within the frame exactly as segments are: every
                // entry with a byte-identical compiled style shares ONE slot, so N
                // nodes that look alike upload one GpuStyle, not N. Transparent to the
                // shader, which still reads per-entry `style_idx`.
                let style_hash = hash_gpu_style(&gpu_style);
                gpu_entry.style_idx =
                    *pipeline
                        .frame_style_slots
                        .entry(style_hash)
                        .or_insert_with(|| {
                            let idx = style_base + style_batch.len() as u32;
                            style_batch.push(gpu_style);
                            idx
                        });
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

            // Record what this slot now occupies so a later frame can reuse it.
            let state = SlotState {
                geom_hash,
                seg_start,
                seg_count: pipeline.segments_buffer.len() as u32 - seg_start,
                entry_start,
                entry_count: pipeline.entries_buffer.len() as u32 - entry_start,
                style_start,
                style_count: pipeline.styles_buffer.len() as u32 - style_start,
            };
            if draw_slot >= pipeline.slots.len() {
                pipeline.slots.resize(draw_slot + 1, None);
            }
            pipeline.slots[draw_slot] = Some(state);
        }

        let entry_count = self.entries.len() as u32;
        let camera_pos = types::GpuVec2::new(self.camera_position.0, self.camera_position.1);
        let grid_origin = types::GpuVec2::new(bounds.x * scale, bounds.y * scale);
        let grid_cols = ((bounds.width * scale / TILE_SIZE).ceil() as u32).max(1);
        let grid_rows = ((bounds.height * scale / TILE_SIZE).ceil() as u32).max(1);

        // Allocate tile region
        let tile_base = pipeline.total_tiles;
        pipeline.total_tiles += grid_cols * grid_rows;

        // Grow spatial index buffers if needed. The cull dispatch is DEFERRED to
        // the first draw, so no prior primitive has computed tiles yet - there is
        // nothing to preserve, and the grown (fresh) buffer is filled by the single
        // batched dispatch against the FINAL buffer. Hence a plain resize, no copy.
        if pipeline.total_tiles > pipeline.tile_capacity {
            let new_cap = (pipeline.total_tiles as f32 * 1.5) as u32;
            let (tc, te) = create_tile_buffers(device, new_cap);
            pipeline.tile_counts_buffer = tc;
            pipeline.tile_entries_buffer = te;
            pipeline.tile_capacity = new_cap;
            pipeline.spatial_index_gen += 1;
        }

        // This draw's index into the DrawData buffer. The batched cull reads it
        // from the dispatch z-axis (workgroup_id.z); `draw_slot` carries it to the
        // matching render instance in `draw`.
        let draw_index = pipeline.draw_data_buffer.len() as u32; // index after push
        self.draw_slot.store(draw_index, Ordering::Relaxed);

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
                &pipeline.tile_counts_buffer,
                &pipeline.tile_entries_buffer,
            );
            pipeline.bind_group_gens = gens;
        }

        // Record this cull dispatch; the FIRST draw runs them all in one encoder +
        // one submit (against the now-final buffers). Stash device/queue so the
        // immutable `draw` can build and submit that batch.
        pipeline
            .pending_dispatches
            .get_mut()
            .unwrap()
            .push((grid_cols, grid_rows));
        if pipeline.frame_queue.is_none() {
            pipeline.frame_device = Some(device.clone());
            pipeline.frame_queue = Some(queue.clone());
        }

        // Static-background texture cache: decide whether to render the
        // background direct, populate the cache texture this frame, or blit a
        // cached frame. The compute pass above built this primitive's
        // tile index and `render_group0` references its buffers, so the cache can
        // render the background here using the same pipeline + instance index.
        if self.cache_background {
            use crate::pipeline::bg_cache::BgMode;
            let key = self.background_key(
                (bounds.x * scale, bounds.y * scale),
                grid_cols,
                grid_rows,
                scale,
            );
            let tw = viewport.physical_width();
            let th = viewport.physical_height();
            match pipeline.bg_cache.decide(device, key, tw, th) {
                BgMode::Direct => pipeline.bg_blit_index = None,
                BgMode::Blit => pipeline.bg_blit_index = Some(draw_index),
                BgMode::Populate => {
                    // The cull is deferred, so the texture can only be populated
                    // AFTER the batched compute. Record it; the first draw renders
                    // it once the tiles exist.
                    *pipeline.pending_bg_populate.get_mut().unwrap() = Some(draw_index);
                    pipeline.bg_blit_index = Some(draw_index);
                }
            }
        }

        pipeline.frame_stats.entry_count += entry_count;
        pipeline.frame_stats.prepare_cpu_us += prepare_start.elapsed().as_micros() as u64;
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        // All prepares are done before any draw: the FIRST draw runs every cull
        // dispatch in ONE encoder + ONE submit (vs one submit per primitive) and
        // any deferred background-cache populate, before any primitive is drawn.
        if !pipeline.compute_submitted.swap(true, Ordering::Relaxed) {
            pipeline.run_deferred_compute();
        }
        // The `DrawData` slot assigned to THIS primitive in `prepare` (not a
        // draw-order counter): iced skips drawing off-viewport instances it still
        // prepared, so a counter would desync every later primitive's slot.
        let draw_idx = self.draw_slot.load(Ordering::Relaxed);
        // Static-background cache hit/populate: blit the cached texture instead
        // of running the fullscreen SDF fragment pass (the fill-rate win).
        if pipeline.bg_blit_index == Some(draw_idx) {
            pipeline.bg_cache.blit(render_pass);
            return true;
        }
        pipeline.record_sdf_instance(render_pass, draw_idx);
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
        let shape = Shape::line([0.0, 0.0], [10.0, 0.0]);
        let s = Style::stroke(iced::Color::WHITE, crate::pattern::Pattern::solid(2.0));
        p.push(&shape, &s, [0.0, 0.0]);
        assert_eq!(p.entry_count(), 1);
    }
}
