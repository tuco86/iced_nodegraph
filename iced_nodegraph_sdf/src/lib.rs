//! Segment-based SDF renderer for Iced.
//!
//! Provides exact, resolution-independent distance fields from a single
//! geometric primitive: the circular arc, stored as endpoints plus a signed
//! curvature. A straight line is an arc of zero curvature, a point an arc of
//! zero length, and a cubic bezier a CPU-fitted spline of arcs - so the GPU
//! evaluates one distance function for every shape. Closed contours combine via
//! [`boolean`] set operations, a tile spatial index culls per pixel, and
//! front-to-back premultiplied compositing has an opaque early-out.
//!
//! See `README.md` and `ARCHITECTURE.md` for the full design and its invariants.
//!
//! This crate is the rendering engine behind `iced_nodegraph`. Most users do not
//! depend on it directly: `iced_nodegraph` drives it internally and re-exports the
//! part of its surface that node-graph styling touches (`Pattern` and its
//! `PatternType`). Reach for this crate directly only for custom SDF rendering.
//!
//! # Builders
//!
//! - [`Curve`] - Disconnected segments and factory shapes (edges, lines, beziers, rects, circles)
//! - [`ShapeBuilder`] - Connected open or closed contours (nodes, pin cutouts)
//! - [`Tiling`] - Infinite repeating backgrounds (grid, dots, triangles, hex)
//!
//! Closed contours combine via [`boolean`] operations (union, difference,
//! intersection) for compound shapes such as node bodies with pin cutouts.
//!
//! # Rendering
//!
//! ```no_run
//! use iced_nodegraph_sdf::{Shape, Style, Pattern, SdfPrimitive};
//! use iced::Color;
//!
//! let (cam_x, cam_y, zoom, elapsed) = (0.0, 0.0, 1.0, 0.0);
//!
//! // A node body: a rounded box with two pin cutouts, authored as set algebra.
//! let node = Shape::rounded_box([160.0, 90.0], [8.0; 4])
//!     - Shape::circle(5.0).translate([-80.0, -20.0])
//!     - Shape::circle(5.0).translate([80.0, 20.0]);
//! let style = Style::stroke(Color::WHITE, Pattern::solid(2.0));
//!
//! let mut prim = SdfPrimitive::new();
//! prim.push(&node, &style, [300.0, 200.0]); // placed at world (300, 200)
//! let prim = prim.camera(cam_x, cam_y, zoom).time(elapsed);
//! ```

pub(crate) mod biarc;
pub mod boolean;
pub mod color;
pub mod curve;
pub mod drawable;
pub mod pattern;
pub(crate) mod segment;
mod shape;
pub mod style;
pub mod tiling;

pub(crate) mod compile;
pub(crate) mod pipeline;
pub mod primitive;
pub(crate) mod shared;

// Public API re-exports
pub use color::ColorQuad;
pub use curve::{Curve, ShapeBuilder};
pub use drawable::Drawable;
pub use pattern::Pattern;
pub use pipeline::types::SdfStats;
pub use primitive::{DebugFlags, SdfPrimitive, sdf_stats};
pub use shape::{Shape, ShapeCache};
pub use style::{Stop, Style, Transfer};
pub use tiling::Tiling;
