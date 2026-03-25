//! Style definitions for SDF rendering.
//!
//! A Style describes how a drawable is rendered: fill color, gradient,
//! pattern, blur, expand, and outline.

use iced::Color;

use crate::pattern::Pattern;

/// Fill mode for a drawable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    /// Solid color fill.
    Solid(Color),
    /// Linear gradient by angle (radians).
    Gradient { start: Color, end: Color, angle: f32 },
    /// Gradient along arc-length parameter (0.0 to 1.0).
    ArcLengthGradient { start: Color, end: Color },
    /// IQ-style sine-wave distance field visualization.
    DistanceField,
}

/// Outline drawn at the shape boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub thickness: f32,
    pub color: Color,
}

/// Rendering style for a drawable.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Fill color or gradient.
    pub fill: Fill,
    /// Expand/contract amount (positive = expand outward).
    pub expand: f32,
    /// Blur amount (gaussian blur radius).
    pub blur: f32,
    /// Stroke pattern (None = fill mode, Some = stroke mode).
    pub pattern: Option<Pattern>,
    /// Outline at the shape boundary.
    pub outline: Option<Outline>,
}

impl Style {
    /// Solid color fill.
    pub fn solid(color: Color) -> Self {
        Self { fill: Fill::Solid(color), expand: 0.0, blur: 0.0, pattern: None, outline: None }
    }

    /// Linear gradient fill by angle.
    pub fn gradient(start: Color, end: Color, angle: f32) -> Self {
        Self { fill: Fill::Gradient { start, end, angle }, expand: 0.0, blur: 0.0, pattern: None, outline: None }
    }

    /// Arc-length gradient fill.
    pub fn arc_gradient(start: Color, end: Color) -> Self {
        Self { fill: Fill::ArcLengthGradient { start, end }, expand: 0.0, blur: 0.0, pattern: None, outline: None }
    }

    /// IQ-style sine-wave distance field visualization.
    pub fn distance_field() -> Self {
        Self { fill: Fill::DistanceField, expand: 0.0, blur: 0.0, pattern: None, outline: None }
    }

    /// Stroke with color and pattern.
    pub fn stroke(color: Color, pattern: Pattern) -> Self {
        Self { fill: Fill::Solid(color), expand: 0.0, blur: 0.0, pattern: Some(pattern), outline: None }
    }

    /// Set expand/contract amount.
    pub fn expand(mut self, amount: f32) -> Self { self.expand = amount; self }

    /// Set blur amount.
    pub fn blur(mut self, amount: f32) -> Self { self.blur = amount; self }

    /// Set outline at shape boundary.
    pub fn outline(mut self, thickness: f32, color: Color) -> Self {
        self.outline = Some(Outline { thickness, color });
        self
    }

    /// Set stroke pattern.
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Whether this style has active animations (flow speed).
    pub fn is_animated(&self) -> bool {
        self.pattern.as_ref().is_some_and(|p| p.is_animated())
    }

    /// Whether this style is a fill (no pattern).
    pub fn is_fill(&self) -> bool {
        self.pattern.is_none()
    }

    /// Maximum visual extent beyond the shape boundary.
    pub fn max_effect_radius(&self) -> f32 {
        if matches!(self.fill, Fill::DistanceField) {
            return f32::INFINITY;
        }
        let mut r = self.expand.abs() + self.blur;
        if let Some(ref p) = self.pattern {
            r += p.thickness / 2.0;
        }
        if let Some(ref o) = self.outline {
            r = r.max(o.thickness / 2.0);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_style() {
        let s = Style::solid(Color::WHITE);
        assert!(s.is_fill());
        assert!(!s.is_animated());
    }

    #[test]
    fn test_stroke_style() {
        let s = Style::stroke(Color::WHITE, Pattern::solid(2.0));
        assert!(!s.is_fill());
    }

    #[test]
    fn test_max_effect_radius() {
        let s = Style::solid(Color::WHITE).expand(5.0).blur(3.0);
        assert_eq!(s.max_effect_radius(), 8.0);
    }
}
