//! The single source of cable geometry (`plan/routing-pins.md`).
//!
//! A cable is a chain of hops: a node pin, the anchor orbits it wraps, and the
//! node pin at the far end. It is emitted as one [`PathSeg`] chain for
//! [`iced_nodegraph_sdf::Shape::path`], which folds every segment through one
//! contour builder - so a dash or flow pattern phases once over the whole
//! cable instead of restarting at each waypoint.
//!
//! An orbit is met TANGENTIALLY, and which of the two tangents it takes is not
//! stored anywhere: [`build`] derives it from the run, taking the SHORT way
//! round between the neighbouring stations. The wrap itself is an exact
//! circular arc. Straight in, round the corner, straight out - taut, not
//! approximated, and the legs meet the arc without a kink because their control
//! points lie along the wrap direction.
//!
//! The pin-to-pin case is one direct tangent-bezier leg: same control-point
//! formula, same [`adaptive_bezier_length`].

use super::{adaptive_bezier_length, pin_side_direction};
use crate::style::EdgeCurve;

/// Which way a cable wraps an anchor, as seen on screen.
///
/// The orientation vocabulary of the geometry: a wrap is one signed arc sweep,
/// and this is its sign. Never stored on a connection - [`build`] derives it
/// per frame from the stations either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Hand {
    /// The cable passes its attachment point travelling clockwise on screen.
    Clockwise,
    /// The cable passes its attachment point travelling counter-clockwise.
    CounterClockwise,
}

impl Hand {
    /// The other way round.
    pub(crate) fn flip(self) -> Hand {
        match self {
            Hand::Clockwise => Hand::CounterClockwise,
            Hand::CounterClockwise => Hand::Clockwise,
        }
    }

    /// Signed unit of the wrap: `+1` clockwise, matching the sign convention of
    /// a screen-space (y-down) angle sweep.
    pub(crate) fn sign(self) -> f32 {
        match self {
            Hand::Clockwise => 1.0,
            Hand::CounterClockwise => -1.0,
        }
    }
}

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

/// One station on a cable, in the order the cable passes through them.
///
/// [`Hop::Wrap`] is a station the cable is carried ON THROUGH, so it needs a
/// station either side to derive its wrap direction from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hop {
    /// A node pin: its position and its side's outward normal.
    Pin { point: [f32; 2], side: u32 },
    /// A through-station: the ring carries the cable on, wrapping the short way
    /// round between its neighbours.
    Wrap { orbit: Orbit },
}

/// The built cable geometry for one connection: a start point plus its segment
/// chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EdgePath {
    pub start: [f32; 2],
    pub segs: Vec<PathSeg>,
}

/// The closest point on a cable to a probe: how far away it is, and how far
/// along the cable it sits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Nearest {
    pub distance: f32,
    pub arc_len: f32,
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
            best = best.min(seg_nearest(cursor, seg, p).distance);
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

    /// The closest point on the cable to `p`: how far away it is, and how far
    /// along the cable it sits.
    ///
    /// The arc length is exact along a line and an arc, and along a bezier leg
    /// is measured over the same chords [`seg_length`] sums - so it is in the
    /// same units as [`Self::total_len`], and [`Self::point_at`] takes it back
    /// to the point it was found at. A cable with no segments is just its
    /// start point, at arc length zero.
    pub(crate) fn nearest(&self, p: [f32; 2]) -> Nearest {
        let mut cursor = self.start;
        let mut walked = 0.0;
        let mut best = Nearest {
            distance: dist2(p, self.start).sqrt(),
            arc_len: 0.0,
        };
        for seg in &self.segs {
            let near = seg_nearest(cursor, seg, p);
            if near.distance < best.distance {
                best = Nearest {
                    distance: near.distance,
                    arc_len: walked + near.along,
                };
            }
            walked += near.length;
            cursor = seg_end(cursor, seg);
        }
        best
    }

    /// Arc length of the whole cable.
    pub(crate) fn total_len(&self) -> f32 {
        let mut cursor = self.start;
        let mut walked = 0.0;
        for seg in &self.segs {
            walked += seg_length(cursor, seg);
            cursor = seg_end(cursor, seg);
        }
        walked
    }

    /// The sub-path covering the arc-length window `from_len`..`to_len`,
    /// clamped to the cable's own length.
    ///
    /// Segments outside the window are dropped and the ones at its edges are
    /// cut, so the result starts at `from_len` and ends at `to_len`. Along a
    /// bezier leg the cut lands there by inverting the chord walk
    /// [`seg_length`] measures with, so a window taken around a
    /// [`Self::nearest`] hit covers the stretch of cable that hit was found
    /// on. An empty or inverted window is the single point at `from_len` with
    /// no segments, which callers skip.
    pub(crate) fn slice(&self, from_len: f32, to_len: f32) -> EdgePath {
        let total = self.total_len();
        let from = from_len.clamp(0.0, total);
        let to = to_len.clamp(0.0, total);
        let mut out = EdgePath {
            start: self.point_at(from),
            segs: Vec::new(),
        };
        if to <= from {
            return out;
        }
        let mut cursor = self.start;
        let mut walked = 0.0;
        for seg in &self.segs {
            let len = seg_length(cursor, seg);
            let end = walked + len;
            if end > from && walked < to && len > 1e-6 {
                let t0 = seg_param_at_len(cursor, seg, from - walked);
                let t1 = seg_param_at_len(cursor, seg, to - walked);
                if t1 > t0 {
                    out.segs.push(seg_slice(cursor, seg, t0, t1));
                }
            }
            walked = end;
            cursor = seg_end(cursor, seg);
        }
        out
    }

    /// The point `len` along the cable, clamped to its ends.
    fn point_at(&self, len: f32) -> [f32; 2] {
        let mut cursor = self.start;
        let mut walked = 0.0;
        for seg in &self.segs {
            let seg_len = seg_length(cursor, seg);
            if len <= walked + seg_len {
                let t = seg_param_at_len(cursor, seg, len - walked);
                return seg_point_at(cursor, seg, t);
            }
            walked += seg_len;
            cursor = seg_end(cursor, seg);
        }
        cursor
    }
}

/// Everything one cable's geometry says: the path to stroke, plus where it
/// meets each ring it passes.
///
/// The touch points are what a gesture aims at, so they are read off the same
/// build the frame is drawn from rather than re-derived beside it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Built {
    pub path: EdgePath,
    /// One entry per [`Hop::Wrap`] the cable rounds, in hop order. A wrap
    /// whose geometry did not resolve contributes nothing.
    pub touches: Vec<RingTouch>,
}

/// Where a cable meets one ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RingTouch {
    /// Index into the hop slice this was built from.
    pub hop: usize,
    /// The tangent point the cable arrives at.
    pub entry: [f32; 2],
    /// The tangent point the cable leaves along.
    pub exit: [f32; 2],
    /// Arc-length window the wrap occupies within the whole built path: the
    /// length consumed before the arc, and the length consumed after it.
    pub span: (f32, f32),
}

/// Builds the cable through `hops`, in order.
///
/// Two pin hops is the plain edge: one direct leg. Each wrap in between adds
/// its entry leg and the arc that leaves it, taking whichever of the two ways
/// round realizes the shorter sweep.
///
/// Two wraps in a row are the belt between two pulleys: the run between them is
/// the tangent common to both rings, so a cable chaining several anchors is
/// taut across the whole chain. That tangent exists for one PAIR of hands, so
/// the pair is settled together - the ring a run reaches keeps the hand the
/// exit aimed at it was built for.
///
/// A wrap whose orbit swallows the previous station, a pair of orbits too close
/// for a common tangent, and a wrap the cable cannot leave again are skipped so
/// a frame still renders.
pub(crate) fn build(hops: &[Hop], curve: &EdgeCurve) -> Built {
    let empty = Built {
        path: EdgePath {
            start: [0.0, 0.0],
            segs: Vec::new(),
        },
        touches: Vec::new(),
    };
    let n = hops.len();
    if n == 0 {
        return empty;
    }
    // What a neighbouring station is AIMED at for a direction decision: the
    // real attachment points are not known until the hand is, so a ring stands
    // in for itself by its centre.
    let aim = |i: usize| match hops[i] {
        Hop::Pin { point, .. } => point,
        Hop::Wrap { orbit } => orbit.center,
    };
    let hand_at = |i: usize| -> Option<Hand> {
        // A wrap is carried on through, so it needs a station either side to
        // aim between: one in first or last position has none.
        if i == 0 || i + 1 == n {
            return None;
        }
        let Hop::Wrap { orbit } = hops[i] else {
            return None;
        };
        pick_hand(orbit, aim(i - 1), aim(i + 1))
    };
    // Where the cable leaves the ring at `hop` wrapping it with `hand`, as the
    // choice of that hand needs to see it: a pin hands it that pin's own
    // tangent, reversed because the cable is read from the far end there, and
    // a further ring hands it the run its aim points name. `None` where the
    // ring has nothing to carry the cable on to.
    let exit_toward = |hop: usize, orbit: Orbit, hand: Hand| -> Option<Attachment> {
        if hop + 1 == n {
            return None;
        }
        match hops[hop + 1] {
            Hop::Wrap { orbit: far } => Some(belt((orbit, hand), (far, hand_at(hop + 1)?))?.0),
            Hop::Pin { .. } => {
                let a = orbit.attachment(aim(hop + 1), hand.flip())?;
                Some(Attachment {
                    point: a.point,
                    direction: negate(a.direction),
                })
            }
        }
    };

    let mut segs = Vec::with_capacity(n * 2);
    let mut touches = Vec::new();
    // A cable runs pin to pin: a leading wrap has no run to set off along.
    let (start, start_side) = match hops[0] {
        Hop::Pin { point, side } => (point, side),
        Hop::Wrap { .. } => return empty,
    };
    let mut dir = pin_side_direction(start_side);
    let mut on_tangent = false;
    let mut cursor = start;
    // Length of everything pushed so far, so each wrap can record where along
    // the finished cable its arc sits.
    let mut walked = 0.0;
    // The hand a wrap's own exit committed the ring it runs to, so that ring
    // takes the hand its run was cut for rather than choosing again.
    let mut bound_hand = None;

    for i in 1..n {
        // The hand the run into this hop was cut for, if the wrap before it
        // laid one. Taken, so it binds this hop and no later one.
        let bound = bound_hand.take();
        let from = Leg {
            point: cursor,
            dir,
            tangent: on_tangent,
        };
        match hops[i] {
            Hop::Pin { point, side } => {
                let normal = pin_side_direction(side);
                push_leg(
                    &mut segs,
                    &mut walked,
                    from,
                    Leg {
                        point,
                        dir: normal,
                        tangent: false,
                    },
                    curve,
                );
                cursor = point;
                dir = normal;
                on_tangent = false;
            }
            Hop::Wrap { orbit } => {
                // A wrap is carried on through, so it needs a station either
                // side: one in last position has nowhere to carry the cable to.
                if i + 1 == n {
                    continue;
                }
                // Both ways round, realized in full, and the shorter arc wins,
                // unless the run into the ring already bound it to one hand.
                // `tangent_from_the_run` walks the entry OFF the tangent the
                // aim points name, so a hand read from those aims can come
                // back with a near-zero sweep that realizes the other side of
                // the exit - and a sweep normalised into `[0, TAU)` turns that
                // overshoot into a lap of the whole ring. The sweep the arc is
                // built from is the only one worth comparing. It costs a
                // second [`TANGENT_PASSES`] walk per wrap.
                //
                // Resolved before anything is pushed, so a wrap the cable
                // cannot leave again contributes neither a leg nor a touch.
                let realized = |hand: Hand| -> Option<(Attachment, Attachment, Option<Hand>)> {
                    let entry = tangent_from_the_run(orbit, hand, from)?;
                    let Hop::Wrap { orbit: far } = hops[i + 1] else {
                        return Some((entry, exit_toward(i, orbit, hand)?, None));
                    };
                    // A run tangent to both rings is only the common tangent
                    // while the far ring wraps the way the run was cut for, so
                    // its hand is settled here, by the comparison it would
                    // make on arrival: the smaller sweep between the run this
                    // exit hands it and the station beyond that.
                    let (exit, far_hand, _) = [Hand::Clockwise, Hand::CounterClockwise]
                        .into_iter()
                        .filter_map(|far_hand| {
                            let (exit, landing) = belt((orbit, hand), (far, far_hand))?;
                            let leaving = exit_toward(i + 1, far, far_hand)?;
                            let sweep = far.sweep(landing.point, leaving.point, far_hand);
                            Some((exit, far_hand, sweep))
                        })
                        .min_by(|a, b| a.2.abs().total_cmp(&b.2.abs()))?;
                    Some((entry, exit, Some(far_hand)))
                };
                let wrap = [Hand::Clockwise, Hand::CounterClockwise]
                    .into_iter()
                    .filter(|hand| bound.is_none_or(|committed| committed == *hand))
                    .filter_map(|hand| {
                        let (entry, exit, beyond) = realized(hand)?;
                        let sweep = orbit.sweep(entry.point, exit.point, hand);
                        Some((entry, exit, sweep, beyond))
                    })
                    .min_by(|a, b| a.2.abs().total_cmp(&b.2.abs()));
                let Some((entry, exit, sweep, beyond)) = wrap else {
                    continue;
                };
                // The leg arrives ALONG the wrap, so its far control point is
                // placed against the travel direction. Anything else puts a
                // right-angle kink where the cable meets the circle.
                push_leg(
                    &mut segs,
                    &mut walked,
                    from,
                    Leg {
                        point: entry.point,
                        dir: negate(entry.direction),
                        tangent: true,
                    },
                    curve,
                );
                let arc = PathSeg::Arc {
                    center: orbit.center,
                    radius: orbit.radius,
                    sweep,
                };
                let span_start = walked;
                walked += seg_length(entry.point, &arc);
                segs.push(arc);
                touches.push(RingTouch {
                    hop: i,
                    entry: entry.point,
                    exit: exit.point,
                    span: (span_start, walked),
                });
                cursor = exit.point;
                dir = exit.direction;
                on_tangent = true;
                bound_hand = beyond;
            }
        }
    }

    Built {
        path: EdgePath { start, segs },
        touches,
    }
}

/// Which way the cable wraps `orbit`, judged from where its neighbours sit.
///
/// Of the two directions, the one whose arc between entry and exit tangent
/// sweeps less. The aim points stand in for the real attachment points - a
/// pin's own position, the cursor for a preview, and a neighbouring ring's
/// centre - so this reads the hand a ring wants before any leg into it has
/// been laid.
///
/// `None` when neither direction has a tangent at all, which is an aim point
/// inside the ring on both sides.
fn pick_hand(orbit: Orbit, prev_aim: [f32; 2], next_aim: [f32; 2]) -> Option<Hand> {
    let swept = |hand: Hand| {
        let entry = orbit.attachment(prev_aim, hand)?;
        let exit = orbit.attachment(next_aim, hand.flip())?;
        Some(orbit.sweep(entry.point, exit.point, hand).abs())
    };
    match (swept(Hand::Clockwise), swept(Hand::CounterClockwise)) {
        (Some(cw), Some(ccw)) if ccw < cw => Some(Hand::CounterClockwise),
        (Some(_), _) => Some(Hand::Clockwise),
        (None, Some(_)) => Some(Hand::CounterClockwise),
        (None, None) => None,
    }
}

fn negate(v: [f32; 2]) -> [f32; 2] {
    [-v[0], -v[1]]
}

/// Rotate a vector a quarter turn.
fn rot90(v: [f32; 2]) -> [f32; 2] {
    [-v[1], v[0]]
}

/// The straight run between two orbits: where the cable leaves `from` and where
/// it meets `to`, tangent to BOTH.
///
/// The belt-around-two-pulleys problem. Wrapping both the same way takes the
/// outer tangent, wrapping them opposite ways takes the crossed one, and a
/// single signed radius `s1*r1 - s2*r2` covers both: the leg has to sit that
/// far off the line of centres for the two normals to line up.
///
/// `None` when the circles are too close for such a tangent to exist - one
/// swallows the other, or they overlap in the crossed case.
pub(crate) fn belt(from: (Orbit, Hand), to: (Orbit, Hand)) -> Option<(Attachment, Attachment)> {
    let (a, ha) = from;
    let (b, hb) = to;
    let d = [b.center[0] - a.center[0], b.center[1] - a.center[1]];
    let span = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let offset = ha.sign() * a.radius - hb.sign() * b.radius;
    if span <= offset.abs() {
        return None;
    }
    // Travel direction of the leg: the heading whose perpendicular is `offset`
    // away from the line of centres.
    let heading = d[1].atan2(d[0]) + (offset / span).asin();
    let u = [heading.cos(), heading.sin()];
    let n = rot90(u);
    let touch = |orbit: Orbit, hand: Hand| {
        let s = -hand.sign();
        Attachment {
            point: [
                orbit.center[0] + orbit.radius * s * n[0],
                orbit.center[1] + orbit.radius * s * n[1],
            ],
            direction: u,
        }
    };
    Some((touch(a, ha), touch(b, hb)))
}

/// One leg between two stations: the widget's tangent-bezier construction with
/// pre-resolved endpoints and tangents, so the pin-to-pin case is bit-for-bit
/// the curve `edge_shape` builds.
///
/// `from_dir` points the way the cable leaves `from`; `to_dir` points AWAY from
/// the way it arrives at `to`, so both control points sit outside the leg.
/// Control-point length for a leg that has to turn a full quarter circle onto
/// the line it wants to run along.
///
/// A leg that ends on a tangent already has the straight line as its ideal
/// shape - the tangent point lies ON the line out of the pin. Only the pin end
/// pulls away from it, the more so the further it has to turn, so the reach is
/// scaled down by that turn rather than left at the full pin-to-pin length.
/// A leg already pointing down the line keeps its full reach and stays
/// straight; a leg square to it gets exactly this much.
///
/// The offset actually reached is about half of it, so a square exit leaves its
/// line by roughly 23 world pixels against the 41 an unscaled reach gives. That
/// is the dial between a cable that bulges out of the node and one strained
/// flat against it.
const TAUT_EXIT: f32 = 45.0;

/// How many times the touch point is re-read before it is taken as settled.
///
/// Two passes cover the ordinary case; the count only bites where the station
/// sits close enough to the orbit that the reach keeps changing with it, and
/// there the moves shrink by about eight to one each pass.
const TANGENT_PASSES: usize = 4;

/// Move below which a re-read has stopped saying anything, in world pixels.
const TANGENT_SETTLED: f32 = 0.05;

/// Where a leg starting at `from` meets `orbit`, taken from the line the cable
/// actually runs in rather than from the station it left.
///
/// A leg out of a pin does not set off toward the anchor: it leaves square to
/// the node and only then turns down its line, so that line starts at the
/// control point, not at the pin. Read the tangent from the pin and the cable
/// bends AGAINST the wrap just before touching it, then snaps back - a small
/// reversal the eye catches as a kink.
///
/// Where the control point sits depends in turn on where the leg ends, so this
/// is a fixed point rather than a formula, and it is walked to instead of
/// solved. It is worth reaching: there the control point lies ON the tangent,
/// which puts the leg's last two control spans along it as well. The cable then
/// arrives with zero curvature, straight down the tangent, so it can neither
/// counter-bend into the wrap nor cut the corner inside it.
fn tangent_from_the_run(orbit: Orbit, hand: Hand, from: Leg) -> Option<Attachment> {
    let mut touch = orbit.attachment(from.point, hand)?;
    // Already running down its own line: the control point lies on it, so every
    // re-read lands in the same place.
    if from.tangent {
        return Some(touch);
    }
    for _ in 0..TANGENT_PASSES {
        let (reach, _) = leg_reaches(
            from,
            Leg {
                point: touch.point,
                dir: negate(touch.direction),
                tangent: true,
            },
        );
        let pivot = [
            from.point[0] + from.dir[0] * reach,
            from.point[1] + from.dir[1] * reach,
        ];
        // A pivot swallowed by the orbit has no tangent; the last good reading
        // stands.
        let Some(next) = orbit.attachment(pivot, hand) else {
            break;
        };
        let moved = (next.point[0] - touch.point[0]).hypot(next.point[1] - touch.point[1]);
        touch = next;
        if moved < TANGENT_SETTLED {
            break;
        }
    }
    Some(touch)
}

/// One end of a leg: where it is, which way the cable runs there, and whether
/// that direction is a tangent the cable already lies along rather than a
/// pin's normal it has to turn away from.
#[derive(Debug, Clone, Copy)]
struct Leg {
    point: [f32; 2],
    dir: [f32; 2],
    tangent: bool,
}

/// How far each end's control point reaches.
///
/// A leg between two pins is an S-curve and wants symmetry. A leg with one
/// tangent end wants the line the tangent already lies on, so its free end
/// bends only as far as leaving the node square requires. Two tangents are the
/// line, and neither end pulls off it.
fn leg_reaches(from: Leg, to: Leg) -> (f32, f32) {
    let l = adaptive_bezier_length(from.point, to.point);
    let d = [to.point[0] - from.point[0], to.point[1] - from.point[1]];
    match (from.tangent, to.tangent) {
        (false, true) => (bow_limited(l, from.dir, d), l),
        (true, false) => (l, bow_limited(l, to.dir, d)),
        _ => (l, l),
    }
}

/// Pushes the segment of one leg and advances the running arc length by it.
fn push_leg(segs: &mut Vec<PathSeg>, walked: &mut f32, from: Leg, to: Leg, curve: &EdgeCurve) {
    let Leg {
        point: from_point,
        dir: from_dir,
        ..
    } = from;
    let Leg {
        point: to_point,
        dir: to_dir,
        ..
    } = to;
    let seg = match curve {
        EdgeCurve::Line => PathSeg::Line { to: to_point },
        EdgeCurve::BezierCubic => {
            let (l_from, l_to) = leg_reaches(from, to);
            let c1 = [
                from_point[0] + from_dir[0] * l_from,
                from_point[1] + from_dir[1] * l_from,
            ];
            let c2 = [
                to_point[0] + to_dir[0] * l_to,
                to_point[1] + to_dir[1] * l_to,
            ];
            PathSeg::Bezier {
                c1,
                c2,
                to: to_point,
            }
        }
    };
    *walked += seg_length(from_point, &seg);
    segs.push(seg);
}

/// Shortens a control length by how far its end has to turn onto `span`.
///
/// The sine of that turn is what decides how far the curve leaves the span, so
/// the reach is [`TAUT_EXIT`] at a quarter turn and grows as the leg lines up
/// with the span - where a long reach costs no bow at all. Never longer than
/// `l`, which already accounts for how much room there is.
fn bow_limited(l: f32, dir: [f32; 2], span: [f32; 2]) -> f32 {
    let len = (span[0] * span[0] + span[1] * span[1]).sqrt();
    if len <= f32::EPSILON {
        return l;
    }
    let turn = (dir[0] * span[1] - dir[1] * span[0]).abs() / len;
    l.min(TAUT_EXIT / turn.max(f32::EPSILON))
}

/// How many chords a curved segment is flattened into.
///
/// The queries above are exact on a line and on an arc's radius; along a
/// bezier leg, and across any curve for intersection, they are answered off
/// this polyline. One count serves all of them, so a length, a hit's distance
/// along the cable and a cut are read from the same chords and agree with each
/// other rather than to within the gap between two flattenings.
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

/// Where a probe lands against one segment.
struct SegNearest {
    /// Distance from the probe to the segment.
    distance: f32,
    /// Arc length from the segment's start to the closest point on it.
    along: f32,
    /// Arc length of the whole segment.
    length: f32,
}

/// Distance from `p` to `seg`, and where along `seg` the closest point to it
/// sits.
fn seg_nearest(cursor: [f32; 2], seg: &PathSeg, p: [f32; 2]) -> SegNearest {
    // A line and an arc run at constant speed in their parameter, so the
    // closest point's parameter scales straight to an arc length. A cubic does
    // not, and its own walk measures the length instead.
    let by_param = |(distance, t): (f32, f32)| {
        let length = seg_length(cursor, seg);
        SegNearest {
            distance,
            along: t * length,
            length,
        }
    };
    match *seg {
        PathSeg::Line { to } => by_param(nearest_on_segment(p, cursor, to)),
        PathSeg::Arc {
            center,
            radius,
            sweep,
        } => by_param(nearest_on_arc(p, cursor, center, radius, sweep)),
        PathSeg::Bezier { c1, c2, to } => nearest_on_bezier(p, cursor, c1, c2, to),
    }
}

/// Point at parameter `t` in `[0, 1]` along `seg`, starting from `cursor`.
fn seg_point_at(cursor: [f32; 2], seg: &PathSeg, t: f32) -> [f32; 2] {
    match *seg {
        PathSeg::Line { to } => lerp(cursor, to, t),
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

/// `a` moved a `t` fraction of the way to `b`.
fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Arc length of `seg` starting from `cursor`.
///
/// Exact on a line and on an arc; a bezier is summed over its
/// [`CURVE_FLATTEN_SEGMENTS`] chords, so it reads a little short of the true
/// curve - under a ten-thousandth of it on a cable-length leg. Lengths feed
/// hit-testing and glow placement, where that gap sits well inside a stroke
/// width.
fn seg_length(cursor: [f32; 2], seg: &PathSeg) -> f32 {
    match *seg {
        PathSeg::Line { to } => dist2(cursor, to).sqrt(),
        PathSeg::Arc { radius, sweep, .. } => sweep.abs() * radius,
        PathSeg::Bezier { c1, c2, to } => {
            let mut prev = cursor;
            let mut len = 0.0;
            for i in 1..=CURVE_FLATTEN_SEGMENTS {
                let cur = cubic_point(cursor, c1, c2, to, i as f32 / CURVE_FLATTEN_SEGMENTS as f32);
                len += dist2(prev, cur).sqrt();
                prev = cur;
            }
            len
        }
    }
}

/// The parameter at arc length `len` along `seg`, inverting the walk
/// [`seg_length`] measures with - so a length that came out of
/// [`EdgePath::nearest`] names the point that hit was found at.
fn seg_param_at_len(cursor: [f32; 2], seg: &PathSeg, len: f32) -> f32 {
    match *seg {
        // Constant speed in the parameter: the fraction of the length taken IS
        // the parameter.
        PathSeg::Line { .. } | PathSeg::Arc { .. } => {
            (len / seg_length(cursor, seg).max(1e-6)).clamp(0.0, 1.0)
        }
        PathSeg::Bezier { c1, c2, to } => cubic_param_at_len([cursor, c1, c2, to], len),
    }
}

/// The part of `seg` between parameters `t0` and `t1`, starting from `cursor`.
///
/// An arc leaves its start implicit - it is read from wherever the path left
/// off - so a cut that trims the start only lands right if the caller begins
/// the segment at the arc's own `t0` point.
fn seg_slice(cursor: [f32; 2], seg: &PathSeg, t0: f32, t1: f32) -> PathSeg {
    match *seg {
        PathSeg::Line { to } => PathSeg::Line {
            to: lerp(cursor, to, t1),
        },
        PathSeg::Arc {
            center,
            radius,
            sweep,
        } => PathSeg::Arc {
            center,
            radius,
            sweep: sweep * (t1 - t0),
        },
        PathSeg::Bezier { c1, c2, to } => {
            let [_, c1, c2, to] = cubic_between([cursor, c1, c2, to], t0, t1);
            PathSeg::Bezier { c1, c2, to }
        }
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

/// Distance from `p` to the finite segment `a`-`b` and the parameter of the
/// closest point on it, so a point past either end measures to that end rather
/// than to the infinite line.
fn nearest_on_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 1e-12 {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let proj = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    (dist2(p, proj).sqrt(), t)
}

/// Distance from `p` to the SWEPT part of the circle only, and the parameter
/// of the closest point: within the sweep it is the radial offset, outside it
/// the distance to the nearer arc end, which is that end's own parameter. A
/// point sitting on the circle but off the wrap is far from the cable, not on
/// it.
fn nearest_on_arc(
    p: [f32; 2],
    cursor: [f32; 2],
    center: [f32; 2],
    radius: f32,
    sweep: f32,
) -> (f32, f32) {
    let start = arc_start_angle(cursor, center);
    let rel = [p[0] - center[0], p[1] - center[1]];
    let ang = rel[1].atan2(rel[0]);
    if angle_in_sweep(ang, start, sweep) {
        let off = ((ang - start) * sweep.signum()).rem_euclid(std::f32::consts::TAU);
        let t = (off / sweep.abs().max(1e-6)).clamp(0.0, 1.0);
        return (
            ((rel[0] * rel[0] + rel[1] * rel[1]).sqrt() - radius).abs(),
            t,
        );
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
    let (to_start, to_end) = (dist2(p, a).sqrt(), dist2(p, b).sqrt());
    if to_end < to_start {
        return (to_end, 1.0);
    }
    (to_start, 0.0)
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

/// Splits the cubic at `t`, returning the control points of the part before it
/// and the part after.
fn cubic_split(p: [[f32; 2]; 4], t: f32) -> ([[f32; 2]; 4], [[f32; 2]; 4]) {
    let a = lerp(p[0], p[1], t);
    let b = lerp(p[1], p[2], t);
    let c = lerp(p[2], p[3], t);
    let d = lerp(a, b, t);
    let e = lerp(b, c, t);
    let f = lerp(d, e, t);
    ([p[0], a, d, f], [f, e, c, p[3]])
}

/// Control points of the cubic restricted to `t0`..`t1`.
fn cubic_between(p: [[f32; 2]; 4], t0: f32, t1: f32) -> [[f32; 2]; 4] {
    let (left, _) = cubic_split(p, t1);
    cubic_split(left, t0 / t1.max(f32::EPSILON)).1
}

/// Distance from `p` to the cubic, the arc length at the closest point on it,
/// and the cubic's own length, all off one walk of its
/// [`CURVE_FLATTEN_SEGMENTS`] chords.
///
/// The chords are accumulated rather than the parameter scaled: arc length
/// along a cubic is not linear in the parameter, and the two disagree by more
/// than a stroke width over a cable-length leg.
fn nearest_on_bezier(
    p: [f32; 2],
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
) -> SegNearest {
    let mut prev = p0;
    let mut walked = 0.0;
    let mut best = SegNearest {
        distance: f32::MAX,
        along: 0.0,
        length: 0.0,
    };
    for i in 1..=CURVE_FLATTEN_SEGMENTS {
        let cur = cubic_point(p0, p1, p2, p3, i as f32 / CURVE_FLATTEN_SEGMENTS as f32);
        let chord = dist2(prev, cur).sqrt();
        let (distance, along) = nearest_on_segment(p, prev, cur);
        if distance < best.distance {
            best.distance = distance;
            best.along = walked + along * chord;
        }
        walked += chord;
        prev = cur;
    }
    best.length = walked;
    best
}

/// The parameter at arc length `len` along the cubic, inverting the chord walk
/// [`seg_length`] measures it with: the chord that length falls in, taken the
/// same fraction into that chord's own parameter span. A length past either
/// end is that end.
fn cubic_param_at_len(p: [[f32; 2]; 4], len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    let step = 1.0 / CURVE_FLATTEN_SEGMENTS as f32;
    let mut prev = p[0];
    let mut walked = 0.0;
    for i in 1..=CURVE_FLATTEN_SEGMENTS {
        let t = i as f32 * step;
        let cur = cubic_point(p[0], p[1], p[2], p[3], t);
        let chord = dist2(prev, cur).sqrt();
        if walked + chord >= len {
            let along = if chord > 1e-6 {
                (len - walked) / chord
            } else {
                0.0
            };
            return t - step + along * step;
        }
        walked += chord;
        prev = cur;
    }
    1.0
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

/// Where two chords cross, if they do.
///
/// Gated on [`segments_intersect`], so that straddle rule decides it and a
/// collinear overlap is no crossing. Past the gate the two chords are not
/// parallel - parallel ones give `t`'s line the same signed area at both ends
/// of `s`, which the gate rejects - so that area changes sign along `s`, and
/// the crossing is at the parameter where it reaches zero.
///
/// A pair with an end in common is dropped whatever the gate makes of it: two
/// straight chords meeting at a point meet ONLY there, so no crossing is lost,
/// and that point is the pin the two cables share rather than anything they do
/// between pins.
fn chord_crossing(s: [[f32; 2]; 2], t: [[f32; 2]; 2]) -> Option<[f32; 2]> {
    if !segments_intersect(s[0], s[1], t[0], t[1]) || shares_end(s, t) {
        return None;
    }
    let d1 = cross(t[0], t[1], s[0]);
    let d2 = cross(t[0], t[1], s[1]);
    Some(lerp(s[0], s[1], d1 / (d1 - d2)))
}

/// Whether the two chords have an end in common.
///
/// Compared exactly, because a pin's own coordinates are what both cables are
/// built from: a leg carries its far end verbatim, and a bezier sampled at its
/// last step reduces to that same end point.
fn shares_end(s: [[f32; 2]; 2], t: [[f32; 2]; 2]) -> bool {
    s.iter().any(|p| t.contains(p))
}

/// The polyline a cable is flattened to: its start point, then the end of
/// every chord in path order.
///
/// A line is a chord already. An arc and a bezier leg are sampled at
/// [`CURVE_FLATTEN_SEGMENTS`] equal parameter steps - the same flattening
/// [`seg_intersects`] probes against.
pub(crate) fn polyline(path: &EdgePath) -> Vec<[f32; 2]> {
    let mut points = Vec::with_capacity(1 + path.segs.len() * CURVE_FLATTEN_SEGMENTS);
    points.push(path.start);
    let mut cursor = path.start;
    for seg in &path.segs {
        match seg {
            PathSeg::Line { to } => points.push(*to),
            PathSeg::Arc { .. } | PathSeg::Bezier { .. } => {
                for i in 1..=CURVE_FLATTEN_SEGMENTS {
                    let t = i as f32 / CURVE_FLATTEN_SEGMENTS as f32;
                    points.push(seg_point_at(cursor, seg, t));
                }
            }
        }
        cursor = seg_end(cursor, seg);
    }
    points
}

/// Every point where two cables cross, one per crossing pair of chords.
///
/// Both cables are flattened by [`polyline`] and every chord of one is put
/// against every chord of the other, so a point sits within the chordal error
/// of the true curve crossing rather than exactly on it, and two cables that
/// interleave across several chords report a point for each pair.
///
/// Not counted: the pin two cables share. The chords that meet there have an
/// end in common, which [`chord_crossing`] drops, so what is left is what the
/// cables do away from it.
///
/// Nothing the widget draws needs every crossing - what a frame asks is how many
/// land in a corridor, which [`crossings_between_flattened`] answers without
/// walking the rest. This stays as the unrestricted count that answer is checked
/// against.
#[cfg(test)]
pub(crate) fn crossing_points(a: &EdgePath, b: &EdgePath) -> Vec<[f32; 2]> {
    let (pa, pb) = (polyline(a), polyline(b));
    let mut points = Vec::new();
    for s in pa.windows(2) {
        for t in pb.windows(2) {
            if let Some(p) = chord_crossing([s[0], s[1]], [t[0], t[1]]) {
                points.push(p);
            }
        }
    }
    points
}

/// The open space between two anchors a pair of cables flies together: each
/// anchor's centre, and the ring a crossing there has to clear to be out in the
/// run rather than at the wrap.
///
/// The ring is the OUTERMOST one its anchor shows, so how far a corridor starts
/// from a centre follows how many cables that anchor carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Corridor {
    pub from: Orbit,
    pub to: Orbit,
}

/// One corridor with the axis quantities every test on it needs, resolved once.
struct Band {
    from: Orbit,
    to: Orbit,
    axis: [f32; 2],
    len2: f32,
}

impl Band {
    /// `None` for a corridor between two centres in the same place, which has no
    /// axis to project onto and so holds nothing.
    fn new(corridor: &Corridor) -> Option<Self> {
        let axis = [
            corridor.to.center[0] - corridor.from.center[0],
            corridor.to.center[1] - corridor.from.center[1],
        ];
        let len2 = axis[0] * axis[0] + axis[1] * axis[1];
        (len2 >= 1e-6).then_some(Self {
            from: corridor.from,
            to: corridor.to,
            axis,
            len2,
        })
    }

    /// Where `point` falls along the axis, 0 at one centre and 1 at the other.
    fn along(&self, point: [f32; 2]) -> f32 {
        ((point[0] - self.from.center[0]) * self.axis[0]
            + (point[1] - self.from.center[1]) * self.axis[1])
            / self.len2
    }

    /// Whether a crossing at `point` counts: between the two centres, and clear
    /// of both rings. Inside a ring is the wrap itself rather than the run.
    fn holds(&self, point: [f32; 2]) -> bool {
        (0.0..=1.0).contains(&self.along(point))
            && dist2(point, self.from.center) > self.from.radius * self.from.radius
            && dist2(point, self.to.center) > self.to.radius * self.to.radius
    }

    /// Whether no point of the chord can be one this band holds.
    ///
    /// Exact, not conservative, on both counts: `along` is affine along a chord,
    /// so two ends past the same end of the axis put the whole chord there, and a
    /// disc is convex, so two ends inside a ring put the whole chord inside it.
    fn rejects(&self, chord: [[f32; 2]; 2]) -> bool {
        let (one, other) = (self.along(chord[0]), self.along(chord[1]));
        if (one < 0.0 && other < 0.0) || (one > 1.0 && other > 1.0) {
            return true;
        }
        let inside = |circle: &Orbit| {
            chord
                .iter()
                .all(|end| dist2(*end, circle.center) <= circle.radius * circle.radius)
        };
        inside(&self.from) || inside(&self.to)
    }
}

/// The axis-aligned bounds of a chord, as `[min x, min y, max x, max y]`.
fn chord_bounds(chord: [[f32; 2]; 2]) -> [f32; 4] {
    [
        chord[0][0].min(chord[1][0]),
        chord[0][1].min(chord[1][1]),
        chord[0][0].max(chord[1][0]),
        chord[0][1].max(chord[1][1]),
    ]
}

/// Whether two bounds overlap, which two crossing chords must.
fn bounds_overlap(s: [f32; 4], t: [f32; 4]) -> bool {
    s[0] <= t[2] && t[0] <= s[2] && s[1] <= t[3] && t[1] <= s[3]
}

/// How many crossings of two cables land inside `corridors`, off flattenings the
/// caller already holds.
///
/// The rule per crossing is [`Band::holds`], which is the whole of what a
/// corridor crossing is; a point held by more than one corridor counts once, so
/// this is the count over their union.
///
/// The work is cut down before any pairwise test rather than after: a chord no
/// corridor can hold a point of is dropped outright, and of what survives only
/// pairs whose bounds overlap are put against each other. Both rejects are
/// exact, so the crossings counted here are the crossings [`crossing_points`]
/// reports, filtered by the rule and nothing else.
///
/// Taking flattenings rather than paths is what lets a search measuring one
/// cable against several flatten it once: [`polyline`] is the single largest
/// cost left in this call, and a cable in `n` pairs would otherwise pay it `n`
/// times.
pub(crate) fn crossings_between_flattened(
    a: &[[f32; 2]],
    b: &[[f32; 2]],
    corridors: &[Corridor],
) -> usize {
    let bands: Vec<Band> = corridors.iter().filter_map(Band::new).collect();
    if bands.is_empty() {
        return 0;
    }
    let live = |points: &[[f32; 2]]| -> Vec<([[f32; 2]; 2], [f32; 4])> {
        points
            .windows(2)
            .map(|pair| [pair[0], pair[1]])
            .filter(|chord| bands.iter().any(|band| !band.rejects(*chord)))
            .map(|chord| (chord, chord_bounds(chord)))
            .collect()
    };
    let (one, other) = (live(a), live(b));
    let mut count = 0;
    for (s, s_bounds) in &one {
        for (t, t_bounds) in &other {
            if !bounds_overlap(*s_bounds, *t_bounds) {
                continue;
            }
            if let Some(point) = chord_crossing(*s, *t)
                && bands.iter().any(|band| band.holds(point))
            {
                count += 1;
            }
        }
    }
    count
}

/// [`crossings_between_flattened`] for a caller holding the two paths, which
/// flattens both here.
///
/// The frame flattens once per cable and calls the slice form; this is for a
/// caller measuring one pair, where there is nothing to reuse.
#[cfg(test)]
pub(crate) fn crossings_between(a: &EdgePath, b: &EdgePath, corridors: &[Corridor]) -> usize {
    crossings_between_flattened(&polyline(a), &polyline(b), corridors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::{PinRef, Station};
    use crate::node_pin::PinDirection;
    use iced_widget::core::Point;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

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

    /// `v` scaled to unit length, or `None` when it is too short to have a
    /// direction at all.
    fn unit(v: [f32; 2]) -> Option<[f32; 2]> {
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        (len > f32::EPSILON).then(|| [v[0] / len, v[1] / len])
    }

    /// Where walking `path` from its own start lands: the point a slice whose
    /// arc is trimmed only reaches if its start was set to that arc's entry.
    fn walked_end(path: &EdgePath) -> [f32; 2] {
        path.point_at(path.total_len())
    }

    /// A far pin placed so the short way round `orbit` from `pin` is `hand`.
    ///
    /// Square off the line through pin and centre, on the side the wrap is to
    /// pass: the exit tangent then sits a quarter turn from the entry one going
    /// that way and three quarters going the other.
    fn exit_pin_for(orbit: Orbit, pin: [f32; 2], hand: Hand) -> [f32; 2] {
        let u = unit([orbit.center[0] - pin[0], orbit.center[1] - pin[1]])
            .expect("the pin is not on the centre");
        let n = rot90(u);
        let s = 400.0 * hand.sign();
        [orbit.center[0] + s * n[0], orbit.center[1] + s * n[1]]
    }

    /// A leg onto a tangent runs taut: the tangent point already lies on the
    /// straight line out of the pin, so the only thing that can pull the cable
    /// off that line is the pin's own exit.
    ///
    /// Without the scaling the pin end reaches as far as a pin-to-pin curve
    /// would, whatever the angle, and the run into an anchor bulges instead of
    /// straining. The bound here is loose on purpose - it pins the shape down
    /// as a strained run rather than fixing a particular tension.
    #[test]
    fn a_leg_onto_a_tangent_hugs_its_line() {
        let pin = [0.0, 0.0];
        let orbit = Orbit {
            center: [0.0, 300.0],
            radius: 20.0,
        };
        // A pin facing right with the anchor square below it: the worst case,
        // where the cable turns a quarter circle to reach the line.
        let path = build(
            &[
                Hop::Pin {
                    point: pin,
                    side: 1,
                },
                Hop::Wrap { orbit },
                Hop::Pin {
                    point: exit_pin_for(orbit, pin, Hand::Clockwise),
                    side: 1,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;
        let Some(PathSeg::Bezier { c1, c2, to }) = path.segs.first().copied() else {
            panic!("expected a bezier entry leg, got {:?}", path.segs);
        };

        // Distance from the straight pin-to-tangent line, sampled along the leg.
        let span = [to[0] - pin[0], to[1] - pin[1]];
        let len = (span[0] * span[0] + span[1] * span[1]).sqrt();
        let worst = (0..=64)
            .map(|i| {
                let p = seg_point_at(pin, &PathSeg::Bezier { c1, c2, to }, i as f32 / 64.0);
                ((p[0] - pin[0]) * span[1] - (p[1] - pin[1]) * span[0]).abs() / len
            })
            .fold(0.0f32, f32::max);
        // A quarter-turn exit measures about 23 off its line at this reach,
        // against about 41 with the reach left unscaled. The bound sits between
        // them: it pins the shape down as a strained run without freezing a
        // particular tension.
        assert!(
            worst <= TAUT_EXIT * 0.7,
            "the leg bows {worst} off its line, too far for the {TAUT_EXIT} reach \
             it is allowed",
        );
        // Still a curve, not a corner: it does leave the pin square.
        assert!(
            worst > 1.0,
            "the leg left the pin along the line instead of square to it",
        );
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
        )
        .path;

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
        )
        .path;
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
                Hop::Wrap { orbit: ORBIT },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;

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

    /// A leg arrives straight down the tangent it is about to wrap.
    ///
    /// The touch point read from the pin is the tangent of a line the cable
    /// never travels: it leaves square to the node first. Aiming at that point
    /// curls the last stretch the other way, so leg and arc meet in a reversal
    /// that is tangent-continuous and still reads as a kink.
    ///
    /// Read from the line the cable actually runs in - and walked to its fixed
    /// point - the leg's last two control spans lie ALONG the tangent. That is
    /// the property worth pinning: it makes the arrival curvature exactly zero,
    /// which is both why the kink goes and why the cable cannot cut inside the
    /// circle on its way in.
    #[test]
    fn a_leg_arrives_straight_down_its_tangent() {
        let cases = [
            ([0.0f32, 0.0], [360.0f32, 147.0], 16.0f32),
            // Square to the pin, and close enough that the reach keeps moving
            // with the touch point - the slowest case to settle.
            ([0.0, 0.0], [0.0, 300.0], 20.0),
            ([0.0, 0.0], [70.0, 45.0], 16.0),
        ];
        for (pin, center, radius) in cases {
            let orbit = Orbit { center, radius };
            for hand in [Hand::Clockwise, Hand::CounterClockwise] {
                let path = build(
                    &[
                        Hop::Pin {
                            point: pin,
                            side: 1,
                        },
                        Hop::Wrap { orbit },
                        Hop::Pin {
                            point: exit_pin_for(orbit, pin, hand),
                            side: 1,
                        },
                    ],
                    &EdgeCurve::BezierCubic,
                )
                .path;
                let Some(PathSeg::Bezier { c1, c2, to }) = path.segs.first().copied() else {
                    panic!("expected a bezier entry leg, got {:?}", path.segs);
                };
                // The layout was built for this hand, so the derivation has to
                // agree - otherwise the case below is testing the other side.
                let PathSeg::Arc { sweep, .. } = path.segs[1] else {
                    panic!("{center:?} {hand:?}: no wrap: {:?}", path.segs);
                };
                assert_eq!(
                    sweep > 0.0,
                    hand == Hand::Clockwise,
                    "{center:?} {hand:?}: the run derived the other way round",
                );

                let (a, b) = (
                    [to[0] - c2[0], to[1] - c2[1]],
                    [c2[0] - c1[0], c2[1] - c1[1]],
                );
                let arrival = (2.0 / 3.0) * cross(a, b) / dist(a, [0.0, 0.0]).powi(3);
                assert!(
                    arrival.abs() < 1e-4,
                    "{center:?} {hand:?}: the leg arrives bending {arrival}, not down \
                     the tangent",
                );
                // And it touches the circle rather than reaching through it.
                assert!(
                    (dist(to, center) - radius).abs() < 1e-2,
                    "{center:?} {hand:?}: the leg ends off the ring",
                );
            }
        }
    }

    /// The bug the compass model had: a leg that meets the circle radially
    /// leaves a 90 degree kink. The incoming control point must lie along the
    /// wrap direction, so the leg and the arc share a tangent.
    #[test]
    fn the_leg_meets_the_arc_without_a_kink() {
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap { orbit: ORBIT },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;

        let PathSeg::Bezier { c2, to, .. } = path.segs[0] else {
            unreachable!()
        };
        let PathSeg::Arc { sweep, .. } = path.segs[1] else {
            unreachable!()
        };
        let hand = if sweep > 0.0 {
            Hand::Clockwise
        } else {
            Hand::CounterClockwise
        };
        // Direction the bezier arrives with, and the arc's tangent AT THAT
        // POINT, must be parallel and point the same way. The arc's tangent is
        // read off the arc - taking it from the attachment construction instead
        // would only restate how the touch point was picked.
        let arrival = [to[0] - c2[0], to[1] - c2[1]];
        let theta = (to[1] - ORBIT.center[1]).atan2(to[0] - ORBIT.center[0]);
        let expected = travel_dir(theta, hand);
        assert!(
            cross(arrival, expected).abs() / dist(to, c2) < 1e-2,
            "leg arrives at {arrival:?}, arc leaves along {expected:?}",
        );
        assert!(dot(arrival, expected) > 0.0, "leg arrives against the wrap");
    }

    /// Of the two ways round, the cable takes the one that sweeps less.
    ///
    /// Nothing stores a direction, so this is the whole of "no cable wraps
    /// almost all the way round an anchor": the long way is never emitted, and
    /// mirroring the far pin mirrors the wrap.
    #[test]
    fn a_wrap_takes_the_short_way_round() {
        let head = [0.0, 200.0];
        let mut sweeps = Vec::new();
        for exit in [[200.0f32, 500.0f32], [200.0, -100.0]] {
            let path = build(
                &[
                    Hop::Pin {
                        point: head,
                        side: 1,
                    },
                    Hop::Wrap { orbit: ORBIT },
                    Hop::Pin {
                        point: exit,
                        side: 2,
                    },
                ],
                &EdgeCurve::BezierCubic,
            )
            .path;
            let PathSeg::Arc { sweep, .. } = path.segs[1] else {
                panic!("{exit:?}: no wrap: {:?}", path.segs);
            };
            assert!(
                sweep.abs() <= PI + 1e-3,
                "{exit:?}: the wrap sweeps {sweep}, the long way round",
            );
            sweeps.push(sweep);
        }
        assert!(
            sweeps[0] * sweeps[1] < 0.0,
            "mirrored exits wrap the same way: {sweeps:?}",
        );
    }

    /// The chords a path flattens to, as ordered pairs.
    fn chords(path: &EdgePath) -> Vec<[[f32; 2]; 2]> {
        polyline(path).windows(2).map(|w| [w[0], w[1]]).collect()
    }

    /// Whether any chord of `a` crosses any chord of `b`.
    fn chords_cross(a: &[[[f32; 2]; 2]], b: &[[[f32; 2]; 2]]) -> bool {
        a.iter()
            .any(|s| b.iter().any(|t| chord_crossing(*s, *t).is_some()))
    }

    /// Whether two cables cross anywhere along their length.
    fn cables_cross(a: &EdgePath, b: &EdgePath) -> bool {
        !crossing_points(a, b).is_empty()
    }

    /// A cable laid straight between two pins, so its flattening is the two
    /// pins themselves.
    fn straight_cable(from: [f32; 2], to: [f32; 2]) -> EdgePath {
        build(
            &[
                Hop::Pin {
                    point: from,
                    side: 1,
                },
                Hop::Pin { point: to, side: 0 },
            ],
            &EdgeCurve::Line,
        )
        .path
    }

    /// Two straight cables meet once, where their two lines cross.
    ///
    /// A straight cable is ONE chord, so nothing here is approximated: the only
    /// error is in the crossing parameter, a ratio of two signed areas good to
    /// a part in 1e-7, which over a 100-unit chord is 1e-5 units. The bound is
    /// two orders above that, and the crossing in fact reads (50, 50) exactly.
    #[test]
    fn two_straight_cables_cross_where_their_lines_meet() {
        let a = straight_cable([0.0, 0.0], [100.0, 100.0]);
        let b = straight_cable([0.0, 100.0], [100.0, 0.0]);
        let points = crossing_points(&a, &b);
        assert_eq!(points.len(), 1, "one crossing, read as {points:?}");
        let off = dist2(points[0], [50.0, 50.0]).sqrt();
        assert!(
            off < 1e-3,
            "the crossing reads {:?}, {off} off (50, 50)",
            points[0],
        );
    }

    /// Cables held apart the whole way never cross, however finely they are
    /// flattened: not two parallel lines, and not two legs that keep their
    /// offset across the bow.
    #[test]
    fn cables_held_apart_do_not_cross() {
        let a = straight_cable([0.0, 0.0], [200.0, 0.0]);
        let b = straight_cable([0.0, 30.0], [200.0, 30.0]);
        let parallel = crossing_points(&a, &b);
        assert!(parallel.is_empty(), "parallel lines cross at {parallel:?}");
        let leg = |y: f32| {
            build(
                &[
                    Hop::Pin {
                        point: [0.0, y],
                        side: 1,
                    },
                    Hop::Pin {
                        point: [200.0, y + 40.0],
                        side: 0,
                    },
                ],
                &EdgeCurve::BezierCubic,
            )
            .path
        };
        let offset = crossing_points(&leg(0.0), &leg(60.0));
        assert!(offset.is_empty(), "offset legs cross at {offset:?}");
    }

    /// A pin two cables share is not a crossing - neither for two cables
    /// leaving it, nor for one arriving where the other leaves.
    ///
    /// Two cables OUT of one pin cannot report it whatever they do next: the
    /// chords that meet there each run off to one side, and the straddle test
    /// would need the single signed area between them to be positive and
    /// negative at once. A cable ARRIVING there is the case the shared-end drop
    /// is for - the two determinants that vanish are then on opposite chords,
    /// which the straddle test can satisfy.
    #[test]
    fn cables_sharing_a_pin_do_not_cross_at_it() {
        let pin = [0.0f32, 0.0];
        let leaving = |target: [f32; 2]| {
            build(
                &[
                    Hop::Pin {
                        point: pin,
                        side: 1,
                    },
                    Hop::Pin {
                        point: target,
                        side: 0,
                    },
                ],
                &EdgeCurve::BezierCubic,
            )
            .path
        };
        let down = leaving([220.0, -160.0]);
        let diverging = crossing_points(&down, &leaving([220.0, 160.0]));
        assert!(
            diverging.is_empty(),
            "two cables out of one pin cross at {diverging:?}",
        );
        let arriving = build(
            &[
                Hop::Pin {
                    point: [-260.0, 200.0],
                    side: 1,
                },
                Hop::Pin {
                    point: pin,
                    side: 0,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;
        let met = crossing_points(&down, &arriving);
        assert!(
            met.is_empty(),
            "the cable arriving at the pin crosses the one leaving it at {met:?}",
        );
    }

    /// A cable that wraps an anchor on the far side of a straight one crosses
    /// it on the way out and on the way back, and both crossings are reported.
    #[test]
    fn a_cable_crossing_another_twice_reports_both() {
        let line = straight_cable([0.0, 0.0], [400.0, 0.0]);
        let over = build(
            &[
                Hop::Pin {
                    point: [50.0, 60.0],
                    side: 1,
                },
                Hop::Wrap {
                    orbit: Orbit {
                        center: [200.0, -80.0],
                        radius: 40.0,
                    },
                },
                Hop::Pin {
                    point: [350.0, 60.0],
                    side: 0,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;
        let points = crossing_points(&line, &over);
        assert_eq!(points.len(), 2, "two crossings, read as {points:?}");
        assert!(
            dist2(points[0], points[1]).sqrt() > 100.0,
            "the two crossings are one place read twice: {points:?}",
        );
        for p in points {
            assert!(
                line.distance(p) < 1e-3 && over.distance(p) < 1e-2,
                "{p:?} sits {} off the straight cable and {} off the wrapping one",
                line.distance(p),
                over.distance(p),
            );
        }
    }

    /// A crossing on the wrap itself, held to what the flattening allows.
    ///
    /// The probe runs radially out of the anchor centre through the MIDDLE of
    /// one chord of the flattened wrap - mid-sweep is a sample point, so half a
    /// step past it is not. A chord of a [`CURVE_FLATTEN_SEGMENTS`]-way split
    /// of the sweep falls short of the circle by its sagitta,
    /// `r * (1 - cos(step / 2))`, and radially that is the whole error: 0.02
    /// units on this 40-unit ring, and always inward, because the chords of an
    /// arc are inscribed.
    #[test]
    fn a_crossing_on_the_wrap_lands_inside_the_ring_by_the_chord_sagitta() {
        let (path, entry, sweep) = wrapped_cable();
        let step = sweep / CURVE_FLATTEN_SEGMENTS as f32;
        let angle = arc_start_angle(entry, ORBIT.center) + sweep / 2.0 + step / 2.0;
        let probe = straight_cable(ORBIT.center, polar(angle, 2.0 * ORBIT.radius));
        let points = crossing_points(&path, &probe);
        assert_eq!(
            points.len(),
            1,
            "one crossing on the wrap, read as {points:?}"
        );
        let sagitta = ORBIT.radius * (1.0 - (step / 2.0).cos());
        let off = dist2(points[0], polar(angle, ORBIT.radius)).sqrt();
        assert!(
            off <= sagitta + 1e-4,
            "the crossing reads {off} off the ring, past the {sagitta} a chord falls short",
        );
        let radius = dist2(points[0], ORBIT.center).sqrt();
        assert!(
            radius < ORBIT.radius,
            "the crossing reads {radius} out, not inside the {} ring",
            ORBIT.radius,
        );
    }

    /// Whether the two cables cross when the first takes the inner ring, for
    /// pins placed on a circle around the anchor at these bearings.
    fn crosses_with_inner(inner: (f32, f32), outer: (f32, f32)) -> bool {
        const REACH: f32 = 300.0;
        let cable = |ends: (f32, f32), radius: f32| {
            let at = |angle: f32| [REACH * angle.cos(), REACH * angle.sin()];
            build(
                &[
                    Hop::Pin {
                        point: at(ends.0),
                        side: 1,
                    },
                    Hop::Wrap {
                        orbit: Orbit {
                            center: [0.0, 0.0],
                            radius,
                        },
                    },
                    Hop::Pin {
                        point: at(ends.1),
                        side: 3,
                    },
                ],
                &EdgeCurve::BezierCubic,
            )
            .path
        };
        cables_cross(&cable(inner, 16.0), &cable(outer, 26.0))
    }

    /// Ordering the wraps by span never makes a layout worse, and fixes a real
    /// share of them.
    ///
    /// The rule is a heuristic over four free bearings, so it is measured
    /// rather than argued. The measurement it has to pass is one-sided: there
    /// must be NO layout where the derived nesting crosses and the opposite one
    /// does not, because a rule that trades one user's crossing for another's
    /// buys nothing. The sweep also counts how many it repairs, so the rule has
    /// to keep earning its place if the geometry around it changes.
    #[test]
    fn ordering_wraps_by_span_never_makes_a_layout_worse() {
        let short_way = |a: f32, b: f32| {
            let d = (b - a).rem_euclid(TAU);
            d.min(TAU - d)
        };
        let mut narrower_inside = 0usize;
        let mut wider_inside = 0usize;
        let mut worse = Vec::new();
        let mut repaired = 0usize;
        let mut cases = 0usize;
        let step = TAU / 12.0;
        for a0 in 0..12 {
            for a1 in 0..12 {
                for b0 in 0..12 {
                    for b1 in 0..12 {
                        let a = (a0 as f32 * step, a1 as f32 * step);
                        let b = (b0 as f32 * step, b1 as f32 * step);
                        // A cable whose two pins coincide is not a layout any
                        // graph produces, and its span is zero by construction.
                        if a0 == a1 || b0 == b1 {
                            continue;
                        }
                        let (sa, sb) = (short_way(a.0, a.1), short_way(b.0, b.1));
                        // Equal spans are the tie the derivation breaks by edge
                        // index, so there is no geometric preference to test.
                        if (sa - sb).abs() < 1e-4 {
                            continue;
                        }
                        cases += 1;
                        let (narrow, wide) = if sa < sb { (a, b) } else { (b, a) };
                        let good = crosses_with_inner(narrow, wide);
                        let bad = crosses_with_inner(wide, narrow);
                        narrower_inside += usize::from(good);
                        wider_inside += usize::from(bad);
                        if good && !bad {
                            worse.push((narrow, wide));
                        }
                        repaired += usize::from(bad && !good);
                    }
                }
            }
        }
        assert!(cases > 10_000, "the sweep collapsed to {cases} layouts");
        assert!(
            worse.is_empty(),
            "{} of {cases} layouts are made WORSE by the rule, e.g. {:?}",
            worse.len(),
            worse.first(),
        );
        // Both nestings cross in most of this sweep: pins are placed on a
        // circle around the anchor with fixed pin sides, so the bezier legs
        // swing wide and interleave far more often than a real graph's would.
        // What the rule is judged on is the one-sided comparison above.
        assert!(
            repaired > 1_000,
            "the rule only repairs {repaired} of {cases} layouts \
             ({narrower_inside} crossings against {wider_inside}): \
             not worth deriving",
        );
    }

    /// The inner of the two rings a corridor layout nests its cables on -
    /// orbit 0 of either anchor.
    const CORRIDOR_INNER: f32 = 16.0;

    /// The outer of those two rings - orbit 1 of either anchor.
    const CORRIDOR_OUTER: f32 = 26.0;

    /// How far short of each anchor centre the corridor window stops, in world
    /// pixels.
    ///
    /// An arc never leaves its own circle, so keeping [`CORRIDOR_OUTER`] of
    /// axial reach either side of a centre out of the window excludes both
    /// wraps; the rest is room for the leg that arrives tangentially beside a
    /// ring.
    const CORRIDOR_CLEARANCE: f32 = CORRIDOR_OUTER + 30.0;

    /// A cable running pin, anchor `a`, anchor `b`, pin: the shortest hop chain
    /// with a corridor in it.
    ///
    /// The middle leg is the belt common to both rings, so with the anchors a
    /// few hundred units apart it is a long stretch of open cable - the space
    /// two cables sharing the corridor have to cross in. Each anchor's radius
    /// comes from the caller, because that radial order is what a nesting is.
    fn corridor_cable(pins: ([f32; 2], [f32; 2]), a: Orbit, b: Orbit) -> EdgePath {
        build(
            &[
                Hop::Pin {
                    point: pins.0,
                    side: 1,
                },
                Hop::Wrap { orbit: a },
                Hop::Wrap { orbit: b },
                Hop::Pin {
                    point: pins.1,
                    side: 3,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path
    }

    /// Which way [`corridor_cable`] wrapped each of its two anchors, as the
    /// sign of the arc laid there.
    ///
    /// The chain is leg, arc, belt, arc, leg, so the two arcs are segments 1
    /// and 3. A wrap whose geometry did not resolve contributes neither a leg
    /// nor an arc, which shortens the chain and misses the lookup - so `None`
    /// is also the answer for a cable that never made it through both.
    fn corridor_hands(path: &EdgePath) -> Option<(bool, bool)> {
        let sign = |i: usize| match path.segs.get(i) {
            Some(PathSeg::Arc { sweep, .. }) => Some(*sweep > 0.0),
            _ => None,
        };
        Some((sign(1)?, sign(3)?))
    }

    /// The chords of `path` inside the open corridor between the anchor centres
    /// `a` and `b`: both ends project onto the a-to-b axis at least
    /// [`CORRIDOR_CLEARANCE`] inside either centre.
    ///
    /// The clip is what makes "crosses between the anchors" a measurement. Two
    /// cables sharing a corridor share both rings and both pin neighbourhoods
    /// as well, and they meet there for reasons no radial order can touch, so
    /// an unclipped count says nothing about the corridor itself.
    fn corridor_chords(path: &EdgePath, a: [f32; 2], b: [f32; 2]) -> Vec<[[f32; 2]; 2]> {
        let axis = [b[0] - a[0], b[1] - a[1]];
        let len2 = axis[0] * axis[0] + axis[1] * axis[1];
        let margin = CORRIDOR_CLEARANCE / len2.sqrt();
        let inside = |p: [f32; 2]| {
            let along = ((p[0] - a[0]) * axis[0] + (p[1] - a[1]) * axis[1]) / len2;
            (margin..=1.0 - margin).contains(&along)
        };
        chords(path)
            .into_iter()
            .filter(|c| inside(c[0]) && inside(c[1]))
            .collect()
    }

    /// Two cables that share a corridor and wrap both its anchors the same way
    /// stay apart between the anchors when they are nested alike, and must meet
    /// when the nesting is flipped.
    ///
    /// The belt between two rings is the tangent common to both, and wrapped
    /// the same hand at each end it sits off the line of centres by the radius
    /// it is wrapped at. Nested alike, the two belts hold 16 and 26 off that
    /// line the whole way - parallel, and apart. Flip the nesting and one belt
    /// runs 16 to 26 while the other runs 26 to 16 across it, so they have to
    /// meet: here at (179, -21), halfway along a 360-unit corridor.
    ///
    /// Both layouts cross SOMEWHERE - the two cables leave pins 150 units apart
    /// and their entry legs interleave at (-42, 17), short of the first anchor -
    /// which is why the corridor count is clipped and why [`cables_cross`] over
    /// the whole length cannot answer this.
    ///
    /// Alike is the answer for the SAME hand at both ends only. Wrap the two
    /// ends opposite ways and the belt is the crossed tangent, which changes
    /// sides between the anchors, so the comparison inverts and nesting alike
    /// becomes the arrangement that meets. That is why the assignment counts a
    /// candidate's crossings instead of deriving them from radial order: the
    /// hand is picked by the geometry from the radii, so no rule over ring
    /// numbers alone can be right for both families.
    #[test]
    fn two_cables_nested_alike_do_not_cross_between_the_anchors() {
        let (a, b) = ([0.0f32, 0.0], [360.0f32, 0.0]);
        let orbit = |center: [f32; 2], radius: f32| Orbit { center, radius };
        // Both cables enter from the left and leave to the right, and both pins
        // of each sit on the same side of the line of centres, so each wraps
        // both anchors the same hand and its belt is an outer tangent.
        let near = ([-260.0f32, 150.0], [620.0f32, 150.0]);
        let far = ([-240.0f32, 300.0], [600.0f32, 300.0]);
        for (nesting, radii, expected) in [
            (
                "alike",
                [
                    CORRIDOR_INNER,
                    CORRIDOR_INNER,
                    CORRIDOR_OUTER,
                    CORRIDOR_OUTER,
                ],
                false,
            ),
            (
                "flipped",
                [
                    CORRIDOR_INNER,
                    CORRIDOR_OUTER,
                    CORRIDOR_OUTER,
                    CORRIDOR_INNER,
                ],
                true,
            ),
        ] {
            let u = corridor_cable(near, orbit(a, radii[0]), orbit(b, radii[1]));
            let v = corridor_cable(far, orbit(a, radii[2]), orbit(b, radii[3]));
            assert_eq!(
                (corridor_hands(&u), corridor_hands(&v)),
                (Some((true, true)), Some((true, true))),
                "{nesting}: the layout no longer wraps both anchors one way, \
                 so its belts are not the outer tangents this measures",
            );
            assert!(
                cables_cross(&u, &v),
                "{nesting}: the cables no longer meet outside the corridor, \
                 so the clip has nothing left to separate",
            );
            let crossed = chords_cross(&corridor_chords(&u, a, b), &corridor_chords(&v, a, b));
            assert_eq!(
                crossed, expected,
                "{nesting}: crossing between the anchors reads {crossed}",
            );
        }
    }

    /// An anchor the cable has to double back to still gets a SHORT wrap, and
    /// keeps it.
    ///
    /// Moving nodes reverses a run without touching the route, so an anchor can
    /// end up off the END of one: the cable leaves the pin, travels out to the
    /// ring and comes back the way it came. Entry and exit then sit on nearly
    /// the same spot, and a sweep normalised into `[0, TAU)` reads an entry a
    /// hair PAST the exit as a full lap rather than as the hook it is. The
    /// second case is one: its aim points sweep -0.34 rad, and taking that
    /// hand realizes -6.28.
    #[test]
    fn a_wrap_the_cable_doubles_back_to_stays_short() {
        let cases = [
            // The run heads LEFT and down while the ring sits far to the RIGHT
            // of both pins, so both legs reach it from the same side.
            (
                Orbit {
                    center: [400.0, 100.0],
                    radius: 20.0,
                },
                [-200.0f32, 200.0f32],
            ),
            // A tight ring just off the run, close enough that the entry the
            // leg realizes and the entry the aim points name fall either side
            // of the exit.
            (
                Orbit {
                    center: [-25.0, -125.0],
                    radius: 29.0,
                },
                [-50.0, -300.0],
            ),
        ];
        for (orbit, far) in cases {
            let built = build(
                &[
                    Hop::Pin {
                        point: [0.0, 0.0],
                        side: 1,
                    },
                    Hop::Wrap { orbit },
                    Hop::Pin {
                        point: far,
                        side: 0,
                    },
                ],
                &EdgeCurve::BezierCubic,
            );
            let sweeps: Vec<f32> = built
                .path
                .segs
                .iter()
                .filter_map(|seg| match *seg {
                    PathSeg::Arc { sweep, .. } => Some(sweep),
                    _ => None,
                })
                .collect();
            assert_eq!(
                sweeps.len(),
                1,
                "{orbit:?} -> {far:?}: the wrap was dropped, so the cable \
                 ignores an anchor its route names: {:?}",
                built.path.segs,
            );
            assert!(
                sweeps[0].abs() < PI,
                "{orbit:?} -> {far:?}: the cable takes {} rad round the ring; \
                 doubling back should hook the near side, not lap it",
                sweeps[0],
            );
        }
    }

    /// How far past the aim points' own ideal a realized sweep may run, in
    /// radians.
    ///
    /// [`tangent_from_the_run`] reads the entry off the line the leg runs in,
    /// not off the station it left, so the arc it hands back is not the arc the
    /// aim points name. Half a radian is loose room around that move, not a
    /// bound the geometry gives.
    const AIM_DRIFT: f32 = 0.5;

    /// No layout takes the long way round.
    ///
    /// The hand is not stored anywhere, so this is the property the whole
    /// derivation rests on, and the layouts that break it are not the obvious
    /// ones: they are where the realized entry and the exit land within a
    /// whisker of each other, and a sweep normalised into `[0, TAU)` reads a
    /// hair of overshoot as a lap of the ring.
    ///
    /// Half a turn is the ceiling wherever there is a short way round at all.
    /// Not every layout has one: a run that doubles back onto a ring close
    /// enough that both tangent pairs cover most of the circle has to wrap more
    /// than half of it whichever way it goes, and there the ceiling is what the
    /// ring's own tangents ask for. The grid walks the ring across, along and
    /// past the run at four radii and swings the far pin all the way round it,
    /// which lands in both regions from either side.
    #[test]
    fn no_wrap_takes_the_long_way_round() {
        let pin = [0.0, 0.0];
        // The shorter way round read off the pins themselves: exact tangents,
        // no leg to walk, so it stands apart from what the builder realizes.
        let aimed = |orbit: Orbit, far: [f32; 2]| {
            [Hand::Clockwise, Hand::CounterClockwise]
                .into_iter()
                .filter_map(|hand| {
                    let entry = orbit.attachment(pin, hand)?;
                    let exit = orbit.attachment(far, hand.flip())?;
                    Some(orbit.sweep(entry.point, exit.point, hand).abs())
                })
                .min_by(f32::total_cmp)
        };
        let mut wraps = 0usize;
        let mut over: Vec<(Orbit, [f32; 2], f32, f32)> = Vec::new();
        for radius in [11.0f32, 17.0, 23.0, 29.0] {
            for cx in (-300..=500).step_by(100) {
                for cy in (-300..=500).step_by(100) {
                    for fx in (-400..=600).step_by(250) {
                        for fy in (-400..=600).step_by(250) {
                            let orbit = Orbit {
                                center: [cx as f32, cy as f32],
                                radius,
                            };
                            let far = [fx as f32, fy as f32];
                            let built = build(
                                &[
                                    Hop::Pin {
                                        point: pin,
                                        side: 1,
                                    },
                                    Hop::Wrap { orbit },
                                    Hop::Pin {
                                        point: far,
                                        side: 0,
                                    },
                                ],
                                &EdgeCurve::BezierCubic,
                            );
                            // A ring swallowing either pin has no tangent to
                            // wrap and is dropped, so there is no arc to judge.
                            let Some(sweep) = built.path.segs.iter().find_map(|seg| match *seg {
                                PathSeg::Arc { sweep, .. } => Some(sweep),
                                _ => None,
                            }) else {
                                continue;
                            };
                            wraps += 1;
                            let ideal = aimed(orbit, far).expect("the wrap resolved a tangent");
                            let ceiling = PI.max(ideal + AIM_DRIFT);
                            if sweep.abs() > ceiling {
                                over.push((orbit, far, sweep, ideal));
                            }
                        }
                    }
                }
            }
        }
        assert!(
            wraps > 5_000,
            "the grid wrapped only {wraps} of its layouts, so it says little",
        );
        assert!(
            over.is_empty(),
            "{} of {wraps} layouts wrap further than the ring asks for, worst \
             {:?}",
            over.len(),
            over.iter().max_by(|a, b| a.2.abs().total_cmp(&b.2.abs())),
        );
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

    /// A cable chaining two anchors: the straight run between them must be
    /// tangent to BOTH rings, or it visibly clips one of them - and it must be
    /// STRAIGHT, because the line tangent to two rings is the one line touching
    /// both, so a run that bows is tangent to neither ring where it bows.
    ///
    /// Three layouts. Wrapping both rings the same way and wrapping them
    /// opposite ways are different lines - the outer belt and the crossed one -
    /// and the far pin is what decides which. The third is a pair whose aim
    /// points name one hand for the far ring while the run realizes the other,
    /// which is the case where an exit committed against the aim rather than
    /// against the hand the ring takes would leave the run off its own chord.
    #[test]
    fn the_run_between_two_orbits_is_tangent_to_both() {
        let mut wraps = Vec::new();
        let cases = [
            (
                [0.0f32, 200.0f32],
                ORBIT,
                Orbit {
                    center: [520.0, 260.0],
                    radius: 25.0,
                },
                [700.0f32, 500.0f32],
            ),
            (
                [0.0, 200.0],
                ORBIT,
                Orbit {
                    center: [520.0, 260.0],
                    radius: 25.0,
                },
                [700.0, 20.0],
            ),
            (
                [-300.0, -200.0],
                Orbit {
                    center: [200.0, 200.0],
                    radius: 16.0,
                },
                Orbit {
                    center: [300.0, 100.0],
                    radius: 26.0,
                },
                [500.0, -300.0],
            ),
        ];
        for (head, a, b, tail) in cases {
            let path = build(
                &[
                    Hop::Pin {
                        point: head,
                        side: 1,
                    },
                    Hop::Wrap { orbit: a },
                    Hop::Wrap { orbit: b },
                    Hop::Pin {
                        point: tail,
                        side: 0,
                    },
                ],
                &EdgeCurve::BezierCubic,
            )
            .path;
            // leg, wrap, run, wrap, leg
            assert_eq!(path.segs.len(), 5, "{tail:?}: {:?}", path.segs);

            // Where the run starts: the end of the first wrap.
            let mut cursor = path.start;
            for seg in &path.segs[..2] {
                cursor = seg_end(cursor, seg);
            }
            let PathSeg::Bezier { c1, c2, to } = path.segs[2] else {
                panic!("{tail:?}: the run is not a leg: {:?}", path.segs[2]);
            };

            for (label, on, ring, control) in [("leaves", cursor, a, c1), ("lands", to, b, c2)] {
                assert!(
                    (dist(on, ring.center) - ring.radius).abs() < 1e-2,
                    "{tail:?}: the run {label} off its ring",
                );
                // Tangent means the run's direction there is perpendicular to
                // the radius; both control points lie along that direction.
                let along = [control[0] - on[0], control[1] - on[1]];
                let radial = [on[0] - ring.center[0], on[1] - ring.center[1]];
                assert!(
                    dot(along, radial).abs() / (dist(control, on) * ring.radius) < 1e-2,
                    "{tail:?}: the run is not tangent where it {label}",
                );
            }

            // Straight: the control points of a run tangent to two rings sit on
            // the run's own chord, so the leg between the rings IS that line.
            let chord = [to[0] - cursor[0], to[1] - cursor[1]];
            let span = dist(cursor, to);
            let off = |p: [f32; 2]| {
                ((p[0] - cursor[0]) * chord[1] - (p[1] - cursor[1]) * chord[0]).abs() / span
            };
            assert!(
                off(c1).max(off(c2)) < 1e-2,
                "{tail:?}: the run leaves its own chord by {}",
                off(c1).max(off(c2)),
            );

            let (PathSeg::Arc { sweep: first, .. }, PathSeg::Arc { sweep: second, .. }) =
                (path.segs[1], path.segs[3])
            else {
                panic!("{tail:?}: the wraps are not arcs: {:?}", path.segs);
            };
            wraps.push((first > 0.0, second > 0.0));
        }
        assert_ne!(
            wraps[0], wraps[1],
            "both layouts take the same belt, so the crossed one is untested",
        );
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
                Hop::Wrap { orbit: ORBIT },
                Hop::Pin {
                    point: far,
                    side: 0,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;
        assert_eq!(path.segs.len(), 1);
        assert!(matches!(path.segs[0], PathSeg::Bezier { to, .. } if to == far));
    }

    /// The leg-arc-leg cable of [`closed_chain_is_leg_arc_leg`], plus the point
    /// its arc starts at and the arc's signed sweep - everything a query test
    /// needs to name a spot on the wrap.
    fn wrapped_cable() -> (EdgePath, [f32; 2], f32) {
        let built = wrapped_built();
        let PathSeg::Bezier { to: entry, .. } = built.path.segs[0] else {
            panic!("first segment is not a leg: {:?}", built.path.segs[0]);
        };
        let PathSeg::Arc { sweep, .. } = built.path.segs[1] else {
            panic!("middle segment is not an arc: {:?}", built.path.segs[1]);
        };
        (built.path, entry, sweep)
    }

    /// The same cable as [`wrapped_cable`], with the ring touches it recorded.
    fn wrapped_built() -> Built {
        build(
            &[
                Hop::Pin {
                    point: [0.0, 200.0],
                    side: 1,
                },
                Hop::Wrap { orbit: ORBIT },
                Hop::Pin {
                    point: [200.0, 500.0],
                    side: 2,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
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
        .path
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

    /// A window is exactly the stretch of cable between its two arc lengths:
    /// it starts and ends where the cable does at those lengths, and it is as
    /// long as the window is wide.
    #[test]
    fn a_slice_spans_its_arc_length_window() {
        let path = line_cable();
        let total = path.total_len();
        assert!((total - 100.0).abs() < 1e-3, "the cable reads {total} long");

        let cut = path.slice(20.0, 70.0);
        assert!(
            dist(cut.start, path.point_at(20.0)) < 1e-3,
            "the slice starts at {:?}, 20 along is {:?}",
            cut.start,
            path.point_at(20.0),
        );
        assert!(
            dist(walked_end(&cut), path.point_at(70.0)) < 1e-3,
            "the slice ends at {:?}, 70 along is {:?}",
            walked_end(&cut),
            path.point_at(70.0),
        );
        assert!(
            (cut.total_len() - 50.0).abs() < 1e-3,
            "a 50 wide window sliced {} of cable",
            cut.total_len(),
        );
    }

    /// A window inside a wrap keeps just the sweep it covers - and because an
    /// arc reads its start angle from wherever the path left off, the slice's
    /// own start has to BE the arc point at the window's near edge, or its end
    /// lands somewhere else entirely on the circle.
    #[test]
    fn a_slice_through_a_wrap_starts_on_the_arc() {
        let built = wrapped_built();
        let touch = built.touches[0];
        let arc_len = touch.span.1 - touch.span.0;
        let from = touch.span.0 + arc_len * 0.25;
        let to = touch.span.0 + arc_len * 0.75;
        let cut = built.path.slice(from, to);

        assert_eq!(cut.segs.len(), 1, "not just the arc: {:?}", cut.segs);
        assert!(matches!(cut.segs[0], PathSeg::Arc { .. }));
        assert!(
            ORBIT.ring_distance(cut.start) < 1e-2,
            "the slice starts at {:?}, off the ring it cuts",
            cut.start,
        );
        assert!(
            (cut.total_len() - (to - from)).abs() < 1e-2,
            "a {} wide window sliced {} of arc",
            to - from,
            cut.total_len(),
        );
        assert!(
            dist(walked_end(&cut), built.path.point_at(to)) < 1e-2,
            "the slice ends at {:?}, the window ends at {:?}",
            walked_end(&cut),
            built.path.point_at(to),
        );
    }

    /// A window the cable does not cover is not a panic: inverted collapses to
    /// the point it starts at, over-long is simply the whole cable.
    #[test]
    fn a_slice_clamps_to_the_cable() {
        let path = line_cable();

        let inverted = path.slice(60.0, 20.0);
        assert!(inverted.segs.is_empty(), "{:?}", inverted.segs);
        assert!(dist(inverted.start, path.point_at(60.0)) < 1e-3);

        let over = path.slice(-50.0, 500.0);
        assert!(
            (over.total_len() - path.total_len()).abs() < 1e-3,
            "the whole cable sliced {} of {}",
            over.total_len(),
            path.total_len(),
        );
        assert!(dist(over.start, path.start) < 1e-3);
        assert!(dist(walked_end(&over), walked_end(&path)) < 1e-3);
    }

    /// Beside the middle of a straight cable the nearest point IS the middle:
    /// the lateral offset is the distance, and half the length is how far along
    /// the hit sits.
    #[test]
    fn nearest_reports_how_far_along_it_hit() {
        let path = line_cable();
        let near = path.nearest([50.0, 25.0]);
        assert!(
            (near.distance - 25.0).abs() < 1e-3,
            "25 beside the cable reads {}",
            near.distance,
        );
        assert!(
            (near.arc_len - path.total_len() / 2.0).abs() < 1e-3,
            "hit {} along a cable {} long",
            near.arc_len,
            path.total_len(),
        );
    }

    /// Arc length along a bezier leg is measured on the curve: a cubic's
    /// length is not linear in its parameter, and every query that places a
    /// press or a glow window compares against a length.
    ///
    /// One leg, 1000.1 units long by a walk far finer than the flattening the
    /// queries answer from. A probe sitting ON the curve 24 units in - the
    /// width of the end zone at zoom 1 - reads 23.98, and one 400 in reads
    /// 399.98. Either answer taken back through [`EdgePath::point_at`] lands
    /// within a quarter unit of the probe, and the 24 unit window a hover
    /// opens around a hit covers 24 units of cable.
    #[test]
    fn nearest_measures_arc_length_along_a_bezier() {
        let path = build(
            &[
                Hop::Pin {
                    point: [0.0, 0.0],
                    side: 1,
                },
                Hop::Pin {
                    point: [912.0, 406.0],
                    side: 0,
                },
            ],
            &EdgeCurve::BezierCubic,
        )
        .path;
        assert_eq!(path.segs.len(), 1, "not one leg: {:?}", path.segs);
        let PathSeg::Bezier { c1, c2, to } = path.segs[0] else {
            panic!("the leg is not a bezier: {:?}", path.segs[0]);
        };
        let control = [path.start, c1, c2, to];
        // The curve walked far finer than the queries flatten it: what the leg
        // really measures, and the point really at a given length along it.
        const FINE: usize = 8192;
        let sample = |i: usize| {
            cubic_point(
                control[0],
                control[1],
                control[2],
                control[3],
                i as f32 / FINE as f32,
            )
        };
        let length: f32 = (1..=FINE).map(|i| dist(sample(i - 1), sample(i))).sum();
        let point_at = |target: f32| -> [f32; 2] {
            let mut walked = 0.0;
            for i in 1..=FINE {
                let (prev, cur) = (sample(i - 1), sample(i));
                let chord = dist(prev, cur);
                if walked + chord >= target {
                    return lerp(prev, cur, (target - walked) / chord);
                }
                walked += chord;
            }
            control[3]
        };
        assert!(
            (length - 1000.1).abs() < 0.1,
            "the leg measures {length}, not the thousand units this case is about",
        );
        assert!(
            (path.total_len() - length).abs() < 0.2,
            "the cable reads {} long against {length}",
            path.total_len(),
        );

        for along in [24.0, 400.0, 800.0] {
            let probe = point_at(along);
            let near = path.nearest(probe);
            assert!(
                near.distance < 0.1,
                "the probe sits on the curve, and reads {} off it",
                near.distance,
            );
            assert!(
                (near.arc_len - along).abs() < 0.1,
                "a probe {along} along the leg reports {}",
                near.arc_len,
            );
            let back = path.point_at(near.arc_len);
            assert!(
                dist(back, probe) < 0.5,
                "{along} along came back at {back:?}, {} from the probe",
                dist(back, probe),
            );
            let window = path.slice(near.arc_len - 12.0, near.arc_len + 12.0);
            assert!(
                (window.total_len() - 24.0).abs() < 0.5,
                "a 24 wide window at {along} covers {} of cable",
                window.total_len(),
            );
        }
    }

    /// The query that classifies a press as a wrap grab: a probe just outside
    /// the ring at the middle of the sweep measures its radial offset, and the
    /// arc length it reports lands in the middle of that wrap's own span - which
    /// is what ties the hit back to the anchor being wrapped.
    #[test]
    fn nearest_on_a_wrap_lands_inside_its_span() {
        let built = wrapped_built();
        let touch = built.touches[0];
        let PathSeg::Arc { sweep, .. } = built.path.segs[1] else {
            panic!("middle segment is not an arc: {:?}", built.path.segs[1]);
        };
        let mid = arc_start_angle(touch.entry, ORBIT.center) + sweep / 2.0;
        let near = built.path.nearest(polar(mid, ORBIT.radius + 12.0));

        assert!(
            (near.distance - 12.0).abs() < 1e-2,
            "12 outside the ring reads {}",
            near.distance,
        );
        let middle = (touch.span.0 + touch.span.1) / 2.0;
        assert!(
            (near.arc_len - middle).abs() < 1e-2,
            "mid-sweep hit {} along, the wrap's span is {:?}",
            near.arc_len,
            touch.span,
        );
    }

    /// A touch's span is the stretch of the finished cable its arc occupies: as
    /// long as the arc itself, with the entry leg before it and the exit leg
    /// after.
    #[test]
    fn a_ring_touch_spans_its_arc() {
        let built = wrapped_built();
        let touch = built.touches[0];
        let PathSeg::Arc { radius, sweep, .. } = built.path.segs[1] else {
            panic!("middle segment is not an arc: {:?}", built.path.segs[1]);
        };

        assert_eq!(touch.hop, 1, "the touch names the wrong hop");
        let arc_len = sweep.abs() * radius;
        assert!(
            (touch.span.1 - touch.span.0 - arc_len).abs() < 1e-2,
            "the span covers {}, the arc is {arc_len} long",
            touch.span.1 - touch.span.0,
        );
        assert!(
            touch.span.0 > 0.0,
            "the span starts at the cable's own start, with no entry leg",
        );
        assert!(
            touch.span.1 < built.path.total_len(),
            "the span reaches the cable's end, with no exit leg",
        );
    }

    /// Orbit 0's radius at every anchor of a gate scene.
    const GATE_INNER: f32 = 16.0;

    /// The radial step between one anchor's rings, so an anchor carrying four
    /// cables shows rings at 16, 26, 36 and 46.
    const GATE_STEP: f32 = 10.0;

    /// Where a gate scene's anchor ids start. Anchors have their own id space,
    /// so this only has to be stable, not clear of the pins.
    const GATE_ANCHOR_ID: usize = 0;

    /// How far inside either pin an anchor centre has to project onto a cable's
    /// own run, as a fraction of it.
    const GATE_MARGIN: f32 = 0.06;

    /// The least gap between two anchors' projections onto one cable's run, so
    /// the visiting order the projection gives is not a coin toss.
    const GATE_ORDER: f32 = 0.04;

    /// A vocabulary whose node, anchor and edge ids are all `usize`, so a
    /// failure message names the piece it means by value.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct GateIds;

    impl crate::Ids for GateIds {
        type NodeId = usize;
        type PinId = usize;
        type EdgeId = usize;
        type AnchorId = usize;
        type Payload = ();
    }

    type GateGraph<'a> = crate::node_graph::NodeGraph<
        'a,
        GateIds,
        (),
        iced_widget::core::Theme,
        iced_widget::renderer::Renderer,
    >;

    /// A linear congruential generator, so a whole scene is named by one integer
    /// and rebuilt from it.
    ///
    /// The multiplier is Knuth's MMIX one and the output is the top 31 bits,
    /// where the low bits' short periods do not reach.
    struct Lcg(u64);

    impl Lcg {
        fn seeded(seed: u64) -> Self {
            let mut lcg = Self(seed);
            lcg.bits();
            lcg
        }

        fn bits(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as u32
        }

        /// A value in `[lo, hi)`.
        fn range(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * (self.bits() as f32 / (1u64 << 31) as f32)
        }
    }

    /// One cable's two pins, output first.
    #[derive(Debug, Clone, Copy)]
    struct Ends {
        head: [f32; 2],
        head_side: u32,
        tail: [f32; 2],
        tail_side: u32,
    }

    /// A layout the gate measures: the anchors every cable wraps, and the pins
    /// each cable runs between.
    #[derive(Debug)]
    struct Scene {
        seed: u64,
        anchors: Vec<[f32; 2]>,
        cables: Vec<Ends>,
    }

    /// The side whose outward normal points most nearly from `pin` toward
    /// `toward` - the side a cable heading that way would be wired to.
    fn facing_side(pin: [f32; 2], toward: [f32; 2]) -> u32 {
        let want = [toward[0] - pin[0], toward[1] - pin[1]];
        let score = |side: u32| {
            let d = pin_side_direction(side);
            d[0] * want[0] + d[1] * want[1]
        };
        (0..4)
            .max_by(|&a, &b| score(a).total_cmp(&score(b)))
            .unwrap_or(1)
    }

    /// A scene of `anchors` anchors with `cables` cables through ALL of them, so
    /// every pair shares every anchor and every corridor is real.
    ///
    /// `None` when some cable's anchors do not fall in route order along its own
    /// run, inside both pins: such a cable doubles back through the corridor
    /// instead of transiting it, and its legs sit where its wraps are.
    fn gate_scene(seed: u64, anchors: usize, cables: usize) -> Option<Scene> {
        let mut rng = Lcg::seeded(seed);
        let axis = rng.range(0.0, TAU);
        let (cos, sin) = (axis.cos(), axis.sin());
        let mut centers: Vec<[f32; 2]> = Vec::with_capacity(anchors);
        let mut along = 0.0;
        for _ in 0..anchors {
            let across = rng.range(-70.0, 70.0);
            centers.push([cos * along - sin * across, sin * along + cos * across]);
            along += rng.range(300.0, 460.0);
        }
        let (first, last) = (centers[0], centers[anchors - 1]);
        let mut ends = Vec::with_capacity(cables);
        for _ in 0..cables {
            // Each pin is drawn from the half turn facing away from the far end
            // of the route, which is where a cable that transits the chain has
            // to leave from - though the draw reaches far enough round it that
            // the check below still throws layouts out.
            let out = axis + PI + rng.range(-FRAC_PI_2, FRAC_PI_2);
            let back = axis + rng.range(-FRAC_PI_2, FRAC_PI_2);
            let (near, far) = (rng.range(220.0, 380.0), rng.range(220.0, 380.0));
            let head = [first[0] + near * out.cos(), first[1] + near * out.sin()];
            let tail = [last[0] + far * back.cos(), last[1] + far * back.sin()];
            ends.push(Ends {
                head,
                head_side: facing_side(head, first),
                tail,
                tail_side: facing_side(tail, last),
            });
        }
        let transits = |end: &Ends| {
            let run = [end.tail[0] - end.head[0], end.tail[1] - end.head[1]];
            let len2 = run[0] * run[0] + run[1] * run[1];
            if len2 < 1e-6 {
                return false;
            }
            let mut previous = GATE_MARGIN - GATE_ORDER;
            centers.iter().all(|c| {
                let at = ((c[0] - end.head[0]) * run[0] + (c[1] - end.head[1]) * run[1]) / len2;
                let ordered = at >= previous + GATE_ORDER && at <= 1.0 - GATE_MARGIN;
                previous = at;
                ordered
            })
        };
        if !ends.iter().all(transits) {
            return None;
        }
        Some(Scene {
            seed,
            anchors: centers,
            cables: ends,
        })
    }

    /// The corridors of a gate scene, at the reach every anchor shows: the ones
    /// production counts, then those plus the rest.
    ///
    /// Every cable wraps every anchor in visiting order, so each pair of cables
    /// shares the whole set. Production takes the corridors between CONSECUTIVE
    /// shared anchors, on the reasoning that a band spanning an anchor covers
    /// nothing the two halves either side of it miss; the second list is every
    /// pair, which is what that reasoning is checked against.
    fn gate_corridors(centers: &[[f32; 2]], reach: f32) -> (Vec<Corridor>, Vec<Corridor>) {
        let ring = |center: [f32; 2]| Orbit {
            center,
            radius: reach,
        };
        let consecutive: Vec<Corridor> = centers
            .windows(2)
            .map(|pair| Corridor {
                from: ring(pair[0]),
                to: ring(pair[1]),
            })
            .collect();
        let every: Vec<Corridor> = centers
            .iter()
            .enumerate()
            .flat_map(|(i, &from)| {
                centers[i + 1..].iter().map(move |&to| Corridor {
                    from: ring(from),
                    to: ring(to),
                })
            })
            .collect();
        (consecutive, every)
    }

    /// Crossings between two cables that land in one of `bands`, counted twice
    /// over.
    ///
    /// The first count mirrors production's classification off
    /// [`crossing_points`], with the projection and both reaches written out here
    /// rather than called, so it depends on nothing [`crossings_between`] does.
    /// The second IS [`crossings_between`], which drops chords and pairs before
    /// testing them. The two are asserted equal on every pair the sweep
    /// evaluates, tens of thousands of them, because the whole worth of the
    /// region-limited count is that it changes the work and not the answer.
    fn corridor_crossings(a: &EdgePath, b: &EdgePath, bands: &[Corridor]) -> usize {
        let holds = |point: [f32; 2]| {
            bands.iter().any(|corridor| {
                let (from, to) = (corridor.from, corridor.to);
                let axis = [to.center[0] - from.center[0], to.center[1] - from.center[1]];
                let len2 = axis[0] * axis[0] + axis[1] * axis[1];
                if len2 < 1e-6 {
                    return false;
                }
                let at = ((point[0] - from.center[0]) * axis[0]
                    + (point[1] - from.center[1]) * axis[1])
                    / len2;
                if !(0.0..=1.0).contains(&at) {
                    return false;
                }
                dist2(point, from.center) > from.radius * from.radius
                    && dist2(point, to.center) > to.radius * to.radius
            })
        };
        let filtered = crossing_points(a, b)
            .into_iter()
            .filter(|point| holds(*point))
            .count();
        let region = crossings_between(a, b, bands);
        assert_eq!(
            region, filtered,
            "the region-limited count reads {region} where filtering every \
             crossing through the corridor rule reads {filtered}",
        );
        region
    }

    /// Every arrangement of one scene's rings, built and counted on demand.
    ///
    /// A cable's geometry depends only on the ring it takes at each anchor, and a
    /// pair's crossings only on the two cables' own rings, so both are kept per
    /// ring tuple: the brute force asks for the same pair thousands of times.
    struct Arrangements {
        /// Each cable's hop chain as `edge_hops` lowered it, wraps included.
        template: Vec<Vec<Hop>>,
        /// `at[cable][anchor]` is the hop of that cable which wraps that anchor.
        at: Vec<Vec<usize>>,
        centers: Vec<[f32; 2]>,
        /// The corridors production counts, between consecutive anchors.
        bands: Vec<Corridor>,
        /// Those plus every spanning one, which the count is checked against.
        every: Vec<Corridor>,
        /// One past the highest ring tuple: `cables ^ anchors`.
        radix: usize,
        paths: Vec<Vec<Option<EdgePath>>>,
        pairs: Vec<Vec<Option<usize>>>,
    }

    impl Arrangements {
        fn new(template: Vec<Vec<Hop>>, at: Vec<Vec<usize>>, centers: Vec<[f32; 2]>) -> Self {
            let cables = template.len();
            let radix = cables.pow(centers.len() as u32);
            let reach = GATE_INNER + GATE_STEP * (cables - 1) as f32;
            let (bands, every) = gate_corridors(&centers, reach);
            Self {
                template,
                at,
                centers,
                bands,
                every,
                radix,
                paths: vec![vec![None; radix]; cables],
                pairs: vec![vec![None; radix * radix]; cables * (cables - 1) / 2],
            }
        }

        /// One cable's ring per anchor, as a single index.
        fn tuple(&self, orbits: &[u8]) -> usize {
            orbits
                .iter()
                .rev()
                .fold(0, |id, &orbit| id * self.template.len() + orbit as usize)
        }

        /// Builds one cable on one ring tuple, unless it is built already.
        fn ensure(&mut self, cable: usize, tuple: usize) {
            if self.paths[cable][tuple].is_some() {
                return;
            }
            let cables = self.template.len();
            let mut hops = self.template[cable].clone();
            let mut rest = tuple;
            for (anchor, &hop) in self.at[cable].iter().enumerate() {
                let ring = rest % cables;
                rest /= cables;
                hops[hop] = Hop::Wrap {
                    orbit: Orbit {
                        center: self.centers[anchor],
                        radius: GATE_INNER + GATE_STEP * ring as f32,
                    },
                };
            }
            self.paths[cable][tuple] = Some(build(&hops, &EdgeCurve::default()).path);
        }

        /// The corridor crossings of a whole arrangement, `orbits[cable][anchor]`.
        fn crossings(&mut self, orbits: &[Vec<u8>]) -> usize {
            let tuples: Vec<usize> = orbits.iter().map(|rings| self.tuple(rings)).collect();
            let mut total = 0;
            let mut pair = 0;
            for u in 0..tuples.len() {
                for v in u + 1..tuples.len() {
                    let key = tuples[u] * self.radix + tuples[v];
                    if self.pairs[pair][key].is_none() {
                        self.ensure(u, tuples[u]);
                        self.ensure(v, tuples[v]);
                        let count = corridor_crossings(
                            self.paths[u][tuples[u]].as_ref().expect("built just above"),
                            self.paths[v][tuples[v]].as_ref().expect("built just above"),
                            &self.bands,
                        );
                        self.pairs[pair][key] = Some(count);
                    }
                    total += self.pairs[pair][key].expect("filled just above");
                    pair += 1;
                }
            }
            total
        }

        /// How many crossings the bands SPANNING an anchor hold that the ones
        /// either side of it do not, for this arrangement.
        ///
        /// Production counts consecutive anchors only. A spanning band clears the
        /// reach of its two ends and not of the anchor between them, so it holds
        /// crossings that happen among that anchor's own rings, which both halves
        /// exclude. This is how many the narrower list gives up.
        fn spanning(&mut self, orbits: &[Vec<u8>]) -> usize {
            let tuples: Vec<usize> = orbits.iter().map(|rings| self.tuple(rings)).collect();
            let mut extra = 0;
            for u in 0..tuples.len() {
                for v in u + 1..tuples.len() {
                    self.ensure(u, tuples[u]);
                    self.ensure(v, tuples[v]);
                    let one = self.paths[u][tuples[u]].as_ref().expect("built just above");
                    let other = self.paths[v][tuples[v]].as_ref().expect("built just above");
                    extra += crossings_between(one, other, &self.every)
                        - crossings_between(one, other, &self.bands);
                }
            }
            extra
        }
    }

    /// Every ordering of `n` cables on `n` rings.
    fn permutations(n: usize) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = vec![Vec::new()];
        for value in 0..n as u8 {
            let mut grown = Vec::with_capacity(out.len() * (value as usize + 1));
            for perm in &out {
                for at in 0..=perm.len() {
                    let mut next = perm.clone();
                    next.insert(at, value);
                    grown.push(next);
                }
            }
            out = grown;
        }
        out
    }

    /// Every anchor's cables ordered by how far each wraps it, ties by edge
    /// index: the arrangement the assignment seeds with, before any candidate is
    /// measured.
    ///
    /// The key is read through production's own `wrap_span`, so what is ordered
    /// here is what the seed orders.
    fn containment_orbits(scene: &Scene) -> Vec<Vec<u8>> {
        let anchors = scene.anchors.len();
        let mut orbits = vec![vec![0u8; anchors]; scene.cables.len()];
        for (anchor, &center) in scene.anchors.iter().enumerate() {
            let mut order: Vec<(f32, usize)> = scene
                .cables
                .iter()
                .enumerate()
                .map(|(cable, ends)| {
                    let previous = if anchor == 0 {
                        ends.head
                    } else {
                        scene.anchors[anchor - 1]
                    };
                    let next = scene.anchors.get(anchor + 1).copied().unwrap_or(ends.tail);
                    (crate::node_graph::wrap_span(center, previous, next), cable)
                })
                .collect();
            order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            for (ring, &(_, cable)) in order.iter().enumerate() {
                orbits[cable][anchor] = ring as u8;
            }
        }
        orbits
    }

    /// Which classes of move still take a corridor crossing out of an
    /// arrangement, measured only where the search fell short of the best there
    /// is - an arrangement that is already best has nothing to escape from.
    ///
    /// The first two are exactly what `orbits::refine` searches. An improvement
    /// left in either means the budget cut the descent off; an improvement in
    /// neither means the search sits in a local minimum of its own
    /// neighbourhood, and the last two then say whether a wider one would help.
    #[derive(Debug, Clone, Copy, Default)]
    struct Escapes {
        /// Exchange two cables' rings at ONE anchor.
        local: bool,
        /// Exchange two cables at EVERY anchor they share.
        coupled: bool,
        /// Exchange two cables over a proper PART of their shared route, which
        /// is neither of the above.
        partial: bool,
        /// Any other ordering of one anchor's rings. Contains `local`, and adds
        /// the orderings no single exchange reaches - a three-cycle above all.
        reseated: bool,
    }

    impl Escapes {
        /// Whether the move is one `orbits::refine` would have found had it not
        /// run out of budget.
        fn within_reach(&self) -> bool {
            self.local || self.coupled
        }

        /// Whether nothing tried steps out of the arrangement at all.
        fn none(&self) -> bool {
            !self.local && !self.coupled && !self.partial && !self.reseated
        }
    }

    /// What one gate scene measured.
    #[derive(Debug, Clone, Copy)]
    struct Measured {
        /// Corridor crossings in the arrangement `edge_hops` returned.
        chosen: usize,
        /// The same count for the pure containment seed.
        containment: usize,
        /// The least any arrangement of the rings achieves.
        best: usize,
        /// What still improves on the chosen arrangement, all false where it is
        /// already the best there is.
        escapes: Escapes,
        /// Crossings in the chosen arrangement that a band spanning an anchor
        /// holds and the consecutive ones production counts do not.
        spanning: usize,
    }

    /// Drives `NodeGraph::edge_hops` over `scene` and measures the arrangement it
    /// returned against the containment seed and against every arrangement
    /// there is.
    ///
    /// `None` when the geometry dropped a wrap: that cable never reached the
    /// corridor, so there is nothing to score.
    fn measure(scene: &Scene) -> Option<Measured> {
        let anchors = scene.anchors.len();
        let cables = scene.cables.len();
        let mut graph = GateGraph::default();
        for (index, center) in scene.anchors.iter().enumerate() {
            graph = graph.push_anchor(crate::node_graph::anchor(
                GATE_ANCHOR_ID + index,
                Point::new(center[0], center[1]),
            ));
        }
        for cable in 0..cables {
            graph = graph.push_edge(
                crate::node_graph::edge(
                    cable,
                    PinRef::new(2 * cable, 0),
                    PinRef::new(2 * cable + 1, 0),
                )
                .route((0..anchors).map(|anchor| GATE_ANCHOR_ID + anchor)),
            );
        }
        let station = |pin: &PinRef<GateIds>| -> Option<Station> {
            let ends = scene.cables.get(pin.node_id / 2)?;
            Some(if pin.node_id.is_multiple_of(2) {
                Station {
                    point: ends.head,
                    side: ends.head_side,
                    direction: Some(PinDirection::Output),
                }
            } else {
                Station {
                    point: ends.tail,
                    side: ends.tail_side,
                    direction: Some(PinDirection::Input),
                }
            })
        };
        let centers = scene.anchors.clone();
        let ring = |anchor: usize, orbit: u8| -> Option<Orbit> {
            Some(Orbit {
                center: *centers.get(anchor)?,
                radius: GATE_INNER + GATE_STEP * orbit as f32,
            })
        };
        let curve = |_edge: usize| EdgeCurve::default();
        let built = graph.edge_hops(&station, &ring, &curve, None);
        assert_eq!(
            built.len(),
            cables,
            "seed {}: {} of {cables} edges lowered to a cable",
            scene.seed,
            built.len(),
        );

        let mut template = Vec::with_capacity(cables);
        let mut at = Vec::with_capacity(cables);
        let mut production = Vec::with_capacity(cables);
        let mut chosen = vec![vec![0u8; anchors]; cables];
        for (cable, geometry) in built.iter().enumerate() {
            assert_eq!(
                geometry.edge, cable,
                "seed {}: the cables came back out of edge order",
                scene.seed,
            );
            assert_eq!(
                geometry.rings.len(),
                anchors,
                "seed {}: cable {cable} wraps {} of {anchors} anchors",
                scene.seed,
                geometry.rings.len(),
            );
            let mut hops = Vec::with_capacity(anchors);
            for (visited, &(hop, (anchor, orbit))) in geometry.rings.iter().enumerate() {
                assert_eq!(
                    anchor, visited,
                    "seed {}: cable {cable} reaches anchor {anchor} in position \
                     {visited}, so its run doubles back",
                    scene.seed,
                );
                chosen[cable][anchor] = orbit;
                hops.push(hop);
            }
            let path = build(&geometry.hops, &EdgeCurve::default()).path;
            let arcs = path
                .segs
                .iter()
                .filter(|seg| matches!(seg, PathSeg::Arc { .. }))
                .count();
            // A wrap the geometry could not resolve leaves no arc, and a cable
            // that never made it round an anchor has no corridor to share.
            if arcs != anchors {
                return None;
            }
            template.push(geometry.hops.clone());
            at.push(hops);
            production.push(path);
        }

        let mut arrangements = Arrangements::new(template, at, scene.anchors.clone());
        // Re-gearing a cable onto the rings it was given has to reproduce the
        // cable production built, or every other arrangement measured here is
        // some other cable's.
        for (cable, path) in production.iter().enumerate() {
            let tuple = arrangements.tuple(&chosen[cable]);
            arrangements.ensure(cable, tuple);
            assert_eq!(
                arrangements.paths[cable][tuple].as_ref(),
                Some(path),
                "seed {}: cable {cable} re-geared onto its own rings is not the \
                 cable production built",
                scene.seed,
            );
        }

        let chosen_count = arrangements.crossings(&chosen);
        let containment = arrangements.crossings(&containment_orbits(scene));

        let perms = permutations(cables);
        let mut picks: Vec<&[u8]> = Vec::with_capacity(anchors);
        let mut orbits = vec![vec![0u8; anchors]; cables];
        let mut best = usize::MAX;
        for combination in 0..perms.len().pow(anchors as u32) {
            let mut rest = combination;
            picks.clear();
            for _ in 0..anchors {
                picks.push(perms[rest % perms.len()].as_slice());
                rest /= perms.len();
            }
            for (cable, rings) in orbits.iter_mut().enumerate() {
                for (ring, ordering) in rings.iter_mut().zip(&picks) {
                    *ring = ordering[cable];
                }
            }
            best = best.min(arrangements.crossings(&orbits));
        }

        let escapes = if chosen_count > best {
            escapes(&mut arrangements, &chosen, chosen_count, &perms)
        } else {
            Escapes::default()
        };

        Some(Measured {
            chosen: chosen_count,
            containment,
            best,
            escapes,
            spanning: arrangements.spanning(&chosen),
        })
    }

    /// Which moves still improve on `chosen`, over every pair of cables and
    /// every part of the route they share.
    ///
    /// One exhaustive walk covers three of the four classes: a pair exchanged
    /// over a non-empty subset of the anchors is `orbits::refine`'s local move
    /// at a singleton, its coupled move at the full set, and the partial move in
    /// between. The fourth reorders one anchor outright, which is what reaches a
    /// three-cycle no exchange does.
    fn escapes(
        arrangements: &mut Arrangements,
        chosen: &[Vec<u8>],
        chosen_count: usize,
        perms: &[Vec<u8>],
    ) -> Escapes {
        let cables = chosen.len();
        let anchors = chosen[0].len();
        let mut found = Escapes::default();
        let mut candidate = chosen.to_vec();
        for inner in 0..cables {
            for outer in inner + 1..cables {
                for part in 1u32..1 << anchors {
                    candidate[inner].copy_from_slice(&chosen[inner]);
                    candidate[outer].copy_from_slice(&chosen[outer]);
                    for anchor in 0..anchors {
                        if part & (1 << anchor) != 0 {
                            candidate[inner][anchor] = chosen[outer][anchor];
                            candidate[outer][anchor] = chosen[inner][anchor];
                        }
                    }
                    if arrangements.crossings(&candidate) >= chosen_count {
                        continue;
                    }
                    match part.count_ones() as usize {
                        1 => found.local = true,
                        touched if touched == anchors => found.coupled = true,
                        _ => found.partial = true,
                    }
                }
                candidate[inner].copy_from_slice(&chosen[inner]);
                candidate[outer].copy_from_slice(&chosen[outer]);
            }
        }
        for anchor in 0..anchors {
            for ordering in perms {
                for (cable, rings) in candidate.iter_mut().enumerate() {
                    rings.copy_from_slice(&chosen[cable]);
                    rings[anchor] = ordering[cable];
                }
                found.reseated |= arrangements.crossings(&candidate) < chosen_count;
            }
        }
        found
    }

    /// The layout a sweep did worst on, by how far the search fell short of the
    /// best arrangement there was.
    #[derive(Debug, Clone, Copy)]
    struct Worst {
        gap: usize,
        seed: u64,
        anchors: usize,
        cables: usize,
        measured: Measured,
    }

    impl Worst {
        /// Everything a failure needs to name the layout and read what happened
        /// on it, the seed included so it can be rerun on its own.
        fn describe(&self) -> String {
            format!(
                "{} anchors, {} cables, seed {}: the search left {} corridor \
                 crossings, containment leaves {}, the best arrangement there is \
                 leaves {} - a gap of {}, {}",
                self.anchors,
                self.cables,
                self.seed,
                self.measured.chosen,
                self.measured.containment,
                self.measured.best,
                self.gap,
                if self.gap == 0 {
                    "which is the best arrangement there is"
                } else if self.measured.escapes.within_reach() {
                    "with a move the search does look at still improving, so the \
                     budget ran out"
                } else if self.measured.escapes.partial {
                    "a local minimum of both searched neighbourhoods that one \
                     exchange over PART of a shared route steps out of"
                } else if self.measured.escapes.reseated {
                    "a local minimum of both searched neighbourhoods that one \
                     anchor reordered outright steps out of"
                } else {
                    "a local minimum that no exchange over any part of a shared \
                     route and no reordering of one anchor steps out of"
                },
            )
        }
    }

    /// What a sweep of one scene shape found.
    #[derive(Debug, Default)]
    struct Tally {
        anchors: usize,
        cables: usize,
        layouts: usize,
        /// Draws whose cables would not transit the corridor, so never
        /// generated.
        skipped: usize,
        /// Layouts where the geometry dropped a wrap, leaving no corridor.
        dropped: usize,
        /// How often `chosen - best` took each value, from zero up.
        gap: Vec<usize>,
        /// Layouts whose containment seed genuinely crosses a corridor.
        crossing: usize,
        /// Layouts where the search took a corridor crossing out that
        /// containment alone leaves in, which is the whole point of it running.
        improved: usize,
        /// Layouts left short with nothing left improving in either
        /// neighbourhood `orbits::refine` searches: a local minimum rather than a
        /// spent budget.
        stalled: usize,
        /// Layouts left short while a move the search does look at was still
        /// improving, which only a spent budget explains.
        budgeted: usize,
        /// Stalled layouts one exchange over PART of a shared route steps out of.
        partial: usize,
        /// Stalled layouts one anchor reordered outright steps out of.
        reseated: usize,
        /// Stalled layouts nothing tried steps out of at all.
        sealed: usize,
        /// Layouts whose chosen arrangement has a crossing that only a band
        /// spanning an anchor holds, so production's consecutive bands leave it
        /// uncounted.
        spanning: usize,
        worst: Option<Worst>,
    }

    impl Tally {
        /// Layouts where the search found the best arrangement there was.
        fn exact(&self) -> usize {
            self.gap.first().copied().unwrap_or(0)
        }

        fn absorb(&mut self, other: &Tally) {
            self.layouts += other.layouts;
            self.skipped += other.skipped;
            self.dropped += other.dropped;
            self.crossing += other.crossing;
            self.improved += other.improved;
            self.stalled += other.stalled;
            self.budgeted += other.budgeted;
            self.partial += other.partial;
            self.reseated += other.reseated;
            self.sealed += other.sealed;
            self.spanning += other.spanning;
            if self.gap.len() < other.gap.len() {
                self.gap.resize(other.gap.len(), 0);
            }
            for (total, count) in self.gap.iter_mut().zip(&other.gap) {
                *total += count;
            }
            if other
                .worst
                .is_some_and(|worst| self.worst.is_none_or(|held| held.gap < worst.gap))
            {
                self.worst = other.worst;
            }
        }
    }

    /// Measures every scene of one shape drawn from `seeds`.
    ///
    /// This is where the never-worse guarantee is asserted, once per layout: the
    /// search only ever accepts a measured improvement over what it holds, so it
    /// cannot come out above the containment seed it started from.
    fn measure_seeds(anchors: usize, cables: usize, seeds: std::ops::Range<u64>) -> Tally {
        let mut tally = Tally {
            anchors,
            cables,
            ..Tally::default()
        };
        for seed in seeds {
            let Some(scene) = gate_scene(seed, anchors, cables) else {
                tally.skipped += 1;
                continue;
            };
            let Some(measured) = measure(&scene) else {
                tally.dropped += 1;
                continue;
            };
            tally.layouts += 1;
            let shape = format!("{anchors} anchors, {cables} cables, seed {seed}");
            assert!(
                measured.chosen <= measured.containment,
                "{shape}: the search left {} corridor crossings where containment \
                 alone leaves {} - a measured swap was accepted that made the \
                 arrangement worse",
                measured.chosen,
                measured.containment,
            );
            assert!(
                measured.best <= measured.chosen,
                "{shape}: the brute force bottomed out at {} corridor crossings, \
                 above the {} the search chose, so it does not cover every \
                 arrangement",
                measured.best,
                measured.chosen,
            );
            let gap = measured.chosen - measured.best;
            if tally.gap.len() <= gap {
                tally.gap.resize(gap + 1, 0);
            }
            tally.gap[gap] += 1;
            tally.crossing += usize::from(measured.containment > 0);
            tally.improved += usize::from(measured.chosen < measured.containment);
            tally.spanning += usize::from(measured.spanning > 0);
            if gap > 0 {
                if measured.escapes.within_reach() {
                    tally.budgeted += 1;
                } else {
                    tally.stalled += 1;
                    tally.partial += usize::from(measured.escapes.partial);
                    tally.reseated += usize::from(measured.escapes.reseated);
                    tally.sealed += usize::from(measured.escapes.none());
                }
            }
            if tally.worst.is_none_or(|worst| worst.gap < gap) {
                tally.worst = Some(Worst {
                    gap,
                    seed,
                    anchors,
                    cables,
                    measured,
                });
            }
        }
        tally
    }

    /// Sweeps `count` draws of one shape, split across as many threads as the
    /// machine offers.
    ///
    /// A scene is measured on its own and the chunks are folded in seed order,
    /// so what comes back does not depend on how the work was split - the split
    /// buys the brute force its sample, nothing else.
    fn sweep(anchors: usize, cables: usize, count: u64) -> Tally {
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .clamp(1, count.max(1) as usize) as u64;
        let chunks: Vec<Tally> = std::thread::scope(|scope| {
            let running: Vec<_> = (0..threads)
                .map(|chunk| {
                    let seeds = count * chunk / threads..count * (chunk + 1) / threads;
                    scope.spawn(move || measure_seeds(anchors, cables, seeds))
                })
                .collect();
            running
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
                .collect()
        });
        let mut tally = Tally {
            anchors,
            cables,
            ..Tally::default()
        };
        for chunk in &chunks {
            tally.absorb(chunk);
        }
        tally
    }

    /// The scene shapes the gate sweeps, and how many draws of each.
    ///
    /// The brute force is `cables!` orderings at every one of `anchors` anchors,
    /// each arrangement built and counted off real geometry, so the draw count
    /// falls as that product grows: at 24 orderings across 3 anchors one layout
    /// costs around 1.7 s of a debug build, against 0.7 ms at the other end.
    const GATE_SHAPES: [(usize, usize, u64); 6] = [
        (2, 2, 300),
        (2, 3, 300),
        (3, 2, 300),
        (2, 4, 150),
        (3, 3, 200),
        (3, 4, 40),
    ];

    /// The orbit search never comes out above containment, improves on it
    /// wherever containment crosses, and finds the best arrangement there is
    /// wherever the frame gives it the candidates to look.
    ///
    /// Every cable of a gate scene wraps every anchor, so every pair shares
    /// every anchor and every corridor between them is real. Three counts are
    /// taken off real geometry per layout: the arrangement `edge_hops` chose,
    /// the pure containment seed, and the least any arrangement leaves, that one
    /// by brute force over every ordering at every anchor independently. Each
    /// count is taken twice, once through [`crossings_between`] and once by
    /// filtering [`crossing_points`], and the two are asserted equal every time.
    /// See [`corridor_crossings`].
    ///
    /// The never-worse guarantee holds without exception, per layout and so per
    /// shape, which is what the assertion in [`measure_seeds`] says. Beyond it
    /// the measurement splits by whether the candidate budget let the descent
    /// finish:
    ///
    /// | anchors | cables | layouts | best found | worst gap | budget |
    /// | ------- | ------ | ------- | ---------- | --------- | ------ |
    /// | 2       | 2      | 278     | 100%       | 0         | ample  |
    /// | 2       | 3      | 266     | 92.1%      | 2         | ample  |
    /// | 3       | 2      | 252     | 98.4%      | 2         | ample  |
    /// | 2       | 4      | 129     | 69.0%      | 2         | spent  |
    /// | 3       | 3      | 158     | 72.8%      | 3         | spent  |
    /// | 3       | 4      | 28      | 0%         | 5         | spent  |
    ///
    /// 975 of 1111 layouts over the whole sweep, with 179 draws thrown out as
    /// doubling back and no wrap dropped. Containment alone crosses a corridor in
    /// 849 of those layouts and the search comes out under it on 728 of them, so
    /// what the guarantee covers is a search that moves rather than one that sits
    /// on its seed.
    ///
    /// Where the budget is spent the shortfall is the frame, not blindness. One
    /// round of the neighbourhood at three anchors and four cables is eighteen
    /// exchanges at single anchors plus six along whole routes, and
    /// `orbits::budget` affords fewer than that, so the search cannot complete a
    /// round: 24 of those 28 layouts still had an improving move in front of them
    /// when they ran out. It still never came out above containment and still
    /// improved on it in 24 of the 28. The ceiling is set by what a frame can
    /// afford - `edge_hops` runs two or three times a frame - and not by what the
    /// search would like, so the share above is recorded rather than asserted.
    ///
    /// Where the budget is ample the descent runs to a local minimum, and 98
    /// layouts over the sweep end in one. Both neighbourhoods `orbits::refine`
    /// searches were checked at every shortfall - two cables exchanged at ONE
    /// anchor, and the same two exchanged at EVERY anchor they share - so
    /// narrowing either would show up here as a worse share. Widening further is
    /// measured too: of the 98, one exchange over PART of a shared route steps out
    /// of 21 and one anchor reordered outright steps out of 22, while 55 yield to
    /// neither and want something other than a single downhill move. A third
    /// neighbourhood is therefore worth about a fifth of what is left, which is a
    /// trade for whoever finds a cheaper crossing count: while budget is the
    /// binding constraint, searching wider costs frame time that searching deeper
    /// wants.
    ///
    /// One more measured detail behind production's band list: it counts the
    /// corridors between CONSECUTIVE shared anchors, and a band spanning an
    /// anchor is not merely the two halves either side of it. It clears the reach
    /// of its two ends only, so it also holds crossings among the middle anchor's
    /// own rings, which both halves exclude. Over the arrangements this sweep
    /// evaluates that difference is common; in the arrangements the search
    /// actually chose it appears in 10 layouts of 1111.
    ///
    /// The shares above are a measured property of a bounded local search over
    /// this draw of scenes, not a claim of optimality. What is asserted are floors
    /// well below them: two thirds of the crossing layouts improved, and 85%
    /// exact where the budget is ample, against a measured 85.7% and 92.1%.
    /// Sweeping 1111 layouts costs ~15s, which is 93% of the lib suite's wall
    /// time, so it is kept off the inner loop: what it measures is the search's
    /// quality distribution, not whether the geometry is correct. CI runs it as
    /// its own step; locally, `cargo test -p iced_nodegraph --lib -- --ignored`.
    #[test]
    #[ignore = "search-quality sweep: ~15s over 1111 layouts"]
    fn the_orbit_search_holds_up_against_every_arrangement() {
        let shapes: Vec<Tally> = GATE_SHAPES
            .iter()
            .map(|&(anchors, cables, count)| sweep(anchors, cables, count))
            .collect();
        let mut total = Tally::default();
        for shape in &shapes {
            total.absorb(shape);
        }
        let report = |tally: &Tally| {
            format!(
                "{} layouts ({} skipped as doubling back, \
                 {} with a wrap the geometry dropped), {} found the best \
                 arrangement, gaps {:?}, {} crossing under containment alone and \
                 {} of those improved on it, {} out of budget and {} in a local \
                 minimum - of those {} yield to an exchange over part of a route, \
                 {} to one anchor reordered, {} to nothing tried; {} carry a \
                 crossing only a spanning band would count; worst {}",
                tally.layouts,
                tally.skipped,
                tally.dropped,
                tally.exact(),
                tally.gap,
                tally.crossing,
                tally.improved,
                tally.budgeted,
                tally.stalled,
                tally.partial,
                tally.reseated,
                tally.sealed,
                tally.spanning,
                tally
                    .worst
                    .map_or("nothing measured".to_owned(), |worst| worst.describe()),
            )
        };
        let named = |shape: &Tally| {
            format!(
                "{} anchors, {} cables: {}",
                shape.anchors,
                shape.cables,
                report(shape),
            )
        };
        let sweep: String = shapes.iter().map(|shape| named(shape) + "\n").collect();

        // A sweep that generated nothing, or threw every layout away, would pass
        // every comparison below by having nothing to compare.
        assert!(
            total.layouts > 900 && total.skipped > 0,
            "the sweep did not generate the situation it measures: {}\n{sweep}",
            report(&total),
        );
        // Containment alone has to cross for the comparison to say anything: a
        // sweep of layouts nothing can get wrong cannot tell a search that works
        // from one that does not.
        assert!(
            total.crossing * 2 > total.layouts,
            "containment crosses a corridor in only {} of {} layouts, so the \
             comparison mostly measures layouts with nothing to fix: {}\n{sweep}",
            total.crossing,
            total.layouts,
            report(&total),
        );
        for shape in &shapes {
            assert!(
                shape.crossing * 3 >= shape.layouts,
                "one shape carries almost no crossings to fix: {}\n{sweep}",
                named(shape),
            );
        }
        // A search that never moved off its seed would satisfy the never-worse
        // comparison in `measure_seeds` by doing nothing at all. This is the
        // vacuity guard: what the design promises is an improvement on
        // containment wherever containment crosses, not the best arrangement.
        assert!(
            total.improved * 3 >= total.crossing * 2,
            "the search improved on containment in only {} of the {} layouts where \
             containment crosses, so it is barely doing anything: {}\n{sweep}",
            total.improved,
            total.crossing,
            report(&total),
        );
        // Exactness is only a fair thing to ask where the budget let the descent
        // finish, which is where no shortfall had a move left in reach. Where it
        // did not, the share is recorded above and gated on nothing: the search
        // ran out of frame, and the frame is what sets the budget.
        //
        // Which shapes those are is asserted first: the three smallest are the
        // ones this sweep measured budget to spare on, and a ceiling low enough
        // to starve them all would leave the only floor on finding the BEST
        // arrangement asked of nothing.
        let ample: Vec<(usize, usize)> = shapes
            .iter()
            .filter(|shape| shape.budgeted == 0)
            .map(|shape| (shape.anchors, shape.cables))
            .collect();
        for shape in [(2, 2), (2, 3), (3, 2)] {
            assert!(
                ample.contains(&shape),
                "{} anchors and {} cables ran out of budget, and the exactness \
                 floor only holds shapes that did not: it is asked of {ample:?}\
                 \n{sweep}",
                shape.0,
                shape.1,
            );
        }
        for shape in shapes.iter().filter(|shape| shape.budgeted == 0) {
            assert!(
                shape.exact() * 20 >= shape.layouts * 17,
                "with the budget seeing every move, the search found the best \
                 arrangement in {} of {} layouts of one shape, under the 85% this \
                 sweep measured 92.1% at worst for: {}\n{sweep}",
                shape.exact(),
                shape.layouts,
                named(shape),
            );
        }
    }
}
