//! Built-in theme-driven default styles.
//!
//! Each `default_*_style` translates the iced [`Theme`] palette into one
//! complete, concrete style. It is both what the widget draws when no closure is
//! set, and the base a user closure overrides via struct-update - so a host
//! closure can always reach everything the default reaches.
//!
//! The per-element defaults take a status and express its feedback in full: a
//! selected node is not just a recolored border (see [`default_node_style`]), and
//! an edge marked for cutting takes the cutting tool's own color. The two
//! overlay defaults ([`default_selection_box_style`],
//! [`default_cutting_tool_style`]) have no status - the overlay exists only while
//! its gesture is running.
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

use iced_nodegraph_sdf::Pattern;
use iced_widget::core::{Color, Theme};

use super::{
    CuttingToolStyle, EdgeCurve, EdgeStatus, EdgeStyle, NodeStatus, NodeStyle, PinShape, PinStatus,
    PinStyle, SelectionBoxStyle,
};

/// Complete theme-derived node style, with the selected look expressed in full
/// rather than as a border tweak.
///
/// A selected node reads as *brought forward*: the accent border gains a
/// translucent accent halo, the body goes fully opaque, and the drop shadow
/// deepens - reinforcing the z-promotion the widget already applies. All four
/// are style-level (color bands and a stop chain), so switching selection does
/// not touch node geometry or the shape cache.
///
/// Override any of it by returning your own [`NodeStyle`] from the node's
/// `style` closure; the closure receives the [`NodeStatus`], so a host is not
/// limited to recoloring the border.
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
            // iced reserves `primary` for active/selected affordances; the base
            // border deliberately stays on the neutral background ramp so the
            // accent means exactly one thing here.
            let accent = palette.primary.base.color;
            NodeStyle {
                border_color: accent.into(),
                border_pattern: Pattern::solid(2.0),
                // An outward band on the silhouette, so the halo reads at any
                // zoom without moving the outline.
                border_outline_width: 3.0,
                border_outline_color: Color { a: 0.35, ..accent }.into(),
                opacity: 1.0,
                shadow_distance: shadow_distance * 1.5,
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
        EdgeStatus::PendingCut => EdgeStyle {
            // An edge marked for cutting takes the cutting tool's own color, so
            // the trail and its victims read as one gesture.
            stroke_color: default_cutting_tool_style(theme).color.into(),
            ..base
        },
    }
}

/// Theme-derived style of the selection box.
///
/// The accent hue at two alphas: a translucent wash so nodes stay legible
/// underneath, and an opaque-enough outline to read against both.
pub fn default_selection_box_style(theme: &Theme) -> SelectionBoxStyle {
    let accent = theme.extended_palette().primary.base.color;
    SelectionBoxStyle {
        fill: Color { a: 0.15, ..accent },
        border_color: Color { a: 0.6, ..accent },
        border_width: 1.5,
    }
}

/// Theme-derived style of the edge-cutting trail.
///
/// Cutting is destructive, so it paints in the theme's `danger` color rather
/// than the accent.
pub fn default_cutting_tool_style(theme: &Theme) -> CuttingToolStyle {
    CuttingToolStyle {
        color: theme.extended_palette().danger.base.color,
        width: 3.0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ColorQuad;
    use super::*;

    /// Selection is expressed on four independent channels, not just the border,
    /// and every one of them is style-level: switching selection must not cost a
    /// geometry rebuild.
    #[test]
    fn selected_node_reads_as_brought_forward() {
        let t = Theme::Dark;
        let accent = t.extended_palette().primary.base.color;
        let idle = default_node_style(&t, NodeStatus::Idle);
        let sel = default_node_style(&t, NodeStatus::Selected);

        assert_eq!(sel.border_color, accent.into(), "border takes the accent");
        assert!(
            sel.border_outline_width > 0.0,
            "a halo ring distinguishes selection at any zoom"
        );
        assert_eq!(sel.opacity, 1.0, "a selected node is fully opaque");
        assert!(
            sel.shadow_distance > idle.shadow_distance,
            "the shadow deepens, reinforcing the z-promotion"
        );

        // Geometry-affecting fields must match, or the shape cache gains an
        // entry per selection state.
        assert_eq!(sel.corner_radius, idle.corner_radius);
        assert_eq!(sel.shadow_offset, idle.shadow_offset);
    }

    /// An edge marked for cutting must take the cutting tool's own color, so the
    /// trail and the edges it will destroy read as one gesture.
    #[test]
    fn pending_cut_tints_stroke_with_the_cutting_tool_color() {
        let t = Theme::Dark;
        let o = default_edge_style(&t, EdgeStatus::PendingCut);
        assert_eq!(
            o.stroke_color,
            ColorQuad::solid(default_cutting_tool_style(&t).color)
        );
    }
}
