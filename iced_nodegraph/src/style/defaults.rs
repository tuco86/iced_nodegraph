//! Built-in theme-driven default styles.
//!
//! Each `default_*_style` is the library's default style closure: it translates
//! the iced [`Theme`] palette into a complete, concrete style and layers the
//! status feedback (selection border, pending-cut tint) on top. It is both the
//! effective default the widget draws when no closure is set, and the base a
//! user closure overrides via struct-update:
//!
//! ```ignore
//! node.style(|theme, status| NodeStyle {
//!     fill_color: Color::WHITE.into(),      // user override wins
//!     ..default_node_style(theme, status)   // theme base + status fills the rest
//! })
//! ```
//!
//! The valid-target pin pulse is time-based and stays in the widget, so
//! [`default_pin_style`] has no static `ValidTarget` feedback.

use iced::{Color, Theme};
use iced_nodegraph_sdf::Pattern;

use super::{
    EdgeCurve, EdgeStatus, EdgeStyle, NodeStatus, NodeStyle, PinShape, PinStatus, PinStyle,
    SelectionStyle,
};

/// Complete theme-derived node style with status feedback layered on top:
/// `Idle` is the plain theme base; `Selected` swaps in the theme selection
/// border.
pub fn default_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle {
    let palette = theme.extended_palette();
    let bg = palette.background.base.color;
    let bg_weak = palette.background.weak.color;
    // Pull the theme's primary accent into the node so it reads as part of the
    // theme rather than neutral gray. The body keeps a faint tint; the border
    // carries most of the accent signal.
    let accent = palette.primary.base.color;

    // Linear color blend, t in [0, 1] from a toward b.
    let mix = |a: Color, b: Color, t: f32| {
        Color::from_rgb(
            a.r + (b.r - a.r) * t,
            a.g + (b.g - a.g) * t,
            a.b + (b.b - a.b) * t,
        )
    };

    /// Accent share mixed into the node body fill.
    const FILL_TINT: f32 = 0.08;
    /// Accent share mixed into the node border.
    const BORDER_TINT: f32 = 0.55;

    let (node_fill, node_border, opacity, shadow_color, shadow_distance) = if palette.is_dark {
        let neutral_fill = Color::from_rgb(
            bg.r + (bg_weak.r - bg.r) * 0.3,
            bg.g + (bg_weak.g - bg.g) * 0.3,
            bg.b + (bg_weak.b - bg.b) * 0.3,
        );
        let nb = mix(bg_weak, accent, BORDER_TINT);
        (
            mix(neutral_fill, accent, FILL_TINT),
            Color::from_rgba(nb.r, nb.g, nb.b, 0.85),
            0.75,
            Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            4.0,
        )
    } else {
        let neutral_fill = Color::from_rgb(
            bg.r - (bg.r - bg_weak.r) * 0.15,
            bg.g - (bg.g - bg_weak.g) * 0.15,
            bg.b - (bg.b - bg_weak.b) * 0.15,
        );
        let nb = mix(bg_weak, accent, BORDER_TINT);
        (
            mix(neutral_fill, accent, FILL_TINT),
            Color::from_rgba(nb.r, nb.g, nb.b, 0.9),
            0.85,
            Color::from_rgba(0.0, 0.0, 0.0, 0.22),
            6.0,
        )
    };

    let base = NodeStyle {
        fill_color: node_fill.into(),
        corner_radius: 5.0,
        opacity,
        border_color: node_border.into(),
        border_pattern: Pattern::solid(1.0),
        border_outline_width: 0.0,
        border_outline_color: Color::TRANSPARENT.into(),
        shadow_color,
        shadow_distance,
        shadow_offset: (2.0, 2.0),
    };

    match status {
        NodeStatus::Idle => base,
        NodeStatus::Selected => {
            let sel = SelectionStyle::from_theme(theme);
            NodeStyle {
                border_color: sel.selected_border_color.into(),
                border_pattern: Pattern::solid(sel.selected_border_width),
                ..base
            }
        }
    }
}

/// Complete theme-derived pin style. The valid-target pulse is time-based and
/// applied by the widget, so both states share the same base.
pub fn default_pin_style(theme: &Theme, _status: PinStatus) -> PinStyle {
    let palette = theme.extended_palette();
    let secondary = palette.secondary.base.color;
    let text = palette.background.base.text;

    if palette.is_dark {
        PinStyle {
            color: Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.7).into(),
            radius: 6.0,
            shape: PinShape::Circle,
            border_color: Color::TRANSPARENT.into(),
            border_width: 0.0,
        }
    } else {
        PinStyle {
            color: Color::from_rgba(secondary.r * 0.7, secondary.g * 0.7, secondary.b * 0.7, 0.8)
                .into(),
            radius: 6.0,
            shape: PinShape::Circle,
            border_color: Color::from_rgba(text.r, text.g, text.b, 0.3).into(),
            border_width: 1.0,
        }
    }
}

/// Complete theme-derived edge style with status feedback: `Idle` is a 2px solid
/// stroke in the theme's secondary color; `PendingCut` tints the stroke with the
/// theme's edge-cutting color.
///
/// The default stroke is a single concrete color. To make an edge follow its
/// connected pins (e.g. a port-typed color), build the gradient from each
/// endpoint's [`PinInfo`](crate::PinInfo) in the edge `style` closure and
/// struct-update over this base.
pub fn default_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle {
    let palette = theme.extended_palette();
    // Unused-color sentinel for the off fields (border, outlines, shadow).
    let none = Color::TRANSPARENT;
    let base = EdgeStyle {
        stroke_color: palette.secondary.base.color.into(),
        pattern: Pattern::solid(2.0),
        stroke_outline_width: 0.0,
        stroke_outline_color: none.into(),
        border_color: none.into(),
        border_width: 0.0,
        border_gap: 0.5,
        border_outline_width: 0.0,
        border_outline_color: none.into(),
        border_background: none.into(),
        shadow_color: none.into(),
        shadow_expand: 0.0,
        shadow_blur: 0.0,
        shadow_offset: (0.0, 0.0),
        curve: EdgeCurve::BezierCubic,
    };

    match status {
        EdgeStatus::Idle => base,
        EdgeStatus::PendingCut => {
            let sel = SelectionStyle::from_theme(theme);
            EdgeStyle {
                stroke_color: sel.edge_cutting_color.into(),
                ..base
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ColorQuad;
    use super::*;

    #[test]
    fn selected_node_sets_border() {
        let t = Theme::Dark;
        let sel = SelectionStyle::from_theme(&t);
        let o = default_node_style(&t, NodeStatus::Selected);
        assert_eq!(o.border_color, sel.selected_border_color.into());
        assert_eq!(o.border_pattern.thickness, sel.selected_border_width);
    }

    #[test]
    fn pending_cut_tints_stroke() {
        let t = Theme::Dark;
        let sel = SelectionStyle::from_theme(&t);
        let o = default_edge_style(&t, EdgeStatus::PendingCut);
        assert_eq!(o.stroke_color, ColorQuad::solid(sel.edge_cutting_color));
    }
}
