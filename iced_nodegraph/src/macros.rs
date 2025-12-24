//! Macro-based API for ergonomic node graph construction.
//!
//! This module provides declarative macros for building node graphs:
//! - [`node_graph!`] - Creates a new graph builder
//! - [`node!`] - Adds a node and returns a handle
//! - [`edge!`] - Connects two pins
//!
//! # Example
//!
//! ```rust,ignore
//! use iced_nodegraph::{node_graph, node, edge, NodeConfig};
//! use iced::widget::text;
//!
//! let mut graph = node_graph!();
//!
//! let node_a = node!(graph, (100.0, 150.0), text("Node A"));
//! let node_b = node!(graph, (300.0, 150.0), text("Node B"),
//!     config: NodeConfig::new().corner_radius(10.0)
//! );
//!
//! edge!(graph, node_a.pin(0) => node_b.pin(0));
//! ```

/// Creates a new NodeGraph builder.
///
/// # Basic Usage
/// ```rust,ignore
/// let mut graph = node_graph!();
/// ```
///
/// # With Configuration
/// ```rust,ignore
/// let mut graph = node_graph!(
///     defaults: GraphDefaults::new().node(NodeConfig::new().corner_radius(10.0)),
///     on_connect: |from, to| Message::Connected { from, to },
///     on_move: |id, pos| Message::Moved { id, pos },
/// );
/// ```
#[macro_export]
macro_rules! node_graph {
    // No arguments - return default builder
    () => {
        $crate::node_graph()
    };

    // With configuration block
    (
        $(defaults: $defaults:expr)?
        $(, on_connect: $on_connect:expr)?
        $(, on_disconnect: $on_disconnect:expr)?
        $(, on_move: $on_move:expr)?
        $(, on_select: $on_select:expr)?
        $(, on_clone: $on_clone:expr)?
        $(, on_delete: $on_delete:expr)?
        $(, on_group_move: $on_group_move:expr)?
        $(, on_event: $on_event:expr)?
        $(,)?
    ) => {{
        #[allow(unused_mut)]
        let mut builder = $crate::node_graph();
        $(builder = builder.defaults($defaults);)?
        $(builder = builder.on_connect($on_connect);)?
        $(builder = builder.on_disconnect($on_disconnect);)?
        $(builder = builder.on_move($on_move);)?
        $(builder = builder.on_select($on_select);)?
        $(builder = builder.on_clone($on_clone);)?
        $(builder = builder.on_delete($on_delete);)?
        $(builder = builder.on_group_move($on_group_move);)?
        $(builder = builder.on_event($on_event);)?
        builder
    }};
}

/// Adds a node to the graph and returns a [`crate::NodeHandle`].
///
/// # Basic Usage
/// ```rust,ignore
/// let node_a = node!(graph, (100.0, 150.0), text("Node A"));
/// ```
///
/// # With Config
/// ```rust,ignore
/// let node_b = node!(graph, (300.0, 150.0), text("Node B"),
///     config: NodeConfig::new().fill_color(Color::from_rgb(0.2, 0.3, 0.4))
/// );
/// ```
///
/// # With Style Preset
/// ```rust,ignore
/// let node_c = node!(graph, (500.0, 150.0), text("Node C"),
///     style: NodeStyle::input()
/// );
/// ```
#[macro_export]
macro_rules! node {
    // Basic: graph, position, content
    ($graph:expr, ($x:expr, $y:expr), $content:expr) => {{
        let node_id = $graph.push_node_returning(
            $crate::iced::Point::new($x as f32, $y as f32),
            $content,
        );
        $crate::NodeHandle::new(node_id)
    }};

    // With config (partial overrides)
    ($graph:expr, ($x:expr, $y:expr), $content:expr, config: $config:expr) => {{
        let node_id = $graph.push_node_config_returning(
            $crate::iced::Point::new($x as f32, $y as f32),
            $content,
            $config,
        );
        $crate::NodeHandle::new(node_id)
    }};

    // With complete style (for presets)
    ($graph:expr, ($x:expr, $y:expr), $content:expr, style: $style:expr) => {{
        let node_id = $graph.push_node_config_returning(
            $crate::iced::Point::new($x as f32, $y as f32),
            $content,
            $crate::NodeConfig::from($style),
        );
        $crate::NodeHandle::new(node_id)
    }};
}

/// Adds an edge connecting two pins.
///
/// # Using NodeHandles
/// ```rust,ignore
/// let node_a = node!(graph, (100.0, 100.0), text("A"));
/// let node_b = node!(graph, (300.0, 100.0), text("B"));
/// edge!(graph, node_a.pin(0) => node_b.pin(0));
/// ```
///
/// # Using PinReferences directly
/// ```rust,ignore
/// edge!(graph, PinReference::new(0, 0) => PinReference::new(1, 0));
/// ```
///
/// # With Config
/// ```rust,ignore
/// edge!(graph, from => to,
///     config: EdgeConfig::new().thickness(3.0)
/// );
/// ```
///
/// # With Style Preset
/// ```rust,ignore
/// edge!(graph, from => to,
///     style: EdgeStyle::data_flow()
/// );
/// ```
#[macro_export]
macro_rules! edge {
    // Basic: from => to
    ($graph:expr, $from:expr => $to:expr) => {
        $graph.push_edge($from.into(), $to.into())
    };

    // With config (partial overrides)
    ($graph:expr, $from:expr => $to:expr, config: $config:expr) => {{
        use $crate::Cascade;
        let resolved = $config.apply_to(&$crate::EdgeStyle::default());
        $graph.push_edge_styled($from.into(), $to.into(), resolved)
    }};

    // With complete style
    ($graph:expr, $from:expr => $to:expr, style: $style:expr) => {
        $graph.push_edge_styled($from.into(), $to.into(), $style)
    };
}
