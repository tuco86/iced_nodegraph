//! Generic SDF (Signed Distance Field) renderer for Iced.
//!
//! This crate provides a GPU-accelerated SDF renderer with:
//! - Combinable SDF primitives (Circle, Box, RoundedBox, Line, Bezier)
//! - Boolean operations (Union, Subtract, Intersect)
//! - Smooth blending operations
//! - Layer-based rendering with expand, blur, and gradient support
//!
//! # Example
//!
//! ```ignore
//! use iced_sdf::{Sdf, SdfPrimitive, Layer};
//! use iced::Color;
//!
//! // Each SdfPrimitive is submitted individually via renderer.draw_primitive().
//! // Iced's pipeline automatically batches them into shared GPU buffers.
//!
//! let node = SdfPrimitive::new(
//!     Sdf::rounded_box([50.0, 25.0], [50.0, 25.0], 8.0),
//! )
//! .layers(vec![
//!     Layer::solid(Color::BLACK).expand(6.0).blur(4.0),  // Shadow
//!     Layer::solid(Color::from_rgb(0.8, 0.8, 0.8)),      // Fill
//! ])
//! .screen_bounds([0.0, 0.0, 120.0, 70.0])
//! .camera(cam_x, cam_y, zoom);
//!
//! renderer.draw_primitive(bounds, node);
//! ```
//!
//! # Operators
//!
//! For ergonomic API, operators are available:
//! - `a | b` = union
//! - `a - b` = subtract
//!
//! ```ignore
//! let combined = Sdf::circle([0.0, 0.0], 20.0)
//!     | Sdf::circle([30.0, 0.0], 20.0);  // Union
//!
//! let punched = Sdf::rect([0.0, 0.0], [50.0, 50.0])
//!     - Sdf::circle([0.0, 0.0], 25.0);  // Subtract
//! ```

pub mod batch;
pub mod compile;
pub mod eval;
pub mod layer;
pub mod pattern;
pub mod pipeline;
pub mod primitive;
pub mod shape;
pub mod shared;

// Public API re-exports
pub use batch::SdfBatch;
pub use eval::{evaluate, SdfResult};
pub use layer::Layer;
pub use pattern::Pattern;
pub use primitive::{SdfBatchPrimitive, SdfPrimitive};
pub use shape::{Sdf, SdfNode};
