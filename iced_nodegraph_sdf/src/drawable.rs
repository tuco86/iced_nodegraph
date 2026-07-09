//! Compiled drawable: the result of building a Curve, Shape, or Tiling.
//!
//! A Drawable holds pre-computed segment geometry and arc-length data,
//! ready for upload to the GPU.

use glam::Vec2;

/// A single arc segment - the ONE geometric primitive ("Arc is all you need").
///
/// Encoded by its endpoints plus a signed curvature (see `crate::segment`):
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
        Self {
            signed,
            start,
            end,
            curvature: 0.0,
            heading: 0.0,
            arc_start,
            arc_end,
        }
    }

    /// Zero-length junction point at `pos`; `heading` is the interior bisector.
    pub(crate) fn point(pos: Vec2, heading: f32, signed: bool, arc_at: f32) -> Self {
        Self {
            signed,
            start: pos,
            end: pos,
            curvature: 0.0,
            heading,
            arc_start: arc_at,
            arc_end: arc_at,
        }
    }

    /// One MINOR arc; the caller guarantees `|sweep| < PI` (use [`Self::push_arc`]
    /// for arbitrary sweeps, which splits wider arcs).
    pub(crate) fn arc(
        center: Vec2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
        signed: bool,
        arc_start: f32,
        arc_end: f32,
    ) -> Self {
        let (start, end, curvature) =
            crate::segment::from_center_arc(center, radius, start_angle, sweep);
        Self {
            signed,
            start,
            end,
            curvature,
            heading: 0.0,
            arc_start,
            arc_end,
        }
    }

    /// Append `(center, radius, start_angle, sweep)` as one or more MINOR sub-arcs
    /// (each `|sweep| < PI`; a full-circle pin becomes 4 quarters), advancing the
    /// cumulative arc length `acc`. The single place arcs are split for the
    /// arc-only model, so the endpoint+curvature reconstruction stays unambiguous.
    pub(crate) fn push_arc(
        out: &mut Vec<Segment>,
        center: Vec2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
        signed: bool,
        acc: &mut f32,
    ) {
        // Split at PI/2 granularity, but with a tiny epsilon so a sweep landing
        // exactly on a multiple (a quarter/half-circle corner) does not flip
        // between 1 and 2 sub-arcs on float noise - two construction paths for the
        // same geometry must yield the same segment count.
        let n = ((sweep.abs() / (std::f32::consts::PI * 0.5) - 1e-4).ceil() as u32).max(1);
        let sub = sweep / n as f32;
        let seg_len = radius * sub.abs();
        for i in 0..n {
            let a0 = start_angle + sub * i as f32;
            out.push(Segment::arc(
                center,
                radius,
                a0,
                sub,
                signed,
                *acc,
                *acc + seg_len,
            ));
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
        Segment::push_arc(
            &mut segments,
            center,
            radius,
            start_angle,
            sweep,
            false,
            &mut acc,
        );
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
                ArcPiece::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep,
                    ..
                } => {
                    Segment::push_arc(
                        &mut segments,
                        center,
                        radius,
                        start_angle,
                        sweep,
                        false,
                        &mut cum,
                    );
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

    /// Dense-polyline reference of a cubic - the arc-spline test oracle.
    ///
    /// v3 deleted the true-cubic GPU SDF, so the golden gates can no longer
    /// render an analytic cubic to compare against. Sampling the cubic into `n`
    /// exact line segments gives an INDEPENDENT faithful reference (it does not
    /// touch the biarc fitter), so a structural arc-spline error - a giant arc
    /// or full circle - still diverges from it and fails the gate.
    #[cfg(test)]
    pub(crate) fn bezier_polyline(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, n: u32) -> Self {
        let pt = |t: f32| -> Vec2 {
            let u = 1.0 - t;
            p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
        };
        let mut segments = Vec::with_capacity(n as usize);
        let mut cum = 0.0_f32;
        let mut prev = pt(0.0);
        for i in 1..=n {
            let cur = pt(i as f32 / n as f32);
            let len = prev.distance(cur);
            segments.push(Segment::line(prev, cur, false, cum, cum + len));
            cum += len;
            prev = cur;
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
        // A collinear cubic arc-splines to a single line of length ~30.
        let d = Drawable::bezier_arcs(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(20.0, 0.0),
            Vec2::new(30.0, 0.0),
            0.1,
        );
        assert_eq!(d.segment_count(), 1);
        assert!((d.total_arc_length() - 30.0).abs() < 0.1);
    }

    #[test]
    fn curved_edge_never_collapses_to_a_line() {
        // Regression for the "edge suddenly becomes a straight line on move/drag"
        // flicker. Mirror the widget (pin-side tangents + adaptive control length)
        // and sweep the target endpoint over a grid. A near-collinear S-curve (its
        // t=0.5 point on the chord) made `circle_through` return a ~1e9-radius arc
        // that the deviation gate accepted (f32 cancellation), then `from_center_arc`
        // collapsed it to a zero-length point -> the edge vanished / snapped straight.
        // No position where the true cubic is clearly curved (chord deviation > 5px)
        // may yield a single line segment.
        let dirs = [
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, -1.0),
            Vec2::new(0.0, 1.0),
        ];
        let cubic = |p0: Vec2, c0: Vec2, c1: Vec2, p3: Vec2, t: f32| {
            let u = 1.0 - t;
            p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p3 * (t * t * t)
        };
        let mut collapses = Vec::new();
        let p0 = Vec2::new(0.0, 0.0);
        for df in dirs {
            for dt in dirs {
                let mut ty = -200.0;
                while ty <= 200.0 {
                    let mut tx = -200.0;
                    while tx <= 200.0 {
                        let p3 = Vec2::new(tx, ty);
                        let l = 80.0_f32.min(p0.distance(p3) * 0.5).max(1.0);
                        let c0 = p0 + df * l;
                        let c1 = p3 + dt * l;
                        let chord = p3 - p0;
                        let cl = chord.length();
                        let mut dev = 0.0_f32;
                        for k in 1..16 {
                            let q = cubic(p0, c0, c1, p3, k as f32 / 16.0);
                            let d = if cl < 1e-6 {
                                q.distance(p0)
                            } else {
                                let n = Vec2::new(-chord.y, chord.x) / cl;
                                (q - p0).dot(n).abs()
                            };
                            dev = dev.max(d);
                        }
                        let arcs = Drawable::bezier_arcs(p0, c0, c1, p3, 0.05);
                        let is_line =
                            arcs.segments.len() == 1 && arcs.segments[0].curvature.abs() < 1e-6;
                        if dev > 5.0 && is_line {
                            collapses.push((df, dt, p3, dev));
                        }
                        tx += 10.0;
                    }
                    ty += 10.0;
                }
            }
        }
        assert!(
            collapses.is_empty(),
            "{} curved edges collapsed to a line, e.g. {:?}",
            collapses.len(),
            collapses.first(),
        );
    }

    #[test]
    fn arc_spline_field_tracks_the_cubic() {
        // The arc-spline's nearest-segment field must hug the source cubic
        // everywhere (geometry/encoding correctness), even for a SHORT tight-loop
        // edge - the config class that looked glitchy on screen. Sampling the true
        // cubic and taking the min |seg_sdf| over the spline is the on-curve
        // residual; the chain must also stay C0-contiguous with exact end pins.
        for (p0, cp0, cp1, p3) in [
            // Short tight loop (delta ~[16,6], opposing tangents).
            (
                Vec2::new(-8.0, -3.0),
                Vec2::new(0.5, -3.0),
                Vec2::new(8.0, 5.5),
                Vec2::new(8.0, 3.0),
            ),
            // Gentle S edge.
            (
                Vec2::new(-120.0, -40.0),
                Vec2::new(-40.0, -40.0),
                Vec2::new(40.0, 40.0),
                Vec2::new(120.0, 40.0),
            ),
        ] {
            let arcs = Drawable::bezier_arcs(p0, cp0, cp1, p3, 0.05);
            // Endpoints are re-derived from center+angle, so allow sub-pixel drift.
            assert!(
                arcs.segments.first().unwrap().start.distance(p0) < 1e-3,
                "first ~ p0"
            );
            assert!(
                arcs.segments.last().unwrap().end.distance(p3) < 1e-3,
                "last ~ p3"
            );
            for w in arcs.segments.windows(2) {
                assert!(w[0].end.distance(w[1].start) < 1e-3, "arc-spline has a gap");
            }
            let pt = |t: f32| {
                let u = 1.0 - t;
                p0 * (u * u * u)
                    + cp0 * (3.0 * u * u * t)
                    + cp1 * (3.0 * u * t * t)
                    + p3 * (t * t * t)
            };
            let mut worst = 0.0_f32;
            for i in 0..=200 {
                let q = pt(i as f32 / 200.0);
                let field = arcs
                    .segments
                    .iter()
                    .map(|s| {
                        crate::segment::seg_sdf(q, s.start, s.end, s.curvature, s.heading).abs()
                    })
                    .fold(f32::INFINITY, f32::min);
                worst = worst.max(field);
            }
            assert!(worst < 0.2, "arc-spline field is {worst}px off the cubic");
        }
    }

    #[test]
    fn translated_shifts_geometry_and_bounds() {
        let d = Drawable::single_line(Vec2::new(1.0, 2.0), Vec2::new(4.0, 6.0));
        let t = d.translated(10.0, -3.0);

        // Endpoints move by the offset.
        assert_eq!(t.segments[0].start, Vec2::new(11.0, -1.0));
        assert_eq!(t.segments[0].end, Vec2::new(14.0, 3.0));
        // Bounds shift with the geometry.
        assert_eq!(t.bounds(), [11.0, -1.0, 14.0, 3.0]);
        // Translation-invariant data is preserved; the original is untouched.
        assert!((t.total_arc_length() - d.total_arc_length()).abs() < 1e-6);
        assert_eq!(d.segments[0].start, Vec2::new(1.0, 2.0));
        assert_eq!(d.segments[0].end, Vec2::new(4.0, 6.0));
    }
}
