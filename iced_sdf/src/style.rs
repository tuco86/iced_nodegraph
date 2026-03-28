//! Unified 4-color style system.
//!
//! Each style defines a 2D color field:
//! - Arc-length axis (0..1): near_start → near_end
//! - Distance axis (dist_from..dist_to): near → far
//!
//! ```text
//!                 arc=0              arc=1
//! dist_from:  near_start         near_end
//! dist_to:    far_start          far_end
//! ```

use iced::Color;

use crate::pattern::Pattern;

/// Rendering style: 4 corner colors + distance range + optional pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Color at (arc=0, dist=dist_from).
    pub near_start: Color,
    /// Color at (arc=1, dist=dist_from).
    pub near_end: Color,
    /// Color at (arc=0, dist=dist_to).
    pub far_start: Color,
    /// Color at (arc=1, dist=dist_to).
    pub far_end: Color,
    /// Inner distance boundary.
    pub dist_from: f32,
    /// Outer distance boundary.
    pub dist_to: f32,
    /// Optional pattern (modifies effective distance).
    pub pattern: Option<Pattern>,
    /// Special: IQ distance field visualization.
    pub distance_field: bool,
}

impl Style {
    /// Solid color fill (interior of closed shape).
    pub fn solid(color: Color) -> Self {
        Self::uniform(color, -1e6, 0.0)
    }

    /// Stroke with uniform color and thickness.
    pub fn stroke(color: Color, pattern: Pattern) -> Self {
        let ht = pattern.thickness * 0.5;
        Self {
            near_start: color, near_end: color,
            far_start: color, far_end: color,
            dist_from: -ht, dist_to: ht,
            pattern: Some(pattern),
            distance_field: false,
        }
    }

    /// Arc-length gradient (start → end) over a fill.
    pub fn arc_gradient(start: Color, end: Color) -> Self {
        Self {
            near_start: start, near_end: end,
            far_start: start, far_end: end,
            dist_from: -1e6, dist_to: 0.0,
            pattern: None, distance_field: false,
        }
    }

    /// Arc-length gradient stroke with pattern.
    pub fn arc_gradient_stroke(start: Color, end: Color, pattern: Pattern) -> Self {
        let ht = pattern.thickness * 0.5;
        Self {
            near_start: start, near_end: end,
            far_start: start, far_end: end,
            dist_from: -ht, dist_to: ht,
            pattern: Some(pattern),
            distance_field: false,
        }
    }

    /// Shadow: color fades to transparent over distance range.
    pub fn shadow(color: Color, radius: f32) -> Self {
        let transparent = Color::from_rgba(color.r, color.g, color.b, 0.0);
        Self {
            near_start: color, near_end: color,
            far_start: transparent, far_end: transparent,
            dist_from: 0.0, dist_to: radius,
            pattern: None, distance_field: false,
        }
    }

    /// Blur helper: same as shadow but covers both sides.
    pub fn blur(color: Color, radius: f32) -> Self {
        let transparent = Color::from_rgba(color.r, color.g, color.b, 0.0);
        Self {
            near_start: color, near_end: color,
            far_start: transparent, far_end: transparent,
            dist_from: -radius, dist_to: radius,
            pattern: None, distance_field: false,
        }
    }

    /// IQ distance field visualization.
    pub fn distance_field() -> Self {
        Self {
            near_start: Color::from_rgb(0.9, 0.6, 0.3),  // outside: orange
            near_end: Color::from_rgb(0.9, 0.6, 0.3),
            far_start: Color::from_rgb(0.65, 0.85, 1.0),  // inside: blue
            far_end: Color::from_rgb(0.65, 0.85, 1.0),
            dist_from: 0.0, dist_to: 0.0,
            pattern: None, distance_field: true,
        }
    }

    /// Set pattern.
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        let ht = pattern.thickness * 0.5;
        self.pattern = Some(pattern);
        self.dist_from = -ht;
        self.dist_to = ht;
        self
    }

    /// Set distance range explicitly.
    pub fn dist_range(mut self, from: f32, to: f32) -> Self {
        self.dist_from = from;
        self.dist_to = to;
        self
    }

    /// Expand the distance range outward.
    pub fn expand(mut self, amount: f32) -> Self {
        self.dist_from -= amount;
        self.dist_to += amount;
        self
    }

    /// Uniform color over a distance range.
    fn uniform(color: Color, from: f32, to: f32) -> Self {
        Self {
            near_start: color, near_end: color,
            far_start: color, far_end: color,
            dist_from: from, dist_to: to,
            pattern: None, distance_field: false,
        }
    }

    /// Whether this style has active animations.
    pub fn is_animated(&self) -> bool {
        self.pattern.as_ref().is_some_and(|p| p.is_animated())
    }

    /// Whether this style is a fill (no pattern, negative distance visible).
    pub fn is_fill(&self) -> bool {
        self.pattern.is_none() && self.dist_from < -100.0
    }

    /// Maximum visual extent beyond the shape boundary.
    pub fn max_effect_radius(&self) -> f32 {
        if self.distance_field { return f32::INFINITY; }
        self.dist_to.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_is_fill() {
        let s = Style::solid(Color::WHITE);
        assert!(s.is_fill());
    }

    #[test]
    fn stroke_is_not_fill() {
        let s = Style::stroke(Color::WHITE, Pattern::solid(2.0));
        assert!(!s.is_fill());
    }

    #[test]
    fn shadow_effect_radius() {
        let s = Style::shadow(Color::BLACK, 10.0);
        assert_eq!(s.max_effect_radius(), 10.0);
    }
}
