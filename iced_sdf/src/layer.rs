//! Layer configuration for SDF rendering.
//!
//! Each layer maps the SDF distance to a color with optional effects.

use iced::Color;

use crate::pattern::Pattern;
use crate::pipeline::types::SdfLayer;

/// Layer flags.
const FLAG_GRADIENT: u32 = 1;
const FLAG_GRADIENT_U: u32 = 2;
const FLAG_HAS_PATTERN: u32 = 4;
const FLAG_DISTANCE_FIELD: u32 = 8;

/// A rendering layer for SDF shapes.
///
/// Layers are composited back-to-front, allowing complex visual effects
/// like shadows, outlines, and gradients.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Fill color.
    pub color: Color,
    /// Gradient end color (if using gradient).
    pub gradient_color: Option<Color>,
    /// Gradient along u (arc-length) instead of angle.
    pub gradient_along_u: bool,
    /// Gradient angle in radians.
    pub gradient_angle: f32,
    /// Expand/contract amount (positive = expand).
    pub expand: f32,
    /// Blur amount (gaussian blur radius).
    pub blur: f32,
    /// Optional pattern for stroke rendering.
    pub pattern: Option<Pattern>,
    /// Distance field visualization mode (IQ style).
    pub distance_field: bool,
}

impl Layer {
    /// Create a solid color layer.
    pub fn solid(color: Color) -> Self {
        Self {
            color,
            gradient_color: None,
            gradient_along_u: false,
            gradient_angle: 0.0,
            expand: 0.0,
            blur: 0.0,
            pattern: None,
            distance_field: false,
        }
    }

    /// Create a layer with a linear gradient.
    pub fn gradient(start: Color, end: Color, angle: f32) -> Self {
        Self {
            color: start,
            gradient_color: Some(end),
            gradient_along_u: false,
            gradient_angle: angle,
            expand: 0.0,
            blur: 0.0,
            pattern: None,
            distance_field: false,
        }
    }

    /// Create a layer with a gradient along the shape's arc-length.
    ///
    /// Default scale is 0.01 (one full gradient cycle per 100 world units).
    /// Use `.gradient_scale()` to normalize to a specific arc-length.
    pub fn gradient_u(start: Color, end: Color) -> Self {
        Self {
            color: start,
            gradient_color: Some(end),
            gradient_along_u: true,
            gradient_angle: 0.01,
            expand: 0.0,
            blur: 0.0,
            pattern: None,
            distance_field: false,
        }
    }

    /// Create a distance field visualization layer (IQ/Shadertoy style).
    ///
    /// Shows the raw distance field with color bands, boundary highlight,
    /// and inside/outside coloring. Useful for debugging and visualization.
    pub fn distance_field(outside: Color, inside: Color) -> Self {
        Self {
            color: outside,
            gradient_color: Some(inside),
            gradient_along_u: false,
            gradient_angle: 0.0,
            expand: 0.0,
            blur: 0.0,
            pattern: None,
            distance_field: true,
        }
    }

    /// Create a stroke layer with pattern.
    pub fn stroke(color: Color, pattern: Pattern) -> Self {
        Self {
            color,
            gradient_color: None,
            gradient_along_u: false,
            gradient_angle: 0.0,
            expand: 0.0,
            blur: 0.0,
            pattern: Some(pattern),
            distance_field: false,
        }
    }

    /// Set expand/contract amount.
    pub fn expand(mut self, amount: f32) -> Self {
        self.expand = amount;
        self
    }

    /// Set blur amount.
    pub fn blur(mut self, amount: f32) -> Self {
        self.blur = amount;
        self
    }

    /// Set arc-length gradient scale (used with `gradient_u`).
    ///
    /// Pass `1.0 / total_arc_length` to normalize the gradient to 0..1
    /// along the full shape.
    pub fn gradient_scale(mut self, scale: f32) -> Self {
        self.gradient_angle = scale;
        self
    }

    /// Set pattern for stroke rendering.
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Convert to GPU representation.
    pub fn to_gpu(&self) -> SdfLayer {
        let mut flags = 0u32;
        if self.distance_field {
            flags |= FLAG_DISTANCE_FIELD;
        }
        let gradient_color = if let Some(gc) = self.gradient_color {
            if !self.distance_field {
                flags |= FLAG_GRADIENT;
            }
            if self.gradient_along_u {
                flags |= FLAG_GRADIENT_U;
            }
            glam::Vec4::new(gc.r, gc.g, gc.b, gc.a)
        } else {
            glam::Vec4::ZERO
        };

        // Pattern data
        let (pattern_type, thickness, pattern_param0, pattern_param1, pattern_param2, flow_speed) =
            if let Some(ref p) = self.pattern {
                flags |= FLAG_HAS_PATTERN;
                p.to_gpu()
            } else {
                (0, 0.0, 0.0, 0.0, 0.0, 0.0)
            };

        SdfLayer {
            color: glam::Vec4::new(self.color.r, self.color.g, self.color.b, self.color.a),
            gradient_color,
            expand: self.expand,
            blur: self.blur,
            gradient_angle: self.gradient_angle,
            flags,
            pattern_type,
            thickness,
            pattern_param0,
            pattern_param1,
            pattern_param2,
            flow_speed,
        }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::solid(Color::WHITE)
    }
}

/// Default IQ-style distance field colors (warm outside, cool inside).
impl Layer {
    /// IQ-style distance field with default colors.
    pub fn distance_field_default() -> Self {
        Self::distance_field(
            Color::from_rgb(0.9, 0.6, 0.3),
            Color::from_rgb(0.65, 0.85, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_layer() {
        let layer = Layer::solid(Color::from_rgb(1.0, 0.0, 0.0));
        let gpu = layer.to_gpu();
        assert_eq!(gpu.color.x, 1.0);
        assert_eq!(gpu.flags, 0);
    }

    #[test]
    fn test_gradient_layer() {
        let layer = Layer::gradient(Color::WHITE, Color::BLACK, std::f32::consts::PI);
        let gpu = layer.to_gpu();
        assert_eq!(gpu.flags & FLAG_GRADIENT, FLAG_GRADIENT);
        assert_eq!(gpu.gradient_angle, std::f32::consts::PI);
    }

    #[test]
    fn test_expand_blur() {
        let layer = Layer::solid(Color::BLACK).expand(5.0).blur(3.0);
        let gpu = layer.to_gpu();
        assert_eq!(gpu.expand, 5.0);
        assert_eq!(gpu.blur, 3.0);
    }

    #[test]
    fn test_stroke_layer() {
        let layer = Layer::stroke(
            Color::from_rgb(1.0, 0.0, 0.0),
            Pattern::dashed(2.0, 10.0, 5.0),
        );
        let gpu = layer.to_gpu();
        assert_eq!(gpu.flags & FLAG_HAS_PATTERN, FLAG_HAS_PATTERN);
        assert_eq!(gpu.pattern_type, 1); // dashed
        assert_eq!(gpu.thickness, 2.0);
    }

    #[test]
    fn test_pattern_flow() {
        let layer = Layer::stroke(Color::WHITE, Pattern::solid(3.0).flow(100.0));
        let gpu = layer.to_gpu();
        assert_eq!(gpu.flow_speed, 100.0);
    }
}
