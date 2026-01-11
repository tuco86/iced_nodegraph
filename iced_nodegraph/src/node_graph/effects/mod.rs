//! GPU rendering effects for NodeGraph.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `NodePrimitive` - Node with background/foreground layer support (includes pins)
//! - `EdgesPrimitive` - Batched edge rendering
//! - `BoxSelectPrimitive` - Box selection overlay
//! - `CuttingToolPrimitive` - Edge cutting line overlay

pub use primitives::{
    BoxSelectPrimitive, CuttingToolPrimitive, EdgeRenderData, EdgesPrimitive, GridPrimitive,
    NodeLayer, NodePrimitive, PinRenderData,
};

pub(crate) mod pipeline;
pub mod primitives;
pub(crate) mod shared;
