//! Style resolver for cascading style system.
//!
//! This module provides the central style resolution logic that combines
//! theme defaults, graph-level defaults, and per-item configuration.

use iced::Theme;

use super::cascade::Cascade;
use super::config::{EdgeConfig, GraphConfig, NodeConfig, PinConfig};
use super::theme_defaults::ThemeDefaults;
use super::{EdgeStyle, GraphStyle, NodeStyle, PinStyle};

/// Graph-level default configurations for all item types.
///
/// This is the second layer of the style cascade, applied after theme defaults
/// but before per-item configurations.
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{GraphDefaults, NodeConfig, EdgeConfig};
///
/// let defaults = GraphDefaults::new()
///     .node(NodeConfig::new()
///         .corner_radius(10.0)
///         .opacity(0.8))
///     .edge(EdgeConfig::new()
///         .thickness(3.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct GraphDefaults {
    /// Default node configuration for this graph
    pub node: NodeConfig,
    /// Default edge configuration for this graph
    pub edge: EdgeConfig,
    /// Default pin configuration for this graph
    pub pin: PinConfig,
    /// Graph canvas configuration
    pub graph: GraphConfig,
}

impl GraphDefaults {
    /// Creates empty graph defaults with no overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the default node configuration.
    pub fn node(mut self, config: NodeConfig) -> Self {
        self.node = config;
        self
    }

    /// Sets the default edge configuration.
    pub fn edge(mut self, config: EdgeConfig) -> Self {
        self.edge = config;
        self
    }

    /// Sets the default pin configuration.
    pub fn pin(mut self, config: PinConfig) -> Self {
        self.pin = config;
        self
    }

    /// Sets the graph canvas configuration.
    pub fn graph(mut self, config: GraphConfig) -> Self {
        self.graph = config;
        self
    }

    /// Returns true if any defaults are set.
    pub fn has_overrides(&self) -> bool {
        self.node.has_overrides()
            || self.edge.has_overrides()
            || self.pin.has_overrides()
            || self.graph.has_overrides()
    }
}

/// Central style resolver implementing the three-layer cascade.
///
/// The cascade order is:
/// 1. **Theme Defaults** - Base styles derived from iced::Theme
/// 2. **Graph Defaults** - Graph-wide overrides (optional)
/// 3. **Item Config** - Per-item overrides (optional)
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{StyleResolver, GraphDefaults, NodeConfig};
/// use iced::Theme;
///
/// let graph_defaults = GraphDefaults::new()
///     .node(NodeConfig::new().corner_radius(10.0));
///
/// let resolver = StyleResolver::new(&Theme::Dark, Some(&graph_defaults));
///
/// // Resolve with per-item override
/// let item_config = NodeConfig::new().opacity(0.9);
/// let final_style = resolver.resolve_node(Some(&item_config));
/// ```
#[derive(Debug, Clone)]
pub struct StyleResolver<'a> {
    /// Layer 1: Derived from iced::Theme
    theme_defaults: ThemeDefaults,
    /// Layer 2: Graph-level defaults (optional)
    graph_defaults: Option<&'a GraphDefaults>,
}

impl<'a> StyleResolver<'a> {
    /// Creates a new resolver with theme defaults and optional graph defaults.
    ///
    /// # Arguments
    /// * `theme` - The iced Theme to derive base styles from
    /// * `graph_defaults` - Optional graph-level style overrides
    pub fn new(theme: &Theme, graph_defaults: Option<&'a GraphDefaults>) -> Self {
        Self {
            theme_defaults: ThemeDefaults::from_theme(theme),
            graph_defaults,
        }
    }

    /// Creates a resolver based on dark/light mode detection.
    ///
    /// Use this when you don't have direct access to the iced Theme
    /// but can detect the theme mode via text color luminance.
    ///
    /// # Arguments
    /// * `is_dark` - Whether the theme is dark (detected via text luminance)
    /// * `graph_defaults` - Optional graph-level style overrides
    pub fn from_is_dark(is_dark: bool, graph_defaults: Option<&'a GraphDefaults>) -> Self {
        Self {
            theme_defaults: ThemeDefaults::from_is_dark(is_dark),
            graph_defaults,
        }
    }

    /// Creates a resolver without theme (uses fallback defaults).
    ///
    /// Use this when theme is not available.
    pub fn without_theme(graph_defaults: Option<&'a GraphDefaults>) -> Self {
        Self {
            theme_defaults: ThemeDefaults::fallback(),
            graph_defaults,
        }
    }

    /// Returns a reference to the theme defaults.
    pub fn theme_defaults(&self) -> &ThemeDefaults {
        &self.theme_defaults
    }

    /// Resolves a node's final style through the cascade.
    ///
    /// Cascade order: Theme Defaults -> Graph Defaults -> Item Config
    #[inline]
    pub fn resolve_node(&self, item_config: Option<&NodeConfig>) -> NodeStyle {
        // Start with theme defaults
        let mut result = self.theme_defaults.node.clone();

        // Apply graph defaults if present
        if let Some(graph) = self.graph_defaults {
            if graph.node.has_overrides() {
                result = graph.node.apply_to(&result);
            }
        }

        // Apply item-level config if present
        if let Some(item) = item_config {
            if item.has_overrides() {
                result = item.apply_to(&result);
            }
        }

        result
    }

    /// Resolves an edge's final style through the cascade.
    ///
    /// Cascade order: Theme Defaults -> Graph Defaults -> Item Config
    #[inline]
    pub fn resolve_edge(&self, item_config: Option<&EdgeConfig>) -> EdgeStyle {
        let mut result = self.theme_defaults.edge.clone();

        if let Some(graph) = self.graph_defaults {
            if graph.edge.has_overrides() {
                result = graph.edge.apply_to(&result);
            }
        }

        if let Some(item) = item_config {
            if item.has_overrides() {
                result = item.apply_to(&result);
            }
        }

        result
    }

    /// Resolves a pin's final style through the cascade.
    ///
    /// Cascade order: Theme Defaults -> Graph Defaults -> Item Config
    #[inline]
    pub fn resolve_pin(&self, item_config: Option<&PinConfig>) -> PinStyle {
        let mut result = self.theme_defaults.pin.clone();

        if let Some(graph) = self.graph_defaults {
            if graph.pin.has_overrides() {
                result = graph.pin.apply_to(&result);
            }
        }

        if let Some(item) = item_config {
            if item.has_overrides() {
                result = item.apply_to(&result);
            }
        }

        result
    }

    /// Resolves the graph canvas style through the cascade.
    ///
    /// Cascade order: Theme Defaults -> Graph Defaults
    #[inline]
    pub fn resolve_graph(&self) -> GraphStyle {
        let mut result = self.theme_defaults.graph.clone();

        if let Some(graph) = self.graph_defaults {
            if graph.graph.has_overrides() {
                result = graph.graph.apply_to(&result);
            }
        }

        result
    }
}

/// Resolves a node's final style using the full cascade.
///
/// This is a convenience function for resolving node styles when building
/// node content that needs access to the final resolved values (e.g., for
/// title bar corner radius that should match the node's border settings).
///
/// # Arguments
/// * `theme` - The iced Theme for base styles
/// * `graph_defaults` - Optional graph-level style overrides
/// * `node_config` - Optional per-node style overrides
///
/// # Example
/// ```rust
/// use iced_nodegraph::style::{resolve_node_style, GraphDefaults, NodeConfig};
/// use iced::Theme;
///
/// // Simple case: just theme defaults
/// let style = resolve_node_style(&Theme::Dark, None, None);
/// assert!(style.corner_radius > 0.0);
///
/// // With graph defaults
/// let defaults = GraphDefaults::new()
///     .node(NodeConfig::new().corner_radius(10.0));
/// let style = resolve_node_style(&Theme::Dark, Some(&defaults), None);
/// assert_eq!(style.corner_radius, 10.0);
///
/// // With per-node override
/// let node_cfg = NodeConfig::new().border_width(2.0);
/// let style = resolve_node_style(&Theme::Dark, Some(&defaults), Some(&node_cfg));
/// assert_eq!(style.corner_radius, 10.0); // from graph defaults
/// assert_eq!(style.border_width, 2.0);   // from node config
/// ```
pub fn resolve_node_style(
    theme: &Theme,
    graph_defaults: Option<&GraphDefaults>,
    node_config: Option<&NodeConfig>,
) -> NodeStyle {
    StyleResolver::new(theme, graph_defaults).resolve_node(node_config)
}

#[cfg(test)]
mod tests {
    use iced::Color;

    use super::*;

    #[test]
    fn test_resolver_theme_only() {
        let resolver = StyleResolver::new(&Theme::Dark, None);

        let node_style = resolver.resolve_node(None);
        let edge_style = resolver.resolve_edge(None);
        let graph_style = resolver.resolve_graph();

        // Should return theme defaults
        assert_eq!(node_style.fill_color, resolver.theme_defaults.node.fill_color);
        assert_eq!(edge_style.thickness, resolver.theme_defaults.edge.thickness);
        assert_eq!(
            graph_style.background_color,
            resolver.theme_defaults.graph.background_color
        );
    }

    #[test]
    fn test_resolver_with_graph_defaults() {
        let graph_defaults = GraphDefaults::new()
            .node(NodeConfig::new().corner_radius(20.0))
            .edge(EdgeConfig::new().thickness(5.0));

        let resolver = StyleResolver::new(&Theme::Dark, Some(&graph_defaults));

        let node_style = resolver.resolve_node(None);
        let edge_style = resolver.resolve_edge(None);

        assert_eq!(node_style.corner_radius, 20.0);
        assert_eq!(edge_style.thickness, 5.0);
    }

    #[test]
    fn test_resolver_with_item_override() {
        let graph_defaults = GraphDefaults::new().node(NodeConfig::new().corner_radius(20.0));

        let resolver = StyleResolver::new(&Theme::Dark, Some(&graph_defaults));

        // Item overrides graph default
        let item_config = NodeConfig::new().corner_radius(30.0);
        let node_style = resolver.resolve_node(Some(&item_config));

        assert_eq!(node_style.corner_radius, 30.0);
    }

    #[test]
    fn test_full_cascade() {
        // Theme defaults (base layer)
        let graph_defaults = GraphDefaults::new().node(
            NodeConfig::new()
                .corner_radius(10.0) // Override theme
                .opacity(0.8),       // Override theme
        );

        let resolver = StyleResolver::new(&Theme::Dark, Some(&graph_defaults));

        // Item config (top layer)
        let item_config = NodeConfig::new().fill_color(Color::from_rgb(1.0, 0.0, 0.0)); // Override graph default

        let node_style = resolver.resolve_node(Some(&item_config));

        // Should have:
        // - fill_color from item_config
        // - corner_radius from graph_defaults
        // - opacity from graph_defaults
        // - border_color from theme_defaults
        assert_eq!(node_style.fill_color, Color::from_rgb(1.0, 0.0, 0.0));
        assert_eq!(node_style.corner_radius, 10.0);
        assert_eq!(node_style.opacity, 0.8);
        assert_eq!(
            node_style.border_color,
            resolver.theme_defaults.node.border_color
        );
    }

    #[test]
    fn test_empty_configs_no_overhead() {
        let graph_defaults = GraphDefaults::new(); // Empty
        assert!(!graph_defaults.has_overrides());

        let resolver = StyleResolver::new(&Theme::Dark, Some(&graph_defaults));

        // Should return theme defaults unmodified
        let node_style = resolver.resolve_node(None);
        assert_eq!(node_style.fill_color, resolver.theme_defaults.node.fill_color);
    }

    #[test]
    fn test_without_theme() {
        let resolver = StyleResolver::without_theme(None);

        let node_style = resolver.resolve_node(None);
        let fallback = ThemeDefaults::fallback();

        assert_eq!(node_style.corner_radius, fallback.node.corner_radius);
    }
}
