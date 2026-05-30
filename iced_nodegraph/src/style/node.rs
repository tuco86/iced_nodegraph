//! `NodeStyle`: per-node visual style.
//!
//! Written as a flat, concrete struct and expanded by [`#[style]`](style) into
//! the typestate form: `NodeStyle<Partial>` (user overlay, `Option` per field,
//! `None` = inherit) and `NodeStyle<Resolved>` (renderer form, concrete per
//! field). The macro also generates `Clone`/`Debug`/`PartialEq`, `Default` for
//! the overlay, builder setters, and `merge`/`resolve`/`merge_theme`. Only
//! `from_theme` (palette logic) stays hand-written below.
//!
//! On/off is encoded by sentinels (border thickness 0, shadow blur/alpha 0), so
//! every field is a plain value and inheritance is per-field, never
//! `Option<Option<T>>`.
//!
use iced::{Color, Theme};
use iced_nodegraph_macros::style;
use iced_sdf::Pattern;

use super::color::ColorQuad;
use super::mode::{Partial, Resolved, StyleMode};

/// Visual style for a node.
///
/// Color fields are [`ColorQuad`]s (the four iced_sdf corners); a plain `Color`
/// coerces to a solid quad, so simple cases stay one-liners.
#[style]
pub struct NodeStyle {
    // Body
    /// Fill color of the node body.
    pub fill_color: ColorQuad,
    /// Corner radius in world-space pixels.
    pub corner_radius: f32,
    /// Body opacity (0.0 to 1.0).
    pub opacity: f32,

    // Border (pattern thickness 0 = no border)
    /// Border color.
    pub border_color: ColorQuad,
    /// Border stroke pattern (thickness, dash/gap, flow). Thickness 0 = none.
    pub border_pattern: Pattern,
    /// Outline ring width around the border. 0 = no outline.
    pub border_outline_width: f32,
    /// Outline ring color.
    pub border_outline_color: ColorQuad,

    // Shadow (distance gradient near -> far via shadow_color; fade the far alpha
    // to 0 for a soft edge. Near alpha 0 = no shadow.)
    /// Shadow color as a distance gradient: inner (near) -> outer (far).
    pub shadow_color: ColorQuad,
    /// Gradient distance from inner to outer edge in world-space pixels.
    pub shadow_distance: f32,
    /// Shadow offset in world-space pixels (x, y).
    pub shadow_offset: (f32, f32),
}

impl NodeStyle<Resolved> {
    /// Theme-derived base style. Ports the legacy `NodeStyle::from_theme`
    /// palette logic onto the flat fields.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let bg_weak = palette.background.weak.color;

        if palette.is_dark {
            let node_fill = Color::from_rgba(
                bg.r + (bg_weak.r - bg.r) * 0.3,
                bg.g + (bg_weak.g - bg.g) * 0.3,
                bg.b + (bg_weak.b - bg.b) * 0.3,
                1.0,
            );
            let node_border =
                Color::from_rgba(bg_weak.r * 1.2, bg_weak.g * 1.2, bg_weak.b * 1.2, 0.8);
            Self {
                fill_color: ColorQuad::solid(node_fill),
                corner_radius: 5.0,
                opacity: 0.75,
                border_color: ColorQuad::solid(node_border),
                border_pattern: Pattern::solid(1.0),
                border_outline_width: 0.0,
                border_outline_color: ColorQuad::solid(Color::TRANSPARENT),
                shadow_color: ColorQuad::fade(Color::from_rgba(0.0, 0.0, 0.0, 0.15)),
                shadow_distance: 4.0,
                shadow_offset: (2.0, 2.0),
            }
        } else {
            let node_fill = Color::from_rgba(
                bg.r - (bg.r - bg_weak.r) * 0.15,
                bg.g - (bg.g - bg_weak.g) * 0.15,
                bg.b - (bg.b - bg_weak.b) * 0.15,
                1.0,
            );
            let node_border =
                Color::from_rgba(bg_weak.r * 0.9, bg_weak.g * 0.9, bg_weak.b * 0.9, 0.9);
            Self {
                fill_color: ColorQuad::solid(node_fill),
                corner_radius: 5.0,
                opacity: 0.85,
                border_color: ColorQuad::solid(node_border),
                border_pattern: Pattern::solid(1.0),
                border_outline_width: 0.0,
                border_outline_color: ColorQuad::solid(Color::TRANSPARENT),
                shadow_color: ColorQuad::fade(Color::from_rgba(0.0, 0.0, 0.0, 0.12)),
                shadow_distance: 6.0,
                shadow_offset: (2.0, 2.0),
            }
        }
    }

    /// Input node preset (blue tint).
    pub fn input() -> Self {
        Self::preset(
            Color::from_rgb(0.15, 0.20, 0.30),
            Color::from_rgb(0.30, 0.45, 0.70),
            1.5,
            6.0,
            0.85,
            Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            8.0,
            (4.0, 4.0),
        )
    }

    /// Process node preset (green tint).
    pub fn process() -> Self {
        Self::preset(
            Color::from_rgb(0.18, 0.28, 0.18),
            Color::from_rgb(0.35, 0.60, 0.35),
            1.5,
            4.0,
            0.80,
            Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            8.0,
            (4.0, 4.0),
        )
    }

    /// Output node preset (orange tint).
    pub fn output() -> Self {
        Self::preset(
            Color::from_rgb(0.30, 0.22, 0.15),
            Color::from_rgb(0.75, 0.55, 0.30),
            2.0,
            8.0,
            0.85,
            Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            16.0,
            (6.0, 8.0),
        )
    }

    /// Comment node preset (subtle gray, no shadow).
    pub fn comment() -> Self {
        Self::preset(
            Color::from_rgba(0.20, 0.20, 0.22, 0.5),
            Color::from_rgba(0.40, 0.40, 0.44, 0.5),
            1.0,
            3.0,
            0.60,
            Color::TRANSPARENT,
            0.0,
            (0.0, 0.0),
        )
    }

    /// Builds a resolved node style from solid fill/border colors plus shadow.
    #[allow(clippy::too_many_arguments)]
    fn preset(
        fill: Color,
        border: Color,
        border_width: f32,
        corner_radius: f32,
        opacity: f32,
        shadow: Color,
        shadow_distance: f32,
        shadow_offset: (f32, f32),
    ) -> Self {
        Self {
            fill_color: ColorQuad::solid(fill),
            corner_radius,
            opacity,
            border_color: ColorQuad::solid(border),
            border_pattern: Pattern::solid(border_width),
            border_outline_width: 0.0,
            border_outline_color: ColorQuad::solid(Color::TRANSPARENT),
            shadow_color: ColorQuad::fade(shadow),
            shadow_distance,
            shadow_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_resolves_over_theme_base() {
        let base = NodeStyle::<Resolved>::from_theme(&Theme::Dark);
        // Color coerces to a solid ColorQuad via the `impl Into` setter.
        let overlay = NodeStyle::new().fill_color(Color::WHITE).opacity(1.0);
        let resolved = overlay.resolve(&base);

        assert_eq!(resolved.fill_color, ColorQuad::solid(Color::WHITE)); // overridden
        assert_eq!(resolved.opacity, 1.0); // overridden
        assert_eq!(resolved.corner_radius, base.corner_radius); // inherited
        assert_eq!(resolved.border_pattern, base.border_pattern); // inherited
    }

    #[test]
    fn merge_prefers_self() {
        let a = NodeStyle::new().fill_color(Color::WHITE);
        let b = NodeStyle::new().fill_color(Color::BLACK).border_outline_width(2.0);
        let m = a.merge(&b);

        assert_eq!(m.fill_color, Some(ColorQuad::solid(Color::WHITE))); // self wins
        assert_eq!(m.border_outline_width, Some(2.0)); // filled from other
    }
}
