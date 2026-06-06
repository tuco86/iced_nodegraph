//! `EdgeStyle`: per-edge visual style.
//!
//! A flat, concrete struct the renderer consumes directly. See [`super::node`]
//! for the override-via-struct-update pattern over [`default_edge_style`](crate::default_edge_style).
//! The legacy `EdgeBorder`/`EdgeShadow` nested structs are flattened into
//! grouped fields here.
//!
//! Color fields are [`ColorQuad`]s. The stroke `color` is an arc-length gradient
//! start -> end. To make an edge follow its connected pins' colors, derive the
//! quad from each endpoint's [`PinInfo`](crate::PinInfo) in the edge `style`
//! closure; the style itself carries only concrete colors. The shadow uses all
//! four quad corners: arc gradient along the edge crossed with the distance fade
//! to transparent. On/off is a sentinel: border `width` 0, stroke/border outline
//! `width` 0, shadow `blur` 0 or color alpha 0.
//!
use iced::Color;
use iced_nodegraph_sdf::Pattern;

use super::EdgeCurve;
use super::color::ColorQuad;

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
    /// Plain stroke baseline: no outline, border, or shadow; bezier path.
    fn stroke(color: ColorQuad, pattern: Pattern) -> Self {
        let none = ColorQuad::solid(Color::TRANSPARENT);
        Self {
            stroke_color: color,
            pattern,
            stroke_outline_width: 0.0,
            stroke_outline_color: none,
            border_color: none,
            border_width: 0.0,
            border_gap: 0.5,
            border_outline_width: 0.0,
            border_outline_color: none,
            border_background: none,
            shadow_color: none,
            shadow_expand: 0.0,
            shadow_blur: 0.0,
            shadow_offset: (0.0, 0.0),
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Data flow preset (blue, bezier).
    pub fn data_flow() -> Self {
        Self::stroke(
            ColorQuad::solid(Color::from_rgb(0.3, 0.6, 1.0)),
            Pattern::solid(2.5),
        )
    }

    /// Error preset (red, marching ants, with border ring).
    pub fn error() -> Self {
        let red = Color::from_rgb(0.9, 0.2, 0.2);
        let mut s = Self::stroke(
            ColorQuad::solid(red),
            Pattern::dashed(2.0, 6.0, 4.0).flow(30.0),
        );
        s.border_color = ColorQuad::solid(red);
        s.border_width = 1.0;
        s.border_gap = 0.5;
        s
    }

    /// Disabled preset (gray, dashed).
    pub fn disabled() -> Self {
        Self::stroke(
            ColorQuad::solid(Color::from_rgb(0.5, 0.5, 0.5)),
            Pattern::dashed(1.5, 12.0, 6.0),
        )
    }

    /// Highlighted preset (bright yellow, with soft border ring).
    pub fn highlighted() -> Self {
        let yellow = Color::from_rgb(1.0, 0.8, 0.2);
        let mut s = Self::stroke(ColorQuad::solid(yellow), Pattern::solid(3.0));
        s.border_color = ColorQuad::solid(Color::from_rgba(1.0, 1.0, 1.0, 0.3));
        s.border_width = 2.0;
        s.border_gap = 1.0;
        s
    }

    /// Debug preset (dotted cyan, straight line).
    pub fn debug() -> Self {
        let mut s = Self::stroke(
            ColorQuad::solid(Color::from_rgb(0.0, 1.0, 1.0)),
            Pattern::dotted(8.0, 2.0),
        );
        s.curve = EdgeCurve::Line;
        s
    }

    /// Stroke width from the pattern.
    pub fn get_width(&self) -> f32 {
        self.pattern.thickness
    }

    /// Whether the stroke pattern is animated.
    pub fn has_motion(&self) -> bool {
        self.pattern.flow_speed.abs() > 0.001
    }

    /// Animation speed (0.0 if static).
    pub fn motion_speed(&self) -> f32 {
        self.pattern.flow_speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_update_overrides_over_default() {
        use crate::style::{EdgeStatus, default_edge_style};
        let base = default_edge_style(&iced::Theme::Dark, EdgeStatus::Idle);
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
        let mut s = EdgeStyle::data_flow();
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
