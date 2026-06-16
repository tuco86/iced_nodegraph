//! Style definitions for NodeGraph visual customization.
//!
//! Node, edge, and pin styles are flat, concrete structs ([`NodeStyle`],
//! [`EdgeStyle`], [`PinStyle`]): the fully populated form the renderer consumes.
//! The theme-derived defaults are [`default_node_style`], [`default_edge_style`]
//! and [`default_pin_style`]; override individual fields with struct-update
//! syntax over them. See [`ColorQuad`] for the unified color type.
//!
//! [`GraphStyle`] and [`SelectionStyle`] (canvas background, selection overlay,
//! drag-edge colors) are also plain structs; they are not per-element styles.

use iced::{Color, Theme};

mod color;
mod defaults;
mod edge;
mod node;
mod pin;
mod sdf;

pub use color::ColorQuad;
pub use defaults::{default_edge_style, default_node_style, default_pin_style};
pub use edge::EdgeStyle;
pub use node::NodeStyle;
pub use pin::PinStyle;

// SDF layer decomposition (crate-internal, used by the widget renderer).
pub(crate) use sdf::EdgeGeometry;

/// Shape of a pin indicator.
///
/// Different shapes help users visually distinguish pin types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum PinShape {
    /// Standard circular pin (default)
    #[default]
    Circle = 0,
    /// Square pin for data ports
    Square = 1,
    /// Diamond pin for control flow
    Diamond = 2,
    /// Triangle pin pointing outward
    Triangle = 3,
}

// ============================================================================
// Status Enums for Widget-Side Styling
// ============================================================================

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

// ============================================================================
// Edge Curve Types
// ============================================================================

/// Edge path curve type determining the shape of the connection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EdgeCurve {
    /// Smooth cubic bezier curve (default)
    #[default]
    BezierCubic,
    /// Direct straight line between pins
    Line,
}

// ============================================================================
// Graph Style
// ============================================================================

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

/// Complete graph style configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStyle {
    /// Background color for the canvas.
    pub background_color: Color,
    /// Optional tiling drawn over `background_color` (grid, dots, ...).
    pub tiling: Option<TilingBackground>,
    /// Drag edge color when connection is invalid.
    pub drag_edge_color: Color,
    /// Drag edge color when connection is valid.
    pub drag_edge_valid_color: Color,
    /// Selection style for node highlighting and box selection.
    pub selection_style: SelectionStyle,
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
            tiling: None,
            selection_style: SelectionStyle::default(),
        }
    }
}

impl GraphStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    pub fn drag_edge_color(mut self, color: Color) -> Self {
        self.drag_edge_color = color;
        self
    }

    pub fn drag_edge_valid_color(mut self, color: Color) -> Self {
        self.drag_edge_valid_color = color;
        self
    }

    pub fn selection_style(mut self, style: SelectionStyle) -> Self {
        self.selection_style = style;
        self
    }

    /// Sets a tiling background (grid, dots, ...) drawn over `background_color`.
    pub fn tiling(mut self, tiling: TilingBackground) -> Self {
        self.tiling = Some(tiling);
        self
    }

    /// Creates a dark theme graph style.
    pub fn dark() -> Self {
        Self::default()
    }

    /// Creates a light theme graph style.
    pub fn light() -> Self {
        Self {
            background_color: Color::from_rgb(0.95, 0.95, 0.96),
            drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
            drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
            tiling: None,
            selection_style: SelectionStyle::default(),
        }
    }

    /// Creates a graph style derived from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let secondary = palette.secondary.base.color;
        let success = palette.success.base.color;
        // Subtle theme-derived grid as the default canvas backdrop.
        let grid = TilingBackground::grid(
            40.0,
            1.0,
            Color {
                a: 0.35,
                ..palette.background.strong.color
            },
        );

        if palette.is_dark {
            Self {
                background_color: Color::from_rgb(bg.r * 0.7, bg.g * 0.7, bg.b * 0.7),
                drag_edge_color: Color::from_rgb(
                    secondary.r * 0.9 + 0.1,
                    secondary.g * 0.6,
                    secondary.b * 0.3,
                ),
                drag_edge_valid_color: Color::from_rgb(
                    success.r * 0.6,
                    success.g * 0.9,
                    success.b * 0.6,
                ),
                tiling: Some(grid),
                selection_style: SelectionStyle::from_theme(theme),
            }
        } else {
            Self {
                background_color: Color::from_rgb(
                    bg.r * 0.98 + 0.02,
                    bg.g * 0.98 + 0.02,
                    bg.b * 0.98 + 0.02,
                ),
                drag_edge_color: Color::from_rgb(
                    secondary.r * 0.8,
                    secondary.g * 0.5,
                    secondary.b * 0.2,
                ),
                drag_edge_valid_color: Color::from_rgb(
                    success.r * 0.5,
                    success.g * 0.8,
                    success.b * 0.5,
                ),
                tiling: Some(grid),
                selection_style: SelectionStyle::from_theme(theme),
            }
        }
    }
}

// ============================================================================
// Selection Style
// ============================================================================

/// Style configuration for node selection and hover highlighting.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionStyle {
    /// Border color for selected nodes
    pub selected_border_color: Color,
    /// Border width for selected nodes
    pub selected_border_width: f32,
    /// Fill color for the box selection rectangle (semi-transparent)
    pub box_select_fill: Color,
    /// Border color for the box selection rectangle
    pub box_select_border: Color,
    /// Color for the edge cutting line
    pub edge_cutting_color: Color,
    /// Color for hover glow effect on nodes
    pub hover_glow_color: Color,
    /// Radius for hover glow effect in world units
    pub hover_glow_radius: f32,
}

impl Default for SelectionStyle {
    fn default() -> Self {
        Self {
            selected_border_color: Color::from_rgb(0.3, 0.6, 1.0),
            selected_border_width: 2.5,
            box_select_fill: Color::from_rgba(0.3, 0.6, 1.0, 0.15),
            box_select_border: Color::from_rgba(0.3, 0.6, 1.0, 0.6),
            edge_cutting_color: Color::from_rgb(1.0, 0.3, 0.3),
            hover_glow_color: Color::from_rgb(0.5, 0.7, 1.0),
            hover_glow_radius: 6.0,
        }
    }
}

impl SelectionStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_border_color(mut self, color: Color) -> Self {
        self.selected_border_color = color;
        self
    }

    pub fn selected_border_width(mut self, width: f32) -> Self {
        self.selected_border_width = width;
        self
    }

    pub fn box_select_fill(mut self, color: Color) -> Self {
        self.box_select_fill = color;
        self
    }

    pub fn box_select_border(mut self, color: Color) -> Self {
        self.box_select_border = color;
        self
    }

    pub fn hover_glow_color(mut self, color: Color) -> Self {
        self.hover_glow_color = color;
        self
    }

    pub fn hover_glow_radius(mut self, radius: f32) -> Self {
        self.hover_glow_radius = radius;
        self
    }

    /// Creates a selection style derived from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let primary = palette.primary.base.color;

        if palette.is_dark {
            Self {
                selected_border_color: primary,
                selected_border_width: 2.5,
                box_select_fill: Color::from_rgba(primary.r, primary.g, primary.b, 0.15),
                box_select_border: Color::from_rgba(primary.r, primary.g, primary.b, 0.6),
                edge_cutting_color: Color::from_rgb(1.0, 0.3, 0.3),
                hover_glow_color: Color::from_rgb(
                    primary.r * 0.7 + 0.3,
                    primary.g * 0.7 + 0.3,
                    primary.b * 0.9 + 0.1,
                ),
                hover_glow_radius: 6.0,
            }
        } else {
            Self {
                selected_border_color: primary,
                selected_border_width: 2.5,
                box_select_fill: Color::from_rgba(primary.r, primary.g, primary.b, 0.12),
                box_select_border: Color::from_rgba(primary.r, primary.g, primary.b, 0.5),
                edge_cutting_color: Color::from_rgb(0.9, 0.2, 0.2),
                hover_glow_color: Color::from_rgb(
                    primary.r * 0.8,
                    primary.g * 0.8,
                    primary.b * 0.9,
                ),
                hover_glow_radius: 5.0,
            }
        }
    }
}
