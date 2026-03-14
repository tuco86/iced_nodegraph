//! GPU data structures for SDF rendering.
//!
//! These types are serialized to GPU buffers using encase's ShaderType.
//! Layout must match the corresponding WGSL structs in shader.wgsl.

#![allow(dead_code)]

use encase::ShaderType;

/// Per-draw parameters stored in a storage buffer.
///
/// Indexed by `instance_index` in the fragment shader so each
/// draw call reads its own camera, shape range, and settings.
#[derive(Clone, Debug, ShaderType)]
pub struct DrawData {
    /// Camera position (pan offset).
    pub camera_position: glam::Vec2,
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// OS scale factor (logical to physical pixel ratio).
    pub scale_factor: f32,
    /// Animation time in seconds.
    pub time: f32,
    /// Debug flags.
    pub debug_flags: u32,
    /// First shape index in the shapes buffer for this draw.
    pub shape_start: u32,
    /// Number of shapes for this draw.
    pub shape_count: u32,
}

/// A single shape's metadata for the compute/render pipeline.
///
/// Each shape has its own SDF ops and layers, referenced by offset into
/// the flat ops/layers storage buffers.
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct ShapeInstance {
    /// Screen-space bounding box: (x, y, width, height).
    pub bounds: glam::Vec4,
    /// Offset into the ops buffer for this shape's RPN operations.
    pub ops_offset: u32,
    /// Number of RPN operations for this shape.
    pub ops_count: u32,
    /// Offset into the layers buffer for this shape's layers.
    pub layers_offset: u32,
    /// Number of layers for this shape.
    pub layers_count: u32,
    /// Max effect radius (expand + blur + outline) for tile culling.
    pub max_radius: f32,
    /// Whether this shape has a fill layer (affects cull distance calc).
    pub has_fill: u32,
    pub _pad2: u32,
    pub _pad3: u32,
}

/// A single SDF operation (primitive or CSG op).
///
/// The GPU evaluates these in RPN order using a stack.
/// Layout: 48 bytes (3 x vec4).
#[derive(Clone, Debug, ShaderType)]
pub struct SdfOp {
    /// Operation type:
    /// - 0-15: Primitives (Circle, Box, RoundedBox, Line, Bezier)
    /// - 16-31: Boolean ops (Union, Subtract, Intersect, SmoothUnion, SmoothSubtract)
    /// - 32-47: Modifiers (Round, Onion)
    pub op_type: u32,
    /// Flags for operation-specific behavior.
    pub flags: u32,
    /// Padding for 16-byte alignment.
    pub _pad0: u32,
    pub _pad1: u32,
    /// Primary parameters (position, size, control points).
    pub param0: glam::Vec4,
    /// Secondary parameters.
    pub param1: glam::Vec4,
    /// Tertiary parameters (reserved).
    pub param2: glam::Vec4,
}

impl Default for ShapeInstance {
    fn default() -> Self {
        Self {
            bounds: glam::Vec4::ZERO,
            ops_offset: 0,
            ops_count: 0,
            layers_offset: 0,
            layers_count: 0,
            max_radius: 0.0,
            has_fill: 0,
            _pad2: 0,
            _pad3: 0,
        }
    }
}

impl Default for SdfOp {
    fn default() -> Self {
        Self {
            op_type: 0,
            flags: 0,
            _pad0: 0,
            _pad1: 0,
            param0: glam::Vec4::ZERO,
            param1: glam::Vec4::ZERO,
            param2: glam::Vec4::ZERO,
        }
    }
}

/// A rendering layer with styling.
///
/// Each layer takes the SDF result and maps it to a color
/// with optional effects (expand, blur, gradient, pattern).
#[derive(Clone, Debug, ShaderType)]
pub struct SdfLayer {
    /// Fill color (RGBA).
    pub color: glam::Vec4,
    /// Gradient end color (if using gradient).
    pub gradient_color: glam::Vec4,
    /// Expand/contract amount (positive = expand).
    pub expand: f32,
    /// Blur amount (gaussian blur radius).
    pub blur: f32,
    /// Gradient angle in radians.
    pub gradient_angle: f32,
    /// Layer flags:
    /// - bit 0: use gradient
    /// - bit 1: gradient along u (arc-length) instead of angle
    /// - bit 2: has pattern
    pub flags: u32,
    /// Pattern type: 0=solid, 1=dashed, 2=arrowed, 3=dotted
    pub pattern_type: u32,
    /// Stroke thickness.
    pub thickness: f32,
    /// Pattern parameter 0 (dash length / segment length / spacing).
    pub pattern_param0: f32,
    /// Pattern parameter 1 (gap length / radius).
    pub pattern_param1: f32,
    /// Pattern parameter 2 (angle for arrowed).
    pub pattern_param2: f32,
    /// Flow animation speed (world units per second).
    pub flow_speed: f32,
    /// Outline color (RGBA). Outline is drawn at the boundary of the layer shape.
    pub outline_color: glam::Vec4,
    /// Outline thickness in world units (0 = no outline).
    pub outline_thickness: f32,
    /// Offset for shadow positioning (world units).
    pub offset: glam::Vec2,
}

impl Default for SdfLayer {
    fn default() -> Self {
        Self {
            color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            gradient_color: glam::Vec4::ZERO,
            expand: 0.0,
            blur: 0.0,
            gradient_angle: 0.0,
            flags: 0,
            pattern_type: 0,
            thickness: 0.0,
            pattern_param0: 0.0,
            pattern_param1: 0.0,
            pattern_param2: 0.0,
            flow_speed: 0.0,
            outline_color: glam::Vec4::ZERO,
            outline_thickness: 0.0,
            offset: glam::Vec2::ZERO,
        }
    }
}

impl Default for DrawData {
    fn default() -> Self {
        Self {
            camera_position: glam::Vec2::ZERO,
            camera_zoom: 1.0,
            scale_factor: 1.0,
            time: 0.0,
            debug_flags: 0,
            shape_start: 0,
            shape_count: 0,
        }
    }
}

/// Performance statistics for the SDF pipeline.
///
/// Published at the end of each frame (during `trim()`).
/// Read via `iced_sdf::sdf_stats()`.
#[derive(Clone, Debug, Default)]
pub struct SdfStats {
    /// Number of shapes submitted this frame.
    pub shape_count: u32,
    /// Number of tile instances emitted.
    pub tile_count: u32,
    /// CPU time in prepare() (microseconds).
    pub prepare_cpu_us: u64,
    /// GPU render time (microseconds). None if timestamp queries unavailable.
    pub gpu_time_us: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use encase::ShaderSize;

    #[test]
    fn test_sdf_op_size() {
        let size = SdfOp::SHADER_SIZE.get();
        // 4 u32 (16 bytes) + 3 vec4 (48 bytes) = 64 bytes
        assert_eq!(size, 64, "SdfOp should be 64 bytes");
    }

    #[test]
    fn test_sdf_layer_size() {
        let size = SdfLayer::SHADER_SIZE.get();
        assert!(size > 0, "SdfLayer size should be positive");
        assert!(size % 16 == 0, "SdfLayer size should be 16-byte aligned");
    }

    #[test]
    fn test_draw_data_size() {
        let size = DrawData::SHADER_SIZE.get();
        assert!(size > 0);
        assert_eq!(size % 16, 0, "DrawData size {size} not 16-byte aligned");
    }

    #[test]
    fn test_shape_instance_size() {
        let size = ShapeInstance::SHADER_SIZE.get();
        assert!(size > 0, "ShapeInstance size should be positive");
        assert!(
            size % 16 == 0,
            "ShapeInstance size should be 16-byte aligned"
        );
    }
}
