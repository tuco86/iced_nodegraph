//! Separate Iced primitives for NodeGraph rendering.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `EdgePrimitive` - Single edge rendering
//! - `NodePrimitive` - Single node with Background/Foreground layer support
//!
//! Overlays (box selection, edge cutting) use `iced_sdf::SdfPrimitive` directly.

use crate::node_graph::euclid::WorldPoint;

mod edge;
mod grid;
mod node;

/// Shared per-frame rendering context for all primitives.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext {
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,
    pub time: f32,
}

pub use edge::EdgePrimitive;
pub use grid::GridPrimitive;
pub use node::{NodeLayer, NodePrimitive, PinRenderData};
