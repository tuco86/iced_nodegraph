//! `NodeStyle`: per-node visual style.
//!
//! A flat, concrete struct the renderer consumes directly. The theme-derived
//! base lives in [`default_node_style`](crate::default_node_style); override
//! individual fields with struct-update syntax over it:
//!
//! ```rust
//! use iced::{Color, Theme};
//! use iced_nodegraph::{NodeStatus, NodeStyle, default_node_style};
//!
//! # let (theme, status) = (&Theme::Dark, NodeStatus::Idle);
//! NodeStyle { fill_color: Color::WHITE.into(), ..default_node_style(theme, status) };
//! ```
//!
//! On/off is encoded by sentinels (border thickness 0, shadow blur/alpha 0), so
//! every field is a plain value.
//!
use iced_nodegraph_sdf::Pattern;
use iced_widget::core::{Color, Theme};

use super::defaults::default_node_style;
use super::roles::Roles;
use super::{ColorQuad, NodeStatus, ramp};

/// How far a preset pulls the body toward its hue: a shade more than the
/// selection tint, so a preset still reads as its kind next to a selected
/// neighbour, and small enough that hosted content stays readable on it.
const PRESET_TINT: f32 = 0.18;

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
    /// Input node preset: [`default_node_style`] tinted toward the theme's
    /// `primary`. Use it as a style directly: `.style(NodeStyle::input)`.
    pub fn input(theme: &Theme, status: NodeStatus) -> Self {
        Self::tinted(theme, status, theme.extended_palette().primary.base.color)
    }

    /// Process node preset: [`default_node_style`] tinted toward the theme's
    /// `success`.
    pub fn process(theme: &Theme, status: NodeStatus) -> Self {
        Self::tinted(theme, status, theme.extended_palette().success.base.color)
    }

    /// Output node preset: [`default_node_style`] tinted toward the theme's
    /// `warning`.
    pub fn output(theme: &Theme, status: NodeStatus) -> Self {
        Self::tinted(theme, status, theme.extended_palette().warning.base.color)
    }

    /// Comment node preset: a translucent body flush with the canvas, a
    /// dashed border and no shadow, so it reads as an annotation rather than
    /// an object. Selection feedback comes through as in the default.
    pub fn comment(theme: &Theme, status: NodeStatus) -> Self {
        let roles = Roles::of(theme);
        let base = default_node_style(theme, status);
        let border_pattern = match status {
            NodeStatus::Idle => Pattern::dashed(1.0, 6.0, 4.0),
            NodeStatus::Selected => Pattern::dashed(2.0, 6.0, 4.0),
        };
        Self {
            fill_color: Color {
                a: 0.6,
                ..roles.canvas
            }
            .into(),
            border_pattern,
            shadow_color: Color::TRANSPARENT,
            shadow_distance: 0.0,
            shadow_offset: (0.0, 0.0),
            ..base
        }
    }

    /// The default with its body and idle border pulled toward `hue`. A
    /// selected node keeps the default's accent border and halo, so the
    /// preset colors the kind and the theme colors the selection.
    fn tinted(theme: &Theme, status: NodeStatus, hue: Color) -> Self {
        let roles = Roles::of(theme);
        let hue = roles.legible(hue);
        let base = default_node_style(theme, status);
        let fill_color = ColorQuad::solid(ramp::blend(roles.body, hue, PRESET_TINT));
        match status {
            NodeStatus::Idle => Self {
                fill_color,
                border_color: hue.into(),
                ..base
            },
            NodeStatus::Selected => Self { fill_color, ..base },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_widget::core::Theme;

    #[test]
    fn struct_update_overrides_over_default() {
        use crate::style::{NodeStatus, default_node_style};
        let base = default_node_style(&Theme::Dark, NodeStatus::Idle);
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
