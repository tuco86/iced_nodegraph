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
///
/// Computed in a frame translated so `a` is at the origin. Edges are fit in WORLD
/// coordinates, and the circumcenter formula squares the operands; with absolute
/// coordinates far from the origin those squares are huge and their differences
/// (which should be small) lose f32 precision catastrophically, collapsing the
/// fit into a giant or near-full-circle arc. Translating to a local frame keeps
/// the squared terms small and the result stable.
fn circle_through(a: Vec2, b: Vec2, c: Vec2) -> Option<(Vec2, f32)> {
    let bl = b - a;
    let cl = c - a;
    let d = 2.0 * (bl.x * cl.y - bl.y * cl.x);
    if d.abs() < 1e-9 {
        return None;
    }
    let b2 = bl.length_squared();
    let c2 = cl.length_squared();
    let ux = (cl.y * b2 - bl.y * c2) / d;
    let uy = (bl.x * c2 - cl.x * b2) / d;
    let local_center = Vec2::new(ux, uy);
    let radius = local_center.length();
    // A giant radius means the three points are NEAR-collinear (the exact-collinear
    // `d ~ 0` case rounds to a huge but finite circle, not None). Such a fit is
    // numerically meaningless: at radius ~1e9 the deviation gate's
    // `|dist - radius|` rounds to 0 in f32 (ulp >> tol), so a wildly-off "arc" is
    // accepted, and `from_center_arc` then cancels it (1e9 - 1e9) into a
    // zero-length point at the origin - an edge vanishing / snapping to a line.
    // Treat it as collinear: return None so the caller splits (or takes the chord).
    // 1e5 is far above any real edge arc (those have sub-pixel sagitta past it, so
    // chord-first already lines them) and well below the f32-cancellation zone.
    if !radius.is_finite() || radius > 1.0e5 {
        return None;
    }
    Some((a + local_center, radius))
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

/// A single arc may never bend more than this in one piece. A cubic sub-piece
/// with monotone curvature bends well under half a turn; a fit that reports a
/// larger sweep is the noisy near-straight / wrong-direction case that renders
/// as a giant arc or full circle (the deviation gate is blind to it, since it
/// only measures distance to the CIRCLE, not the chosen ARC). Such a piece is
/// subdivided instead, so the artifact cannot reach the GPU.
const MAX_ARC_SWEEP: f32 = std::f32::consts::PI * 0.95;

fn line_piece(start: Vec2, end: Vec2) -> ArcPiece {
    ArcPiece::Line {
        start,
        end,
        length: start.distance(end),
    }
}

fn recurse(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, tol: f32, depth: u32, out: &mut Vec<ArcPiece>) {
    // Chord-first: a piece within tolerance of its straight chord is a line.
    // This is exact and stable, and it sidesteps the unstable arc fit a
    // near-straight piece induces (a far-off circle center whose sweep direction
    // is numerical noise) - the source of the "giant arc" artifact.
    if deviation(p0, p1, p2, p3, None) <= tol {
        out.push(line_piece(p0, p3));
        return;
    }

    let mid = bezier_point(p0, p1, p2, p3, 0.5);
    let fit = circle_through(p0, mid, p3);
    let arc = fit.map(|(center, radius)| {
        let (start_angle, sweep) = signed_sweep(center, p0, mid, p3);
        (center, radius, start_angle, sweep)
    });
    let dev = deviation(p0, p1, p2, p3, fit);
    let sweep_ok = arc.is_some_and(|(_, _, _, sweep)| sweep.abs() <= MAX_ARC_SWEEP);

    // 12 levels = up to 4096 pieces, a hard backstop against pathological input.
    let terminal = depth >= 12;
    if sweep_ok && (dev <= tol || terminal) {
        let (center, radius, start_angle, sweep) = arc.unwrap();
        out.push(ArcPiece::Arc {
            center,
            radius,
            start_angle,
            sweep,
            start: p0,
            end: p3,
            length: radius * sweep.abs(),
        });
        return;
    }
    if terminal {
        // Out of subdivision budget with no sane arc: a chord line is the safe
        // last resort (the piece is tiny at this depth, so the error is small).
        out.push(line_piece(p0, p3));
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
    fn far_from_origin_curve_within_tolerance() {
        // Edges are built at WORLD coordinates (not localized), so a curve far
        // from the origin must fit just as well as one at the origin. If the
        // circumcircle is computed in absolute coords, the large squared terms
        // lose f32 precision and the fit collapses to a giant/near-full-circle
        // arc - the reported "some edges become full circles" bug.
        // Sweep an offset AND a near-straight backward edge (the common
        // node-graph case: output-right to input-left, horizontal tangents).
        let tol = 0.25;
        for &off in &[
            Vec2::new(0.0, 0.0),
            Vec2::new(1800.0, -1200.0),
            Vec2::new(40000.0, 25000.0),
        ] {
            // A gentle near-straight edge: tiny bow over a long span. This is the
            // case that fits a huge-radius circle whose sweep direction is noisy.
            let curves = [
                (
                    Vec2::new(-150.0, 0.0),
                    Vec2::new(-50.0, 1.0),
                    Vec2::new(50.0, -1.0),
                    Vec2::new(150.0, 0.0),
                ),
                (
                    Vec2::new(-120.0, -40.0),
                    Vec2::new(-40.0, -40.0),
                    Vec2::new(40.0, 40.0),
                    Vec2::new(120.0, 40.0),
                ),
            ];
            for (a, b, c, d) in curves {
                let (p0, p1, p2, p3) = (a + off, b + off, c + off, d + off);
                let pieces = cubic_to_arcs(p0, p1, p2, p3, tol);
                let dev = spline_deviation(p0, p1, p2, p3, &pieces);
                assert!(
                    dev <= tol * 1.5,
                    "off={off:?} deviation {dev} exceeded tol {tol}; pieces={pieces:?}",
                );
                for piece in &pieces {
                    if let ArcPiece::Arc { radius, sweep, .. } = *piece {
                        assert!(
                            sweep.abs() < std::f32::consts::PI,
                            "off={off:?}: arc sweep {sweep} implausibly large; pieces={pieces:?}",
                        );
                        assert!(
                            radius < 1.0e6,
                            "off={off:?}: arc radius {radius} absurd; pieces={pieces:?}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn backward_edge_loop_no_giant_arc() {
        // The real failure: a "backward" edge (output-right pin to a target node
        // to the LEFT) has control points pointing away from each other, so the
        // cubic loops. p0->cp0 heads right, cp1->p3 heads left from far left.
        let tol = 0.05; // the widget's edge tolerance
        for &off in &[Vec2::new(0.0, 0.0), Vec2::new(2400.0, -900.0)] {
            let p0 = Vec2::new(0.0, 0.0) + off;
            let cp0 = Vec2::new(80.0, 0.0) + off;
            let cp1 = Vec2::new(-280.0, 50.0) + off;
            let p3 = Vec2::new(-200.0, 50.0) + off;
            let pieces = cubic_to_arcs(p0, cp0, cp1, p3, tol);
            let dev = spline_deviation(p0, cp0, cp1, p3, &pieces);
            assert!(
                dev <= tol * 2.0,
                "off={off:?} backward-edge deviation {dev} exceeded tol {tol}; \
                 pieces={pieces:?}",
            );
            for piece in &pieces {
                if let ArcPiece::Arc { radius, sweep, .. } = *piece {
                    assert!(
                        sweep.abs() <= std::f32::consts::PI + 1e-3,
                        "off={off:?}: giant sweep {sweep}; pieces={pieces:?}",
                    );
                    assert!(radius < 1.0e6, "off={off:?}: absurd radius {radius}");
                }
            }
        }
    }

    #[test]
    fn widget_edge_configs_never_giant_arc_or_full_circle() {
        // Brute-force the exact edge geometry the widget builds: every pin-side
        // tangent pair, endpoints in all relative directions (forward, backward,
        // vertical, diagonal, short, long), at the origin and far from it. The
        // arc-spline of every such cubic must stay within tolerance and must
        // never contain a giant-radius or near-full-circle arc - the two reported
        // artifacts. This is the permanent regression guard for the edge bug.
        let dirs = [
            Vec2::new(-1.0, 0.0), // Left
            Vec2::new(1.0, 0.0),  // Right
            Vec2::new(0.0, -1.0), // Top
            Vec2::new(0.0, 1.0),  // Bottom
        ];
        let deltas = [
            Vec2::new(200.0, 0.0),
            Vec2::new(-200.0, 0.0), // backward
            Vec2::new(0.0, 150.0),
            Vec2::new(0.0, -150.0),
            Vec2::new(-180.0, 60.0), // backward + offset
            Vec2::new(220.0, -140.0),
            Vec2::new(15.0, 5.0),    // very short
            Vec2::new(-12.0, -40.0), // short backward
        ];
        let offsets = [
            Vec2::ZERO,
            Vec2::new(1500.0, -900.0),
            Vec2::new(-3000.0, 2000.0),
        ];
        let tol = 0.05_f32; // the widget's edge tolerance

        let mut worst_dev = 0.0_f32;
        let mut max_sweep = 0.0_f32;
        let mut max_radius = 0.0_f32;
        for &off in &offsets {
            for &delta in &deltas {
                for &df in &dirs {
                    for &dt in &dirs {
                        let p0 = off;
                        let p3 = off + delta;
                        // mirror adaptive_bezier_length: min(80, half dist, >=1).
                        let l = 80.0_f32.min(delta.length() * 0.5).max(1.0);
                        let cp0 = p0 + df * l;
                        let cp1 = p3 + dt * l;
                        let pieces = cubic_to_arcs(p0, cp0, cp1, p3, tol);
                        assert!(!pieces.is_empty());
                        let dev = spline_deviation(p0, cp0, cp1, p3, &pieces);
                        worst_dev = worst_dev.max(dev);
                        for piece in &pieces {
                            if let ArcPiece::Arc { radius, sweep, .. } = *piece {
                                max_sweep = max_sweep.max(sweep.abs());
                                max_radius = max_radius.max(radius);
                                assert!(
                                    sweep.abs() <= std::f32::consts::PI,
                                    "FULL-CIRCLE artifact: sweep={sweep} off={off:?} \
                                     delta={delta:?} df={df:?} dt={dt:?}; pieces={pieces:?}",
                                );
                                assert!(
                                    radius < 1.0e6,
                                    "GIANT-ARC artifact: radius={radius} off={off:?} \
                                     delta={delta:?} df={df:?} dt={dt:?}",
                                );
                            }
                        }
                        assert!(
                            dev <= tol * 3.0,
                            "deviation {dev} off={off:?} delta={delta:?} df={df:?} dt={dt:?}",
                        );
                    }
                }
            }
        }
        // Sanity: the suite actually exercised arcs (not all lines).
        assert!(max_sweep > 0.0, "no arcs were produced - test is vacuous");
        eprintln!(
            "edge configs: worst_dev={worst_dev:.4} max_sweep={max_sweep:.3} \
             max_radius={max_radius:.1}"
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
