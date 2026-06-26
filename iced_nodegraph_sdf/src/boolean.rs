//! Boolean operations (union, difference, intersection) on closed contours.
//!
//! The renderer evaluates a closed shape with a *nearest-segment* SDF: for each
//! pixel it finds the closest segment and takes that segment's sign (right side
//! = interior = negative for a CW contour). Point junctions (zero-length
//! segments) own the corners and resolve the sign in concave wedges.
//!
//! To combine shapes we therefore cannot just `min`/`max` distance fields - we
//! must produce a new *clean* boundary chain (lines + arcs) with correct
//! winding and junction points. This module clips two regions against each
//! other and re-stitches the surviving boundary into a fresh [`Drawable`].
//!
//! Only `Line` and `Arc` segments participate; bezier segments are not
//! supported as boolean operands (node shapes never use them).

use std::f32::consts::TAU;

use glam::Vec2;

use crate::drawable::{Drawable, Segment};

/// Geometric tolerance in world units. Endpoints closer than this are merged.
const EPS: f32 = 1e-3;

/// A single oriented boundary edge. Travel direction matters: the interior of a
/// CW contour lies to the *right* of the travel direction.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    Line {
        a: Vec2,
        b: Vec2,
    },
    /// Circular arc. `sweep` is signed: positive = clockwise (center to the
    /// right of travel), matching `ShapeBuilder`.
    Arc {
        center: Vec2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
    },
}

impl Edge {
    fn point_at(&self, t: f32) -> Vec2 {
        match *self {
            Edge::Line { a, b } => a.lerp(b, t),
            Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => {
                let ang = start_angle + sweep * t;
                center + Vec2::new(ang.cos(), ang.sin()) * radius
            }
        }
    }

    fn start(&self) -> Vec2 {
        self.point_at(0.0)
    }

    fn end(&self) -> Vec2 {
        self.point_at(1.0)
    }

    /// Unit travel direction at parameter `t`.
    fn dir_at(&self, t: f32) -> Vec2 {
        match *self {
            Edge::Line { a, b } => (b - a).normalize_or_zero(),
            Edge::Arc {
                start_angle, sweep, ..
            } => {
                let ang = start_angle + sweep * t;
                // d/dt of (cos,sin) is (-sin,cos); sign(sweep) sets travel sense.
                let s = if sweep >= 0.0 { 1.0 } else { -1.0 };
                Vec2::new(-ang.sin(), ang.cos()) * s
            }
        }
    }

    fn length(&self) -> f32 {
        match *self {
            Edge::Line { a, b } => a.distance(b),
            Edge::Arc { radius, sweep, .. } => sweep.abs() * radius,
        }
    }

    fn reverse(&self) -> Edge {
        match *self {
            Edge::Line { a, b } => Edge::Line { a: b, b: a },
            Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => Edge::Arc {
                center,
                radius,
                start_angle: start_angle + sweep,
                sweep: -sweep,
            },
        }
    }

    /// Split into sub-edges at the given parameters (each in `(0,1)`).
    /// Parameters are sorted and de-duplicated internally.
    fn split(&self, params: &[f32]) -> Vec<Edge> {
        let mut ts: Vec<f32> = params
            .iter()
            .copied()
            .filter(|&t| t > 1e-4 && t < 1.0 - 1e-4)
            .collect();
        ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ts.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        if ts.is_empty() {
            return vec![*self];
        }
        let mut out = Vec::with_capacity(ts.len() + 1);
        let mut prev = 0.0;
        for &t in ts.iter().chain(std::iter::once(&1.0)) {
            out.push(self.sub(prev, t));
            prev = t;
        }
        out
    }

    /// Sub-edge spanning parameter range `[t0, t1]`.
    fn sub(&self, t0: f32, t1: f32) -> Edge {
        match *self {
            Edge::Line { a, b } => Edge::Line {
                a: a.lerp(b, t0),
                b: a.lerp(b, t1),
            },
            Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => Edge::Arc {
                center,
                radius,
                start_angle: start_angle + sweep * t0,
                sweep: sweep * (t1 - t0),
            },
        }
    }
}

/// Map an absolute angle onto an arc's `[0,1]` parameter, or `None` if the
/// angle is outside the arc's sweep.
fn arc_param(angle: f32, start: f32, sweep: f32) -> Option<f32> {
    let s_abs = sweep.abs();
    if s_abs < 1e-6 {
        return None;
    }
    let rel = if sweep >= 0.0 {
        (angle - start).rem_euclid(TAU)
    } else {
        (start - angle).rem_euclid(TAU)
    };
    if rel <= s_abs + 1e-3 {
        Some((rel / s_abs).clamp(0.0, 1.0))
    } else if rel >= TAU - 1e-3 {
        // Numerically just shy of the start point.
        Some(0.0)
    } else {
        None
    }
}

// --- Intersections -------------------------------------------------------

/// Intersection parameters `(t_a, t_b)` between two edges, both in `[0,1]`.
fn intersect(a: &Edge, b: &Edge) -> Vec<(f32, f32)> {
    match (a, b) {
        (Edge::Line { a: a0, b: a1 }, Edge::Line { a: b0, b: b1 }) => line_line(*a0, *a1, *b0, *b1),
        (
            Edge::Line { a: l0, b: l1 },
            Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            },
        ) => line_arc(*l0, *l1, *center, *radius, *start_angle, *sweep),
        (
            Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            },
            Edge::Line { a: l0, b: l1 },
        ) => line_arc(*l0, *l1, *center, *radius, *start_angle, *sweep)
            .into_iter()
            .map(|(t_line, t_arc)| (t_arc, t_line))
            .collect(),
        (
            Edge::Arc {
                center: c0,
                radius: r0,
                start_angle: s0,
                sweep: w0,
            },
            Edge::Arc {
                center: c1,
                radius: r1,
                start_angle: s1,
                sweep: w1,
            },
        ) => arc_arc(*c0, *r0, *s0, *w0, *c1, *r1, *s1, *w1),
    }
}

fn in_unit(t: f32) -> bool {
    (-1e-4..=1.0 + 1e-4).contains(&t)
}

fn line_line(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Vec<(f32, f32)> {
    let d1 = a1 - a0;
    let d2 = b1 - b0;
    let denom = d1.x * d2.y - d1.y * d2.x;
    if denom.abs() < 1e-9 {
        return Vec::new(); // parallel or collinear
    }
    let diff = b0 - a0;
    let t = (diff.x * d2.y - diff.y * d2.x) / denom;
    let s = (diff.x * d1.y - diff.y * d1.x) / denom;
    if in_unit(t) && in_unit(s) {
        vec![(t.clamp(0.0, 1.0), s.clamp(0.0, 1.0))]
    } else {
        Vec::new()
    }
}

fn line_arc(
    l0: Vec2,
    l1: Vec2,
    center: Vec2,
    radius: f32,
    start: f32,
    sweep: f32,
) -> Vec<(f32, f32)> {
    let d = l1 - l0;
    let f = l0 - center;
    let aa = d.dot(d);
    if aa < 1e-12 {
        return Vec::new();
    }
    let bb = 2.0 * f.dot(d);
    let cc = f.dot(f) - radius * radius;
    let disc = bb * bb - 4.0 * aa * cc;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.max(0.0).sqrt();
    // The two roots are (-bb +/- sq) / (2*aa), so their separation in the line
    // parameter t is sq/aa; multiplied by the segment length |d| = sqrt(aa) that
    // is sq/sqrt(aa) - the world-space distance between the two intersection
    // points. Comparing it to the world-unit EPS (1e-3) keeps this tangency test
    // consistent with every other distance check in this module.
    let roots = if sq / aa.sqrt() < EPS {
        // Tangent: the two contacts coincide within EPS, treat as one point.
        vec![-bb / (2.0 * aa)]
    } else {
        vec![(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)]
    };
    let mut out = Vec::new();
    for t in roots {
        if !in_unit(t) {
            continue;
        }
        let p = l0 + d * t;
        let ang = (p.y - center.y).atan2(p.x - center.x);
        if let Some(s) = arc_param(ang, start, sweep) {
            out.push((t.clamp(0.0, 1.0), s));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn arc_arc(
    c0: Vec2,
    r0: f32,
    s0: f32,
    w0: f32,
    c1: Vec2,
    r1: f32,
    s1: f32,
    w1: f32,
) -> Vec<(f32, f32)> {
    let d = c1 - c0;
    let dist = d.length();
    if dist < 1e-9 {
        return Vec::new(); // concentric: either coincident or no crossing
    }
    if dist > r0 + r1 + EPS || dist < (r0 - r1).abs() - EPS {
        return Vec::new();
    }
    // Distance from c0 to the radical line.
    let a = (r0 * r0 - r1 * r1 + dist * dist) / (2.0 * dist);
    let h_sq = r0 * r0 - a * a;
    let h = h_sq.max(0.0).sqrt();
    let mid = c0 + d * (a / dist);
    let perp = Vec2::new(-d.y, d.x) / dist;
    let candidates = if h < 1e-5 {
        vec![mid]
    } else {
        vec![mid + perp * h, mid - perp * h]
    };
    let mut out = Vec::new();
    for p in candidates {
        let ang0 = (p.y - c0.y).atan2(p.x - c0.x);
        let ang1 = (p.y - c1.y).atan2(p.x - c1.x);
        if let (Some(t0), Some(t1)) = (arc_param(ang0, s0, w0), arc_param(ang1, s1, w1)) {
            out.push((t0, t1));
        }
    }
    out
}

// --- Region conversion ---------------------------------------------------

/// One closed boundary loop.
type Loop = Vec<Edge>;

/// Decompose a closed [`Drawable`] into one or more boundary loops. `Point`
/// junctions are dropped (regenerated on output); beziers are unsupported.
fn from_drawable(d: &Drawable) -> Vec<Loop> {
    debug_assert!(
        d.is_closed(),
        "boolean ops require a closed contour; got an open drawable"
    );
    let mut edges = Vec::new();
    for seg in &d.segments {
        if seg.start == seg.end {
            // junction marker, no geometry
        } else if seg.curvature == 0.0 {
            edges.push(Edge::Line {
                a: seg.start,
                b: seg.end,
            });
        } else {
            let (center, radius, start_angle, sweep) =
                crate::segment::arc_params(seg.start, seg.end, seg.curvature)
                    .expect("a non-degenerate curved segment has arc params");
            edges.push(Edge::Arc {
                center,
                radius,
                start_angle,
                sweep,
            });
        }
    }
    let mut loops = edges_to_loops(edges);
    close_loops(&mut loops);
    loops
}

/// Ensure every loop forms a closed cycle (last edge's endpoint == first edge's
/// start point). The winding-based evaluator relies on closure; an open loop
/// would leak ray crossings and corrupt the inside/outside test. In debug
/// builds an unclosed loop is treated as a caller bug and trips an assertion;
/// in release the loop is repaired by appending a closing edge so rendering
/// stays robust. A lone full-circle arc already satisfies start == end.
fn close_loops(loops: &mut [Loop]) {
    for loop_ in loops.iter_mut() {
        let (Some(first), Some(last)) = (loop_.first().copied(), loop_.last().copied()) else {
            continue;
        };
        let start = first.start();
        let end = last.end();
        if !end.abs_diff_eq(start, EPS) {
            debug_assert!(
                false,
                "boolean op received an unclosed loop: start {start:?} != end {end:?}"
            );
            loop_.push(Edge::Line { a: end, b: start });
        }
    }
}

/// Partition a flat, ordered edge list into closed loops by connectivity.
fn edges_to_loops(edges: Vec<Edge>) -> Vec<Loop> {
    let mut loops = Vec::new();
    let mut current: Loop = Vec::new();
    for e in edges {
        if !current.is_empty() {
            let loop_start = current[0].start();
            let prev_end = current.last().unwrap().end();
            // A new loop begins when this edge does not continue the previous
            // one, or the previous edge already closed back to the start.
            if !e.start().abs_diff_eq(prev_end, EPS * 4.0)
                || prev_end.abs_diff_eq(loop_start, EPS * 4.0)
            {
                loops.push(std::mem::take(&mut current));
            }
        }
        current.push(e);
    }
    if !current.is_empty() {
        loops.push(current);
    }
    loops
}

/// True if `p` lies inside `region`, using the nonzero-winding rule so that
/// overlapping same-orientation loops (e.g. several pin cutouts) count as a
/// single filled area and oppositely-wound holes subtract correctly.
fn inside_region(p: Vec2, region: &[Loop]) -> bool {
    // Generic direction (cos 1, sin 1) avoids axis-aligned degeneracies.
    let u = Vec2::new(0.540_302_3, 0.841_470_9);
    let mut winding = 0i32;
    for loop_ in region {
        for e in loop_ {
            winding += ray_crossings(p, u, e);
        }
    }
    winding != 0
}

/// Signed forward crossings of the ray `p + k*u` (k > 0) with an edge. Each
/// crossing contributes +/-1 by the sign of `u x travel_direction`.
fn ray_crossings(p: Vec2, u: Vec2, e: &Edge) -> i32 {
    fn cross(a: Vec2, b: Vec2) -> f32 {
        a.x * b.y - a.y * b.x
    }
    match *e {
        Edge::Line { a, b } => {
            let d = b - a;
            // p + k u = a + t d
            let det = u.x * (-d.y) - (-d.x) * u.y;
            if det.abs() < 1e-9 {
                return 0;
            }
            let rhs = a - p;
            let k = (rhs.x * (-d.y) - (-d.x) * rhs.y) / det;
            let t = (u.x * rhs.y - u.y * rhs.x) / det;
            if k > EPS && t > 0.0 && t < 1.0 {
                if cross(u, d) > 0.0 { 1 } else { -1 }
            } else {
                0
            }
        }
        Edge::Arc {
            center,
            radius,
            start_angle,
            sweep,
        } => {
            let q = p - center;
            let b_coef = u.dot(q);
            let c_coef = q.dot(q) - radius * radius;
            let disc = b_coef * b_coef - c_coef;
            if disc < 0.0 {
                return 0;
            }
            let sq = disc.sqrt();
            let mut winding = 0;
            for k in [-b_coef - sq, -b_coef + sq] {
                if k <= EPS {
                    continue;
                }
                let hit = p + u * k;
                let ang = (hit.y - center.y).atan2(hit.x - center.x);
                if let Some(t) = arc_param(ang, start_angle, sweep)
                    && t > 1e-4
                    && t < 1.0 - 1e-4
                {
                    let dir = e.dir_at(t);
                    winding += if cross(u, dir) > 0.0 { 1 } else { -1 };
                }
            }
            winding
        }
    }
}

// --- Boolean core ---------------------------------------------------------

/// Which boundary pieces a boolean operation keeps.
#[derive(Clone, Copy)]
enum BoolOp {
    Union,
    Intersection,
    /// `A - B`.
    Difference,
}

/// Distance a classification probe is nudged off an edge to decide which side
/// is interior. Far larger than intersection noise (~1e-4), far smaller than
/// any real feature (pin radii are several world units).
const PROBE: f32 = 1e-2;

fn boolean(a: &Drawable, b: &Drawable, op: BoolOp) -> Drawable {
    let mut a_loops = from_drawable(a);
    let mut b_loops = from_drawable(b);

    // Recenter into a local frame before any geometry math. Segments carry
    // absolute world coordinates, so a shape dragged far from the origin runs
    // every intersection/stitch computation at large magnitudes where float32
    // precision (ULP grows with magnitude) erodes the fixed tolerances. Working
    // around a shared local origin makes precision depend only on a shape's own
    // size, not its world position. The result is shifted back at the end.
    let ab = a.bounds();
    let bb = b.bounds();
    let origin = Vec2::new(
        (ab[0].min(bb[0]) + ab[2].max(bb[2])) * 0.5,
        (ab[1].min(bb[1]) + ab[3].max(bb[3])) * 0.5,
    );
    offset_loops(&mut a_loops, -origin);
    offset_loops(&mut b_loops, -origin);

    // Whether the result region contains `p`.
    let inside_result = |p: Vec2| -> bool {
        let in_a = inside_region(p, &a_loops);
        let in_b = inside_region(p, &b_loops);
        match op {
            BoolOp::Union => in_a || in_b,
            BoolOp::Intersection => in_a && in_b,
            BoolOp::Difference => in_a && !in_b,
        }
    };

    // Split every edge at every pairwise intersection (including A-A and B-B
    // self-crossings, so overlapping cutouts are handled) so that no sub-edge
    // straddles another boundary.
    let edges: Vec<Edge> = a_loops
        .iter()
        .chain(b_loops.iter())
        .flatten()
        .copied()
        .collect();
    let mut params: Vec<Vec<f32>> = vec![Vec::new(); edges.len()];
    for i in 0..edges.len() {
        for j in 0..edges.len() {
            if i == j {
                continue;
            }
            for (ti, _tj) in intersect(&edges[i], &edges[j]) {
                params[i].push(ti);
            }
        }
    }
    let subs: Vec<Edge> = edges
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.split(&params[i]))
        .collect();

    // A sub-edge is on the result boundary iff its two sides disagree on
    // membership. Orient it so the interior (inside) lies to the right.
    let mut selected: Vec<Edge> = Vec::new();
    for e in &subs {
        if e.length() <= EPS {
            continue;
        }
        let mid = e.point_at(0.5);
        let right = interior_normal(e.dir_at(0.5));
        let inside_right = inside_result(mid + right * PROBE);
        let inside_left = inside_result(mid - right * PROBE);
        match (inside_right, inside_left) {
            (true, false) => selected.push(*e),
            (false, true) => selected.push(e.reverse()),
            _ => {} // both sides same: not a boundary (e.g. duplicate cutout)
        }
    }

    dedup_edges(&mut selected);
    let mut loops = stitch(selected);
    // Back to world coordinates.
    offset_loops(&mut loops, origin);
    build_drawable(&loops)
}

/// Translate every edge of every loop by `o` (positions only; radii/angles are
/// translation-invariant).
fn offset_loops(loops: &mut [Loop], o: Vec2) {
    for loop_ in loops.iter_mut() {
        for e in loop_.iter_mut() {
            match e {
                Edge::Line { a, b } => {
                    *a += o;
                    *b += o;
                }
                Edge::Arc { center, .. } => {
                    *center += o;
                }
            }
        }
    }
}

/// Drop edges that coincide geometrically (e.g. two identical pin cutouts both
/// surviving classification). Keeps one representative.
fn dedup_edges(edges: &mut Vec<Edge>) {
    let mut kept: Vec<Edge> = Vec::with_capacity(edges.len());
    for e in edges.drain(..) {
        let dup = kept.iter().any(|k| {
            (k.start().abs_diff_eq(e.start(), EPS) && k.end().abs_diff_eq(e.end(), EPS)
                || k.start().abs_diff_eq(e.end(), EPS) && k.end().abs_diff_eq(e.start(), EPS))
                && k.point_at(0.5).abs_diff_eq(e.point_at(0.5), EPS)
        });
        if !dup {
            kept.push(e);
        }
    }
    *edges = kept;
}

/// Connect oriented sub-edges into closed loops by nearest-neighbour matching.
///
/// At each step the chain continues to the remaining edge whose start is
/// *nearest* the current tail, rather than the first within a fixed absolute
/// tolerance. In a valid boolean result the true continuation is essentially
/// coincident (a few float ULP) while every other edge start is at least a
/// feature-size away, so the nearest match is unambiguous — and, crucially,
/// independent of the absolute coordinate magnitude. A fixed tolerance (the old
/// `4*EPS`) erodes against float32 precision once a node is dragged into large
/// world coordinates: a single junction then exceeds it, the whole multi-edge
/// loop fails to close and is dropped, leaving an empty contour (observed on the
/// dense Edge Config node ~900 units from the origin).
fn stitch(mut remaining: Vec<Edge>) -> Vec<Loop> {
    let mut loops = Vec::new();
    while let Some(first) = remaining.pop() {
        let start0 = first.start();
        let mut loop_: Loop = vec![first];
        loop {
            let tail = loop_.last().unwrap().end();
            // Distance to close the loop back to its start.
            let close_d = tail.distance(start0);
            // Nearest remaining edge start to the tail.
            let nearest = remaining
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.start().distance(tail)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match nearest {
                // Continue while some edge start is at least as near as closing
                // back to the loop start; otherwise close here. A lone
                // full-circle arc has tail == start (close_d == 0), so it closes
                // immediately instead of grabbing an unrelated edge.
                Some((i, d)) if d <= close_d => {
                    loop_.push(remaining.remove(i));
                }
                _ => break,
            }
        }
        // Accept loops that return to their start. The true closure gap is
        // float noise (a few ULP); an open chain is off by a whole feature.
        // Since the boolean now runs in a recentered local frame, a small fixed
        // tolerance suffices regardless of world position. A lone full-circle
        // arc already has start == end.
        let end = loop_.last().unwrap().end();
        if end.distance(start0) <= 8.0 * EPS {
            loops.push(loop_);
        }
    }
    loops
}

// --- Drawable assembly ----------------------------------------------------

/// Interior-pointing unit normal of a CW edge's travel direction.
fn interior_normal(dir: Vec2) -> Vec2 {
    Vec2::new(-dir.y, dir.x)
}

/// Build a closed [`Drawable`] from result loops, inserting `Point` junctions
/// at tangent discontinuities so the renderer signs concave corners correctly.
fn build_drawable(loops: &[Loop]) -> Drawable {
    let mut segs: Vec<Segment> = Vec::new();
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut acc = 0.0f32;

    let mut grow = |p: Vec2| {
        min = min.min(p);
        max = max.max(p);
    };

    for loop_ in loops {
        let n = loop_.len();
        for (i, e) in loop_.iter().enumerate() {
            // Junction point between the previous edge and this one.
            let prev = if i == 0 { &loop_[n - 1] } else { &loop_[i - 1] };
            let d_in = prev.dir_at(1.0);
            let d_out = e.dir_at(0.0);
            if d_in.dot(d_out) < 1.0 - 1e-5 {
                let interior = (interior_normal(d_in) + interior_normal(d_out)).normalize_or_zero();
                if interior != Vec2::ZERO {
                    let pos = e.start();
                    let heading = interior.y.atan2(interior.x);
                    grow(pos);
                    segs.push(Segment::point(pos, heading, true, acc));
                }
            }

            match *e {
                Edge::Line { a, b } => {
                    grow(a);
                    grow(b);
                    let len = e.length();
                    segs.push(Segment::line(a, b, true, acc, acc + len));
                    acc += len;
                }
                Edge::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep,
                } => {
                    grow(center - Vec2::splat(radius));
                    grow(center + Vec2::splat(radius));
                    // Split into minor sub-arcs (the arc-only invariant) while
                    // advancing the cumulative arc length.
                    Segment::push_arc(
                        &mut segs,
                        center,
                        radius,
                        start_angle,
                        sweep,
                        true,
                        &mut acc,
                    );
                }
            }
        }
    }

    if segs.is_empty() {
        min = Vec2::ZERO;
        max = Vec2::ZERO;
    }
    Drawable::from_boolean_segments(segs, acc, [min.x, min.y, max.x, max.y])
}

// --- Public API -----------------------------------------------------------

/// Union of two closed shapes (`A ∪ B`).
pub fn union(a: &Drawable, b: &Drawable) -> Drawable {
    boolean(a, b, BoolOp::Union)
}

/// Difference of two closed shapes (`A - B`), i.e. `A` with `B` removed.
pub fn difference(a: &Drawable, b: &Drawable) -> Drawable {
    boolean(a, b, BoolOp::Difference)
}

/// Intersection of two closed shapes (`A ∩ B`).
pub fn intersection(a: &Drawable, b: &Drawable) -> Drawable {
    boolean(a, b, BoolOp::Intersection)
}

/// Concatenate several closed shapes into a single multi-loop region. The loops
/// are not unified; overlaps are resolved by the nonzero-winding rule when the
/// region participates in a boolean op. Useful for collecting all pin cutouts
/// of a node before subtracting them in one pass.
pub fn merge(shapes: &[Drawable]) -> Drawable {
    let mut segs = Vec::new();
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for d in shapes {
        segs.extend(d.segments.iter().copied());
        let b = d.bounds();
        min = min.min(Vec2::new(b[0], b[1]));
        max = max.max(Vec2::new(b[2], b[3]));
    }
    if segs.is_empty() {
        min = Vec2::ZERO;
        max = Vec2::ZERO;
    }
    Drawable::from_boolean_segments(segs, 0.0, [min.x, min.y, max.x, max.y])
}

/// Subtract many shapes from `base` in a single pass (`base - ∪cuts`).
/// Overlapping and coincident cuts are handled cleanly.
pub fn difference_many(base: &Drawable, cuts: &[Drawable]) -> Drawable {
    if cuts.is_empty() {
        return base.clone();
    }
    difference(base, &merge(cuts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::Curve;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn line_line_cross_center() {
        // Horizontal line through origin, vertical line through origin.
        let h = Edge::Line {
            a: Vec2::new(-10.0, 0.0),
            b: Vec2::new(10.0, 0.0),
        };
        let v = Edge::Line {
            a: Vec2::new(0.0, -10.0),
            b: Vec2::new(0.0, 10.0),
        };
        let hits = intersect(&h, &v);
        assert_eq!(hits.len(), 1);
        assert!(approx(hits[0].0, 0.5) && approx(hits[0].1, 0.5));
        assert!(h.point_at(hits[0].0).abs_diff_eq(Vec2::ZERO, 1e-3));
    }

    #[test]
    fn parallel_lines_no_hit() {
        let a = Edge::Line {
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(10.0, 0.0),
        };
        let b = Edge::Line {
            a: Vec2::new(0.0, 5.0),
            b: Vec2::new(10.0, 5.0),
        };
        assert!(intersect(&a, &b).is_empty());
    }

    #[test]
    fn line_through_circle_two_points() {
        // Full circle radius 5 at origin, horizontal line y=0 crossing it.
        let circle = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        let line = Edge::Line {
            a: Vec2::new(-10.0, 0.0),
            b: Vec2::new(10.0, 0.0),
        };
        let hits = intersect(&line, &circle);
        assert_eq!(hits.len(), 2, "line should cross circle twice");
        let xs: Vec<f32> = hits.iter().map(|(t, _)| line.point_at(*t).x).collect();
        assert!(xs.iter().any(|x| approx(*x, -5.0)));
        assert!(xs.iter().any(|x| approx(*x, 5.0)));
    }

    #[test]
    fn line_tangent_to_circle_one_point() {
        let circle = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        let line = Edge::Line {
            a: Vec2::new(-10.0, 5.0),
            b: Vec2::new(10.0, 5.0),
        };
        let hits = intersect(&line, &circle);
        assert_eq!(hits.len(), 1, "tangent line touches once");
        assert!(approx(line.point_at(hits[0].0).x, 0.0));
    }

    #[test]
    fn line_misses_arc_outside_sweep() {
        // Quarter arc on the +x,+y side only; line crosses the circle on -x side.
        let arc = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: FRAC_PI_2,
        };
        let line = Edge::Line {
            a: Vec2::new(-10.0, 1.0),
            b: Vec2::new(-1.0, 1.0),
        };
        assert!(intersect(&line, &arc).is_empty());
    }

    #[test]
    fn two_circles_two_points() {
        let c0 = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        let c1 = Edge::Arc {
            center: Vec2::new(6.0, 0.0),
            radius: 5.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        let hits = intersect(&c0, &c1);
        assert_eq!(hits.len(), 2, "overlapping circles cross twice");
        // Both intersection points are at x = 3 by symmetry.
        for (t, _) in &hits {
            assert!(approx(c0.point_at(*t).x, 3.0));
        }
    }

    #[test]
    fn disjoint_circles_no_hit() {
        let c0 = Edge::Arc {
            center: Vec2::ZERO,
            radius: 2.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        let c1 = Edge::Arc {
            center: Vec2::new(10.0, 0.0),
            radius: 2.0,
            start_angle: 0.0,
            sweep: TAU,
        };
        assert!(intersect(&c0, &c1).is_empty());
    }

    // --- Dense-node pin-cutout regressions ---------------------------------
    // A node body with many pin cutouts (circles centered on an edge) must
    // produce a closed, non-empty contour. The Edge Config node in the
    // hello_world demo (≈20+ stacked left-edge pins) degenerated to a
    // non-closed/empty outline, which the renderer then dropped entirely.

    /// Sanity: one cutout notch on an edge stays closed.
    #[test]
    fn rect_minus_one_edge_circle_closed() {
        let body = Curve::rect([80.0, 200.0], [80.0, 200.0]);
        let out = difference_many(&body, &[Curve::circle([0.0, 100.0], 6.0)]);
        assert!(out.segment_count() > 0, "empty");
        assert!(out.is_closed(), "not closed");
    }

    /// Many well-separated cutouts on one edge (the dense-node case).
    #[test]
    fn rect_minus_many_edge_circles_closed() {
        let body = Curve::rect([80.0, 200.0], [80.0, 200.0]); // x in [0,160], y in [0,400]
        let cuts: Vec<Drawable> = (0..20)
            .map(|i| Curve::circle([0.0, 20.0 + i as f32 * 18.0], 3.4))
            .collect();
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty result");
        assert!(out.is_closed(), "result not closed");
    }

    /// Same on a rounded body (matches the actual node shape with corner arcs).
    #[test]
    fn rounded_rect_minus_many_edge_circles_closed() {
        let body = Curve::rounded_rect([80.0, 200.0], [80.0, 200.0], 8.0);
        let cuts: Vec<Drawable> = (0..20)
            .map(|i| Curve::circle([0.0, 20.0 + i as f32 * 18.0], 3.4))
            .collect();
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty result");
        assert!(out.is_closed(), "result not closed");
    }

    /// A cutout straddling a (rounded) corner — arc-vs-arc near-tangency.
    #[test]
    fn rounded_rect_minus_corner_circle_closed() {
        let body = Curve::rounded_rect([80.0, 200.0], [80.0, 200.0], 8.0);
        // Near the top-left rounded corner.
        let out = difference_many(&body, &[Curve::circle([0.0, 0.0], 5.0)]);
        assert!(out.segment_count() > 0, "empty");
        assert!(out.is_closed(), "not closed");
    }

    /// Same dense cutouts but far from the origin: nodes are dragged into large
    /// world coordinates, where absolute EPS/JOIN tolerances erode against
    /// float32 precision and the contour can fail to stitch closed.
    #[test]
    fn rect_minus_many_edge_circles_far_from_origin_closed() {
        let (ox, oy) = (12345.0_f32, 6789.0_f32);
        let body = Curve::rect([ox + 80.0, oy + 200.0], [80.0, 200.0]);
        let cuts: Vec<Drawable> = (0..20)
            .map(|i| Curve::circle([ox, oy + 20.0 + i as f32 * 18.0], 3.4))
            .collect();
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty result far from origin");
        assert!(out.is_closed(), "result not closed far from origin");
    }

    /// Exact geometry captured from the hello_world Edge Config node, which
    /// produced a degenerate (non-closed/empty) outline. ~26 left-edge cutouts
    /// plus one on the right border; well separated, yet it failed to stitch.
    #[test]
    fn edge_config_node_real_geometry_closed() {
        let body = Curve::rounded_rect([641.626, 643.551], [75.000, 302.600], 8.000);
        let cuts = [
            Curve::circle([566.626, 384.251], 2.400),
            Curve::circle([716.626, 384.251], 2.400),
            Curve::circle([566.626, 428.851], 2.400),
            Curve::circle([566.626, 445.851], 2.400),
            Curve::circle([566.626, 462.851], 2.400),
            Curve::circle([566.626, 479.851], 2.400),
            Curve::circle([566.626, 496.851], 2.400),
            Curve::circle([566.626, 513.851], 2.400),
            Curve::circle([566.626, 554.551], 2.400),
            Curve::circle([566.626, 571.551], 2.400),
            Curve::circle([566.626, 588.551], 2.400),
            Curve::circle([566.626, 605.551], 2.400),
            Curve::circle([566.626, 622.551], 2.400),
            Curve::circle([566.626, 663.251], 2.400),
            Curve::circle([566.626, 680.251], 2.400),
            Curve::circle([566.626, 697.251], 2.400),
            Curve::circle([566.626, 714.251], 2.400),
            Curve::circle([566.626, 731.251], 2.400),
            Curve::circle([566.626, 748.251], 2.400),
            Curve::circle([566.626, 765.251], 2.400),
            Curve::circle([566.626, 782.251], 2.400),
            Curve::circle([566.626, 822.951], 2.400),
            Curve::circle([566.626, 839.951], 2.400),
            Curve::circle([566.626, 856.951], 2.400),
            Curve::circle([566.626, 873.951], 2.400),
            Curve::circle([566.626, 890.951], 2.400),
            Curve::circle([566.626, 907.951], 2.400),
        ];
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty result");
        assert!(out.is_closed(), "result not closed");
    }

    /// Extreme world coordinates (~1e6): only correct because the boolean
    /// recenters to a local frame before doing geometry. Absolute math here
    /// would have float32 ULP ~0.06, swamping any fixed tolerance.
    #[test]
    fn rect_minus_many_edge_circles_extreme_coords_closed() {
        let (ox, oy) = (1_000_000.0_f32, 2_000_000.0_f32);
        let body = Curve::rect([ox + 80.0, oy + 200.0], [80.0, 200.0]);
        let cuts: Vec<Drawable> = (0..20)
            .map(|i| Curve::circle([ox, oy + 20.0 + i as f32 * 18.0], 3.4))
            .collect();
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty at extreme coords");
        assert!(out.is_closed(), "not closed at extreme coords");
    }

    /// Two near-tangent cutouts (spacing ≈ 2r) on an edge.
    #[test]
    fn rect_minus_near_tangent_circles_closed() {
        let body = Curve::rect([80.0, 200.0], [80.0, 200.0]);
        let r = 4.0;
        let cuts = [
            Curve::circle([0.0, 100.0], r),
            Curve::circle([0.0, 100.0 + 2.0 * r - 0.02], r), // almost touching
        ];
        let out = difference_many(&body, &cuts);
        assert!(out.segment_count() > 0, "empty");
        assert!(out.is_closed(), "not closed");
    }

    #[test]
    fn split_line_midpoint() {
        let l = Edge::Line {
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(10.0, 0.0),
        };
        let parts = l.split(&[0.5]);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].end().abs_diff_eq(Vec2::new(5.0, 0.0), 1e-3));
        assert!(parts[1].start().abs_diff_eq(Vec2::new(5.0, 0.0), 1e-3));
    }

    #[test]
    fn split_arc_preserves_endpoints() {
        let arc = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: PI,
        };
        let parts = arc.split(&[0.5]);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].start().abs_diff_eq(arc.start(), 1e-3));
        assert!(parts[1].end().abs_diff_eq(arc.end(), 1e-3));
        assert!(parts[0].end().abs_diff_eq(parts[1].start(), 1e-3));
    }

    #[test]
    fn reverse_swaps_endpoints() {
        let l = Edge::Line {
            a: Vec2::new(1.0, 2.0),
            b: Vec2::new(3.0, 4.0),
        };
        let r = l.reverse();
        assert!(r.start().abs_diff_eq(l.end(), 1e-3));
        assert!(r.end().abs_diff_eq(l.start(), 1e-3));

        let arc = Edge::Arc {
            center: Vec2::ZERO,
            radius: 5.0,
            start_angle: 0.0,
            sweep: PI,
        };
        let ra = arc.reverse();
        assert!(ra.start().abs_diff_eq(arc.end(), 1e-3));
        assert!(ra.end().abs_diff_eq(arc.start(), 1e-3));
    }

    // --- CPU SDF oracle (mirrors shader sd_line / sd_arc_segment / sd_point) ---

    fn cpu_sd_line(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let ba = b - a;
        let pa = p - a;
        let len_sq = ba.dot(ba);
        let t = if len_sq > 0.0 {
            (pa.dot(ba) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dist = (p - (a + ba * t)).length();
        let n = Vec2::new(-ba.y, ba.x);
        if len_sq > 0.0 && pa.dot(n) > 0.0 {
            -dist
        } else {
            dist
        }
    }

    fn cpu_sd_arc(p: Vec2, center: Vec2, radius: f32, start: f32, sweep: f32) -> f32 {
        let offset = p - center;
        let dtc = offset.length();
        let angle = offset.y.atan2(offset.x);
        let rel = if sweep > 0.0 {
            (angle - start).rem_euclid(TAU)
        } else {
            (angle - start) - ((angle - start) / TAU).ceil() * TAU
        };
        let on_arc = (sweep > 0.0 && rel <= sweep) || (sweep < 0.0 && rel >= sweep);
        if on_arc {
            let dist = (dtc - radius).abs();
            let v = if sweep > 0.0 {
                radius - dtc
            } else {
                -(radius - dtc)
            };
            if v > 0.0 { -dist } else { dist }
        } else {
            let end_angle = start + sweep;
            let p_start = center + Vec2::new(start.cos(), start.sin()) * radius;
            let p_end = center + Vec2::new(end_angle.cos(), end_angle.sin()) * radius;
            let (pt, ang) = if (p - p_start).length() < (p - p_end).length() {
                (p_start, start)
            } else {
                (p_end, end_angle)
            };
            let dist = (p - pt).length();
            let tangent = Vec2::new(-ang.sin(), ang.cos()) * sweep.signum();
            let n = Vec2::new(-tangent.y, tangent.x);
            if (p - pt).dot(n) > 0.0 { -dist } else { dist }
        }
    }

    fn cpu_sd_point(p: Vec2, pos: Vec2, heading: f32) -> f32 {
        let dist = (p - pos).length() - 0.01;
        let dist = dist.max(0.0);
        let right = Vec2::new(heading.cos(), heading.sin());
        if (p - pos).dot(right) > 0.0 {
            -dist
        } else {
            dist
        }
    }

    /// Nearest-segment signed distance, exactly as the shader composites a shape.
    fn cpu_eval(p: Vec2, d: &Drawable) -> f32 {
        let mut best = f32::MAX;
        for seg in &d.segments {
            let sd = if seg.start == seg.end {
                cpu_sd_point(p, seg.start, seg.heading)
            } else if seg.curvature == 0.0 {
                cpu_sd_line(p, seg.start, seg.end)
            } else {
                let (center, radius, start_angle, sweep) =
                    crate::segment::arc_params(seg.start, seg.end, seg.curvature).unwrap();
                cpu_sd_arc(p, center, radius, start_angle, sweep)
            };
            if sd.abs() < best.abs() {
                best = sd;
            }
        }
        best
    }

    /// Sample a grid; assert the result's SDF sign matches `expected` membership
    /// everywhere outside a thin band around the input boundaries.
    fn assert_matches<F: Fn(Vec2) -> bool>(
        result: &Drawable,
        inputs: &[&Drawable],
        expected: F,
        range: (f32, f32, f32, f32),
    ) {
        let (x0, y0, x1, y1) = range;
        let margin = 1.5;
        let step = 1.0;
        // Irrational jitter so no sample lands exactly on an axis-aligned edge's
        // supporting line, where the nearest-segment sign is numerically
        // unstable (a measure-zero artifact of the sampling, not the geometry).
        let mut x = x0 + 0.137;
        let mut mismatches = 0;
        while x <= x1 {
            let mut y = y0 + 0.071;
            while y <= y1 {
                let p = Vec2::new(x, y);
                // Skip points near any input boundary (sign is ambiguous there).
                let near = inputs.iter().any(|d| cpu_eval(p, d).abs() < margin);
                if !near {
                    let inside_result = cpu_eval(p, result) < 0.0;
                    if inside_result != expected(p) {
                        mismatches += 1;
                        if mismatches <= 5 {
                            eprintln!(
                                "mismatch at {p:?}: result_inside={inside_result} expected={}",
                                expected(p)
                            );
                        }
                    }
                }
                y += step;
            }
            x += step;
        }
        assert_eq!(
            mismatches, 0,
            "{mismatches} sign mismatches vs expected set"
        );
    }

    fn circ_inside(p: Vec2, c: Vec2, r: f32) -> bool {
        p.distance(c) < r
    }

    /// Membership in an arbitrary input shape, via its own nearest-segment SDF.
    /// Reliable for single convex/simple loops (rects, rounded rects, circles).
    fn shape_inside(p: Vec2, d: &Drawable) -> bool {
        cpu_eval(p, d) < 0.0
    }

    #[test]
    fn difference_box_minus_circle_on_edge() {
        // Rounded rect centered at origin, half-size 60x40, corner radius 6.
        let rect = Curve::rounded_rect([0.0, 0.0], [60.0, 40.0], 6.0);
        // Circle straddling the right edge (a pin cutout poking inward).
        let circle = Curve::circle([60.0, 0.0], 8.0);
        let result = super::difference(&rect, &circle);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&rect, &circle],
            |p| shape_inside(p, &rect) && !circ_inside(p, Vec2::new(60.0, 0.0), 8.0),
            (-80.0, -60.0, 80.0, 60.0),
        );
    }

    #[test]
    fn union_two_overlapping_rects() {
        // Offset in both axes so no edges are collinear (collinear overlap is a
        // separate degeneracy handled later).
        let a = Curve::rect([0.0, 0.0], [30.0, 20.0]);
        let b = Curve::rect([25.0, 12.0], [30.0, 20.0]);
        let result = super::union(&a, &b);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&a, &b],
            |p| shape_inside(p, &a) || shape_inside(p, &b),
            (-50.0, -40.0, 80.0, 60.0),
        );
    }

    fn no_circ(p: Vec2, centers: &[Vec2], r: f32) -> bool {
        !centers.iter().any(|c| circ_inside(p, *c, r))
    }

    #[test]
    fn pin_cutouts_overlapping_on_left_edge() {
        // Rounded node, several pin cutouts on the left edge (x = -60), spaced
        // 8 apart with radius 6 so adjacent cutouts overlap.
        let rect = Curve::rounded_rect([0.0, 0.0], [60.0, 40.0], 6.0);
        let r = 6.0;
        let centers: Vec<Vec2> = [-16.0, -8.0, 0.0, 8.0]
            .iter()
            .map(|&y| Vec2::new(-60.0, y))
            .collect();
        let circles: Vec<Drawable> = centers
            .iter()
            .map(|c| Curve::circle([c.x, c.y], r))
            .collect();
        let result = super::difference_many(&rect, &circles);
        let cuts = super::merge(&circles);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&rect, &cuts],
            |p| shape_inside(p, &rect) && no_circ(p, &centers, r),
            (-80.0, -60.0, 80.0, 60.0),
        );
    }

    #[test]
    fn pin_cutouts_exactly_coincident() {
        // Two identical cutouts at the same spot must not produce a tangled
        // chain - the result is a single clean notch.
        let rect = Curve::rounded_rect([0.0, 0.0], [60.0, 40.0], 6.0);
        let r = 6.0;
        let c = Vec2::new(-60.0, 0.0);
        let dup = super::merge(&[Curve::circle([c.x, c.y], r), Curve::circle([c.x, c.y], r)]);
        let result = super::difference(&rect, &dup);
        assert!(result.is_closed());
        // No zero-length or NaN segments crept in.
        for seg in &result.segments {
            assert!(seg.start.is_finite() && seg.end.is_finite());
            assert!(seg.curvature.is_finite() && seg.heading.is_finite());
        }
        assert_matches(
            &result,
            &[&rect, &dup],
            |p| shape_inside(p, &rect) && !circ_inside(p, c, r),
            (-80.0, -60.0, 80.0, 60.0),
        );
    }

    #[test]
    fn difference_circle_fully_inside_makes_hole() {
        // A cutout entirely within the body becomes an interior hole loop.
        let rect = Curve::rounded_rect([0.0, 0.0], [60.0, 40.0], 6.0);
        let hole = Curve::circle([0.0, 0.0], 10.0);
        let result = super::difference(&rect, &hole);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&rect, &hole],
            |p| shape_inside(p, &rect) && !circ_inside(p, Vec2::ZERO, 10.0),
            (-80.0, -60.0, 80.0, 60.0),
        );
    }

    #[test]
    fn difference_cutout_over_rounded_corner() {
        // Cutout overlapping the top-right rounded corner (arc-arc clipping).
        let rect = Curve::rounded_rect([0.0, 0.0], [60.0, 40.0], 10.0);
        let c = Vec2::new(60.0, -40.0);
        let circle = Curve::circle([c.x, c.y], 14.0);
        let result = super::difference(&rect, &circle);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&rect, &circle],
            |p| shape_inside(p, &rect) && !circ_inside(p, c, 14.0),
            (-80.0, -60.0, 80.0, 60.0),
        );
    }

    #[test]
    fn union_collinear_shared_edges() {
        // Equal heights -> top and bottom edges are collinear (shared lines).
        let a = Curve::rect([0.0, 0.0], [30.0, 20.0]);
        let b = Curve::rect([40.0, 0.0], [30.0, 20.0]);
        let result = super::union(&a, &b);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&a, &b],
            |p| shape_inside(p, &a) || shape_inside(p, &b),
            (-50.0, -40.0, 90.0, 40.0),
        );
    }

    #[test]
    fn intersection_two_circles() {
        let a = Curve::circle([0.0, 0.0], 20.0);
        let b = Curve::circle([15.0, 0.0], 20.0);
        let result = super::intersection(&a, &b);
        assert!(result.is_closed());
        assert_matches(
            &result,
            &[&a, &b],
            |p| circ_inside(p, Vec2::ZERO, 20.0) && circ_inside(p, Vec2::new(15.0, 0.0), 20.0),
            (-30.0, -30.0, 45.0, 30.0),
        );
    }

    #[test]
    #[should_panic(expected = "closed contour")]
    fn open_contour_input_trips_assert() {
        // An open stroke is not a valid boolean operand; the closed-loop guard
        // must catch it in debug builds rather than silently mis-rendering.
        let open = Curve::shape([0.0, 0.0], FRAC_PI_2)
            .line(40.0)
            .line(40.0)
            .end();
        let square = Curve::rect([0.0, 0.0], [20.0, 20.0]);
        let _ = super::difference(&square, &open);
    }
}
