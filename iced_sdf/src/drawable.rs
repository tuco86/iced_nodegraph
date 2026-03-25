//! Compiled drawable: the result of building a Curve, Shape, or Tiling.
//!
//! A Drawable holds pre-computed segment geometry and arc-length data,
//! ready for upload to the GPU.

use glam::Vec2;

/// Segment type discriminant (matches GPU constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SegmentType {
    Line = 0,
    Arc = 1,
    CubicBezier = 2,
    /// Junction point between segments. Defines sign at corners.
    /// geom0 = (px, py, heading, 0)
    Point = 3,
}

/// A single geometric segment with arc-length parameterization.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub segment_type: SegmentType,
    /// Geometry: interpretation depends on segment_type.
    /// Line: geom0 = (ax, ay, bx, by), geom1 unused
    /// CubicBezier: geom0 = (p0x, p0y, p1x, p1y), geom1 = (p2x, p2y, p3x, p3y)
    /// Arc: geom0 = (cx, cy, radius, start_angle), geom1 = (sweep_angle, 0, 0, 0)
    pub geom0: [f32; 4],
    pub geom1: [f32; 4],
    /// Cumulative arc length at segment start.
    pub arc_start: f32,
    /// Cumulative arc length at segment end.
    pub arc_end: f32,
}

/// Draw entry type discriminant (matches GPU constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DrawableType {
    /// Single curve segment (line or bezier). Stroke only.
    CurveSegment = 0,
    /// Closed or open shape (multiple connected segments). May be filled.
    Shape = 1,
    /// Infinite repeating tiling pattern.
    Tiling = 2,
}

/// Tiling type discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TilingType {
    Grid = 0,
    Dots = 1,
}

/// Compiled result from a Curve, Shape, or Tiling builder.
#[derive(Debug, Clone)]
pub struct Drawable {
    pub(crate) drawable_type: DrawableType,
    pub(crate) segments: Vec<Segment>,
    pub(crate) total_arc_length: f32,
    pub(crate) bounds: [f32; 4], // world-space AABB: [min_x, min_y, max_x, max_y]
    pub(crate) is_closed: bool,
    // Tiling-specific
    pub(crate) tiling_type: Option<TilingType>,
    pub(crate) tiling_params: [f32; 4],
}

impl Drawable {
    /// Total arc length of the drawable (sum of all segment lengths).
    pub fn total_arc_length(&self) -> f32 {
        self.total_arc_length
    }

    /// World-space bounding box: [min_x, min_y, max_x, max_y].
    pub fn bounds(&self) -> [f32; 4] {
        self.bounds
    }

    /// Whether this drawable is a closed contour (fillable).
    pub fn is_closed(&self) -> bool {
        self.is_closed
    }

    /// Number of geometric segments.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Create a line segment drawable (convenience for Curve::single_line).
    pub(crate) fn single_line(a: Vec2, b: Vec2) -> Self {
        let length = a.distance(b);
        let min_x = a.x.min(b.x);
        let min_y = a.y.min(b.y);
        let max_x = a.x.max(b.x);
        let max_y = a.y.max(b.y);
        Self {
            drawable_type: DrawableType::CurveSegment,
            segments: vec![Segment {
                segment_type: SegmentType::Line,
                geom0: [a.x, a.y, b.x, b.y],
                geom1: [0.0; 4],
                arc_start: 0.0,
                arc_end: length,
            }],
            total_arc_length: length,
            bounds: [min_x, min_y, max_x, max_y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Create a single point segment drawable.
    pub(crate) fn single_point(pos: Vec2, heading: f32) -> Self {
        Self {
            drawable_type: DrawableType::CurveSegment,
            segments: vec![Segment {
                segment_type: SegmentType::Point,
                geom0: [pos.x, pos.y, heading, 0.0],
                geom1: [0.0; 4],
                arc_start: 0.0,
                arc_end: 0.0,
            }],
            total_arc_length: 0.0,
            bounds: [pos.x, pos.y, pos.x, pos.y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Create a single arc segment drawable.
    pub(crate) fn single_arc(center: Vec2, radius: f32, start_angle: f32, sweep: f32) -> Self {
        let arc_length = sweep.abs() * radius;
        Self {
            drawable_type: DrawableType::CurveSegment,
            segments: vec![Segment {
                segment_type: SegmentType::Arc,
                geom0: [center.x, center.y, radius, start_angle],
                geom1: [sweep, 0.0, 0.0, 0.0],
                arc_start: 0.0,
                arc_end: arc_length,
            }],
            total_arc_length: arc_length,
            bounds: [center.x - radius, center.y - radius, center.x + radius, center.y + radius],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Create a cubic bezier segment drawable (convenience for Curve::single_bezier).
    pub(crate) fn single_bezier(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Self {
        let length = bezier_arc_length(p0, p1, p2, p3);
        let min_x = p0.x.min(p1.x).min(p2.x).min(p3.x);
        let min_y = p0.y.min(p1.y).min(p2.y).min(p3.y);
        let max_x = p0.x.max(p1.x).max(p2.x).max(p3.x);
        let max_y = p0.y.max(p1.y).max(p2.y).max(p3.y);
        Self {
            drawable_type: DrawableType::CurveSegment,
            segments: vec![Segment {
                segment_type: SegmentType::CubicBezier,
                geom0: [p0.x, p0.y, p1.x, p1.y],
                geom1: [p2.x, p2.y, p3.x, p3.y],
                arc_start: 0.0,
                arc_end: length,
            }],
            total_arc_length: length,
            bounds: [min_x, min_y, max_x, max_y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Create a tiling drawable.
    pub(crate) fn new_tiling(tiling_type: TilingType, params: [f32; 4]) -> Self {
        Self {
            drawable_type: DrawableType::Tiling,
            segments: Vec::new(),
            total_arc_length: 0.0,
            bounds: [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::INFINITY],
            is_closed: false,
            tiling_type: Some(tiling_type),
            tiling_params: params,
        }
    }
}

/// Compute cubic bezier arc length using 5-point Gauss-Legendre quadrature.
pub(crate) fn bezier_arc_length(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
    // Gauss-Legendre 5-point weights and abscissae on [0, 1]
    const POINTS: [(f32, f32); 5] = [
        (0.5 * 0.2369269, 0.5 * (1.0 - 0.9061798)),
        (0.5 * 0.4786287, 0.5 * (1.0 - 0.5384693)),
        (0.5 * 0.5688889, 0.5),
        (0.5 * 0.4786287, 0.5 * (1.0 + 0.5384693)),
        (0.5 * 0.2369269, 0.5 * (1.0 + 0.9061798)),
    ];

    let mut length = 0.0;
    for &(w, t) in &POINTS {
        let dt = bezier_derivative(p0, p1, p2, p3, t);
        length += w * dt.length();
    }
    length
}

/// Cubic bezier derivative at parameter t.
fn bezier_derivative(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    3.0 * u * u * (p1 - p0) + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (p3 - p2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line() {
        let d = Drawable::single_line(Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert_eq!(d.segment_count(), 1);
        assert!((d.total_arc_length() - 10.0).abs() < 0.001);
        assert!(!d.is_closed());
    }

    #[test]
    fn test_single_bezier() {
        let d = Drawable::single_bezier(
            Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0),
            Vec2::new(20.0, 0.0), Vec2::new(30.0, 0.0),
        );
        assert_eq!(d.segment_count(), 1);
        // Straight-line bezier should have arc length ~30
        assert!((d.total_arc_length() - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_bezier_arc_length_curved() {
        let len = bezier_arc_length(
            Vec2::new(0.0, 0.0), Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0),
        );
        // Quarter-circle-ish curve, should be > straight distance (14.14) and < perimeter
        assert!(len > 14.0);
        assert!(len < 30.0);
    }
}
