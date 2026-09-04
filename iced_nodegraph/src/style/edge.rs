//! `EdgeStyle`: per-edge visual style.
//!
//! A flat, concrete struct the renderer consumes directly. See [`super::node`]
//! for the override-via-struct-update pattern over [`default_edge_style`](crate::default_edge_style).
//! Stroke, border ring and shadow are flat field groups rather than nested
//! structs, so a single struct-update expression can reach any one of them.
//!
//! Color fields are [`ColorQuad`]s. The stroke `color` is an arc-length gradient
//! start -> end. To make an edge follow its connected pins' colors, derive the
//! quad from each endpoint's [`PinInfo`](crate::PinInfo) in the edge `style`
//! closure; the style itself carries only concrete colors. The shadow uses all
//! four quad corners: arc gradient along the edge crossed with the distance fade
//! to transparent. On/off is a sentinel: border `width` 0, stroke/border outline
//! `width` 0, shadow `blur` 0 or color alpha 0.
//!
use iced_nodegraph_sdf::Pattern;
use iced_widget::core::{Color, Theme};

use super::defaults::default_edge_style;
use super::roles::Roles;
use super::{ColorQuad, EdgeCurve, EdgeStatus, ramp};

/// Visual style for an edge.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeStyle {
    // Stroke (stroke_color: arc gradient start -> end)
    /// Stroke color as an arc-length gradient (start pin -> end pin).
    pub stroke_color: ColorQuad,
    /// Stroke pattern (thickness, dash/gap, flow).
    pub pattern: Pattern,
    /// Outline ring width on the stroke. 0 = no outline.
    pub stroke_outline_width: f32,
    /// Outline ring color on the stroke.
    pub stroke_outline_color: ColorQuad,

    // Border ring (width 0 = no border)
    /// Border ring color (arc gradient start -> end).
    pub border_color: ColorQuad,
    /// Border ring width. 0 = no border.
    pub border_width: f32,
    /// Gap between stroke and border ring.
    pub border_gap: f32,
    /// Outline ring width on the border. 0 = no outline.
    pub border_outline_width: f32,
    /// Outline ring color on the border.
    pub border_outline_color: ColorQuad,
    /// Background fill color for the border gap (arc gradient).
    pub border_background: ColorQuad,

    // Shadow (quad fades to transparent over distance; blur 0 / alpha 0 = none)
    /// Shadow color: arc gradient along the edge, faded over distance.
    pub shadow_color: ColorQuad,
    /// Expand the shadow band beyond the stroke.
    pub shadow_expand: f32,
    /// Shadow blur (distance fade) in world-space pixels. 0 = no shadow.
    pub shadow_blur: f32,
    /// Shadow offset in world-space pixels (x, y).
    pub shadow_offset: (f32, f32),

    // Path
    /// Curve shape of the connection.
    pub curve: EdgeCurve,
}

impl EdgeStyle {
    /// Data-flow preset: a slightly heavier stroke in the theme's `primary`.
    /// Use it as a style directly: `.style(|t, s, _, _| EdgeStyle::data_flow(t, s))`.
    pub fn data_flow(theme: &Theme, status: EdgeStatus) -> Self {
        let roles = Roles::of(theme);
        let hue = roles.legible(theme.extended_palette().primary.base.color);
        Self::stroked(theme, status, hue, Pattern::solid(2.5))
    }

    /// Error preset: marching ants in the theme's `danger` with a border ring
    /// of the same hue.
    pub fn error(theme: &Theme, status: EdgeStatus) -> Self {
        let hue = Roles::of(theme).danger;
        let mut s = Self::stroked(
            theme,
            status,
            hue,
            Pattern::dashed(2.0, 6.0, 4.0).flow(30.0),
        );
        s.border_color = hue.into();
        s.border_width = 1.0;
        s.border_gap = 0.5;
        s
    }

    /// Disabled preset: a thin dash halfway between the canvas and the wire
    /// color, so it recedes without vanishing.
    pub fn disabled(theme: &Theme, status: EdgeStatus) -> Self {
        let roles = Roles::of(theme);
        let hue = ramp::blend(roles.canvas, roles.wire, 0.5);
        Self::stroked(theme, status, hue, Pattern::dashed(1.5, 12.0, 6.0))
    }

    /// Highlighted preset: a wide stroke in the theme's `warning` with a soft
    /// ring in the pin color.
    pub fn highlighted(theme: &Theme, status: EdgeStatus) -> Self {
        let roles = Roles::of(theme);
        let hue = roles.legible(theme.extended_palette().warning.base.color);
        let mut s = Self::stroked(theme, status, hue, Pattern::solid(3.0));
        s.border_color = Color {
            a: 0.3,
            ..roles.terminal
        }
        .into();
        s.border_width = 2.0;
        s.border_gap = 1.0;
        s
    }

    /// Debug preset: the wire color dotted along a straight line.
    pub fn debug(theme: &Theme, status: EdgeStatus) -> Self {
        let hue = Roles::of(theme).wire;
        let mut s = Self::stroked(theme, status, hue, Pattern::dotted(8.0, 2.0));
        s.curve = EdgeCurve::Line;
        s
    }

    /// [`default_edge_style`] with the idle stroke replaced by `hue` and
    /// `pattern`. An edge marked for cutting keeps the default's `danger`
    /// stroke, so the cutting feedback wins over the preset's hue.
    fn stroked(theme: &Theme, status: EdgeStatus, hue: Color, pattern: Pattern) -> Self {
        let base = default_edge_style(theme, status);
        let stroke_color = match status {
            EdgeStatus::Idle => ColorQuad::solid(hue),
            EdgeStatus::PendingCut => base.stroke_color,
        };
        Self {
            stroke_color,
            pattern,
            ..base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_widget::core::Theme;

    #[test]
    fn struct_update_overrides_over_default() {
        use crate::style::{EdgeStatus, default_edge_style};
        let base = default_edge_style(&Theme::Dark, EdgeStatus::Idle);
        let style = EdgeStyle {
            border_width: 2.0,
            curve: EdgeCurve::Line,
            ..base
        };

        assert_eq!(style.border_width, 2.0); // override wins
        assert_eq!(style.curve, EdgeCurve::Line); // override wins
        assert_eq!(style.pattern, Pattern::solid(2.0)); // inherited from default
    }

    #[test]
    fn sdf_layers_preserves_stroke_pattern() {
        let mut s = EdgeStyle::data_flow(&Theme::Dark, EdgeStatus::Idle);
        s.pattern = Pattern::dashed(2.0, 12.0, 6.0);
        let layers = s.sdf_layers();
        let stroke = &layers[0]; // stroke is the front layer
        let pat = stroke.style.pattern.expect("stroke lost its pattern");
        assert!(
            matches!(
                pat.pattern_type,
                iced_nodegraph_sdf::pattern::PatternType::Dashed { .. }
            ),
            "stroke pattern is not Dashed: {:?}",
            pat.pattern_type
        );
    }
}
