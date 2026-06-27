//! GPU data structures for segment-based SDF rendering.
//!
//! Layout must match the corresponding WGSL structs in shader.wgsl.

#![allow(dead_code)]

use encase::ShaderType;

/// WGSL `vec2<f32>`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GpuVec2(pub [f32; 2]);

/// WGSL `vec4<f32>`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GpuVec4(pub [f32; 4]);

impl GpuVec2 {
    pub const ZERO: Self = Self([0.0; 2]);
    pub fn new(x: f32, y: f32) -> Self {
        Self([x, y])
    }
}

impl GpuVec4 {
    pub const ZERO: Self = Self([0.0; 4]);
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self([x, y, z, w])
    }
}

impl AsRef<[f32; 2]> for GpuVec2 {
    fn as_ref(&self) -> &[f32; 2] {
        &self.0
    }
}
impl AsMut<[f32; 2]> for GpuVec2 {
    fn as_mut(&mut self) -> &mut [f32; 2] {
        &mut self.0
    }
}
impl From<[f32; 2]> for GpuVec2 {
    fn from(v: [f32; 2]) -> Self {
        Self(v)
    }
}

impl AsRef<[f32; 4]> for GpuVec4 {
    fn as_ref(&self) -> &[f32; 4] {
        &self.0
    }
}
impl AsMut<[f32; 4]> for GpuVec4 {
    fn as_mut(&mut self) -> &mut [f32; 4] {
        &mut self.0
    }
}
impl From<[f32; 4]> for GpuVec4 {
    fn from(v: [f32; 4]) -> Self {
        Self(v)
    }
}

encase::impl_vector!(2, GpuVec2, f32; using AsRef AsMut From);
encase::impl_vector!(4, GpuVec4, f32; using AsRef AsMut From);

// --- GPU Structs ---

/// A single arc segment - the ONE geometric primitive ("Arc is all you need").
/// `curvature == 0` is a line, `start == end` is a point (sign from `heading`),
/// else the minor arc of radius `1/|curvature|`. 64 bytes (4 x vec4).
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct GpuSegment {
    /// Segment flags. Bit 0: signed (part of closed contour).
    pub flags: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    /// Endpoints in the entry's LOCAL frame: (start.x, start.y, end.x, end.y).
    pub endpoints: GpuVec4,
    /// Arc encoding: (curvature, heading, 0, 0). Signed curvature `1/radius`
    /// (`0` = line, sign selects the bulge side); `heading` is the interior
    /// bisector, meaningful only for a point (`start == end`).
    pub params: GpuVec4,
    /// Arc-length range: (arc_start, arc_end, total_arc_length, 0).
    pub arc_range: GpuVec4,
}

/// A draw entry / command: a unit in the spatial index.
/// 80 bytes (5 x vec4).
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct GpuDrawEntry {
    /// Entry type: 0=curve_segment, 1=shape, 2=tiling.
    pub entry_type: u32,
    /// Index into styles buffer.
    pub style_idx: u32,
    /// Z-order (lower = closer to viewer, rendered first).
    pub z_order: u32,
    /// Flags: bit 0 = is_closed.
    pub flags: u32,
    /// World-space AABB: (min_x, min_y, max_x, max_y).
    pub bounds: GpuVec4,
    /// Offset into segments buffer.
    pub segment_start: u32,
    /// Number of segments.
    pub segment_count: u32,
    /// Tiling type: 0=grid, 1=dots.
    pub tiling_type: u32,
    pub _pad: u32,
    /// Tiling params: (spacing_x, spacing_y, thickness/radius, 0).
    pub tiling_params: GpuVec4,
    /// Per-INSTANCE placement (D1). The entry's segments are stored in a local
    /// frame; the shader evaluates them against `world_p - translate`. `(0,0)`
    /// leaves geometry at the origin. Holding the translate on
    /// the command (not the segment) lets identical shapes at different
    /// positions share ONE segment range - the GPU-instancing prerequisite.
    pub translate: GpuVec2,
    pub _translate_pad: GpuVec2,
}

/// Rendering style: a distance-stop chain + pattern. `MAX_STOPS` stops, each a
/// pair of arc colors (`stop_start` at arc 0, `stop_end` at arc 1) at a signed
/// distance. Distances are packed 4 per vec4 in `stop_dist`. Layout must match
/// the WGSL `GpuStyle`.
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct GpuStyle {
    /// Per-stop color at arc 0.
    pub stop_start: [GpuVec4; crate::style::MAX_STOPS],
    /// Per-stop color at arc 1.
    pub stop_end: [GpuVec4; crate::style::MAX_STOPS],
    /// Per-stop signed distance, packed 4 per vec4 (`MAX_STOPS / 4` vec4s).
    pub stop_dist: [GpuVec4; crate::style::MAX_STOPS / 4],
    /// Number of active stops (1..=MAX_STOPS).
    pub stop_count: u32,
    /// Flags.
    pub flags: u32,
    /// Pattern type.
    pub pattern_type: u32,
    /// Pattern stroke thickness.
    pub pattern_thickness: f32,
    pub pattern_param0: f32,
    pub pattern_param1: f32,
    pub pattern_param2: f32,
    pub flow_speed: f32,
    /// Transfer warp (A3): 0=linear, 1=smoothstep, 2=gamma.
    pub transfer_type: u32,
    /// Transfer parameter (gamma exponent when `transfer_type == 2`).
    pub transfer_param: f32,
    pub _transfer_pad0: u32,
    pub _transfer_pad1: u32,
}

/// Per-draw-call parameters.
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct DrawData {
    /// Widget bounds origin in physical pixels.
    pub bounds_origin: GpuVec2,
    /// Camera position (pan offset).
    pub camera_position: GpuVec2,
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// OS scale factor.
    pub scale_factor: f32,
    /// Animation time in seconds.
    pub time: f32,
    /// Debug flags.
    pub debug_flags: u32,
    /// Total draw entries for this call.
    pub entry_count: u32,
    /// Offset into draw_entries buffer.
    pub entry_start: u32,
    /// Tile grid columns (0 = no spatial index, iterate all).
    pub grid_cols: u32,
    /// Tile grid rows.
    pub grid_rows: u32,
    /// Tile base offset into tile buffers.
    pub tile_base: u32,
    pub _pad0: u32,
    /// Cursor in tile-local physical pixels, for the hovered-tile debug mode.
    pub mouse_px: GpuVec2,
}

/// Minimal compute uniform: just the index into DrawData storage buffer.
/// Everything else is read from DrawData (shared with fragment shader).
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct ComputeUniforms {
    pub draw_index: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

// --- Defaults ---

impl Default for GpuSegment {
    fn default() -> Self {
        Self {
            flags: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            endpoints: GpuVec4::ZERO,
            params: GpuVec4::ZERO,
            arc_range: GpuVec4::ZERO,
        }
    }
}

impl Default for GpuDrawEntry {
    fn default() -> Self {
        Self {
            entry_type: 0,
            style_idx: 0,
            z_order: 0,
            flags: 0,
            bounds: GpuVec4::ZERO,
            segment_start: 0,
            segment_count: 0,
            tiling_type: 0,
            _pad: 0,
            tiling_params: GpuVec4::ZERO,
            translate: GpuVec2::ZERO,
            _translate_pad: GpuVec2::ZERO,
        }
    }
}

impl Default for GpuStyle {
    fn default() -> Self {
        Self {
            stop_start: [GpuVec4::new(1.0, 1.0, 1.0, 1.0); crate::style::MAX_STOPS],
            stop_end: [GpuVec4::new(1.0, 1.0, 1.0, 1.0); crate::style::MAX_STOPS],
            stop_dist: [GpuVec4::ZERO; crate::style::MAX_STOPS / 4],
            stop_count: 1,
            flags: 0,
            pattern_type: 0,
            pattern_thickness: 1.0,
            pattern_param0: 0.0,
            pattern_param1: 0.0,
            pattern_param2: 0.0,
            flow_speed: 0.0,
            transfer_type: 0,
            transfer_param: 0.0,
            _transfer_pad0: 0,
            _transfer_pad1: 0,
        }
    }
}

impl Default for DrawData {
    fn default() -> Self {
        Self {
            bounds_origin: GpuVec2::ZERO,
            camera_position: GpuVec2::ZERO,
            camera_zoom: 1.0,
            scale_factor: 1.0,
            time: 0.0,
            debug_flags: 0,
            entry_count: 0,
            entry_start: 0,
            grid_cols: 0,
            grid_rows: 0,
            tile_base: 0,
            _pad0: 0,
            mouse_px: GpuVec2::ZERO,
        }
    }
}

/// Performance statistics from the last completed frame.
///
/// `#[non_exhaustive]` so new metrics stay a semver-additive patch: the v3 gates
/// read these counters, and "v3 is faster" is an opinion without them. The dedup
/// metrics (`cache_*`, `unique_shapes`, `segment_count`) quantify Improvement A:
/// on a static graph `cache_hit_rate` -> ~1.0 (the R4 contract) and
/// `unique_shapes` << `entry_count` when many nodes share a shape.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct SdfStats {
    /// Draw commands submitted this frame (one per fill/border/shadow/edge).
    pub entry_count: u32,
    /// Spatial-index tiles allocated this frame.
    pub tile_count: u32,
    /// CPU time spent in `prepare` this frame.
    pub prepare_cpu_us: u64,
    /// Distinct shapes whose geometry was uploaded this frame (after dedup). The
    /// per-frame GPU-instancing analogue of the shape cache: identical shapes
    /// upload their segments ONCE, so this is << `entry_count` on repeated nodes.
    pub unique_shapes: u32,
    /// `GpuSegment`s uploaded this frame. With instancing this tracks
    /// unique-shape geometry, not draw count.
    pub segment_count: u32,
    /// Shape-cache hits over the pipeline's lifetime (Improvement A).
    pub cache_hits: u64,
    /// Shape-cache misses (each a boolean->arcs evaluation) over the lifetime.
    pub cache_misses: u64,
    /// `cache_hits / (cache_hits + cache_misses)`; ~1.0 on a static graph is the
    /// R4 cache-hit-rate contract.
    pub cache_hit_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use encase::ShaderSize;

    #[test]
    fn test_gpu_segment_size() {
        let size = GpuSegment::SHADER_SIZE.get();
        assert_eq!(size, 64, "GpuSegment should be 64 bytes");
    }

    #[test]
    fn test_gpu_draw_entry_size() {
        let size = GpuDrawEntry::SHADER_SIZE.get();
        assert_eq!(size, 80, "GpuDrawEntry should be 80 bytes");
    }

    #[test]
    fn test_gpu_style_size() {
        let size = GpuStyle::SHADER_SIZE.get();
        assert_eq!(size % 16, 0, "GpuStyle must be 16-byte aligned, got {size}");
    }

    #[test]
    fn test_draw_data_alignment() {
        let size = DrawData::SHADER_SIZE.get();
        assert_eq!(size % 16, 0, "DrawData must be 16-byte aligned, got {size}");
    }
}
