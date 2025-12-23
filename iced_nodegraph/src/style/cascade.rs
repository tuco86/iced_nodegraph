//! Cascade trait for style merging.
//!
//! Provides zero-allocation style merging by applying partial config overrides
//! to base resolved styles.

use iced::Color;

use super::config::{
    EdgeConfig, GraphConfig, NodeConfig, PinConfig, SelectionConfig, ShadowConfig,
};
use super::{
    EdgeStyle, GraphStyle, NodeStyle, PinStyle, SelectionStyle, ShadowStyle,
};

/// Trait for applying partial configuration overrides to a base style.
///
/// This enables the cascading style system where later layers override earlier ones:
/// `Theme Defaults -> Graph Defaults -> Item Config`
///
/// The implementation uses `unwrap_or()` for zero-allocation merging.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{Cascade, NodeConfig, NodeStyle};
/// use iced::Color;
///
/// let base = NodeStyle::default();
/// let override_config = NodeConfig::new()
///     .fill_color(Color::from_rgb(0.5, 0.0, 0.0));
///
/// let merged = override_config.apply_to(&base);
/// assert_eq!(merged.fill_color, Color::from_rgb(0.5, 0.0, 0.0));
/// assert_eq!(merged.corner_radius, base.corner_radius); // unchanged
/// ```
pub trait Cascade {
    /// The resolved style type (all fields concrete, no Options).
    type Resolved;

    /// Apply this config's overrides to a base resolved style.
    ///
    /// Returns a new resolved style with overrides applied.
    /// Fields not set in this config retain their base values.
    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved;
}

impl Cascade for NodeConfig {
    type Resolved = NodeStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        // Handle shadow merging specially
        let shadow = match (&self.shadow, &base.shadow) {
            // Explicit shadow config provided
            (Some(shadow_config), Some(base_shadow)) => {
                // Check if explicitly disabled
                if shadow_config.enabled == Some(false) {
                    None
                } else {
                    Some(shadow_config.apply_to(base_shadow))
                }
            }
            // Override config has shadow, base doesn't
            (Some(shadow_config), None) => {
                if shadow_config.enabled == Some(false) {
                    None
                } else {
                    // Create new shadow from config with defaults
                    Some(ShadowStyle {
                        offset: shadow_config.offset.unwrap_or((4.0, 4.0)),
                        blur_radius: shadow_config.blur_radius.unwrap_or(8.0),
                        color: shadow_config
                            .color
                            .unwrap_or(Color::from_rgba(0.0, 0.0, 0.0, 0.3)),
                    })
                }
            }
            // No override, use base
            (None, base_shadow) => base_shadow.clone(),
        };

        NodeStyle {
            fill_color: self.fill_color.unwrap_or(base.fill_color),
            border_color: self.border_color.unwrap_or(base.border_color),
            border_width: self.border_width.unwrap_or(base.border_width),
            corner_radius: self.corner_radius.unwrap_or(base.corner_radius),
            opacity: self.opacity.unwrap_or(base.opacity),
            shadow,
        }
    }
}

impl Cascade for ShadowConfig {
    type Resolved = ShadowStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        ShadowStyle {
            offset: self.offset.unwrap_or(base.offset),
            blur_radius: self.blur_radius.unwrap_or(base.blur_radius),
            color: self.color.unwrap_or(base.color),
        }
    }
}

impl Cascade for EdgeConfig {
    type Resolved = EdgeStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        EdgeStyle {
            color: self.color.unwrap_or(base.color),
            thickness: self.thickness.unwrap_or(base.thickness),
            edge_type: self.edge_type.unwrap_or(base.edge_type),
            dash_pattern: self.dash_pattern.or(base.dash_pattern),
            animation: self.animation.or(base.animation),
        }
    }
}

impl Cascade for PinConfig {
    type Resolved = PinStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        PinStyle {
            color: self.color.unwrap_or(base.color),
            radius: self.radius.unwrap_or(base.radius),
            shape: self.shape.unwrap_or(base.shape),
            border_color: self.border_color.or(base.border_color),
            border_width: self.border_width.unwrap_or(base.border_width),
        }
    }
}

impl Cascade for GraphConfig {
    type Resolved = GraphStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        let selection_style = match &self.selection {
            Some(sel_config) => sel_config.apply_to(&base.selection_style),
            None => base.selection_style.clone(),
        };

        GraphStyle {
            background_color: self.background_color.unwrap_or(base.background_color),
            grid_color: self.grid_color.unwrap_or(base.grid_color),
            drag_edge_color: self.drag_edge_color.unwrap_or(base.drag_edge_color),
            drag_edge_valid_color: self.drag_edge_valid_color.unwrap_or(base.drag_edge_valid_color),
            selection_style,
        }
    }
}

impl Cascade for SelectionConfig {
    type Resolved = SelectionStyle;

    fn apply_to(&self, base: &Self::Resolved) -> Self::Resolved {
        SelectionStyle {
            selected_border_color: self.border_color.unwrap_or(base.selected_border_color),
            selected_border_width: self.border_width.unwrap_or(base.selected_border_width),
            box_select_fill: self.box_fill.unwrap_or(base.box_select_fill),
            box_select_border: self.box_border.unwrap_or(base.box_select_border),
        }
    }
}

// Conversion traits for backwards compatibility
impl From<NodeStyle> for NodeConfig {
    fn from(style: NodeStyle) -> Self {
        NodeConfig {
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

impl From<EdgeStyle> for EdgeConfig {
    fn from(style: EdgeStyle) -> Self {
        EdgeConfig {
            color: Some(style.color),
            thickness: Some(style.thickness),
            edge_type: Some(style.edge_type),
            dash_pattern: style.dash_pattern,
            animation: style.animation,
        }
    }
}

impl From<PinStyle> for PinConfig {
    fn from(style: PinStyle) -> Self {
        PinConfig {
            color: Some(style.color),
            radius: Some(style.radius),
            shape: Some(style.shape),
            border_color: style.border_color,
            border_width: Some(style.border_width),
        }
    }
}

impl From<GraphStyle> for GraphConfig {
    fn from(style: GraphStyle) -> Self {
        GraphConfig {
            background_color: Some(style.background_color),
            grid_color: Some(style.grid_color),
            drag_edge_color: Some(style.drag_edge_color),
            drag_edge_valid_color: Some(style.drag_edge_valid_color),
            selection: Some(SelectionConfig {
                border_color: Some(style.selection_style.selected_border_color),
                border_width: Some(style.selection_style.selected_border_width),
                box_fill: Some(style.selection_style.box_select_fill),
                box_border: Some(style.selection_style.box_select_border),
            }),
        }
    }
}

impl From<SelectionStyle> for SelectionConfig {
    fn from(style: SelectionStyle) -> Self {
        SelectionConfig {
            border_color: Some(style.selected_border_color),
            border_width: Some(style.selected_border_width),
            box_fill: Some(style.box_select_fill),
            box_border: Some(style.box_select_border),
        }
    }
}

impl From<ShadowStyle> for ShadowConfig {
    fn from(style: ShadowStyle) -> Self {
        ShadowConfig {
            offset: Some(style.offset),
            blur_radius: Some(style.blur_radius),
            color: Some(style.color),
            enabled: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_config_cascade() {
        let base = NodeStyle::default();
        let config = NodeConfig::new()
            .fill_color(Color::from_rgb(1.0, 0.0, 0.0))
            .corner_radius(15.0);

        let merged = config.apply_to(&base);

        assert_eq!(merged.fill_color, Color::from_rgb(1.0, 0.0, 0.0));
        assert_eq!(merged.corner_radius, 15.0);
        // Unchanged fields
        assert_eq!(merged.border_color, base.border_color);
        assert_eq!(merged.border_width, base.border_width);
        assert_eq!(merged.opacity, base.opacity);
    }

    #[test]
    fn test_empty_config_returns_base() {
        let base = NodeStyle::default();
        let config = NodeConfig::new();

        let merged = config.apply_to(&base);

        assert_eq!(merged.fill_color, base.fill_color);
        assert_eq!(merged.border_color, base.border_color);
        assert_eq!(merged.corner_radius, base.corner_radius);
    }

    #[test]
    fn test_shadow_disable() {
        let base = NodeStyle::default();
        assert!(base.shadow.is_some());

        let config = NodeConfig::new().no_shadow();
        let merged = config.apply_to(&base);

        assert!(merged.shadow.is_none());
    }

    #[test]
    fn test_edge_config_cascade() {
        let base = EdgeStyle::default();
        let config = EdgeConfig::new()
            .color(Color::from_rgb(0.0, 1.0, 0.0))
            .thickness(5.0);

        let merged = config.apply_to(&base);

        assert_eq!(merged.color, Color::from_rgb(0.0, 1.0, 0.0));
        assert_eq!(merged.thickness, 5.0);
        assert_eq!(merged.edge_type, base.edge_type);
    }

    #[test]
    fn test_from_node_style_roundtrip() {
        let original = NodeStyle::process();
        let config: NodeConfig = original.clone().into();
        let restored = config.apply_to(&NodeStyle::default());

        assert_eq!(restored.fill_color, original.fill_color);
        assert_eq!(restored.border_color, original.border_color);
        assert_eq!(restored.corner_radius, original.corner_radius);
    }

    #[test]
    fn test_multiple_cascade_layers() {
        let base = NodeStyle::default();

        // Layer 1: Graph defaults
        let layer1 = NodeConfig::new().corner_radius(10.0).opacity(0.8);

        // Layer 2: Item override
        let layer2 = NodeConfig::new().fill_color(Color::from_rgb(0.5, 0.0, 0.0));

        // Apply cascade: base -> layer1 -> layer2
        let after_layer1 = layer1.apply_to(&base);
        let final_style = layer2.apply_to(&after_layer1);

        assert_eq!(final_style.corner_radius, 10.0); // From layer1
        assert_eq!(final_style.opacity, 0.8); // From layer1
        assert_eq!(final_style.fill_color, Color::from_rgb(0.5, 0.0, 0.0)); // From layer2
        assert_eq!(final_style.border_color, base.border_color); // From base
    }
}
