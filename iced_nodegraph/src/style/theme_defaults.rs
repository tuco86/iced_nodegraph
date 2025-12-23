//! Theme-derived default styles.
//!
//! This module provides the base layer of the style cascade by deriving
//! default styles from the iced Theme's extended palette.

use iced::{Color, Theme};

use super::{
    EdgeStyle, EdgeType, GraphStyle, NodeStyle, PinShape, PinStyle, SelectionStyle, ShadowStyle,
};

/// Complete style defaults derived from an iced Theme.
///
/// This is the base layer of the style cascade. All values are concrete
/// (no Options) and ready for GPU consumption.
///
/// # Cascade Order
/// ```text
/// ThemeDefaults (base) -> GraphDefaults (override) -> Item Config (override)
/// ```
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::ThemeDefaults;
/// use iced::Theme;
///
/// let defaults = ThemeDefaults::from_theme(&Theme::Dark);
/// // Use defaults.node, defaults.edge, etc.
/// ```
#[derive(Debug, Clone)]
pub struct ThemeDefaults {
    /// Default node style derived from theme
    pub node: NodeStyle,
    /// Default edge style derived from theme
    pub edge: EdgeStyle,
    /// Default pin style derived from theme
    pub pin: PinStyle,
    /// Default graph style derived from theme
    pub graph: GraphStyle,
}

impl ThemeDefaults {
    /// Derive complete style defaults from an iced Theme's extended palette.
    ///
    /// This extracts colors from the theme's palette to create cohesive
    /// defaults that match the current theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();

        if palette.is_dark {
            Self::dark_defaults(&palette)
        } else {
            Self::light_defaults(&palette)
        }
    }

    /// Create defaults based on dark/light mode detection.
    ///
    /// Use this when you don't have direct access to the iced Theme
    /// but can detect the theme mode via text color luminance.
    pub fn from_is_dark(is_dark: bool) -> Self {
        if is_dark {
            Self::dark_fallback()
        } else {
            Self::light_fallback()
        }
    }

    /// Dark theme defaults without palette (fallback colors).
    fn dark_fallback() -> Self {
        Self {
            node: NodeStyle {
                fill_color: Color::from_rgb(0.14, 0.14, 0.16),
                border_color: Color::from_rgb(0.20, 0.20, 0.22),
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.75,
                shadow: Some(ShadowStyle::subtle()),
            },
            edge: EdgeStyle {
                color: Color::from_rgba(0.9, 0.9, 0.9, 0.7),
                thickness: 2.0,
                edge_type: EdgeType::Bezier,
                dash_pattern: None,
                animation: None,
            },
            pin: PinStyle {
                color: Color::from_rgb(0.5, 0.5, 0.5),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: None,
                border_width: 1.0,
            },
            graph: GraphStyle {
                background_color: Color::from_rgb(0.08, 0.08, 0.09),
                grid_color: Color::from_rgb(0.20, 0.20, 0.22),
                drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
                drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
                selection_style: SelectionStyle {
                    selected_border_color: Color::from_rgb(0.3, 0.6, 1.0),
                    selected_border_width: 2.5,
                    box_select_fill: Color::from_rgba(0.3, 0.6, 1.0, 0.15),
                    box_select_border: Color::from_rgba(0.3, 0.6, 1.0, 0.6),
                },
            },
        }
    }

    /// Light theme defaults without palette (fallback colors).
    fn light_fallback() -> Self {
        Self {
            node: NodeStyle {
                fill_color: Color::from_rgb(0.96, 0.96, 0.97),
                border_color: Color::from_rgb(0.80, 0.80, 0.82),
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.85,
                shadow: Some(ShadowStyle {
                    offset: (2.0, 2.0),
                    blur_radius: 6.0,
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                }),
            },
            edge: EdgeStyle {
                color: Color::from_rgba(0.2, 0.2, 0.2, 0.6),
                thickness: 2.0,
                edge_type: EdgeType::Bezier,
                dash_pattern: None,
                animation: None,
            },
            pin: PinStyle {
                color: Color::from_rgb(0.4, 0.4, 0.4),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: Some(Color::from_rgb(0.3, 0.3, 0.3)),
                border_width: 1.0,
            },
            graph: GraphStyle {
                background_color: Color::from_rgb(0.92, 0.92, 0.93),
                grid_color: Color::from_rgb(0.80, 0.80, 0.82),
                drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
                drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
                selection_style: SelectionStyle {
                    selected_border_color: Color::from_rgb(0.2, 0.5, 0.9),
                    selected_border_width: 2.5,
                    box_select_fill: Color::from_rgba(0.2, 0.5, 0.9, 0.12),
                    box_select_border: Color::from_rgba(0.2, 0.5, 0.9, 0.5),
                },
            },
        }
    }

    /// Creates defaults for dark themes.
    fn dark_defaults(palette: &iced::theme::palette::Extended) -> Self {
        let primary = palette.primary.base.color;
        let text = palette.background.base.text;

        // Derive selection colors from primary
        let selection_color = primary;

        Self {
            node: NodeStyle {
                fill_color: Color::from_rgb(0.14, 0.14, 0.16),
                border_color: Color::from_rgb(0.20, 0.20, 0.22),
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.75,
                shadow: Some(ShadowStyle::subtle()),
            },
            edge: EdgeStyle {
                color: Color::from_rgba(text.r, text.g, text.b, 0.7),
                thickness: 2.0,
                edge_type: EdgeType::Bezier,
                dash_pattern: None,
                animation: None,
            },
            pin: PinStyle {
                color: Color::from_rgb(0.5, 0.5, 0.5),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: None,
                border_width: 1.0,
            },
            graph: GraphStyle {
                background_color: Color::from_rgb(0.08, 0.08, 0.09),
                grid_color: Color::from_rgb(0.20, 0.20, 0.22),
                drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
                drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
                selection_style: SelectionStyle {
                    selected_border_color: selection_color,
                    selected_border_width: 2.5,
                    box_select_fill: Color::from_rgba(
                        selection_color.r,
                        selection_color.g,
                        selection_color.b,
                        0.15,
                    ),
                    box_select_border: Color::from_rgba(
                        selection_color.r,
                        selection_color.g,
                        selection_color.b,
                        0.6,
                    ),
                },
            },
        }
    }

    /// Creates defaults for light themes.
    fn light_defaults(palette: &iced::theme::palette::Extended) -> Self {
        let primary = palette.primary.base.color;
        let text = palette.background.base.text;

        // Derive selection colors from primary
        let selection_color = primary;

        Self {
            node: NodeStyle {
                fill_color: Color::from_rgb(0.96, 0.96, 0.97),
                border_color: Color::from_rgb(0.80, 0.80, 0.82),
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.85,
                shadow: Some(ShadowStyle {
                    offset: (2.0, 2.0),
                    blur_radius: 6.0,
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                }),
            },
            edge: EdgeStyle {
                color: Color::from_rgba(text.r, text.g, text.b, 0.6),
                thickness: 2.0,
                edge_type: EdgeType::Bezier,
                dash_pattern: None,
                animation: None,
            },
            pin: PinStyle {
                color: Color::from_rgb(0.4, 0.4, 0.4),
                radius: 6.0,
                shape: PinShape::Circle,
                border_color: Some(Color::from_rgb(0.3, 0.3, 0.3)),
                border_width: 1.0,
            },
            graph: GraphStyle {
                background_color: Color::from_rgb(0.92, 0.92, 0.93),
                grid_color: Color::from_rgb(0.80, 0.80, 0.82),
                drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
                drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
                selection_style: SelectionStyle {
                    selected_border_color: selection_color,
                    selected_border_width: 2.5,
                    box_select_fill: Color::from_rgba(
                        selection_color.r,
                        selection_color.g,
                        selection_color.b,
                        0.12,
                    ),
                    box_select_border: Color::from_rgba(
                        selection_color.r,
                        selection_color.g,
                        selection_color.b,
                        0.5,
                    ),
                },
            },
        }
    }

    /// Creates fallback defaults without theme (dark style).
    ///
    /// Use this when no theme is available.
    pub fn fallback() -> Self {
        Self {
            node: NodeStyle::default(),
            edge: EdgeStyle::default(),
            pin: PinStyle::default(),
            graph: GraphStyle::default(),
        }
    }
}

impl Default for ThemeDefaults {
    fn default() -> Self {
        Self::fallback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_defaults() {
        let defaults = ThemeDefaults::from_theme(&Theme::Dark);

        // Should have dark background
        assert!(defaults.graph.background_color.r < 0.2);
        assert!(defaults.graph.background_color.g < 0.2);
        assert!(defaults.graph.background_color.b < 0.2);
    }

    #[test]
    fn test_light_theme_defaults() {
        let defaults = ThemeDefaults::from_theme(&Theme::Light);

        // Should have light background
        assert!(defaults.graph.background_color.r > 0.8);
        assert!(defaults.graph.background_color.g > 0.8);
        assert!(defaults.graph.background_color.b > 0.8);
    }

    #[test]
    fn test_all_standard_themes() {
        use super::super::STANDARD_THEMES;

        for theme in &STANDARD_THEMES {
            let defaults = ThemeDefaults::from_theme(theme);

            // All defaults should have valid values
            assert!(defaults.node.opacity > 0.0);
            assert!(defaults.edge.thickness > 0.0);
            assert!(defaults.pin.radius > 0.0);
        }
    }

    #[test]
    fn test_fallback() {
        let defaults = ThemeDefaults::fallback();

        // Should produce valid defaults
        assert!(defaults.node.corner_radius >= 0.0);
        assert!(defaults.edge.thickness > 0.0);
    }
}
