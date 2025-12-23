//! Partial configuration types for cascading style overrides.
//!
//! These types use `Option<T>` fields to allow partial overrides in the style cascade:
//! `Theme Defaults -> Graph Defaults -> Item Config`

use iced::Color;

use super::{DashPattern, EdgeAnimation, EdgeType, PinShape};

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
}

/// Partial shadow configuration for cascading style overrides.
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
}

/// Partial edge configuration for cascading style overrides.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{EdgeConfig, EdgeType};
/// use iced::Color;
///
/// let config = EdgeConfig::new()
///     .color(Color::from_rgb(0.3, 0.6, 1.0))
///     .thickness(3.0)
///     .edge_type(EdgeType::Bezier);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeConfig {
    /// Edge line color
    pub color: Option<Color>,
    /// Line thickness in world-space pixels
    pub thickness: Option<f32>,
    /// Edge path type (bezier, straight, step, etc.)
    pub edge_type: Option<EdgeType>,
    /// Optional dash pattern
    pub dash_pattern: Option<DashPattern>,
    /// Optional animation effects
    pub animation: Option<EdgeAnimation>,
}

impl EdgeConfig {
    /// Creates an empty config with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the edge color override.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the edge thickness override.
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = Some(thickness);
        self
    }

    /// Sets the edge type override.
    pub fn edge_type(mut self, edge_type: EdgeType) -> Self {
        self.edge_type = Some(edge_type);
        self
    }

    /// Sets the dash pattern override.
    pub fn dash_pattern(mut self, pattern: DashPattern) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    /// Sets the animation override.
    pub fn animation(mut self, animation: EdgeAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Returns true if this config has any overrides set.
    pub fn has_overrides(&self) -> bool {
        self.color.is_some()
            || self.thickness.is_some()
            || self.edge_type.is_some()
            || self.dash_pattern.is_some()
            || self.animation.is_some()
    }
}

/// Partial pin configuration for cascading style overrides.
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
}

/// Partial graph configuration for cascading style overrides.
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
}

/// Partial selection style configuration for cascading overrides.
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
            .color(Color::from_rgb(0.3, 0.6, 1.0))
            .thickness(3.0)
            .edge_type(EdgeType::SmoothStep);

        assert!(config.color.is_some());
        assert_eq!(config.thickness, Some(3.0));
        assert_eq!(config.edge_type, Some(EdgeType::SmoothStep));
    }

    #[test]
    fn test_shadow_config_none() {
        let config = ShadowConfig::none();
        assert_eq!(config.enabled, Some(false));
    }
}
