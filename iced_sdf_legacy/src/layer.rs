//! Layer configuration for SDF rendering.
//!
//! Each layer maps the SDF distance to a color with optional effects.

use iced::Color;

use crate::pattern::Pattern;
use crate::pipeline::types::{GpuVec2, GpuVec4, SdfLayer};

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
    color: Color,
    gradient_color: Option<Color>,
    gradient_along_u: bool,
    gradient_angle: f32,
    expand: f32,
    blur: f32,
    pattern: Option<Pattern>,
    distance_field: bool,
    outline_thickness: f32,
    outline_color: Color,
    offset: [f32; 2],
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
            outline_thickness: 0.0,
            outline_color: Color::BLACK,
            offset: [0.0, 0.0],
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
            outline_thickness: 0.0,
            outline_color: Color::BLACK,
            offset: [0.0, 0.0],
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
            outline_thickness: 0.0,
            outline_color: Color::BLACK,
            offset: [0.0, 0.0],
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
            outline_thickness: 0.0,
            outline_color: Color::BLACK,
            offset: [0.0, 0.0],
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
            outline_thickness: 0.0,
            outline_color: Color::BLACK,
            offset: [0.0, 0.0],
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

    /// Set gradient end color.
    pub fn gradient_color(mut self, color: Color) -> Self {
        self.gradient_color = Some(color);
        self
    }

    /// Set gradient along arc-length (u parameter).
    pub fn gradient_along_u(mut self, along_u: bool) -> Self {
        self.gradient_along_u = along_u;
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

    /// Add an outline at the boundary of this layer's shape.
    ///
    /// The outline follows the same shape as the fill (including patterns).
    /// For dashed strokes, the outline wraps around each dash.
    pub fn outline(mut self, thickness: f32, color: Color) -> Self {
        self.outline_thickness = thickness;
        self.outline_color = color;
        self
    }

    /// Set offset for shadow positioning.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = [x, y];
        self
    }

    /// Set pattern for stroke rendering.
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Compute the effective signed distance to this layer's visible boundary.
    ///
    /// For solid fills: same as raw dist (adjusted by expand).
    /// For strokes: `|raw_dist - expand| - thickness/2`.
    pub fn visual_distance(&self, raw_dist: f32) -> f32 {
        let d = raw_dist - self.expand;
        match &self.pattern {
            Some(p) => d.abs() - p.thickness * 0.5,
            None => d,
        }
    }

    /// Whether this layer has active time-dependent animations.
    ///
    /// Returns `true` if the layer's pattern has a non-zero flow speed.
    pub fn is_animated(&self) -> bool {
        self.pattern.as_ref().is_some_and(|p| p.flow_speed != 0.0)
    }

    /// Whether this layer fills the shape interior (no pattern = solid fill).
    ///
    /// Fill layers render everywhere inside the shape boundary (dist <= 0),
    /// so interior tiles must not be culled.
    pub fn is_fill(&self) -> bool {
        self.pattern.is_none() && !self.distance_field
    }

    /// Maximum radius of visual effect beyond the shape boundary.
    ///
    /// Used for tile culling: a tile can be skipped if the SDF distance
    /// minus the tile half-diagonal exceeds this radius.
    ///
    /// Returns `f32::INFINITY` for distance field layers, since they
    /// render color bands at every distance and need the full quad.
    pub fn max_effect_radius(&self) -> f32 {
        if self.distance_field {
            return f32::INFINITY;
        }
        self.expand.abs()
            + self.blur
            + self.outline_thickness
            + self.pattern
                .map(|p| p.thickness * 0.5)
                .unwrap_or(0.0)
            + self.offset[0].abs().max(self.offset[1].abs())
    }

    /// Convert to GPU representation.
    pub(crate) fn to_gpu(&self) -> SdfLayer {
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
            GpuVec4::new(gc.r, gc.g, gc.b, gc.a)
        } else {
            GpuVec4::ZERO
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
            color: GpuVec4::new(self.color.r, self.color.g, self.color.b, self.color.a),
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
            outline_color: GpuVec4::new(
                self.outline_color.r,
                self.outline_color.g,
                self.outline_color.b,
                self.outline_color.a,
            ),
            outline_thickness: self.outline_thickness,
            offset: GpuVec2::new(self.offset[0], self.offset[1]),
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
        assert_eq!(gpu.color.0[0], 1.0);
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

    #[test]
    fn test_max_effect_radius() {
        let layer = Layer::solid(Color::WHITE).expand(5.0).blur(3.0).outline(2.0, Color::BLACK);
        assert_eq!(layer.max_effect_radius(), 10.0); // 5 + 3 + 2

        let layer = Layer::stroke(Color::WHITE, Pattern::solid(4.0));
        assert_eq!(layer.max_effect_radius(), 2.0); // pattern thickness * 0.5

        let layer = Layer::distance_field(Color::WHITE, Color::BLACK);
        assert!(layer.max_effect_radius().is_infinite());

        let layer = Layer::solid(Color::WHITE).offset(4.0, 3.0);
        assert_eq!(layer.max_effect_radius(), 4.0); // max(|4|, |3|)

        let layer = Layer::solid(Color::WHITE).expand(2.0).blur(1.0).offset(-5.0, 5.0);
        assert_eq!(layer.max_effect_radius(), 8.0); // 2 + 1 + 5
    }

    #[test]
    fn test_builder_methods() {
        let layer = Layer::solid(Color::WHITE)
            .gradient_color(Color::BLACK)
            .gradient_along_u(true)
            .gradient_scale(0.5);
        let gpu = layer.to_gpu();
        assert_eq!(gpu.flags & FLAG_GRADIENT, FLAG_GRADIENT);
        assert_eq!(gpu.flags & FLAG_GRADIENT_U, FLAG_GRADIENT_U);
        assert_eq!(gpu.gradient_angle, 0.5);
    }

    #[test]
    fn test_is_animated_static() {
        assert!(!Layer::solid(Color::WHITE).is_animated());
        assert!(!Layer::stroke(Color::WHITE, Pattern::solid(2.0)).is_animated());
        assert!(!Layer::stroke(Color::WHITE, Pattern::dashed(2.0, 10.0, 5.0)).is_animated());
        assert!(!Layer::distance_field(Color::WHITE, Color::BLACK).is_animated());
    }

    #[test]
    fn test_is_animated_with_flow() {
        let layer = Layer::stroke(Color::WHITE, Pattern::solid(2.0).flow(50.0));
        assert!(layer.is_animated());

        let layer = Layer::stroke(Color::WHITE, Pattern::dashed(2.0, 10.0, 5.0).flow(100.0));
        assert!(layer.is_animated());
    }

    #[test]
    fn test_is_animated_zero_flow() {
        let layer = Layer::stroke(Color::WHITE, Pattern::solid(2.0).flow(0.0));
        assert!(!layer.is_animated());
    }
}
