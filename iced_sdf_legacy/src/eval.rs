//! CPU-side SDF evaluation for hit-testing.
//!
//! Mirrors the GPU shader logic for use in interaction handling.

use std::f32::consts::PI;

use glam::Vec2;

use crate::shape::SdfNode;

/// Result of SDF evaluation.
#[derive(Clone, Copy, Debug)]
pub struct SdfResult {
    /// Signed distance to the shape boundary.
    pub dist: f32,
    /// Arc-length parameter along the shape contour.
    pub u: f32,
}

impl SdfResult {
    /// Create a new SDF result.
    pub fn new(dist: f32, u: f32) -> Self {
        Self { dist, u }
    }

    /// Check if point is inside the shape (negative distance).
    pub fn is_inside(&self) -> bool {
        self.dist < 0.0
    }

    /// Check if point is within threshold distance of the shape.
    pub fn is_near(&self, threshold: f32) -> bool {
        self.dist.abs() < threshold
    }
}

/// Evaluate an SDF at a given point.
pub fn evaluate(node: &SdfNode, point: Vec2) -> SdfResult {
    match node {
        SdfNode::Circle { center, radius } => sd_circle(point, *center, *radius),
        SdfNode::Box { center, half_size } => sd_box(point, *center, *half_size),
        SdfNode::RoundedBox {
            center,
            half_size,
            corner_radius,
        } => sd_rounded_box(point, *center, *half_size, *corner_radius),
        SdfNode::Line { a, b } => sd_line(point, *a, *b),
        SdfNode::Bezier { p0, p1, p2, p3 } => sd_bezier(point, *p0, *p1, *p2, *p3),

        SdfNode::Ellipse { ab } => SdfResult::new(sd_ellipse(point, *ab), 0.0),
        SdfNode::Triangle { p0, p1, p2 } => {
            let d0 = sd_line(point, *p0, *p1).dist;
            let d1 = sd_line(point, *p1, *p2).dist;
            let d2 = sd_line(point, *p2, *p0).dist;
            let e0 = *p1 - *p0;
            let e2 = *p0 - *p2;
            let s = (e0.x * e2.y - e0.y * e2.x).signum();
            let v0 = point - *p0;
            let inside = s * (v0.x * e0.y - v0.y * e0.x) >= 0.0;
            let min_d = d0.min(d1).min(d2);
            SdfResult::new(if inside { -min_d } else { min_d }, 0.0)
        }
        SdfNode::QuadBezier { p0, p1, p2 } => {
            SdfResult::new(sd_quad_bezier(point, *p0, *p1, *p2), 0.0)
        }
        SdfNode::EquilateralTriangle { radius } => {
            SdfResult::new(sd_equilateral_triangle(point, *radius), 0.0)
        }
        SdfNode::IsoscelesTriangle { q } => {
            SdfResult::new(sd_isosceles_triangle(point, *q), 0.0)
        }
        SdfNode::Rhombus { b } => SdfResult::new(sd_rhombus(point, *b), 0.0),
        SdfNode::Trapezoid { r1, r2, he } => {
            SdfResult::new(sd_trapezoid(point, *r1, *r2, *he), 0.0)
        }
        SdfNode::Parallelogram { wi, he, sk } => {
            SdfResult::new(sd_parallelogram(point, *wi, *he, *sk), 0.0)
        }
        SdfNode::Pentagon { radius } => SdfResult::new(sd_pentagon(point, *radius), 0.0),
        SdfNode::Hexagon { radius } => SdfResult::new(sd_hexagon(point, *radius), 0.0),
        SdfNode::Octagon { radius } => SdfResult::new(sd_octagon(point, *radius), 0.0),
        SdfNode::Hexagram { radius } => SdfResult::new(sd_hexagram(point, *radius), 0.0),
        SdfNode::Star { radius, n, m } => {
            SdfResult::new(sd_star(point, *radius, *n, *m), 0.0)
        }
        SdfNode::Pie { angle, radius } => {
            let sc = Vec2::new(angle.sin(), angle.cos());
            SdfResult::new(sd_pie(point, sc, *radius), 0.0)
        }
        SdfNode::Arc { angle, ra, rb } => {
            let sc = Vec2::new(angle.sin(), angle.cos());
            SdfResult::new(sd_arc(point, sc, *ra, *rb), 0.0)
        }
        SdfNode::CutDisk { radius, h } => {
            SdfResult::new(sd_cut_disk(point, *radius, *h), 0.0)
        }
        SdfNode::Heart => SdfResult::new(sd_heart(point), 0.0),
        SdfNode::Egg { ra, rb } => SdfResult::new(sd_egg(point, *ra, *rb), 0.0),
        SdfNode::Moon { d, ra, rb } => SdfResult::new(sd_moon(point, *d, *ra, *rb), 0.0),
        SdfNode::Vesica { r, d } => SdfResult::new(sd_vesica(point, *r, *d), 0.0),
        SdfNode::UnevenCapsule { r1, r2, h } => {
            SdfResult::new(sd_uneven_capsule(point, *r1, *r2, *h), 0.0)
        }
        SdfNode::OrientedBox { a, b, thickness } => {
            SdfResult::new(sd_oriented_box(point, *a, *b, *thickness), 0.0)
        }
        SdfNode::Horseshoe { angle, radius, w } => {
            let sc = Vec2::new(angle.sin(), angle.cos());
            SdfResult::new(sd_horseshoe(point, sc, *radius, *w), 0.0)
        }
        SdfNode::RoundedX { w, r } => SdfResult::new(sd_rounded_x(point, *w, *r), 0.0),
        SdfNode::Cross { b, r } => SdfResult::new(sd_cross(point, *b, *r), 0.0),
        SdfNode::Parabola { k } => SdfResult::new(sd_parabola(point, *k), 0.0),
        SdfNode::CoolS => SdfResult::new(sd_cool_s(point), 0.0),
        SdfNode::BlobbyCross { he } => SdfResult::new(sd_blobby_cross(point, *he), 0.0),

        SdfNode::Union(a, b) => {
            let ra = evaluate(a, point);
            let rb = evaluate(b, point);
            op_union(ra, rb)
        }
        SdfNode::Subtract(a, b) => {
            let ra = evaluate(a, point);
            let rb = evaluate(b, point);
            op_subtract(ra, rb)
        }
        SdfNode::Intersect(a, b) => {
            let ra = evaluate(a, point);
            let rb = evaluate(b, point);
            op_intersect(ra, rb)
        }
        SdfNode::SmoothUnion { a, b, k } => {
            let ra = evaluate(a, point);
            let rb = evaluate(b, point);
            op_smooth_union(ra, rb, *k)
        }
        SdfNode::SmoothSubtract { a, b, k } => {
            let ra = evaluate(a, point);
            let rb = evaluate(b, point);
            op_smooth_subtract(ra, rb, *k)
        }

        SdfNode::Round { node, radius } => {
            let r = evaluate(node, point);
            op_round(r, *radius)
        }
        SdfNode::Onion { node, thickness } => {
            let r = evaluate(node, point);
            op_onion(r, *thickness)
        }

        SdfNode::Dash {
            node,
            dash,
            gap,
            thickness,
            angle,
            speed: _,
        } => {
            let r = evaluate(node, point);
            let perimeter = node.perimeter().unwrap_or(0.0);
            op_dash(r, *dash, *gap, *thickness, *angle, perimeter)
        }
        SdfNode::Arrow {
            node,
            segment,
            gap,
            thickness,
            angle,
            speed: _,
        } => {
            let r = evaluate(node, point);
            let perimeter = node.perimeter().unwrap_or(0.0);
            op_arrow(r, *segment, *gap, *thickness, *angle, perimeter)
        }
    }
}

// Helper functions

fn ndot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x - a.y * b.y
}

fn dot2(v: Vec2) -> f32 {
    v.dot(v)
}

fn signed_pow(x: f32, e: f32) -> f32 {
    x.signum() * x.abs().powf(e)
}

// Primitive SDFs

fn sd_circle(p: Vec2, center: Vec2, radius: f32) -> SdfResult {
    let d = (p - center).length() - radius;
    let angle = (p.y - center.y).atan2(p.x - center.x);
    let u = (angle + PI) * radius;
    SdfResult::new(d, u)
}

fn sd_box(p: Vec2, center: Vec2, half_size: Vec2) -> SdfResult {
    let q = (p - center).abs() - half_size;
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);

    let rel = p - center;
    let w = half_size.x;
    let h = half_size.y;

    let u = if (rel.y + h).abs() < 0.001 && rel.x.abs() <= w {
        2.0 * w + 2.0 * h + (w - rel.x)
    } else if (rel.x - w).abs() < 0.001 && rel.y.abs() <= h {
        w + (h - rel.y)
    } else if (rel.y - h).abs() < 0.001 && rel.x.abs() <= w {
        w + rel.x
    } else {
        2.0 * w + h + (h + rel.y)
    };

    SdfResult::new(d, u)
}

fn sd_rounded_box(p: Vec2, center: Vec2, half_size: Vec2, r: f32) -> SdfResult {
    let q = (p - center).abs() - half_size + r;
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r;
    let base = sd_box(p, center, half_size);
    SdfResult::new(d, base.u)
}

fn sd_line(p: Vec2, a: Vec2, b: Vec2) -> SdfResult {
    let pa = p - a;
    let ba = b - a;
    let h = (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0);
    let d = (pa - ba * h).length();
    let u = h * ba.length();
    SdfResult::new(d, u)
}

fn sd_bezier(p: Vec2, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> SdfResult {
    let num_samples = 16;
    let mut min_dist = f32::MAX;
    let mut best_t = 0.0;

    for i in 0..=num_samples {
        let t = i as f32 / num_samples as f32;
        let pos = cubic_bezier(p0, p1, p2, p3, t);
        let dist = (p - pos).length();
        if dist < min_dist {
            min_dist = dist;
            best_t = t;
        }
    }

    let dt = 1.0 / num_samples as f32;
    for i in -4..=4 {
        let t = (best_t + i as f32 * dt * 0.25).clamp(0.0, 1.0);
        let pos = cubic_bezier(p0, p1, p2, p3, t);
        let dist = (p - pos).length();
        if dist < min_dist {
            min_dist = dist;
            best_t = t;
        }
    }

    let curve_length = (p1 - p0).length() + (p2 - p1).length() + (p3 - p2).length();
    let u = best_t * curve_length;

    SdfResult::new(min_dist, u)
}

fn cubic_bezier(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let mt = 1.0 - t;
    mt * mt * mt * p0 + 3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t * p3
}

// Exact SDF implementations ported from WGSL shader

fn sd_ellipse(p_in: Vec2, ab_in: Vec2) -> f32 {
    let mut p = p_in.abs();
    let mut ab = ab_in;
    if p.x > p.y {
        p = Vec2::new(p.y, p.x);
        ab = Vec2::new(ab.y, ab.x);
    }
    let l = ab.y * ab.y - ab.x * ab.x;
    let m = ab.x * p.x / l;
    let m2 = m * m;
    let n = ab.y * p.y / l;
    let n2 = n * n;
    let c = (m2 + n2 - 1.0) / 3.0;
    let c3 = c * c * c;
    let q = c3 + m2 * n2 * 2.0;
    let d = c3 + m2 * n2;
    let g = m + m * n2;
    let co = if d < 0.0 {
        let h = (q / c3).acos() / 3.0;
        let s = h.cos();
        let t = h.sin() * 3.0_f32.sqrt();
        let rx = (-c * (s + t + 2.0) + m2).sqrt();
        let ry = (-c * (s - t + 2.0) + m2).sqrt();
        (ry + l.signum() * rx + g.abs() / (rx * ry) - m) / 2.0
    } else {
        let h = 2.0 * m * n * d.sqrt();
        let s = signed_pow(q + h, 1.0 / 3.0);
        let u = signed_pow(q - h, 1.0 / 3.0);
        let rx = -s - u - c * 4.0 + 2.0 * m2;
        let ry = (s - u) * 3.0_f32.sqrt();
        let rm = (rx * rx + ry * ry).sqrt();
        (ry / (rm - rx).sqrt() + 2.0 * g / rm - m) / 2.0
    };
    let r = ab * Vec2::new(co, (1.0 - co * co).sqrt());
    (r - p).length() * (p.y - r.y).signum()
}

fn sd_equilateral_triangle(p_in: Vec2, r: f32) -> f32 {
    let k = 3.0_f32.sqrt();
    let mut p = p_in;
    p.x = p.x.abs() - r;
    p.y += r / k;
    if p.x + k * p.y > 0.0 {
        p = Vec2::new(p.x - k * p.y, -k * p.x - p.y) / 2.0;
    }
    p.x -= p.x.clamp(-2.0 * r, 0.0);
    -p.length() * p.y.signum()
}

fn sd_isosceles_triangle(p_in: Vec2, q: Vec2) -> f32 {
    let mut p = p_in;
    p.x = p.x.abs();
    let a = p - q * (p.dot(q) / q.dot(q)).clamp(0.0, 1.0);
    let b = p - q * Vec2::new((p.x / q.x).clamp(0.0, 1.0), 1.0);
    let s = -q.y.signum();
    let da = Vec2::new(dot2(a), s * (p.x * q.y - p.y * q.x));
    let db = Vec2::new(dot2(b), s * (p.y - q.y));
    let d = da.min(db);
    -d.x.sqrt() * d.y.signum()
}

fn sd_rhombus(p_in: Vec2, b: Vec2) -> f32 {
    let p = p_in.abs();
    let h = (ndot(b - 2.0 * p, b) / b.dot(b)).clamp(-1.0, 1.0);
    let d = (p - 0.5 * b * Vec2::new(1.0 - h, 1.0 + h)).length();
    d * (p.x * b.y + p.y * b.x - b.x * b.y).signum()
}

fn sd_trapezoid(p_in: Vec2, r1: f32, r2: f32, he: f32) -> f32 {
    let mut p = p_in;
    let k1 = Vec2::new(r2, he);
    let k2 = Vec2::new(r2 - r1, 2.0 * he);
    p.x = p.x.abs();
    let ca = Vec2::new(
        p.x - p.x.min(if p.y < 0.0 { r1 } else { r2 }),
        p.y.abs() - he,
    );
    let cb = p - k1 + k2 * ((k1 - p).dot(k2) / k2.dot(k2)).clamp(0.0, 1.0);
    let s = if cb.x < 0.0 && ca.y < 0.0 {
        -1.0
    } else {
        1.0
    };
    s * dot2(ca).min(dot2(cb)).sqrt()
}

fn sd_parallelogram(p_in: Vec2, wi: f32, he: f32, sk: f32) -> f32 {
    let e = Vec2::new(sk, he);
    let mut p = if p_in.y < 0.0 { -p_in } else { p_in };
    let mut w = p - e;
    w.x = w.x - w.x.clamp(-wi, wi);
    let mut d = Vec2::new(dot2(w), -w.y);
    let s = p.x * e.y - p.y * e.x;
    if s < 0.0 {
        p = -p;
    }
    let mut v = p - Vec2::new(wi, 0.0);
    v = v - e * (v.dot(e) / e.dot(e)).clamp(-1.0, 1.0);
    d = d.min(Vec2::new(dot2(v), wi * he - s.abs()));
    d.x.sqrt() * (-d.y).signum()
}

fn sd_pentagon(p_in: Vec2, r: f32) -> f32 {
    let k = Vec2::new(0.809_017, 0.587_785_24);
    let kz = 0.726_542_53_f32;
    let mut p = Vec2::new(p_in.x.abs(), p_in.y);
    let d1 = Vec2::new(-k.x, k.y);
    p -= 2.0 * d1.dot(p).min(0.0) * d1;
    let d2 = Vec2::new(k.x, k.y);
    p -= 2.0 * d2.dot(p).min(0.0) * d2;
    p -= Vec2::new(p.x.clamp(-r * kz, r * kz), r);
    p.length() * p.y.signum()
}

fn sd_hexagon(p_in: Vec2, r: f32) -> f32 {
    let k = Vec2::new(-0.866_025_4, 0.5);
    let kz = 0.577_350_26_f32;
    let mut p = p_in.abs();
    p -= 2.0 * k.dot(p).min(0.0) * k;
    p -= Vec2::new(p.x.clamp(-kz * r, kz * r), r);
    p.length() * p.y.signum()
}

fn sd_octagon(p_in: Vec2, r: f32) -> f32 {
    let k = Vec2::new(-0.923_879_5, 0.382_683_43);
    let kz = 0.414_213_57_f32;
    let mut p = p_in.abs();
    p -= 2.0 * k.dot(p).min(0.0) * k;
    let k2 = Vec2::new(-k.x, k.y);
    p -= 2.0 * k2.dot(p).min(0.0) * k2;
    p -= Vec2::new(p.x.clamp(-kz * r, kz * r), r);
    p.length() * p.y.signum()
}

fn sd_hexagram(p_in: Vec2, r: f32) -> f32 {
    let k = Vec2::new(-0.5, 0.866_025_4);
    let kz = 0.577_350_26_f32;
    let kw = 1.732_050_8_f32;
    let mut p = p_in.abs();
    p -= 2.0 * k.dot(p).min(0.0) * k;
    let kyx = Vec2::new(k.y, k.x);
    p -= 2.0 * kyx.dot(p).min(0.0) * kyx;
    p -= Vec2::new(p.x.clamp(r * kz, r * kw), r);
    p.length() * p.y.signum()
}

fn sd_star(p_in: Vec2, r: f32, n: u32, m: f32) -> f32 {
    let an = PI / n as f32;
    let en = PI / m;
    let acs = Vec2::new(an.cos(), an.sin());
    let ecs = Vec2::new(en.cos(), en.sin());
    let bn = ((p_in.x.atan2(p_in.y) % (2.0 * an)) + 2.0 * an) % (2.0 * an) - an;
    let mut p = p_in.length() * Vec2::new(bn.cos(), bn.sin().abs());
    p -= r * acs;
    p += ecs * (-p.dot(ecs)).clamp(0.0, r * acs.y / ecs.y);
    p.length() * p.x.signum()
}

fn sd_pie(p_in: Vec2, sc: Vec2, r: f32) -> f32 {
    let mut p = p_in;
    p.x = p.x.abs();
    let l = p.length() - r;
    let m = (p - sc * p.dot(sc).clamp(0.0, r)).length();
    l.max(m * (sc.y * p.x - sc.x * p.y).signum())
}

fn sd_arc(p_in: Vec2, sc: Vec2, ra: f32, rb: f32) -> f32 {
    let mut p = p_in;
    p.x = p.x.abs();
    if sc.y * p.x > sc.x * p.y {
        return (p - sc * ra).length() - rb;
    }
    (p.length() - ra).abs() - rb
}

fn sd_cut_disk(p_in: Vec2, r: f32, h: f32) -> f32 {
    let mut p = p_in;
    let w = (r * r - h * h).sqrt();
    p.x = p.x.abs();
    let s = ((h - r) * p.x * p.x + w * w * (h + r - 2.0 * p.y)).max(h * p.x - w * p.y);
    if s < 0.0 {
        return p.length() - r;
    }
    if p.x < w {
        return h - p.y;
    }
    (p - Vec2::new(w, h)).length()
}

fn sd_heart(p_in: Vec2) -> f32 {
    let mut p = Vec2::new(p_in.x, -p_in.y);
    p.x = p.x.abs();
    if p.y + p.x > 1.0 {
        return dot2(p - Vec2::new(0.25, 0.75)).sqrt() - 2.0_f32.sqrt() / 4.0;
    }
    let t = (p.x + p.y).max(0.0) * 0.5;
    dot2(p - Vec2::new(0.0, 1.0))
        .min(dot2(p - Vec2::splat(t)))
        .sqrt()
        * (p.x - p.y).signum()
}

fn sd_egg(p_in: Vec2, ra: f32, rb: f32) -> f32 {
    let k = 3.0_f32.sqrt();
    let mut p = Vec2::new(p_in.x, -p_in.y);
    p.x = p.x.abs();
    let r = ra - rb;
    if p.y < 0.0 {
        return p.length() - r - rb;
    }
    if k * (p.x + r) < p.y {
        return (p - Vec2::new(0.0, k * r)).length() - rb;
    }
    (p + Vec2::new(r, 0.0)).length() - 2.0 * r - rb
}

fn sd_moon(p_in: Vec2, d: f32, ra: f32, rb: f32) -> f32 {
    let mut p = p_in;
    p.y = p.y.abs();
    let a = (ra * ra - rb * rb + d * d) / (2.0 * d);
    let b = (ra * ra - a * a).max(0.0).sqrt();
    if d * (p.x * b - p.y * a) > d * d * (b - p.y).max(0.0) {
        return (p - Vec2::new(a, b)).length();
    }
    (p.length() - ra).max(-(( p - Vec2::new(d, 0.0)).length() - rb))
}

fn sd_vesica(p_in: Vec2, r: f32, d: f32) -> f32 {
    let p = p_in.abs();
    let b = (r * r - d * d).sqrt();
    if (p.y - b) * d > p.x * b {
        return (p - Vec2::new(0.0, b)).length();
    }
    (p - Vec2::new(-d, 0.0)).length() - r
}

fn sd_uneven_capsule(p_in: Vec2, r1: f32, r2: f32, h: f32) -> f32 {
    let mut p = p_in;
    p.x = p.x.abs();
    let b = (r1 - r2) / h;
    let a = (1.0 - b * b).sqrt();
    let k = p.dot(Vec2::new(-b, a));
    if k < 0.0 {
        return p.length() - r1;
    }
    if k > a * h {
        return (p - Vec2::new(0.0, h)).length() - r2;
    }
    p.dot(Vec2::new(a, b)) - r1
}

fn sd_oriented_box(p: Vec2, a: Vec2, b: Vec2, th: f32) -> f32 {
    let l = (b - a).length();
    let d = (b - a) / l;
    let mut q = p - (a + b) * 0.5;
    q = Vec2::new(d.x * q.x + d.y * q.y, -d.y * q.x + d.x * q.y);
    q = q.abs() - Vec2::new(l, th) * 0.5;
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

fn sd_horseshoe(p_in: Vec2, sc: Vec2, r: f32, w: Vec2) -> f32 {
    let mut p = Vec2::new(p_in.x.abs(), p_in.y);
    let l = p.length();
    p = Vec2::new(-sc.x * p.x + sc.y * p.y, sc.y * p.x + sc.x * p.y);
    p = Vec2::new(
        if p.y > 0.0 || p.x > 0.0 {
            p.x
        } else {
            l * (-sc.x).signum()
        },
        if p.x > 0.0 { p.y } else { l },
    );
    p = Vec2::new(p.x, (p.y - r).abs()) - w;
    p.max(Vec2::ZERO).length() + p.x.max(p.y).min(0.0)
}

fn sd_rounded_x(p: Vec2, w: f32, r: f32) -> f32 {
    let q = p.abs();
    (q - Vec2::splat((q.x + q.y).min(w) * 0.5)).length() - r
}

fn sd_cross(p_in: Vec2, b: Vec2, r: f32) -> f32 {
    let mut p = p_in.abs();
    if p.y > p.x {
        p = Vec2::new(p.y, p.x);
    }
    let q = p - b;
    let k = q.x.max(q.y);
    let w = if k > 0.0 {
        q
    } else {
        Vec2::new(b.y - p.x, -k)
    };
    w.max(Vec2::ZERO).length() * k.signum() + r
}

fn sd_parabola(pos: Vec2, k: f32) -> f32 {
    let mut p = pos;
    p.x = p.x.abs();
    let ik = 1.0 / k;
    let pp = ik * (p.y - 0.5 * ik) / 3.0;
    let q = 0.25 * ik * ik * p.x;
    let h = q * q - pp * pp * pp;
    let r = h.abs().sqrt();
    let x = if h > 0.0 {
        (q + r).powf(1.0 / 3.0) - (q - r).abs().powf(1.0 / 3.0) * (r - q).signum()
    } else {
        2.0 * (r.atan2(q) / 3.0).cos() * pp.sqrt()
    };
    (p - Vec2::new(x, k * x * x)).length() * (p.x - x).signum()
}

fn sd_cool_s(p_in: Vec2) -> f32 {
    let mut p = p_in;
    let six = if p.y < 0.0 { -p.x } else { p.x };
    p.x = p.x.abs();
    p.y = p.y.abs() - 0.2;
    let rex = p.x - (p.x / 0.4).round().min(0.4);
    let aby = (p.y - 0.2).abs() - 0.6;

    let clamp_val1 = (0.5 * (six - p.y)).clamp(0.0, 0.2);
    let v1 = Vec2::new(six, -p.y) - Vec2::splat(clamp_val1);
    let mut d = dot2(v1);
    let clamp_val2 = (0.5 * (p.x - aby)).clamp(0.0, 0.4);
    let v2 = Vec2::new(p.x, -aby) - Vec2::splat(clamp_val2);
    d = d.min(dot2(v2));
    let v3 = Vec2::new(rex, p.y - p.y.clamp(0.0, 0.4));
    d = d.min(dot2(v3));

    let s = 2.0 * p.x + aby + (aby + 0.4).abs() - 0.4;
    d.sqrt() * s.signum()
}

fn sd_blobby_cross(pos: Vec2, he: f32) -> f32 {
    let mut p = pos.abs();
    p = Vec2::new((p.x - p.y).abs(), 1.0 - p.x - p.y) / 2.0_f32.sqrt();
    let pp = (he - p.y - 0.25 / he) / (6.0 * he);
    let q = p.x / (he * he * 16.0);
    let h = q * q - pp * pp * pp;
    let r = h.abs().sqrt();
    let x = if h > 0.0 {
        (q + r).powf(1.0 / 3.0) - (q - r).abs().powf(1.0 / 3.0) * (r - q).signum()
    } else {
        2.0 * pp.sqrt() * ((q / (pp * pp.sqrt())).acos() / 3.0).cos()
    };
    let x = x.min(2.0_f32.sqrt() / 2.0);
    let z = Vec2::new(x, he * (1.0 - 2.0 * x * x)) - p;
    z.length() * z.y.signum()
}

fn sd_quad_bezier(pos: Vec2, a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let a_coeff = b - a;
    let b_coeff = a - 2.0 * b + c;
    let c_coeff = a_coeff * 2.0;
    let d = a - pos;
    let kk = 1.0 / b_coeff.dot(b_coeff);
    let kx = kk * a_coeff.dot(b_coeff);
    let ky = kk * (2.0 * a_coeff.dot(a_coeff) + d.dot(b_coeff)) / 3.0;
    let kz = kk * d.dot(a_coeff);
    let p = ky - kx * kx;
    let p3 = p * p * p;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    let h = q * q + 4.0 * p3;
    let res = if h >= 0.0 {
        let sh = h.sqrt();
        let x = Vec2::new(sh - q, -sh - q) / 2.0;
        let uv = Vec2::new(
            signed_pow(x.x, 1.0 / 3.0),
            signed_pow(x.y, 1.0 / 3.0),
        );
        let t = (uv.x + uv.y - kx).clamp(0.0, 1.0);
        dot2(d + (c_coeff + b_coeff * t) * t)
    } else {
        let z = (-p).sqrt();
        let v = (q / (p * z * 2.0)).acos() / 3.0;
        let m = v.cos();
        let n = v.sin() * 1.732_050_8;
        let t1 = ((m + m) * z - kx).clamp(0.0, 1.0);
        let t2 = ((-n - m) * z - kx).clamp(0.0, 1.0);
        dot2(d + (c_coeff + b_coeff * t1) * t1)
            .min(dot2(d + (c_coeff + b_coeff * t2) * t2))
    };
    res.sqrt()
}

// CSG Operations

fn op_union(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist < b.dist {
        a
    } else {
        b
    }
}

fn op_subtract(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist > -b.dist {
        SdfResult::new(a.dist, a.u)
    } else {
        SdfResult::new(-b.dist, b.u)
    }
}

fn op_intersect(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist > b.dist {
        a
    } else {
        b
    }
}

fn op_smooth_union(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = (0.5 + 0.5 * (b.dist - a.dist) / k).clamp(0.0, 1.0);
    let d = lerp(b.dist, a.dist, h) - k * h * (1.0 - h);
    let u = lerp(b.u, a.u, h);
    SdfResult::new(d, u)
}

fn op_smooth_subtract(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = (0.5 - 0.5 * (a.dist + b.dist) / k).clamp(0.0, 1.0);
    let d = lerp(a.dist, -b.dist, h) + k * h * (1.0 - h);
    let u = lerp(a.u, b.u, h);
    SdfResult::new(d, u)
}

// Modifiers

fn op_round(a: SdfResult, r: f32) -> SdfResult {
    SdfResult::new(a.dist - r, a.u)
}

fn op_onion(a: SdfResult, thickness: f32) -> SdfResult {
    SdfResult::new(a.dist.abs() - thickness, a.u)
}

/// 2D box distance (used by dash/arrow pattern ops).
fn sd_box_2d(p: Vec2, b: Vec2) -> f32 {
    let d = p.abs() - b;
    d.max(Vec2::ZERO).length() + d.x.max(d.y).min(0.0)
}

/// Quantize period to tile evenly around a closed curve.
fn quantize_period(period: f32, perimeter: f32) -> f32 {
    if perimeter > 0.0 {
        let n = (perimeter / period).round();
        if n > 0.0 {
            perimeter / n
        } else {
            period
        }
    } else {
        period
    }
}

/// Dash pattern: repeating dashes along contour with angled caps.
fn op_dash(
    a: SdfResult,
    dash: f32,
    gap: f32,
    thickness: f32,
    angle: f32,
    perimeter: f32,
) -> SdfResult {
    let period = dash + gap;
    let actual_period = quantize_period(period, perimeter);
    let ratio = dash / period;
    let actual_dash = actual_period * ratio;
    let half_dash = actual_dash * 0.5;
    let half_thickness = thickness * 0.5;
    let tan_angle = angle.tan();

    let shifted_u = a.u - a.dist * tan_angle;

    let nearest = (shifted_u / actual_period).round() * actual_period;
    let dist_along = shifted_u - nearest;

    let d = sd_box_2d(Vec2::new(dist_along, a.dist), Vec2::new(half_dash, half_thickness));
    SdfResult::new(d, a.u)
}

/// Arrow pattern: repeating angled slashes crossing the contour.
fn op_arrow(
    a: SdfResult,
    segment: f32,
    gap: f32,
    thickness: f32,
    angle: f32,
    perimeter: f32,
) -> SdfResult {
    let period = segment + gap;
    let actual_period = quantize_period(period, perimeter);
    let ratio = segment / period;
    let actual_seg = actual_period * ratio;
    let half_seg = actual_seg * 0.5;
    let half_thickness = thickness * 0.5;
    let tan_angle = angle.tan();

    let shifted_u = a.u - a.dist.abs() * tan_angle;

    let nearest = (shifted_u / actual_period).round() * actual_period;
    let dist_along = shifted_u - nearest;

    let d = sd_box_2d(Vec2::new(dist_along, a.dist), Vec2::new(half_seg, half_thickness));
    SdfResult::new(d, a.u)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Sdf;

    #[test]
    fn test_circle_inside() {
        let sdf = Sdf::circle([0.0, 0.0], 10.0);
        let result = evaluate(sdf.node(), Vec2::new(0.0, 0.0));
        assert!(result.is_inside());
        assert_eq!(result.dist, -10.0);
    }

    #[test]
    fn test_circle_outside() {
        let sdf = Sdf::circle([0.0, 0.0], 10.0);
        let result = evaluate(sdf.node(), Vec2::new(20.0, 0.0));
        assert!(!result.is_inside());
        assert_eq!(result.dist, 10.0);
    }

    #[test]
    fn test_circle_on_boundary() {
        let sdf = Sdf::circle([0.0, 0.0], 10.0);
        let result = evaluate(sdf.node(), Vec2::new(10.0, 0.0));
        assert!(result.is_near(0.1));
    }

    #[test]
    fn test_box_inside() {
        let sdf = Sdf::rect([0.0, 0.0], [10.0, 10.0]);
        let result = evaluate(sdf.node(), Vec2::new(0.0, 0.0));
        assert!(result.is_inside());
    }

    #[test]
    fn test_union() {
        let sdf = Sdf::circle([0.0, 0.0], 10.0) | Sdf::circle([15.0, 0.0], 10.0);
        // Point in first circle
        let r1 = evaluate(sdf.node(), Vec2::new(0.0, 0.0));
        assert!(r1.is_inside());
        // Point in second circle
        let r2 = evaluate(sdf.node(), Vec2::new(15.0, 0.0));
        assert!(r2.is_inside());
        // Point between circles (inside union due to overlap)
        let r3 = evaluate(sdf.node(), Vec2::new(7.5, 0.0));
        assert!(r3.is_inside());
    }

    #[test]
    fn test_subtract() {
        let sdf = Sdf::rect([0.0, 0.0], [20.0, 20.0]) - Sdf::circle([0.0, 0.0], 10.0);
        // Point inside hole (should be outside result)
        let r1 = evaluate(sdf.node(), Vec2::new(0.0, 0.0));
        assert!(!r1.is_inside());
        // Point in remaining box
        let r2 = evaluate(sdf.node(), Vec2::new(15.0, 15.0));
        assert!(r2.is_inside());
    }

    #[test]
    fn test_line() {
        let sdf = Sdf::line([0.0, 0.0], [10.0, 0.0]);
        let result = evaluate(sdf.node(), Vec2::new(5.0, 2.0));
        assert!((result.dist - 2.0).abs() < 0.001);
        assert!((result.u - 5.0).abs() < 0.001);
    }
}
