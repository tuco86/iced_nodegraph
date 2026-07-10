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

use crate::compile::{
    ENTRY_TILING, EntryMeta, FLAG_CLOSED, compile_local_at, entry_from_meta, entry_meta,
};
use crate::pattern::PatternType;
use crate::pipeline::arena::ArenaAlloc;
use crate::pipeline::overflow::OverflowProbe;
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
    resident_hits: 0,
    geometry_rebuilds: 0,
    arena_compactions: 0,
    coarse_demand_max: 0,
    coarse_overflow_tiles: 0,
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
/// Frames a resident geometry block may go unused before eviction returns its
/// arena ranges to the free lists. Small on purpose: blocks in steady use are
/// touched every frame (age 0), so this only bounds how long CHURNED content
/// (a dragged primitive mints a new block per frame) lingers. A block that
/// comes back within the window (a quick select/deselect toggle) reuses its
/// residency; beyond it, only that one primitive re-compiles.
const RESIDENT_MAX_AGE: u64 = 8;
/// Compaction heuristic: reset the residency state when any arena's high-water
/// mark exceeds `COMPACT_SLACK_FACTOR x` its live count AND the floor below.
/// The floor keeps small scenes from compacting over noise; the factor bounds
/// steady-state waste to a constant multiple of the live data.
const COMPACT_MIN_HIGH_WATER: u32 = 4096;
const COMPACT_SLACK_FACTOR: u32 = 4;

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

// --- Arena residency (plan/arena-residency.md) ---
//
// The segment/entry/style buffers are persistent arenas: a primitive's
// compiled output is allocated once, NEVER moves while resident, and is keyed
// by CONTENT (`geometry_hash`), not by draw slot. Reuse therefore survives any
// reorder of the prepare order (selection z-resort, node add/remove) - the
// coupling the old packed-per-frame slots could not break: entries reference
// segments and styles of OTHER primitives by absolute index, and packing
// front-to-back made every offset positional.

/// A primitive's resident geometry: the arena ranges its compiled output
/// occupies plus everything a later frame needs to draw it without
/// re-evaluating anything.
struct ResidentBlock {
    /// Globally unique block id (monotonic, never reused). Scatter-slot
    /// records key on this to prove "same bytes" across frames without
    /// comparing ranges (a freed range can be re-allocated to new content).
    block_gen: u64,
    /// The block's contiguous range in the entry arena. Draws need entry
    /// contiguity only WITHIN one primitive; `DrawData.entry_start` points
    /// here, wherever the block sits.
    entry_start: u32,
    entry_count: u32,
    /// Shape hashes this block references in `resident_shapes` (refcounted
    /// ONCE per block); decremented when the block is evicted.
    shape_refs: Vec<u64>,
    /// Compiled-style hashes this block references in `resident_styles`.
    style_refs: Vec<u64>,
    /// Draw-slot-free scatter data: flattened (entry, segment) absolute index
    /// pairs of OPEN entries. Each frame that cannot skip prefixes the current
    /// draw slot and pushes to the packed cull lists - flat u32 writes, never
    /// a re-evaluation.
    pairs: Vec<u32>,
    /// Absolute entry indices of CLOSED entries (same per-frame prefixing).
    closed: Vec<u32>,
    /// The primitive's tiling entry ids (sentinel-padded) for
    /// `DrawData.tilings`; the compile loop never runs on a hit, so they are
    /// replayed from here.
    tilings: [u32; TILING_RESERVE as usize],
    /// `frame_counter` value of the last frame that used this block. Blocks
    /// unused for [`RESIDENT_MAX_AGE`] frames are evicted in `trim`.
    last_used: u64,
}

/// A unique shape's segments, resident in the segment arena and shared by
/// every block whose entries reference the range - the GPU-instancing dedup,
/// now cross-frame and refcounted. `meta` carries what entry construction
/// needs beyond the range, so an instance of a resident shape never
/// re-evaluates it: for a non-cacheable edge stroke that skips the whole biarc
/// fit when only placement or style changed.
struct ShapeResidency {
    seg_start: u32,
    meta: EntryMeta,
    /// Resident blocks referencing this range; freed back to the arena at 0.
    refs: u32,
    /// Block gen that last took a ref: dedups refcounting within one block (a
    /// block with N instances of a shape holds ONE ref).
    last_ref_gen: u64,
}

/// A deduplicated compiled style resident in the style arena (a single slot),
/// mirroring [`ShapeResidency`] for styles.
struct StyleResidency {
    idx: u32,
    refs: u32,
    last_ref_gen: u64,
}

/// Per draw-slot record of the scatter-list ranges written last frame. The
/// packed cull lists stay per-frame POSITIONAL (they embed the draw slot), so
/// their reuse is cursor-coupled like the old geometry path - but their
/// content is SELF-CONTAINED (the own block's indices, the own slot), so
/// block identity plus cursor equality suffices and no cross-slot poison
/// exists; a forced rebuild is a cheap prefix-write from the resident block.
#[derive(Clone, Copy)]
struct ScatterSlot {
    /// [`ResidentBlock::gen`] whose data the ranges hold.
    block_gen: u64,
    pair_start: u32,
    pair_count: u32,
    closed_start: u32,
    closed_count: u32,
}

pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    // Data buffers. `draw_data` is per-frame (packed in prepare order); the
    // entry/segment/style buffers are persistent ARENAS whose ranges are
    // placed by the allocators below and survive `trim`.
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
    /// Resident geometry blocks keyed by [`SdfPrimitive::geometry_hash`]:
    /// byte-identical primitives reuse their block wherever they sit in the
    /// prepare order. Survives `trim`; unused blocks age out (see
    /// [`RESIDENT_MAX_AGE`]) and a compaction resets the whole map.
    resident: HashMap<u64, ResidentBlock>,
    /// Frame-surviving map recipe-hash -> resident segment range + entry
    /// metadata (refcounted; see [`ShapeResidency`]). The first instance of a
    /// shape EVER uploads its segments; every later identical instance - in
    /// any primitive, any frame - references that range (GPU instancing).
    resident_shapes: HashMap<u64, ShapeResidency>,
    /// Frame-surviving map compiled-style-hash -> resident `style_idx`.
    /// Mirrors `resident_shapes` for styles: entries that look identical share
    /// ONE `GpuStyle` slot instead of duplicating ~336 bytes each.
    resident_styles: HashMap<u64, StyleResidency>,
    /// Range allocators for the three geometry arenas (element indices into
    /// the matching buffers).
    seg_arena: ArenaAlloc,
    entry_arena: ArenaAlloc,
    style_arena: ArenaAlloc,
    /// Per draw-slot scatter-list record of the LAST frame (see
    /// [`ScatterSlot`]). Survives `trim`.
    scatter_slots: Vec<Option<ScatterSlot>>,
    /// Frame counter (incremented in `trim`); drives block LRU aging.
    frame_counter: u64,
    /// Monotonic [`ResidentBlock::gen`] source.
    next_block_gen: u64,
    /// Lifetime arena-compaction count (reported in [`types::SdfStats`]).
    compactions: u64,
    /// Reused scratch for the per-frame scatter prefix-writes.
    pair_scratch: Vec<u32>,
    closed_scratch: Vec<u32>,
    /// Per draw-slot cull key of the LAST frame (see [`cull_key`]): every
    /// `DrawData` field except `time`. Survives `trim`; compared during
    /// `prepare` to detect whether the resident spatial index is still valid.
    prev_cull_keys: Vec<u64>,
    /// Set when any prepare this frame invalidates the resident spatial index:
    /// a geometry rebuild or scatter-list rewrite (list bytes changed or
    /// moved), a cull-key mismatch (camera, viewport, grid, entry ranges, new
    /// draw slot), or an index-buffer regrowth. Cleared in `trim`; read by
    /// `run_deferred_compute`, which SKIPS the whole cull dispatch while the
    /// index is valid (idle redraws and time-only animation frames).
    cull_dirty: bool,
    /// Async readback of the coarse demand counters (overflow telemetry, see
    /// [`crate::pipeline::overflow`]). Mutex for the same reason as
    /// `pending_dispatches`: the copy is recorded from the immutable `draw`.
    overflow_probe: Mutex<OverflowProbe>,
    /// Most recent completed demand readback: (max per-tile demand, tiles over
    /// the usable cap). Sticky between culls; reported in [`types::SdfStats`].
    coarse_demand: (u32, u32),
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
            resident: HashMap::new(),
            resident_shapes: HashMap::new(),
            resident_styles: HashMap::new(),
            seg_arena: ArenaAlloc::new(),
            entry_arena: ArenaAlloc::new(),
            style_arena: ArenaAlloc::new(),
            scatter_slots: Vec::new(),
            frame_counter: 0,
            next_block_gen: 0,
            compactions: 0,
            pair_scratch: Vec::new(),
            closed_scratch: Vec::new(),
            prev_cull_keys: Vec::new(),
            cull_dirty: true,
            overflow_probe: Mutex::new(OverflowProbe::new()),
            coarse_demand: (0, 0),
        }
    }

    fn trim(&mut self) {
        // Harvest a completed coarse-demand readback, if any (non-blocking;
        // polls only while one is outstanding). Values are sticky until the
        // next completed readback so idle frames keep reporting the last cull.
        if let Some(device) = self.frame_device.as_ref()
            && let Some(report) = self
                .overflow_probe
                .get_mut()
                .harvest(device, MAX_COARSE_SLOTS - TILING_RESERVE)
        {
            self.coarse_demand = (report.demand_max, report.overflow_tiles);
        }
        self.frame_stats.coarse_demand_max = self.coarse_demand.0;
        self.frame_stats.coarse_overflow_tiles = self.coarse_demand.1;
        // Capture frame metrics from the arenas/cache BEFORE clearing.
        self.frame_stats.tile_count = self.total_fine_tiles;
        self.frame_stats.segment_count = self.seg_arena.live();
        self.frame_stats.unique_shapes = self.resident_shapes.len() as u32;
        self.frame_stats.unique_styles = self.resident_styles.len() as u32;
        self.frame_stats.cache_hits = self.shape_cache.hits();
        self.frame_stats.cache_misses = self.shape_cache.misses();
        self.frame_stats.cache_hit_rate = self.shape_cache.hit_rate();
        self.frame_stats.cull_skipped = !self.cull_dirty;
        self.frame_stats.arena_compactions = self.compactions;
        *LAST_STATS.lock() = self.frame_stats.clone();
        self.frame_stats = types::SdfStats::default();
        // Stale tail keys of a shrunken draw set must not validate a later,
        // larger frame's slots.
        self.prev_cull_keys.truncate(self.draw_data_buffer.len());
        self.cull_dirty = false;
        // Per-frame buffers only: entries/segments/styles are arena-resident
        // and survive across frames.
        self.draw_data_buffer.clear();
        self.cull_pairs_buffer.clear();
        self.cull_closed_buffer.clear();
        self.total_fine_tiles = 0;
        self.total_coarse_tiles = 0;
        self.pending_dispatches.get_mut().clear();
        self.compute_submitted
            .store(false, std::sync::atomic::Ordering::Relaxed);
        // Residency lifecycle: age out unused blocks, then compact when the
        // arenas' high-water mark runs too far ahead of the live data.
        self.frame_counter += 1;
        self.evict_expired();
        self.maybe_compact();
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
    /// Evicts resident blocks unused for more than [`RESIDENT_MAX_AGE`] frames:
    /// their entry range returns to the arena and their shape/style refcounts
    /// drop (freeing shared ranges that hit zero). Blocks in steady use are
    /// touched every frame and never age; this only reclaims churn (a dragged
    /// primitive mints a new block per frame) and content that left the scene.
    fn evict_expired(&mut self) {
        let mut expired: Vec<u64> = Vec::new();
        for (k, b) in &self.resident {
            if self.frame_counter - b.last_used > RESIDENT_MAX_AGE {
                expired.push(*k);
            }
        }
        for k in expired {
            if let Some(b) = self.resident.remove(&k) {
                self.release_block(b);
            }
        }
    }

    /// Returns an evicted block's arena ranges: the entry range directly, the
    /// shared shape/style ranges via refcount (freed at zero).
    fn release_block(&mut self, block: ResidentBlock) {
        self.entry_arena.free(block.entry_start, block.entry_count);
        for h in &block.shape_refs {
            if let Some(r) = self.resident_shapes.get_mut(h) {
                r.refs -= 1;
                if r.refs == 0 {
                    self.seg_arena.free(r.seg_start, r.meta.segment_count);
                    self.resident_shapes.remove(h);
                }
            }
        }
        for h in &block.style_refs {
            if let Some(r) = self.resident_styles.get_mut(h) {
                r.refs -= 1;
                if r.refs == 0 {
                    self.style_arena.free(r.idx, 1);
                    self.resident_styles.remove(h);
                }
            }
        }
    }

    /// Compaction: when any arena's high-water mark runs
    /// [`COMPACT_SLACK_FACTOR`]x ahead of its live count (fragmentation the
    /// free lists cannot reclaim), drop the WHOLE residency state and let the
    /// next frame rebuild tightly packed. One frame of full re-evaluation and
    /// re-upload - the old per-reorder behavior as the rare worst case.
    /// Running between frames (from `trim`) means no draw of the current frame
    /// can hold references into the dropped arenas.
    fn maybe_compact(&mut self) {
        fn slack(a: &ArenaAlloc) -> bool {
            a.high_water() > COMPACT_MIN_HIGH_WATER
                && a.high_water() > a.live().saturating_mul(COMPACT_SLACK_FACTOR)
        }
        if !(slack(&self.seg_arena) || slack(&self.entry_arena) || slack(&self.style_arena)) {
            return;
        }
        self.resident.clear();
        self.resident_shapes.clear();
        self.resident_styles.clear();
        self.seg_arena.clear();
        self.entry_arena.clear();
        self.style_arena.clear();
        self.segments_buffer.clear();
        self.entries_buffer.clear();
        self.styles_buffer.clear();
        self.scatter_slots.clear();
        // The resident spatial index references the dropped ranges; rebuild.
        self.cull_dirty = true;
        self.compactions += 1;
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
        let mut probe = self.overflow_probe.lock();
        {
            let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
                label: Some("sdf_scatter"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &self.compute_group0, &[]);
            // Each kernel binds its own group 1 (8-storage-buffers-per-stage
            // limit, see the WGSL).
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
        }
        // Overflow telemetry: snapshot the demand counters BETWEEN scatter and
        // sort - the only window where they hold TRUE demand (the sort kernel
        // overwrites each count with the clamped render list length). The copy
        // lands in a staging buffer mapped asynchronously after submit; `trim`
        // harvests it a frame later without blocking. Skipped while a readback
        // is still outstanding (sampling, never queueing). Splitting the pass
        // costs one begin/end; ordering between passes and copies of one
        // encoder is guaranteed, so the sort still sees every scattered slot.
        probe.record_copy(
            device,
            &mut encoder,
            &self.coarse_counts_buffer,
            self.total_coarse_tiles as u64 * 4,
        );
        {
            let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
                label: Some("sdf_sort_fine"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &self.compute_group0, &[]);
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
        probe.map_pending();
    }
}
/// Compiles a primitive's entries into freshly allocated arena ranges and
/// returns the [`ResidentBlock`] describing them - the residency MISS path.
///
/// A free function over explicit pipeline fields (not `&mut SdfPipeline`) so
/// the caller can hold the `resident` map entry open while building the block
/// (disjoint field borrows).
///
/// Per entry, segments resolve in three tiers:
/// 1. shape already RESIDENT (any frame, any primitive): reference the range,
///    take one refcount per block - no evaluation at all, which for a
///    non-cacheable edge stroke skips the whole biarc fit;
/// 2. shape new but seen EARLIER IN THIS BLOCK: reference its pending local
///    range (classic per-frame instancing);
/// 3. genuinely new: evaluate (through the boolean cache when cacheable) and
///    compile into the pending batch.
///
/// Styles mirror the same three tiers with single-slot ranges.
///
/// Pending entries carry batch-LOCAL offsets; after one allocation per arena
/// they are fixed up to absolute indices and uploaded in ONE write per buffer
/// (per-entry writes were the dominant prepare cost before bulk uploads).
#[allow(clippy::too_many_arguments)]
fn compile_block(
    device: &Device,
    queue: &Queue,
    entries: &[DrawEntry],
    shape_cache: &mut ShapeCache,
    resident_shapes: &mut HashMap<u64, ShapeResidency>,
    resident_styles: &mut HashMap<u64, StyleResidency>,
    seg_arena: &mut ArenaAlloc,
    entry_arena: &mut ArenaAlloc,
    style_arena: &mut ArenaAlloc,
    segments_buffer: &mut buffer::Buffer<types::GpuSegment>,
    entries_buffer: &mut buffer::Buffer<types::GpuDrawEntry>,
    styles_buffer: &mut buffer::Buffer<types::GpuStyle>,
    seg_scratch: &mut Vec<types::GpuSegment>,
    block_gen: u64,
    frame: u64,
) -> ResidentBlock {
    seg_scratch.clear();
    let mut new_styles: Vec<types::GpuStyle> = Vec::new();
    let mut entry_batch: Vec<types::GpuDrawEntry> = Vec::with_capacity(entries.len());
    let mut shape_refs: Vec<u64> = Vec::new();
    let mut style_refs: Vec<u64> = Vec::new();
    // Entries whose segment_start / style_idx are batch-local and need the
    // arena base added after allocation.
    let mut seg_fixups: Vec<u32> = Vec::new();
    let mut style_fixups: Vec<u32> = Vec::new();
    // First instances of NEW shapes/styles in this block: hash -> pending
    // local offset (+ metadata for shapes).
    let mut local_new_shapes: HashMap<u64, (u32, EntryMeta)> = HashMap::new();
    let mut local_new_styles: HashMap<u64, u32> = HashMap::new();

    for (i, entry) in entries.iter().enumerate() {
        let hash = entry.shape.hash();
        let (mut gpu_entry, gpu_style) = if let Some(r) = resident_shapes.get_mut(&hash) {
            if r.last_ref_gen != block_gen {
                r.last_ref_gen = block_gen;
                r.refs += 1;
                shape_refs.push(hash);
            }
            entry_from_meta(
                &r.meta,
                &entry.style,
                i as u32,
                entry.placement,
                r.seg_start,
            )
        } else if let Some(&(local_start, ref meta)) = local_new_shapes.get(&hash) {
            seg_fixups.push(i as u32);
            entry_from_meta(meta, &entry.style, i as u32, entry.placement, local_start)
        } else {
            // Evaluate the shape to LOCAL geometry: cacheable booleans come
            // from the frame-surviving cache (one boolean for all identical
            // shapes); cheap primitives and ephemeral strokes (edges) evaluate
            // fresh. The clone breaks the cache borrow before the batch is
            // touched; it copies arcs, not the boolean.
            let local = if entry.shape.is_cacheable() {
                shape_cache.get_or_eval(&entry.shape).clone()
            } else {
                entry.shape.evaluate()
            };
            let local_start = seg_scratch.len() as u32;
            // Base 0: the entry's segment_start comes out batch-local and is
            // fixed up to `seg_base + local_start` after allocation.
            let out = compile_local_at(
                &local,
                &entry.style,
                i as u32,
                entry.placement,
                0,
                seg_scratch,
            );
            local_new_shapes.insert(hash, (local_start, entry_meta(&local)));
            seg_fixups.push(i as u32);
            out
        };
        // Deduplicate styles exactly as segments: every entry with a
        // byte-identical compiled style shares ONE resident slot, so N nodes
        // that look alike hold one GpuStyle, not N. Transparent to the shader,
        // which still reads per-entry `style_idx`.
        let style_hash = hash_gpu_style(&gpu_style);
        gpu_entry.style_idx = if let Some(r) = resident_styles.get_mut(&style_hash) {
            if r.last_ref_gen != block_gen {
                r.last_ref_gen = block_gen;
                r.refs += 1;
                style_refs.push(style_hash);
            }
            r.idx
        } else if let Some(&local_idx) = local_new_styles.get(&style_hash) {
            style_fixups.push(i as u32);
            local_idx
        } else {
            let local_idx = new_styles.len() as u32;
            new_styles.push(gpu_style);
            local_new_styles.insert(style_hash, local_idx);
            style_fixups.push(i as u32);
            local_idx
        };
        entry_batch.push(gpu_entry);
    }

    // One allocation and one bulk write per arena; fix the pending local
    // offsets up to absolute indices first.
    let seg_base = seg_arena.alloc(seg_scratch.len() as u32);
    let style_base = style_arena.alloc(new_styles.len() as u32);
    let entry_start = entry_arena.alloc(entry_batch.len() as u32);
    for &i in &seg_fixups {
        entry_batch[i as usize].segment_start += seg_base;
    }
    for &i in &style_fixups {
        entry_batch[i as usize].style_idx += style_base;
    }
    segments_buffer.write_at(device, queue, seg_base as usize, seg_scratch);
    styles_buffer.write_at(device, queue, style_base as usize, &new_styles);
    entries_buffer.write_at(device, queue, entry_start as usize, &entry_batch);
    seg_scratch.clear();

    // The pending shapes/styles are resident now; register them for sharing.
    for (hash, (local_start, meta)) in local_new_shapes {
        resident_shapes.insert(
            hash,
            ShapeResidency {
                seg_start: seg_base + local_start,
                meta,
                refs: 1,
                last_ref_gen: block_gen,
            },
        );
        shape_refs.push(hash);
    }
    for (hash, local_idx) in local_new_styles {
        resident_styles.insert(
            hash,
            StyleResidency {
                idx: style_base + local_idx,
                refs: 1,
                last_ref_gen: block_gen,
            },
        );
        style_refs.push(hash);
    }

    // Scatter classification (see plan/scatter-binning.md), draw-slot-free:
    // tilings ride per-draw, closed entries go to the interior-aware kernel,
    // open entries expand to per-segment index pairs. Indices are ABSOLUTE,
    // so instanced entries reference the shared resident range.
    let mut tilings = [CULL_SENTINEL; TILING_RESERVE as usize];
    let mut pairs: Vec<u32> = Vec::new();
    let mut closed: Vec<u32> = Vec::new();
    for (i, e) in entry_batch.iter().enumerate() {
        let entry_abs = entry_start + i as u32;
        if e.entry_type == ENTRY_TILING {
            if let Some(slot) = tilings.iter_mut().find(|t| **t == CULL_SENTINEL) {
                *slot = entry_abs;
            } else {
                debug_assert!(
                    false,
                    "more than {TILING_RESERVE} tiling entries in one primitive",
                );
            }
        } else if e.flags & FLAG_CLOSED != 0 {
            closed.push(entry_abs);
        } else {
            for s in e.segment_start..e.segment_start + e.segment_count {
                pairs.extend_from_slice(&[entry_abs, s]);
            }
        }
    }

    ResidentBlock {
        block_gen,
        entry_start,
        entry_count: entry_batch.len() as u32,
        shape_refs,
        style_refs,
        pairs,
        closed,
        tilings,
        last_used: frame,
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
            // Invalidate the slot's scatter record: while this primitive is
            // empty, later slots' scatter ranges pack shifted-down over its
            // resident range, so a later frame with the old content must NOT
            // stale-match (`Buffer::skip` reclaims by LENGTH, not content).
            if let Some(s) = pipeline.scatter_slots.get_mut(draw_index as usize) {
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
        let pair_start = pipeline.cull_pairs_buffer.len() as u32;
        let closed_start = pipeline.cull_closed_buffer.len() as u32;
        let draw_slot = pipeline.draw_data_buffer.len();

        // Geometry residency (plan/arena-residency.md): a primitive whose
        // compiled bytes are identical to a resident block's (hash over every
        // entry's shape, placement and style) reuses that block WHEREVER it
        // sits in the prepare order - no eval, no upload. A miss compiles into
        // freshly allocated arena ranges; nothing else moves, so a reorder or
        // an earlier rebuild invalidates nothing.
        let geom_hash = self.geometry_hash();
        let frame = pipeline.frame_counter;
        let (block_gen, entry_start, dd_tilings) = match pipeline.resident.entry(geom_hash) {
            std::collections::hash_map::Entry::Occupied(e) => {
                let b = e.into_mut();
                b.last_used = frame;
                pipeline.frame_stats.resident_hits += 1;
                (b.block_gen, b.entry_start, b.tilings)
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                // The resident spatial index references the current draw set's
                // entry/segment indices; new content means the cull must rerun.
                pipeline.cull_dirty = true;
                pipeline.frame_stats.geometry_rebuilds += 1;
                pipeline.next_block_gen += 1;
                let b = v.insert(compile_block(
                    device,
                    queue,
                    &self.entries,
                    &mut pipeline.shape_cache,
                    &mut pipeline.resident_shapes,
                    &mut pipeline.resident_styles,
                    &mut pipeline.seg_arena,
                    &mut pipeline.entry_arena,
                    &mut pipeline.style_arena,
                    &mut pipeline.segments_buffer,
                    &mut pipeline.entries_buffer,
                    &mut pipeline.styles_buffer,
                    &mut pipeline.segment_scratch,
                    pipeline.next_block_gen,
                    frame,
                ));
                (b.block_gen, b.entry_start, b.tilings)
            }
        };

        // Scatter work lists (plan/scatter-binning.md): still per-frame packed
        // - they embed the draw slot - with the same skip-or-push lifecycle.
        // Same block at the same cursors means the resident list bytes are
        // valid; anything else re-pushes the block's draw-slot-free index
        // pairs with the current slot prefixed. Flat u32 writes; geometry is
        // never re-evaluated here.
        let record = pipeline
            .scatter_slots
            .get(draw_slot)
            .copied()
            .flatten()
            .filter(|r| {
                r.block_gen == block_gen
                    && r.pair_start == pair_start
                    && r.closed_start == closed_start
            });
        if let Some(r) = record {
            pipeline.cull_pairs_buffer.skip(r.pair_count as usize);
            pipeline.cull_closed_buffer.skip(r.closed_count as usize);
        } else {
            // The list bytes on the GPU change; the resident index is stale.
            pipeline.cull_dirty = true;
            let block = &pipeline.resident[&geom_hash];
            let mut pair_batch = std::mem::take(&mut pipeline.pair_scratch);
            pair_batch.clear();
            pair_batch.reserve(block.pairs.len() / 2 * 3);
            for pair in block.pairs.chunks_exact(2) {
                pair_batch.extend_from_slice(&[draw_slot as u32, pair[0], pair[1]]);
            }
            let mut closed_batch = std::mem::take(&mut pipeline.closed_scratch);
            closed_batch.clear();
            closed_batch.reserve(block.closed.len() * 2);
            for &e in &block.closed {
                closed_batch.extend_from_slice(&[draw_slot as u32, e]);
            }
            let _ = pipeline
                .cull_pairs_buffer
                .push_bulk(device, queue, &pair_batch);
            let _ = pipeline
                .cull_closed_buffer
                .push_bulk(device, queue, &closed_batch);
            if draw_slot >= pipeline.scatter_slots.len() {
                pipeline.scatter_slots.resize(draw_slot + 1, None);
            }
            pipeline.scatter_slots[draw_slot] = Some(ScatterSlot {
                block_gen,
                pair_start,
                pair_count: pair_batch.len() as u32,
                closed_start,
                closed_count: closed_batch.len() as u32,
            });
            pair_batch.clear();
            closed_batch.clear();
            pipeline.pair_scratch = pair_batch;
            pipeline.closed_scratch = closed_batch;
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
