//! `PinStyle`: per-pin visual style.
//!
//! A flat, concrete struct the renderer consumes directly. See [`super::node`]
//! for the override-via-struct-update pattern over [`default_pin_style`](crate::default_pin_style).
//! Color fields are [`ColorQuad`]s; a plain `Color` coerces to a solid quad.
//! Border on/off is the `border_width` sentinel (0 = no border).
//!
use iced::Color;

use super::ColorQuad;
use super::PinShape;

/// Visual style for a pin indicator.
#[derive(Debug, Clone, PartialEq)]
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

impl PinStyle {
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

    #[test]
    fn struct_update_overrides_over_default() {
        use crate::style::{PinStatus, default_pin_style};
        let base = default_pin_style(&iced::Theme::Dark, PinStatus::Idle);
        let style = PinStyle {
            radius: 10.0,
            shape: PinShape::Square,
            ..base.clone()
        };

        assert_eq!(style.radius, 10.0); // override wins
        assert_eq!(style.shape, PinShape::Square); // override wins
        assert_eq!(style.color, base.color); // inherited from default
    }
}
