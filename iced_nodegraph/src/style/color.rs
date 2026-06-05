//! `ColorQuad`: the four corner colors of an iced_nodegraph_sdf `Style`.
//!
//! An iced_nodegraph_sdf style is always a 2D color field: the arc-length axis
//! (start -> end) crossed with the distance axis (near -> far). `ColorQuad`
//! packages those four corners uniformly so node/edge/pin styles can expose the
//! full gradient capability through a single field type, with constructors for
//! the common cases. `From<Color>` keeps the solid case a one-liner.

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
}
