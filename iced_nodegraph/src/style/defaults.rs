//! Built-in theme-driven default styles.
//!
//! Each `default_*_style` is the library's default style closure: it translates
//! the iced [`Theme`] palette into a *complete* [`Partial`] overlay (every field
//! set) and layers the status feedback (selection border, pending-cut tint) on
//! top. Because the overlay is complete, [`resolve`](crate::style::Partial) can
//! finalize it without a base, and user closures layer their overrides on top:
//!
//! ```ignore
//! node.style(|theme, status| {
//!     NodeStyle::new()
//!         .fill_color(Color::WHITE)         // user override wins
//!         .merge(&default_node_style(theme, status)) // theme base + status fills the rest
//!         .resolve()                        // every field now set
//! })
//! ```
//!
//! The `resolved_*_style` wrappers are the effective default the widget draws
//! when no closure is set: the default overlay resolved directly.
//!
//! The valid-target pin pulse is time-based and stays in the widget, so
//! [`default_pin_style`] has no static `ValidTarget` feedback.

use iced::{Color, Theme};
use iced_sdf::Pattern;

use super::{
    EdgeCurve, EdgeStatus, EdgeStyle, NodeStatus, NodeStyle, Partial, PinShape, PinStatus,
    PinStyle, Resolved, SelectionStyle,
};

/// Complete theme-derived node style with status feedback layered on top:
/// `Idle` is the plain theme base; `Selected` swaps in the theme selection
/// border.
pub fn default_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle<Partial> {
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

    let base = NodeStyle::new()
        .fill_color(node_fill)
        .corner_radius(5.0)
        .opacity(opacity)
        .border_color(node_border)
        .border_pattern(Pattern::solid(1.0))
        .border_outline_width(0.0)
        .border_outline_color(Color::TRANSPARENT)
        .shadow_color(shadow_color)
        .shadow_distance(shadow_distance)
        .shadow_offset((2.0, 2.0));

    match status {
        NodeStatus::Idle => base,
        NodeStatus::Selected => {
            let sel = SelectionStyle::from_theme(theme);
            base.border_color(sel.selected_border_color)
                .border_pattern(Pattern::solid(sel.selected_border_width))
        }
    }
}

/// Complete theme-derived pin style. The valid-target pulse is time-based and
/// applied by the widget, so both states share the same base.
pub fn default_pin_style(theme: &Theme, _status: PinStatus) -> PinStyle<Partial> {
    let palette = theme.extended_palette();
    let secondary = palette.secondary.base.color;
    let text = palette.background.base.text;

    if palette.is_dark {
        PinStyle::new()
            .color(Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.7))
            .radius(6.0)
            .shape(PinShape::Circle)
            .border_color(Color::TRANSPARENT)
            .border_width(0.0)
    } else {
        PinStyle::new()
            .color(Color::from_rgba(
                secondary.r * 0.7,
                secondary.g * 0.7,
                secondary.b * 0.7,
                0.8,
            ))
            .radius(6.0)
            .shape(PinShape::Circle)
            .border_color(Color::from_rgba(text.r, text.g, text.b, 0.3))
            .border_width(1.0)
    }
}

/// Complete theme-derived edge style with status feedback: `Idle` is a 2px solid
/// stroke inheriting the pin colors; `PendingCut` tints the stroke with the
/// theme's edge-cutting color.
pub fn default_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle<Partial> {
    // TRANSPARENT stroke ends mean "inherit from the connected pins".
    let base = EdgeStyle::new()
        .stroke_color(Color::TRANSPARENT)
        .pattern(Pattern::solid(2.0))
        .stroke_outline_width(0.0)
        .stroke_outline_color(Color::TRANSPARENT)
        .border_color(Color::TRANSPARENT)
        .border_width(0.0)
        .border_gap(0.5)
        .border_outline_width(0.0)
        .border_outline_color(Color::TRANSPARENT)
        .border_background(Color::TRANSPARENT)
        .shadow_color(Color::TRANSPARENT)
        .shadow_expand(0.0)
        .shadow_blur(0.0)
        .shadow_offset((0.0, 0.0))
        .curve(EdgeCurve::BezierCubic);

    match status {
        EdgeStatus::Idle => base,
        EdgeStatus::PendingCut => {
            let sel = SelectionStyle::from_theme(theme);
            base.stroke_color(sel.edge_cutting_color)
        }
    }
}

/// Effective default node style: the complete default overlay, resolved.
pub fn resolved_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle<Resolved> {
    default_node_style(theme, status).resolve()
}

/// Effective default pin style: the complete default overlay, resolved.
pub fn resolved_pin_style(theme: &Theme, status: PinStatus) -> PinStyle<Resolved> {
    default_pin_style(theme, status).resolve()
}

/// Effective default edge style: the complete default overlay, resolved.
pub fn resolved_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle<Resolved> {
    default_edge_style(theme, status).resolve()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_complete() {
        // Every default overlay must set every field, else `resolve()` panics.
        // Cover both palette branches (dark/light) and all status arms, since a
        // field set only in one branch or arm would slip past a single case.
        for t in [Theme::Dark, Theme::Light] {
            let _ = default_node_style(&t, NodeStatus::Idle).resolve();
            let _ = default_node_style(&t, NodeStatus::Selected).resolve();
            let _ = default_pin_style(&t, PinStatus::Idle).resolve();
            let _ = default_pin_style(&t, PinStatus::ValidTarget).resolve();
            let _ = default_edge_style(&t, EdgeStatus::Idle).resolve();
            let _ = default_edge_style(&t, EdgeStatus::PendingCut).resolve();
        }
    }

    #[test]
    fn selected_node_sets_border() {
        let t = Theme::Dark;
        let sel = SelectionStyle::from_theme(&t);
        let o = default_node_style(&t, NodeStatus::Selected);
        assert_eq!(o.border_color, Some(sel.selected_border_color.into()));
        assert_eq!(
            o.border_pattern.map(|p| p.thickness),
            Some(sel.selected_border_width)
        );
    }

    #[test]
    fn pending_cut_tints_stroke() {
        let t = Theme::Dark;
        let sel = SelectionStyle::from_theme(&t);
        let o = default_edge_style(&t, EdgeStatus::PendingCut);
        assert_eq!(o.stroke_color, Some(sel.edge_cutting_color.into()));
    }
}
