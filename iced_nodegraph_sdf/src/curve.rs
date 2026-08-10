//! Geometry construction entry point.
//!
//! `Curve` provides static methods for all geometry:
//! - Single segments: `Curve::line()`, `Curve::point()`, `Curve::arc_segment()`,
//!   `Curve::bezier()`
//! - Connected closed contours: `Curve::shape()` returns a [`ShapeBuilder`]
//! - Factory shapes: `Curve::rounded_rect_with_radii()`, `Curve::circle()`
//!
//! All angles are in **radians**. Use `std::f32::consts::{PI, FRAC_PI_2}`.
//!
//! **Heading convention**: 0 = UP, positive = clockwise.
//! - 0 = up (0, -1)
//! - PI/2 = right (1, 0)
//! - PI = down (0, 1)
//! - 3PI/2 = left (-1, 0)
//!
//! **Signed distance**: right side of segment = negative.
//! For a CW contour, interior = right side = negative.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::Vec2;

use crate::drawable::{Drawable, DrawableType, Segment};

/// Arc-spline tolerance (world units) for approximating cubic beziers as arcs in
/// the arc-only model. Fixed on purpose: screen-space error = `tol * zoom`, i.e.
/// <= 1.0 px at the widget's maximum zoom of 10 and far below that everywhere
/// else, and a fixed value keeps shape recipe hashes (and thus resident edge
/// geometry) zoom-invariant.
///
/// Halving this doubles the arc count of every curved edge, which multiplies
/// through the whole pipeline: more segments to scatter, more slots per tile,
/// and more `eval_segment` calls in the fragment's per-pixel min-reduction.
const CUBIC_ARC_TOL: f32 = 0.1;

/// Geometry construction namespace.
pub struct Curve;

impl Curve {
    /// Single line segment from `a` to `b`.
    pub fn line(a: impl Into<[f32; 2]>, b: impl Into<[f32; 2]>) -> Drawable {
        Drawable::single_line(Vec2::from(a.into()), Vec2::from(b.into()))
    }

    /// Single junction point with heading (radians). Useful for debugging.
    pub fn point(pos: impl Into<[f32; 2]>, heading: f32) -> Drawable {
        Drawable::single_point(Vec2::from(pos.into()), heading)
    }

    /// Single arc segment (center, radius, start_angle, sweep in radians).
    pub fn arc_segment(
        center: impl Into<[f32; 2]>,
        radius: f32,
        start_angle: f32,
        sweep: f32,
    ) -> Drawable {
        Drawable::single_arc(Vec2::from(center.into()), radius, start_angle, sweep)
    }

    /// Single cubic bezier segment.
    pub fn bezier(
        p0: impl Into<[f32; 2]>,
        p1: impl Into<[f32; 2]>,
        p2: impl Into<[f32; 2]>,
        p3: impl Into<[f32; 2]>,
    ) -> Drawable {
        // A4 arcs-only: fit the cubic with a biarc spline on the CPU so the
        // shader needs no cubic solver. `tol` is sub-pixel at zoom 1, keeping the
        // approximation within the AA bar; the spline preserves arc-length so
        // dash/flow parametrization matches the cubic.
        Drawable::bezier_arcs(
            Vec2::from(p0.into()),
            Vec2::from(p1.into()),
            Vec2::from(p2.into()),
            Vec2::from(p3.into()),
            CUBIC_ARC_TOL,
        )
    }

    /// Start a connected contour at `position` with `heading` (radians).
    ///
    /// Heading 0 = UP, PI/2 = RIGHT, PI = DOWN. Angles are clockwise.
    pub fn shape(position: impl Into<[f32; 2]>, heading: f32) -> ShapeBuilder {
        let pos = Vec2::from(position.into());
        ShapeBuilder {
            start: pos,
            cursor: pos,
            heading,
            segments: Vec::new(),
        }
    }

    /// Closed rounded rectangle with four independent corner radii, ordered
    /// `[top_left, top_right, bottom_right, bottom_left]`. The contour walks
    /// clockwise from just past the top-left corner, one arc per corner.
    ///
    /// Each radius is clamped to the shorter half-extent, which also bounds any
    /// two radii on one side to that side's length, so the straight runs never
    /// go negative.
    pub(crate) fn rounded_rect_with_radii(
        center: impl Into<[f32; 2]>,
        half_size: impl Into<[f32; 2]>,
        radii: [f32; 4],
    ) -> Drawable {
        let c = Vec2::from(center.into());
        let h = Vec2::from(half_size.into());
        let rmax = h.x.min(h.y);
        let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, rmax));
        let (w, hh) = (h.x * 2.0, h.y * 2.0);
        // Start after the top-left corner, heading RIGHT (PI/2)
        Curve::shape([c.x - h.x + tl, c.y - h.y], FRAC_PI_2)
            .line((w - tl - tr).max(0.0))
            .arc(tr, FRAC_PI_2)
            .line((hh - tr - br).max(0.0))
            .arc(br, FRAC_PI_2)
            .line((w - br - bl).max(0.0))
            .arc(bl, FRAC_PI_2)
            .line((hh - bl - tl).max(0.0))
            .arc(tl, FRAC_PI_2)
            .close()
    }

    /// Closed circle.
    pub fn circle(center: impl Into<[f32; 2]>, radius: f32) -> Drawable {
        let c = Vec2::from(center.into());
        // Start at top (0, -r), heading RIGHT (PI/2), sweep full circle CW
        Curve::shape([c.x, c.y - radius], FRAC_PI_2)
            .arc(radius, TAU)
            .close()
    }
}

// --- ShapeBuilder ---

/// Builder for connected closed contours.
///
/// **Heading**: 0 = UP, positive = clockwise. All angles in radians.
/// **Right side** of each segment = negative distance.
#[derive(Debug, Clone)]
pub struct ShapeBuilder {
    start: Vec2,
    cursor: Vec2,
    heading: f32, // radians: 0=UP, PI/2=RIGHT, PI=DOWN
    segments: Vec<ShapeSegment>,
}

#[derive(Debug, Clone)]
enum ShapeSegment {
    Line {
        a: Vec2,
        b: Vec2,
    },
    Arc {
        center: Vec2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
    },
}

impl ShapeBuilder {
    // --- Turtle API ---

    /// Move forward by `length` in the current heading direction.
    pub fn line(mut self, length: f32) -> Self {
        let dir = self.heading_vec();
        let end = self.cursor + dir * length;
        self.segments.push(ShapeSegment::Line {
            a: self.cursor,
            b: end,
        });
        self.cursor = end;
        self
    }

    /// Arc forward. Positive sweep = clockwise (center to the RIGHT).
    /// Single exact arc segment, no approximation.
    pub fn arc(mut self, radius: f32, sweep: f32) -> Self {
        // A non-positive radius is a sharp corner (e.g. a rounded box with a
        // zero corner radius): turn in place by `sweep` and emit no segment.
        // Emitting a zero-radius arc would trip `from_center_arc`'s positive-
        // radius invariant downstream.
        if radius <= 0.0 {
            self.heading += sweep;
            return self;
        }
        let perp = if sweep >= 0.0 {
            self.right_vec()
        } else {
            self.left_vec()
        };
        let center = self.cursor + perp * radius;
        let start_offset = self.cursor - center;
        let start_angle = start_offset.y.atan2(start_offset.x);

        self.segments.push(ShapeSegment::Arc {
            center,
            radius,
            start_angle,
            sweep,
        });

        let end_angle = start_angle + sweep;
        self.cursor = center + Vec2::new(end_angle.cos(), end_angle.sin()) * radius;
        self.heading += sweep;
        self
    }

    // --- Finalize ---

    /// Close the contour. Fillable.
    pub fn close(mut self) -> Drawable {
        let gap = self.cursor.distance(self.start);
        if gap > 1e-4 {
            self.segments.push(ShapeSegment::Line {
                a: self.cursor,
                b: self.start,
            });
        }
        self.build_drawable()
    }

    // --- Internal ---

    /// Direction vector for current heading. 0=UP=(0,-1), PI/2=RIGHT=(1,0).
    fn heading_vec(&self) -> Vec2 {
        Vec2::new(self.heading.sin(), -self.heading.cos())
    }

    /// Right perpendicular of heading (90 degrees CW).
    fn right_vec(&self) -> Vec2 {
        Vec2::new(self.heading.cos(), self.heading.sin())
    }

    /// Left perpendicular of heading (90 degrees CCW).
    fn left_vec(&self) -> Vec2 {
        Vec2::new(-self.heading.cos(), -self.heading.sin())
    }

    /// Lowers the authored segments to the one GPU primitive: lines pass
    /// through and arcs are split into minor sub-arcs. Every contour a
    /// [`ShapeBuilder`] produces is closed, so the result is always fillable.
    fn build_drawable(self) -> Drawable {
        let mut gpu_segments: Vec<Segment> = Vec::with_capacity(self.segments.len());
        let mut acc = 0.0f32;

        for seg in &self.segments {
            match seg {
                ShapeSegment::Line { a, b } => {
                    let len = a.distance(*b);
                    gpu_segments.push(Segment::line(*a, *b, true, acc, acc + len));
                    acc += len;
                }
                ShapeSegment::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep,
                } => {
                    Segment::push_arc(
                        &mut gpu_segments,
                        *center,
                        *radius,
                        *start_angle,
                        *sweep,
                        true,
                        &mut acc,
                    );
                }
            }
        }

        let total_length = acc;

        // Exact AABB from each segment's true extent (tight for arcs).
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for seg in &gpu_segments {
            seg.grow_aabb(&mut min, &mut max);
        }
        if gpu_segments.is_empty() {
            min = Vec2::ZERO;
            max = Vec2::ZERO;
        }

        Drawable {
            drawable_type: DrawableType::Shape,
            segments: gpu_segments,
            total_arc_length: total_length,
            bounds: [min.x, min.y, max.x, max.y],
            is_closed: true,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }
}

/// Compute signed area of a polygon. Negative = CW in screen Y-down.
#[cfg(test)]
fn signed_area(segments: &[ShapeSegment]) -> f32 {
    let mut area = 0.0;
    for seg in segments {
        let (ax, ay, bx, by) = match seg {
            ShapeSegment::Line { a, b } => (a.x, a.y, b.x, b.y),
            ShapeSegment::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => {
                let end_angle = start_angle + sweep;
                let a = *center + Vec2::new(start_angle.cos(), start_angle.sin()) * *radius;
                let b = *center + Vec2::new(end_angle.cos(), end_angle.sin()) * *radius;
                (a.x, a.y, b.x, b.y)
            }
        };
        area += (bx - ax) * (by + ay);
    }
    area * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// The widget clamps camera zoom to this; screen error is `tol * zoom`, so
    /// the maximum is where the arc-spline approximation is most visible.
    const MAX_WIDGET_ZOOM: f32 = 10.0;

    /// [`CUBIC_ARC_TOL`] is a QUALITY contract, not a free tuning knob: raising
    /// it cuts arc counts (and with them scatter work, slots per tile and
    /// per-pixel `eval_segment` calls) but bends every curve further from the
    /// cubic it approximates. The error is `tol * zoom`, so it is worst when
    /// zoomed all the way in - which is also where the arc-count saving is
    /// smallest, since a tightly zoomed curve spans few world units anyway.
    ///
    /// Measured: at zoom 10 against a 20x finer reference spline, tol 0.1
    /// shifts at most 137/255 on a channel, tol 0.5 flips pixels outright
    /// (255/255 - fully stroke to fully background). One pixel of screen error
    /// is the agreed ceiling.
    #[test]
    fn cubic_tolerance_stays_within_one_screen_pixel_at_max_zoom() {
        let worst_screen_error = CUBIC_ARC_TOL * MAX_WIDGET_ZOOM;
        assert!(
            worst_screen_error <= 1.0,
            "CUBIC_ARC_TOL {CUBIC_ARC_TOL} gives {worst_screen_error} px of error at zoom \
             {MAX_WIDGET_ZOOM}; the contract is <= 1.0 px. Raising it is a visible-quality \
             change, not a perf tweak - see `bezier_tessellation_matches_a_finer_reference`."
        );
    }

    fn assert_near(a: f32, b: f32, eps: f32, msg: &str) {
        assert!((a - b).abs() < eps, "{msg}: {a} != {b} (eps={eps})");
    }

    fn assert_vec_near(a: Vec2, b: Vec2, eps: f32, msg: &str) {
        assert!((a - b).length() < eps, "{msg}: {a:?} != {b:?}");
    }

    // --- Heading convention ---

    #[test]
    fn heading_0_is_up() {
        let s = Curve::shape([0.0, 0.0], 0.0).line(10.0).close();
        let seg = &s.segments[0];
        // Should go from (0,0) to (0, -10)
        assert_near(seg.end.x, 0.0, 0.01, "end x");
        assert_near(seg.end.y, -10.0, 0.01, "end y");
    }

    #[test]
    fn heading_pi2_is_right() {
        let s = Curve::shape([0.0, 0.0], FRAC_PI_2).line(10.0).close();
        let seg = &s.segments[0];
        assert_near(seg.end.x, 10.0, 0.01, "end x");
        assert_near(seg.end.y, 0.0, 0.01, "end y");
    }

    #[test]
    fn heading_pi_is_down() {
        let s = Curve::shape([0.0, 0.0], PI).line(10.0).close();
        let seg = &s.segments[0];
        assert_near(seg.end.x, 0.0, 0.01, "end x");
        assert_near(seg.end.y, 10.0, 0.01, "end y");
    }

    #[test]
    fn positive_sweep_turns_clockwise() {
        // Start UP, turn PI/2 CW -> heading RIGHT. A zero-radius arc turns in
        // place and emits nothing, so the two lines are segments 0 and 1.
        let s = Curve::shape([0.0, 0.0], 0.0)
            .line(5.0)
            .arc(0.0, FRAC_PI_2)
            .line(5.0)
            .close();
        let seg2 = &s.segments[1];
        assert_near(seg2.end.x, 5.0, 0.01, "end x");
        assert_near(seg2.end.y, -5.0, 0.01, "end y");
    }

    // --- Connectivity ---

    #[test]
    fn segments_are_connected() {
        let d = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .close();
        // Filter to the straight runs; `close` may append one more.
        let lines: Vec<_> = d
            .segments
            .iter()
            .filter(|s| s.curvature == 0.0 && s.start != s.end)
            .collect();
        for i in 0..lines.len() - 1 {
            let end = lines[i].end;
            let start = lines[i + 1].start;
            assert_vec_near(end, start, 0.01, &format!("line {i}->{}", i + 1));
        }
    }

    #[test]
    fn close_returns_to_start() {
        let d = Curve::shape([5.0, 5.0], FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .close();
        let last = d.segments.last().unwrap();
        let end = last.end;
        let start = d.segments[0].start;
        assert_vec_near(end, start, 0.1, "close returns to start");
    }

    // --- Winding ---

    #[test]
    fn cw_square_has_negative_signed_area() {
        // CW in screen Y-down: RIGHT → DOWN → LEFT → UP
        // Shoelace gives negative for CW in Y-down
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0);
        let area = signed_area(&builder.segments);
        assert!(
            area < 0.0,
            "CW square in Y-down should have negative signed area, got {area}"
        );
    }

    // --- Factory shapes ---

    #[test]
    fn rect_perimeter() {
        let d = Curve::rounded_rect_with_radii([0.0, 0.0], [50.0, 30.0], [0.0; 4]);
        assert!(d.is_closed());
        assert_near(d.total_arc_length(), 320.0, 1.0, "rect perimeter");
    }

    #[test]
    fn rect_is_cw() {
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(100.0)
            .arc(0.0, FRAC_PI_2)
            .line(60.0)
            .arc(0.0, FRAC_PI_2)
            .line(100.0)
            .arc(0.0, FRAC_PI_2)
            .line(60.0);
        assert!(
            signed_area(&builder.segments) < 0.0,
            "CW rect in Y-down should have negative area"
        );
    }

    // --- Arc ---

    #[test]
    fn arc_cw_quarter_circle() {
        // Start heading RIGHT at (0,0), CW arc PI/2 with radius 10
        // Center is to the RIGHT of heading = DOWN = (0, 10)
        // Arc goes from (0,0) CW quarter circle to (10, 10)
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2).arc(10.0, FRAC_PI_2);
        // Check cursor ended at (10, 10)
        assert_near(builder.cursor.x, 10.0, 0.5, "arc end x");
        assert_near(builder.cursor.y, 10.0, 0.5, "arc end y");
        // Check it's a single Arc segment
        assert_eq!(builder.segments.len(), 1);
        match &builder.segments[0] {
            ShapeSegment::Arc { center, radius, .. } => {
                assert_near(center.x, 0.0, 0.1, "center x");
                assert_near(center.y, 10.0, 0.1, "center y");
                assert_near(*radius, 10.0, 0.01, "radius");
            }
            _ => panic!("expected Arc segment"),
        }
    }

    // --- CPU SDF eval (mirrors shader sd_line / eval_shape) ---

    /// CPU-side sd_line matching shader: returns (unsigned_dist, v).
    /// v > 0 = right side of segment in screen Y-down = inside for CW.
    fn cpu_sd_line(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
        let ba = b - a;
        let pa = p - a;
        let len_sq = ba.dot(ba);
        let t = if len_sq > 0.0 {
            (pa.dot(ba) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let proj = a + ba * t;
        let dist = (p - proj).length();
        let n = Vec2::new(-ba.y, ba.x); // same as shader
        let v = if len_sq > 0.0 {
            pa.dot(n) / len_sq.sqrt()
        } else {
            0.0
        };
        (dist, v)
    }

    fn cpu_sd_point(p: Vec2, pos: Vec2, heading: f32) -> (f32, f32) {
        let dist = (p - pos).length();
        let right = Vec2::new(heading.cos(), heading.sin());
        let v = (p - pos).dot(right);
        (dist, v)
    }

    /// CPU-side eval_shape: find nearest segment, sign from v.
    fn cpu_eval_shape(p: Vec2, drawable: &Drawable) -> f32 {
        let mut min_dist = f32::MAX;
        let mut best_v = 0.0f32;
        for seg in &drawable.segments {
            let (dist, v) = if seg.start == seg.end {
                cpu_sd_point(p, seg.start, seg.heading)
            } else if seg.curvature == 0.0 {
                cpu_sd_line(p, seg.start, seg.end)
            } else {
                continue; // Arc not needed for basic tests
            };
            if dist < min_dist {
                min_dist = dist;
                best_v = v;
            }
        }
        if best_v > 0.0 { -min_dist } else { min_dist }
    }

    #[test]
    fn cw_square_center_is_inside() {
        // CW square: (0,0) → (10,0) → (10,10) → (0,10) → close
        let d = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .close();
        let center = Vec2::new(5.0, 5.0);
        let dist = cpu_eval_shape(center, &d);
        assert!(
            dist < 0.0,
            "center of CW square should be negative (inside), got {dist}"
        );
    }

    #[test]
    fn cw_square_outside_is_positive() {
        let d = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .arc(0.0, FRAC_PI_2)
            .line(10.0)
            .close();
        let outside = Vec2::new(-5.0, 5.0);
        let dist = cpu_eval_shape(outside, &d);
        assert!(
            dist > 0.0,
            "point outside CW square should be positive, got {dist}"
        );
    }

    #[test]
    fn single_line_right_side_is_negative() {
        // Line going RIGHT: (0,0) → (10,0)
        // Right side in screen Y-down = below = positive Y
        let d = Curve::line([0.0, 0.0], [10.0, 0.0]);
        let seg = &d.segments[0];
        let a = seg.start;
        let b = seg.end;

        let below = Vec2::new(5.0, 5.0); // right side
        let (_, v) = cpu_sd_line(below, a, b);
        assert!(
            v > 0.0,
            "point below rightward line should have v > 0 (right side), got {v}"
        );

        let above = Vec2::new(5.0, -5.0); // left side
        let (_, v) = cpu_sd_line(above, a, b);
        assert!(
            v < 0.0,
            "point above rightward line should have v < 0 (left side), got {v}"
        );
    }

    #[test]
    fn rect_factory_center_is_inside() {
        let d = Curve::rounded_rect_with_radii([0.0, 0.0], [50.0, 30.0], [0.0; 4]);
        let dist = cpu_eval_shape(Vec2::new(0.0, 0.0), &d);
        assert!(
            dist < 0.0,
            "center of rect should be inside (negative), got {dist}"
        );
    }

    #[test]
    fn rect_factory_outside_is_positive() {
        let d = Curve::rounded_rect_with_radii([0.0, 0.0], [50.0, 30.0], [0.0; 4]);
        let dist = cpu_eval_shape(Vec2::new(100.0, 0.0), &d);
        assert!(
            dist > 0.0,
            "far point should be outside (positive), got {dist}"
        );
    }

    // --- Single segments ---

    #[test]
    fn test_single_line() {
        let d = Curve::line([0.0, 0.0], [10.0, 0.0]);
        assert_eq!(d.segment_count(), 1);
        assert_near(d.total_arc_length(), 10.0, 0.001, "line length");
    }

    #[test]
    fn test_single_bezier() {
        let d = Curve::bezier([0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]);
        assert_eq!(d.segment_count(), 1);
        assert_near(d.total_arc_length(), 30.0, 0.5, "bezier length");
    }

    #[test]
    fn test_bounds() {
        let d = Curve::line([-5.0, -3.0], [10.0, 7.0]);
        let b = d.bounds();
        assert_eq!(b[0], -5.0);
        assert_eq!(b[1], -3.0);
        assert_eq!(b[2], 10.0);
        assert_eq!(b[3], 7.0);
    }
}
