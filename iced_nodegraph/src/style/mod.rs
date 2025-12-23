//! Style definitions for NodeGraph visual customization.
//!
//! This module provides style types for customizing the appearance of nodes,
//! edges, and the overall graph canvas.
//!
//! ## Cascading Style System
//!
//! The style system supports a three-layer cascade for flexible customization:
//!
//! 1. **Theme Defaults** - Base styles derived from `iced::Theme`
//! 2. **Graph Defaults** - Graph-wide overrides via [`GraphDefaults`]
//! 3. **Item Config** - Per-item overrides via [`NodeConfig`], [`EdgeConfig`], etc.
//!
//! ```rust
//! use iced_nodegraph::style::{GraphDefaults, NodeConfig, EdgeConfig};
//!
//! let defaults = GraphDefaults::new()
//!     .node(NodeConfig::new()
//!         .corner_radius(10.0)
//!         .opacity(0.8))
//!     .edge(EdgeConfig::new()
//!         .thickness(3.0));
//! ```

use iced::{Color, Theme};

// Submodules for cascading style system
mod cascade;
mod config;
mod resolver;
mod theme_defaults;

// Re-export cascading style types
pub use cascade::Cascade;
pub use config::{EdgeConfig, GraphConfig, NodeConfig, PinConfig, SelectionConfig, ShadowConfig};
pub use resolver::{GraphDefaults, StyleResolver};
pub use theme_defaults::ThemeDefaults;

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

/// Style configuration for pins.
///
/// Controls the rendering of connection points on nodes.
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
    /// Creates a new PinStyle with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the pin color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the pin radius.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Sets the pin shape.
    pub fn shape(mut self, shape: PinShape) -> Self {
        self.shape = shape;
        self
    }

    /// Sets the border color.
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    /// Sets the border width.
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    /// Removes the border.
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
}

/// Shadow configuration for nodes.
///
/// Creates a soft shadow effect beneath nodes to add depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    /// Horizontal and vertical offset in world-space pixels.
    /// Positive values move the shadow right/down.
    pub offset: (f32, f32),
    /// Blur radius in world-space pixels.
    /// Larger values create softer shadows.
    pub blur_radius: f32,
    /// Shadow color (typically semi-transparent black).
    pub color: Color,
}

impl Default for ShadowStyle {
    fn default() -> Self {
        Self {
            offset: (4.0, 4.0),
            blur_radius: 8.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        }
    }
}

impl ShadowStyle {
    /// Creates a new shadow style with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the shadow offset.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// Sets the blur radius.
    pub fn blur_radius(mut self, radius: f32) -> Self {
        self.blur_radius = radius;
        self
    }

    /// Sets the shadow color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Creates a subtle shadow preset.
    pub fn subtle() -> Self {
        Self {
            offset: (2.0, 2.0),
            blur_radius: 4.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
        }
    }

    /// Creates a medium shadow preset (default).
    pub fn medium() -> Self {
        Self::default()
    }

    /// Creates a strong shadow preset for elevated elements.
    pub fn strong() -> Self {
        Self {
            offset: (6.0, 8.0),
            blur_radius: 16.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
        }
    }

    /// Creates a glow effect (centered shadow with color).
    pub fn glow(color: Color) -> Self {
        Self {
            offset: (0.0, 0.0),
            blur_radius: 12.0,
            color: Color::from_rgba(color.r, color.g, color.b, 0.5),
        }
    }
}

/// Style configuration for a node's visual appearance.
///
/// Controls the rendering of node containers in the graph.
#[derive(Debug, Clone, PartialEq)]
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
    /// Optional drop shadow
    pub shadow: Option<ShadowStyle>,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill_color: Color::from_rgb(0.14, 0.14, 0.16),
            border_color: Color::from_rgb(0.20, 0.20, 0.22),
            border_width: 1.0,
            corner_radius: 5.0,
            opacity: 0.75,
            shadow: Some(ShadowStyle::subtle()),
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

    /// Sets the shadow style.
    pub fn shadow(mut self, shadow: ShadowStyle) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Removes the shadow.
    pub fn no_shadow(mut self) -> Self {
        self.shadow = None;
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
            shadow: Some(ShadowStyle::medium()),
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
            shadow: Some(ShadowStyle::medium()),
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
            shadow: Some(ShadowStyle::strong()),
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
            shadow: None, // Comments are subtle, no shadow
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
    /// Multiple particles flowing along edge (requires flow_speed > 0)
    pub particles: bool,
    /// Rainbow/HSV color cycling animation
    pub rainbow: bool,
}

impl Default for EdgeAnimation {
    fn default() -> Self {
        Self {
            flow_speed: 0.0,
            pulse: false,
            glow: false,
            particles: false,
            rainbow: false,
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
            particles: false,
            rainbow: false,
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

    /// Enables particles effect (multiple flowing dots).
    pub fn particles(mut self) -> Self {
        self.particles = true;
        self
    }

    /// Enables rainbow color cycling.
    pub fn rainbow(mut self) -> Self {
        self.rainbow = true;
        self
    }

    /// Creates a data flow animation (moderate speed with glow).
    pub fn data_flow() -> Self {
        Self::flow(30.0).glow()
    }

    /// Creates a particle stream animation.
    pub fn particle_stream() -> Self {
        Self::flow(60.0).particles()
    }

    /// Creates a rainbow animation (slow color cycling).
    pub fn rainbow_flow() -> Self {
        Self::flow(20.0).rainbow()
    }

    /// Creates an error animation (fast pulsing).
    pub fn error() -> Self {
        Self {
            flow_speed: 50.0,
            pulse: true,
            glow: false,
            particles: false,
            rainbow: false,
        }
    }
}

/// Style configuration for edges/connections.
///
/// Controls the rendering of connection lines between pins.
#[derive(Debug, Clone, PartialEq)]
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
    ///
    /// Flag bits:
    /// - bit 0: animated dash pattern
    /// - bit 1: glow effect
    /// - bit 2: pulse/breathing effect
    /// - bit 3: particles (multiple flowing dots)
    /// - bit 4: rainbow color cycling
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
            if anim.particles {
                flags |= 8; // bit 3: particles
            }
            if anim.rainbow {
                flags |= 16; // bit 4: rainbow
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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

/// All standard iced themes for easy enumeration in UI.
///
/// Useful for theme pickers and settings menus.
///
/// ```rust
/// use iced_nodegraph::style::{STANDARD_THEMES, theme_name};
///
/// // Iterate over all themes
/// for theme in &STANDARD_THEMES {
///     println!("{}", theme_name(theme));
/// }
///
/// // Use as a Vec for UI components
/// let themes: Vec<_> = STANDARD_THEMES.to_vec();
/// assert_eq!(themes.len(), 22);
/// ```
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
///
/// Useful for UI labels and serialization.
///
/// ```rust
/// use iced_nodegraph::style::theme_name;
/// use iced::Theme;
///
/// assert_eq!(theme_name(&Theme::CatppuccinMocha), "Catppuccin Mocha");
/// assert_eq!(theme_name(&Theme::Dark), "Dark");
/// ```
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

    #[test]
    fn test_standard_themes_count() {
        assert_eq!(STANDARD_THEMES.len(), 22);
    }

    #[test]
    fn test_theme_name() {
        assert_eq!(theme_name(&Theme::Dark), "Dark");
        assert_eq!(theme_name(&Theme::Light), "Light");
        assert_eq!(theme_name(&Theme::CatppuccinMocha), "Catppuccin Mocha");
        assert_eq!(theme_name(&Theme::Nord), "Nord");
        assert_eq!(theme_name(&Theme::Dracula), "Dracula");
    }

    #[test]
    fn test_all_standard_themes_have_names() {
        for theme in &STANDARD_THEMES {
            let name = theme_name(theme);
            assert!(!name.is_empty(), "Theme should have a name");
            assert_ne!(name, "Custom", "Standard themes should not be Custom");
        }
    }
}
