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
//! use iced_sdf::{Sdf, Layer, SdfPrimitive};
//! use iced::Color;
//!
//! // Create a shape with holes
//! let shape = Sdf::rounded_box([0.0, 0.0], [100.0, 50.0], 8.0)
//!     .subtract(Sdf::circle([50.0, 0.0], 15.0))
//!     .subtract(Sdf::circle([-50.0, 0.0], 15.0));
//!
//! // Create layers for rendering (back to front)
//! let layers = vec![
//!     Layer::solid(Color::BLACK).expand(8.0).blur(4.0),  // Shadow
//!     Layer::solid(Color::from_rgb(0.3, 0.3, 0.3)).expand(2.0),  // Outline
//!     Layer::solid(Color::from_rgb(0.8, 0.8, 0.8)),  // Fill
//! ];
//!
//! // Create the primitive
//! let primitive = SdfPrimitive::new(shape).layers(layers);
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

pub mod compile;
pub mod eval;
pub mod layer;
pub mod pattern;
pub mod pipeline;
pub mod primitive;
pub mod shape;
pub mod shared;

// Public API re-exports
pub use eval::{evaluate, SdfResult};
pub use layer::Layer;
pub use pattern::Pattern;
pub use primitive::SdfPrimitive;
pub use shape::{Sdf, SdfNode};
