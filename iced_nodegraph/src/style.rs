//! Style definitions for NodeGraph visual customization.
//!
//! This module provides style types for customizing the appearance of nodes,
//! edges, and the overall graph canvas.

use iced::Color;

/// Style configuration for a node's visual appearance.
///
/// Controls the rendering of node containers in the graph.
#[derive(Debug, Clone)]
pub struct NodeStyle {
    /// Fill color for the node body
    pub fill_color: Color,
    /// Border color
    pub border_color: Color,
    /// Border width in world-space pixels
    pub border_width: f32,
    /// Corner radius for rounded corners
    pub corner_radius: f32,
    /// Node opacity (0.0 to 1.0)
    pub opacity: f32,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill_color: Color::from_rgb(0.14, 0.14, 0.16),
            border_color: Color::from_rgb(0.20, 0.20, 0.22),
            border_width: 1.0,
            corner_radius: 5.0,
            opacity: 0.75,
        }
    }
}

impl NodeStyle {
    /// Creates a new NodeStyle with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the fill color.
    pub fn fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }

    /// Sets the border color.
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Sets the border width.
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    /// Sets the corner radius.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Sets the opacity.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Creates a style preset for input nodes (blue tint).
    pub fn input() -> Self {
        Self {
            fill_color: Color::from_rgb(0.15, 0.20, 0.30),
            border_color: Color::from_rgb(0.30, 0.45, 0.70),
            border_width: 1.5,
            corner_radius: 6.0,
            opacity: 0.85,
        }
    }

    /// Creates a style preset for process nodes (green tint).
    pub fn process() -> Self {
        Self {
            fill_color: Color::from_rgb(0.18, 0.28, 0.18),
            border_color: Color::from_rgb(0.35, 0.60, 0.35),
            border_width: 1.5,
            corner_radius: 4.0,
            opacity: 0.80,
        }
    }

    /// Creates a style preset for output nodes (orange tint).
    pub fn output() -> Self {
        Self {
            fill_color: Color::from_rgb(0.30, 0.22, 0.15),
            border_color: Color::from_rgb(0.75, 0.55, 0.30),
            border_width: 2.0,
            corner_radius: 8.0,
            opacity: 0.85,
        }
    }

    /// Creates a style preset for comment nodes (subtle gray).
    pub fn comment() -> Self {
        Self {
            fill_color: Color::from_rgba(0.20, 0.20, 0.22, 0.5),
            border_color: Color::from_rgba(0.40, 0.40, 0.44, 0.5),
            border_width: 1.0,
            corner_radius: 3.0,
            opacity: 0.60,
        }
    }
}

/// Edge rendering type determining the path shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum EdgeType {
    /// Smooth cubic bezier curve (default)
    #[default]
    Bezier = 0,
    /// Direct straight line
    Straight = 1,
    /// Orthogonal path with rounded corners
    SmoothStep = 2,
    /// Orthogonal path with sharp corners
    Step = 3,
}

/// Dash pattern configuration for edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashPattern {
    /// Length of each dash in world-space pixels
    pub dash_length: f32,
    /// Length of each gap in world-space pixels
    pub gap_length: f32,
    /// Whether the pattern should animate (marching ants effect)
    pub animated: bool,
}

impl Default for DashPattern {
    fn default() -> Self {
        Self {
            dash_length: 8.0,
            gap_length: 4.0,
            animated: false,
        }
    }
}

impl DashPattern {
    /// Creates a new dash pattern.
    pub fn new(dash_length: f32, gap_length: f32) -> Self {
        Self {
            dash_length,
            gap_length,
            animated: false,
        }
    }

    /// Sets whether the pattern animates.
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    /// Creates a dotted pattern (equal dash and gap).
    pub fn dotted() -> Self {
        Self::new(4.0, 4.0)
    }

    /// Creates a dashed pattern (longer dashes).
    pub fn dashed() -> Self {
        Self::new(12.0, 6.0)
    }

    /// Creates an animated marching ants pattern.
    pub fn marching_ants() -> Self {
        Self::new(6.0, 4.0).animated(true)
    }
}

/// Animation configuration for edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeAnimation {
    /// Flow speed in pixels per second (positive = toward target)
    pub flow_speed: f32,
    /// Pulsing/breathing effect
    pub pulse: bool,
    /// Outer glow effect
    pub glow: bool,
}

impl Default for EdgeAnimation {
    fn default() -> Self {
        Self {
            flow_speed: 0.0,
            pulse: false,
            glow: false,
        }
    }
}

impl EdgeAnimation {
    /// Creates a new animation with flow speed.
    pub fn flow(speed: f32) -> Self {
        Self {
            flow_speed: speed,
            pulse: false,
            glow: false,
        }
    }

    /// Enables pulse effect.
    pub fn pulse(mut self) -> Self {
        self.pulse = true;
        self
    }

    /// Enables glow effect.
    pub fn glow(mut self) -> Self {
        self.glow = true;
        self
    }

    /// Creates a data flow animation (moderate speed with glow).
    pub fn data_flow() -> Self {
        Self::flow(30.0).glow()
    }

    /// Creates an error animation (fast pulsing).
    pub fn error() -> Self {
        Self {
            flow_speed: 50.0,
            pulse: true,
            glow: false,
        }
    }
}

/// Style configuration for edges/connections.
///
/// Controls the rendering of connection lines between pins.
#[derive(Debug, Clone)]
pub struct EdgeStyle {
    /// Edge line color
    pub color: Color,
    /// Line thickness in world-space pixels
    pub thickness: f32,
    /// Edge path type (bezier, straight, step, etc.)
    pub edge_type: EdgeType,
    /// Optional dash pattern (None = solid line)
    pub dash_pattern: Option<DashPattern>,
    /// Optional animation effects
    pub animation: Option<EdgeAnimation>,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            // Transparent color means "use global edge color from theme"
            color: Color::TRANSPARENT,
            thickness: 2.0,
            edge_type: EdgeType::default(),
            dash_pattern: None,
            animation: None,
        }
    }
}

impl EdgeStyle {
    /// Creates a new EdgeStyle with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the edge color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the edge thickness.
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    /// Sets the edge type.
    pub fn edge_type(mut self, edge_type: EdgeType) -> Self {
        self.edge_type = edge_type;
        self
    }

    /// Sets the dash pattern.
    pub fn dash_pattern(mut self, pattern: DashPattern) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    /// Sets the animation.
    pub fn animation(mut self, animation: EdgeAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Makes the edge a solid line (removes dash pattern).
    pub fn solid(mut self) -> Self {
        self.dash_pattern = None;
        self
    }

    /// Creates a data flow style (blue, animated glow).
    pub fn data_flow() -> Self {
        Self {
            color: Color::from_rgb(0.3, 0.6, 1.0),
            thickness: 2.5,
            edge_type: EdgeType::Bezier,
            dash_pattern: None,
            animation: Some(EdgeAnimation::data_flow()),
        }
    }

    /// Creates a control flow style (white, straight).
    pub fn control_flow() -> Self {
        Self {
            color: Color::WHITE,
            thickness: 2.0,
            edge_type: EdgeType::SmoothStep,
            dash_pattern: None,
            animation: None,
        }
    }

    /// Creates an error style (red, animated dotted).
    pub fn error() -> Self {
        Self {
            color: Color::from_rgb(0.9, 0.2, 0.2),
            thickness: 2.0,
            edge_type: EdgeType::Bezier,
            dash_pattern: Some(DashPattern::marching_ants()),
            animation: Some(EdgeAnimation::error()),
        }
    }

    /// Creates a disabled style (gray, dashed).
    pub fn disabled() -> Self {
        Self {
            color: Color::from_rgb(0.5, 0.5, 0.5),
            thickness: 1.5,
            edge_type: EdgeType::Bezier,
            dash_pattern: Some(DashPattern::dashed()),
            animation: None,
        }
    }

    /// Creates a highlighted style (bright, glowing).
    pub fn highlighted() -> Self {
        Self {
            color: Color::from_rgb(1.0, 0.8, 0.2),
            thickness: 3.0,
            edge_type: EdgeType::Bezier,
            dash_pattern: None,
            animation: Some(EdgeAnimation::flow(0.0).glow()),
        }
    }

    /// Computes animation flags for GPU buffer.
    pub fn animation_flags(&self) -> u32 {
        let mut flags = 0u32;
        if let Some(ref dash) = self.dash_pattern {
            if dash.animated {
                flags |= 1; // bit 0: animated dash
            }
        }
        if let Some(ref anim) = self.animation {
            if anim.glow {
                flags |= 2; // bit 1: glow
            }
            if anim.pulse {
                flags |= 4; // bit 2: pulse
            }
        }
        flags
    }

    /// Gets the flow speed (0.0 if no animation).
    pub fn flow_speed(&self) -> f32 {
        self.animation.map(|a| a.flow_speed).unwrap_or(0.0)
    }
}

/// Complete graph style configuration.
///
/// Controls the appearance of the graph canvas background and drag feedback.
#[derive(Debug, Clone)]
pub struct GraphStyle {
    /// Background color of the canvas
    pub background_color: Color,
    /// Grid line color (for future grid rendering)
    pub grid_color: Color,
    /// Drag edge color when connection is invalid
    pub drag_edge_color: Color,
    /// Drag edge color when connection is valid
    pub drag_edge_valid_color: Color,
    /// Selection style for node highlighting and box selection
    pub selection_style: SelectionStyle,
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            grid_color: Color::from_rgb(0.20, 0.20, 0.22),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
            selection_style: SelectionStyle::default(),
        }
    }
}

impl GraphStyle {
    /// Creates a new GraphStyle with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the background color.
    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Sets the grid color.
    pub fn grid_color(mut self, color: Color) -> Self {
        self.grid_color = color;
        self
    }

    /// Sets the drag edge color for invalid connections.
    pub fn drag_edge_color(mut self, color: Color) -> Self {
        self.drag_edge_color = color;
        self
    }

    /// Sets the drag edge color for valid connections.
    pub fn drag_edge_valid_color(mut self, color: Color) -> Self {
        self.drag_edge_valid_color = color;
        self
    }

    /// Sets the selection style.
    pub fn selection_style(mut self, style: SelectionStyle) -> Self {
        self.selection_style = style;
        self
    }

    /// Creates a dark theme graph style.
    pub fn dark() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            grid_color: Color::from_rgb(0.20, 0.20, 0.22),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
            selection_style: SelectionStyle::default(),
        }
    }

    /// Creates a light theme graph style.
    pub fn light() -> Self {
        Self {
            background_color: Color::from_rgb(0.92, 0.92, 0.93),
            grid_color: Color::from_rgb(0.70, 0.70, 0.72),
            drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
            drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
            selection_style: SelectionStyle::default(),
        }
    }
}

/// Style configuration for node selection highlighting.
///
/// Controls the visual appearance of selected nodes and the box selection rectangle.
#[derive(Debug, Clone)]
pub struct SelectionStyle {
    /// Border color for selected nodes
    pub selected_border_color: Color,
    /// Border width for selected nodes (typically thicker than normal)
    pub selected_border_width: f32,
    /// Fill color for the box selection rectangle (semi-transparent)
    pub box_select_fill: Color,
    /// Border color for the box selection rectangle
    pub box_select_border: Color,
}

impl Default for SelectionStyle {
    fn default() -> Self {
        Self {
            selected_border_color: Color::from_rgb(0.3, 0.6, 1.0),
            selected_border_width: 2.5,
            box_select_fill: Color::from_rgba(0.3, 0.6, 1.0, 0.15),
            box_select_border: Color::from_rgba(0.3, 0.6, 1.0, 0.6),
        }
    }
}

impl SelectionStyle {
    /// Creates a new SelectionStyle with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the selected border color.
    pub fn selected_border_color(mut self, color: Color) -> Self {
        self.selected_border_color = color;
        self
    }

    /// Sets the selected border width.
    pub fn selected_border_width(mut self, width: f32) -> Self {
        self.selected_border_width = width;
        self
    }

    /// Sets the box selection fill color.
    pub fn box_select_fill(mut self, color: Color) -> Self {
        self.box_select_fill = color;
        self
    }

    /// Sets the box selection border color.
    pub fn box_select_border(mut self, color: Color) -> Self {
        self.box_select_border = color;
        self
    }
}

/// Calculates relative luminance of a color using WCAG 2.0 formula.
///
/// This is used for proper theme detection instead of naive brightness.
/// Returns a value between 0.0 (black) and 1.0 (white).
pub fn relative_luminance(color: Color) -> f32 {
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
///
/// Light text (high luminance) indicates a dark background theme.
pub fn is_dark_theme(text_color: Color) -> bool {
    relative_luminance(text_color) > 0.5
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
        // Light text on dark background
        assert!(is_dark_theme(Color::WHITE));
        assert!(is_dark_theme(Color::from_rgb(0.9, 0.9, 0.9)));

        // Dark text on light background
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
}
