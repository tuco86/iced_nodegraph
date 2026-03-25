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
    pub fn new(x: f32, y: f32) -> Self { Self([x, y]) }
}

impl GpuVec4 {
    pub const ZERO: Self = Self([0.0; 4]);
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self { Self([x, y, z, w]) }
}

impl AsRef<[f32; 2]> for GpuVec2 { fn as_ref(&self) -> &[f32; 2] { &self.0 } }
impl AsMut<[f32; 2]> for GpuVec2 { fn as_mut(&mut self) -> &mut [f32; 2] { &mut self.0 } }
impl From<[f32; 2]> for GpuVec2 { fn from(v: [f32; 2]) -> Self { Self(v) } }

impl AsRef<[f32; 4]> for GpuVec4 { fn as_ref(&self) -> &[f32; 4] { &self.0 } }
impl AsMut<[f32; 4]> for GpuVec4 { fn as_mut(&mut self) -> &mut [f32; 4] { &mut self.0 } }
impl From<[f32; 4]> for GpuVec4 { fn from(v: [f32; 4]) -> Self { Self(v) } }

encase::impl_vector!(2, GpuVec2, f32; using AsRef AsMut From);
encase::impl_vector!(4, GpuVec4, f32; using AsRef AsMut From);

// --- GPU Structs ---

/// A single geometric segment (line, arc, or cubic bezier).
/// 64 bytes (4 x vec4).
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct GpuSegment {
    /// Segment type: 0=line, 1=arc, 2=cubic_bezier.
    pub segment_type: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    /// Primary geometry. Line: (ax,ay,bx,by). Bezier: (p0x,p0y,p1x,p1y).
    pub geom0: GpuVec4,
    /// Secondary geometry. Bezier: (p2x,p2y,p3x,p3y). Arc: (cx,cy,r,start_angle).
    pub geom1: GpuVec4,
    /// Arc-length range: (arc_start, arc_end, total_arc_length, 0).
    pub arc_range: GpuVec4,
}

/// A draw entry: a unit in the spatial index.
/// 64 bytes (4 x vec4).
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
}

/// Rendering style: 4 corner colors + distance range + pattern.
/// 128 bytes (8 x vec4).
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct GpuStyle {
    /// Color at (arc=0, dist=from).
    pub near_start: GpuVec4,
    /// Color at (arc=1, dist=from).
    pub near_end: GpuVec4,
    /// Color at (arc=0, dist=to).
    pub far_start: GpuVec4,
    /// Color at (arc=1, dist=to).
    pub far_end: GpuVec4,
    /// Inner distance boundary.
    pub dist_from: f32,
    /// Outer distance boundary.
    pub dist_to: f32,
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
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
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
    pub _pad1: u32,
    pub _pad2: u32,
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
            segment_type: 0, _pad0: 0, _pad1: 0, _pad2: 0,
            geom0: GpuVec4::ZERO, geom1: GpuVec4::ZERO, arc_range: GpuVec4::ZERO,
        }
    }
}

impl Default for GpuDrawEntry {
    fn default() -> Self {
        Self {
            entry_type: 0, style_idx: 0, z_order: 0, flags: 0,
            bounds: GpuVec4::ZERO, segment_start: 0, segment_count: 0,
            tiling_type: 0, _pad: 0, tiling_params: GpuVec4::ZERO,
        }
    }
}

impl Default for GpuStyle {
    fn default() -> Self {
        Self {
            near_start: GpuVec4::new(1.0, 1.0, 1.0, 1.0),
            near_end: GpuVec4::new(1.0, 1.0, 1.0, 1.0),
            far_start: GpuVec4::new(1.0, 1.0, 1.0, 1.0),
            far_end: GpuVec4::new(1.0, 1.0, 1.0, 1.0),
            dist_from: -1e6, dist_to: 0.0,
            flags: 0, pattern_type: 0,
            pattern_thickness: 1.0,
            pattern_param0: 0.0, pattern_param1: 0.0, pattern_param2: 0.0,
            flow_speed: 0.0, _pad0: 0.0, _pad1: 0.0, _pad2: 0.0,
        }
    }
}

impl Default for DrawData {
    fn default() -> Self {
        Self {
            bounds_origin: GpuVec2::ZERO, camera_position: GpuVec2::ZERO,
            camera_zoom: 1.0, scale_factor: 1.0, time: 0.0, debug_flags: 0,
            entry_count: 0, entry_start: 0, grid_cols: 0, grid_rows: 0,
            tile_base: 0, _pad0: 0, _pad1: 0, _pad2: 0,
        }
    }
}

/// Performance statistics.
#[derive(Clone, Debug, Default)]
pub struct SdfStats {
    pub entry_count: u32,
    pub tile_count: u32,
    pub prepare_cpu_us: u64,
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
        assert_eq!(size, 64, "GpuDrawEntry should be 64 bytes");
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
