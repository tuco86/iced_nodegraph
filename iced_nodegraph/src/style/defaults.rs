//! Built-in status-driven default styles.
//!
//! Each `default_*_style` returns a [`Partial`] overlay carrying only the
//! status feedback (selection border, pending-cut tint); `Idle` is an empty
//! overlay that inherits the theme base. They are both the library fallback the
//! widget uses when an element has no `.style()` closure, and the building block
//! user closures layer on:
//!
//! ```ignore
//! node.style(|theme, status| {
//!     default_node_style(theme, status)        // status feedback (Partial)
//!         .fill_color(Color::WHITE)            // user override, same builder
//!         .resolve(&NodeStyle::from_theme(theme))
//! })
//! ```
//!
//! The `resolved_*_style` wrappers are the effective default the widget draws
//! when no closure is set: the status overlay resolved over the theme base.
//!
//! The valid-target pin pulse is time-based and stays in the widget, so
//! [`default_pin_style`] has no static `ValidTarget` feedback.

use iced::Theme;
use iced_sdf::Pattern;

use super::{
    EdgeStatus, EdgeStyle, NodeStatus, NodeStyle, Partial, PinStatus, PinStyle, Resolved,
    SelectionStyle,
};

/// Status overlay for a node: `Idle` inherits the theme base; `Selected`
/// applies the theme selection border.
pub fn default_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle<Partial> {
    match status {
        NodeStatus::Idle => NodeStyle::new(),
        NodeStatus::Selected => {
            let sel = SelectionStyle::from_theme(theme);
            NodeStyle::new()
                .border_color(sel.selected_border_color)
                .border_pattern(Pattern::solid(sel.selected_border_width))
        }
    }
}

/// Status overlay for a pin. The valid-target pulse is time-based and applied by
/// the widget, so both states inherit the theme base.
pub fn default_pin_style(_theme: &Theme, _status: PinStatus) -> PinStyle<Partial> {
    PinStyle::new()
}

/// Status overlay for an edge: `Idle` inherits the theme base; `PendingCut`
/// tints the stroke with the theme's edge-cutting color.
pub fn default_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle<Partial> {
    match status {
        EdgeStatus::Idle => EdgeStyle::new(),
        EdgeStatus::PendingCut => {
            let sel = SelectionStyle::from_theme(theme);
            EdgeStyle::new().stroke_color(sel.edge_cutting_color)
        }
    }
}

/// Effective default node style: status overlay resolved over the theme base.
pub fn resolved_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle<Resolved> {
    default_node_style(theme, status).resolve(&NodeStyle::from_theme(theme))
}

/// Effective default pin style: status overlay resolved over the theme base.
pub fn resolved_pin_style(theme: &Theme, status: PinStatus) -> PinStyle<Resolved> {
    default_pin_style(theme, status).resolve(&PinStyle::from_theme(theme))
}

/// Effective default edge style: status overlay resolved over the theme base.
pub fn resolved_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle<Resolved> {
    default_edge_style(theme, status).resolve(&EdgeStyle::from_theme(theme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_overlays_are_empty() {
        let t = Theme::Dark;
        assert_eq!(default_node_style(&t, NodeStatus::Idle), NodeStyle::new());
        assert_eq!(default_pin_style(&t, PinStatus::Idle), PinStyle::new());
        assert_eq!(default_edge_style(&t, EdgeStatus::Idle), EdgeStyle::new());
    }

    #[test]
    fn selected_node_sets_border() {
        let t = Theme::Dark;
        let sel = SelectionStyle::from_theme(&t);
        let o = default_node_style(&t, NodeStatus::Selected);
        assert!(o.border_color.is_some());
        assert_eq!(
            o.border_pattern.map(|p| p.thickness),
            Some(sel.selected_border_width)
        );
    }

    #[test]
    fn pending_cut_tints_stroke() {
        let t = Theme::Dark;
        let o = default_edge_style(&t, EdgeStatus::PendingCut);
        assert!(o.stroke_color.is_some());
    }

    #[test]
    fn resolved_idle_matches_theme_base() {
        let t = Theme::Dark;
        assert_eq!(
            resolved_node_style(&t, NodeStatus::Idle),
            NodeStyle::from_theme(&t)
        );
    }
}
