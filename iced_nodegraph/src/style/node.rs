//! `NodeStyle`: per-node visual style.
//!
//! A flat, concrete struct the renderer consumes directly. The theme-derived
//! base lives in [`default_node_style`](crate::default_node_style); override
//! individual fields with struct-update syntax over it:
//!
//! ```ignore
//! NodeStyle { fill_color: Color::WHITE.into(), ..default_node_style(theme, status) }
//! ```
//!
//! On/off is encoded by sentinels (border thickness 0, shadow blur/alpha 0), so
//! every field is a plain value.
//!
use iced::Color;
use iced_nodegraph_sdf::Pattern;

use super::color::ColorQuad;

/// Visual style for a node.
///
/// Color fields are [`ColorQuad`]s (the four iced_nodegraph_sdf corners); a plain `Color`
/// coerces to a solid quad via `into()`.
#[derive(Debug, Clone, PartialEq)]
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
    // cutouts), offset by `shadow_offset`, as a single stop chain: full shadow
    // inside the silhouette, fading to nothing at `shadow_distance`. Only the
    // base color is user-facing; the chain derives its alpha from it. Alpha 0
    // or distance 0 = no shadow.
    /// Base shadow color. The widget modulates its alpha across the bands.
    pub shadow_color: Color,
    /// Blur half-width across the shape edge, in world-space pixels.
    pub shadow_distance: f32,
    /// Shadow offset in world-space pixels (x, y).
    pub shadow_offset: (f32, f32),
}

impl NodeStyle {
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

    #[test]
    fn struct_update_overrides_over_default() {
        use crate::style::{NodeStatus, default_node_style};
        let base = default_node_style(&iced::Theme::Dark, NodeStatus::Idle);
        // Color coerces to a solid ColorQuad via `into()`.
        let style = NodeStyle {
            fill_color: Color::WHITE.into(),
            opacity: 1.0,
            ..base
        };

        assert_eq!(style.fill_color, ColorQuad::solid(Color::WHITE)); // override wins
        assert_eq!(style.opacity, 1.0); // override wins
        assert_eq!(style.corner_radius, 5.0); // inherited from theme default
        assert_eq!(style.border_pattern, Pattern::solid(1.0)); // inherited
    }
}
