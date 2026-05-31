//! `PinStyle`: per-pin visual style.
//!
//! Flat, concrete struct expanded by [`#[style]`](style) into the typestate form
//! (`PinStyle<Partial>` overlay / `PinStyle<Resolved>` renderer form). See
//! [`super::node`] for the full pattern. Color fields are [`ColorQuad`]s; a plain
//! `Color` coerces to a solid quad. Border on/off is the `border_width` sentinel
//! (0 = no border).
//!
use iced::{Color, Theme};
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
    /// Theme-derived base style.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let secondary = palette.secondary.base.color;
        let text = palette.background.base.text;

        if palette.is_dark {
            Self {
                color: ColorQuad::solid(Color::from_rgba(
                    secondary.r,
                    secondary.g,
                    secondary.b,
                    0.7,
                )),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: ColorQuad::solid(Color::TRANSPARENT),
                border_width: 0.0,
            }
        } else {
            Self {
                color: ColorQuad::solid(Color::from_rgba(
                    secondary.r * 0.7,
                    secondary.g * 0.7,
                    secondary.b * 0.7,
                    0.8,
                )),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: ColorQuad::solid(Color::from_rgba(text.r, text.g, text.b, 0.3)),
                border_width: 1.0,
            }
        }
    }

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
    fn partial_resolves_over_theme_base() {
        let base = PinStyle::<Resolved>::from_theme(&Theme::Dark);
        let overlay = PinStyle::new().radius(10.0).shape(PinShape::Square);
        let resolved = overlay.resolve(&base);

        assert_eq!(resolved.radius, 10.0); // overridden
        assert_eq!(resolved.shape, PinShape::Square); // overridden
        assert_eq!(resolved.color, base.color); // inherited
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
