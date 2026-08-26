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

use crate::biarc::{ArcPiece, cubic_to_arcs};
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

/// Shortest segment worth emitting, in local units.
///
/// A shorter one carries no direction. The GPU primitive stores a segment as
/// its `start`/`end` pair plus a curvature, and reads `heading` only for a
/// deliberate junction point (`Segment::point`, whose caller supplies the
/// interior bisector), so a span the endpoints cannot resolve is encoded as a
/// junction facing heading zero and paints a spur off the contour.
///
/// Every producer answers to it: the two straight steps
/// ([`ShapeBuilder::line`], [`ShapeBuilder::line_to`]) against their length,
/// the two arcs ([`ShapeBuilder::arc`], [`ShapeBuilder::arc_to`]) against the
/// arc length they would lay down, the arc-spline pieces of a cubic against
/// their own, and [`ShapeBuilder::close`] against the gap left between the
/// cursor and the contour start.
///
/// Skipping such a segment changes no geometry: the neighbours already meet at
/// the point it would have occupied, and it contributes nothing to the
/// cumulative arc length a dash or flow pattern phases over.
const MIN_SEGMENT: f32 = 1e-4;

/// Whether an arc of `radius` sweeping `sweep` radians is too small to emit.
///
/// A non-positive radius is a sharp corner (a rounded box with a zero corner
/// radius, or a cable wrapping an anchor whose orbit offset is zero) and has no
/// arc at all; `from_center_arc` asserts a positive radius, so encoding one
/// would trip that invariant. Above zero the arc still collapses once its
/// length falls under [`MIN_SEGMENT`]: its endpoints land within the tolerance
/// the encoding resolves a point at, leaving curvature and heading both zero.
fn arc_below_floor(radius: f32, sweep: f32) -> bool {
    radius <= 0.0 || radius * sweep.abs() < MIN_SEGMENT
}

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
    /// Cubic bezier, arc-splined on the CPU when the contour is built (there
    /// is no GPU bezier: the one GPU primitive is the circular arc).
    CubicBezier {
        p0: Vec2,
        p1: Vec2,
        p2: Vec2,
        p3: Vec2,
    },
}

impl ShapeBuilder {
    // --- Turtle API ---

    /// Move forward by `length` in the current heading direction.
    ///
    /// A length below [`MIN_SEGMENT`] moves nothing and emits no segment, the
    /// straight-line twin of [`arc`](Self::arc) turning in place on a
    /// non-positive radius. A rounded rectangle whose corner radius fills its
    /// half-extent asks for exactly this on all four sides.
    pub fn line(mut self, length: f32) -> Self {
        if length.abs() < MIN_SEGMENT {
            return self;
        }
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
    ///
    /// An arc under the [`MIN_SEGMENT`] floor turns the heading by `sweep` in
    /// place and emits nothing, leaving the cursor where it is.
    pub fn arc(mut self, radius: f32, sweep: f32) -> Self {
        if arc_below_floor(radius, sweep) {
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

    // --- Absolute API ---
    //
    // The turtle above walks by length and sweep; these three place segments at
    // explicit coordinates instead, for a path whose points are already known
    // (`Shape::path`). Both share one cursor and heading, so the two styles can
    // be mixed and the running arc length stays continuous either way.

    /// Straight segment from the cursor to `end`, which becomes the new cursor.
    ///
    /// A step shorter than [`MIN_SEGMENT`] leaves the cursor and the heading
    /// where they are and emits nothing, so a path whose consecutive points
    /// coincide is the path without them.
    pub fn line_to(mut self, end: impl Into<[f32; 2]>) -> Self {
        let end = Vec2::from(end.into());
        let dir = end - self.cursor;
        if dir.length() < MIN_SEGMENT {
            return self;
        }
        self.heading = heading_from_dir(dir);
        self.segments.push(ShapeSegment::Line {
            a: self.cursor,
            b: end,
        });
        self.cursor = end;
        self
    }

    /// Arc around `center` with explicit `radius` and `sweep` radians. The
    /// start angle comes from the running cursor, so a caller never restates
    /// where the arc begins.
    ///
    /// An arc under the [`MIN_SEGMENT`] floor turns the heading by `sweep` in
    /// place and emits nothing, leaving the cursor where it is - the same step
    /// [`arc`](Self::arc) takes, so a cable wrap the routing shrinks to nothing
    /// drops out of the path instead of encoding a directionless point.
    pub fn arc_to(mut self, center: impl Into<[f32; 2]>, radius: f32, sweep: f32) -> Self {
        if arc_below_floor(radius, sweep) {
            self.heading += sweep;
            return self;
        }
        let center = Vec2::from(center.into());
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

    /// Cubic bezier from the cursor to `end` via the two control points.
    pub fn bezier_to(
        mut self,
        cp1: impl Into<[f32; 2]>,
        cp2: impl Into<[f32; 2]>,
        end: impl Into<[f32; 2]>,
    ) -> Self {
        let cp1 = Vec2::from(cp1.into());
        let cp2 = Vec2::from(cp2.into());
        let end = Vec2::from(end.into());
        self.segments.push(ShapeSegment::CubicBezier {
            p0: self.cursor,
            p1: cp1,
            p2: cp2,
            p3: end,
        });
        let tangent = end - cp2;
        if tangent.length_squared() > 1e-10 {
            self.heading = heading_from_dir(tangent);
        }
        self.cursor = end;
        self
    }

    // --- Finalize ---

    /// Close the contour. Fillable.
    ///
    /// A gap under the [`MIN_SEGMENT`] floor needs no closing segment: the
    /// contour already ends where it began, as far as the encoding can tell.
    pub fn close(mut self) -> Drawable {
        let gap = self.cursor.distance(self.start);
        if gap >= MIN_SEGMENT {
            self.segments.push(ShapeSegment::Line {
                a: self.cursor,
                b: self.start,
            });
        }
        self.build_drawable(true)
    }

    /// End the contour open: a stroke, with no closing segment back to the
    /// start and no interior for a fill to claim.
    pub fn end(self) -> Drawable {
        self.build_drawable(false)
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
    /// through, arcs are split into minor sub-arcs, and cubics are arc-splined
    /// on the CPU. `closed` marks the contour fillable; an open one is a stroke
    /// with no interior.
    fn build_drawable(self, closed: bool) -> Drawable {
        let mut gpu_segments: Vec<Segment> = Vec::with_capacity(self.segments.len());
        let mut acc = 0.0f32;

        for seg in &self.segments {
            match seg {
                ShapeSegment::Line { a, b } => {
                    let len = a.distance(*b);
                    gpu_segments.push(Segment::line(*a, *b, closed, acc, acc + len));
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
                        closed,
                        &mut acc,
                    );
                }
                ShapeSegment::CubicBezier { p0, p1, p2, p3 } => {
                    for piece in cubic_to_arcs(*p0, *p1, *p2, *p3, CUBIC_ARC_TOL) {
                        // A piece under the floor is the same directionless
                        // span a straight step or an arc is refused for. A
                        // cubic within tolerance of a chord of no length fits
                        // as exactly one such piece.
                        if piece.length() < MIN_SEGMENT {
                            continue;
                        }
                        match piece {
                            ArcPiece::Line { start, end, length } => {
                                gpu_segments.push(Segment::line(
                                    start,
                                    end,
                                    closed,
                                    acc,
                                    acc + length,
                                ));
                                acc += length;
                            }
                            ArcPiece::Arc {
                                center,
                                radius,
                                start_angle,
                                sweep,
                                ..
                            } => {
                                Segment::push_arc(
                                    &mut gpu_segments,
                                    center,
                                    radius,
                                    start_angle,
                                    sweep,
                                    closed,
                                    &mut acc,
                                );
                            }
                        }
                    }
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
            is_closed: closed,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }
}

/// Heading for a direction vector, in the builder's convention (0 = UP,
/// positive = clockwise). Inverse of `heading_vec`.
fn heading_from_dir(dir: Vec2) -> f32 {
    dir.x.atan2(-dir.y)
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
            // The chord is the polygon edge a bezier contributes; the bulge
            // either side of it cancels for a winding-direction check.
            ShapeSegment::CubicBezier { p0, p3, .. } => (p0.x, p0.y, p3.x, p3.y),
        };
        area += (bx - ax) * (by + ay);
    }
    area * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::POINT_EPS;
    use crate::shape::{PathSeg, Shape};
    use std::f32::consts::PI;

    /// A rounded rectangle whose corner radius fills its half-extent is a
    /// circle, and its four sides have nothing left to draw.
    ///
    /// A zero-length side must not reach the GPU: a straight segment there
    /// carries no direction, and the primitive reads `heading` for a
    /// zero-length span, so one emitted anyway paints a spur pointing at the
    /// builder's zero heading rather than nothing at all.
    #[test]
    fn a_fully_rounded_box_emits_no_zero_length_sides() {
        let r = 3.0;
        let d = Curve::rounded_rect_with_radii([0.0, 0.0], [r, r], [r; 4]);
        for seg in &d.segments {
            assert!(
                seg.start.distance(seg.end) > 0.0,
                "a zero-length segment reached the GPU: {seg:?}"
            );
            // Four quarter turns and nothing else, so what is left describes
            // the circle the radii asked for.
            for p in [seg.start, seg.end] {
                assert!(
                    (p.length() - r).abs() < 1e-3,
                    "{p:?} is not on the circle of radius {r}"
                );
            }
        }
    }

    /// The guard is a floor on segment length, not a special case for rounded
    /// boxes: a path that revisits its own cursor skips the step instead of
    /// emitting a directionless one.
    #[test]
    fn a_repeated_path_point_emits_no_segment() {
        let plain = Curve::shape([0.0, 0.0], 0.0)
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .end();
        let doubled = Curve::shape([0.0, 0.0], 0.0)
            .line_to([10.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .end();
        assert_eq!(plain.segments.len(), doubled.segments.len());
        assert!(
            (plain.total_arc_length - doubled.total_arc_length).abs() < 1e-4,
            "a skipped step must not change the length a dash phases over"
        );
    }

    /// No segment reaching the GPU may be too short to carry a direction: the
    /// primitive reads `heading` for a zero-length span and the builder has no
    /// interior bisector to put there, so one emitted anyway paints a spur off
    /// the contour at heading zero.
    fn assert_no_directionless_segment(d: &Drawable) {
        for seg in &d.segments {
            assert!(
                seg.start.distance(seg.end) > POINT_EPS,
                "a directionless segment reached the GPU: {seg:?}"
            );
        }
    }

    /// `AnchorStyle::orbit_radius` is `orbit_offset + k * orbit_spacing` and
    /// nothing floors `orbit_offset`, so a host style can hand a cable wrap a
    /// zero radius. `Orbit::attachment` then returns the centre for both
    /// tangent points and the arc has no geometry at all, so the coordinate API
    /// must turn in place on it exactly as the turtle API does rather than
    /// encode a circle of radius zero.
    #[test]
    fn a_zero_radius_path_arc_turns_in_place() {
        let d = Shape::path(
            [0.0, 0.0],
            [
                PathSeg::Line { to: [10.0, 0.0] },
                PathSeg::Arc {
                    center: [10.0, 0.0],
                    radius: 0.0,
                    sweep: FRAC_PI_2,
                },
                PathSeg::Line { to: [10.0, 10.0] },
            ],
        )
        .evaluate();
        assert_no_directionless_segment(&d);
        assert_eq!(
            d.segments.len(),
            2,
            "the two straight runs, and nothing for the radius-free arc"
        );
    }

    /// The floor is on the arc a wrap lays down, not only on its radius:
    /// `edge_path::build` selects the smaller of the two hands' sweeps, so a
    /// wrap that barely turns arrives with a full radius and a product below
    /// what the endpoint + curvature encoding can resolve. Both bands are
    /// covered: below `POINT_EPS` the endpoints collapse outright, and between
    /// there and [`MIN_SEGMENT`] the arc is shorter than any straight step the
    /// builder would keep.
    #[test]
    fn a_path_arc_shorter_than_the_floor_emits_nothing() {
        let radius = 20.0;
        for arc_len in [POINT_EPS * 0.5, MIN_SEGMENT * 0.5] {
            let d = Shape::path(
                [0.0, 0.0],
                [
                    PathSeg::Line { to: [10.0, 0.0] },
                    PathSeg::Arc {
                        center: [10.0, 20.0],
                        radius,
                        sweep: arc_len / radius,
                    },
                ],
            )
            .evaluate();
            assert_no_directionless_segment(&d);
            assert_eq!(
                d.segments.len(),
                1,
                "an arc {arc_len} long is under the floor and is not a segment"
            );
        }
    }

    /// Whether `close` still owes the contour a closing segment is the same
    /// question `line_to` answers about a step, against the same threshold. So
    /// walking back to the start explicitly and letting `close` do it produce
    /// the same contour at every gap, and the threshold is where it bites.
    #[test]
    fn close_and_line_to_share_one_threshold() {
        for gap in [MIN_SEGMENT * 0.5, MIN_SEGMENT, MIN_SEGMENT * 2.0] {
            let walk = || {
                Curve::shape([0.0, 0.0], 0.0)
                    .line_to([10.0, 0.0])
                    .line_to([10.0, 10.0])
                    .line_to([gap, 0.0])
            };
            let implicit = walk().close();
            let explicit = walk().line_to([0.0, 0.0]).close();
            assert_eq!(
                implicit.segments.len(),
                explicit.segments.len(),
                "gap {gap}: `close` and `line_to` disagree on whether it is a segment"
            );
            let expected = if gap < MIN_SEGMENT { 3 } else { 4 };
            assert_eq!(
                implicit.segments.len(),
                expected,
                "gap {gap} against a floor of {MIN_SEGMENT}"
            );
        }
    }

    /// The arc-spline lowering answers to the floor too. A cubic whose control
    /// points all coincide is within tolerance of its own chord, so it fits as
    /// one `ArcPiece::Line` of zero length - the spur again, by a different
    /// route into `Segment::line`.
    #[test]
    fn a_degenerate_cubic_emits_no_zero_length_line() {
        let d = Curve::shape([0.0, 0.0], 0.0)
            .line_to([10.0, 0.0])
            .bezier_to([10.0, 0.0], [10.0, 0.0], [10.0, 0.0])
            .end();
        assert_no_directionless_segment(&d);
        assert_eq!(
            d.segments.len(),
            1,
            "only the straight run before the cubic"
        );
    }

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
        assert_near(d.total_arc_length, 320.0, 1.0, "rect perimeter");
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
        assert_near(d.total_arc_length, 10.0, 0.001, "line length");
    }

    #[test]
    fn test_single_bezier() {
        let d = Curve::bezier([0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]);
        assert_eq!(d.segment_count(), 1);
        assert_near(d.total_arc_length, 30.0, 0.5, "bezier length");
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
