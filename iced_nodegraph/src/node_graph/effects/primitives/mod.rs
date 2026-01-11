//! Separate Iced primitives for NodeGraph rendering.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `EdgesPrimitive` - Batched edge rendering
//! - `NodePrimitive` - Single node with Background/Foreground layer support
//! - `BoxSelectPrimitive` - Box selection overlay
//! - `CuttingToolPrimitive` - Edge cutting line overlay

mod cutting_tool;
mod edges;
mod grid;
mod node;
mod select_box;

pub use cutting_tool::CuttingToolPrimitive;
pub use edges::{EdgeRenderData, EdgesPrimitive};
pub use grid::GridPrimitive;
pub use node::{NodeLayer, NodePrimitive, PinRenderData};
pub use select_box::BoxSelectPrimitive;
