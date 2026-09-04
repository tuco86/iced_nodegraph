//! Style definitions for NodeGraph visual customization.
//!
//! Node, edge, and pin styles are flat, concrete structs ([`NodeStyle`],
//! [`EdgeStyle`], [`PinStyle`]): the fully populated form the renderer consumes.
//! The theme-derived defaults are [`default_node_style`], [`default_edge_style`]
//! and [`default_pin_style`]; override individual fields with struct-update
//! syntax over them. See [`ColorQuad`] for the unified color type.
//!
//! The graph's own chrome follows the same shape, one type per thing the widget
//! draws itself: [`GraphStyle`] for the canvas, [`SelectionBoxStyle`] for the
//! selection box, [`CuttingToolStyle`] for the edge-cutting trail.

use iced_widget::core::Color;

mod anchor;
mod defaults;
mod edge;
mod node;
mod pin;
mod ramp;
mod roles;
mod sdf;

pub use anchor::AnchorStyle;
pub use defaults::{
    default_anchor_style, default_cutting_tool_style, default_edge_style, default_graph_style,
    default_node_style, default_pin_style, default_selection_box_style,
};
pub use edge::EdgeStyle;
pub use node::NodeStyle;
pub use pin::PinStyle;

// `ColorQuad` lives in iced_nodegraph_sdf (the Style/Stop builders consume it
// directly); re-exported here so it stays part of this crate's style surface.
pub use iced_nodegraph_sdf::ColorQuad;

// SDF layer decomposition (crate-internal, used by the widget renderer).
pub(crate) use sdf::EdgeGeometry;

/// Shape of a pin indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinShape {
    /// Round indicator.
    #[default]
    Circle,
    /// Square indicator, sized to the area of the circle it replaces.
    Square,
}

/// Node status for styling purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeStatus {
    /// Normal state, not selected
    #[default]
    Idle,
    /// Node is part of the current selection
    Selected,
}

/// Pin status for styling purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinStatus {
    /// Normal state
    #[default]
    Idle,
    /// Pin is a valid drop target during edge dragging
    ValidTarget,
}

/// Edge status for styling purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeStatus {
    /// Normal state
    #[default]
    Idle,
    /// Edge is pending deletion (during edge cutting)
    PendingCut,
}

/// What an anchor is doing this frame, for its style closure to key off.
///
/// Hover and drop-target feedback only. An anchor cannot be selected: selection
/// rides on [`Node::selected`](crate::Node::selected) and an anchor has no such
/// builder, so there is no state a `Selected` variant could ever report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorStatus {
    /// Nothing is happening to it.
    #[default]
    Idle,
    /// The cursor is over the anchor's core.
    Hovered,
    /// A route drag is in flight and this anchor is one of the anchors it may
    /// attach to.
    ValidTarget,
}

/// Edge path curve type determining the shape of the connection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EdgeCurve {
    /// Smooth cubic bezier curve (default)
    #[default]
    BezierCubic,
    /// Direct straight line between pins
    Line,
}

/// The repeating pattern of a [`TilingBackground`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TilingKind {
    /// Rectangular grid lines.
    #[default]
    Grid,
    /// Array of dots.
    Dots,
    /// Equilateral triangle grid.
    Triangles,
    /// Regular hexagonal grid.
    Hex,
}

/// A tiling background (grid, dots, ...) drawn over the canvas
/// [`background_color`](GraphStyle::background_color), panning and zooming with
/// the camera and repeating infinitely across the viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilingBackground {
    /// Which repeating pattern to draw.
    pub kind: TilingKind,
    /// Cell pitch in world units (grid/triangle/hex line spacing, or dot spacing).
    pub spacing: f32,
    /// Line thickness for `Grid`/`Triangles`/`Hex`, or dot radius for `Dots`,
    /// in world units.
    pub thickness: f32,
    /// Pattern color.
    pub color: Color,
}

impl TilingBackground {
    /// Grid lines with the given spacing, line thickness and color.
    pub fn grid(spacing: f32, thickness: f32, color: Color) -> Self {
        Self {
            kind: TilingKind::Grid,
            spacing,
            thickness,
            color,
        }
    }

    /// Dot array with the given spacing, dot radius and color.
    pub fn dots(spacing: f32, radius: f32, color: Color) -> Self {
        Self {
            kind: TilingKind::Dots,
            spacing,
            thickness: radius,
            color,
        }
    }

    /// Equilateral triangle grid with the given edge spacing, thickness and color.
    pub fn triangles(spacing: f32, thickness: f32, color: Color) -> Self {
        Self {
            kind: TilingKind::Triangles,
            spacing,
            thickness,
            color,
        }
    }

    /// Hexagonal grid with the given flat-to-flat spacing, thickness and color.
    pub fn hex(spacing: f32, thickness: f32, color: Color) -> Self {
        Self {
            kind: TilingKind::Hex,
            spacing,
            thickness,
            color,
        }
    }
}

/// The canvas: background color and the optional tiling drawn over it.
///
/// The theme-derived base is [`default_graph_style`]; override it with
/// [`NodeGraph::graph_style`](crate::NodeGraph::graph_style). The transient
/// overlays have their own types ([`SelectionBoxStyle`], [`CuttingToolStyle`]),
/// so this is only what is always on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStyle {
    /// Background color for the canvas.
    pub background_color: Color,
    /// Optional tiling drawn over `background_color` (grid, dots, ...).
    pub tiling: Option<TilingBackground>,
}

/// Style of the selection box the widget draws while dragging over empty canvas.
///
/// The theme-derived base is [`default_selection_box_style`]; override it with
/// [`NodeGraph::selection_box_style`](crate::NodeGraph::selection_box_style).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBoxStyle {
    /// Fill of the rectangle. Usually translucent, since nodes sit under it.
    pub fill: Color,
    /// Stroke color of the rectangle.
    pub border_color: Color,
    /// Stroke width in SCREEN pixels: the widget divides it by the zoom, so the
    /// outline stays equally readable at every zoom, like the hit thresholds.
    pub border_width: f32,
}

/// Style of the trail the edge-cutting gesture draws behind the cursor.
///
/// The theme-derived base is [`default_cutting_tool_style`]; override it with
/// [`NodeGraph::cutting_tool_style`](crate::NodeGraph::cutting_tool_style).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuttingToolStyle {
    /// Color of the trail.
    pub color: Color,
    /// Stroke width in SCREEN pixels, scaled like [`SelectionBoxStyle::border_width`].
    pub width: f32,
}
