//! Separate Iced primitives for NodeGraph rendering.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `EdgePrimitive` - Single edge rendering
//! - `NodePrimitive` - Single node with Background/Foreground layer support
//! - `BoxSelectPrimitive` - Box selection overlay
//! - `CuttingToolPrimitive` - Edge cutting line overlay

use crate::node_graph::euclid::WorldPoint;

mod cutting_tool;
mod edge;
mod grid;
mod node;
mod select_box;

/// Shared per-frame rendering context for all primitives.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext {
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,
    pub time: f32,
}

pub use cutting_tool::CuttingToolPrimitive;
pub use edge::EdgePrimitive;
pub use grid::GridPrimitive;
pub use node::{NodeLayer, NodePrimitive, PinRenderData};
pub use select_box::BoxSelectPrimitive;
