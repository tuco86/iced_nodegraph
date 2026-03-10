//! GPU rendering effects for NodeGraph.
//!
//! Each primitive type participates in Iced's layer system for correct compositing:
//! - `GridPrimitive` - Background grid pattern
//! - `NodePrimitive` - Node with background/foreground layer support (includes pins)
//! - `EdgePrimitive` - Single edge rendering
//!
//! Overlays (box selection, edge cutting) use `iced_sdf::SdfPrimitive` directly.

pub use primitives::{GridPrimitive, NodeLayer, NodePrimitive, PinRenderData, RenderContext};

pub(crate) mod pipeline;
pub mod primitives;
pub(crate) mod shared;
