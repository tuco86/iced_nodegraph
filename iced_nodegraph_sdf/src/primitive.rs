//! SDF rendering primitive for Iced.
//!
//! Each SdfPrimitive compiles its drawables into GPU buffers, dispatches
//! a compute shader to build the tile spatial index, then renders via
//! a fullscreen triangle that reads the index for per-tile evaluation.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use web_time::Instant;

use iced::Rectangle;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, Device, Queue, TextureFormat,
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use std::collections::HashMap;

use crate::compile::{ENTRY_TILING, FLAG_CLOSED, compile_local_at, entry_referencing};
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
    cull_skipped: false,
});

/// Read performance statistics from the last completed frame.
pub fn sdf_stats() -> types::SdfStats {
    LAST_STATS.lock().clone()
}

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
// Two-level index: 64px coarse tiles (4x4 fine tiles) hold the (segment, entry)
// results; 16px fine tiles hold 16-bit indices into them.
const COARSE_FACTOR: u32 = 4;
// Coarse: 512 (segment_idx, entry_idx) pairs per tile (scatter appends
// first-come; the sort kernel clamps, reserving slots for tilings).
const MAX_COARSE_SLOTS: u32 = 512;
const COARSE_STRIDE: u32 = MAX_COARSE_SLOTS * 2;
// Fine: 128 16-bit indices per tile, packed 2 per u32.
const MAX_FINE_SLOTS: u32 = 128;
const FINE_STRIDE: u32 = MAX_FINE_SLOTS / 2;
// Per-draw tiling slots in the scatter lists; sentinel-padded.
const TILING_RESERVE: u32 = 4;
const CULL_SENTINEL: u32 = u32::MAX;

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

/// Folds every [`types::DrawData`] field EXCEPT `time` into a key. The spatial
/// index depends on camera, viewport, grid geometry and entry ranges - but
/// never on the animation clock (reach bands and tile boxes are
/// time-independent; time only animates style evaluation in the fragment
/// shader). A time-only change must therefore compare EQUAL so the resident
/// index is kept and the cull dispatch is skipped.
fn cull_key(d: &types::DrawData) -> u64 {
    let mut h = KeyHasher::new();
    h.f32(d.bounds_origin.0[0]);
    h.f32(d.bounds_origin.0[1]);
    h.f32(d.camera_position.0[0]);
    h.f32(d.camera_position.0[1]);
    h.f32(d.camera_zoom);
    h.f32(d.scale_factor);
    h.u32(d.entry_count);
    h.u32(d.entry_start);
    h.u32(d.grid_cols);
    h.u32(d.grid_rows);
    h.u32(d.tile_base);
    h.u32(d.coarse_cols);
    h.u32(d.coarse_rows);
    h.u32(d.coarse_base);
    for t in d.tilings {
        h.u32(t);
    }
    h.0
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
///
/// Slots are POSITIONAL (prepare order), and reuse is additionally coupled to the
/// buffer cursors because the geometry buffers are packed front-to-back each
/// frame. Keying slots by content hash alone would NOT survive a reorder (e.g.
/// the selection-driven z-resort): the resident bytes would sit at the wrong
/// offsets, and entries reference segments/styles of OTHER primitives by
/// absolute index (cross-primitive instancing + style dedup), so relocating a
/// block requires reference fix-ups. Order-independent reuse therefore needs
/// stable arena residency for segment/entry/style blocks - future work, planned
/// with the scatter-binning index.
#[derive(Clone, Copy)]
struct SlotState {
    geom_hash: u64,
    /// The draw slot the primitive occupied. The scatter lists embed the draw
    /// index, so reuse requires the same slot (an empty primitive earlier in
    /// the order shifts draw indices without moving any buffer cursor).
    draw_slot: u32,
    seg_start: u32,
    seg_count: u32,
    entry_start: u32,
    entry_count: u32,
    style_start: u32,
    style_count: u32,
    /// Scatter-list ranges (u32 elements: pairs are triples, closed are 2-tuples).
    pair_start: u32,
    pair_count: u32,
    closed_start: u32,
    closed_count: u32,
    /// The primitive's tiling entry ids (sentinel-padded); under reuse the
    /// compile loop never runs, so the ids for `DrawData.tilings` come from
    /// here.
    tilings: [u32; TILING_RESERVE as usize],
}

pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    // Data buffers
    draw_data_buffer: buffer::Buffer<types::DrawData>,
    entries_buffer: buffer::Buffer<types::GpuDrawEntry>,
    segments_buffer: buffer::Buffer<types::GpuSegment>,
    styles_buffer: buffer::Buffer<types::GpuStyle>,
    // Two-level spatial index. Coarse (64px) tiles hold the (segment, entry)
    // results; fine (16px) tiles hold 16-bit indices into the parent coarse tile.
    coarse_counts_buffer: iced::wgpu::Buffer,
    coarse_slots_buffer: iced::wgpu::Buffer,
    fine_counts_buffer: iced::wgpu::Buffer,
    fine_slots_buffer: iced::wgpu::Buffer,
    /// Scatter work lists (see plan/scatter-binning.md): flat u32 lists with
    /// the same clear/skip/push_bulk slot-reuse lifecycle as the geometry
    /// buffers. `cull_pairs` holds (draw, entry, segment) triples of open
    /// entries; `cull_closed` holds (draw, entry) pairs of closed entries.
    /// Tiling entry ids ride inside `DrawData` (no extra storage binding).
    cull_pairs_buffer: buffer::Buffer<u32>,
    cull_closed_buffer: buffer::Buffer<u32>,
    /// Live scatter-list element counts for the kernels ([triples, pairs]),
    /// written once per culled frame (`arrayLength` reports capacity, not the
    /// live length).
    cull_meta_buffer: iced::wgpu::Buffer,
    fine_capacity: u32,
    coarse_capacity: u32,
    /// Hard ceilings so neither slot binding exceeds the device's
    /// `max_storage_buffer_binding_size`. A frame that would allocate more (many
    /// large overlapping primitives, e.g. nodes stacked into one spot) falls the
    /// excess draws back to grid 0 = iterate-all instead of growing past the limit
    /// and panicking.
    max_fine_tiles: u32,
    max_coarse_tiles: u32,
    spatial_index_gen: u64,
    // Bind groups
    render_group0: BindGroup,
    compute_group0: BindGroup,
    /// Per-kernel group-1 bind groups: the compute stage must stay within the
    /// WebGPU spec-default 8 storage buffers per stage, so each kernel binds
    /// only its own 4 group-1 buffers (group 0 holds the other 4).
    scatter_open_group1: BindGroup,
    scatter_closed_group1: BindGroup,
    sort_group1: BindGroup,
    // draws, entries, segments, styles, spatial, pairs, closed
    bind_group_gens: (u64, u64, u64, u64, u64, u64, u64),
    // Frame state
    total_fine_tiles: u32,
    total_coarse_tiles: u32,
    // Deferred cull-compute: each prepare only RECORDS its dispatch params here;
    // the FIRST draw runs ALL culls in one encoder + one `queue.submit` (was one
    // submit per primitive - ~70us each, the dominant prepare cost). Because the
    // dispatch is recorded after every prepare, the tile buffer can grow freely
    // during prepares with NO copy (nothing is computed until the end). Mutex/
    // AtomicBool (not RefCell/Cell) because the Pipeline must be Sync per iced's
    // `Primitive` bound. `frame_device`/`frame_queue` are cloned in prepare so the
    // immutable `draw` can build + submit the batched compute.
    pending_dispatches: Mutex<Vec<(u32, u32)>>, // (grid_cols, grid_rows) per culled draw
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
    /// Per draw-slot cull key of the LAST frame (see [`cull_key`]): every
    /// `DrawData` field except `time`. Survives `trim`; compared during
    /// `prepare` to detect whether the resident spatial index is still valid.
    prev_cull_keys: Vec<u64>,
    /// Set when any prepare this frame invalidates the resident spatial index:
    /// a geometry rebuild (buffer bytes changed or moved), a cull-key mismatch
    /// (camera, viewport, grid, entry ranges, new draw slot), or an
    /// index-buffer regrowth. Cleared in `trim`; read by
    /// `run_deferred_compute`, which SKIPS the whole cull dispatch while the
    /// index is valid (idle redraws and time-only animation frames).
    cull_dirty: bool,
    /// Set by the FIRST rebuild of a frame; poisons slot reuse for every
    /// later prepare (see the reuse-filter comment). Cleared in `trim`.
    frame_rebuilt: bool,
}

/// The two-level index buffers, in bind order for compute group 1:
/// (coarse_counts, coarse_slots, fine_counts, fine_slots).
fn create_index_buffers(
    device: &Device,
    fine_cap: u32,
    coarse_cap: u32,
) -> (
    iced::wgpu::Buffer,
    iced::wgpu::Buffer,
    iced::wgpu::Buffer,
    iced::wgpu::Buffer,
) {
    let fine_cap = fine_cap.max(1) as u64;
    let coarse_cap = coarse_cap.max(1) as u64;
    let usage = BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC;
    let buf = |label, size| {
        device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    (
        buf("sdf_coarse_counts", coarse_cap * 4),
        buf("sdf_coarse_slots", coarse_cap * COARSE_STRIDE as u64 * 4),
        buf("sdf_fine_counts", fine_cap * 4),
        buf("sdf_fine_slots", fine_cap * FINE_STRIDE as u64 * 4),
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
    fine_counts: &iced::wgpu::Buffer,
    fine_slots: &iced::wgpu::Buffer,
    coarse_slots: &iced::wgpu::Buffer,
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
                resource: fine_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: fine_slots.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 6,
                resource: coarse_slots.as_entire_binding(),
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

/// Group 1 for the scatter kernels: coarse outputs + the kernel's work list
/// (open triples or closed pairs) + the live-count meta buffer.
fn create_scatter_group1(
    device: &Device,
    shared: &SharedSdfResources,
    coarse_counts: &iced::wgpu::Buffer,
    coarse_slots: &iced::wgpu::Buffer,
    cull_list: &buffer::Buffer<u32>,
    cull_meta: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_scatter_g1"),
        layout: &shared.compute_scatter_group1_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: coarse_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: coarse_slots.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: cull_list.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: cull_meta.as_entire_binding(),
            },
        ],
    })
}

/// Group 1 for the sort/fine kernel: coarse outputs + fine outputs.
fn create_sort_group1(
    device: &Device,
    shared: &SharedSdfResources,
    coarse_counts: &iced::wgpu::Buffer,
    coarse_slots: &iced::wgpu::Buffer,
    fine_counts: &iced::wgpu::Buffer,
    fine_slots: &iced::wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("sdf_sort_g1"),
        layout: &shared.compute_sort_group1_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: coarse_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: coarse_slots.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: fine_counts.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: fine_slots.as_entire_binding(),
            },
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
        let cull_pairs_buffer = buffer::Buffer::new(device, Some("sdf_cull_pairs"), usage);
        let cull_closed_buffer = buffer::Buffer::new(device, Some("sdf_cull_closed"), usage);
        let cull_meta_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_cull_meta"),
            size: 8,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (coarse_counts_buffer, coarse_slots_buffer, fine_counts_buffer, fine_slots_buffer) =
            create_index_buffers(device, 256, 64);

        let render_group0 = create_render_group0(
            device,
            &shared,
            &draw_data_buffer,
            &entries_buffer,
            &segments_buffer,
            &styles_buffer,
            &fine_counts_buffer,
            &fine_slots_buffer,
            &coarse_slots_buffer,
        );
        let compute_group0 = create_compute_group0(
            device,
            &shared,
            &draw_data_buffer,
            &entries_buffer,
            &segments_buffer,
            &styles_buffer,
        );
        let scatter_open_group1 = create_scatter_group1(
            device,
            &shared,
            &coarse_counts_buffer,
            &coarse_slots_buffer,
            &cull_pairs_buffer,
            &cull_meta_buffer,
        );
        let scatter_closed_group1 = create_scatter_group1(
            device,
            &shared,
            &coarse_counts_buffer,
            &coarse_slots_buffer,
            &cull_closed_buffer,
            &cull_meta_buffer,
        );
        let sort_group1 = create_sort_group1(
            device,
            &shared,
            &coarse_counts_buffer,
            &coarse_slots_buffer,
            &fine_counts_buffer,
            &fine_slots_buffer,
        );

        let limit = device.limits().max_storage_buffer_binding_size as u64;
        Self {
            shared,
            draw_data_buffer,
            entries_buffer,
            segments_buffer,
            styles_buffer,
            cull_pairs_buffer,
            cull_closed_buffer,
            cull_meta_buffer,
            coarse_counts_buffer,
            coarse_slots_buffer,
            fine_counts_buffer,
            fine_slots_buffer,
            fine_capacity: 256,
            coarse_capacity: 64,
            // Each slot binding must stay under the device's storage-binding limit.
            max_fine_tiles: (limit / (FINE_STRIDE as u64 * 4)).max(256) as u32,
            max_coarse_tiles: (limit / (COARSE_STRIDE as u64 * 4)).max(64) as u32,
            spatial_index_gen: 0,
            render_group0,
            compute_group0,
            scatter_open_group1,
            scatter_closed_group1,
            sort_group1,
            bind_group_gens: (0, 0, 0, 0, 0, 0, 0),
            total_fine_tiles: 0,
            total_coarse_tiles: 0,
            pending_dispatches: Mutex::new(Vec::new()),
            compute_submitted: std::sync::atomic::AtomicBool::new(false),
            frame_device: None,
            frame_queue: None,
            segment_scratch: Vec::new(),
            frame_stats: types::SdfStats::default(),
            shape_cache: ShapeCache::new(4096),
            frame_shape_slots: HashMap::new(),
            frame_style_slots: HashMap::new(),
            slots: Vec::new(),
            prev_cull_keys: Vec::new(),
            cull_dirty: true,
            frame_rebuilt: false,
        }
    }

    fn trim(&mut self) {
        // Capture frame metrics from the buffers/cache BEFORE clearing them.
        self.frame_stats.tile_count = self.total_fine_tiles;
        self.frame_stats.segment_count = self.segments_buffer.len() as u32;
        self.frame_stats.unique_shapes = self.frame_shape_slots.len() as u32;
        self.frame_stats.unique_styles = self.frame_style_slots.len() as u32;
        self.frame_stats.cache_hits = self.shape_cache.hits();
        self.frame_stats.cache_misses = self.shape_cache.misses();
        self.frame_stats.cache_hit_rate = self.shape_cache.hit_rate();
        self.frame_stats.cull_skipped = !self.cull_dirty;
        *LAST_STATS.lock() = self.frame_stats.clone();
        self.frame_stats = types::SdfStats::default();
        // Stale tail keys of a shrunken draw set must not validate a later,
        // larger frame's slots.
        self.prev_cull_keys.truncate(self.draw_data_buffer.len());
        self.cull_dirty = false;
        self.frame_rebuilt = false;
        self.draw_data_buffer.clear();
        self.entries_buffer.clear();
        self.segments_buffer.clear();
        self.styles_buffer.clear();
        self.cull_pairs_buffer.clear();
        self.cull_closed_buffer.clear();
        self.frame_shape_slots.clear();
        self.frame_style_slots.clear();
        self.total_fine_tiles = 0;
        self.total_coarse_tiles = 0;
        self.pending_dispatches.get_mut().clear();
        self.compute_submitted
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl SdfPipeline {
    /// Records the fullscreen-triangle SDF instance draw for `draw_index` (set
    /// render pipeline + group0 + draw `0..3`).
    ///
    /// Wrapped in a `sdf_shade` debug group so GPU captures (Nsight Graphics,
    /// RenderDoc, PIX) attribute the fragment work to a named block. Debug markers
    /// need no device feature and are no-ops without a capture tool attached.
    fn record_sdf_instance(&self, pass: &mut iced::wgpu::RenderPass<'_>, draw_index: u32) {
        pass.push_debug_group("sdf_shade");
        pass.set_pipeline(&self.shared.render_pipeline);
        pass.set_bind_group(0, &self.render_group0, &[]);
        pass.draw(0..3, draw_index..draw_index + 1);
        pass.pop_debug_group();
    }

    /// Records `key` (see [`cull_key`]) for draw slot `slot`, marking the
    /// resident spatial index dirty on any mismatch - a new slot or a changed
    /// DrawData-sans-time. Called once per prepare, so an unchanged frame
    /// leaves `cull_dirty` false and the cull dispatch is skipped.
    fn note_cull_key(&mut self, slot: usize, key: u64) {
        if self.prev_cull_keys.get(slot).copied() != Some(key) {
            self.cull_dirty = true;
            if slot >= self.prev_cull_keys.len() {
                self.prev_cull_keys.resize(slot + 1, 0);
            }
            self.prev_cull_keys[slot] = key;
        }
    }

    /// Runs every cull dispatch recorded this frame in ONE encoder + ONE submit.
    /// Called once, from the first `draw` (all prepares are complete, so the buffers
    /// are final). Replaces the former one-submit-per-primitive path, whose
    /// `queue.submit` overhead was the dominant `prepare` cost.
    fn run_deferred_compute(&self) {
        let (Some(device), Some(queue)) = (self.frame_device.as_ref(), self.frame_queue.as_ref())
        else {
            return;
        };
        // Resident-index skip: when no prepare this frame invalidated the
        // spatial index (geometry, cameras, viewports and grids all unchanged -
        // an idle redraw or a time-only animation frame), the buffers still
        // hold last frame's exact cull result and the dispatch is skipped.
        if !self.cull_dirty {
            return;
        }
        let dispatches = self.pending_dispatches.lock();
        if dispatches.is_empty() {
            return;
        }
        // Live scatter-list lengths for the kernels; `arrayLength` would report
        // buffer capacity.
        let pair_triples = (self.cull_pairs_buffer.len() / 3) as u32;
        let closed_pairs = (self.cull_closed_buffer.len() / 2) as u32;
        let mut meta = [0u8; 8];
        meta[..4].copy_from_slice(&pair_triples.to_le_bytes());
        meta[4..].copy_from_slice(&closed_pairs.to_le_bytes());
        queue.write_buffer(&self.cull_meta_buffer, 0, &meta);

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sdf_compute_batch"),
        });
        // The scatter kernels append via atomicAdd; the per-tile counts must
        // start at zero. Clearing the whole buffer is cheaper than tracking the
        // used range and the slots themselves need no clear (bounded by count).
        encoder.clear_buffer(&self.coarse_counts_buffer, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
                label: Some("sdf_spatial_index"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &self.compute_group0, &[]);
            // Storage writes are ordered between dispatches of one pass, so the
            // sort kernel sees every scattered slot. Each kernel binds its own
            // group 1 (8-storage-buffers-per-stage limit, see the WGSL).
            //
            // Work-item dispatches are 1D flattened; x is capped at 65535
            // workgroups and y extends it (see `scatter_flat_id`).
            if pair_triples > 0 {
                let wgs = pair_triples.div_ceil(64);
                pass.set_pipeline(&self.shared.scatter_open_pipeline);
                pass.set_bind_group(1, &self.scatter_open_group1, &[]);
                pass.dispatch_workgroups(wgs.min(65535), wgs.div_ceil(65535), 1);
            }
            if closed_pairs > 0 {
                // One workgroup per closed entry.
                pass.set_pipeline(&self.shared.scatter_closed_pipeline);
                pass.set_bind_group(1, &self.scatter_closed_group1, &[]);
                pass.dispatch_workgroups(closed_pairs.min(65535), closed_pairs.div_ceil(65535), 1);
            }
            // Sort + fine re-cull: one workgroup per coarse tile. The z-axis is
            // the draw index (read as workgroup_id.z); x/y are sized to the
            // LARGEST draw grid, smaller draws' surplus workgroups self-abort at
            // the grid bound. z is bounded by maxComputeWorkgroupsPerDimension.
            let max_cols = dispatches.iter().map(|&(c, _)| c).max().unwrap_or(0);
            let max_rows = dispatches.iter().map(|&(_, r)| r).max().unwrap_or(0);
            let draw_count = self.draw_data_buffer.len() as u32;
            if max_cols > 0 && max_rows > 0 && draw_count > 0 {
                pass.set_pipeline(&self.shared.sort_fine_pipeline);
                pass.set_bind_group(1, &self.sort_group1, &[]);
                pass.dispatch_workgroups(
                    max_cols.div_ceil(COARSE_FACTOR),
                    max_rows.div_ceil(COARSE_FACTOR),
                    draw_count,
                );
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
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
            let draw_index = pipeline.draw_data_buffer.len() as u32;
            self.draw_slot.store(draw_index, Ordering::Relaxed);
            // Invalidate the slot record: while this primitive is empty, later
            // primitives pack shifted-down and overwrite its resident ranges,
            // so a later frame with the old content must NOT stale-match
            // (`Buffer::skip` reclaims by LENGTH, not content).
            if let Some(s) = pipeline.slots.get_mut(draw_index as usize) {
                *s = None;
            }
            // `DrawData::default()` carries sentinel tiling ids.
            let dd = types::DrawData::default();
            pipeline.note_cull_key(draw_index as usize, cull_key(&dd));
            let _ = pipeline.draw_data_buffer.push(device, queue, dd);
            return;
        }

        let prepare_start = Instant::now();
        let scale = viewport.scale_factor();
        let entry_start = pipeline.entries_buffer.len() as u32;
        let seg_start = pipeline.segments_buffer.len() as u32;
        let style_start = pipeline.styles_buffer.len() as u32;
        let pair_start = pipeline.cull_pairs_buffer.len() as u32;
        let closed_start = pipeline.cull_closed_buffer.len() as u32;
        let draw_slot = pipeline.draw_data_buffer.len();

        // Skip the whole geometry rebuild when this slot's primitive is byte-for-
        // byte identical to last frame AND the buffer cursors line up with where it
        // wrote then (so no earlier primitive shifted the packed offsets). The
        // resident segments/entries/styles are then reused in place - no eval, no
        // upload - by advancing the cursors over them.
        //
        // `!frame_rebuilt` guards cross-primitive references: instancing and
        // style dedup make a later primitive's resident entries point into an
        // EARLIER primitive's segment/style ranges by absolute index. An
        // earlier rebuild with unchanged counts leaves every cursor aligned
        // while replacing the referenced bytes (a recolor or resize keeps the
        // segment count), so the first rebuild of a frame poisons reuse for
        // every later slot. Order-independent reuse needs arena residency
        // (plan/arena-residency.md).
        let geom_hash = self.geometry_hash();
        let reuse = pipeline
            .slots
            .get(draw_slot)
            .copied()
            .flatten()
            .filter(|s| {
                !pipeline.frame_rebuilt
                    && s.geom_hash == geom_hash
                    && s.draw_slot == draw_slot as u32
                    && s.seg_start == seg_start
                    && s.entry_start == entry_start
                    && s.style_start == style_start
                    && s.pair_start == pair_start
                    && s.closed_start == closed_start
            });

        // The draw's tiling entry ids for `DrawData.tilings`: recorded by the
        // compile loop on rebuild, replayed from the slot record under reuse.
        let dd_tilings;
        if let Some(s) = reuse {
            pipeline.segments_buffer.skip(s.seg_count as usize);
            pipeline.entries_buffer.skip(s.entry_count as usize);
            pipeline.styles_buffer.skip(s.style_count as usize);
            pipeline.cull_pairs_buffer.skip(s.pair_count as usize);
            pipeline.cull_closed_buffer.skip(s.closed_count as usize);
            dd_tilings = s.tilings;
        } else {
            // The resident spatial index references the old buffer contents;
            // a rebuild changes (or moves) them, so the cull must rerun - and
            // every LATER slot's reuse is off the table (see the reuse filter).
            pipeline.cull_dirty = true;
            pipeline.frame_rebuilt = true;
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
            let mut pair_batch: Vec<u32> = Vec::new();
            let mut closed_batch: Vec<u32> = Vec::new();
            let mut tiling_ids = [CULL_SENTINEL; TILING_RESERVE as usize];

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
                // Scatter work-list classification (see plan/scatter-binning.md):
                // tilings ride per-draw, closed entries go to the interior-aware
                // kernel, open entries expand to per-segment triples. Indices are
                // ABSOLUTE, so instanced entries reference the shared range.
                let entry_abs = entry_start + i as u32;
                if gpu_entry.entry_type == ENTRY_TILING {
                    if let Some(slot) = tiling_ids.iter_mut().find(|t| **t == CULL_SENTINEL) {
                        *slot = entry_abs;
                    } else {
                        debug_assert!(
                            false,
                            "more than {TILING_RESERVE} tiling entries in one primitive",
                        );
                    }
                } else if gpu_entry.flags & FLAG_CLOSED != 0 {
                    closed_batch.extend_from_slice(&[draw_slot as u32, entry_abs]);
                } else {
                    let seg_end = gpu_entry.segment_start + gpu_entry.segment_count;
                    for s in gpu_entry.segment_start..seg_end {
                        pair_batch.extend_from_slice(&[draw_slot as u32, entry_abs, s]);
                    }
                }
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
            let _ = pipeline
                .cull_pairs_buffer
                .push_bulk(device, queue, &pair_batch);
            let _ = pipeline
                .cull_closed_buffer
                .push_bulk(device, queue, &closed_batch);
            seg_batch.clear();
            pipeline.segment_scratch = seg_batch; // restore the reused allocation

            // Record what this slot now occupies so a later frame can reuse it.
            let state = SlotState {
                geom_hash,
                draw_slot: draw_slot as u32,
                seg_start,
                seg_count: pipeline.segments_buffer.len() as u32 - seg_start,
                entry_start,
                entry_count: pipeline.entries_buffer.len() as u32 - entry_start,
                style_start,
                style_count: pipeline.styles_buffer.len() as u32 - style_start,
                pair_start,
                pair_count: pipeline.cull_pairs_buffer.len() as u32 - pair_start,
                closed_start,
                closed_count: pipeline.cull_closed_buffer.len() as u32 - closed_start,
                tilings: tiling_ids,
            };
            if draw_slot >= pipeline.slots.len() {
                pipeline.slots.resize(draw_slot + 1, None);
            }
            pipeline.slots[draw_slot] = Some(state);
            dd_tilings = tiling_ids;
        }

        let entry_count = self.entries.len() as u32;
        let camera_pos = types::GpuVec2::new(self.camera_position.0, self.camera_position.1);
        let grid_origin = types::GpuVec2::new(bounds.x * scale, bounds.y * scale);
        let mut grid_cols = ((bounds.width * scale / TILE_SIZE).ceil() as u32).max(1);
        let mut grid_rows = ((bounds.height * scale / TILE_SIZE).ceil() as u32).max(1);

        // Coarse grid: one 64px tile per 4x4 block of fine tiles.
        let mut coarse_cols = grid_cols.div_ceil(COARSE_FACTOR);
        let mut coarse_rows = grid_rows.div_ceil(COARSE_FACTOR);

        // Allocate this primitive's tile regions. If EITHER level would push its
        // total past the device's storage-binding limit (e.g. many large
        // overlapping primitives, like a pile of nodes stacked into one spot),
        // this draw falls back to grid 0 = "no spatial index, iterate all entries"
        // instead. Slower for that draw, but it renders correctly and never panics.
        let want_fine = grid_cols as u64 * grid_rows as u64;
        let want_coarse = coarse_cols as u64 * coarse_rows as u64;
        let tile_base;
        let coarse_base;
        if pipeline.total_fine_tiles as u64 + want_fine > pipeline.max_fine_tiles as u64
            || pipeline.total_coarse_tiles as u64 + want_coarse > pipeline.max_coarse_tiles as u64
        {
            grid_cols = 0;
            grid_rows = 0;
            coarse_cols = 0;
            coarse_rows = 0;
            tile_base = 0;
            coarse_base = 0;
        } else {
            tile_base = pipeline.total_fine_tiles;
            coarse_base = pipeline.total_coarse_tiles;
            pipeline.total_fine_tiles += grid_cols * grid_rows;
            pipeline.total_coarse_tiles += coarse_cols * coarse_rows;
        }

        // Grow index buffers if needed. The cull dispatch is DEFERRED to the first
        // draw, so no prior primitive has computed tiles yet - there is nothing to
        // preserve, and the grown (fresh) buffers are filled by the single batched
        // dispatch against the FINAL buffers. Hence a plain resize, no copy. Capped
        // per level so neither binding exceeds the device limit.
        if pipeline.total_fine_tiles > pipeline.fine_capacity
            || pipeline.total_coarse_tiles > pipeline.coarse_capacity
        {
            let new_fine = if pipeline.total_fine_tiles > pipeline.fine_capacity {
                ((pipeline.total_fine_tiles as f32 * 1.5) as u32).min(pipeline.max_fine_tiles)
            } else {
                pipeline.fine_capacity
            };
            let new_coarse = if pipeline.total_coarse_tiles > pipeline.coarse_capacity {
                ((pipeline.total_coarse_tiles as f32 * 1.5) as u32).min(pipeline.max_coarse_tiles)
            } else {
                pipeline.coarse_capacity
            };
            let (cc, cs, fc, fs) = create_index_buffers(device, new_fine, new_coarse);
            pipeline.coarse_counts_buffer = cc;
            pipeline.coarse_slots_buffer = cs;
            pipeline.fine_counts_buffer = fc;
            pipeline.fine_slots_buffer = fs;
            pipeline.fine_capacity = new_fine;
            pipeline.coarse_capacity = new_coarse;
            pipeline.spatial_index_gen += 1;
            // The recreated index buffers no longer hold last frame's result.
            pipeline.cull_dirty = true;
        }

        // This draw's index into the DrawData buffer. The batched cull reads it
        // from the dispatch z-axis (workgroup_id.z); `draw_slot` carries it to the
        // matching render instance in `draw`.
        let draw_index = pipeline.draw_data_buffer.len() as u32; // index after push
        self.draw_slot.store(draw_index, Ordering::Relaxed);

        let dd = types::DrawData {
            bounds_origin: grid_origin,
            camera_position: camera_pos,
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            entry_count,
            entry_start,
            grid_cols,
            grid_rows,
            tile_base,
            coarse_cols,
            coarse_rows,
            coarse_base,
            tilings: dd_tilings,
        };
        pipeline.note_cull_key(draw_index as usize, cull_key(&dd));
        let _ = pipeline.draw_data_buffer.push(device, queue, dd);

        // Recreate bind groups if any buffer generation changed
        let gens = (
            pipeline.draw_data_buffer.generation(),
            pipeline.entries_buffer.generation(),
            pipeline.segments_buffer.generation(),
            pipeline.styles_buffer.generation(),
            pipeline.spatial_index_gen,
            pipeline.cull_pairs_buffer.generation(),
            pipeline.cull_closed_buffer.generation(),
        );
        if gens != pipeline.bind_group_gens {
            pipeline.render_group0 = create_render_group0(
                device,
                &pipeline.shared,
                &pipeline.draw_data_buffer,
                &pipeline.entries_buffer,
                &pipeline.segments_buffer,
                &pipeline.styles_buffer,
                &pipeline.fine_counts_buffer,
                &pipeline.fine_slots_buffer,
                &pipeline.coarse_slots_buffer,
            );
            pipeline.compute_group0 = create_compute_group0(
                device,
                &pipeline.shared,
                &pipeline.draw_data_buffer,
                &pipeline.entries_buffer,
                &pipeline.segments_buffer,
                &pipeline.styles_buffer,
            );
            pipeline.scatter_open_group1 = create_scatter_group1(
                device,
                &pipeline.shared,
                &pipeline.coarse_counts_buffer,
                &pipeline.coarse_slots_buffer,
                &pipeline.cull_pairs_buffer,
                &pipeline.cull_meta_buffer,
            );
            pipeline.scatter_closed_group1 = create_scatter_group1(
                device,
                &pipeline.shared,
                &pipeline.coarse_counts_buffer,
                &pipeline.coarse_slots_buffer,
                &pipeline.cull_closed_buffer,
                &pipeline.cull_meta_buffer,
            );
            pipeline.sort_group1 = create_sort_group1(
                device,
                &pipeline.shared,
                &pipeline.coarse_counts_buffer,
                &pipeline.coarse_slots_buffer,
                &pipeline.fine_counts_buffer,
                &pipeline.fine_slots_buffer,
            );
            pipeline.bind_group_gens = gens;
        }

        // Record this cull dispatch; the FIRST draw runs them all in one encoder +
        // one submit (against the now-final buffers). Stash device/queue so the
        // immutable `draw` can build and submit that batch.
        pipeline
            .pending_dispatches
            .get_mut()
            .push((grid_cols, grid_rows));
        if pipeline.frame_queue.is_none() {
            pipeline.frame_device = Some(device.clone());
            pipeline.frame_queue = Some(queue.clone());
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
