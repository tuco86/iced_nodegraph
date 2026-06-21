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

    // A node is a raised surface over the canvas. iced fills its container
    // surfaces (`container::rounded_box`) with `background.weak` and draws
    // dividers/borders (`rule`, the slider rail) one ramp step up at
    // `background.strong`. We follow that: a neutral background-ramp border,
    // not a primary tint - the accent is reserved for the selection border,
    // exactly as iced reserves `primary` for active/selected affordances. The
    // ramp is perceptual (oklch) and self-adapts to dark/light, so no hand mix.
    let fill = palette.background.weak.color;
    let border = palette.background.strong.color;

    // Opacity and shadow are genuinely light/dark dependent (a black shadow
    // reads differently against a dark canvas), not theme-hue mappings.
    let (opacity, shadow_color, shadow_distance) = if palette.is_dark {
        (0.75, Color::from_rgba(0.0, 0.0, 0.0, 0.3), 4.0)
    } else {
        (0.85, Color::from_rgba(0.0, 0.0, 0.0, 0.22), 6.0)
    };

    let base = NodeStyle {
        fill_color: fill.into(),
        corner_radius: 5.0,
        opacity,
        border_color: border.into(),
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

    // Pins are the node graph's interactive marks - the role iced gives to
    // slider handles and radio dots, which all paint in `primary`. A filled dot
    // needs no border (the slider handle is borderless too); the palette accent
    // adapts to dark/light on its own, so no per-theme channel scaling.
    PinStyle {
        color: palette.primary.base.color.into(),
        radius: 6.0,
        shape: PinShape::Circle,
        border_color: Color::TRANSPARENT.into(),
        border_width: 0.0,
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
