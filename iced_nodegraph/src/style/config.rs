//! Configuration types for node graph styling.
//!
//! These types use `Option<T>` fields to allow partial overrides. Use `merge()`
//! to combine configs where `self` takes priority over `other`.

use iced::Color;

use super::{
    BackgroundPattern, BackgroundStyle, BorderStyle, DashCap, EdgeCurve, PinShape, StrokeCap,
    StrokePattern, StrokeStyle,
};

/// Partial node configuration for cascading style overrides.
///
/// All fields are optional - only set fields will override the base style.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::NodeConfig;
/// use iced::Color;
///
/// let config = NodeConfig::new()
///     .fill_color(Color::from_rgb(0.2, 0.3, 0.4))
///     .corner_radius(10.0);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeConfig {
    /// Fill color for the node body
    pub fill_color: Option<Color>,
    /// Border color
    pub border_color: Option<Color>,
    /// Border width in world-space pixels
    pub border_width: Option<f32>,
    /// Corner radius for rounded corners
    pub corner_radius: Option<f32>,
    /// Node opacity (0.0 to 1.0)
    pub opacity: Option<f32>,
    /// Optional drop shadow configuration
    pub shadow: Option<ShadowConfig>,
}

impl NodeConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the fill color override.
    pub fn fill_color(mut self, color: impl Into<Color>) -> Self {
        self.fill_color = Some(color.into());
        self
    }

    /// Sets the border color override.
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Sets the border width override.
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Sets the corner radius override.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Sets the opacity override.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Sets the shadow configuration override.
    pub fn shadow(mut self, shadow: ShadowConfig) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Removes the shadow (explicit override to no shadow).
    pub fn no_shadow(mut self) -> Self {
        self.shadow = Some(ShadowConfig::none());
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.fill_color.is_some()
            || self.border_color.is_some()
            || self.border_width.is_some()
            || self.corner_radius.is_some()
            || self.opacity.is_some()
            || self.shadow.is_some()
    }

    /// Merges two configs. Self takes priority, other fills gaps.
    ///
    /// # Example
    /// ```rust
    /// use iced_nodegraph::style::NodeConfig;
    /// use iced::Color;
    ///
    /// let defaults = NodeConfig::new().corner_radius(10.0).opacity(0.9);
    /// let specific = NodeConfig::new().fill_color(Color::from_rgb(1.0, 0.0, 0.0));
    /// let merged = specific.merge(&defaults);
    ///
    /// assert!(merged.fill_color.is_some()); // from specific
    /// assert_eq!(merged.corner_radius, Some(10.0)); // from defaults
    /// ```
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            fill_color: self.fill_color.or(other.fill_color),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
            corner_radius: self.corner_radius.or(other.corner_radius),
            opacity: self.opacity.or(other.opacity),
            shadow: self.shadow.clone().or(other.shadow.clone()),
        }
    }
}

/// Shadow configuration for node drop shadows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShadowConfig {
    /// Horizontal and vertical offset in world-space pixels
    pub offset: Option<(f32, f32)>,
    /// Blur radius in world-space pixels
    pub blur_radius: Option<f32>,
    /// Shadow color
    pub color: Option<Color>,
    /// Whether shadow is enabled (false = explicit disable)
    pub enabled: Option<bool>,
}

impl ShadowConfig {
    /// Creates an empty shadow config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a config that explicitly disables shadows.
    pub fn none() -> Self {
        Self {
            enabled: Some(false),
            ..Default::default()
        }
    }

    /// Sets the shadow offset override.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Some((x, y));
        self
    }

    /// Sets the blur radius override.
    pub fn blur_radius(mut self, radius: f32) -> Self {
        self.blur_radius = Some(radius);
        self
    }

    /// Sets the shadow color override.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Explicitly enables the shadow.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Merges two shadow configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            offset: self.offset.or(other.offset),
            blur_radius: self.blur_radius.or(other.blur_radius),
            color: self.color.or(other.color),
            enabled: self.enabled.or(other.enabled),
        }
    }
}

/// Partial background configuration for cascading style overrides.
///
/// All fields are optional - only set fields will override the base style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundConfig {
    /// Pattern type
    pub pattern: Option<BackgroundPattern>,

    // Colors
    /// Background fill color
    pub background_color: Option<Color>,
    /// Primary pattern color (major lines/elements)
    pub primary_color: Option<Color>,
    /// Secondary pattern color (minor lines/elements)
    pub secondary_color: Option<Color>,

    // Spacing
    /// Minor grid/pattern spacing in world-space pixels
    pub minor_spacing: Option<f32>,
    /// Major grid spacing. Some(None) = explicitly no major grid
    pub major_spacing: Option<Option<f32>>,

    // Line properties
    /// Minor line width in world-space pixels
    pub minor_width: Option<f32>,
    /// Major line width in world-space pixels
    pub major_width: Option<f32>,
    /// Minor line opacity (0.0 - 1.0)
    pub minor_opacity: Option<f32>,
    /// Major line opacity (0.0 - 1.0)
    pub major_opacity: Option<f32>,

    // Pattern-specific
    /// Dot radius (for Dots pattern)
    pub dot_radius: Option<f32>,
    /// Line angle in radians (for Lines/Crosshatch patterns)
    pub line_angle: Option<f32>,
    /// Crosshatch secondary angle
    pub crosshatch_angle: Option<f32>,
    /// Hex orientation (true = pointy-top)
    pub hex_pointy_top: Option<bool>,

    // Adaptive zoom
    /// Enable adaptive spacing
    pub adaptive_zoom: Option<bool>,
    /// Minimum screen-space spacing before pattern doubles
    pub adaptive_min_spacing: Option<f32>,
    /// Maximum screen-space spacing before pattern halves
    pub adaptive_max_spacing: Option<f32>,
    /// Fade range for minor elements at zoom extremes
    pub adaptive_fade_range: Option<f32>,
}

impl BackgroundConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the pattern type.
    pub fn pattern(mut self, pattern: BackgroundPattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Sets the background color.
    pub fn background_color(mut self, color: impl Into<Color>) -> Self {
        self.background_color = Some(color.into());
        self
    }

    /// Sets the primary pattern color.
    pub fn primary_color(mut self, color: impl Into<Color>) -> Self {
        self.primary_color = Some(color.into());
        self
    }

    /// Sets the secondary pattern color.
    pub fn secondary_color(mut self, color: impl Into<Color>) -> Self {
        self.secondary_color = Some(color.into());
        self
    }

    /// Sets the minor spacing.
    pub fn minor_spacing(mut self, spacing: f32) -> Self {
        self.minor_spacing = Some(spacing);
        self
    }

    /// Sets the major spacing.
    pub fn major_spacing(mut self, spacing: f32) -> Self {
        self.major_spacing = Some(Some(spacing));
        self
    }

    /// Explicitly disables major grid.
    pub fn no_major_grid(mut self) -> Self {
        self.major_spacing = Some(None);
        self
    }

    /// Sets the minor line width.
    pub fn minor_width(mut self, width: f32) -> Self {
        self.minor_width = Some(width);
        self
    }

    /// Sets the major line width.
    pub fn major_width(mut self, width: f32) -> Self {
        self.major_width = Some(width);
        self
    }

    /// Sets the minor line opacity.
    pub fn minor_opacity(mut self, opacity: f32) -> Self {
        self.minor_opacity = Some(opacity);
        self
    }

    /// Sets the major line opacity.
    pub fn major_opacity(mut self, opacity: f32) -> Self {
        self.major_opacity = Some(opacity);
        self
    }

    /// Sets the dot radius.
    pub fn dot_radius(mut self, radius: f32) -> Self {
        self.dot_radius = Some(radius);
        self
    }

    /// Sets the line angle.
    pub fn line_angle(mut self, angle_rad: f32) -> Self {
        self.line_angle = Some(angle_rad);
        self
    }

    /// Sets the crosshatch angle.
    pub fn crosshatch_angle(mut self, angle_rad: f32) -> Self {
        self.crosshatch_angle = Some(angle_rad);
        self
    }

    /// Sets hex orientation.
    pub fn hex_pointy_top(mut self, pointy: bool) -> Self {
        self.hex_pointy_top = Some(pointy);
        self
    }

    /// Enables or disables adaptive zoom.
    pub fn adaptive_zoom(mut self, enabled: bool) -> Self {
        self.adaptive_zoom = Some(enabled);
        self
    }

    /// Sets the adaptive zoom thresholds.
    pub fn adaptive_thresholds(mut self, min: f32, max: f32) -> Self {
        self.adaptive_min_spacing = Some(min);
        self.adaptive_max_spacing = Some(max);
        self
    }

    /// Sets the adaptive fade range.
    pub fn adaptive_fade(mut self, range: f32) -> Self {
        self.adaptive_fade_range = Some(range);
        self
    }

    /// Applies this config as overrides to a base style.
    ///
    /// Fields set in this config will override the base style values.
    /// Unset fields will keep the base style values.
    pub fn apply_to(&self, base: BackgroundStyle) -> BackgroundStyle {
        BackgroundStyle {
            pattern: self.pattern.unwrap_or(base.pattern),
            background_color: self.background_color.unwrap_or(base.background_color),
            primary_color: self.primary_color.unwrap_or(base.primary_color),
            secondary_color: self.secondary_color.unwrap_or(base.secondary_color),
            minor_spacing: self.minor_spacing.unwrap_or(base.minor_spacing),
            major_spacing: self.major_spacing.unwrap_or(base.major_spacing),
            minor_width: self.minor_width.unwrap_or(base.minor_width),
            major_width: self.major_width.unwrap_or(base.major_width),
            minor_opacity: self.minor_opacity.unwrap_or(base.minor_opacity),
            major_opacity: self.major_opacity.unwrap_or(base.major_opacity),
            dot_radius: self.dot_radius.unwrap_or(base.dot_radius),
            line_angle: self.line_angle.unwrap_or(base.line_angle),
            crosshatch_angle: self.crosshatch_angle.unwrap_or(base.crosshatch_angle),
            hex_pointy_top: self.hex_pointy_top.unwrap_or(base.hex_pointy_top),
            adaptive_zoom: self.adaptive_zoom.unwrap_or(base.adaptive_zoom),
            adaptive_min_spacing: self
                .adaptive_min_spacing
                .unwrap_or(base.adaptive_min_spacing),
            adaptive_max_spacing: self
                .adaptive_max_spacing
                .unwrap_or(base.adaptive_max_spacing),
            adaptive_fade_range: self.adaptive_fade_range.unwrap_or(base.adaptive_fade_range),
        }
    }

    /// Applies this config to the default BackgroundStyle.
    pub fn resolve(&self) -> BackgroundStyle {
        self.apply_to(BackgroundStyle::default())
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.pattern.is_some()
            || self.background_color.is_some()
            || self.primary_color.is_some()
            || self.secondary_color.is_some()
            || self.minor_spacing.is_some()
            || self.major_spacing.is_some()
            || self.minor_width.is_some()
            || self.major_width.is_some()
            || self.minor_opacity.is_some()
            || self.major_opacity.is_some()
            || self.dot_radius.is_some()
            || self.line_angle.is_some()
            || self.crosshatch_angle.is_some()
            || self.hex_pointy_top.is_some()
            || self.adaptive_zoom.is_some()
            || self.adaptive_min_spacing.is_some()
            || self.adaptive_max_spacing.is_some()
            || self.adaptive_fade_range.is_some()
    }

    /// Merges two background configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            pattern: self.pattern.or(other.pattern),
            background_color: self.background_color.or(other.background_color),
            primary_color: self.primary_color.or(other.primary_color),
            secondary_color: self.secondary_color.or(other.secondary_color),
            minor_spacing: self.minor_spacing.or(other.minor_spacing),
            major_spacing: self.major_spacing.or(other.major_spacing),
            minor_width: self.minor_width.or(other.minor_width),
            major_width: self.major_width.or(other.major_width),
            minor_opacity: self.minor_opacity.or(other.minor_opacity),
            major_opacity: self.major_opacity.or(other.major_opacity),
            dot_radius: self.dot_radius.or(other.dot_radius),
            line_angle: self.line_angle.or(other.line_angle),
            crosshatch_angle: self.crosshatch_angle.or(other.crosshatch_angle),
            hex_pointy_top: self.hex_pointy_top.or(other.hex_pointy_top),
            adaptive_zoom: self.adaptive_zoom.or(other.adaptive_zoom),
            adaptive_min_spacing: self.adaptive_min_spacing.or(other.adaptive_min_spacing),
            adaptive_max_spacing: self.adaptive_max_spacing.or(other.adaptive_max_spacing),
            adaptive_fade_range: self.adaptive_fade_range.or(other.adaptive_fade_range),
        }
    }
}

impl From<BackgroundStyle> for BackgroundConfig {
    fn from(style: BackgroundStyle) -> Self {
        Self {
            pattern: Some(style.pattern),
            background_color: Some(style.background_color),
            primary_color: Some(style.primary_color),
            secondary_color: Some(style.secondary_color),
            minor_spacing: Some(style.minor_spacing),
            major_spacing: Some(style.major_spacing),
            minor_width: Some(style.minor_width),
            major_width: Some(style.major_width),
            minor_opacity: Some(style.minor_opacity),
            major_opacity: Some(style.major_opacity),
            dot_radius: Some(style.dot_radius),
            line_angle: Some(style.line_angle),
            crosshatch_angle: Some(style.crosshatch_angle),
            hex_pointy_top: Some(style.hex_pointy_top),
            adaptive_zoom: Some(style.adaptive_zoom),
            adaptive_min_spacing: Some(style.adaptive_min_spacing),
            adaptive_max_spacing: Some(style.adaptive_max_spacing),
            adaptive_fade_range: Some(style.adaptive_fade_range),
        }
    }
}

/// Partial stroke configuration for edge styling.
///
/// All fields are optional - only set fields will override the base style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StrokeConfig {
    /// Stroke width in world-space pixels
    pub width: Option<f32>,
    /// Color at the source pin (t=0). TRANSPARENT = use source pin color.
    pub start_color: Option<Color>,
    /// Color at the target pin (t=1). TRANSPARENT = use target pin color.
    pub end_color: Option<Color>,
    /// Line pattern (solid, dashed, dotted, etc.)
    pub pattern: Option<StrokePattern>,
    /// End cap style for stroke endpoints
    pub cap: Option<StrokeCap>,
    /// Cap style for individual dash segments
    pub dash_cap: Option<DashCap>,
}

impl StrokeConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the stroke width override.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets a solid color (both start and end).
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        let c = color.into();
        self.start_color = Some(c);
        self.end_color = Some(c);
        self
    }

    /// Sets the start color (at source pin).
    pub fn start_color(mut self, color: impl Into<Color>) -> Self {
        self.start_color = Some(color.into());
        self
    }

    /// Sets the end color (at target pin).
    pub fn end_color(mut self, color: impl Into<Color>) -> Self {
        self.end_color = Some(color.into());
        self
    }

    /// Sets the line pattern override.
    pub fn pattern(mut self, pattern: StrokePattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Sets the end cap style override.
    pub fn cap(mut self, cap: StrokeCap) -> Self {
        self.cap = Some(cap);
        self
    }

    /// Sets the dash cap style override.
    pub fn dash_cap(mut self, dash_cap: DashCap) -> Self {
        self.dash_cap = Some(dash_cap);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.width.is_some()
            || self.start_color.is_some()
            || self.end_color.is_some()
            || self.pattern.is_some()
            || self.cap.is_some()
            || self.dash_cap.is_some()
    }

    /// Merges two stroke configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            width: self.width.or(other.width),
            start_color: self.start_color.or(other.start_color),
            end_color: self.end_color.or(other.end_color),
            pattern: self.pattern.clone().or(other.pattern.clone()),
            cap: self.cap.or(other.cap),
            dash_cap: self.dash_cap.or(other.dash_cap),
        }
    }
}

impl From<StrokeStyle> for StrokeConfig {
    fn from(style: StrokeStyle) -> Self {
        Self {
            width: Some(style.width),
            start_color: Some(style.start_color),
            end_color: Some(style.end_color),
            pattern: Some(style.pattern),
            cap: Some(style.cap),
            dash_cap: Some(style.dash_cap),
        }
    }
}

/// Partial border configuration for edge styling.
///
/// All fields are optional - only set fields will override the base style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BorderConfig {
    /// Border width in world-space pixels
    pub width: Option<f32>,
    /// Radial gap between stroke and border
    pub gap: Option<f32>,
    /// Border color
    pub color: Option<Color>,
    /// Explicit enabled flag (None = inherit, Some(false) = force disable)
    pub enabled: Option<bool>,
}

impl BorderConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the border width override.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets the gap override.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = Some(gap);
        self
    }

    /// Sets the border color override.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Explicitly enables or disables the border.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.width.is_some() || self.gap.is_some() || self.color.is_some() || self.enabled.is_some()
    }

    /// Merges two border configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            width: self.width.or(other.width),
            gap: self.gap.or(other.gap),
            color: self.color.or(other.color),
            enabled: self.enabled.or(other.enabled),
        }
    }
}

impl From<BorderStyle> for BorderConfig {
    fn from(style: BorderStyle) -> Self {
        Self {
            width: Some(style.width),
            gap: Some(style.gap),
            color: Some(style.color),
            enabled: Some(true),
        }
    }
}

/// Edge shadow configuration (partial overrides for EdgeShadowStyle).
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::EdgeShadowConfig;
/// use iced::Color;
///
/// let shadow = EdgeShadowConfig::new()
///     .blur(6.0)
///     .color(Color::from_rgba(0.0, 0.0, 0.0, 0.4))
///     .offset(2.0, 2.0);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeShadowConfig {
    /// Shadow blur radius
    pub blur: Option<f32>,
    /// Shadow color
    pub color: Option<Color>,
    /// Shadow offset (x, y)
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    /// Explicitly enable/disable shadow
    pub enabled: Option<bool>,
}

impl EdgeShadowConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the blur radius override.
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = Some(blur);
        self
    }

    /// Sets the shadow color override.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the shadow offset override.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = Some(x);
        self.offset_y = Some(y);
        self
    }

    /// Explicitly enables or disables the shadow.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.blur.is_some()
            || self.color.is_some()
            || self.offset_x.is_some()
            || self.offset_y.is_some()
            || self.enabled.is_some()
    }

    /// Merges two shadow configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            blur: self.blur.or(other.blur),
            color: self.color.or(other.color),
            offset_x: self.offset_x.or(other.offset_x),
            offset_y: self.offset_y.or(other.offset_y),
            enabled: self.enabled.or(other.enabled),
        }
    }
}

impl From<super::EdgeShadowStyle> for EdgeShadowConfig {
    fn from(style: super::EdgeShadowStyle) -> Self {
        Self {
            blur: Some(style.blur),
            color: Some(style.color),
            offset_x: Some(style.offset.0),
            offset_y: Some(style.offset.1),
            enabled: Some(true),
        }
    }
}

/// Edge configuration for connection lines with layer-based composition.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{EdgeConfig, StrokeConfig, EdgeCurve, StrokePattern};
/// use iced::Color;
///
/// // Solid color edge
/// let config = EdgeConfig::new()
///     .stroke(StrokeConfig::new()
///         .color(Color::from_rgb(0.3, 0.6, 1.0))
///         .width(3.0))
///     .curve(EdgeCurve::BezierCubic);
///
/// // Shorthand for simple solid color
/// let simple = EdgeConfig::new()
///     .solid_color(Color::WHITE)
///     .width(2.0);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeConfig {
    /// Stroke layer configuration
    pub stroke: Option<StrokeConfig>,
    /// Border layer configuration
    pub border: Option<BorderConfig>,
    /// Shadow layer configuration
    pub shadow: Option<EdgeShadowConfig>,
    /// Edge curve type
    pub curve: Option<EdgeCurve>,
}

impl EdgeConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the stroke configuration.
    pub fn stroke(mut self, stroke: StrokeConfig) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Sets the border configuration.
    pub fn border(mut self, border: BorderConfig) -> Self {
        self.border = Some(border);
        self
    }

    /// Explicitly disables the border.
    pub fn no_border(mut self) -> Self {
        self.border = Some(BorderConfig {
            enabled: Some(false),
            ..Default::default()
        });
        self
    }

    /// Sets the shadow configuration.
    pub fn shadow(mut self, shadow: EdgeShadowConfig) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Explicitly disables the shadow.
    pub fn no_shadow(mut self) -> Self {
        self.shadow = Some(EdgeShadowConfig {
            enabled: Some(false),
            ..Default::default()
        });
        self
    }

    /// Sets the curve type.
    pub fn curve(mut self, curve: EdgeCurve) -> Self {
        self.curve = Some(curve);
        self
    }

    // === Convenience Methods ===

    /// Sets a solid color (shorthand for stroke color).
    pub fn solid_color(mut self, color: impl Into<Color>) -> Self {
        let c = color.into();
        let stroke = self.stroke.get_or_insert_with(Default::default);
        stroke.start_color = Some(c);
        stroke.end_color = Some(c);
        self
    }

    /// Sets the stroke width (shorthand).
    pub fn width(mut self, width: f32) -> Self {
        let stroke = self.stroke.get_or_insert_with(Default::default);
        stroke.width = Some(width);
        self
    }

    /// Alias for width.
    pub fn thickness(self, thickness: f32) -> Self {
        self.width(thickness)
    }

    /// Sets the stroke pattern (shorthand).
    pub fn pattern(mut self, pattern: StrokePattern) -> Self {
        let stroke = self.stroke.get_or_insert_with(Default::default);
        stroke.pattern = Some(pattern);
        self
    }

    /// Sets the dash cap style (shorthand).
    pub fn dash_cap(mut self, dash_cap: DashCap) -> Self {
        let stroke = self.stroke.get_or_insert_with(Default::default);
        stroke.dash_cap = Some(dash_cap);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.stroke.is_some()
            || self.border.is_some()
            || self.shadow.is_some()
            || self.curve.is_some()
    }

    /// Merges two edge configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            stroke: match (&self.stroke, &other.stroke) {
                (Some(s), Some(o)) => Some(s.merge(o)),
                (Some(s), None) => Some(s.clone()),
                (None, Some(o)) => Some(o.clone()),
                (None, None) => None,
            },
            border: match (&self.border, &other.border) {
                (Some(s), Some(o)) => Some(s.merge(o)),
                (Some(s), None) => Some(s.clone()),
                (None, Some(o)) => Some(o.clone()),
                (None, None) => None,
            },
            shadow: match (&self.shadow, &other.shadow) {
                (Some(s), Some(o)) => Some(s.merge(o)),
                (Some(s), None) => Some(s.clone()),
                (None, Some(o)) => Some(o.clone()),
                (None, None) => None,
            },
            curve: self.curve.or(other.curve),
        }
    }
}

/// Pin configuration for connection points.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{PinConfig, PinShape};
/// use iced::Color;
///
/// let config = PinConfig::new()
///     .color(Color::from_rgb(0.3, 0.8, 0.4))
///     .radius(8.0)
///     .shape(PinShape::Diamond);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinConfig {
    /// Pin indicator color
    pub color: Option<Color>,
    /// Pin indicator radius in world-space pixels
    pub radius: Option<f32>,
    /// Shape of the pin indicator
    pub shape: Option<PinShape>,
    /// Border color
    pub border_color: Option<Color>,
    /// Border width in world-space pixels
    pub border_width: Option<f32>,
}

impl PinConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the pin color override.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the pin radius override.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Sets the pin shape override.
    pub fn shape(mut self, shape: PinShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the border color override.
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Sets the border width override.
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.color.is_some()
            || self.radius.is_some()
            || self.shape.is_some()
            || self.border_color.is_some()
            || self.border_width.is_some()
    }

    /// Merges two pin configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            color: self.color.or(other.color),
            radius: self.radius.or(other.radius),
            shape: self.shape.or(other.shape),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
        }
    }
}

/// Graph configuration for canvas and background.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::GraphConfig;
/// use iced::Color;
///
/// let config = GraphConfig::new()
///     .background_color(Color::from_rgb(0.1, 0.1, 0.12));
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphConfig {
    /// Background color of the canvas
    pub background_color: Option<Color>,
    /// Grid line color
    pub grid_color: Option<Color>,
    /// Drag edge color when connection is invalid
    pub drag_edge_color: Option<Color>,
    /// Drag edge color when connection is valid
    pub drag_edge_valid_color: Option<Color>,
    /// Selection style configuration
    pub selection: Option<SelectionConfig>,
}

impl GraphConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the background color override.
    pub fn background_color(mut self, color: impl Into<Color>) -> Self {
        self.background_color = Some(color.into());
        self
    }

    /// Sets the grid color override.
    pub fn grid_color(mut self, color: impl Into<Color>) -> Self {
        self.grid_color = Some(color.into());
        self
    }

    /// Sets the drag edge color override (invalid connection).
    pub fn drag_edge_color(mut self, color: impl Into<Color>) -> Self {
        self.drag_edge_color = Some(color.into());
        self
    }

    /// Sets the drag edge valid color override.
    pub fn drag_edge_valid_color(mut self, color: impl Into<Color>) -> Self {
        self.drag_edge_valid_color = Some(color.into());
        self
    }

    /// Sets the selection style override.
    pub fn selection(mut self, selection: SelectionConfig) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.background_color.is_some()
            || self.grid_color.is_some()
            || self.drag_edge_color.is_some()
            || self.drag_edge_valid_color.is_some()
            || self.selection.is_some()
    }

    /// Merges two graph configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            background_color: self.background_color.or(other.background_color),
            grid_color: self.grid_color.or(other.grid_color),
            drag_edge_color: self.drag_edge_color.or(other.drag_edge_color),
            drag_edge_valid_color: self.drag_edge_valid_color.or(other.drag_edge_valid_color),
            selection: match (&self.selection, &other.selection) {
                (Some(s), Some(o)) => Some(s.merge(o)),
                (Some(s), None) => Some(s.clone()),
                (None, Some(o)) => Some(o.clone()),
                (None, None) => None,
            },
        }
    }
}

/// Selection style configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionConfig {
    /// Border color for selected nodes
    pub border_color: Option<Color>,
    /// Border width for selected nodes
    pub border_width: Option<f32>,
    /// Fill color for box selection rectangle
    pub box_fill: Option<Color>,
    /// Border color for box selection rectangle
    pub box_border: Option<Color>,
}

impl SelectionConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the selected border color override.
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Sets the selected border width override.
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Sets the box selection fill color override.
    pub fn box_fill(mut self, color: impl Into<Color>) -> Self {
        self.box_fill = Some(color.into());
        self
    }

    /// Sets the box selection border color override.
    pub fn box_border(mut self, color: impl Into<Color>) -> Self {
        self.box_border = Some(color.into());
        self
    }

    /// Merges two selection configs. Self takes priority, other fills gaps.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
            box_fill: self.box_fill.or(other.box_fill),
            box_border: self.box_border.or(other.box_border),
        }
    }
}

// Conversions from Style types to Config types (for backwards compatibility)
// These set ALL fields, overriding all theme defaults.

impl From<super::NodeStyle> for NodeConfig {
    /// Converts a complete NodeStyle to NodeConfig, setting all fields.
    fn from(style: super::NodeStyle) -> Self {
        Self {
            fill_color: Some(style.fill_color),
            border_color: Some(style.border_color),
            border_width: Some(style.border_width),
            corner_radius: Some(style.corner_radius),
            opacity: Some(style.opacity),
            shadow: style.shadow.map(|s| ShadowConfig {
                offset: Some(s.offset),
                blur_radius: Some(s.blur_radius),
                color: Some(s.color),
                enabled: Some(true),
            }),
        }
    }
}

impl From<super::EdgeStyle> for EdgeConfig {
    /// Converts a complete EdgeStyle to EdgeConfig, setting all fields.
    fn from(style: super::EdgeStyle) -> Self {
        Self {
            stroke: style.stroke.map(|s| s.into()),
            border: style.border.map(|b| b.into()),
            shadow: style.shadow.map(|s| s.into()),
            curve: Some(style.curve),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_config_builder() {
        let config = NodeConfig::new()
            .fill_color(Color::from_rgb(0.5, 0.5, 0.5))
            .corner_radius(10.0)
            .opacity(0.9);

        assert_eq!(config.fill_color, Some(Color::from_rgb(0.5, 0.5, 0.5)));
        assert_eq!(config.corner_radius, Some(10.0));
        assert_eq!(config.opacity, Some(0.9));
        assert!(config.border_color.is_none());
        assert!(config.has_overrides());
    }

    #[test]
    fn test_empty_config_has_no_overrides() {
        let config = NodeConfig::new();
        assert!(!config.has_overrides());
    }

    #[test]
    fn test_edge_config_builder() {
        let config = EdgeConfig::new()
            .solid_color(Color::from_rgb(0.3, 0.6, 1.0))
            .thickness(3.0)
            .curve(EdgeCurve::OrthogonalSmooth { radius: 15.0 });

        let stroke = config.stroke.unwrap();
        assert!(stroke.start_color.is_some());
        assert!(stroke.end_color.is_some());
        assert_eq!(stroke.width, Some(3.0));
        assert_eq!(
            config.curve,
            Some(EdgeCurve::OrthogonalSmooth { radius: 15.0 })
        );
    }

    #[test]
    fn test_shadow_config_none() {
        let config = ShadowConfig::none();
        assert_eq!(config.enabled, Some(false));
    }

    #[test]
    fn test_node_config_merge() {
        let defaults = NodeConfig::new().corner_radius(10.0).opacity(0.9);
        let specific = NodeConfig::new().fill_color(Color::from_rgb(1.0, 0.0, 0.0));
        let merged = specific.merge(&defaults);

        // specific values take priority
        assert_eq!(merged.fill_color, Some(Color::from_rgb(1.0, 0.0, 0.0)));
        // defaults fill the gaps
        assert_eq!(merged.corner_radius, Some(10.0));
        assert_eq!(merged.opacity, Some(0.9));
        // unset in both stays None
        assert!(merged.border_color.is_none());
    }

    #[test]
    fn test_edge_config_merge() {
        let defaults = EdgeConfig::new().thickness(2.0);
        let specific = EdgeConfig::new().solid_color(Color::WHITE);
        let merged = specific.merge(&defaults);

        let stroke = merged.stroke.unwrap();
        assert_eq!(stroke.start_color, Some(Color::WHITE));
        assert_eq!(stroke.width, Some(2.0));
    }

    #[test]
    fn test_stroke_config_merge() {
        let defaults = StrokeConfig::new().width(2.0).cap(StrokeCap::Round);
        let specific = StrokeConfig::new().color(Color::WHITE);
        let merged = specific.merge(&defaults);

        assert_eq!(merged.start_color, Some(Color::WHITE));
        assert_eq!(merged.end_color, Some(Color::WHITE));
        assert_eq!(merged.width, Some(2.0));
        assert_eq!(merged.cap, Some(StrokeCap::Round));
    }

    #[test]
    fn test_border_config_merge() {
        let defaults = BorderConfig::new().width(1.5).gap(0.5);
        let specific = BorderConfig::new().color(Color::BLACK);
        let merged = specific.merge(&defaults);

        assert_eq!(merged.color, Some(Color::BLACK));
        assert_eq!(merged.width, Some(1.5));
        assert_eq!(merged.gap, Some(0.5));
    }

    #[test]
    fn test_pin_config_merge() {
        let defaults = PinConfig::new().radius(6.0).shape(PinShape::Circle);
        let specific = PinConfig::new().color(Color::BLACK);
        let merged = specific.merge(&defaults);

        assert_eq!(merged.color, Some(Color::BLACK));
        assert_eq!(merged.radius, Some(6.0));
        assert_eq!(merged.shape, Some(PinShape::Circle));
    }

    #[test]
    fn test_graph_config_merge_with_nested_selection() {
        let defaults = GraphConfig::new()
            .background_color(Color::BLACK)
            .selection(SelectionConfig::new().border_width(2.0));
        let specific = GraphConfig::new()
            .grid_color(Color::WHITE)
            .selection(SelectionConfig::new().border_color(Color::from_rgb(1.0, 0.0, 0.0)));
        let merged = specific.merge(&defaults);

        assert_eq!(merged.background_color, Some(Color::BLACK));
        assert_eq!(merged.grid_color, Some(Color::WHITE));
        // Nested selection merge
        let sel = merged.selection.unwrap();
        assert_eq!(sel.border_color, Some(Color::from_rgb(1.0, 0.0, 0.0)));
        assert_eq!(sel.border_width, Some(2.0));
    }
}
