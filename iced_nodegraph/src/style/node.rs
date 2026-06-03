//! `NodeStyle`: per-node visual style.
//!
//! Written as a flat, concrete struct and expanded by [`#[style]`](style) into
//! the typestate form: `NodeStyle<Partial>` (user overlay, `Option` per field,
//! `None` = inherit) and `NodeStyle<Resolved>` (renderer form, concrete per
//! field). The macro also generates `Clone`/`Debug`/`PartialEq`, `Default` for
//! the overlay, builder setters, and `merge`/`resolve`. The theme-derived base
//! lives in [`default_node_style`](crate::default_node_style); only the named
//! presets stay hand-written below.
//!
//! On/off is encoded by sentinels (border thickness 0, shadow blur/alpha 0), so
//! every field is a plain value and inheritance is per-field, never
//! `Option<Option<T>>`.
//!
use iced::Color;
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

    // Shadow. The widget renders the node's real SDF silhouette (with pin
    // cutouts), offset by `shadow_offset`, as three distance bands: full shadow
    // inside, a soft ramp across the edge, fading to nothing outside. Only the
    // base color is user-facing; the bands derive their alpha from it. Alpha 0
    // or distance 0 = no shadow.
    /// Base shadow color. The widget modulates its alpha across the bands.
    pub shadow_color: Color,
    /// Blur half-width across the shape edge, in world-space pixels.
    pub shadow_distance: f32,
    /// Shadow offset in world-space pixels (x, y).
    pub shadow_offset: (f32, f32),
}

impl NodeStyle<Resolved> {
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
            shadow_color: shadow,
            shadow_distance,
            shadow_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Theme;

    #[test]
    fn overlay_merged_over_default_resolves() {
        use crate::style::{NodeStatus, default_node_style};
        // Color coerces to a solid ColorQuad via the `impl Into` setter.
        let overlay = NodeStyle::new().fill_color(Color::WHITE).opacity(1.0);
        let base = default_node_style(&Theme::Dark, NodeStatus::Idle);
        let resolved = overlay.merge(&base).resolve();

        assert_eq!(resolved.fill_color, ColorQuad::solid(Color::WHITE)); // overlay wins
        assert_eq!(resolved.opacity, 1.0); // overlay wins
        assert_eq!(resolved.corner_radius, 5.0); // inherited from theme default
        assert_eq!(resolved.border_pattern, Pattern::solid(1.0)); // inherited
    }

    #[test]
    fn merge_prefers_self() {
        let a = NodeStyle::new().fill_color(Color::WHITE);
        let b = NodeStyle::new()
            .fill_color(Color::BLACK)
            .border_outline_width(2.0);
        let m = a.merge(&b);

        assert_eq!(m.fill_color, Some(ColorQuad::solid(Color::WHITE))); // self wins
        assert_eq!(m.border_outline_width, Some(2.0)); // filled from other
    }
}
