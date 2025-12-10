pub use primitive::Node;
pub use primitive::Pin;
pub use primitive::{GpuPhysicsRequest, Layer, NodeGraphPrimitive, PhysicsEdgeData, PhysicsVertexData};
// pub use primitive::Edge;

// Re-export physics types for external use
pub use pipeline::types::{PhysicsEdgeMeta, PhysicsUniforms, PhysicsVertex};

pub(crate) mod pipeline;
mod primitive;
