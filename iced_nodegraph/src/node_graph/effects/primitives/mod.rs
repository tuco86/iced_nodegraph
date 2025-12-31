//! Separate Iced primitives for NodeGraph rendering.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `EdgesPrimitive` - Batched edge rendering
//! - `NodePrimitive` - Single node with Background/Foreground layer support

mod edges;
mod grid;
mod node;

pub use edges::{EdgeRenderData, EdgesPrimitive};
pub use grid::GridPrimitive;
pub use node::{NodeLayer, NodePrimitive, PinRenderData};
