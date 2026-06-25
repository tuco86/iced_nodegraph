//! Compiled drawable: the result of building a Curve, Shape, or Tiling.
//!
//! A Drawable holds pre-computed segment geometry and arc-length data,
//! ready for upload to the GPU.

use glam::Vec2;

/// A single arc segment - the ONE geometric primitive ("Arc is all you need").
///
/// Encoded by its endpoints plus a signed curvature (see [`crate::segment`]):
/// `curvature == 0` is a straight line, `start == end` is a point (junction
/// marker, sign from `heading`), otherwise it is the minor arc of radius
/// `1/|curvature|` bulging to the side `curvature`'s sign selects. There is no
/// separate Line / Cubic / Point type: those are degenerate arcs.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    /// Part of a closed contour: SDF returns signed distance (negative = interior).
    pub signed: bool,
    pub start: Vec2,
    pub end: Vec2,
    /// Signed curvature (`1/radius`); `0` = line. Sign selects the bulge side.
    pub curvature: f32,
    /// Interior-bisector heading; only meaningful for a point (`start == end`).
    pub heading: f32,
    /// Cumulative arc length at segment start.
    pub arc_start: f32,
    /// Cumulative arc length at segment end.
    pub arc_end: f32,
}

impl Segment {
    /// Straight line `start -> end`.
    pub(crate) fn line(start: Vec2, end: Vec2, signed: bool, arc_start: f32, arc_end: f32) -> Self {
        Self { signed, start, end, curvature: 0.0, heading: 0.0, arc_start, arc_end }
    }

    /// Zero-length junction point at `pos`; `heading` is the interior bisector.
    pub(crate) fn point(pos: Vec2, heading: f32, signed: bool, arc_at: f32) -> Self {
        Self { signed, start: pos, end: pos, curvature: 0.0, heading, arc_start: arc_at, arc_end: arc_at }
    }

    /// One MINOR arc; the caller guarantees `|sweep| < PI` (use [`Self::push_arc`]
    /// for arbitrary sweeps, which splits wider arcs).
    pub(crate) fn arc(
        center: Vec2, radius: f32, start_angle: f32, sweep: f32,
        signed: bool, arc_start: f32, arc_end: f32,
    ) -> Self {
        let (start, end, curvature) = crate::segment::from_center_arc(center, radius, start_angle, sweep);
        Self { signed, start, end, curvature, heading: 0.0, arc_start, arc_end }
    }

    /// Append `(center, radius, start_angle, sweep)` as one or more MINOR sub-arcs
    /// (each `|sweep| < PI`; a full-circle pin becomes 4 quarters), advancing the
    /// cumulative arc length `acc`. The single place arcs are split for the
    /// arc-only model, so the endpoint+curvature reconstruction stays unambiguous.
    pub(crate) fn push_arc(
        out: &mut Vec<Segment>, center: Vec2, radius: f32, start_angle: f32, sweep: f32,
        signed: bool, acc: &mut f32,
    ) {
        let n = ((sweep.abs() / (std::f32::consts::PI * 0.5)).ceil() as u32).max(1);
        let sub = sweep / n as f32;
        let seg_len = radius * sub.abs();
        for i in 0..n {
            let a0 = start_angle + sub * i as f32;
            out.push(Segment::arc(center, radius, a0, sub, signed, *acc, *acc + seg_len));
            *acc += seg_len;
        }
    }

    /// Grow an AABB `(min, max)` by this segment's exact extent.
    pub(crate) fn grow_aabb(&self, min: &mut Vec2, max: &mut Vec2) {
        let (lo, hi) = crate::segment::seg_aabb(self.start, self.end, self.curvature);
        *min = min.min(lo);
        *max = max.max(hi);
    }
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
    Triangles = 2,
    Hex = 3,
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

    /// Returns a copy shifted by `(dx, dy)` in world space.
    ///
    /// Translates every segment's positional geometry and the cached bounds;
    /// radii, angles and arc lengths are translation-invariant. Cheaper than
    /// rebuilding a shape at a new origin, e.g. to reuse a node silhouette for
    /// its offset shadow. Curve and shape drawables only; a tiling pattern's
    /// origin is not adjusted.
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        let mut out = self.clone();
        let off = Vec2::new(dx, dy);
        for seg in &mut out.segments {
            // Endpoints are positions; curvature/heading are translation-invariant.
            seg.start += off;
            seg.end += off;
        }
        out.bounds[0] += dx;
        out.bounds[1] += dy;
        out.bounds[2] += dx;
        out.bounds[3] += dy;
        out
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
            segments: vec![Segment::line(a, b, false, 0.0, length)],
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
            segments: vec![Segment::point(pos, heading, false, 0.0)],
            total_arc_length: 0.0,
            bounds: [pos.x, pos.y, pos.x, pos.y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Create a single arc segment drawable (`sweep` of any magnitude, split into
    /// minor sub-arcs).
    pub(crate) fn single_arc(center: Vec2, radius: f32, start_angle: f32, sweep: f32) -> Self {
        let mut segments = Vec::new();
        let mut acc = 0.0;
        Segment::push_arc(&mut segments, center, radius, start_angle, sweep, false, &mut acc);
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for s in &segments {
            s.grow_aabb(&mut lo, &mut hi);
        }
        Self {
            drawable_type: DrawableType::CurveSegment,
            segments,
            total_arc_length: acc,
            bounds: [lo.x, lo.y, hi.x, hi.y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Approximate a cubic bezier as an arc-spline drawable (the v3 "arcs-only"
    /// edge): the cubic is fit by circular arcs and lines within `tol` world
    /// units, deleting the per-pixel bezier SDF. Segments carry cumulative
    /// arc length so dash spacing and flow speed match the bezier's `u`.
    // Wired into the widget edge build path under `sdf-v3` in a later step; for
    // now exercised by the arc-spline golden gate.
    #[allow(dead_code)]
    pub(crate) fn bezier_arcs(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, tol: f32) -> Self {
        use crate::biarc::{ArcPiece, cubic_to_arcs};

        let pieces = cubic_to_arcs(p0, p1, p2, p3, tol);
        let mut segments = Vec::with_capacity(pieces.len());
        let mut cum = 0.0_f32;

        for piece in &pieces {
            let len = piece.length();
            match *piece {
                // Biarc pieces are already minor arcs (|sweep| < 0.95*PI), so each
                // becomes one Segment; push_arc would split only a wider sweep.
                ArcPiece::Arc { center, radius, start_angle, sweep, .. } => {
                    Segment::push_arc(&mut segments, center, radius, start_angle, sweep, false, &mut cum);
                }
                ArcPiece::Line { start, end, .. } => {
                    segments.push(Segment::line(start, end, false, cum, cum + len));
                    cum += len;
                }
            }
        }

        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for s in &segments {
            s.grow_aabb(&mut lo, &mut hi);
        }

        Self {
            drawable_type: DrawableType::CurveSegment,
            segments,
            total_arc_length: cum,
            bounds: [lo.x, lo.y, hi.x, hi.y],
            is_closed: false,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }

    /// Assemble a closed shape from segments produced by a boolean operation.
    pub(crate) fn from_boolean_segments(
        segments: Vec<Segment>,
        total_arc_length: f32,
        bounds: [f32; 4],
    ) -> Self {
        Self {
            drawable_type: DrawableType::Shape,
            segments,
            total_arc_length,
            bounds,
            is_closed: true,
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
            bounds: [
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ],
            is_closed: false,
            tiling_type: Some(tiling_type),
            tiling_params: params,
        }
    }
}

/// Whether the cardinal angle `theta` lies within the arc that starts at
/// `start_angle` and turns by signed `sweep` (radians). Used for tight arc bbox.
#[allow(dead_code)]
fn angle_in_arc(start_angle: f32, sweep: f32, theta: f32) -> bool {
    let lo = start_angle.min(start_angle + sweep);
    let span = sweep.abs();
    let d = (theta - lo).rem_euclid(std::f32::consts::TAU);
    d <= span + 1e-4
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
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(20.0, 0.0),
            Vec2::new(30.0, 0.0),
        );
        assert_eq!(d.segment_count(), 1);
        // Straight-line bezier should have arc length ~30
        assert!((d.total_arc_length() - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_bezier_arc_length_curved() {
        let len = bezier_arc_length(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(10.0, 0.0),
        );
        // Quarter-circle-ish curve, should be > straight distance (14.14) and < perimeter
        assert!(len > 14.0);
        assert!(len < 30.0);
    }

    #[test]
    fn translated_shifts_geometry_and_bounds() {
        let d = Drawable::single_line(Vec2::new(1.0, 2.0), Vec2::new(4.0, 6.0));
        let t = d.translated(10.0, -3.0);

        // Endpoints (geom0 = ax, ay, bx, by) move by the offset.
        assert_eq!(t.segments[0].geom0, [11.0, -1.0, 14.0, 3.0]);
        // Bounds shift with the geometry.
        assert_eq!(t.bounds(), [11.0, -1.0, 14.0, 3.0]);
        // Translation-invariant data is preserved; the original is untouched.
        assert!((t.total_arc_length() - d.total_arc_length()).abs() < 1e-6);
        assert_eq!(d.segments[0].geom0, [1.0, 2.0, 4.0, 6.0]);
    }
}
