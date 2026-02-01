//! GPU data structures for SDF rendering.
//!
//! These types are serialized to GPU buffers using encase's ShaderType.
//! Layout must match the corresponding WGSL structs in shader.wgsl.

#![allow(dead_code)]

use encase::ShaderType;

/// Global uniforms for the SDF shader.
#[derive(Clone, Debug, ShaderType)]
pub struct Uniforms {
    /// Viewport size in pixels.
    pub viewport_size: glam::Vec2,
    /// Camera position (pan offset).
    pub camera_position: glam::Vec2,
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Time in seconds for animations.
    pub time: f32,
    /// Number of SDF operations in the buffer.
    pub num_ops: u32,
    /// Number of layers.
    pub num_layers: u32,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encase::ShaderSize;

    #[test]
    fn test_uniforms_size() {
        let size = Uniforms::SHADER_SIZE.get();
        assert!(size > 0, "Uniforms size should be positive");
        assert!(size % 16 == 0, "Uniforms size should be 16-byte aligned");
    }

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
}
