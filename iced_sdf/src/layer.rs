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
        }
    }

    /// Create a layer with a gradient along the shape's arc-length.
    pub fn gradient_u(start: Color, end: Color) -> Self {
        Self {
            color: start,
            gradient_color: Some(end),
            gradient_along_u: true,
            gradient_angle: 0.0,
            expand: 0.0,
            blur: 0.0,
            pattern: None,
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

    /// Set pattern for stroke rendering.
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Convert to GPU representation.
    pub fn to_gpu(&self) -> SdfLayer {
        let mut flags = 0u32;
        let gradient_color = if let Some(gc) = self.gradient_color {
            flags |= FLAG_GRADIENT;
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
