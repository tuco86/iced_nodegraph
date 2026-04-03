//! Geometry construction entry point.
//!
//! `Curve` provides static methods for all geometry:
//! - Single segments: `Curve::line()`, `Curve::bezier()`
//! - Connected contours: `Curve::shape()` returns a [`ShapeBuilder`]
//! - Factory shapes: `Curve::rect()`, `Curve::rounded_rect()`, `Curve::circle()`
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

use crate::drawable::{Drawable, DrawableType, Segment, SegmentType, bezier_arc_length};

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
        center: impl Into<[f32; 2]>, radius: f32, start_angle: f32, sweep: f32,
    ) -> Drawable {
        Drawable::single_arc(Vec2::from(center.into()), radius, start_angle, sweep)
    }

    /// Single cubic bezier segment.
    pub fn bezier(
        p0: impl Into<[f32; 2]>, p1: impl Into<[f32; 2]>,
        p2: impl Into<[f32; 2]>, p3: impl Into<[f32; 2]>,
    ) -> Drawable {
        Drawable::single_bezier(
            Vec2::from(p0.into()), Vec2::from(p1.into()),
            Vec2::from(p2.into()), Vec2::from(p3.into()),
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

    /// Closed rectangle. Starts heading RIGHT, builds CW.
    pub fn rect(center: impl Into<[f32; 2]>, half_size: impl Into<[f32; 2]>) -> Drawable {
        let c = Vec2::from(center.into());
        let h = Vec2::from(half_size.into());
        // Start top-left, heading RIGHT (PI/2)
        Curve::shape([c.x - h.x, c.y - h.y], FRAC_PI_2)
            .line(h.x * 2.0).angle(FRAC_PI_2) // top edge, turn down
            .line(h.y * 2.0).angle(FRAC_PI_2) // right edge, turn left
            .line(h.x * 2.0).angle(FRAC_PI_2) // bottom edge, turn up
            .line(h.y * 2.0)                   // left edge
            .close()
    }

    /// Closed rounded rectangle.
    pub fn rounded_rect(
        center: impl Into<[f32; 2]>, half_size: impl Into<[f32; 2]>, radius: f32,
    ) -> Drawable {
        let c = Vec2::from(center.into());
        let h = Vec2::from(half_size.into());
        let r = radius.min(h.x).min(h.y);
        let w = h.x * 2.0 - r * 2.0;
        let hh = h.y * 2.0 - r * 2.0;
        // Start after top-left corner, heading RIGHT (PI/2)
        Curve::shape([c.x - h.x + r, c.y - h.y], FRAC_PI_2)
            .line(w).arc(r, FRAC_PI_2)
            .line(hh).arc(r, FRAC_PI_2)
            .line(w).arc(r, FRAC_PI_2)
            .line(hh).arc(r, FRAC_PI_2)
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

/// Builder for connected contours (open or closed).
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
    Line { a: Vec2, b: Vec2 },
    Arc { center: Vec2, radius: f32, start_angle: f32, sweep: f32 },
    CubicBezier { p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2 },
    /// Junction point at a corner. Heading = bisector of adjacent segments.
    Point { pos: Vec2, heading: f32 },
}

impl ShapeBuilder {
    // --- Turtle API ---

    /// Move forward by `length` in the current heading direction.
    pub fn line(mut self, length: f32) -> Self {
        let dir = self.heading_vec();
        let end = self.cursor + dir * length;
        self.segments.push(ShapeSegment::Line { a: self.cursor, b: end });
        self.cursor = end;
        self
    }

    /// Turn heading by `radians`. Positive = clockwise.
    /// Emits a junction point at the current position with bisector heading.
    pub fn angle(mut self, radians: f32) -> Self {
        let heading_before = self.heading;
        self.heading += radians;
        // Junction point with bisector heading between incoming and outgoing
        let bisector = (heading_before + self.heading) * 0.5;
        self.segments.push(ShapeSegment::Point {
            pos: self.cursor,
            heading: bisector,
        });
        self
    }

    /// Arc forward. Positive sweep = clockwise (center to the RIGHT).
    /// Single exact arc segment, no approximation.
    pub fn arc(mut self, radius: f32, sweep: f32) -> Self {
        let perp = if sweep >= 0.0 {
            self.right_vec()
        } else {
            self.left_vec()
        };
        let center = self.cursor + perp * radius;
        let start_offset = self.cursor - center;
        let start_angle = start_offset.y.atan2(start_offset.x);

        self.segments.push(ShapeSegment::Arc {
            center, radius, start_angle, sweep,
        });

        let end_angle = start_angle + sweep;
        self.cursor = center + Vec2::new(end_angle.cos(), end_angle.sin()) * radius;
        self.heading += sweep;
        self
    }

    // --- Coordinate API ---

    /// Line to an absolute point.
    pub fn line_to(mut self, end: impl Into<[f32; 2]>) -> Self {
        let end = Vec2::from(end.into());
        let dir = end - self.cursor;
        if dir.length_squared() > 1e-10 {
            self.heading = heading_from_dir(dir);
        }
        self.segments.push(ShapeSegment::Line { a: self.cursor, b: end });
        self.cursor = end;
        self
    }

    /// Arc around `center` with explicit `radius` and `sweep` radians.
    pub fn arc_to(
        mut self, center: impl Into<[f32; 2]>, radius: f32, sweep: f32,
    ) -> Self {
        let center = Vec2::from(center.into());
        let start_offset = self.cursor - center;
        let start_angle = start_offset.y.atan2(start_offset.x);

        self.segments.push(ShapeSegment::Arc {
            center, radius, start_angle, sweep,
        });

        let end_angle = start_angle + sweep;
        self.cursor = center + Vec2::new(end_angle.cos(), end_angle.sin()) * radius;
        self.heading += sweep;
        self
    }

    /// Cubic bezier to endpoint with control points.
    pub fn bezier_to(
        mut self,
        cp1: impl Into<[f32; 2]>, cp2: impl Into<[f32; 2]>, end: impl Into<[f32; 2]>,
    ) -> Self {
        let cp1 = Vec2::from(cp1.into());
        let cp2 = Vec2::from(cp2.into());
        let end = Vec2::from(end.into());
        self.segments.push(ShapeSegment::CubicBezier {
            p0: self.cursor, p1: cp1, p2: cp2, p3: end,
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
    pub fn close(mut self) -> Drawable {
        let gap = self.cursor.distance(self.start);
        if gap > 1e-4 {
            self.segments.push(ShapeSegment::Line { a: self.cursor, b: self.start });
        }
        self.build_drawable(true)
    }

    /// End the contour (open, stroke only).
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

    fn build_drawable(self, closed: bool) -> Drawable {
        let mut gpu_segments = Vec::with_capacity(self.segments.len());
        let mut cumulative_length = 0.0f32;
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let mut lengths = Vec::with_capacity(self.segments.len());
        for seg in &self.segments {
            lengths.push(match seg {
                ShapeSegment::Line { a, b } => a.distance(*b),
                ShapeSegment::Arc { radius, sweep, .. } => sweep.abs() * radius,
                ShapeSegment::CubicBezier { p0, p1, p2, p3 } => {
                    bezier_arc_length(*p0, *p1, *p2, *p3)
                }
                ShapeSegment::Point { .. } => 0.0,
            });
        }
        let total_length: f32 = lengths.iter().sum();

        for (seg, &len) in self.segments.iter().zip(&lengths) {
            let arc_start = cumulative_length;
            cumulative_length += len;
            let arc_end = cumulative_length;

            match seg {
                ShapeSegment::Line { a, b } => {
                    min_x = min_x.min(a.x).min(b.x);
                    min_y = min_y.min(a.y).min(b.y);
                    max_x = max_x.max(a.x).max(b.x);
                    max_y = max_y.max(a.y).max(b.y);
                    gpu_segments.push(Segment {
                        segment_type: SegmentType::Line,
                        signed: closed,
                        geom0: [a.x, a.y, b.x, b.y],
                        geom1: [0.0; 4],
                        arc_start, arc_end,
                    });
                }
                ShapeSegment::Arc { center, radius, start_angle, sweep } => {
                    // Conservative AABB: center ± radius covers all possible arc points
                    min_x = min_x.min(center.x - radius);
                    min_y = min_y.min(center.y - radius);
                    max_x = max_x.max(center.x + radius);
                    max_y = max_y.max(center.y + radius);
                    gpu_segments.push(Segment {
                        segment_type: SegmentType::Arc,
                        signed: closed,
                        geom0: [center.x, center.y, *radius, *start_angle],
                        geom1: [*sweep, 0.0, 0.0, 0.0],
                        arc_start, arc_end,
                    });
                }
                ShapeSegment::CubicBezier { p0, p1, p2, p3 } => {
                    for p in [p0, p1, p2, p3] {
                        min_x = min_x.min(p.x);
                        min_y = min_y.min(p.y);
                        max_x = max_x.max(p.x);
                        max_y = max_y.max(p.y);
                    }
                    gpu_segments.push(Segment {
                        segment_type: SegmentType::CubicBezier,
                        signed: closed,
                        geom0: [p0.x, p0.y, p1.x, p1.y],
                        geom1: [p2.x, p2.y, p3.x, p3.y],
                        arc_start, arc_end,
                    });
                }
                ShapeSegment::Point { pos, heading } => {
                    min_x = min_x.min(pos.x);
                    min_y = min_y.min(pos.y);
                    max_x = max_x.max(pos.x);
                    max_y = max_y.max(pos.y);
                    gpu_segments.push(Segment {
                        segment_type: SegmentType::Point,
                        signed: closed,
                        geom0: [pos.x, pos.y, *heading, 0.0],
                        geom1: [0.0; 4],
                        arc_start, arc_end: arc_start, // zero length
                    });
                }
            }
        }

        Drawable {
            drawable_type: if closed { DrawableType::Shape } else { DrawableType::CurveSegment },
            segments: gpu_segments,
            total_arc_length: total_length,
            bounds: [min_x, min_y, max_x, max_y],
            is_closed: closed,
            tiling_type: None,
            tiling_params: [0.0; 4],
        }
    }
}

/// Compute heading from a direction vector.
/// heading 0 = UP = (0, -1), PI/2 = RIGHT = (1, 0).
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
            ShapeSegment::Arc { center, radius, start_angle, sweep } => {
                let end_angle = start_angle + sweep;
                let a = *center + Vec2::new(start_angle.cos(), start_angle.sin()) * *radius;
                let b = *center + Vec2::new(end_angle.cos(), end_angle.sin()) * *radius;
                (a.x, a.y, b.x, b.y)
            }
            ShapeSegment::CubicBezier { p0, p3, .. } => (p0.x, p0.y, p3.x, p3.y),
            ShapeSegment::Point { .. } => continue, // zero-length, no area contribution
        };
        area += (bx - ax) * (by + ay);
    }
    area * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn assert_near(a: f32, b: f32, eps: f32, msg: &str) {
        assert!((a - b).abs() < eps, "{msg}: {a} != {b} (eps={eps})");
    }

    fn assert_vec_near(a: Vec2, b: Vec2, eps: f32, msg: &str) {
        assert!((a - b).length() < eps, "{msg}: {a:?} != {b:?}");
    }

    // --- Heading convention ---

    #[test]
    fn heading_0_is_up() {
        let s = Curve::shape([0.0, 0.0], 0.0).line(10.0).end();
        let seg = &s.segments[0];
        // Should go from (0,0) to (0, -10)
        assert_near(seg.geom0[2], 0.0, 0.01, "end x");
        assert_near(seg.geom0[3], -10.0, 0.01, "end y");
    }

    #[test]
    fn heading_pi2_is_right() {
        let s = Curve::shape([0.0, 0.0], FRAC_PI_2).line(10.0).end();
        let seg = &s.segments[0];
        assert_near(seg.geom0[2], 10.0, 0.01, "end x");
        assert_near(seg.geom0[3], 0.0, 0.01, "end y");
    }

    #[test]
    fn heading_pi_is_down() {
        let s = Curve::shape([0.0, 0.0], PI).line(10.0).end();
        let seg = &s.segments[0];
        assert_near(seg.geom0[2], 0.0, 0.01, "end x");
        assert_near(seg.geom0[3], 10.0, 0.01, "end y");
    }

    #[test]
    fn angle_positive_is_cw() {
        // Start UP, turn PI/2 CW → heading RIGHT
        // Segments: Line, Point (junction), Line
        let s = Curve::shape([0.0, 0.0], 0.0)
            .line(5.0).angle(FRAC_PI_2).line(5.0).end();
        let seg2 = &s.segments[2]; // second line (index 2, after junction point)
        assert_near(seg2.geom0[2], 5.0, 0.01, "end x");
        assert_near(seg2.geom0[3], -5.0, 0.01, "end y");
    }

    // --- Connectivity ---

    #[test]
    fn segments_are_connected() {
        let d = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0)
            .close();
        // Filter to only Line segments (Points are zero-length junctions)
        let lines: Vec<_> = d.segments.iter()
            .filter(|s| s.segment_type == SegmentType::Line)
            .collect();
        for i in 0..lines.len() - 1 {
            let end = Vec2::new(lines[i].geom0[2], lines[i].geom0[3]);
            let start = Vec2::new(lines[i + 1].geom0[0], lines[i + 1].geom0[1]);
            assert_vec_near(end, start, 0.01, &format!("line {i}->{}", i + 1));
        }
    }

    #[test]
    fn close_returns_to_start() {
        let d = Curve::shape([5.0, 5.0], FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0)
            .close();
        let last = d.segments.last().unwrap();
        let end = Vec2::new(last.geom0[2], last.geom0[3]);
        let start = Vec2::new(d.segments[0].geom0[0], d.segments[0].geom0[1]);
        assert_vec_near(end, start, 0.1, "close returns to start");
    }

    // --- Winding ---

    #[test]
    fn cw_square_has_negative_signed_area() {
        // CW in screen Y-down: RIGHT → DOWN → LEFT → UP
        // Shoelace gives negative for CW in Y-down
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0);
        let area = signed_area(&builder.segments);
        assert!(area < 0.0, "CW square in Y-down should have negative signed area, got {area}");
    }

    // --- Factory shapes ---

    #[test]
    fn rect_perimeter() {
        let d = Curve::rect([0.0, 0.0], [50.0, 30.0]);
        assert!(d.is_closed());
        assert_near(d.total_arc_length(), 320.0, 1.0, "rect perimeter");
    }

    #[test]
    fn rect_is_cw() {
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(100.0).angle(FRAC_PI_2)
            .line(60.0).angle(FRAC_PI_2)
            .line(100.0).angle(FRAC_PI_2)
            .line(60.0);
        assert!(signed_area(&builder.segments) < 0.0, "CW rect in Y-down should have negative area");
    }

    // --- Arc ---

    #[test]
    fn arc_cw_quarter_circle() {
        // Start heading RIGHT at (0,0), CW arc PI/2 with radius 10
        // Center is to the RIGHT of heading = DOWN = (0, 10)
        // Arc goes from (0,0) CW quarter circle to (10, 10)
        let builder = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .arc(10.0, FRAC_PI_2);
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
        let t = if len_sq > 0.0 { (pa.dot(ba) / len_sq).clamp(0.0, 1.0) } else { 0.0 };
        let proj = a + ba * t;
        let dist = (p - proj).length();
        let n = Vec2::new(-ba.y, ba.x); // same as shader
        let v = if len_sq > 0.0 { pa.dot(n) / len_sq.sqrt() } else { 0.0 };
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
            let (dist, v) = match seg.segment_type {
                SegmentType::Line => {
                    let a = Vec2::new(seg.geom0[0], seg.geom0[1]);
                    let b = Vec2::new(seg.geom0[2], seg.geom0[3]);
                    cpu_sd_line(p, a, b)
                }
                SegmentType::Point => {
                    let pos = Vec2::new(seg.geom0[0], seg.geom0[1]);
                    cpu_sd_point(p, pos, seg.geom0[2])
                }
                _ => continue, // Arc/Bezier not needed for basic tests
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
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0)
            .close();
        let center = Vec2::new(5.0, 5.0);
        let dist = cpu_eval_shape(center, &d);
        assert!(dist < 0.0, "center of CW square should be negative (inside), got {dist}");
    }

    #[test]
    fn cw_square_outside_is_positive() {
        let d = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0).angle(FRAC_PI_2)
            .line(10.0)
            .close();
        let outside = Vec2::new(-5.0, 5.0);
        let dist = cpu_eval_shape(outside, &d);
        assert!(dist > 0.0, "point outside CW square should be positive, got {dist}");
    }

    #[test]
    fn single_line_right_side_is_negative() {
        // Line going RIGHT: (0,0) → (10,0)
        // Right side in screen Y-down = below = positive Y
        let d = Curve::line([0.0, 0.0], [10.0, 0.0]);
        let seg = &d.segments[0];
        let a = Vec2::new(seg.geom0[0], seg.geom0[1]);
        let b = Vec2::new(seg.geom0[2], seg.geom0[3]);

        let below = Vec2::new(5.0, 5.0); // right side
        let (_, v) = cpu_sd_line(below, a, b);
        assert!(v > 0.0, "point below rightward line should have v > 0 (right side), got {v}");

        let above = Vec2::new(5.0, -5.0); // left side
        let (_, v) = cpu_sd_line(above, a, b);
        assert!(v < 0.0, "point above rightward line should have v < 0 (left side), got {v}");
    }

    #[test]
    fn rect_factory_center_is_inside() {
        let d = Curve::rect([0.0, 0.0], [50.0, 30.0]);
        let dist = cpu_eval_shape(Vec2::new(0.0, 0.0), &d);
        assert!(dist < 0.0, "center of rect should be inside (negative), got {dist}");
    }

    #[test]
    fn rect_factory_outside_is_positive() {
        let d = Curve::rect([0.0, 0.0], [50.0, 30.0]);
        let dist = cpu_eval_shape(Vec2::new(100.0, 0.0), &d);
        assert!(dist > 0.0, "far point should be outside (positive), got {dist}");
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
