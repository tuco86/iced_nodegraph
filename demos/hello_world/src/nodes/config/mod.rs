//! Config Nodes for Style Configuration
//!
//! These nodes allow building style configurations through node connections.
//! There is one config node per `iced_nodegraph::Catalog` class, each with:
//! - A config input pin for inheritance (merge with parent config)
//! - Individual field input pins (None when not connected)
//! - A config output pin for passing the merged config
//!
//! The `Catalog` node is the sink every chain ends in; `Node Class` assigns a
//! node config to a single node instead.

pub mod anchor_config;
pub mod catalog;
pub mod cutting_tool_config;
pub mod edge_config;
pub mod graph_config;
pub mod minimap_config;
pub mod node_class;
pub mod node_config;
pub mod pin_config;
pub mod selection_box_config;

pub use anchor_config::{AnchorConfigInputs, anchor_config_node};
pub use catalog::catalog_node;
pub use cutting_tool_config::{CuttingToolConfigInputs, cutting_tool_config_node};
pub use edge_config::{EdgeConfigInputs, EdgeSection, EdgeSections, PatternType, edge_config_node};
pub use graph_config::{GraphConfigInputs, graph_config_node};
pub use minimap_config::{MinimapConfigInputs, minimap_config_node};
pub use node_class::{ClassCandidate, node_class_node};
pub use node_config::{NodeConfigInputs, NodeSection, NodeSections, node_config_node};
pub use pin_config::{PinConfigInputs, pin_config_node};
pub use selection_box_config::{SelectionBoxConfigInputs, selection_box_config_node};
