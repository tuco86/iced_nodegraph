//! CPU-side SDF evaluation for hit-testing.
//!
//! Mirrors the GPU shader logic for use in interaction handling.

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
    }
}

// Primitive SDFs

fn sd_circle(p: Vec2, center: Vec2, radius: f32) -> SdfResult {
    let d = (p - center).length() - radius;
    let angle = (p.y - center.y).atan2(p.x - center.x);
    let u = (angle + std::f32::consts::PI) * radius;
    SdfResult::new(d, u)
}

fn sd_box(p: Vec2, center: Vec2, half_size: Vec2) -> SdfResult {
    let q = (p - center).abs() - half_size;
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);

    // Compute perimeter position
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

    // Coarse sampling
    for i in 0..=num_samples {
        let t = i as f32 / num_samples as f32;
        let pos = cubic_bezier(p0, p1, p2, p3, t);
        let dist = (p - pos).length();
        if dist < min_dist {
            min_dist = dist;
            best_t = t;
        }
    }

    // Refinement
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
