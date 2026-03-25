//! Segment-based SDF renderer for Iced.
//!
//! Provides exact distance fields by decomposing shapes into individual
//! segments (lines, arcs, bezier curves). Front-to-back rendering with
//! alpha early-out.
//!
//! # Builders
//!
//! - [`Curve`] - Disconnected segments (edges, lines, beziers)
//! - [`Shape`] - Connected contours (nodes with pin cutouts) [Phase 3]
//! - [`Tiling`] - Infinite repeating backgrounds (grid, dots) [Phase 6]
//!
//! # Rendering
//!
//! ```ignore
//! use iced_sdf::{Curve, Style, Pattern, SdfPrimitive};
//!
//! let edge = Curve::single_bezier([0, 0], [30, -20], [70, 20], [100, 0]);
//! let style = Style::stroke(Color::WHITE, Pattern::solid(2.0));
//!
//! let mut prim = SdfPrimitive::new();
//! prim.push(&edge, &style, [0.0, 0.0, 200.0, 100.0]);
//! let prim = prim.camera(cam_x, cam_y, zoom).time(elapsed);
//! ```

pub mod curve;
pub mod drawable;
pub mod pattern;
pub mod style;
pub mod tiling;

pub(crate) mod compile;
pub(crate) mod pipeline;
pub mod primitive;
pub(crate) mod shared;

// Public API re-exports
pub use curve::{Curve, ShapeBuilder};
pub use drawable::Drawable;
pub use pattern::Pattern;
pub use pipeline::types::SdfStats;
pub use primitive::{SdfPrimitive, sdf_stats};
pub use style::Style;
pub use tiling::Tiling;
