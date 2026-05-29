//! Style definitions for NodeGraph visual customization.
//!
//! This module provides style types for customizing the appearance of nodes,
//! edges, and the overall graph canvas.
//!
//! ## Config vs Style
//!
//! - **Config types** (`NodeConfig`, `EdgeConfig`, etc.) use `Option<T>` fields
//!   for partial overrides. Use `merge()` to combine configs.
//! - **Style types** (`NodeStyle`, `EdgeStyle`, etc.) have concrete values and
//!   are resolved from Config + Theme at render time via `from_theme()`.
//!
//! Edge and node styling uses `iced_sdf::Pattern` for stroke patterns (solid,
//! dashed, dotted, arrowed) and `iced_sdf::Style` for composited rendering.

use iced::{Color, Theme};
use iced_sdf::Pattern;
use std::borrow::Cow;

mod config;
mod sdf;

// Re-export config types
pub use config::{EdgeConfig, GraphConfig, NodeConfig, PinConfig, SelectionConfig, ShadowConfig};

// SDF layer decomposition (crate-internal, used by the widget renderer).
pub(crate) use sdf::{EdgeGeometry, color_with_opacity};

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
// Style Function Types (Iced Toggler Pattern)
// ============================================================================

/// Style callback for nodes.
pub type NodeStyleFn<'a, Theme> = Box<dyn Fn(&Theme, NodeStatus, NodeStyle) -> NodeStyle + 'a>;

/// Style callback for pins.
pub type PinStyleFn<'a, Theme> = Box<dyn Fn(&Theme, PinStatus, PinStyle) -> PinStyle + 'a>;

/// Style callback for edges.
pub type EdgeStyleFn<'a, Theme> = Box<dyn Fn(&Theme, EdgeStatus, EdgeStyle) -> EdgeStyle + 'a>;

// ============================================================================
// Pin Style
// ============================================================================

/// Style configuration for pins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinStyle {
    /// Pin indicator color
    pub color: Color,
    /// Pin indicator radius in world-space pixels
    pub radius: f32,
    /// Shape of the pin indicator
    pub shape: PinShape,
    /// Optional border color (None = no border)
    pub border_color: Option<Color>,
    /// Border width in world-space pixels
    pub border_width: f32,
}

impl Default for PinStyle {
    fn default() -> Self {
        Self {
            color: Color::from_rgb(0.5, 0.5, 0.5),
            radius: 6.0,
            shape: PinShape::Circle,
            border_color: None,
            border_width: 1.0,
        }
    }
}

impl PinStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn shape(mut self, shape: PinShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    pub fn no_border(mut self) -> Self {
        self.border_color = None;
        self
    }

    /// Creates a style preset for data pins (circle, blue).
    pub fn data() -> Self {
        Self {
            color: Color::from_rgb(0.3, 0.6, 1.0),
            radius: 6.0,
            shape: PinShape::Circle,
            border_color: Some(Color::from_rgb(0.5, 0.7, 1.0)),
            border_width: 1.0,
        }
    }

    /// Creates a style preset for execution pins (triangle, white).
    pub fn execution() -> Self {
        Self {
            color: Color::WHITE,
            radius: 7.0,
            shape: PinShape::Triangle,
            border_color: None,
            border_width: 0.0,
        }
    }

    /// Creates a style preset for control flow pins (diamond, yellow).
    pub fn control() -> Self {
        Self {
            color: Color::from_rgb(1.0, 0.85, 0.3),
            radius: 6.0,
            shape: PinShape::Diamond,
            border_color: Some(Color::from_rgb(1.0, 0.95, 0.6)),
            border_width: 1.0,
        }
    }

    /// Creates a style preset for event pins (square, green).
    pub fn event() -> Self {
        Self {
            color: Color::from_rgb(0.3, 0.8, 0.4),
            radius: 5.0,
            shape: PinShape::Square,
            border_color: Some(Color::from_rgb(0.5, 0.9, 0.6)),
            border_width: 1.0,
        }
    }

    /// Creates a pin style derived from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let secondary = palette.secondary.base.color;
        let text = palette.background.base.text;

        if palette.is_dark {
            Self {
                color: Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.7),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: None,
                border_width: 1.0,
            }
        } else {
            Self {
                color: Color::from_rgba(
                    secondary.r * 0.7,
                    secondary.g * 0.7,
                    secondary.b * 0.7,
                    0.8,
                ),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: Some(Color::from_rgba(text.r, text.g, text.b, 0.3)),
                border_width: 1.0,
            }
        }
    }

    /// Returns the scaled radius for the given status.
    pub fn scaled_radius(&self, status: PinStatus, time: f32) -> f32 {
        match status {
            PinStatus::Idle => self.radius,
            PinStatus::ValidTarget => {
                let pulse = (time * 6.0).sin() * 0.5 + 0.5;
                self.radius * (1.0 + pulse * 0.5)
            }
        }
    }
}

// ============================================================================
// Node Shadow
// ============================================================================

/// Shadow configuration for nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeShadow {
    /// Shadow color (typically semi-transparent black).
    pub color: Color,
    /// Horizontal and vertical offset in world-space pixels.
    pub offset: (f32, f32),
    /// Blur radius in world-space pixels.
    pub blur: f32,
}

impl Default for NodeShadow {
    fn default() -> Self {
        Self {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: (4.0, 4.0),
            blur: 8.0,
        }
    }
}

impl NodeShadow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    /// Subtle shadow preset.
    pub fn subtle() -> Self {
        Self {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: (2.0, 2.0),
            blur: 4.0,
        }
    }

    /// Medium shadow preset (default).
    pub fn medium() -> Self {
        Self::default()
    }

    /// Strong shadow preset for elevated elements.
    pub fn strong() -> Self {
        Self {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: (6.0, 8.0),
            blur: 16.0,
        }
    }

    /// Glow effect (centered shadow with color).
    pub fn glow(color: Color) -> Self {
        Self {
            color: Color::from_rgba(color.r, color.g, color.b, 0.5),
            offset: (0.0, 0.0),
            blur: 12.0,
        }
    }
}

// ============================================================================
// Node Border
// ============================================================================

/// Border style for nodes using iced_sdf Pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeBorder {
    /// Border color.
    pub color: Color,
    /// Stroke pattern (Pattern::solid(width) for simple borders).
    pub pattern: Pattern,
    /// Optional outline ring: (width, color).
    pub outline: Option<(f32, Color)>,
}

impl Default for NodeBorder {
    fn default() -> Self {
        Self {
            color: Color::from_rgb(0.20, 0.20, 0.22),
            pattern: Pattern::solid(1.0),
            outline: None,
        }
    }
}

impl NodeBorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = pattern;
        self
    }

    /// Sets a simple solid border width.
    pub fn width(mut self, width: f32) -> Self {
        self.pattern = Pattern::solid(width);
        self
    }

    pub fn outline(mut self, width: f32, color: Color) -> Self {
        self.outline = Some((width, color));
        self
    }
}

// ============================================================================
// Node Style
// ============================================================================

/// Style configuration for a node's visual appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStyle {
    /// Fill color for the node body
    pub fill_color: Color,
    /// Corner radius for rounded corners
    pub corner_radius: f32,
    /// Node opacity (0.0 to 1.0)
    pub opacity: f32,
    /// Optional border
    pub border: Option<NodeBorder>,
    /// Optional drop shadow
    pub shadow: Option<NodeShadow>,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill_color: Color::from_rgb(0.14, 0.14, 0.16),
            corner_radius: 8.0,
            opacity: 0.75,
            border: Some(NodeBorder::default()),
            shadow: Some(NodeShadow::subtle()),
        }
    }
}

impl NodeStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }

    pub fn border(mut self, border: NodeBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// Sets a simple border with color and width (convenience method).
    pub fn simple_border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(NodeBorder::new().color(color).width(width));
        self
    }

    pub fn no_border(mut self) -> Self {
        self.border = None;
        self
    }

    /// Sets the border color (convenience, modifies existing border).
    pub fn border_color(mut self, color: Color) -> Self {
        if let Some(ref mut border) = self.border {
            border.color = color;
        } else {
            self.border = Some(NodeBorder::new().color(color));
        }
        self
    }

    /// Sets the border width (convenience, modifies existing border).
    pub fn border_width(mut self, width: f32) -> Self {
        if let Some(ref mut border) = self.border {
            border.pattern = Pattern::solid(width);
        } else {
            self.border = Some(NodeBorder::new().width(width));
        }
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn shadow(mut self, shadow: NodeShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = None;
        self
    }

    /// Creates a style preset for input nodes (blue tint).
    pub fn input() -> Self {
        Self {
            fill_color: Color::from_rgb(0.15, 0.20, 0.30),
            border: Some(
                NodeBorder::new()
                    .color(Color::from_rgb(0.30, 0.45, 0.70))
                    .width(1.5),
            ),
            corner_radius: 6.0,
            opacity: 0.85,
            shadow: Some(NodeShadow::medium()),
        }
    }

    /// Creates a style preset for process nodes (green tint).
    pub fn process() -> Self {
        Self {
            fill_color: Color::from_rgb(0.18, 0.28, 0.18),
            border: Some(
                NodeBorder::new()
                    .color(Color::from_rgb(0.35, 0.60, 0.35))
                    .width(1.5),
            ),
            corner_radius: 4.0,
            opacity: 0.80,
            shadow: Some(NodeShadow::medium()),
        }
    }

    /// Creates a style preset for output nodes (orange tint).
    pub fn output() -> Self {
        Self {
            fill_color: Color::from_rgb(0.30, 0.22, 0.15),
            border: Some(
                NodeBorder::new()
                    .color(Color::from_rgb(0.75, 0.55, 0.30))
                    .width(2.0),
            ),
            corner_radius: 8.0,
            opacity: 0.85,
            shadow: Some(NodeShadow::strong()),
        }
    }

    /// Creates a style preset for comment nodes (subtle gray).
    pub fn comment() -> Self {
        Self {
            fill_color: Color::from_rgba(0.20, 0.20, 0.22, 0.5),
            border: Some(
                NodeBorder::new()
                    .color(Color::from_rgba(0.40, 0.40, 0.44, 0.5))
                    .width(1.0),
            ),
            corner_radius: 3.0,
            opacity: 0.60,
            shadow: None,
        }
    }

    /// Creates an input node style derived from theme's primary color.
    pub fn input_themed(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let primary = palette.primary.base.color;
        let bg = palette.background.base.color;

        if palette.is_dark {
            Self {
                fill_color: Color::from_rgba(
                    bg.r + (primary.r - bg.r) * 0.15,
                    bg.g + (primary.g - bg.g) * 0.15,
                    bg.b + (primary.b - bg.b) * 0.15,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(primary.r, primary.g, primary.b, 0.6))
                        .width(1.5),
                ),
                corner_radius: 6.0,
                opacity: 0.85,
                shadow: Some(NodeShadow::medium()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - primary.r) * 0.08,
                    1.0 - (1.0 - primary.g) * 0.08,
                    1.0 - (1.0 - primary.b) * 0.08,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(primary.r, primary.g, primary.b, 0.5))
                        .width(1.5),
                ),
                corner_radius: 6.0,
                opacity: 0.90,
                shadow: Some(NodeShadow::subtle()),
            }
        }
    }

    /// Creates a process node style derived from theme's success color.
    pub fn process_themed(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let success = palette.success.base.color;
        let bg = palette.background.base.color;

        if palette.is_dark {
            Self {
                fill_color: Color::from_rgba(
                    bg.r + (success.r - bg.r) * 0.12,
                    bg.g + (success.g - bg.g) * 0.12,
                    bg.b + (success.b - bg.b) * 0.12,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(success.r, success.g, success.b, 0.5))
                        .width(1.5),
                ),
                corner_radius: 4.0,
                opacity: 0.80,
                shadow: Some(NodeShadow::medium()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - success.r) * 0.06,
                    1.0 - (1.0 - success.g) * 0.06,
                    1.0 - (1.0 - success.b) * 0.06,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(success.r, success.g, success.b, 0.4))
                        .width(1.5),
                ),
                corner_radius: 4.0,
                opacity: 0.88,
                shadow: Some(NodeShadow::subtle()),
            }
        }
    }

    /// Creates an output node style derived from theme's secondary color.
    pub fn output_themed(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let secondary = palette.secondary.base.color;
        let bg = palette.background.base.color;

        if palette.is_dark {
            Self {
                fill_color: Color::from_rgba(
                    bg.r + (secondary.r - bg.r) * 0.15,
                    bg.g + (secondary.g - bg.g) * 0.15,
                    bg.b + (secondary.b - bg.b) * 0.15,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.7))
                        .width(2.0),
                ),
                corner_radius: 8.0,
                opacity: 0.85,
                shadow: Some(NodeShadow::strong()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - secondary.r) * 0.10,
                    1.0 - (1.0 - secondary.g) * 0.10,
                    1.0 - (1.0 - secondary.b) * 0.10,
                    1.0,
                ),
                border: Some(
                    NodeBorder::new()
                        .color(Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.6))
                        .width(2.0),
                ),
                corner_radius: 8.0,
                opacity: 0.90,
                shadow: Some(NodeShadow::medium()),
            }
        }
    }

    /// Creates a comment node style from theme's background weak color.
    pub fn comment_themed(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let weak = palette.background.weak.color;

        Self {
            fill_color: Color::from_rgba(weak.r, weak.g, weak.b, 0.5),
            border: Some(
                NodeBorder::new()
                    .color(Color::from_rgba(
                        weak.r * 1.2,
                        weak.g * 1.2,
                        weak.b * 1.2,
                        0.4,
                    ))
                    .width(1.0),
            ),
            corner_radius: 3.0,
            opacity: 0.60,
            shadow: None,
        }
    }

    /// Creates a node style derived from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let bg_weak = palette.background.weak.color;

        if palette.is_dark {
            let node_fill = Color::from_rgba(
                bg.r + (bg_weak.r - bg.r) * 0.3,
                bg.g + (bg_weak.g - bg.g) * 0.3,
                bg.b + (bg_weak.b - bg.b) * 0.3,
                1.0,
            );
            let node_border =
                Color::from_rgba(bg_weak.r * 1.2, bg_weak.g * 1.2, bg_weak.b * 1.2, 0.8);

            Self {
                fill_color: node_fill,
                border: Some(NodeBorder::new().color(node_border).width(1.0)),
                corner_radius: 5.0,
                opacity: 0.75,
                shadow: Some(NodeShadow::subtle()),
            }
        } else {
            let node_fill = Color::from_rgba(
                bg.r - (bg.r - bg_weak.r) * 0.15,
                bg.g - (bg.g - bg_weak.g) * 0.15,
                bg.b - (bg.b - bg_weak.b) * 0.15,
                1.0,
            );
            let node_border =
                Color::from_rgba(bg_weak.r * 0.9, bg_weak.g * 0.9, bg_weak.b * 0.9, 0.9);

            Self {
                fill_color: node_fill,
                border: Some(NodeBorder::new().color(node_border).width(1.0)),
                corner_radius: 5.0,
                opacity: 0.85,
                shadow: Some(NodeShadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                    offset: (2.0, 2.0),
                    blur: 6.0,
                }),
            }
        }
    }

    /// Returns a modified style for the given status.
    /// Returns a borrowed reference for idle nodes (zero-cost) and an owned
    /// modified copy only for selected nodes.
    pub fn for_status(&self, status: NodeStatus, selection: &SelectionStyle) -> Cow<'_, Self> {
        match status {
            NodeStatus::Idle => Cow::Borrowed(self),
            NodeStatus::Selected => {
                let mut s = self.clone();
                s.border = Some(
                    s.border
                        .unwrap_or_default()
                        .color(selection.selected_border_color)
                        .width(selection.selected_border_width),
                );
                Cow::Owned(s)
            }
        }
    }
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
// Edge Shadow
// ============================================================================

/// Shadow style for edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeShadow {
    /// Shadow color (typically semi-transparent).
    pub color: Color,
    /// Gradient end color for shadow (defaults to same as `color`).
    pub end_color: Color,
    /// Expand the shadow beyond the stroke.
    pub expand: f32,
    /// Blur radius in world-space pixels.
    pub blur: f32,
    /// Shadow offset in world-space pixels (x, y).
    pub offset: (f32, f32),
}

impl Default for EdgeShadow {
    fn default() -> Self {
        Self {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            end_color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            expand: 4.0,
            blur: 4.0,
            offset: (0.0, 0.0),
        }
    }
}

impl EdgeShadow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    /// Sets both start and end color to the same value.
    pub fn solid_color(mut self, color: Color) -> Self {
        self.color = color;
        self.end_color = color;
        self
    }

    pub fn expand(mut self, expand: f32) -> Self {
        self.expand = expand;
        self
    }

    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn subtle() -> Self {
        let c = Color::from_rgba(0.0, 0.0, 0.0, 0.15);
        Self {
            color: c,
            end_color: c,
            expand: 2.0,
            blur: 2.0,
            offset: (0.0, 0.0),
        }
    }

    pub fn strong() -> Self {
        let c = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
        Self {
            color: c,
            end_color: c,
            expand: 8.0,
            blur: 8.0,
            offset: (0.0, 0.0),
        }
    }

    pub fn glow(color: Color) -> Self {
        let c = Color::from_rgba(color.r, color.g, color.b, 0.4);
        Self {
            color: c,
            end_color: c,
            expand: 6.0,
            blur: 6.0,
            offset: (0.0, 0.0),
        }
    }
}

// ============================================================================
// Edge Border
// ============================================================================

/// Border ring around edge stroke.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeBorder {
    /// Color at source pin. TRANSPARENT = inherit from pin.
    pub start_color: Color,
    /// Color at target pin. TRANSPARENT = inherit from pin.
    pub end_color: Color,
    /// Border width in world-space pixels.
    pub width: f32,
    /// Gap between stroke and border.
    pub gap: f32,
    /// Optional outline: (width, color).
    pub outline: Option<(f32, Color)>,
    /// Background fill color for the border gap area.
    pub background: Color,
    /// Gradient end color for border background.
    pub background_end: Color,
}

impl Default for EdgeBorder {
    fn default() -> Self {
        Self {
            start_color: Color::TRANSPARENT,
            end_color: Color::TRANSPARENT,
            width: 1.0,
            gap: 0.5,
            outline: None,
            background: Color::TRANSPARENT,
            background_end: Color::TRANSPARENT,
        }
    }
}

impl EdgeBorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.start_color = color;
        self.end_color = color;
        self
    }

    pub fn start_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self
    }

    pub fn end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn outline(mut self, width: f32, color: Color) -> Self {
        self.outline = Some((width, color));
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self.background_end = color;
        self
    }

    pub fn background_gradient(mut self, start: Color, end: Color) -> Self {
        self.background = start;
        self.background_end = end;
        self
    }
}

// ============================================================================
// Edge Style
// ============================================================================

/// Style configuration for edges using iced_sdf Pattern and Style.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::EdgeStyle;
/// use iced_sdf::Pattern;
/// use iced::Color;
///
/// // Simple solid edge using pin colors
/// let simple = EdgeStyle::new();
///
/// // Dashed edge with explicit color
/// let dashed = EdgeStyle::new()
///     .start_color(Color::WHITE)
///     .end_color(Color::WHITE)
///     .pattern(Pattern::dashed(2.0, 12.0, 6.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeStyle {
    /// Color at source pin. TRANSPARENT = inherit from pin.
    pub start_color: Color,
    /// Color at target pin. TRANSPARENT = inherit from pin.
    pub end_color: Color,
    /// Stroke pattern (includes thickness, dash/gap, flow).
    pub pattern: Pattern,
    /// Optional outline on the stroke layer: (width, color).
    pub stroke_outline: Option<(f32, Color)>,
    /// Optional border ring around stroke.
    pub border: Option<EdgeBorder>,
    /// Optional shadow behind edge.
    pub shadow: Option<EdgeShadow>,
    /// Path shape.
    pub curve: EdgeCurve,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            start_color: Color::TRANSPARENT,
            end_color: Color::TRANSPARENT,
            pattern: Pattern::solid(2.0),
            stroke_outline: None,
            border: None,
            shadow: None,
            curve: EdgeCurve::default(),
        }
    }
}

impl EdgeStyle {
    /// Creates a new edge style with defaults (solid 2px, pin colors, bezier).
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self
    }

    pub fn end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    /// Sets a solid color (both start and end).
    pub fn solid_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self.end_color = color;
        self
    }

    /// Sets a gradient from start to end color.
    pub fn gradient(mut self, start: Color, end: Color) -> Self {
        self.start_color = start;
        self.end_color = end;
        self
    }

    /// Uses pin colors for gradient (default behavior).
    pub fn from_pins(mut self) -> Self {
        self.start_color = Color::TRANSPARENT;
        self.end_color = Color::TRANSPARENT;
        self
    }

    pub fn pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = pattern;
        self
    }

    /// Sets the stroke width (updates pattern thickness).
    pub fn width(mut self, width: f32) -> Self {
        self.pattern.thickness = width;
        self
    }

    /// Alias for width.
    pub fn thickness(self, thickness: f32) -> Self {
        self.width(thickness)
    }

    pub fn stroke_outline(mut self, width: f32, color: Color) -> Self {
        self.stroke_outline = Some((width, color));
        self
    }

    pub fn no_stroke_outline(mut self) -> Self {
        self.stroke_outline = None;
        self
    }

    pub fn border(mut self, border: EdgeBorder) -> Self {
        self.border = Some(border);
        self
    }

    pub fn no_border(mut self) -> Self {
        self.border = None;
        self
    }

    pub fn shadow(mut self, shadow: EdgeShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = None;
        self
    }

    pub fn curve(mut self, curve: EdgeCurve) -> Self {
        self.curve = curve;
        self
    }

    // === Preset Styles ===

    /// Creates a data flow style (blue, bezier curve).
    pub fn data_flow() -> Self {
        Self {
            start_color: Color::from_rgb(0.3, 0.6, 1.0),
            end_color: Color::from_rgb(0.3, 0.6, 1.0),
            pattern: Pattern::solid(2.5),
            stroke_outline: None,
            border: None,
            shadow: None,
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Creates an error style (red, animated marching ants with border).
    pub fn error() -> Self {
        let color = Color::from_rgb(0.9, 0.2, 0.2);
        Self {
            start_color: color,
            end_color: color,
            pattern: Pattern::dashed(2.0, 6.0, 4.0).flow(30.0),
            stroke_outline: None,
            border: Some(EdgeBorder::new().width(1.0).gap(0.5).color(color)),
            shadow: None,
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Creates a disabled style (gray, dashed).
    pub fn disabled() -> Self {
        let color = Color::from_rgb(0.5, 0.5, 0.5);
        Self {
            start_color: color,
            end_color: color,
            pattern: Pattern::dashed(1.5, 12.0, 6.0),
            stroke_outline: None,
            border: None,
            shadow: None,
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Creates a highlighted style (bright, with border).
    pub fn highlighted() -> Self {
        let color = Color::from_rgb(1.0, 0.8, 0.2);
        Self {
            start_color: color,
            end_color: color,
            pattern: Pattern::solid(3.0),
            stroke_outline: None,
            border: Some(
                EdgeBorder::new()
                    .width(2.0)
                    .gap(1.0)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.3)),
            ),
            shadow: None,
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Creates a debug/temporary style (dotted, cyan, straight line).
    pub fn debug() -> Self {
        Self {
            start_color: Color::from_rgb(0.0, 1.0, 1.0),
            end_color: Color::from_rgb(0.0, 1.0, 1.0),
            pattern: Pattern::dotted(8.0, 2.0),
            stroke_outline: None,
            border: None,
            shadow: None,
            curve: EdgeCurve::Line,
        }
    }

    /// Returns whether this edge has an animated pattern.
    pub fn has_motion(&self) -> bool {
        self.pattern.flow_speed.abs() > 0.001
    }

    /// Gets the motion speed (0.0 if no motion).
    pub fn motion_speed(&self) -> f32 {
        self.pattern.flow_speed
    }

    /// Returns a modified style for the given status.
    pub fn for_status(&self, status: EdgeStatus) -> Self {
        match status {
            EdgeStatus::Idle => *self,
            EdgeStatus::PendingCut => {
                let mut s = *self;
                s.start_color = Color::from_rgb(1.0, 0.2, 0.2);
                s.end_color = Color::from_rgb(1.0, 0.2, 0.2);
                s
            }
        }
    }

    // === Getter Methods ===

    /// Gets the stroke width from the pattern.
    pub fn get_width(&self) -> f32 {
        self.pattern.thickness
    }

    /// Merges an EdgeConfig into this style, returning a new style.
    pub fn with_config(&self, config: &EdgeConfig) -> Self {
        Self {
            start_color: config.start_color.unwrap_or(self.start_color),
            end_color: config.end_color.unwrap_or(self.end_color),
            pattern: config.pattern.unwrap_or(self.pattern),
            stroke_outline: config.stroke_outline.or(self.stroke_outline),
            border: config.border.or(self.border),
            shadow: config.shadow.or(self.shadow),
            curve: config.curve.unwrap_or(self.curve),
        }
    }

    /// Creates an edge style derived from an iced Theme.
    ///
    /// Currently returns the default style regardless of theme.
    /// The parameter is retained for API consistency with `NodeStyle::from_theme()`
    /// and to allow theme-aware edge styling in the future.
    pub fn from_theme(_theme: &Theme) -> Self {
        Self::new()
    }
}

// ============================================================================
// Graph Style
// ============================================================================

/// Complete graph style configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStyle {
    /// Background color for the canvas.
    pub background_color: Color,
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
            selection_style: SelectionStyle::default(),
        }
    }

    /// Creates a graph style derived from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let secondary = palette.secondary.base.color;
        let success = palette.success.base.color;

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

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculates relative luminance of a color using WCAG 2.0 formula.
/// See: <https://www.w3.org/TR/WCAG20/#relativeluminancedef>
pub fn relative_luminance(color: Color) -> f32 {
    // sRGB to linear conversion per WCAG 2.0 spec
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let r = srgb_to_linear(color.r);
    let g = srgb_to_linear(color.g);
    let b = srgb_to_linear(color.b);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Determines if a theme is dark based on text color luminance.
pub fn is_dark_theme(text_color: Color) -> bool {
    relative_luminance(text_color) > 0.5
}

/// Lightens a color by mixing with white.
pub fn lighten(color: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::from_rgba(
        color.r + (1.0 - color.r) * amount,
        color.g + (1.0 - color.g) * amount,
        color.b + (1.0 - color.b) * amount,
        color.a,
    )
}

/// Darkens a color by mixing with black.
pub fn darken(color: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::from_rgba(
        color.r * (1.0 - amount),
        color.g * (1.0 - amount),
        color.b * (1.0 - amount),
        color.a,
    )
}

/// Creates a semi-transparent version of a color.
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha.clamp(0.0, 1.0))
}

/// Blends two colors together.
pub fn blend(a: Color, b: Color, ratio: f32) -> Color {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;
    Color::from_rgba(
        a.r * inv + b.r * ratio,
        a.g * inv + b.g * ratio,
        a.b * inv + b.b * ratio,
        a.a * inv + b.a * ratio,
    )
}

/// All standard iced themes for easy enumeration in UI.
pub const STANDARD_THEMES: [Theme; 22] = [
    Theme::Light,
    Theme::Dark,
    Theme::Dracula,
    Theme::Nord,
    Theme::SolarizedLight,
    Theme::SolarizedDark,
    Theme::GruvboxLight,
    Theme::GruvboxDark,
    Theme::CatppuccinLatte,
    Theme::CatppuccinFrappe,
    Theme::CatppuccinMacchiato,
    Theme::CatppuccinMocha,
    Theme::TokyoNight,
    Theme::TokyoNightStorm,
    Theme::TokyoNightLight,
    Theme::KanagawaWave,
    Theme::KanagawaDragon,
    Theme::KanagawaLotus,
    Theme::Moonfly,
    Theme::Nightfly,
    Theme::Oxocarbon,
    Theme::Ferra,
];

/// Returns the display name of a theme.
pub fn theme_name(theme: &Theme) -> &'static str {
    match theme {
        Theme::Light => "Light",
        Theme::Dark => "Dark",
        Theme::Dracula => "Dracula",
        Theme::Nord => "Nord",
        Theme::SolarizedLight => "Solarized Light",
        Theme::SolarizedDark => "Solarized Dark",
        Theme::GruvboxLight => "Gruvbox Light",
        Theme::GruvboxDark => "Gruvbox Dark",
        Theme::CatppuccinLatte => "Catppuccin Latte",
        Theme::CatppuccinFrappe => "Catppuccin Frappe",
        Theme::CatppuccinMacchiato => "Catppuccin Macchiato",
        Theme::CatppuccinMocha => "Catppuccin Mocha",
        Theme::TokyoNight => "Tokyo Night",
        Theme::TokyoNightStorm => "Tokyo Night Storm",
        Theme::TokyoNightLight => "Tokyo Night Light",
        Theme::KanagawaWave => "Kanagawa Wave",
        Theme::KanagawaDragon => "Kanagawa Dragon",
        Theme::KanagawaLotus => "Kanagawa Lotus",
        Theme::Moonfly => "Moonfly",
        Theme::Nightfly => "Nightfly",
        Theme::Oxocarbon => "Oxocarbon",
        Theme::Ferra => "Ferra",
        Theme::Custom(_) => "Custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_luminance_black() {
        let lum = relative_luminance(Color::BLACK);
        assert!(lum < 0.01, "Black should have near-zero luminance");
    }

    #[test]
    fn test_luminance_white() {
        let lum = relative_luminance(Color::WHITE);
        assert!(lum > 0.99, "White should have near-one luminance");
    }

    #[test]
    fn test_dark_theme_detection() {
        assert!(is_dark_theme(Color::WHITE));
        assert!(is_dark_theme(Color::from_rgb(0.9, 0.9, 0.9)));
        assert!(!is_dark_theme(Color::BLACK));
        assert!(!is_dark_theme(Color::from_rgb(0.1, 0.1, 0.1)));
    }

    #[test]
    fn test_node_style_builder() {
        let style = NodeStyle::new()
            .fill_color(Color::from_rgb(0.5, 0.5, 0.5))
            .corner_radius(10.0)
            .opacity(0.9);

        assert_eq!(style.corner_radius, 10.0);
        assert_eq!(style.opacity, 0.9);
    }

    #[test]
    fn test_standard_themes_count() {
        assert_eq!(STANDARD_THEMES.len(), 22);
    }

    #[test]
    fn test_theme_name() {
        assert_eq!(theme_name(&Theme::Dark), "Dark");
        assert_eq!(theme_name(&Theme::Light), "Light");
        assert_eq!(theme_name(&Theme::CatppuccinMocha), "Catppuccin Mocha");
    }

    #[test]
    fn test_all_standard_themes_have_names() {
        for theme in &STANDARD_THEMES {
            let name = theme_name(theme);
            assert!(!name.is_empty());
            assert_ne!(name, "Custom");
        }
    }

    #[test]
    fn test_edge_style_default_uses_pin_colors() {
        let style = EdgeStyle::default();
        assert!(style.start_color.a < 0.01);
        assert!(style.end_color.a < 0.01);
    }

    #[test]
    fn test_edge_style_solid_color() {
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let style = EdgeStyle::new().solid_color(red);
        assert_eq!(style.start_color, red);
        assert_eq!(style.end_color, red);
    }

    #[test]
    fn test_edge_style_gradient() {
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let style = EdgeStyle::new().gradient(red, blue);
        assert_eq!(style.start_color, red);
        assert_eq!(style.end_color, blue);
    }
}
