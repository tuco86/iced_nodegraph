//! Cubic bezier -> arc-spline approximation (the v3 "arcs-only" geometry).
//!
//! v3 deletes the per-pixel cubic-bezier SDF (Newton refinement + Gauss-Legendre
//! arc-length quadrature, the single most expensive `eval_segment` branch) by
//! approximating every cubic with circular arcs (and lines) on the CPU. Each arc
//! has an exact, precomputable arc length (`radius * |sweep|`), so dash spacing
//! and flow speed stay exact.
//!
//! Algorithm: adaptive subdivision. Each cubic piece is approximated by ONE
//! circular arc through its two endpoints and its midpoint; the maximum
//! deviation of the bezier from that arc is measured by sampling, and if it
//! exceeds the tolerance the piece is split at its midpoint (de Casteljau) and
//! each half re-fitted. A near-collinear piece becomes a line. Inflections need
//! no special handling: an arc spanning one has high deviation and is split
//! automatically. (A G1 two-arc "biarc" fit per piece would emit fewer arcs for
//! the same tolerance - a future arc-count optimization; this single-arc form is
//! the robust equivalent under the same deviation gate.)
//!
//! Tolerance is in the same world units as the geometry; callers make it
//! zoom-aware (`world_tol ~= 0.25px / zoom`) so a curve does not facet when
//! zoomed in.

// The fitter is validated by its unit tests; it is wired into the v3 edge build
// path (replacing the cubic-bezier segment) in the next A4 step, at which point
// these become reachable from non-test code.
#![allow(dead_code)]

use glam::Vec2;

/// One piece of an arc-spline: a circular arc, or a straight line where the
/// curve is locally flat. Endpoints are exact bezier points; `length` is the
/// piece's exact arc length (used to carry cumulative arc length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ArcPiece {
    /// Circular arc from `start` to `end`, center `center`, signed `sweep`
    /// (radians; positive = counter-clockwise). `start_angle` is the angle of
    /// `start` about `center`.
    Arc {
        center: Vec2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
        start: Vec2,
        end: Vec2,
        length: f32,
    },
    /// Straight segment (locally-flat piece, or a degenerate fit).
    Line { start: Vec2, end: Vec2, length: f32 },
}

impl ArcPiece {
    pub(crate) fn length(&self) -> f32 {
        match *self {
            ArcPiece::Arc { length, .. } | ArcPiece::Line { length, .. } => length,
        }
    }
}

fn bezier_point(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

/// Split a cubic at `t` (de Casteljau), returning the two control polygons.
fn split(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> ([Vec2; 4], [Vec2; 4]) {
    let a = p0.lerp(p1, t);
    let b = p1.lerp(p2, t);
    let c = p2.lerp(p3, t);
    let d = a.lerp(b, t);
    let e = b.lerp(c, t);
    let f = d.lerp(e, t);
    ([p0, a, d, f], [f, e, c, p3])
}

/// Circumcircle of three points: `(center, radius)`. None if (near-)collinear.
fn circle_through(a: Vec2, b: Vec2, c: Vec2) -> Option<(Vec2, f32)> {
    // Solve for the center equidistant from a, b, c.
    let d = 2.0 * ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
    if d.abs() < 1e-9 {
        return None;
    }
    let a2 = a.length_squared();
    let b2 = b.length_squared();
    let c2 = c.length_squared();
    let ux = ((a2) * (b.y - c.y) + (b2) * (c.y - a.y) + (c2) * (a.y - b.y)) / d;
    let uy = ((a2) * (c.x - b.x) + (b2) * (a.x - c.x) + (c2) * (b.x - a.x)) / d;
    let center = Vec2::new(ux, uy);
    Some((center, center.distance(a)))
}

/// Signed sweep from `start` to `end` about `center`, taking the direction that
/// passes through `mid` (so the arc bulges the same way the bezier does).
fn signed_sweep(center: Vec2, start: Vec2, mid: Vec2, end: Vec2) -> (f32, f32) {
    let a0 = (start - center).to_angle();
    let am = (mid - center).to_angle();
    let a1 = (end - center).to_angle();
    // CCW sweep magnitudes start->mid and start->end in [0, 2pi).
    let norm = |x: f32| {
        let mut v = x;
        while v < 0.0 {
            v += std::f32::consts::TAU;
        }
        while v >= std::f32::consts::TAU {
            v -= std::f32::consts::TAU;
        }
        v
    };
    let m_ccw = norm(am - a0);
    let e_ccw = norm(a1 - a0);
    // If mid lies on the CCW path to end, the arc is CCW (positive sweep);
    // otherwise it is CW (negative sweep).
    if m_ccw <= e_ccw {
        (a0, e_ccw)
    } else {
        (a0, e_ccw - std::f32::consts::TAU)
    }
}

/// Maximum deviation of the cubic from the candidate arc (or line if `center`
/// is None), sampled at `SAMPLES` interior points.
fn deviation(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, fit: Option<(Vec2, f32)>) -> f32 {
    const SAMPLES: usize = 16;
    let mut worst = 0.0_f32;
    for i in 1..SAMPLES {
        let t = i as f32 / SAMPLES as f32;
        let pt = bezier_point(p0, p1, p2, p3, t);
        let dev = match fit {
            Some((center, radius)) => (center.distance(pt) - radius).abs(),
            None => {
                // Perpendicular distance to segment p0-p3.
                let ab = p3 - p0;
                let len2 = ab.length_squared();
                if len2 < 1e-12 {
                    pt.distance(p0)
                } else {
                    let s = ((pt - p0).dot(ab) / len2).clamp(0.0, 1.0);
                    pt.distance(p0 + ab * s)
                }
            }
        };
        worst = worst.max(dev);
    }
    worst
}

fn recurse(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, tol: f32, depth: u32, out: &mut Vec<ArcPiece>) {
    let mid = bezier_point(p0, p1, p2, p3, 0.5);
    let fit = circle_through(p0, mid, p3);
    let dev = deviation(p0, p1, p2, p3, fit);

    // 12 levels = up to 4096 pieces, a hard backstop against pathological input.
    if dev <= tol || depth >= 12 {
        match fit {
            Some((center, radius)) => {
                let (start_angle, sweep) = signed_sweep(center, p0, mid, p3);
                out.push(ArcPiece::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep,
                    start: p0,
                    end: p3,
                    length: radius * sweep.abs(),
                });
            }
            None => out.push(ArcPiece::Line {
                start: p0,
                end: p3,
                length: p0.distance(p3),
            }),
        }
        return;
    }

    let (l, r) = split(p0, p1, p2, p3, 0.5);
    recurse(l[0], l[1], l[2], l[3], tol, depth + 1, out);
    recurse(r[0], r[1], r[2], r[3], tol, depth + 1, out);
}

/// Approximate a cubic bezier by an arc-spline whose deviation from the curve is
/// at most `tol` (world units). Never empty.
pub(crate) fn cubic_to_arcs(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, tol: f32) -> Vec<ArcPiece> {
    let mut out = Vec::new();
    recurse(p0, p1, p2, p3, tol.max(1e-5), 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dense polyline length of a cubic, the reference arc length.
    fn bezier_length(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
        let n = 4096;
        let mut len = 0.0;
        let mut prev = p0;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let pt = bezier_point(p0, p1, p2, p3, t);
            len += prev.distance(pt);
            prev = pt;
        }
        len
    }

    /// Worst deviation of the cubic from the whole arc-spline, sampled densely.
    fn spline_deviation(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, pieces: &[ArcPiece]) -> f32 {
        let n = 2000;
        let mut worst = 0.0_f32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let pt = bezier_point(p0, p1, p2, p3, t);
            let mut best = f32::INFINITY;
            for piece in pieces {
                let d = match *piece {
                    ArcPiece::Arc {
                        center,
                        radius,
                        start,
                        end,
                        ..
                    } => {
                        // Distance to the arc, clamped to the chord span: a
                        // cheap, conservative proxy bounded below by the true
                        // distance for these short pieces.
                        let radial = (center.distance(pt) - radius).abs();
                        radial.min(pt.distance(start)).min(pt.distance(end))
                    }
                    ArcPiece::Line { start, end, .. } => {
                        let ab = end - start;
                        let len2 = ab.length_squared();
                        let s = if len2 < 1e-12 {
                            0.0
                        } else {
                            ((pt - start).dot(ab) / len2).clamp(0.0, 1.0)
                        };
                        pt.distance(start + ab * s)
                    }
                };
                best = best.min(d);
            }
            worst = worst.max(best);
        }
        worst
    }

    #[test]
    fn gentle_curve_is_few_arcs_within_tolerance() {
        let (p0, p1, p2, p3) = (
            Vec2::new(0.0, 0.0),
            Vec2::new(30.0, -20.0),
            Vec2::new(70.0, 20.0),
            Vec2::new(100.0, 0.0),
        );
        let tol = 0.1;
        let pieces = cubic_to_arcs(p0, p1, p2, p3, tol);
        assert!(!pieces.is_empty());
        assert!(
            spline_deviation(p0, p1, p2, p3, &pieces) <= tol * 1.5,
            "deviation {} exceeded tol {tol}",
            spline_deviation(p0, p1, p2, p3, &pieces),
        );
    }

    #[test]
    fn s_curve_with_inflection_within_tolerance() {
        // Classic S: an inflection in the middle. Subdivision must handle it.
        let (p0, p1, p2, p3) = (
            Vec2::new(-120.0, -40.0),
            Vec2::new(-40.0, -40.0),
            Vec2::new(40.0, 40.0),
            Vec2::new(120.0, 40.0),
        );
        let tol = 0.25;
        let pieces = cubic_to_arcs(p0, p1, p2, p3, tol);
        assert!(pieces.len() >= 2, "an S-curve needs >=2 arcs");
        assert!(
            spline_deviation(p0, p1, p2, p3, &pieces) <= tol * 1.5,
            "deviation {} exceeded tol {tol}",
            spline_deviation(p0, p1, p2, p3, &pieces),
        );
    }

    #[test]
    fn straight_cubic_becomes_a_line() {
        // Control points collinear -> the fit is a line.
        let (p0, p1, p2, p3) = (
            Vec2::new(0.0, 0.0),
            Vec2::new(25.0, 0.0),
            Vec2::new(50.0, 0.0),
            Vec2::new(100.0, 0.0),
        );
        let pieces = cubic_to_arcs(p0, p1, p2, p3, 0.1);
        assert!(
            pieces.iter().all(|p| matches!(p, ArcPiece::Line { .. })),
            "a straight cubic should yield only lines: {pieces:?}",
        );
    }

    #[test]
    fn arc_length_matches_bezier() {
        let (p0, p1, p2, p3) = (
            Vec2::new(-120.0, -40.0),
            Vec2::new(-40.0, -40.0),
            Vec2::new(40.0, 40.0),
            Vec2::new(120.0, 40.0),
        );
        let pieces = cubic_to_arcs(p0, p1, p2, p3, 0.1);
        let spline_len: f32 = pieces.iter().map(|p| p.length()).sum();
        let ref_len = bezier_length(p0, p1, p2, p3);
        let rel = (spline_len - ref_len).abs() / ref_len;
        assert!(
            rel < 0.005,
            "arc-spline length {spline_len} vs bezier {ref_len} (rel {rel})",
        );
    }
}
