//! Common imports for building a node graph view.
//!
//! `use iced_nodegraph::prelude::*;` pulls in the vocabulary reached for in
//! almost every `view()`: the builders, [`PinRef`], the pin and
//! status types used by `style`/`can_connect` closures, the concrete style
//! structs with their theme-derived `default_*` bases, and the node-content
//! helpers. Graph-level configuration set once (`Camera2D`, `GraphStyle`,
//! `SelectionStyle`) is imported explicitly when opted into.

// Builders: the entry point, the node/edge/pin constructors and the types they
// return (named when writing helpers per node type), and the `pin!` macro.
pub use crate::{Edge, Node, edge, node, node_graph, node_pin, pin};

// Core types named when wiring callbacks and edges.
pub use crate::{NodeGraph, PinRef};

// Pin and status vocabulary passed to `style` / `pin_style` / `can_connect` closures.
pub use crate::{EdgeStatus, NodeStatus, PinDirection, PinEnd, PinInfo, PinSide, PinStatus};

// Input rebinding: the keymap and its combo vocabulary.
pub use crate::{ComboKey, KeyAction, KeyCombo, Keymap};

// Composable `can_connect` predicates (compose `default_can_connect` to keep the
// built-in rules when overriding validation).
pub use crate::connection::{default_can_connect, direction_ok, input_not_occupied, not_same_node};

// Concrete style structs and their theme-derived defaults to layer overrides over.
pub use crate::{
    ColorQuad, EdgeCurve, EdgeStyle, NodeStyle, Pattern, PinShape, PinStyle, default_edge_style,
    default_node_style, default_pin_style,
};

// Rounded header/footer helpers for node interiors.
pub use crate::{node_footer, node_header};
