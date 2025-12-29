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
//! ```rust
//! use iced_nodegraph::style::NodeConfig;
//! use iced::Color;
//!
//! // Define project-wide defaults
//! let my_defaults = NodeConfig::new().corner_radius(10.0).opacity(0.9);
//!
//! // Create specific config that inherits from defaults
//! let special = NodeConfig::new().fill_color(Color::from_rgb(1.0, 0.0, 0.0));
//! let merged = special.merge(&my_defaults);
//!
//! // Use with push_node_styled(), theme fills unset fields at render time
//! ```

use iced::{Color, Theme};

mod config;

// Re-export config types
pub use config::{
    BackgroundConfig, BorderConfig, EdgeConfig, EdgeShadowConfig, GraphConfig, NodeConfig,
    PinConfig, SelectionConfig, ShadowConfig, StrokeConfig,
};

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

    /// Creates a pin style derived from an iced Theme.
    ///
    /// This is the base style for pins when no custom config is provided.
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
            corner_radius: 8.0,
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
                border_color: Color::from_rgba(primary.r, primary.g, primary.b, 0.6),
                border_width: 1.5,
                corner_radius: 6.0,
                opacity: 0.85,
                shadow: Some(ShadowStyle::medium()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - primary.r) * 0.08,
                    1.0 - (1.0 - primary.g) * 0.08,
                    1.0 - (1.0 - primary.b) * 0.08,
                    1.0,
                ),
                border_color: Color::from_rgba(primary.r, primary.g, primary.b, 0.5),
                border_width: 1.5,
                corner_radius: 6.0,
                opacity: 0.90,
                shadow: Some(ShadowStyle::subtle()),
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
                border_color: Color::from_rgba(success.r, success.g, success.b, 0.5),
                border_width: 1.5,
                corner_radius: 4.0,
                opacity: 0.80,
                shadow: Some(ShadowStyle::medium()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - success.r) * 0.06,
                    1.0 - (1.0 - success.g) * 0.06,
                    1.0 - (1.0 - success.b) * 0.06,
                    1.0,
                ),
                border_color: Color::from_rgba(success.r, success.g, success.b, 0.4),
                border_width: 1.5,
                corner_radius: 4.0,
                opacity: 0.88,
                shadow: Some(ShadowStyle::subtle()),
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
                border_color: Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.7),
                border_width: 2.0,
                corner_radius: 8.0,
                opacity: 0.85,
                shadow: Some(ShadowStyle::strong()),
            }
        } else {
            Self {
                fill_color: Color::from_rgba(
                    1.0 - (1.0 - secondary.r) * 0.10,
                    1.0 - (1.0 - secondary.g) * 0.10,
                    1.0 - (1.0 - secondary.b) * 0.10,
                    1.0,
                ),
                border_color: Color::from_rgba(secondary.r, secondary.g, secondary.b, 0.6),
                border_width: 2.0,
                corner_radius: 8.0,
                opacity: 0.90,
                shadow: Some(ShadowStyle::medium()),
            }
        }
    }

    /// Creates a comment node style from theme's background weak color.
    pub fn comment_themed(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let weak = palette.background.weak.color;

        Self {
            fill_color: Color::from_rgba(weak.r, weak.g, weak.b, 0.5),
            border_color: Color::from_rgba(weak.r * 1.2, weak.g * 1.2, weak.b * 1.2, 0.4),
            border_width: 1.0,
            corner_radius: 3.0,
            opacity: 0.60,
            shadow: None,
        }
    }

    /// Creates a node style derived from an iced Theme.
    ///
    /// This is the base style for nodes when no custom config is provided.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let bg_weak = palette.background.weak.color;

        if palette.is_dark {
            // Derive node fill from background (slightly lighter)
            let node_fill = Color::from_rgba(
                bg.r + (bg_weak.r - bg.r) * 0.3,
                bg.g + (bg_weak.g - bg.g) * 0.3,
                bg.b + (bg_weak.b - bg.b) * 0.3,
                1.0,
            );

            // Derive border from weak background
            let node_border =
                Color::from_rgba(bg_weak.r * 1.2, bg_weak.g * 1.2, bg_weak.b * 1.2, 0.8);

            Self {
                fill_color: node_fill,
                border_color: node_border,
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.75,
                shadow: Some(ShadowStyle::subtle()),
            }
        } else {
            // Derive node fill from background (slightly darker for contrast)
            let node_fill = Color::from_rgba(
                bg.r - (bg.r - bg_weak.r) * 0.15,
                bg.g - (bg.g - bg_weak.g) * 0.15,
                bg.b - (bg.b - bg_weak.b) * 0.15,
                1.0,
            );

            // Derive border from weak background
            let node_border =
                Color::from_rgba(bg_weak.r * 0.9, bg_weak.g * 0.9, bg_weak.b * 0.9, 0.9);

            Self {
                fill_color: node_fill,
                border_color: node_border,
                border_width: 1.0,
                corner_radius: 5.0,
                opacity: 0.85,
                shadow: Some(ShadowStyle {
                    offset: (2.0, 2.0),
                    blur_radius: 6.0,
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                }),
            }
        }
    }
}

// ============================================================================
// Edge Curve Types
// ============================================================================

/// Edge path curve type determining the shape of the connection.
///
/// Controls how edges are rendered between pins.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EdgeCurve {
    /// Smooth cubic bezier curve (default)
    #[default]
    BezierCubic,
    /// Quadratic bezier (simpler curve, single control point)
    BezierQuadratic,
    /// Orthogonal path with sharp 90-degree corners
    Orthogonal,
    /// Orthogonal path with rounded corners
    OrthogonalSmooth {
        /// Corner radius in world-space pixels
        radius: f32,
    },
    /// Direct straight line between pins
    Line,
}

impl EdgeCurve {
    /// Returns the GPU type ID for this curve.
    pub fn type_id(&self) -> u32 {
        match self {
            EdgeCurve::BezierCubic => 0,
            EdgeCurve::BezierQuadratic => 1,
            EdgeCurve::Orthogonal => 2,
            EdgeCurve::OrthogonalSmooth { .. } => 3,
            EdgeCurve::Line => 4,
        }
    }

    /// Returns the corner radius (only applicable to OrthogonalSmooth).
    pub fn corner_radius(&self) -> f32 {
        match self {
            EdgeCurve::OrthogonalSmooth { radius } => *radius,
            _ => 0.0,
        }
    }

    /// Creates an orthogonal smooth curve with default radius.
    pub fn smooth(radius: f32) -> Self {
        EdgeCurve::OrthogonalSmooth { radius }
    }
}

// ============================================================================
// Stroke Caps
// ============================================================================

/// End cap style for stroke endpoints.
///
/// Controls how the stroke terminates at its start and end points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum StrokeCap {
    /// Rounded end caps (default)
    #[default]
    Round = 0,
    /// Square end caps extending beyond endpoint by half stroke width
    Square = 1,
    /// Pointed/triangular end caps
    Pointed = 2,
}

/// Cap style for individual dash segments in patterned lines.
///
/// Controls how each dash segment terminates within a dashed/dotted pattern.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DashCap {
    /// Flat end (no extension beyond dash length)
    #[default]
    Butt,
    /// Rounded ends (semicircle at dash endpoints)
    Round,
    /// Square ends (extends by half stroke width)
    Square,
    /// Angled ends (parallelogram style cut)
    Angled {
        /// Angle in radians from perpendicular (0 = butt, PI/4 = 45 degrees)
        angle_rad: f32,
    },
}

impl DashCap {
    /// Returns the GPU type ID for this cap.
    pub fn type_id(&self) -> u32 {
        match self {
            DashCap::Butt => 0,
            DashCap::Round => 1,
            DashCap::Square => 2,
            DashCap::Angled { .. } => 3,
        }
    }

    /// Returns the angle in radians (only applicable to Angled).
    pub fn angle(&self) -> f32 {
        match self {
            DashCap::Angled { angle_rad } => *angle_rad,
            _ => 0.0,
        }
    }
}

// ============================================================================
// Dash Motion
// ============================================================================

/// Direction of dash pattern motion along the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionDirection {
    /// Motion from source to target pin (default)
    #[default]
    Forward,
    /// Motion from target to source pin
    Backward,
}

impl MotionDirection {
    /// Returns the sign multiplier for motion calculations.
    pub fn sign(&self) -> f32 {
        match self {
            MotionDirection::Forward => 1.0,
            MotionDirection::Backward => -1.0,
        }
    }
}

/// Animation configuration for dash pattern motion.
///
/// Controls how dashed/dotted patterns animate along the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashMotion {
    /// Speed in world-space units per second
    pub speed: f32,
    /// Direction of motion along the edge
    pub direction: MotionDirection,
}

impl Default for DashMotion {
    fn default() -> Self {
        Self {
            speed: 30.0,
            direction: MotionDirection::Forward,
        }
    }
}

impl DashMotion {
    /// Creates a new motion with forward direction.
    pub fn forward(speed: f32) -> Self {
        Self {
            speed,
            direction: MotionDirection::Forward,
        }
    }

    /// Creates a new motion with backward direction.
    pub fn backward(speed: f32) -> Self {
        Self {
            speed,
            direction: MotionDirection::Backward,
        }
    }

    /// Sets the direction of motion.
    pub fn with_direction(mut self, direction: MotionDirection) -> Self {
        self.direction = direction;
        self
    }
}

// ============================================================================
// Stroke Patterns
// ============================================================================

/// Line pattern configuration for strokes.
///
/// Supports solid, dashed, dotted, and custom patterns with optional animation.
/// All lengths are in world-space units.
#[derive(Debug, Clone, PartialEq)]
pub enum StrokePattern {
    /// Continuous solid line (default)
    Solid,

    /// Dashed line with configurable dash and gap lengths
    Dashed {
        /// Length of each dash in world-space pixels
        dash: f32,
        /// Length of each gap in world-space pixels
        gap: f32,
        /// Phase offset along the pattern (shifts pattern start)
        phase: f32,
        /// Optional animation motion
        motion: Option<DashMotion>,
    },

    /// Arrowed segments (slashes/arrows) with configurable angle
    /// Creates arrow-like marks (///) crossing the edge at an angle
    Arrowed {
        /// Length of each arrow segment in world-space pixels
        segment: f32,
        /// Gap between segments in world-space pixels
        gap: f32,
        /// Angle of arrows in radians (0 = vertical, PI/4 = 45 degrees forward)
        angle: f32,
        /// Phase offset along the pattern
        phase: f32,
        /// Optional animation motion
        motion: Option<DashMotion>,
    },

    /// Dotted line with configurable spacing
    Dotted {
        /// Distance between dot centers in world-space pixels
        spacing: f32,
        /// Radius of each dot in world-space pixels
        radius: f32,
        /// Phase offset along the pattern
        phase: f32,
        /// Optional animation motion
        motion: Option<DashMotion>,
    },

    /// Dash-dot pattern (morse code style: dash-gap-dot-gap)
    DashDotted {
        /// Length of each dash in world-space pixels
        dash: f32,
        /// Gap after dash
        gap: f32,
        /// Radius of dot
        dot_radius: f32,
        /// Gap after dot
        dot_gap: f32,
        /// Phase offset
        phase: f32,
        /// Optional animation motion
        motion: Option<DashMotion>,
    },

    /// Custom pattern defined by alternating segment lengths
    Custom {
        /// Alternating lengths: [on, off, on, off, ...]
        segments: Vec<f32>,
        /// Phase offset
        phase: f32,
        /// Optional animation motion
        motion: Option<DashMotion>,
    },
}

impl Default for StrokePattern {
    fn default() -> Self {
        Self::Solid
    }
}

impl StrokePattern {
    /// Returns the GPU pattern type ID.
    ///
    /// Pattern IDs: 0=Solid, 1=Dashed, 2=Arrowed, 3=Dotted, 4=DashDotted, 5=Custom
    pub fn type_id(&self) -> u32 {
        match self {
            StrokePattern::Solid => 0,
            StrokePattern::Dashed { .. } => 1,
            StrokePattern::Arrowed { .. } => 2,
            StrokePattern::Dotted { .. } => 3,
            StrokePattern::DashDotted { .. } => 4,
            StrokePattern::Custom { .. } => 5,
        }
    }

    /// Creates a dashed pattern with given dash and gap lengths.
    pub fn dashed(dash: f32, gap: f32) -> Self {
        Self::Dashed {
            dash,
            gap,
            phase: 0.0,
            motion: None,
        }
    }

    /// Creates an arrowed pattern with arrow marks (///) crossing the edge.
    /// Angle is in radians (default PI/4 = 45 degrees forward slash).
    pub fn arrowed(segment: f32, gap: f32, angle: f32) -> Self {
        Self::Arrowed {
            segment,
            gap,
            angle,
            phase: 0.0,
            motion: None,
        }
    }

    /// Creates a dotted pattern with given spacing and radius.
    pub fn dotted(spacing: f32, radius: f32) -> Self {
        Self::Dotted {
            spacing,
            radius,
            phase: 0.0,
            motion: None,
        }
    }

    /// Creates a dash-dot pattern.
    pub fn dash_dotted(dash: f32, gap: f32, dot_radius: f32, dot_gap: f32) -> Self {
        Self::DashDotted {
            dash,
            gap,
            dot_radius,
            dot_gap,
            phase: 0.0,
            motion: None,
        }
    }

    /// Creates an animated marching ants pattern.
    pub fn marching_ants() -> Self {
        Self::Dashed {
            dash: 6.0,
            gap: 4.0,
            phase: 0.0,
            motion: Some(DashMotion::forward(30.0)),
        }
    }

    /// Adds motion animation to the pattern.
    pub fn with_motion(mut self, speed: f32) -> Self {
        let motion = DashMotion::forward(speed);
        match &mut self {
            Self::Solid => {} // No effect on solid
            Self::Dashed { motion: m, .. } => *m = Some(motion),
            Self::Arrowed { motion: m, .. } => *m = Some(motion),
            Self::Dotted { motion: m, .. } => *m = Some(motion),
            Self::DashDotted { motion: m, .. } => *m = Some(motion),
            Self::Custom { motion: m, .. } => *m = Some(motion),
        }
        self
    }

    /// Adds motion with specific direction.
    pub fn with_motion_dir(mut self, speed: f32, direction: MotionDirection) -> Self {
        let motion = DashMotion { speed, direction };
        match &mut self {
            Self::Solid => {}
            Self::Dashed { motion: m, .. } => *m = Some(motion),
            Self::Arrowed { motion: m, .. } => *m = Some(motion),
            Self::Dotted { motion: m, .. } => *m = Some(motion),
            Self::DashDotted { motion: m, .. } => *m = Some(motion),
            Self::Custom { motion: m, .. } => *m = Some(motion),
        }
        self
    }

    /// Sets the phase offset.
    pub fn with_phase(mut self, new_phase: f32) -> Self {
        match &mut self {
            Self::Solid => {}
            Self::Dashed { phase, .. } => *phase = new_phase,
            Self::Arrowed { phase, .. } => *phase = new_phase,
            Self::Dotted { phase, .. } => *phase = new_phase,
            Self::DashDotted { phase, .. } => *phase = new_phase,
            Self::Custom { phase, .. } => *phase = new_phase,
        }
        self
    }

    /// Returns the motion if any.
    pub fn motion(&self) -> Option<&DashMotion> {
        match self {
            Self::Solid => None,
            Self::Dashed { motion, .. } => motion.as_ref(),
            Self::Arrowed { motion, .. } => motion.as_ref(),
            Self::Dotted { motion, .. } => motion.as_ref(),
            Self::DashDotted { motion, .. } => motion.as_ref(),
            Self::Custom { motion, .. } => motion.as_ref(),
        }
    }

    /// Returns the phase offset.
    pub fn phase(&self) -> f32 {
        match self {
            Self::Solid => 0.0,
            Self::Dashed { phase, .. } => *phase,
            Self::Arrowed { phase, .. } => *phase,
            Self::Dotted { phase, .. } => *phase,
            Self::DashDotted { phase, .. } => *phase,
            Self::Custom { phase, .. } => *phase,
        }
    }

    /// Returns the primary pattern parameters for GPU (param1, param2).
    /// For Dashed: (dash, gap), Dotted: (spacing, radius), etc.
    pub fn params(&self) -> (f32, f32) {
        match self {
            Self::Solid => (0.0, 0.0),
            Self::Dashed { dash, gap, .. } => (*dash, *gap),
            Self::Arrowed { segment, gap, .. } => (*segment, *gap),
            Self::Dotted {
                spacing, radius, ..
            } => (*spacing, *radius),
            Self::DashDotted { dash, gap, .. } => (*dash, *gap),
            Self::Custom { segments, .. } => {
                // Return first two segments or zeros
                let p1 = segments.first().copied().unwrap_or(0.0);
                let p2 = segments.get(1).copied().unwrap_or(0.0);
                (p1, p2)
            }
        }
    }

    /// Returns the angle in radians (only applicable to Arrowed pattern).
    pub fn angle(&self) -> f32 {
        match self {
            Self::Arrowed { angle, .. } => *angle,
            _ => 0.0,
        }
    }
}

// ============================================================================
// Stroke Style
// ============================================================================

/// Stroke layer configuration for edges.
///
/// Controls the main visible line of an edge including color, width, pattern, and caps.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// Line width in world-space pixels
    pub width: f32,
    /// Color at start of edge (t=0). TRANSPARENT = use source pin color.
    pub start_color: Color,
    /// Color at end of edge (t=1). TRANSPARENT = use target pin color.
    pub end_color: Color,
    /// Line pattern (solid, dashed, dotted, etc.)
    pub pattern: StrokePattern,
    /// End cap style for the stroke endpoints
    pub cap: StrokeCap,
    /// Cap style for individual dash segments (when using patterns)
    pub dash_cap: DashCap,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 2.0,
            start_color: Color::TRANSPARENT,
            end_color: Color::TRANSPARENT,
            pattern: StrokePattern::Solid,
            cap: StrokeCap::Round,
            dash_cap: DashCap::Butt,
        }
    }
}

impl StrokeStyle {
    /// Creates a new stroke with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the stroke width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sets a solid color (both start and end).
    pub fn color(mut self, color: Color) -> Self {
        self.start_color = color;
        self.end_color = color;
        self
    }

    /// Sets the start color (at source pin).
    pub fn start_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self
    }

    /// Sets the end color (at target pin).
    pub fn end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    /// Creates a gradient from one color to another.
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

    /// Sets the line pattern.
    pub fn pattern(mut self, pattern: StrokePattern) -> Self {
        self.pattern = pattern;
        self
    }

    /// Sets the end cap style.
    pub fn cap(mut self, cap: StrokeCap) -> Self {
        self.cap = cap;
        self
    }

    /// Sets the dash cap style.
    pub fn dash_cap(mut self, dash_cap: DashCap) -> Self {
        self.dash_cap = dash_cap;
        self
    }

    /// Makes this a dashed stroke.
    pub fn dashed(mut self, dash: f32, gap: f32) -> Self {
        self.pattern = StrokePattern::dashed(dash, gap);
        self
    }

    /// Makes this a dotted stroke.
    pub fn dotted(mut self, spacing: f32, radius: f32) -> Self {
        self.pattern = StrokePattern::dotted(spacing, radius);
        self
    }

    /// Applies a StrokeConfig, returning a new StrokeStyle.
    /// Config values override base values.
    pub fn with_config(&self, config: &crate::style::config::StrokeConfig) -> Self {
        Self {
            width: config.width.unwrap_or(self.width),
            start_color: config.start_color.unwrap_or(self.start_color),
            end_color: config.end_color.unwrap_or(self.end_color),
            pattern: config
                .pattern
                .clone()
                .unwrap_or_else(|| self.pattern.clone()),
            cap: config.cap.unwrap_or(self.cap),
            dash_cap: config
                .dash_cap
                .clone()
                .unwrap_or_else(|| self.dash_cap.clone()),
        }
    }
}

// ============================================================================
// Border Style
// ============================================================================

/// Border layer configuration for edges.
///
/// Draws an outer ring around the stroke for emphasis or contrast.
/// The border is rendered behind the stroke.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderStyle {
    /// Border width in world-space pixels
    pub width: f32,
    /// Radial gap between stroke outer edge and border inner edge
    pub gap: f32,
    /// Border color
    pub color: Color,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            gap: 0.5,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
        }
    }
}

impl BorderStyle {
    /// Creates a new border with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the border width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sets the gap between stroke and border.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Sets the border color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Applies a BorderConfig, returning a new BorderStyle.
    /// Config values override base values.
    pub fn with_config(&self, config: &crate::style::config::BorderConfig) -> Self {
        Self {
            width: config.width.unwrap_or(self.width),
            gap: config.gap.unwrap_or(self.gap),
            color: config.color.unwrap_or(self.color),
        }
    }
}

// ============================================================================
// Edge Shadow Style
// ============================================================================

/// Shadow style for edges.
///
/// Creates a soft shadow effect beneath edges for depth and emphasis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeShadowStyle {
    /// Blur radius in world-space pixels.
    /// Larger values create softer shadows.
    pub blur: f32,
    /// Shadow color (typically semi-transparent).
    pub color: Color,
    /// Horizontal and vertical offset in world-space pixels.
    pub offset: (f32, f32),
}

impl Default for EdgeShadowStyle {
    fn default() -> Self {
        Self {
            blur: 4.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: (2.0, 2.0),
        }
    }
}

impl EdgeShadowStyle {
    /// Creates a new shadow with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the blur radius.
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    /// Sets the shadow color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the shadow offset.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// Creates a subtle shadow preset.
    pub fn subtle() -> Self {
        Self {
            blur: 2.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: (1.0, 1.0),
        }
    }

    /// Creates a medium shadow preset (default).
    pub fn medium() -> Self {
        Self::default()
    }

    /// Creates a strong shadow preset.
    pub fn strong() -> Self {
        Self {
            blur: 8.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: (3.0, 3.0),
        }
    }

    /// Creates a glow effect (centered, no offset).
    pub fn glow(color: Color) -> Self {
        Self {
            blur: 6.0,
            color: Color::from_rgba(color.r, color.g, color.b, 0.4),
            offset: (0.0, 0.0),
        }
    }

    /// Applies an EdgeShadowConfig, overriding fields where config has values.
    pub fn with_config(&self, config: &EdgeShadowConfig) -> Self {
        Self {
            blur: config.blur.unwrap_or(self.blur),
            color: config.color.unwrap_or(self.color),
            offset: (
                config.offset_x.unwrap_or(self.offset.0),
                config.offset_y.unwrap_or(self.offset.1),
            ),
        }
    }
}

// ============================================================================
// Edge Style (Layer-based composition)
// ============================================================================

/// Style configuration for edges/connections with layer-based composition.
///
/// Edges consist of optional layers that can be combined:
/// - **Stroke**: The main visible line with color, pattern, and caps
/// - **Border**: An outer ring around the stroke for emphasis/contrast
/// - **Shadow**: Soft shadow for depth
/// - **Curve**: The path shape (bezier, orthogonal, line)
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{EdgeStyle, StrokeStyle, BorderStyle, EdgeCurve, StrokePattern};
/// use iced::Color;
///
/// // Simple solid edge using pin colors
/// let simple = EdgeStyle::new();
///
/// // Edge with explicit color and dashed pattern
/// let dashed = EdgeStyle::new()
///     .stroke(StrokeStyle::new()
///         .width(2.0)
///         .color(Color::WHITE)
///         .pattern(StrokePattern::dashed(12.0, 6.0)));
///
/// // Edge with border for emphasis
/// let emphasized = EdgeStyle::new()
///     .stroke(StrokeStyle::new().width(2.5).color(Color::from_rgb(0.3, 0.6, 1.0)))
///     .border(BorderStyle::new().width(1.5).color(Color::from_rgba(0.0, 0.0, 0.0, 0.5)));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeStyle {
    /// Main stroke layer (None = invisible edge, useful for grouping)
    pub stroke: Option<StrokeStyle>,
    /// Optional border layer drawn behind stroke
    pub border: Option<BorderStyle>,
    /// Optional shadow layer drawn behind everything
    pub shadow: Option<EdgeShadowStyle>,
    /// Path shape for the edge
    pub curve: EdgeCurve,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            stroke: Some(StrokeStyle::default()),
            border: None,
            shadow: None,
            curve: EdgeCurve::default(),
        }
    }
}

impl EdgeStyle {
    /// Creates a new edge style with defaults (solid stroke, no border, bezier curve).
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an invisible edge (useful for grouping/layout).
    pub fn invisible() -> Self {
        Self {
            stroke: None,
            border: None,
            shadow: None,
            curve: EdgeCurve::BezierCubic,
        }
    }

    /// Sets the stroke layer.
    pub fn stroke(mut self, stroke: StrokeStyle) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Removes the stroke layer (makes edge invisible).
    pub fn no_stroke(mut self) -> Self {
        self.stroke = None;
        self
    }

    /// Sets the border layer.
    pub fn border(mut self, border: BorderStyle) -> Self {
        self.border = Some(border);
        self
    }

    /// Removes the border layer.
    pub fn no_border(mut self) -> Self {
        self.border = None;
        self
    }

    /// Sets the shadow layer.
    pub fn shadow(mut self, shadow: EdgeShadowStyle) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Removes the shadow layer.
    pub fn no_shadow(mut self) -> Self {
        self.shadow = None;
        self
    }

    /// Sets the curve type.
    pub fn curve(mut self, curve: EdgeCurve) -> Self {
        self.curve = curve;
        self
    }

    // === Convenience Methods (operate on stroke layer) ===

    /// Sets a solid color for the entire edge (overrides stroke colors).
    pub fn solid_color(mut self, color: Color) -> Self {
        if let Some(ref mut stroke) = self.stroke {
            stroke.start_color = color;
            stroke.end_color = color;
        }
        self
    }

    /// Sets a gradient from start to end color.
    pub fn gradient(mut self, start: Color, end: Color) -> Self {
        if let Some(ref mut stroke) = self.stroke {
            stroke.start_color = start;
            stroke.end_color = end;
        }
        self
    }

    /// Uses pin colors for gradient (default behavior).
    pub fn from_pins(mut self) -> Self {
        if let Some(ref mut stroke) = self.stroke {
            stroke.start_color = Color::TRANSPARENT;
            stroke.end_color = Color::TRANSPARENT;
        }
        self
    }

    /// Sets the stroke width.
    pub fn width(mut self, width: f32) -> Self {
        if let Some(ref mut stroke) = self.stroke {
            stroke.width = width;
        }
        self
    }

    /// Alias for width (backwards compatibility).
    pub fn thickness(self, thickness: f32) -> Self {
        self.width(thickness)
    }

    /// Sets the stroke pattern.
    pub fn pattern(mut self, pattern: StrokePattern) -> Self {
        if let Some(ref mut stroke) = self.stroke {
            stroke.pattern = pattern;
        }
        self
    }

    // === Preset Styles ===

    /// Creates a data flow style (blue, bezier curve).
    pub fn data_flow() -> Self {
        let color = Color::from_rgb(0.3, 0.6, 1.0);
        Self::new()
            .stroke(StrokeStyle::new().width(2.5).color(color))
            .curve(EdgeCurve::BezierCubic)
    }

    /// Creates a control flow style (white, orthogonal with rounded corners).
    pub fn control_flow() -> Self {
        Self::new()
            .stroke(StrokeStyle::new().width(2.0).color(Color::WHITE))
            .curve(EdgeCurve::OrthogonalSmooth { radius: 15.0 })
    }

    /// Creates an error style (red, animated marching ants with border).
    pub fn error() -> Self {
        let color = Color::from_rgb(0.9, 0.2, 0.2);
        Self::new()
            .stroke(
                StrokeStyle::new()
                    .width(2.0)
                    .color(color)
                    .pattern(StrokePattern::marching_ants()),
            )
            .border(BorderStyle::new().width(1.0).gap(0.5).color(color))
            .curve(EdgeCurve::BezierCubic)
    }

    /// Creates a disabled style (gray, dashed).
    pub fn disabled() -> Self {
        let color = Color::from_rgb(0.5, 0.5, 0.5);
        Self::new()
            .stroke(
                StrokeStyle::new()
                    .width(1.5)
                    .color(color)
                    .pattern(StrokePattern::dashed(12.0, 6.0)),
            )
            .curve(EdgeCurve::BezierCubic)
    }

    /// Creates a highlighted style (bright, with border).
    pub fn highlighted() -> Self {
        let color = Color::from_rgb(1.0, 0.8, 0.2);
        Self::new()
            .stroke(StrokeStyle::new().width(3.0).color(color))
            .border(
                BorderStyle::new()
                    .width(2.0)
                    .gap(1.0)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.3)),
            )
            .curve(EdgeCurve::BezierCubic)
    }

    /// Creates a debug/temporary style (dotted, cyan, straight line).
    pub fn debug() -> Self {
        Self::new()
            .stroke(
                StrokeStyle::new()
                    .width(1.5)
                    .color(Color::from_rgb(0.0, 1.0, 1.0))
                    .pattern(StrokePattern::dotted(8.0, 2.0)),
            )
            .curve(EdgeCurve::Line)
    }

    // === GPU Helper Methods ===

    /// Returns whether this edge has an animated pattern.
    pub fn has_motion(&self) -> bool {
        self.stroke
            .as_ref()
            .map(|s| s.pattern.motion().is_some())
            .unwrap_or(false)
    }

    /// Gets the motion speed (0.0 if no motion).
    pub fn motion_speed(&self) -> f32 {
        self.stroke
            .as_ref()
            .and_then(|s| s.pattern.motion())
            .map(|m| m.speed)
            .unwrap_or(0.0)
    }

    /// Gets the motion direction sign (1.0 forward, -1.0 backward).
    pub fn motion_direction_sign(&self) -> f32 {
        self.stroke
            .as_ref()
            .and_then(|s| s.pattern.motion())
            .map(|m| m.direction.sign())
            .unwrap_or(1.0)
    }

    /// Returns GPU flags for this edge style.
    /// - bit 0: has motion (animated pattern)
    /// Note: bit 1/2/3 reserved for glow/pulse/pending_cut (set by shader pipeline)
    /// Border rendering is controlled by border_width > 0, not by a flag.
    pub fn flags(&self) -> u32 {
        let mut flags = 0u32;
        if self.has_motion() {
            flags |= 1;
        }
        // Note: border is NOT a flag - it's rendered when border_width > 0.0
        // Setting bit 1 here would incorrectly trigger the GLOW effect in shader.
        flags
    }

    // === Getter Methods ===

    /// Gets the stroke start color, or TRANSPARENT if no stroke.
    pub fn start_color(&self) -> Color {
        self.stroke
            .as_ref()
            .map(|s| s.start_color)
            .unwrap_or(Color::TRANSPARENT)
    }

    /// Gets the stroke end color, or TRANSPARENT if no stroke.
    pub fn end_color(&self) -> Color {
        self.stroke
            .as_ref()
            .map(|s| s.end_color)
            .unwrap_or(Color::TRANSPARENT)
    }

    /// Gets the stroke width, or 2.0 (default) if no stroke.
    pub fn get_width(&self) -> f32 {
        self.stroke.as_ref().map(|s| s.width).unwrap_or(2.0)
    }

    /// Merges an EdgeConfig into this style, returning a new style.
    /// Config values override base style values.
    pub fn with_config(&self, config: &EdgeConfig) -> Self {
        let stroke = match (&self.stroke, &config.stroke) {
            (Some(base), Some(cfg)) => Some(base.with_config(cfg)),
            (Some(base), None) => Some(base.clone()),
            (None, Some(cfg)) => Some(StrokeStyle::default().with_config(cfg)),
            (None, None) => None,
        };

        let border = match (&self.border, &config.border) {
            (_, Some(cfg)) if cfg.enabled == Some(false) => None,
            (Some(base), Some(cfg)) => Some(base.with_config(cfg)),
            (Some(base), None) => Some(base.clone()),
            (None, Some(cfg)) if cfg.enabled != Some(false) => {
                Some(BorderStyle::default().with_config(cfg))
            }
            (None, _) => None,
        };

        let shadow = match (&self.shadow, &config.shadow) {
            (_, Some(cfg)) if cfg.enabled == Some(false) => None,
            (Some(base), Some(cfg)) => Some(base.with_config(cfg)),
            (Some(base), None) => Some(*base),
            (None, Some(cfg)) if cfg.enabled != Some(false) => {
                Some(EdgeShadowStyle::default().with_config(cfg))
            }
            (None, _) => None,
        };

        Self {
            stroke,
            border,
            shadow,
            curve: config.curve.unwrap_or(self.curve),
        }
    }

    /// Creates an edge style derived from an iced Theme.
    ///
    /// This is the base style for edges when no custom config is provided.
    /// Uses transparent colors to inherit from pin colors.
    pub fn from_theme(_theme: &Theme) -> Self {
        // Edge defaults are theme-independent: use pin colors for gradient
        Self::new()
    }
}

// ============================================================================
// Background Pattern System
// ============================================================================

/// Background pattern type for the graph canvas.
///
/// Each pattern supports adaptive zoom behavior where spacing automatically
/// adjusts to maintain visual clarity at different zoom levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum BackgroundPattern {
    /// No pattern, solid background color only
    None = 0,
    /// Rectangular grid with major/minor lines (default)
    #[default]
    Grid = 1,
    /// Hexagonal honeycomb pattern
    Hex = 2,
    /// Equilateral triangle tessellation
    Triangle = 3,
    /// Regular dot pattern
    Dots = 4,
    /// Parallel diagonal lines
    Lines = 5,
    /// Crosshatch (intersecting diagonal lines)
    Crosshatch = 6,
}

impl BackgroundPattern {
    /// Returns the GPU type ID for this pattern.
    pub fn type_id(&self) -> u32 {
        *self as u32
    }
}

/// Complete background style configuration.
///
/// Controls the rendering of the graph canvas background including pattern,
/// colors, spacing, line widths, and adaptive zoom behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundStyle {
    /// Pattern type (Grid, Hex, Triangle, Dots, Lines, Crosshatch, None)
    pub pattern: BackgroundPattern,

    // === Colors ===
    /// Background fill color
    pub background_color: Color,
    /// Primary pattern color (major lines/elements)
    pub primary_color: Color,
    /// Secondary pattern color (minor lines/elements)
    pub secondary_color: Color,

    // === Spacing ===
    /// Minor grid/pattern spacing in world-space pixels
    pub minor_spacing: f32,
    /// Major grid spacing (typically multiple of minor_spacing).
    /// None = no major grid distinction
    pub major_spacing: Option<f32>,

    // === Line Properties ===
    /// Minor line width in world-space pixels
    pub minor_width: f32,
    /// Major line width in world-space pixels
    pub major_width: f32,
    /// Minor line opacity (0.0 - 1.0)
    pub minor_opacity: f32,
    /// Major line opacity (0.0 - 1.0)
    pub major_opacity: f32,

    // === Pattern-Specific Options ===
    /// Dot radius (for Dots pattern) in world-space pixels
    pub dot_radius: f32,
    /// Line angle in radians (for Lines/Crosshatch patterns).
    /// 0 = horizontal, PI/4 = 45 degrees
    pub line_angle: f32,
    /// Secondary line angle (for Crosshatch, typically -line_angle)
    pub crosshatch_angle: f32,
    /// Hex orientation: true = pointy-top, false = flat-top
    pub hex_pointy_top: bool,

    // === Adaptive Zoom ===
    /// Enable adaptive spacing that adjusts with zoom level
    pub adaptive_zoom: bool,
    /// Minimum screen-space spacing before pattern doubles (prevents too-dense patterns)
    pub adaptive_min_spacing: f32,
    /// Maximum screen-space spacing before pattern halves (prevents too-sparse patterns)
    pub adaptive_max_spacing: f32,
    /// Fade range for minor elements at zoom extremes (0.0 = no fade)
    pub adaptive_fade_range: f32,
}

impl Default for BackgroundStyle {
    fn default() -> Self {
        Self {
            pattern: BackgroundPattern::Grid,
            background_color: Color::from_rgb(0.08, 0.08, 0.09),
            primary_color: Color::from_rgba(1.0, 1.0, 1.0, 0.12),
            secondary_color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
            minor_spacing: 50.0,
            major_spacing: Some(250.0), // Every 5th line is major
            minor_width: 1.0,
            major_width: 2.0,
            minor_opacity: 0.35,
            major_opacity: 0.7,
            dot_radius: 2.0,
            line_angle: std::f32::consts::FRAC_PI_4, // 45 degrees
            crosshatch_angle: -std::f32::consts::FRAC_PI_4, // -45 degrees
            hex_pointy_top: true,
            adaptive_zoom: true,
            adaptive_min_spacing: 20.0, // Double when spacing < 20px on screen
            adaptive_max_spacing: 200.0, // Halve when spacing > 200px on screen
            adaptive_fade_range: 0.3,   // Fade minor lines over 30% of threshold
        }
    }
}

impl BackgroundStyle {
    /// Creates a new BackgroundStyle with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the pattern type.
    pub fn pattern(mut self, pattern: BackgroundPattern) -> Self {
        self.pattern = pattern;
        self
    }

    /// Sets the background color.
    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Sets the primary pattern color (major lines/elements).
    pub fn primary_color(mut self, color: Color) -> Self {
        self.primary_color = color;
        self
    }

    /// Sets the secondary pattern color (minor lines/elements).
    pub fn secondary_color(mut self, color: Color) -> Self {
        self.secondary_color = color;
        self
    }

    /// Sets the minor grid spacing.
    pub fn minor_spacing(mut self, spacing: f32) -> Self {
        self.minor_spacing = spacing;
        self
    }

    /// Sets the major grid spacing.
    pub fn major_spacing(mut self, spacing: f32) -> Self {
        self.major_spacing = Some(spacing);
        self
    }

    /// Disables major grid distinction.
    pub fn no_major_grid(mut self) -> Self {
        self.major_spacing = None;
        self
    }

    /// Sets the minor line width.
    pub fn minor_width(mut self, width: f32) -> Self {
        self.minor_width = width;
        self
    }

    /// Sets the major line width.
    pub fn major_width(mut self, width: f32) -> Self {
        self.major_width = width;
        self
    }

    /// Sets the minor line opacity.
    pub fn minor_opacity(mut self, opacity: f32) -> Self {
        self.minor_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Sets the major line opacity.
    pub fn major_opacity(mut self, opacity: f32) -> Self {
        self.major_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Sets the dot radius (for Dots pattern).
    pub fn dot_radius(mut self, radius: f32) -> Self {
        self.dot_radius = radius;
        self
    }

    /// Sets the line angle in radians (for Lines/Crosshatch patterns).
    pub fn line_angle(mut self, angle_rad: f32) -> Self {
        self.line_angle = angle_rad;
        self
    }

    /// Sets the crosshatch secondary angle.
    pub fn crosshatch_angle(mut self, angle_rad: f32) -> Self {
        self.crosshatch_angle = angle_rad;
        self
    }

    /// Sets hex orientation (true = pointy-top, false = flat-top).
    pub fn hex_pointy_top(mut self, pointy: bool) -> Self {
        self.hex_pointy_top = pointy;
        self
    }

    /// Enables or disables adaptive zoom.
    pub fn adaptive_zoom(mut self, enabled: bool) -> Self {
        self.adaptive_zoom = enabled;
        self
    }

    /// Sets the adaptive zoom thresholds.
    pub fn adaptive_thresholds(mut self, min: f32, max: f32) -> Self {
        self.adaptive_min_spacing = min;
        self.adaptive_max_spacing = max;
        self
    }

    /// Sets the adaptive fade range.
    pub fn adaptive_fade(mut self, range: f32) -> Self {
        self.adaptive_fade_range = range.clamp(0.0, 1.0);
        self
    }

    // === Presets ===

    /// Blueprint-style grid (blue on dark blue).
    pub fn blueprint() -> Self {
        Self {
            pattern: BackgroundPattern::Grid,
            background_color: Color::from_rgb(0.05, 0.08, 0.15),
            primary_color: Color::from_rgba(0.3, 0.5, 0.8, 0.25),
            secondary_color: Color::from_rgba(0.3, 0.5, 0.8, 0.10),
            minor_spacing: 25.0,
            major_spacing: Some(100.0),
            ..Default::default()
        }
    }

    /// Subtle dots pattern.
    pub fn subtle_dots() -> Self {
        Self {
            pattern: BackgroundPattern::Dots,
            background_color: Color::from_rgb(0.12, 0.12, 0.14),
            primary_color: Color::from_rgba(1.0, 1.0, 1.0, 0.15),
            secondary_color: Color::TRANSPARENT,
            minor_spacing: 30.0,
            major_spacing: None,
            dot_radius: 1.5,
            ..Default::default()
        }
    }

    /// Clean hexagonal pattern.
    pub fn hexagonal() -> Self {
        Self {
            pattern: BackgroundPattern::Hex,
            background_color: Color::from_rgb(0.10, 0.10, 0.12),
            primary_color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            secondary_color: Color::TRANSPARENT,
            minor_spacing: 40.0,
            major_spacing: None,
            ..Default::default()
        }
    }

    /// Light theme grid.
    pub fn light() -> Self {
        Self {
            pattern: BackgroundPattern::Grid,
            background_color: Color::from_rgb(0.95, 0.95, 0.96),
            primary_color: Color::from_rgba(0.0, 0.0, 0.0, 0.10),
            secondary_color: Color::from_rgba(0.0, 0.0, 0.0, 0.04),
            ..Default::default()
        }
    }

    /// Creates a background style from an iced Theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let bg = palette.background.base.color;
        let weak = palette.background.weak.color;

        if palette.is_dark {
            Self {
                background_color: Color::from_rgb(bg.r * 0.7, bg.g * 0.7, bg.b * 0.7),
                primary_color: Color::from_rgba(weak.r, weak.g, weak.b, 0.15),
                secondary_color: Color::from_rgba(weak.r, weak.g, weak.b, 0.06),
                ..Default::default()
            }
        } else {
            Self {
                background_color: Color::from_rgb(
                    bg.r * 0.98 + 0.02,
                    bg.g * 0.98 + 0.02,
                    bg.b * 0.98 + 0.02,
                ),
                primary_color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                secondary_color: Color::from_rgba(0.0, 0.0, 0.0, 0.03),
                ..Default::default()
            }
        }
    }
}

/// Complete graph style configuration.
///
/// Controls the appearance of the graph canvas background and drag feedback.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStyle {
    /// Background rendering style
    pub background: BackgroundStyle,
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
            background: BackgroundStyle::default(),
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

    /// Sets the background style.
    pub fn background(mut self, background: BackgroundStyle) -> Self {
        self.background = background;
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
            background: BackgroundStyle::default(),
            drag_edge_color: Color::from_rgb(0.9, 0.6, 0.3),
            drag_edge_valid_color: Color::from_rgb(0.3, 0.8, 0.5),
            selection_style: SelectionStyle::default(),
        }
    }

    /// Creates a light theme graph style.
    pub fn light() -> Self {
        Self {
            background: BackgroundStyle::light(),
            drag_edge_color: Color::from_rgb(0.8, 0.5, 0.2),
            drag_edge_valid_color: Color::from_rgb(0.2, 0.7, 0.4),
            selection_style: SelectionStyle::default(),
        }
    }

    /// Creates a graph style derived from an iced Theme.
    ///
    /// Automatically selects dark or light mode based on the theme's palette.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let secondary = palette.secondary.base.color;
        let success = palette.success.base.color;

        if palette.is_dark {
            Self {
                background: BackgroundStyle::from_theme(theme),
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
                background: BackgroundStyle::from_theme(theme),
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

/// Style configuration for node selection highlighting.
///
/// Controls the visual appearance of selected nodes and the box selection rectangle.
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
            }
        } else {
            Self {
                selected_border_color: primary,
                selected_border_width: 2.5,
                box_select_fill: Color::from_rgba(primary.r, primary.g, primary.b, 0.12),
                box_select_border: Color::from_rgba(primary.r, primary.g, primary.b, 0.5),
            }
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

/// Lightens a color by mixing with white.
///
/// `amount` ranges from 0.0 (no change) to 1.0 (full white).
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::lighten;
/// use iced::Color;
///
/// let blue = Color::from_rgb(0.0, 0.0, 1.0);
/// let light_blue = lighten(blue, 0.5);
/// // Result: RGB(0.5, 0.5, 1.0)
/// ```
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
///
/// `amount` ranges from 0.0 (no change) to 1.0 (full black).
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::darken;
/// use iced::Color;
///
/// let white = Color::WHITE;
/// let gray = darken(white, 0.5);
/// // Result: RGB(0.5, 0.5, 0.5)
/// ```
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
///
/// Preserves RGB values and replaces alpha.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::with_alpha;
/// use iced::Color;
///
/// let blue = Color::from_rgb(0.0, 0.0, 1.0);
/// let semi_blue = with_alpha(blue, 0.5);
/// // Result: RGBA(0.0, 0.0, 1.0, 0.5)
/// ```
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha.clamp(0.0, 1.0))
}

/// Blends two colors together.
///
/// `ratio` controls the blend: 0.0 = full `a`, 1.0 = full `b`.
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

    #[test]
    fn test_edge_style_default_uses_pin_colors() {
        let style = EdgeStyle::default();
        let stroke = style.stroke.unwrap();
        // TRANSPARENT means "use pin colors"
        assert!(stroke.start_color.a < 0.01);
        assert!(stroke.end_color.a < 0.01);
    }

    #[test]
    fn test_edge_style_solid_color() {
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let style = EdgeStyle::new().solid_color(red);
        let stroke = style.stroke.unwrap();

        assert_eq!(stroke.start_color, red);
        assert_eq!(stroke.end_color, red);
    }

    #[test]
    fn test_edge_style_gradient() {
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let style = EdgeStyle::new().gradient(red, blue);
        let stroke = style.stroke.unwrap();

        assert_eq!(stroke.start_color, red);
        assert_eq!(stroke.end_color, blue);
    }

    #[test]
    fn test_edge_style_from_pins() {
        let style = EdgeStyle::new().solid_color(Color::WHITE).from_pins();
        let stroke = style.stroke.unwrap();

        assert!(stroke.start_color.a < 0.01);
        assert!(stroke.end_color.a < 0.01);
    }

    #[test]
    fn test_edge_style_presets_are_solid() {
        // All presets should have explicit colors (not gradients)
        let data_flow = EdgeStyle::data_flow();
        let stroke = data_flow.stroke.unwrap();
        assert_eq!(stroke.start_color, stroke.end_color);

        let control_flow = EdgeStyle::control_flow();
        let stroke = control_flow.stroke.unwrap();
        assert_eq!(stroke.start_color, stroke.end_color);

        let error = EdgeStyle::error();
        let stroke = error.stroke.unwrap();
        assert_eq!(stroke.start_color, stroke.end_color);

        let disabled = EdgeStyle::disabled();
        let stroke = disabled.stroke.unwrap();
        assert_eq!(stroke.start_color, stroke.end_color);

        let highlighted = EdgeStyle::highlighted();
        let stroke = highlighted.stroke.unwrap();
        assert_eq!(stroke.start_color, stroke.end_color);
    }

    #[test]
    fn test_edge_curve_type_ids() {
        assert_eq!(EdgeCurve::BezierCubic.type_id(), 0);
        assert_eq!(EdgeCurve::BezierQuadratic.type_id(), 1);
        assert_eq!(EdgeCurve::Orthogonal.type_id(), 2);
        assert_eq!(EdgeCurve::OrthogonalSmooth { radius: 15.0 }.type_id(), 3);
        assert_eq!(EdgeCurve::Line.type_id(), 4);
    }

    #[test]
    fn test_stroke_pattern_type_ids() {
        // Pattern IDs: 0=Solid, 1=Dashed, 2=Arrowed, 3=Dotted, 4=DashDotted, 5=Custom
        assert_eq!(StrokePattern::Solid.type_id(), 0);
        assert_eq!(StrokePattern::dashed(10.0, 5.0).type_id(), 1);
        assert_eq!(
            StrokePattern::arrowed(8.0, 4.0, std::f32::consts::FRAC_PI_4).type_id(),
            2
        );
        assert_eq!(StrokePattern::dotted(8.0, 2.0).type_id(), 3);
        assert_eq!(StrokePattern::dash_dotted(10.0, 5.0, 2.0, 5.0).type_id(), 4);
    }

    #[test]
    fn test_stroke_pattern_angled() {
        let pattern = StrokePattern::arrowed(10.0, 5.0, std::f32::consts::FRAC_PI_4);
        assert_eq!(pattern.type_id(), 2);
        let (segment, gap) = pattern.params();
        assert_eq!(segment, 10.0);
        assert_eq!(gap, 5.0);
        assert!((pattern.angle() - std::f32::consts::FRAC_PI_4).abs() < 0.001);
    }

    #[test]
    fn test_edge_style_with_border() {
        let style = EdgeStyle::new()
            .stroke(StrokeStyle::new().width(2.0).color(Color::WHITE))
            .border(BorderStyle::new().width(1.5).gap(0.5).color(Color::BLACK));

        assert!(style.stroke.is_some());
        assert!(style.border.is_some());
        assert_eq!(style.border.unwrap().width, 1.5);
    }

    #[test]
    fn test_stroke_pattern_motion() {
        let pattern = StrokePattern::marching_ants();
        assert!(pattern.motion().is_some());
        assert_eq!(pattern.motion().unwrap().speed, 30.0);

        let solid = StrokePattern::Solid;
        assert!(solid.motion().is_none());
    }
}
