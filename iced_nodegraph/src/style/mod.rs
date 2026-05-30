//! Style definitions for NodeGraph visual customization.
//!
//! Node, edge, and pin styles are defined as flat, concrete structs in the
//! [`node`], [`edge`], and [`pin`] submodules and expanded by the `#[style]`
//! attribute macro into a typestate pair: `*Style<Partial>` (a user overlay with
//! `Option` per field, `None` = inherit) and `*Style<Resolved>` (the fully
//! populated form the renderer consumes). See [`mode`] for the markers and
//! [`color::ColorQuad`] for the unified color type.
//!
//! [`GraphStyle`] and [`SelectionStyle`] (canvas background, selection overlay,
//! drag-edge colors) remain plain structs; they are not per-element styles.

use iced::{Color, Theme};

mod color;
mod edge;
mod mode;
mod node;
mod pin;
mod sdf;

pub use color::ColorQuad;
pub use edge::EdgeStyle;
pub use mode::{Partial, Resolved, StyleMode};
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
                hover_glow_color: Color::from_rgb(primary.r * 0.8, primary.g * 0.8, primary.b * 0.9),
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
}
