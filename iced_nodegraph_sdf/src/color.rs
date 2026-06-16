//! `ColorQuad`: the four corner colors of a [`Style`](crate::Style)'s 2D color
//! field.
//!
//! A style is always a 2D color field: the arc-length axis (start -> end)
//! crossed with the distance axis (near -> far). `ColorQuad` packages those four
//! corners uniformly so callers can express the full gradient capability through
//! a single value, with constructors for the common cases. `From<Color>` keeps
//! the solid case a one-liner. The [`Style::quad_band`](crate::Style::quad_band)
//! and [`Style::quad_stroke`](crate::Style::quad_stroke) builders consume it
//! directly.

use iced::Color;

/// Four corner colors: arc-length (start/end) crossed with distance (near/far).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorQuad {
    /// arc = 0, distance = near
    pub near_start: Color,
    /// arc = 1, distance = near
    pub near_end: Color,
    /// arc = 0, distance = far
    pub far_start: Color,
    /// arc = 1, distance = far
    pub far_end: Color,
}

impl ColorQuad {
    /// All four corners the same color.
    pub fn solid(color: Color) -> Self {
        Self {
            near_start: color,
            near_end: color,
            far_start: color,
            far_end: color,
        }
    }

    /// Gradient along the arc-length axis (start -> end), uniform across distance.
    pub fn arc(start: Color, end: Color) -> Self {
        Self {
            near_start: start,
            near_end: end,
            far_start: start,
            far_end: end,
        }
    }

    /// Gradient along the distance axis (near -> far), uniform across arc.
    /// The shadow case: inner (full) -> outer.
    pub fn dist(near: Color, far: Color) -> Self {
        Self {
            near_start: near,
            near_end: near,
            far_start: far,
            far_end: far,
        }
    }

    /// Full control over all four corners.
    pub fn corners(near_start: Color, near_end: Color, far_start: Color, far_end: Color) -> Self {
        Self {
            near_start,
            near_end,
            far_start,
            far_end,
        }
    }

    /// Distance gradient from `color` to the same color at alpha 0 (fade-out).
    pub fn fade(color: Color) -> Self {
        Self::dist(color, Color::from_rgba(color.r, color.g, color.b, 0.0))
    }

    /// The near arc-color pair: `(near_start, near_end)`. The colors a stroke or
    /// a band's near edge uses.
    pub fn arc_pair(&self) -> (Color, Color) {
        (self.near_start, self.near_end)
    }

    /// All four corners with their alpha multiplied by `opacity`.
    pub fn with_opacity(self, opacity: f32) -> Self {
        let f = |c: Color| Color {
            a: c.a * opacity,
            ..c
        };
        Self {
            near_start: f(self.near_start),
            near_end: f(self.near_end),
            far_start: f(self.far_start),
            far_end: f(self.far_end),
        }
    }
}

impl From<Color> for ColorQuad {
    fn from(color: Color) -> Self {
        Self::solid(color)
    }
}

impl From<(Color, Color, Color, Color)> for ColorQuad {
    /// `(near_start, near_end, far_start, far_end)`.
    fn from(c: (Color, Color, Color, Color)) -> Self {
        Self::corners(c.0, c.1, c.2, c.3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_color_is_solid() {
        let q: ColorQuad = Color::WHITE.into();
        assert_eq!(q, ColorQuad::solid(Color::WHITE));
    }

    #[test]
    fn from_tuple_maps_corners() {
        let corners = (
            Color::WHITE,
            Color::BLACK,
            Color::from_rgb(1.0, 0.0, 0.0),
            Color::from_rgb(0.0, 1.0, 0.0),
        );
        let q: ColorQuad = corners.into();
        assert_eq!(
            q,
            ColorQuad::corners(corners.0, corners.1, corners.2, corners.3)
        );
    }

    #[test]
    fn with_opacity_scales_all_alphas() {
        let q = ColorQuad::corners(
            Color::from_rgba(1.0, 0.0, 0.0, 1.0),
            Color::from_rgba(0.0, 1.0, 0.0, 0.5),
            Color::from_rgba(0.0, 0.0, 1.0, 0.8),
            Color::from_rgba(1.0, 1.0, 1.0, 0.2),
        )
        .with_opacity(0.5);
        assert_eq!(q.near_start.a, 0.5);
        assert_eq!(q.near_end.a, 0.25);
        assert_eq!(q.far_start.a, 0.4);
        assert!((q.far_end.a - 0.1).abs() < 1e-6);
        // Color channels untouched.
        assert_eq!(q.near_start.r, 1.0);
    }
}
