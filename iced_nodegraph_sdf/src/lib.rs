//! Segment-based SDF renderer for Iced.
//!
//! Provides exact distance fields by decomposing shapes into individual
//! segments (lines, arcs, bezier curves). Front-to-back rendering with
//! alpha early-out.
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
//! use iced_nodegraph_sdf::{Curve, Style, Pattern, SdfPrimitive};
//! use iced::Color;
//!
//! let (cam_x, cam_y, zoom, elapsed) = (0.0, 0.0, 1.0, 0.0);
//!
//! let edge = Curve::bezier([0.0, 0.0], [30.0, -20.0], [70.0, 20.0], [100.0, 0.0]);
//! let style = Style::stroke(Color::WHITE, Pattern::solid(2.0));
//!
//! let mut prim = SdfPrimitive::new();
//! prim.push(&edge, &style);
//! let prim = prim.camera(cam_x, cam_y, zoom).time(elapsed);
//! ```

pub mod boolean;
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
