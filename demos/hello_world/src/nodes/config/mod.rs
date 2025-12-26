//! Config Nodes for Style Configuration
//!
//! These nodes allow building style configurations through node connections.
//! Each config node has:
//! - A config input pin for inheritance (merge with parent config)
//! - Individual field input pins (None when not connected)
//! - A config output pin for passing the merged config

pub mod apply;
pub mod edge_config;
pub mod node_config;
pub mod pin_config;
pub mod shadow_config;

pub use apply::{apply_to_graph_node, apply_to_node_node};
pub use edge_config::{EdgeConfigInputs, edge_config_node};
pub use node_config::{NodeConfigInputs, node_config_node};
pub use pin_config::{PinConfigInputs, pin_config_node};
pub use shadow_config::{ShadowConfigInputs, shadow_config_node};
