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

/// Style configuration for edges/connections.
///
/// Controls the rendering of connection lines between pins.
#[derive(Debug, Clone)]
pub struct EdgeStyle {
    /// Edge line color
    pub color: Color,
    /// Line thickness in world-space pixels
    pub thickness: f32,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            thickness: 2.0,
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
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            grid_color: Color::from_rgb(0.20, 0.20, 0.22),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
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

    /// Creates a dark theme graph style.
    pub fn dark() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            grid_color: Color::from_rgb(0.20, 0.20, 0.22),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
        }
    }

    /// Creates a light theme graph style.
    pub fn light() -> Self {
        Self {
            background_color: Color::from_rgb(0.92, 0.92, 0.93),
            grid_color: Color::from_rgb(0.70, 0.70, 0.72),
            drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
            drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
        }
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
