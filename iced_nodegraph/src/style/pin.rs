//! `PinStyle`: per-pin visual style.
//!
//! Flat, concrete struct expanded by [`#[style]`](style) into the typestate form
//! (`PinStyle<Partial>` overlay / `PinStyle<Resolved>` renderer form). See
//! [`super::node`] for the full pattern. Color fields are [`ColorQuad`]s; a plain
//! `Color` coerces to a solid quad. Border on/off is the `border_width` sentinel
//! (0 = no border).
//!
use iced::Color;
use iced_nodegraph_macros::style;

use super::PinShape;
use super::color::ColorQuad;
use super::mode::{Partial, Resolved, StyleMode};

/// Visual style for a pin indicator.
#[style]
pub struct PinStyle {
    // Indicator
    /// Pin indicator color.
    pub color: ColorQuad,
    /// Indicator radius in world-space pixels.
    pub radius: f32,
    /// Indicator shape.
    pub shape: PinShape,

    // Border (width 0 = no border)
    /// Border color.
    pub border_color: ColorQuad,
    /// Border width in world-space pixels. 0 = no border.
    pub border_width: f32,
}

impl PinStyle<Resolved> {
    /// Data pin preset (circle, blue).
    pub fn data() -> Self {
        Self {
            color: ColorQuad::solid(Color::from_rgb(0.3, 0.6, 1.0)),
            radius: 6.0,
            shape: PinShape::Circle,
            border_color: ColorQuad::solid(Color::from_rgb(0.5, 0.7, 1.0)),
            border_width: 1.0,
        }
    }

    /// Execution pin preset (triangle, white, borderless).
    pub fn execution() -> Self {
        Self {
            color: ColorQuad::solid(Color::WHITE),
            radius: 7.0,
            shape: PinShape::Triangle,
            border_color: ColorQuad::solid(Color::TRANSPARENT),
            border_width: 0.0,
        }
    }

    /// Control flow pin preset (diamond, yellow).
    pub fn control() -> Self {
        Self {
            color: ColorQuad::solid(Color::from_rgb(1.0, 0.85, 0.3)),
            radius: 6.0,
            shape: PinShape::Diamond,
            border_color: ColorQuad::solid(Color::from_rgb(1.0, 0.95, 0.6)),
            border_width: 1.0,
        }
    }

    /// Event pin preset (square, green).
    pub fn event() -> Self {
        Self {
            color: ColorQuad::solid(Color::from_rgb(0.3, 0.8, 0.4)),
            radius: 5.0,
            shape: PinShape::Square,
            border_color: ColorQuad::solid(Color::from_rgb(0.5, 0.9, 0.6)),
            border_width: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Theme;

    #[test]
    fn overlay_merged_over_default_resolves() {
        use crate::style::{PinStatus, default_pin_style};
        let overlay = PinStyle::new().radius(10.0).shape(PinShape::Square);
        let base = default_pin_style(&Theme::Dark, PinStatus::Idle);
        let resolved = overlay.merge(&base).resolve();

        assert_eq!(resolved.radius, 10.0); // overlay wins
        assert_eq!(resolved.shape, PinShape::Square); // overlay wins
        assert_eq!(resolved.color, base.color.unwrap()); // inherited from default
    }

    #[test]
    fn merge_prefers_self() {
        let a = PinStyle::new().radius(8.0);
        let b = PinStyle::new().radius(4.0).border_width(2.0);
        let m = a.merge(&b);

        assert_eq!(m.radius, Some(8.0)); // self wins
        assert_eq!(m.border_width, Some(2.0)); // filled from other
    }
}
