//! Unified arc segment math - the "Arc is all you need" encoding.
//!
//! Every drawn segment is ONE arc, encoded by its ENDPOINTS plus a signed
//! curvature. The three forms are degenerates of the same primitive:
//!
//! - `curvature == 0`        -> straight line `start -> end`
//! - `start == end`          -> point (junction marker); `heading` gives the sign
//! - otherwise               -> circular arc of radius `1/|curvature|`, the MINOR
//!   arc (`|sweep| < PI`) bulging to the side selected by `curvature`'s sign
//!
//! Why endpoints, not a center/radius/sweep form: a straight line has no finite
//! center, so a center-based encoding cannot express a line as a degenerate arc
//! (the limit is `radius -> infinity`, which is unstorable and the source of the
//! old "giant arc / full circle" artifacts). Endpoints degenerate to a line
//! cleanly at `curvature = 0`, and keep geometry near its own coordinates,
//! avoiding the far-from-origin precision loss the center form fights.
//!
//! Arcs of `|sweep| >= PI` (a full-circle pin is `2*PI`) are split into sub-arcs
//! below `PI` before reaching this encoding, so the minor-arc reconstruction in
//! [`seg_sdf`] is always unambiguous.

use glam::Vec2;
use std::f32::consts::{PI, TAU};

/// `|curvature|` at or below this is treated as a straight line.
pub(crate) const LINE_EPS: f32 = 1e-6;
/// `|end - start|` at or below this is treated as a point.
pub(crate) const POINT_EPS: f32 = 1e-5;

/// Convert a legacy center/radius/start-angle/sweep arc to the endpoint +
/// signed-curvature form. The sweep must already be below `PI` in magnitude
/// (callers split wider arcs first); the sign of the returned curvature encodes
/// which side of the chord the center sits on, which is what lets [`seg_sdf`]
/// reconstruct the exact same minor arc.
pub(crate) fn from_center_arc(
    center: Vec2,
    radius: f32,
    start_angle: f32,
    sweep: f32,
) -> (Vec2, Vec2, f32) {
    debug_assert!(
        sweep.abs() < PI + 1e-3,
        "arc sweep {sweep} must be split below PI before encoding",
    );
    let start = center + Vec2::new(start_angle.cos(), start_angle.sin()) * radius;
    let end_a = start_angle + sweep;
    let end = center + Vec2::new(end_a.cos(), end_a.sin()) * radius;

    let d = end - start;
    let l = d.length();
    let curvature = if l < POINT_EPS || radius <= 0.0 {
        0.0
    } else {
        let u = d / l;
        let n = Vec2::new(-u.y, u.x);
        let m = (start + end) * 0.5;
        let side = (center - m).dot(n).signum();
        side / radius
    };
    (start, end, curvature)
}

/// Signed distance from `p` to the unified segment (negative = interior side).
///
/// Reproduces the legacy `sd_line` / `sd_arc_segment` / `sd_point` fields exactly
/// for the round-tripped encoding - this equivalence is the regression oracle in
/// the tests, and the contract the GPU port must keep.
pub(crate) fn seg_sdf(p: Vec2, start: Vec2, end: Vec2, curvature: f32, heading: f32) -> f32 {
    let d = end - start;
    let l = d.length();

    // Point: zero-length junction, sign from the interior heading.
    if l < POINT_EPS {
        let dist = ((p - start).length() - 0.01).max(0.0);
        let right = Vec2::new(heading.cos(), heading.sin());
        return if (p - start).dot(right) > 0.0 { -dist } else { dist };
    }

    // Line: signed distance to the segment, sign from the perpendicular side.
    if curvature.abs() < LINE_EPS {
        let pa = p - start;
        let t = (pa.dot(d) / d.dot(d)).clamp(0.0, 1.0);
        let dist = (p - (start + d * t)).length();
        let n = Vec2::new(-d.y, d.x);
        return if pa.dot(n) > 0.0 { -dist } else { dist };
    }

    // Arc: reconstruct center + minor sweep from endpoints + signed curvature,
    // then evaluate the legacy arc field.
    let r = 1.0 / curvature.abs();
    let center = arc_center(start, end, curvature).unwrap_or((start + end) * 0.5);
    let a_start = (start - center).y.atan2((start - center).x);
    let sweep = arc_minor_sweep(start, end, center);

    let offset = p - center;
    let dtc = offset.length();
    let angle = offset.y.atan2(offset.x);
    let rel = if sweep > 0.0 {
        (angle - a_start).rem_euclid(TAU)
    } else {
        (angle - a_start) - ((angle - a_start) / TAU).ceil() * TAU
    };
    let on_arc = (sweep > 0.0 && rel <= sweep) || (sweep < 0.0 && rel >= sweep);
    if on_arc {
        let dist = (dtc - r).abs();
        let v = if sweep > 0.0 { r - dtc } else { -(r - dtc) };
        if v > 0.0 { -dist } else { dist }
    } else {
        let p_start = start;
        let p_end = end;
        let (pt, base) = if (p - p_start).length() < (p - p_end).length() {
            (p_start, a_start)
        } else {
            (p_end, a_start + sweep)
        };
        let dist = (p - pt).length();
        let tangent = Vec2::new(-base.sin(), base.cos()) * sweep.signum();
        let nn = Vec2::new(-tangent.y, tangent.x);
        if (p - pt).dot(nn) > 0.0 { -dist } else { dist }
    }
}

/// Center of the circle carrying the arc, reconstructed from endpoints + signed
/// curvature. `None` for a line (`curvature ~ 0`) or a point (`start == end`).
pub(crate) fn arc_center(start: Vec2, end: Vec2, curvature: f32) -> Option<Vec2> {
    let d = end - start;
    let l = d.length();
    if l < POINT_EPS || curvature.abs() < LINE_EPS {
        return None;
    }
    let r = 1.0 / curvature.abs();
    let u = d / l;
    let n = Vec2::new(-u.y, u.x);
    let h = (r * r - (l * 0.5) * (l * 0.5)).max(0.0).sqrt();
    Some((start + end) * 0.5 + n * (curvature.signum() * h))
}

/// Signed minor sweep (|sweep| < PI) from `start` to `end` about `center`.
fn arc_minor_sweep(start: Vec2, end: Vec2, center: Vec2) -> f32 {
    let a_start = (start - center).y.atan2((start - center).x);
    let a_end = (end - center).y.atan2((end - center).x);
    let mut sweep = a_end - a_start;
    if sweep <= -PI {
        sweep += TAU;
    } else if sweep > PI {
        sweep -= TAU;
    }
    sweep
}

/// Exact world-space AABB `(min, max)` of the segment: endpoints plus, for an
/// arc, any cardinal extreme its sweep actually crosses (a tight cull bound).
pub(crate) fn seg_aabb(start: Vec2, end: Vec2, curvature: f32) -> (Vec2, Vec2) {
    let mut lo = start.min(end);
    let mut hi = start.max(end);
    if let Some(center) = arc_center(start, end, curvature) {
        let r = 1.0 / curvature.abs();
        let a_start = (start - center).y.atan2((start - center).x);
        let sweep = arc_minor_sweep(start, end, center);
        for k in 0..4 {
            let theta = k as f32 * (PI * 0.5);
            let mut rel = theta - a_start;
            if sweep >= 0.0 {
                rel = rel.rem_euclid(TAU);
                if rel > sweep {
                    continue;
                }
            } else {
                rel -= (rel / TAU).ceil() * TAU;
                if rel < sweep {
                    continue;
                }
            }
            let pt = center + Vec2::new(theta.cos(), theta.sin()) * r;
            lo = lo.min(pt);
            hi = hi.max(pt);
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Legacy reference fields (mirror boolean.rs cpu_sd_* / the shader) ---

    fn ref_line(p: Vec2, a: Vec2, b: Vec2) -> f32 {
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
        if len_sq > 0.0 && pa.dot(n) > 0.0 { -dist } else { dist }
    }

    fn ref_arc(p: Vec2, center: Vec2, radius: f32, start: f32, sweep: f32) -> f32 {
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
            let v = if sweep > 0.0 { radius - dtc } else { -(radius - dtc) };
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

    fn ref_point(p: Vec2, pos: Vec2, heading: f32) -> f32 {
        let dist = ((p - pos).length() - 0.01).max(0.0);
        let right = Vec2::new(heading.cos(), heading.sin());
        if (p - pos).dot(right) > 0.0 { -dist } else { dist }
    }

    /// The endpoint+curvature arc field equals the legacy center/radius/sweep
    /// field everywhere, for every radius / orientation / sweep direction the
    /// renderer produces (all minor arcs, |sweep| < PI). This is the proof the
    /// new encoding is lossless - the spine of the arc-only migration.
    #[test]
    fn endpoint_arc_field_matches_legacy_center_arc() {
        let mut worst = 0.0_f32;
        for &radius in &[5.0_f32, 20.0, 100.0] {
            for si in 0..8 {
                let start_angle = si as f32 * (TAU / 8.0) + 0.3;
                for &sweep in &[
                    -2.8_f32, -1.7, -PI / 2.0, -0.4, 0.4, PI / 2.0, 1.7, 2.8,
                ] {
                    let (s, e, k) = from_center_arc(Vec2::new(13.0, -7.0), radius, start_angle, sweep);
                    let mut x = -130.0;
                    while x <= 130.0 {
                        let mut y = -130.0;
                        while y <= 130.0 {
                            let p = Vec2::new(x, y);
                            let a = ref_arc(p, Vec2::new(13.0, -7.0), radius, start_angle, sweep);
                            let b = seg_sdf(p, s, e, k, 0.0);
                            worst = worst.max((a - b).abs());
                            y += 3.7;
                        }
                        x += 3.7;
                    }
                }
            }
        }
        assert!(worst < 1e-2, "endpoint arc field deviates from legacy by {worst}");
    }

    #[test]
    fn line_is_zero_curvature_arc() {
        let (a, b) = (Vec2::new(-40.0, 10.0), Vec2::new(60.0, -25.0));
        let mut worst = 0.0_f32;
        let mut x = -80.0;
        while x <= 100.0 {
            let mut y = -70.0;
            while y <= 60.0 {
                let p = Vec2::new(x, y);
                worst = worst.max((ref_line(p, a, b) - seg_sdf(p, a, b, 0.0, 0.0)).abs());
                y += 2.3;
            }
            x += 2.3;
        }
        assert!(worst < 1e-4, "line field deviates by {worst}");
    }

    #[test]
    fn point_is_zero_length_arc() {
        let pos = Vec2::new(8.0, -3.0);
        for hi in 0..8 {
            let heading = hi as f32 * (TAU / 8.0);
            let mut x = -20.0;
            while x <= 36.0 {
                let mut y = -28.0;
                while y <= 22.0 {
                    let p = Vec2::new(x, y);
                    let a = ref_point(p, pos, heading);
                    let b = seg_sdf(p, pos, pos, 0.0, heading);
                    assert!((a - b).abs() < 1e-4, "point field deviates at {p:?}");
                    y += 2.9;
                }
                x += 2.9;
            }
        }
    }
}
