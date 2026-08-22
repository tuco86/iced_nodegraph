//! The single source of cable geometry (`plan/routing-pins.md`).
//!
//! A cable is a chain of hops: a node pin, the anchor orbits it wraps, and the
//! node pin at the far end. It is emitted as one [`PathSeg`] chain for
//! [`iced_nodegraph_sdf::Shape::path`], which folds every segment through one
//! contour builder - so a dash or flow pattern phases once over the whole
//! cable instead of restarting at each waypoint.
//!
//! An orbit is met TANGENTIALLY. From an external point there are two tangents
//! to a circle, and choosing one chooses which way the cable wraps ([`Hand`]);
//! the wrap itself is an exact circular arc. Straight in, round the corner,
//! straight out - taut, not approximated, and the legs meet the arc without a
//! kink because their control points lie along the wrap direction.
//!
//! The pin-to-pin case degrades to exactly the single tangent-bezier leg the
//! widget has always drawn: same control-point formula, same
//! [`adaptive_bezier_length`].

use super::{adaptive_bezier_length, pin_side_direction};
use crate::node_graph::Hand;
use crate::style::EdgeCurve;

/// One edge of a cable path - a direct alias of the SDF's own segment verbs,
/// so a built path is handed to [`iced_nodegraph_sdf::Shape::path`] without a
/// lossy conversion.
pub(crate) type PathSeg = iced_nodegraph_sdf::PathSeg;

/// A circle a cable is laid tangent to: one anchor's orbit, resolved for this
/// frame in the space the cable is built in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Orbit {
    pub center: [f32; 2],
    pub radius: f32,
}

/// Where a cable meets an orbit, and which way it travels from there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Attachment {
    /// The tangent point on the orbit.
    pub point: [f32; 2],
    /// Unit direction of travel at that point, following the wrap.
    pub direction: [f32; 2],
}

impl Orbit {
    /// The tangent point reached from the external point `p`, on the side the
    /// cable wraps toward under `hand`.
    ///
    /// `None` when `p` lies inside the orbit: a point closer to the centre than
    /// `radius` has no tangent at all, so the caller has to fall back rather
    /// than emit a bent line pretending to be one.
    pub(crate) fn attachment(&self, p: [f32; 2], hand: Hand) -> Option<Attachment> {
        let to_p = [p[0] - self.center[0], p[1] - self.center[1]];
        let d = (to_p[0] * to_p[0] + to_p[1] * to_p[1]).sqrt();
        if d <= self.radius {
            return None;
        }
        // The two tangent points sit at +-acos(r/d) off the direction to `p`.
        // Which one is not a free choice: travelling the wrap onward from the
        // point is exactly what the incoming leg has to line up with, so the
        // sign is the wrap direction.
        let alpha = (self.radius / d).acos();
        let theta = to_p[1].atan2(to_p[0]) + hand.sign() * alpha;
        Some(Attachment {
            point: [
                self.center[0] + self.radius * theta.cos(),
                self.center[1] + self.radius * theta.sin(),
            ],
            direction: travel_dir(theta, hand),
        })
    }

    /// Signed sweep from `from` to `to` wrapping the way `hand` says: `hand`'s
    /// sign, magnitude in `[0, TAU)` - the form [`PathSeg::Arc`] wants.
    pub(crate) fn sweep(&self, from: [f32; 2], to: [f32; 2], hand: Hand) -> f32 {
        let angle = |p: [f32; 2]| (p[1] - self.center[1]).atan2(p[0] - self.center[0]);
        let delta = (angle(to) - angle(from)) * hand.sign();
        hand.sign() * delta.rem_euclid(std::f32::consts::TAU)
    }

    /// Which way a cable dragged from `from` should wrap, given where the
    /// cursor let go: the hand whose tangent point is nearer the drop.
    ///
    /// This is the whole of "from the left or from the right" - the user aims
    /// at a side of the ring, and that side IS the wrap direction.
    pub(crate) fn drop_hand(&self, from: [f32; 2], cursor: [f32; 2]) -> Option<Hand> {
        let reach = |hand| {
            self.attachment(from, hand).map(|a| {
                let d = [a.point[0] - cursor[0], a.point[1] - cursor[1]];
                d[0] * d[0] + d[1] * d[1]
            })
        };
        match (reach(Hand::Clockwise), reach(Hand::CounterClockwise)) {
            (Some(cw), Some(ccw)) if cw <= ccw => Some(Hand::Clockwise),
            (Some(_), Some(_)) => Some(Hand::CounterClockwise),
            _ => None,
        }
    }

    /// Distance from `p` to the ring itself, not to its centre - so the orbit
    /// nearest the cursor is the one whose circle it is closest to, whichever
    /// side of that circle the cursor is on.
    pub(crate) fn ring_distance(&self, p: [f32; 2]) -> f32 {
        let d = [p[0] - self.center[0], p[1] - self.center[1]];
        ((d[0] * d[0] + d[1] * d[1]).sqrt() - self.radius).abs()
    }
}

/// Unit travel direction at polar angle `theta`, following `hand`.
///
/// Screen space is y-down, so a positive sweep - and therefore
/// [`Hand::Clockwise`] - is the direction of increasing angle, matching
/// [`PathSeg::Arc`].
fn travel_dir(theta: f32, hand: Hand) -> [f32; 2] {
    let s = hand.sign();
    [-s * theta.sin(), s * theta.cos()]
}

/// Polar angle of `cursor` about `center` - the start angle [`PathSeg::Arc`]
/// leaves implicit, since an arc begins wherever the previous segment ended.
fn arc_start_angle(cursor: [f32; 2], center: [f32; 2]) -> f32 {
    (cursor[1] - center[1]).atan2(cursor[0] - center[0])
}

/// One station on a cable.
///
/// The chain always starts at a [`Hop::Pin`]; a trailing [`Hop::Wrap`] is a
/// half-attached connection, which draws its leg and stops there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hop {
    /// A node pin: its position and its side's outward normal.
    Pin { point: [f32; 2], side: u32 },
    /// An orbit wrap: the circle and which way round it goes.
    Wrap { orbit: Orbit, hand: Hand },
}

/// The built cable geometry for one connection: a start point plus its segment
/// chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EdgePath {
    pub start: [f32; 2],
    pub segs: Vec<PathSeg>,
}

impl EdgePath {
    /// The `Shape::path` recipe for this cable - the single stroke the SDF
    /// renders. Consuming, so the segment buffer moves into the shape instead
    /// of being copied once per frame per connection.
    pub(crate) fn into_shape(self) -> iced_nodegraph_sdf::Shape {
        iced_nodegraph_sdf::Shape::path(self.start, self.segs)
    }

    /// Shortest distance from `p` to any point on the cable.
    ///
    /// The walk carries the running cursor because an arc reads its start
    /// angle from where the previous segment left off.
    pub(crate) fn distance(&self, p: [f32; 2]) -> f32 {
        let mut cursor = self.start;
        let mut best = f32::MAX;
        for seg in &self.segs {
            best = best.min(seg_distance(cursor, seg, p));
            cursor = seg_end(cursor, seg);
        }
        best
    }

    /// Whether the finite probe segment `a`-`b` crosses the cable anywhere.
    ///
    /// Finite at both ends: a probe aimed at the cable but stopping short of
    /// it does not cross, which is what makes a cut gesture's stroke length
    /// mean something.
    pub(crate) fn intersects(&self, a: [f32; 2], b: [f32; 2]) -> bool {
        let mut cursor = self.start;
        for seg in &self.segs {
            if seg_intersects(cursor, seg, a, b) {
                return true;
            }
            cursor = seg_end(cursor, seg);
        }
        false
    }
}

/// Builds the cable through `hops`, in order.
///
/// Two pin hops is the plain edge: one leg, identical to what the widget drew
/// before anchors existed. Each wrap in between adds its entry leg and the arc
/// that leaves it.
///
/// A chain may also START on a wrap - a cable held at a ring while its other
/// end follows the cursor. There is no station before it, so that wrap
/// contributes only the tangent it leaves by, no arc.
///
/// A wrap whose orbit swallows the previous station (no tangent exists) is
/// skipped, so a frame still renders. Two wraps in a row aim the intervening
/// leg at the next orbit's centre rather than solving the common tangent
/// between two circles - an approximation, exact only for pin-to-orbit chains.
pub(crate) fn build(hops: &[Hop], curve: &EdgeCurve) -> EdgePath {
    let mut segs = Vec::with_capacity(hops.len() * 2);
    let empty = EdgePath {
        start: [0.0, 0.0],
        segs: Vec::new(),
    };
    let (start, mut dir) = match (hops.first(), hops.get(1)) {
        (Some(&Hop::Pin { point, side }), _) => (point, pin_side_direction(side)),
        (Some(&Hop::Wrap { orbit, hand }), Some(next)) => {
            let target = match *next {
                Hop::Pin { point, .. } => point,
                Hop::Wrap { orbit: far, .. } => far.center,
            };
            let Some(exit) = orbit.attachment(target, hand.flip()) else {
                return empty;
            };
            (exit.point, negate(exit.direction))
        }
        _ => return empty,
    };
    let mut cursor = start;

    for (i, hop) in hops.iter().enumerate().skip(1) {
        match *hop {
            Hop::Pin { point, side } => {
                let normal = pin_side_direction(side);
                push_leg(&mut segs, cursor, dir, point, normal, curve);
                cursor = point;
                dir = normal;
            }
            Hop::Wrap { orbit, hand } => {
                let Some(entry) = orbit.attachment(cursor, hand) else {
                    continue;
                };
                // The leg arrives ALONG the wrap, so its far control point is
                // placed against the travel direction. Anything else puts a
                // right-angle kink where the cable meets the circle.
                push_leg(
                    &mut segs,
                    cursor,
                    dir,
                    entry.point,
                    negate(entry.direction),
                    curve,
                );
                cursor = entry.point;
                dir = entry.direction;

                // The wrap exists only if the cable carries on; a trailing wrap
                // is a half-attached connection and ends at its tangent point.
                let Some(next) = hops.get(i + 1) else {
                    continue;
                };
                let target = match *next {
                    Hop::Pin { point, .. } => point,
                    Hop::Wrap { orbit: far, .. } => far.center,
                };
                // Read from the far end the cable is reversed, so the exit is
                // the far station's attachment under the opposite hand.
                let Some(exit) = orbit.attachment(target, hand.flip()) else {
                    continue;
                };
                segs.push(PathSeg::Arc {
                    center: orbit.center,
                    radius: orbit.radius,
                    sweep: orbit.sweep(cursor, exit.point, hand),
                });
                cursor = exit.point;
                dir = negate(exit.direction);
            }
        }
    }

    EdgePath { start, segs }
}

fn negate(v: [f32; 2]) -> [f32; 2] {
    [-v[0], -v[1]]
}

/// One leg between two stations: the widget's tangent-bezier construction with
/// pre-resolved endpoints and tangents, so the pin-to-pin case is bit-for-bit
/// the curve `edge_shape` builds.
///
/// `from_dir` points the way the cable leaves `from`; `to_dir` points AWAY from
/// the way it arrives at `to`, so both control points sit outside the leg.
fn push_leg(
    segs: &mut Vec<PathSeg>,
    from: [f32; 2],
    from_dir: [f32; 2],
    to: [f32; 2],
    to_dir: [f32; 2],
    curve: &EdgeCurve,
) {
    match curve {
        EdgeCurve::Line => segs.push(PathSeg::Line { to }),
        EdgeCurve::BezierCubic => {
            let l = adaptive_bezier_length(from, to);
            let c1 = [from[0] + from_dir[0] * l, from[1] + from_dir[1] * l];
            let c2 = [to[0] + to_dir[0] * l, to[1] + to_dir[1] * l];
            segs.push(PathSeg::Bezier { c1, c2, to });
        }
    }
}

/// How many chords a curved segment is flattened into for the queries above:
/// they are exact on a line and on an arc's radius, and polyline
/// approximations along a bezier leg and across any curve for intersection.
const CURVE_FLATTEN_SEGMENTS: usize = 32;

/// End point of `seg` starting from `cursor`.
fn seg_end(cursor: [f32; 2], seg: &PathSeg) -> [f32; 2] {
    match *seg {
        PathSeg::Line { to } | PathSeg::Bezier { to, .. } => to,
        PathSeg::Arc {
            center,
            radius,
            sweep,
        } => {
            let end = arc_start_angle(cursor, center) + sweep;
            [
                center[0] + radius * end.cos(),
                center[1] + radius * end.sin(),
            ]
        }
    }
}

fn seg_distance(cursor: [f32; 2], seg: &PathSeg, p: [f32; 2]) -> f32 {
    match *seg {
        PathSeg::Line { to } => dist_point_segment(p, cursor, to),
        PathSeg::Arc {
            center,
            radius,
            sweep,
        } => dist_point_arc(p, cursor, center, radius, sweep),
        PathSeg::Bezier { c1, c2, to } => dist_point_bezier(p, cursor, c1, c2, to),
    }
}

/// Point at parameter `t` in `[0, 1]` along `seg`, starting from `cursor`.
fn seg_point_at(cursor: [f32; 2], seg: &PathSeg, t: f32) -> [f32; 2] {
    match *seg {
        PathSeg::Line { to } => [
            cursor[0] + (to[0] - cursor[0]) * t,
            cursor[1] + (to[1] - cursor[1]) * t,
        ],
        PathSeg::Arc {
            center,
            radius,
            sweep,
        } => {
            let a = arc_start_angle(cursor, center) + sweep * t;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        }
        PathSeg::Bezier { c1, c2, to } => cubic_point(cursor, c1, c2, to, t),
    }
}

fn seg_intersects(cursor: [f32; 2], seg: &PathSeg, a: [f32; 2], b: [f32; 2]) -> bool {
    match *seg {
        PathSeg::Line { to } => segments_intersect(cursor, to, a, b),
        PathSeg::Arc { .. } | PathSeg::Bezier { .. } => {
            let mut prev = cursor;
            for i in 1..=CURVE_FLATTEN_SEGMENTS {
                let t = i as f32 / CURVE_FLATTEN_SEGMENTS as f32;
                let cur = seg_point_at(cursor, seg, t);
                if segments_intersect(prev, cur, a, b) {
                    return true;
                }
                prev = cur;
            }
            false
        }
    }
}

/// Whether `angle` falls inside the range swept from `start` by `sweep`,
/// measured along the sweep's own direction so a negative sweep reads
/// counter-clockwise.
fn angle_in_sweep(angle: f32, start: f32, sweep: f32) -> bool {
    if sweep == 0.0 {
        return (angle - start).rem_euclid(std::f32::consts::TAU) < 1e-6;
    }
    let off = ((angle - start) * sweep.signum()).rem_euclid(std::f32::consts::TAU);
    off <= sweep.abs() + 1e-6
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// Distance from `p` to the finite segment `a`-`b`, so a point past either end
/// measures to that end rather than to the infinite line.
fn dist_point_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 1e-12 {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let proj = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    dist2(p, proj).sqrt()
}

/// Distance from `p` to the SWEPT part of the circle only: within the sweep it
/// is the radial offset, outside it the distance to the nearer arc end. A
/// point sitting on the circle but off the wrap is far from the cable, not on
/// it.
fn dist_point_arc(p: [f32; 2], cursor: [f32; 2], center: [f32; 2], radius: f32, sweep: f32) -> f32 {
    let start = arc_start_angle(cursor, center);
    let rel = [p[0] - center[0], p[1] - center[1]];
    let ang = rel[1].atan2(rel[0]);
    if angle_in_sweep(ang, start, sweep) {
        return ((rel[0] * rel[0] + rel[1] * rel[1]).sqrt() - radius).abs();
    }
    let end = start + sweep;
    let a = [
        center[0] + radius * start.cos(),
        center[1] + radius * start.sin(),
    ];
    let b = [
        center[0] + radius * end.cos(),
        center[1] + radius * end.sin(),
    ];
    dist2(p, a).sqrt().min(dist2(p, b).sqrt())
}

fn cubic_point(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

fn dist_point_bezier(p: [f32; 2], p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> f32 {
    let mut prev = p0;
    let mut best = f32::MAX;
    for i in 1..=CURVE_FLATTEN_SEGMENTS {
        let t = i as f32 / CURVE_FLATTEN_SEGMENTS as f32;
        let cur = cubic_point(p0, p1, p2, p3, t);
        best = best.min(dist_point_segment(p, prev, cur));
        prev = cur;
    }
    best
}

fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Orientation test for two finite segments: true only when each straddles the
/// other's line, so touching endpoints and collinear overlap read as no
/// crossing.
fn segments_intersect(p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], p4: [f32; 2]) -> bool {
    let d1 = cross(p3, p4, p1);
    let d2 = cross(p3, p4, p2);
    let d3 = cross(p1, p2, p3);
    let d4 = cross(p1, p2, p4);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{PI, TAU};

    const ORBIT: Orbit = Orbit {
        center: [200.0, 200.0],
        radius: 40.0,
    };

    fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
        a[0] * b[1] - a[1] * b[0]
    }

    fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
        a[0] * b[0] + a[1] * b[1]
    }

    /// The regression contract for every edge that exists today: a chain of two
    /// pins must reproduce the widget's own tangent-bezier formula exactly, not
    /// merely something similar. Recomputed here from the same primitives
    /// rather than compared against a recorded constant.
    #[test]
    fn pin_to_pin_matches_edge_shape_formula() {
        let (p0, p1) = ([10.0, 20.0], [300.0, 140.0]);
        let path = build(
            &[
                Hop::Pin { point: p0, side: 1 },
                Hop::Pin { point: p1, side: 0 },
            ],
            &EdgeCurve::BezierCubic,
        );

        let l = adaptive_bezier_length(p0, p1);
        let (d0, d1) = (pin_side_direction(1), pin_side_direction(0));
        assert_eq!(path.start, p0);
        assert_eq!(
            path.segs,
            vec![PathSeg::Bezier {
                c1: [p0[0] + d0[0] * l, p0[1] + d0[1] * l],
                c2: [p1[0] + d1[0] * l, p1[1] + d1[1] * l],
                to: p1,
            }]
        );
    }

    #[test]
    fn pin_to_pin_line() {
        let (p0, p1) = ([0.0, 0.0], [80.0, 0.0]);
        let path = build(
            &[
                Hop::Pin { point: p0, side: 1 },
                Hop::Pin { point: p1, side: 0 },
            ],
            &EdgeCurve::Line,
        );
        assert_eq!(path.segs, vec![PathSeg::Line { to: p1 }]);
    }

    /// The defining property: the attachment lies ON the circle, and the line
    /// back to the external point is perpendicular to the radius there. If this
    /// holds, the leg is taut by construction.
    #[test]
    fn attachment_is_tangent() {
        let p = [420.0, 60.0];
        for hand in [Hand::Clockwise, Hand::CounterClockwise] {
            let a = ORBIT.attachment(p, hand).expect("point is outside");
            assert!(
                (dist(a.point, ORBIT.center) - ORBIT.radius).abs() < 1e-3,
                "{hand:?}: attachment is off the circle",
            );
            let radius = [a.point[0] - ORBIT.center[0], a.point[1] - ORBIT.center[1]];
            let leg = [p[0] - a.point[0], p[1] - a.point[1]];
            assert!(
                dot(radius, leg).abs() < 1e-2,
                "{hand:?}: leg is not perpendicular to the radius",
            );
            // The travel direction is the tangent, and it points along the leg's
            // reverse - the cable arrives heading into the wrap.
            assert!(
                cross(a.direction, leg).abs() < 1e-2,
                "{hand:?}: travel direction is not tangential",
            );
            assert!(
                dot(a.direction, leg) < 0.0,
                "{hand:?}: travel direction points back at the source",
            );
        }
    }

    /// The two hands are the two tangents, and they are distinct - that is what
    /// makes "from the left" and "from the right" different attachments and
    /// what lets an orbit take exactly two cables.
    #[test]
    fn the_two_hands_are_distinct_tangents() {
        let p = [420.0, 60.0];
        let cw = ORBIT.attachment(p, Hand::Clockwise).unwrap();
        let ccw = ORBIT.attachment(p, Hand::CounterClockwise).unwrap();
        assert!(
            dist(cw.point, ccw.point) > 1.0,
            "both hands resolved to the same tangent point",
        );
        // Mirror symmetry about the centre-to-p axis: equal distance from p.
        assert!((dist(cw.point, p) - dist(ccw.point, p)).abs() < 1e-2);
    }

    #[test]
    fn a_point_inside_the_orbit_has_no_tangent() {
        let inside = [ORBIT.center[0] + ORBIT.radius * 0.5, ORBIT.center[1]];
        assert!(ORBIT.attachment(inside, Hand::Clockwise).is_none());
        assert!(ORBIT.attachment(ORBIT.center, Hand::Clockwise).is_none());
    }

    /// Sweep carries the hand's sign and never wraps the wrong way round; a
    /// sign error here would send the cable the long way about the anchor.
    #[test]
    fn sweep_is_signed_by_hand() {
        let east = [ORBIT.center[0] + ORBIT.radius, ORBIT.center[1]];
        let south = [ORBIT.center[0], ORBIT.center[1] + ORBIT.radius];

        let cw = ORBIT.sweep(east, south, Hand::Clockwise);
        assert!((cw - PI / 2.0).abs() < 1e-3, "clockwise east->south: {cw}");

        let ccw = ORBIT.sweep(east, south, Hand::CounterClockwise);
        assert!(
            (ccw + 3.0 * PI / 2.0).abs() < 1e-3,
            "counter-clockwise east->south takes the long way: {ccw}",
        );
        assert!(cw.abs() < TAU && ccw.abs() < TAU);
    }

    /// A closed connection is bezier, arc, bezier - one path, so the pattern
    /// phases across all three.
    #[test]
    fn closed_chain_is_leg_arc_leg() {
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap {
                    orbit: ORBIT,
                    hand: Hand::Clockwise,
                },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        );

        assert_eq!(path.segs.len(), 3);
        let PathSeg::Bezier { to: entry, .. } = path.segs[0] else {
            panic!("first segment is not a leg: {:?}", path.segs[0]);
        };
        let PathSeg::Arc {
            center,
            radius,
            sweep,
        } = path.segs[1]
        else {
            panic!("middle segment is not an arc: {:?}", path.segs[1]);
        };
        assert_eq!(center, ORBIT.center);
        assert_eq!(radius, ORBIT.radius);
        assert!(sweep > 0.0, "clockwise wrap must sweep positive: {sweep}");
        assert!((dist(entry, ORBIT.center) - ORBIT.radius).abs() < 1e-3);
        assert!(matches!(path.segs[2], PathSeg::Bezier { .. }));
    }

    /// The bug the compass model had: a leg that meets the circle radially
    /// leaves a 90 degree kink. The incoming control point must lie along the
    /// wrap direction, so the leg and the arc share a tangent.
    #[test]
    fn the_leg_meets_the_arc_without_a_kink() {
        let hand = Hand::Clockwise;
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap { orbit: ORBIT, hand },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        );

        let PathSeg::Bezier { c2, to, .. } = path.segs[0] else {
            unreachable!()
        };
        // Direction the bezier arrives with, and the arc's tangent at the same
        // point, must be parallel and point the same way.
        let arrival = [to[0] - c2[0], to[1] - c2[1]];
        let expected = ORBIT
            .attachment([0.0, 200.0], hand)
            .expect("pin is outside")
            .direction;
        assert!(
            cross(arrival, expected).abs() / dist(to, c2) < 1e-2,
            "leg arrives at {arrival:?}, arc leaves along {expected:?}",
        );
        assert!(dot(arrival, expected) > 0.0, "leg arrives against the wrap");
    }

    /// Half-attached: one leg to the tangent point and nothing else. No arc,
    /// because there is no second cable to close it.
    #[test]
    fn half_chain_is_one_leg() {
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap {
                    orbit: ORBIT,
                    hand: Hand::Clockwise,
                },
            ],
            &EdgeCurve::BezierCubic,
        );

        assert_eq!(path.segs.len(), 1);
        let PathSeg::Bezier { to, .. } = path.segs[0] else {
            panic!("half chain is not a single leg: {:?}", path.segs)
        };
        assert!((dist(to, ORBIT.center) - ORBIT.radius).abs() < 1e-3);
    }

    /// Dropping picks the side you aimed at: the tangent point nearer the
    /// cursor decides the wrap, which is what "from the left or from the right"
    /// means in practice.
    #[test]
    fn the_drop_side_decides_the_hand() {
        let from = [0.0, 200.0];
        let cw = ORBIT.attachment(from, Hand::Clockwise).unwrap().point;
        let ccw = ORBIT
            .attachment(from, Hand::CounterClockwise)
            .unwrap()
            .point;

        assert_eq!(ORBIT.drop_hand(from, cw), Some(Hand::Clockwise));
        assert_eq!(ORBIT.drop_hand(from, ccw), Some(Hand::CounterClockwise));
    }

    #[test]
    fn a_pin_inside_the_orbit_has_no_drop_side() {
        assert_eq!(ORBIT.drop_hand(ORBIT.center, [0.0, 0.0]), None);
    }

    /// The nearest orbit is measured against the RING, so a cursor sitting on a
    /// wide ring picks that one rather than the tight inner shell it happens to
    /// be closer to the middle of.
    #[test]
    fn ring_distance_measures_to_the_circle() {
        let outer = Orbit {
            center: ORBIT.center,
            radius: 100.0,
        };
        let on_outer = [ORBIT.center[0] + 105.0, ORBIT.center[1]];
        assert!(outer.ring_distance(on_outer) < ORBIT.ring_distance(on_outer));

        let on_ring = [ORBIT.center[0] + ORBIT.radius, ORBIT.center[1]];
        assert!(ORBIT.ring_distance(on_ring) < 1e-3);
        // Inside counts like outside: it is a distance to the circle, not a
        // signed field.
        assert!((ORBIT.ring_distance(ORBIT.center) - ORBIT.radius).abs() < 1e-3);
    }

    /// Render-heal: an orbit that swallows the pin has no tangent, so the wrap
    /// drops out and the cable still reaches the far pin.
    #[test]
    fn a_swallowed_station_drops_its_wrap() {
        let far = [600.0, 200.0];
        let path = build(
            &[
                Hop::Pin {
                    point: ORBIT.center,
                    side: 1,
                },
                Hop::Wrap {
                    orbit: ORBIT,
                    hand: Hand::Clockwise,
                },
                Hop::Pin {
                    point: far,
                    side: 0,
                },
            ],
            &EdgeCurve::BezierCubic,
        );
        assert_eq!(path.segs.len(), 1);
        assert!(matches!(path.segs[0], PathSeg::Bezier { to, .. } if to == far));
    }

    /// The leg-arc-leg cable of [`closed_chain_is_leg_arc_leg`], plus the point
    /// its arc starts at and the arc's signed sweep - everything a query test
    /// needs to name a spot on the wrap.
    fn wrapped_cable() -> (EdgePath, [f32; 2], f32) {
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap {
                    orbit: ORBIT,
                    hand: Hand::Clockwise,
                },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        );
        let PathSeg::Bezier { to: entry, .. } = path.segs[0] else {
            panic!("first segment is not a leg: {:?}", path.segs[0]);
        };
        let PathSeg::Arc { sweep, .. } = path.segs[1] else {
            panic!("middle segment is not an arc: {:?}", path.segs[1]);
        };
        (path, entry, sweep)
    }

    /// A point on `ORBIT`'s centre at polar angle `angle`, `radius` out.
    fn polar(angle: f32, radius: f32) -> [f32; 2] {
        [
            ORBIT.center[0] + radius * angle.cos(),
            ORBIT.center[1] + radius * angle.sin(),
        ]
    }

    fn line_cable() -> EdgePath {
        build(
            &[
                Hop::Pin {
                    point: [0.0, 0.0],
                    side: 1,
                },
                Hop::Pin {
                    point: [100.0, 0.0],
                    side: 0,
                },
            ],
            &EdgeCurve::Line,
        )
    }

    /// The wrap is part of the cable, so a point on it is at distance zero -
    /// a query that only knew about the legs would miss the whole arc.
    #[test]
    fn distance_is_zero_on_the_wrap() {
        let (path, entry, sweep) = wrapped_cable();
        let mid = arc_start_angle(entry, ORBIT.center) + sweep / 2.0;
        let on_wrap = polar(mid, ORBIT.radius);
        assert!(
            path.distance(on_wrap) < 1e-2,
            "point on the wrap reads {} away",
            path.distance(on_wrap),
        );
    }

    /// Off the wrap radially, the distance IS the radial offset - and it counts
    /// the same from inside the circle as from outside.
    #[test]
    fn distance_beside_the_wrap_is_the_radial_offset() {
        let (path, entry, sweep) = wrapped_cable();
        let mid = arc_start_angle(entry, ORBIT.center) + sweep / 2.0;

        let outside = path.distance(polar(mid, ORBIT.radius + 12.0));
        assert!(
            (outside - 12.0).abs() < 1e-2,
            "12px outside reads {outside}"
        );

        let inside = path.distance(polar(mid, ORBIT.radius - 10.0));
        assert!((inside - 10.0).abs() < 1e-2, "10px inside reads {inside}");
    }

    /// The cable follows the SWEPT range, not the circle. A probe on the circle
    /// but diametrically opposite the wrap sits equally far from both arc ends,
    /// and that end distance is the answer - measuring against the full circle
    /// would read zero instead.
    #[test]
    fn a_point_past_the_sweep_measures_to_the_arc_end() {
        let (path, entry, sweep) = wrapped_cable();
        let arc = EdgePath {
            start: entry,
            segs: vec![path.segs[1]],
        };
        let mid = arc_start_angle(entry, ORBIT.center) + sweep / 2.0;
        let opposite = polar(mid + PI, ORBIT.radius);
        assert!(
            ORBIT.ring_distance(opposite) < 1e-3,
            "the probe must lie on the circle for this to mean anything",
        );

        let exit = seg_end(entry, &path.segs[1]);
        let nearer_end = dist(opposite, entry).min(dist(opposite, exit));
        let measured = arc.distance(opposite);
        assert!(
            (measured - nearer_end).abs() < 1e-2,
            "measured {measured}, nearer arc end is {nearer_end} away",
        );
        assert!(
            measured > ORBIT.radius,
            "measured {measured} hugs the circle"
        );
    }

    /// The cable goes ROUND the anchor, so its centre is a full radius away
    /// from every part of the cable - including the leg that leaves the wrap,
    /// which starts at the arc's far end rather than where the arc began.
    #[test]
    fn the_anchor_centre_is_a_radius_from_the_cable() {
        let (path, _, _) = wrapped_cable();
        let measured = path.distance(ORBIT.center);
        assert!(
            (measured - ORBIT.radius).abs() < 1e-2,
            "centre reads {measured} from the cable, radius is {}",
            ORBIT.radius,
        );
    }

    /// The analytic case: a straight cable, where the distance is the
    /// point-to-segment distance and the segment is finite - a point past the
    /// end measures to the end, not to the infinite line.
    #[test]
    fn distance_on_a_straight_cable_is_point_to_segment() {
        let path = line_cable();
        assert!((path.distance([50.0, 25.0]) - 25.0).abs() < 1e-3);
        assert!((path.distance([-30.0, 0.0]) - 30.0).abs() < 1e-3);
    }

    /// Cutting has to catch the cable wherever the stroke crosses it: on a leg,
    /// on the wrap, and nowhere else.
    #[test]
    fn intersects_catches_legs_and_wraps() {
        let (path, entry, _) = wrapped_cable();
        assert!(
            path.intersects([60.0, 150.0], [60.0, 250.0]),
            "a probe across the first leg must cross the cable",
        );

        let arc = EdgePath {
            start: entry,
            segs: vec![path.segs[1]],
        };
        let out = [ORBIT.center[0] + ORBIT.radius * 2.0, ORBIT.center[1]];
        let inner = [ORBIT.center[0] + ORBIT.radius * 0.25, ORBIT.center[1]];
        assert!(
            arc.intersects(inner, out),
            "a probe radially through the wrap must cross it",
        );

        assert!(
            !path.intersects([500.0, 500.0], [600.0, 600.0]),
            "a probe well clear of the cable must not cross it",
        );
    }

    /// The probe is a segment, not a ray: a stroke aimed at the cable but
    /// stopping short of it does not cut.
    #[test]
    fn a_probe_that_stops_short_does_not_cross() {
        let path = line_cable();
        assert!(
            !path.intersects([50.0, 20.0], [50.0, 10.0]),
            "the probe stops 10px above the cable",
        );
        assert!(
            path.intersects([50.0, 20.0], [50.0, -10.0]),
            "extended through the cable, the same probe crosses",
        );
    }
}
