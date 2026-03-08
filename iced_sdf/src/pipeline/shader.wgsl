// Batched SDF Renderer Shader
//
// Renders multiple SDF shapes in a single draw call using instanced quads.
// Each instance has its own RPN ops and layers, referenced by offset into
// flat storage buffers.

// ============================================================================
// Constants
// ============================================================================

// Operation types (must match compile.rs OpType enum)
const OP_CIRCLE: u32 = 0u;
const OP_BOX: u32 = 1u;
const OP_ROUNDED_BOX: u32 = 2u;
const OP_LINE: u32 = 3u;
const OP_BEZIER: u32 = 4u;

const OP_ELLIPSE: u32 = 5u;
const OP_TRIANGLE: u32 = 6u;
const OP_EQUILATERAL_TRIANGLE: u32 = 7u;
const OP_ISOSCELES_TRIANGLE: u32 = 8u;
const OP_RHOMBUS: u32 = 9u;
const OP_TRAPEZOID: u32 = 10u;
const OP_PARALLELOGRAM: u32 = 11u;
const OP_PENTAGON: u32 = 12u;
const OP_HEXAGON: u32 = 13u;
const OP_OCTAGON: u32 = 14u;
const OP_HEXAGRAM: u32 = 15u;

const OP_UNION: u32 = 16u;
const OP_SUBTRACT: u32 = 17u;
const OP_INTERSECT: u32 = 18u;
const OP_SMOOTH_UNION: u32 = 19u;
const OP_SMOOTH_SUBTRACT: u32 = 20u;

const OP_STAR: u32 = 21u;
const OP_PIE: u32 = 22u;
const OP_ARC: u32 = 23u;
const OP_CUT_DISK: u32 = 24u;
const OP_HEART: u32 = 25u;
const OP_EGG: u32 = 26u;
const OP_MOON: u32 = 27u;
const OP_VESICA: u32 = 28u;
const OP_UNEVEN_CAPSULE: u32 = 29u;
const OP_ORIENTED_BOX: u32 = 30u;
const OP_HORSESHOE: u32 = 31u;

const OP_ROUND: u32 = 32u;
const OP_ONION: u32 = 33u;

const OP_ROUNDED_X: u32 = 34u;
const OP_CROSS: u32 = 35u;
const OP_QUAD_BEZIER: u32 = 36u;
const OP_PARABOLA: u32 = 37u;
const OP_COOL_S: u32 = 38u;
const OP_BLOBBY_CROSS: u32 = 39u;

// Layer flags
const LAYER_FLAG_GRADIENT: u32 = 1u;
const LAYER_FLAG_GRADIENT_U: u32 = 2u;
const LAYER_FLAG_HAS_PATTERN: u32 = 4u;
const LAYER_FLAG_DISTANCE_FIELD: u32 = 8u;

// Pattern types
const PATTERN_SOLID: u32 = 0u;
const PATTERN_DASHED: u32 = 1u;
const PATTERN_ARROWED: u32 = 2u;
const PATTERN_DOTTED: u32 = 3u;

const PI: f32 = 3.14159265359;
const MAX_STACK: u32 = 16u;

// ============================================================================
// Data Structures
// ============================================================================

struct Uniforms {
    bounds_origin: vec2<f32>,
    bounds_size: vec2<f32>,
    camera_position: vec2<f32>,
    camera_zoom: f32,
    scale_factor: f32,
    time: f32,
    num_ops: u32,
    num_layers: u32,
    _pad: u32,
}

struct ShapeInstance {
    bounds: vec4<f32>,  // screen-space: x, y, width, height
    ops_offset: u32,
    ops_count: u32,
    layers_offset: u32,
    layers_count: u32,
}

struct SdfOp {
    op_type: u32,
    flags: u32,
    _pad0: u32,
    _pad1: u32,
    param0: vec4<f32>,
    param1: vec4<f32>,
    param2: vec4<f32>,
}

struct SdfLayer {
    color: vec4<f32>,
    gradient_color: vec4<f32>,
    expand: f32,
    blur: f32,
    gradient_angle: f32,
    flags: u32,
    pattern_type: u32,
    thickness: f32,
    pattern_param0: f32,
    pattern_param1: f32,
    pattern_param2: f32,
    flow_speed: f32,
}

struct SdfResult {
    dist: f32,
    u: f32,
}

// ============================================================================
// Bindings
// ============================================================================

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> shapes: array<ShapeInstance>;
@group(0) @binding(2) var<storage, read> ops: array<SdfOp>;
@group(0) @binding(3) var<storage, read> layers: array<SdfLayer>;

// ============================================================================
// SDF Primitives
// ============================================================================

fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> SdfResult {
    let d = length(p - center) - radius;
    let angle = atan2(p.y - center.y, p.x - center.x);
    let u = (angle + PI) * radius;
    return SdfResult(d, u);
}

fn sd_box(p: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>) -> SdfResult {
    let q = abs(p - center) - half_size;
    let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);

    let rel = p - center;
    var u: f32 = 0.0;
    let w = half_size.x;
    let h = half_size.y;

    if abs(rel.y + h) < 0.001 && abs(rel.x) <= w {
        u = 2.0 * w + 2.0 * h + (w - rel.x);
    } else if abs(rel.x - w) < 0.001 && abs(rel.y) <= h {
        u = w + (h - rel.y);
    } else if abs(rel.y - h) < 0.001 && abs(rel.x) <= w {
        u = (w + rel.x);
    } else {
        u = 2.0 * w + h + (h + rel.y);
    }

    return SdfResult(d, u);
}

fn sd_rounded_box(p: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>, r: f32) -> SdfResult {
    let q = abs(p - center) - half_size + r;
    let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let base = sd_box(p, center, half_size);
    return SdfResult(d, base.u);
}

fn sd_line(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> SdfResult {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    let d = length(pa - ba * h);
    let u = h * length(ba);
    return SdfResult(d, u);
}

fn bezier_derivative(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let mt = 1.0 - t;
    return 3.0 * mt * mt * (p1 - p0) + 6.0 * mt * t * (p2 - p1) + 3.0 * t * t * (p3 - p2);
}

fn bezier_arc_length_to(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t_end: f32) -> f32 {
    // Gauss-Legendre 5-point quadrature
    let w0 = 0.2369268850;
    let w1 = 0.4786286705;
    let w2 = 0.5688888889;
    let a0 = 0.9061798459;
    let a1 = 0.5384693101;

    let half_t = t_end * 0.5;
    var len = 0.0;
    len += w0 * length(bezier_derivative(p0, p1, p2, p3, half_t * (1.0 - a0)));
    len += w1 * length(bezier_derivative(p0, p1, p2, p3, half_t * (1.0 - a1)));
    len += w2 * length(bezier_derivative(p0, p1, p2, p3, half_t));
    len += w1 * length(bezier_derivative(p0, p1, p2, p3, half_t * (1.0 + a1)));
    len += w0 * length(bezier_derivative(p0, p1, p2, p3, half_t * (1.0 + a0)));
    return len * half_t;
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

fn sd_bezier(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> SdfResult {
    // Polynomial decomposition: B(t) = At^3 + Bt^2 + Ct + D
    let A = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let B = 3.0 * p0 - 6.0 * p1 + 3.0 * p2;
    let C = -3.0 * p0 + 3.0 * p1;
    let D = p0;

    var min_dist = dot2(p - p0);
    var best_t = 0.0;

    // Coarse search with Newton's method refinement at each sample
    for (var i = 0; i <= 8; i = i + 1) {
        var t = f32(i) / 8.0;

        for (var iter = 0; iter < 4; iter = iter + 1) {
            let t2 = t * t;
            let t3 = t2 * t;
            let point = A * t3 + B * t2 + C * t + D;
            let deriv = 3.0 * A * t2 + 2.0 * B * t + C;
            let deriv2 = 6.0 * A * t + 2.0 * B;
            let diff = point - p;

            let f = dot(diff, deriv);
            let fp = dot(deriv, deriv) + dot(diff, deriv2);

            if (abs(fp) > 0.00001) {
                t = t - f / fp;
            }
            t = clamp(t, 0.0, 1.0);
        }

        let t2 = t * t;
        let t3 = t2 * t;
        let point = A * t3 + B * t2 + C * t + D;
        let dist = dot2(p - point);

        if (dist < min_dist) {
            min_dist = dist;
            best_t = t;
        }
    }

    // Check endpoint
    let end_dist = dot2(p - p3);
    if (end_dist < min_dist) {
        min_dist = end_dist;
        best_t = 1.0;
    }

    let u = bezier_arc_length_to(p0, p1, p2, p3, best_t);
    return SdfResult(sqrt(min_dist), u);
}

fn ndot(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.x - a.y * b.y;
}

fn sd_ellipse(p_in: vec2<f32>, ab_in: vec2<f32>) -> f32 {
    var p = abs(p_in);
    var ab = ab_in;
    if p.x > p.y { p = p.yx; ab = ab.yx; }
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
    var co: f32;
    if d < 0.0 {
        let h = acos(q / c3) / 3.0;
        let s = cos(h);
        let t = sin(h) * sqrt(3.0);
        let rx = sqrt(-c * (s + t + 2.0) + m2);
        let ry = sqrt(-c * (s - t + 2.0) + m2);
        co = (ry + sign(l) * rx + abs(g) / (rx * ry) - m) / 2.0;
    } else {
        let h = 2.0 * m * n * sqrt(d);
        let s = sign(q + h) * pow(abs(q + h), 1.0 / 3.0);
        let u = sign(q - h) * pow(abs(q - h), 1.0 / 3.0);
        let rx = -s - u - c * 4.0 + 2.0 * m2;
        let ry = (s - u) * sqrt(3.0);
        let rm = sqrt(rx * rx + ry * ry);
        co = (ry / sqrt(rm - rx) + 2.0 * g / rm - m) / 2.0;
    }
    let r = ab * vec2(co, sqrt(1.0 - co * co));
    return length(r - p) * sign(p.y - r.y);
}

fn sd_triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0; let e1 = p2 - p1; let e2 = p0 - p2;
    let v0 = p - p0; let v1 = p - p1; let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(min(
        vec2(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
        vec2(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))),
        vec2(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)));
    return -sqrt(d.x) * sign(d.y);
}

fn sd_equilateral_triangle(p_in: vec2<f32>, r: f32) -> f32 {
    let k = sqrt(3.0);
    var p = p_in;
    p.x = abs(p.x) - r;
    p.y = p.y + r / k;
    if p.x + k * p.y > 0.0 {
        p = vec2(p.x - k * p.y, -k * p.x - p.y) / 2.0;
    }
    p.x -= clamp(p.x, -2.0 * r, 0.0);
    return -length(p) * sign(p.y);
}

fn sd_isosceles_triangle(p_in: vec2<f32>, q: vec2<f32>) -> f32 {
    var p = p_in;
    p.x = abs(p.x);
    let a = p - q * clamp(dot(p, q) / dot(q, q), 0.0, 1.0);
    let b = p - q * vec2(clamp(p.x / q.x, 0.0, 1.0), 1.0);
    let s = -sign(q.y);
    let d = min(vec2(dot(a, a), s * (p.x * q.y - p.y * q.x)),
                vec2(dot(b, b), s * (p.y - q.y)));
    return -sqrt(d.x) * sign(d.y);
}

fn sd_rhombus(p_in: vec2<f32>, b: vec2<f32>) -> f32 {
    let p = abs(p_in);
    let h = clamp(ndot(b - 2.0 * p, b) / dot(b, b), -1.0, 1.0);
    let d = length(p - 0.5 * b * vec2(1.0 - h, 1.0 + h));
    return d * sign(p.x * b.y + p.y * b.x - b.x * b.y);
}

fn sd_trapezoid(p_in: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
    var p = p_in;
    let k1 = vec2(r2, he);
    let k2 = vec2(r2 - r1, 2.0 * he);
    p.x = abs(p.x);
    let ca = vec2(p.x - min(p.x, select(r2, r1, p.y < 0.0)), abs(p.y) - he);
    let cb = p - k1 + k2 * clamp(dot(k1 - p, k2) / dot(k2, k2), 0.0, 1.0);
    let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

fn sd_parallelogram(p_in: vec2<f32>, wi: f32, he: f32, sk: f32) -> f32 {
    let e = vec2(sk, he);
    var p = select(p_in, -p_in, p_in.y < 0.0);
    var w = p - e;
    w = vec2(w.x - clamp(w.x, -wi, wi), w.y);
    var d = vec2(dot(w, w), -w.y);
    let s = p.x * e.y - p.y * e.x;
    p = select(p, -p, s < 0.0);
    var v = p - vec2(wi, 0.0);
    v = v - e * clamp(dot(v, e) / dot(e, e), -1.0, 1.0);
    d = min(d, vec2(dot(v, v), wi * he - abs(s)));
    return sqrt(d.x) * sign(-d.y);
}

fn sd_pentagon(p_in: vec2<f32>, r: f32) -> f32 {
    let k = vec3(0.809016994, 0.587785252, 0.726542528);
    var p = vec2(abs(p_in.x), p_in.y);
    p -= 2.0 * min(dot(vec2(-k.x, k.y), p), 0.0) * vec2(-k.x, k.y);
    p -= 2.0 * min(dot(vec2(k.x, k.y), p), 0.0) * vec2(k.x, k.y);
    p -= vec2(clamp(p.x, -r * k.z, r * k.z), r);
    return length(p) * sign(p.y);
}

fn sd_hexagon(p_in: vec2<f32>, r: f32) -> f32 {
    let k = vec3(-0.866025404, 0.5, 0.577350269);
    var p = abs(p_in);
    p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
    p -= vec2(clamp(p.x, -k.z * r, k.z * r), r);
    return length(p) * sign(p.y);
}

fn sd_octagon(p_in: vec2<f32>, r: f32) -> f32 {
    let k = vec3(-0.9238795325, 0.3826834323, 0.4142135623);
    var p = abs(p_in);
    p -= 2.0 * min(dot(vec2(k.x, k.y), p), 0.0) * vec2(k.x, k.y);
    p -= 2.0 * min(dot(vec2(-k.x, k.y), p), 0.0) * vec2(-k.x, k.y);
    p -= vec2(clamp(p.x, -k.z * r, k.z * r), r);
    return length(p) * sign(p.y);
}

fn sd_hexagram(p_in: vec2<f32>, r: f32) -> f32 {
    let k = vec4(-0.5, 0.8660254038, 0.5773502692, 1.7320508076);
    var p = abs(p_in);
    p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
    p -= 2.0 * min(dot(k.yx, p), 0.0) * k.yx;
    p -= vec2(clamp(p.x, r * k.z, r * k.w), r);
    return length(p) * sign(p.y);
}

fn sd_star(p_in: vec2<f32>, r: f32, n: i32, m: f32) -> f32 {
    let an = PI / f32(n);
    let en = PI / m;
    let acs = vec2(cos(an), sin(an));
    let ecs = vec2(cos(en), sin(en));
    let bn = ((atan2(p_in.x, p_in.y) % (2.0 * an)) + 2.0 * an) % (2.0 * an) - an;
    var p = length(p_in) * vec2(cos(bn), abs(sin(bn)));
    p -= r * acs;
    p += ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
    return length(p) * sign(p.x);
}

fn sd_pie(p_in: vec2<f32>, sc: vec2<f32>, r: f32) -> f32 {
    var p = p_in;
    p.x = abs(p.x);
    let l = length(p) - r;
    let m = length(p - sc * clamp(dot(p, sc), 0.0, r));
    return max(l, m * sign(sc.y * p.x - sc.x * p.y));
}

fn sd_arc(p_in: vec2<f32>, sc: vec2<f32>, ra: f32, rb: f32) -> f32 {
    var p = p_in;
    p.x = abs(p.x);
    if sc.y * p.x > sc.x * p.y {
        return length(p - sc * ra) - rb;
    }
    return abs(length(p) - ra) - rb;
}

fn sd_cut_disk(p_in: vec2<f32>, r: f32, h: f32) -> f32 {
    var p = p_in;
    let w = sqrt(r * r - h * h);
    p.x = abs(p.x);
    let s = max((h - r) * p.x * p.x + w * w * (h + r - 2.0 * p.y), h * p.x - w * p.y);
    if s < 0.0 { return length(p) - r; }
    if p.x < w { return h - p.y; }
    return length(p - vec2(w, h));
}

fn sd_heart(p_in: vec2<f32>) -> f32 {
    var p = vec2(p_in.x, -p_in.y);
    p.x = abs(p.x);
    if p.y + p.x > 1.0 {
        return sqrt(dot2(p - vec2(0.25, 0.75))) - sqrt(2.0) / 4.0;
    }
    return sqrt(min(dot2(p - vec2(0.0, 1.0)),
                    dot2(p - 0.5 * max(p.x + p.y, 0.0)))) * sign(p.x - p.y);
}

fn sd_egg(p_in: vec2<f32>, ra: f32, rb: f32) -> f32 {
    let k = sqrt(3.0);
    var p = p_in;
    p.x = abs(p.x);
    let r = ra - rb;
    if p.y < 0.0 {
        return length(p) - r - rb;
    }
    if k * (p.x + r) < p.y {
        return length(p - vec2(0.0, k * r)) - rb;
    }
    return length(p + vec2(r, 0.0)) - 2.0 * r - rb;
}

fn sd_moon(p_in: vec2<f32>, d: f32, ra: f32, rb: f32) -> f32 {
    var p = p_in;
    p.y = abs(p.y);
    let a = (ra * ra - rb * rb + d * d) / (2.0 * d);
    let b = sqrt(max(ra * ra - a * a, 0.0));
    if d * (p.x * b - p.y * a) > d * d * max(b - p.y, 0.0) {
        return length(p - vec2(a, b));
    }
    return max(length(p) - ra, -(length(p - vec2(d, 0.0)) - rb));
}

fn sd_vesica(p_in: vec2<f32>, r: f32, d: f32) -> f32 {
    let p = abs(p_in);
    let b = sqrt(r * r - d * d);
    if (p.y - b) * d > p.x * b {
        return length(p - vec2(0.0, b));
    }
    return length(p - vec2(-d, 0.0)) - r;
}

fn sd_uneven_capsule(p_in: vec2<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    var p = p_in;
    p.x = abs(p.x);
    let b = (r1 - r2) / h;
    let a = sqrt(1.0 - b * b);
    let k = dot(p, vec2(-b, a));
    if k < 0.0 { return length(p) - r1; }
    if k > a * h { return length(p - vec2(0.0, h)) - r2; }
    return dot(p, vec2(a, b)) - r1;
}

fn sd_oriented_box(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, th: f32) -> f32 {
    let l = length(b - a);
    let d = (b - a) / l;
    var q = p - (a + b) * 0.5;
    q = vec2(d.x * q.x + d.y * q.y, -d.y * q.x + d.x * q.y);
    q = abs(q) - vec2(l, th) * 0.5;
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);
}

fn sd_horseshoe(p_in: vec2<f32>, sc: vec2<f32>, r: f32, w: vec2<f32>) -> f32 {
    var p = vec2(abs(p_in.x), p_in.y);
    let l = length(p);
    p = vec2(-sc.x * p.x + sc.y * p.y, sc.y * p.x + sc.x * p.y);
    p = vec2(select(l * sign(-sc.x), p.x, p.y > 0.0 || p.x > 0.0),
             select(l, p.y, p.x > 0.0));
    p = vec2(p.x, abs(p.y - r)) - w;
    return length(max(p, vec2(0.0))) + min(0.0, max(p.x, p.y));
}

fn sd_rounded_x(p: vec2<f32>, w: f32, r: f32) -> f32 {
    let q = abs(p);
    return length(q - min(q.x + q.y, w) * 0.5) - r;
}

fn sd_cross(p_in: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    var p = abs(p_in);
    if p.y > p.x { p = p.yx; }
    let q = p - b;
    let k = max(q.y, q.x);
    let w = select(vec2(b.y - p.x, -k), q, k > 0.0);
    return sign(k) * length(max(w, vec2(0.0))) + r;
}

fn sd_quad_bezier(pos: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - pos;
    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);
    let p = ky - kx * kx;
    let p3 = p * p * p;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    let h = q * q + 4.0 * p3;
    var res: f32;
    if h >= 0.0 {
        let sh = sqrt(h);
        let x = (vec2(sh, -sh) - vec2(q, q)) / 2.0;
        let uv = sign(x) * pow(abs(x), vec2(1.0 / 3.0));
        let t = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        res = dot2(d + (c + b * t) * t);
    } else {
        let z = sqrt(-p);
        let v = acos(q / (p * z * 2.0)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let t = clamp(vec3(m + m, -n - m, n - m) * z - vec3(kx, kx, kx), vec3(0.0), vec3(1.0));
        res = min(dot2(d + (c + b * t.x) * t.x),
                  dot2(d + (c + b * t.y) * t.y));
    }
    return sqrt(res);
}

fn sd_parabola(pos: vec2<f32>, k: f32) -> f32 {
    var p = pos;
    p.x = abs(p.x);
    let ik = 1.0 / k;
    let pp = ik * (p.y - 0.5 * ik) / 3.0;
    let q = 0.25 * ik * ik * p.x;
    let h = q * q - pp * pp * pp;
    let r = sqrt(abs(h));
    var x: f32;
    if h > 0.0 {
        x = pow(q + r, 1.0 / 3.0) - pow(abs(q - r), 1.0 / 3.0) * sign(r - q);
    } else {
        x = 2.0 * cos(atan2(r, q) / 3.0) * sqrt(pp);
    }
    return length(p - vec2(x, k * x * x)) * sign(p.x - x);
}

fn sd_cool_s(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    let six = select(p.x, -p.x, p.y < 0.0);
    p.x = abs(p.x);
    p.y = abs(p.y) - 0.2;
    let rex = p.x - min(round(p.x / 0.4), 0.4);
    let aby = abs(p.y - 0.2) - 0.6;

    let v1 = vec2(six, -p.y) - clamp(0.5 * (six - p.y), 0.0, 0.2);
    var d = dot2(v1);
    let v2 = vec2(p.x, -aby) - clamp(0.5 * (p.x - aby), 0.0, 0.4);
    d = min(d, dot2(v2));
    let v3 = vec2(rex, p.y - clamp(p.y, 0.0, 0.4));
    d = min(d, dot2(v3));

    let s = 2.0 * p.x + aby + abs(aby + 0.4) - 0.4;
    return sqrt(d) * sign(s);
}

fn sd_blobby_cross(pos: vec2<f32>, he: f32) -> f32 {
    var p = abs(pos);
    p = vec2(abs(p.x - p.y), 1.0 - p.x - p.y) / sqrt(2.0);
    let pp = (he - p.y - 0.25 / he) / (6.0 * he);
    let q = p.x / (he * he * 16.0);
    let h = q * q - pp * pp * pp;
    let r = sqrt(abs(h));
    var x: f32;
    if h > 0.0 {
        x = pow(q + r, 1.0 / 3.0) - pow(abs(q - r), 1.0 / 3.0) * sign(r - q);
    } else {
        x = 2.0 * sqrt(pp) * cos(acos(q / (pp * sqrt(pp))) / 3.0);
    }
    x = min(x, sqrt(2.0) / 2.0);
    let z = vec2(x, he * (1.0 - 2.0 * x * x)) - p;
    return length(z) * sign(z.y);
}

// ============================================================================
// CSG Operations
// ============================================================================

fn op_union(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist < b.dist { return a; }
    return b;
}

fn op_subtract(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist > -b.dist { return SdfResult(a.dist, a.u); }
    return SdfResult(-b.dist, b.u);
}

fn op_intersect(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist > b.dist { return a; }
    return b;
}

fn op_smooth_union(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = clamp(0.5 + 0.5 * (b.dist - a.dist) / k, 0.0, 1.0);
    let d = mix(b.dist, a.dist, h) - k * h * (1.0 - h);
    let u = mix(b.u, a.u, h);
    return SdfResult(d, u);
}

fn op_smooth_subtract(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = clamp(0.5 - 0.5 * (a.dist + b.dist) / k, 0.0, 1.0);
    let d = mix(a.dist, -b.dist, h) + k * h * (1.0 - h);
    let u = mix(a.u, b.u, h);
    return SdfResult(d, u);
}

fn op_round(a: SdfResult, r: f32) -> SdfResult {
    return SdfResult(a.dist - r, a.u);
}

fn op_onion(a: SdfResult, thickness: f32) -> SdfResult {
    return SdfResult(abs(a.dist) - thickness, a.u);
}

// ============================================================================
// Pattern Evaluation
// ============================================================================

fn apply_pattern(sdf: SdfResult, layer: SdfLayer) -> f32 {
    let dist = sdf.dist;
    let thickness = layer.thickness;

    var u = sdf.u;
    if layer.flow_speed != 0.0 {
        u = u + uniforms.time * layer.flow_speed;
    }

    switch layer.pattern_type {
        case PATTERN_SOLID: {
            return abs(dist) - thickness * 0.5;
        }
        case PATTERN_DASHED: {
            let dash = layer.pattern_param0;
            let gap = layer.pattern_param1;
            let period = dash + gap;
            let nearest = round(u / period) * period;
            let dist_along = u - nearest;
            let half_dash = dash * 0.5;
            let clamped = clamp(dist_along, -half_dash, half_dash);
            let cap_dist = dist_along - clamped;
            return length(vec2(cap_dist, dist)) - thickness * 0.5;
        }
        case PATTERN_ARROWED: {
            let segment = layer.pattern_param0;
            let gap = layer.pattern_param1;
            let angle = layer.pattern_param2;
            let period = segment + gap;
            let shifted_u = u - dist * tan(angle);
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let half_seg = segment * 0.5;
            let clamped = clamp(dist_along, -half_seg, half_seg);
            let cap_dist = dist_along - clamped;
            return length(vec2(cap_dist, dist)) - thickness * 0.5;
        }
        case PATTERN_DOTTED: {
            let spacing = layer.pattern_param0;
            let radius = layer.pattern_param1;
            let nearest = round(u / spacing) * spacing;
            let dist_to_center = abs(u - nearest);
            return length(vec2(dist_to_center, dist)) - radius;
        }
        default: {
            return dist;
        }
    }
}

// ============================================================================
// SDF Evaluation (Stack-based RPN, per-shape)
// ============================================================================

fn evaluate_sdf(p: vec2<f32>, shape: ShapeInstance) -> SdfResult {
    var stack: array<SdfResult, MAX_STACK>;
    var sp: u32 = 0u;

    let end = shape.ops_offset + shape.ops_count;
    for (var i: u32 = shape.ops_offset; i < end; i++) {
        let op = ops[i];

        switch op.op_type {
            case OP_CIRCLE: {
                stack[sp] = sd_circle(p, op.param0.xy, op.param0.z);
                sp++;
            }
            case OP_BOX: {
                stack[sp] = sd_box(p, op.param0.xy, op.param0.zw);
                sp++;
            }
            case OP_ROUNDED_BOX: {
                stack[sp] = sd_rounded_box(p, op.param0.xy, op.param0.zw, op.param1.x);
                sp++;
            }
            case OP_LINE: {
                stack[sp] = sd_line(p, op.param0.xy, op.param0.zw);
                sp++;
            }
            case OP_BEZIER: {
                stack[sp] = sd_bezier(p, op.param0.xy, op.param0.zw, op.param1.xy, op.param1.zw);
                sp++;
            }
            case OP_ELLIPSE: {
                stack[sp] = SdfResult(sd_ellipse(p, op.param0.xy), 0.0);
                sp++;
            }
            case OP_TRIANGLE: {
                stack[sp] = SdfResult(sd_triangle(p, op.param0.xy, op.param0.zw, op.param1.xy), 0.0);
                sp++;
            }
            case OP_EQUILATERAL_TRIANGLE: {
                stack[sp] = SdfResult(sd_equilateral_triangle(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_ISOSCELES_TRIANGLE: {
                stack[sp] = SdfResult(sd_isosceles_triangle(p, op.param0.xy), 0.0);
                sp++;
            }
            case OP_RHOMBUS: {
                stack[sp] = SdfResult(sd_rhombus(p, op.param0.xy), 0.0);
                sp++;
            }
            case OP_TRAPEZOID: {
                stack[sp] = SdfResult(sd_trapezoid(p, op.param0.x, op.param0.y, op.param0.z), 0.0);
                sp++;
            }
            case OP_PARALLELOGRAM: {
                stack[sp] = SdfResult(sd_parallelogram(p, op.param0.x, op.param0.y, op.param0.z), 0.0);
                sp++;
            }
            case OP_PENTAGON: {
                stack[sp] = SdfResult(sd_pentagon(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_HEXAGON: {
                stack[sp] = SdfResult(sd_hexagon(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_OCTAGON: {
                stack[sp] = SdfResult(sd_octagon(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_HEXAGRAM: {
                stack[sp] = SdfResult(sd_hexagram(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_STAR: {
                stack[sp] = SdfResult(sd_star(p, op.param0.x, i32(op.param0.y), op.param0.z), 0.0);
                sp++;
            }
            case OP_PIE: {
                stack[sp] = SdfResult(sd_pie(p, op.param0.xy, op.param0.z), 0.0);
                sp++;
            }
            case OP_ARC: {
                stack[sp] = SdfResult(sd_arc(p, op.param0.xy, op.param0.z, op.param0.w), 0.0);
                sp++;
            }
            case OP_CUT_DISK: {
                stack[sp] = SdfResult(sd_cut_disk(p, op.param0.x, op.param0.y), 0.0);
                sp++;
            }
            case OP_HEART: {
                stack[sp] = SdfResult(sd_heart(p), 0.0);
                sp++;
            }
            case OP_EGG: {
                stack[sp] = SdfResult(sd_egg(p, op.param0.x, op.param0.y), 0.0);
                sp++;
            }
            case OP_MOON: {
                stack[sp] = SdfResult(sd_moon(p, op.param0.x, op.param0.y, op.param0.z), 0.0);
                sp++;
            }
            case OP_VESICA: {
                stack[sp] = SdfResult(sd_vesica(p, op.param0.x, op.param0.y), 0.0);
                sp++;
            }
            case OP_UNEVEN_CAPSULE: {
                stack[sp] = SdfResult(sd_uneven_capsule(p, op.param0.x, op.param0.y, op.param0.z), 0.0);
                sp++;
            }
            case OP_ORIENTED_BOX: {
                stack[sp] = SdfResult(sd_oriented_box(p, op.param0.xy, op.param0.zw, op.param1.x), 0.0);
                sp++;
            }
            case OP_HORSESHOE: {
                stack[sp] = SdfResult(sd_horseshoe(p, op.param0.xy, op.param0.z, op.param1.xy), 0.0);
                sp++;
            }
            case OP_ROUNDED_X: {
                stack[sp] = SdfResult(sd_rounded_x(p, op.param0.x, op.param0.y), 0.0);
                sp++;
            }
            case OP_CROSS: {
                stack[sp] = SdfResult(sd_cross(p, op.param0.xy, op.param0.z), 0.0);
                sp++;
            }
            case OP_QUAD_BEZIER: {
                stack[sp] = SdfResult(sd_quad_bezier(p, op.param0.xy, op.param0.zw, op.param1.xy), 0.0);
                sp++;
            }
            case OP_PARABOLA: {
                stack[sp] = SdfResult(sd_parabola(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_COOL_S: {
                stack[sp] = SdfResult(sd_cool_s(p), 0.0);
                sp++;
            }
            case OP_BLOBBY_CROSS: {
                stack[sp] = SdfResult(sd_blobby_cross(p, op.param0.x), 0.0);
                sp++;
            }
            case OP_UNION: {
                sp--; let b = stack[sp];
                sp--; let a = stack[sp];
                stack[sp] = op_union(a, b);
                sp++;
            }
            case OP_SUBTRACT: {
                sp--; let b = stack[sp];
                sp--; let a = stack[sp];
                stack[sp] = op_subtract(a, b);
                sp++;
            }
            case OP_INTERSECT: {
                sp--; let b = stack[sp];
                sp--; let a = stack[sp];
                stack[sp] = op_intersect(a, b);
                sp++;
            }
            case OP_SMOOTH_UNION: {
                sp--; let b = stack[sp];
                sp--; let a = stack[sp];
                stack[sp] = op_smooth_union(a, b, op.param0.x);
                sp++;
            }
            case OP_SMOOTH_SUBTRACT: {
                sp--; let b = stack[sp];
                sp--; let a = stack[sp];
                stack[sp] = op_smooth_subtract(a, b, op.param0.x);
                sp++;
            }
            case OP_ROUND: {
                sp--; let a = stack[sp];
                stack[sp] = op_round(a, op.param0.x);
                sp++;
            }
            case OP_ONION: {
                sp--; let a = stack[sp];
                stack[sp] = op_onion(a, op.param0.x);
                sp++;
            }
            default: {}
        }
    }

    if sp > 0u {
        return stack[sp - 1u];
    }
    return SdfResult(1e10, 0.0);
}

// ============================================================================
// Layer Rendering
// ============================================================================

fn render_layer(sdf: SdfResult, layer: SdfLayer) -> vec4<f32> {
    // Distance field visualization (IQ/Shadertoy style)
    if (layer.flags & LAYER_FLAG_DISTANCE_FIELD) != 0u {
        let d = sdf.dist;
        let outside_col = layer.color.rgb;
        let inside_col = layer.gradient_color.rgb;

        // Scale distance to screen pixels, then normalize for band spacing.
        // This ensures consistent band width regardless of world-space scale.
        let dn = d * uniforms.camera_zoom * uniforms.scale_factor * 0.003;

        // Base color: outside vs inside
        var col = select(inside_col, outside_col, d > 0.0);
        // Darken near the boundary
        col *= 1.0 - exp(-6.0 * abs(dn));
        // Distance bands
        col *= 0.8 + 0.2 * cos(150.0 * dn);
        // White boundary line (1px wide in screen space)
        let pixel_dist = abs(d) * uniforms.camera_zoom * uniforms.scale_factor;
        col = mix(col, vec3(1.0), 1.0 - smoothstep(0.0, 1.5, pixel_dist));

        return vec4(col, 1.0);
    }

    var d: f32;

    if (layer.flags & LAYER_FLAG_HAS_PATTERN) != 0u {
        d = apply_pattern(sdf, layer);
    } else {
        d = sdf.dist - layer.expand;
    }

    var alpha: f32;
    if layer.blur > 0.0 {
        alpha = 1.0 - smoothstep(-layer.blur, layer.blur, d);
    } else {
        alpha = select(0.0, 1.0, d < 0.0);
    }

    var color = layer.color;

    if (layer.flags & LAYER_FLAG_GRADIENT) != 0u {
        var t: f32;
        if (layer.flags & LAYER_FLAG_GRADIENT_U) != 0u {
            t = fract(sdf.u * 0.01);
        } else {
            t = 0.5;
        }
        color = mix(layer.color, layer.gradient_color, t);
    }

    return vec4(color.rgb, color.a * alpha);
}

// ============================================================================
// Vertex/Fragment Shaders (Instanced Quads)
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_pos: vec2<f32>,
    @location(1) @interpolate(flat) instance_id: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let shape = shapes[instance_index];

    // Quad corners: 2 triangles from 6 vertices
    var quad = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let corner = quad[vertex_index];

    // Shape bounds in logical pixels, convert to physical
    let screen_pos = (shape.bounds.xy + corner * shape.bounds.zw) * uniforms.scale_factor;

    // NDC relative to widget bounds (Iced sets scissor/viewport to bounds)
    let ndc = (screen_pos - uniforms.bounds_origin) / uniforms.bounds_size * 2.0 - 1.0;

    // Convert screen position to world coordinates for SDF evaluation
    let world_pos = screen_pos / (uniforms.camera_zoom * uniforms.scale_factor) - uniforms.camera_position;

    var out: VertexOutput;
    out.position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    out.world_pos = world_pos;
    out.instance_id = instance_index;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let shape = shapes[in.instance_id];

    // Use interpolated world position from vertex shader
    let sdf = evaluate_sdf(in.world_pos, shape);

    // Composite layers (back to front)
    var color = vec4(0.0);
    let end = shape.layers_offset + shape.layers_count;
    for (var i: u32 = shape.layers_offset; i < end; i++) {
        let layer = layers[i];
        let layer_color = render_layer(sdf, layer);
        color = color * (1.0 - layer_color.a) + layer_color;
    }

    // Discard fully transparent pixels
    if color.a < 0.001 {
        discard;
    }

    return color;
}
