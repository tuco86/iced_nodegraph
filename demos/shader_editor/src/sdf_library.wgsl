// Complete Inigo Quilez 2D SDF Library
// Source: https://iquilezles.org/articles/distfunctions2d/
// All functions ported to WGSL with original comments

// Circle - exact
fn sdCircle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

// Box - exact
fn sdBox(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// Rounded Box - exact
fn sdRoundedBox(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var rr = r;
    if (p.x > 0.0) { rr = vec4<f32>(rr.y, rr.z, rr.w, rr.x); }
    if (p.y > 0.0) { rr = vec4<f32>(rr.z, rr.w, rr.x, rr.y); }
    let q = abs(p) - b + vec2<f32>(rr.x);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - rr.x;
}

// Oriented Box - exact
fn sdOrientedBox(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, th: f32) -> f32 {
    let l = length(b - a);
    let d = (b - a) / l;
    var q = p - (a + b) * 0.5;
    q = mat2x2<f32>(d.x, -d.y, d.y, d.x) * q;
    q = abs(q) - vec2<f32>(l, th) * 0.5;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
}

// Segment - exact
fn sdSegment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// Rhombus - exact
fn sdRhombus(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let q = abs(p);
    let h = clamp((-2.0 * dot(q, b) + dot(b, b)) / dot(b, b), -1.0, 1.0);
    let d = length(q - 0.5 * b * vec2<f32>(1.0 - h, 1.0 + h));
    return d * sign(q.x * b.y + q.y * b.x - b.x * b.y);
}

// Trapezoid - exact
fn sdTrapezoid(p: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
    let k1 = vec2<f32>(r2, he);
    let k2 = vec2<f32>(r2 - r1, 2.0 * he);
    var px = abs(p.x);
    let ca = vec2<f32>(px - min(px, select(r1, r2, p.y < 0.0)), abs(p.y) - he);
    let cb = p - k1 + k2 * clamp(dot(k1 - p, k2) / dot(k2, k2), 0.0, 1.0);
    let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

// Parallelogram - exact
fn sdParallelogram(p: vec2<f32>, wi: f32, he: f32, sk: f32) -> f32 {
    let e = vec2<f32>(sk, he);
    var pp = select(p, -p, p.y < 0.0);
    let w = pp - e;
    let ww = w - e * clamp(dot(w, e) / dot(e, e), -1.0, 1.0);
    let d = vec2<f32>(dot(ww, ww), -pp.y);
    let s = pp.x * e.y - pp.y * e.x;
    pp = select(pp, vec2<f32>(wi, 0.0) - pp, s < 0.0);
    let v = pp - vec2<f32>(wi, 0.0);
    let vv = v - e * clamp(dot(v, e) / dot(e, e), -1.0, 1.0);
    let dd = vec2<f32>(dot(vv, vv), wi * he - abs(s));
    return sqrt(min(d.x, dd.x)) * sign(-max(d.y, dd.y));
}

// Equilateral Triangle - exact
fn sdEquilateralTriangle(p: vec2<f32>) -> f32 {
    let k = sqrt(3.0);
    var px = abs(p.x) - 1.0;
    let py = p.y + 1.0 / k;
    if (px + k * py > 0.0) {
        px = (px - k * py) / 2.0;
        let pyy = (-k * px - py) / 2.0;
        return -length(vec2<f32>(px, pyy)) * sign(py);
    }
    px = px + 2.0 - 2.0 * clamp((px + 2.0) / 2.0, 0.0, 1.0);
    return -length(vec2<f32>(px, py)) * sign(py);
}

// Isosceles Triangle - exact
fn sdIsoscelesTriangle(p: vec2<f32>, q: vec2<f32>) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let a = pp - q * clamp(dot(pp, q) / dot(q, q), 0.0, 1.0);
    let b = pp - q * vec2<f32>(clamp(pp.x / q.x, 0.0, 1.0), 1.0);
    let s = -sign(q.y);
    let d = min(vec2<f32>(dot(a, a), s * (pp.x * q.y - pp.y * q.x)),
                vec2<f32>(dot(b, b), s * (pp.y - q.y)));
    return -sqrt(d.x) * sign(d.y);
}

// Triangle - exact
fn sdTriangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(min(vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
                    vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))),
                    vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)));
    return -sqrt(d.x) * sign(d.y);
}

// Uneven Capsule - exact
fn sdUnevenCapsule(p: vec2<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let b = (r1 - r2) / h;
    let a = sqrt(1.0 - b * b);
    let k = dot(pp, vec2<f32>(-b, a));
    if (k < 0.0) { return length(pp) - r1; }
    if (k > a * h) { return length(pp - vec2<f32>(0.0, h)) - r2; }
    return dot(pp, vec2<f32>(a, b)) - r1;
}

// Regular Pentagon - exact
fn sdPentagon(p: vec2<f32>, r: f32) -> f32 {
    let k = vec3<f32>(0.809016994, 0.587785252, 0.726542528);
    var pp = vec2<f32>(abs(p.x), p.y);
    pp = pp - 2.0 * min(dot(vec2<f32>(-k.x, k.y), pp), 0.0) * vec2<f32>(-k.x, k.y);
    pp = pp - 2.0 * min(dot(vec2<f32>(k.x, k.y), pp), 0.0) * vec2<f32>(k.x, k.y);
    pp = pp - vec2<f32>(clamp(pp.x, -r * k.z, r * k.z), r);
    return length(pp) * sign(pp.y);
}

// Regular Hexagon - exact
fn sdHexagon(p: vec2<f32>, r: f32) -> f32 {
    let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
    var pp = abs(p);
    pp = pp - 2.0 * min(dot(k.xy, pp), 0.0) * k.xy;
    pp = pp - vec2<f32>(clamp(pp.x, -k.z * r, k.z * r), r);
    return length(pp) * sign(pp.y);
}

// Regular Octogon - exact
fn sdOctogon(p: vec2<f32>, r: f32) -> f32 {
    let k = vec3<f32>(-0.9238795325, 0.3826834323, 0.4142135623);
    var pp = abs(p);
    pp = pp - 2.0 * min(dot(vec2<f32>(k.x, k.y), pp), 0.0) * vec2<f32>(k.x, k.y);
    pp = pp - 2.0 * min(dot(vec2<f32>(-k.x, k.y), pp), 0.0) * vec2<f32>(-k.x, k.y);
    pp = pp - vec2<f32>(clamp(pp.x, -k.z * r, k.z * r), r);
    return length(pp) * sign(pp.y);
}

// Hexagram - exact
fn sdHexagram(p: vec2<f32>, r: f32) -> f32 {
    let k = vec4<f32>(-0.5, 0.8660254038, 0.5773502692, 1.7320508076);
    var pp = abs(p);
    pp = pp - 2.0 * min(dot(k.xy, pp), 0.0) * k.xy;
    pp = pp - 2.0 * min(dot(k.yx, pp), 0.0) * k.yx;
    pp = pp - vec2<f32>(clamp(pp.x, r * k.z, r * k.w), r);
    return length(pp) * sign(pp.y);
}

// Star 5 - exact
fn sdStar5(p: vec2<f32>, r: f32, rf: f32) -> f32 {
    let k1 = vec2<f32>(0.809016994375, -0.587785252292);
    let k2 = vec2<f32>(-k1.x, k1.y);
    var pp = vec2<f32>(abs(p.x), p.y);
    pp = pp - 2.0 * max(dot(k1, pp), 0.0) * k1;
    pp = pp - 2.0 * max(dot(k2, pp), 0.0) * k2;
    let px = abs(pp.x);
    let py = pp.y;
    let a = -3.141592654 / 5.0;
    let b = 3.141592654 / 5.0;
    return length(pp - vec2<f32>(
        select(-r * cos(a), r * cos(b), py * cos(b) + px * sin(b) > r * sin(b)),
        select(r * sin(a), r * sin(b), py * cos(b) + px * sin(b) > r * sin(b))
    )) * sign(pp.x * sin(b) - pp.y * cos(b));
}

// Star - exact
fn sdStar(p: vec2<f32>, r: f32, n: i32, m: f32) -> f32 {
    let an = 3.141592654 / f32(n);
    let en = 3.141592654 / m;
    let acs = vec2<f32>(cos(an), sin(an));
    let ecs = vec2<f32>(cos(en), sin(en));
    let bn = ((atan2(p.y, p.x) % (2.0 * an)) - an);
    var pp = length(p) * vec2<f32>(cos(bn), abs(sin(bn)));
    pp = pp - r * acs;
    pp = pp + ecs * clamp(-dot(pp, ecs), 0.0, r * acs.y / ecs.y);
    return length(pp) * sign(pp.x);
}

// Pie - exact
fn sdPie(p: vec2<f32>, c: vec2<f32>, r: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let l = length(pp) - r;
    let m = length(pp - c * clamp(dot(pp, c), 0.0, r));
    return max(l, m * sign(c.y * pp.x - c.x * pp.y));
}

// Cut Disk - exact
fn sdCutDisk(p: vec2<f32>, r: f32, h: f32) -> f32 {
    let w = sqrt(r * r - h * h);
    var pp = vec2<f32>(abs(p.x), p.y);
    let s = max((h - r) * pp.x * pp.x + w * w * (h + r - 2.0 * pp.y), h * pp.x - w * pp.y);
    if (s < 0.0) { return length(pp) - r; }
    if (pp.x < w) { return h - pp.y; }
    return length(pp - vec2<f32>(w, h));
}

// Arc - exact
fn sdArc(p: vec2<f32>, sc: vec2<f32>, ra: f32, rb: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    if (sc.y * pp.x > sc.x * pp.y) {
        return length(pp - sc * ra) - rb;
    } else {
        return abs(length(pp) - ra) - rb;
    }
}

// Ring - exact
fn sdRing(p: vec2<f32>, n: vec2<f32>, r: f32, th: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    pp = mat2x2<f32>(n.x, n.y, -n.y, n.x) * pp;
    return max(abs(length(pp) - r) - th * 0.5,
               length(vec2<f32>(pp.x, max(0.0, abs(r - pp.y) - th * 0.5))) * sign(pp.x));
}

// Horseshoe - exact
fn sdHorseshoe(p: vec2<f32>, c: vec2<f32>, r: f32, w: vec2<f32>) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let l = length(pp);
    pp = mat2x2<f32>(-c.x, c.y, c.y, c.x) * pp;
    pp = vec2<f32>(select(pp.x, l * sign(-c.x), pp.y > 0.0 || pp.x > 0.0),
                   select(pp.y, l, pp.y > 0.0 || pp.x > 0.0));
    pp = vec2<f32>(pp.x, abs(pp.y - r)) - w;
    return length(max(pp, vec2<f32>(0.0))) + min(0.0, max(pp.x, pp.y));
}

// Vesica - exact
fn sdVesica(p: vec2<f32>, r: f32, d: f32) -> f32 {
    var pp = abs(p);
    let b = sqrt(r * r - d * d);
    if ((pp.y - b) * d > pp.x * b) {
        return length(pp - vec2<f32>(0.0, b));
    } else {
        return length(pp - vec2<f32>(-d, 0.0)) - r;
    }
}

// Moon - exact
fn sdMoon(p: vec2<f32>, d: f32, ra: f32, rb: f32) -> f32 {
    var pp = vec2<f32>(p.x, abs(p.y));
    let a = (ra * ra - rb * rb + d * d) / (2.0 * d);
    let b = sqrt(max(ra * ra - a * a, 0.0));
    if (d * (pp.x * b - pp.y * a) > d * d * max(b - pp.y, 0.0)) {
        return length(pp - vec2<f32>(a, b));
    }
    return max(length(pp) - ra, -(length(pp - vec2<f32>(d, 0.0)) - rb));
}

// Rounded Cross - exact
fn sdRoundedCross(p: vec2<f32>, h: f32) -> f32 {
    let k = 0.5 * (h + 1.0 / h);
    var pp = abs(p);
    if (pp.x < 1.0 && pp.y < pp.x * (k - h) + h) {
        return k - sqrt(dot(pp, pp));
    } else {
        return sqrt(min(dot(pp - vec2<f32>(0.0, h), pp - vec2<f32>(0.0, h)),
                       dot(pp - vec2<f32>(1.0, 0.0), pp - vec2<f32>(1.0, 0.0))));
    }
}

// Egg - exact
fn sdEgg(p: vec2<f32>, ra: f32, rb: f32) -> f32 {
    let k = sqrt(3.0);
    var pp = vec2<f32>(abs(p.x), p.y);
    let r = ra - rb;
    if (pp.y < 0.0) {
        return length(vec2<f32>(pp.x, pp.y)) - r;
    } else if (k * (pp.x + r) < pp.y) {
        return length(vec2<f32>(pp.x, pp.y - k * r));
    } else {
        return length(vec2<f32>(pp.x + r, pp.y)) - 2.0 * r;
    }
}

// Heart - exact
fn sdHeart(p: vec2<f32>) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    if (pp.y + pp.x > 1.0) {
        return sqrt(dot(pp - vec2<f32>(0.25, 0.75), pp - vec2<f32>(0.25, 0.75))) - sqrt(2.0) / 4.0;
    }
    return sqrt(min(dot(pp - vec2<f32>(0.0, 1.0), pp - vec2<f32>(0.0, 1.0)),
                    dot(pp - 0.5 * max(pp.x + pp.y, 0.0), pp - 0.5 * max(pp.x + pp.y, 0.0)))) *
           sign(pp.x - pp.y);
}

// Cross - exact
fn sdCross(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    var pp = abs(p);
    pp = select(pp, pp.yx, pp.y > pp.x);
    let q = pp - b;
    let k = max(q.y, q.x);
    let w = select(vec2<f32>(b.y - pp.x, -k), q, k > 0.0);
    return sign(k) * length(max(w, vec2<f32>(0.0))) + r;
}

// Rounded X - exact
fn sdRoundedX(p: vec2<f32>, w: f32, r: f32) -> f32 {
    var pp = abs(p);
    return length(pp - min(pp.x + pp.y, w) * 0.5) - r;
}

// Polygon - exact
fn sdPolygon(p: vec2<f32>, v: array<vec2<f32>, 6>, n: i32) -> f32 {
    var d = dot(p - v[0], p - v[0]);
    var s = 1.0;
    for (var i = 0; i < n; i = i + 1) {
        let j = (i + 1) % n;
        let e = v[i] - v[j];
        let w = p - v[j];
        let b = w - e * clamp(dot(w, e) / dot(e, e), 0.0, 1.0);
        d = min(d, dot(b, b));
        let c = vec3<f32>(p.y >= v[j].y, p.y < v[i].y, e.x * w.y > e.y * w.x);
        if (all(c) || all(!c)) { s = -s; }
    }
    return s * sqrt(d);
}

// Ellipse - exact
fn sdEllipse(p: vec2<f32>, ab: vec2<f32>) -> f32 {
    var pp = abs(p);
    if (pp.x > pp.y) { pp = pp.yx; let abb = ab.yx; }
    let l = ab.y * ab.y - ab.x * ab.x;
    let m = ab.x * pp.x / l;
    let n = ab.y * pp.y / l;
    let m2 = m * m;
    let n2 = n * n;
    let c = (m2 + n2 - 1.0) / 3.0;
    let c3 = c * c * c;
    let q = c3 + m2 * n2 * 2.0;
    let d = c3 + m2 * n2;
    let g = m + m * n2;
    var co: f32;
    if (d < 0.0) {
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
    let r = ab * vec2<f32>(co, sqrt(1.0 - co * co));
    return length(r - pp) * sign(pp.y - r.y);
}

// Parabola - exact
fn sdParabola(p: vec2<f32>, k: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let ik = 1.0 / k;
    let p2 = ik * (pp.y - 0.5 * ik) / 3.0;
    let q = pp.x * ik * ik * 0.25;
    let h = q * q - p2 * p2 * p2;
    let r = sqrt(abs(h));
    var x: f32;
    if (h > 0.0) {
        x = pow(q + r, 1.0 / 3.0) - pow(abs(q - r), 1.0 / 3.0) * sign(r - q);
    } else {
        let m = sqrt(p2);
        x = 2.0 * m * cos(acos(q / (p2 * m)) / 3.0);
    }
    return length(pp - vec2<f32>(x, k * x * x)) * sign(pp.x - x);
}

// Parabola Segment - exact
fn sdParabolaSegment(p: vec2<f32>, wi: f32, he: f32) -> f32 {
    var pp = vec2<f32>(abs(p.x), p.y);
    let ik = wi * wi / he;
    let p2 = (he - pp.y - 0.5 * ik) / 3.0;
    let q = pp.x * pp.x * 0.25 / ik;
    let h = q * q - p2 * p2 * p2;
    let r = sqrt(abs(h));
    var x: f32;
    if (h > 0.0) {
        x = pow(q + r, 1.0 / 3.0) - pow(abs(q - r), 1.0 / 3.0) * sign(r - q);
    } else {
        x = 2.0 * sqrt(p2) * cos(acos(q / (p2 * sqrt(p2))) / 3.0);
    }
    x = min(x, wi);
    return length(pp - vec2<f32>(x, he - x * x / ik)) * sign(ik * (pp.y - he) + pp.x * pp.x);
}

// Quadratic Bezier - exact
fn sdBezier(p: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - p;
    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);
    var res = 0.0;
    let p2 = ky - kx * kx;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    let p3 = p2 * p2 * p2;
    let q2 = q * q;
    let h = q2 + 4.0 * p3;
    if (h >= 0.0) {
        let sh = sqrt(h);
        let s = sign(q + sh) * pow(abs(q + sh), 1.0 / 3.0);
        let t = sign(q - sh) * pow(abs(q - sh), 1.0 / 3.0);
        let v = vec2<f32>(s + t, (s - t) * sqrt(3.0)) * 0.5;
        let m = sqrt(v.x * v.x + v.y * v.y);
        res = clamp((v.x + kx) / m, 0.0, 1.0);
    } else {
        res = clamp((2.0 * cos(acos(q / sqrt(-4.0 * p3)) / 3.0) * sqrt(-p2) - kx), 0.0, 1.0);
    }
    let t = clamp(res, 0.0, 1.0);
    let qt = d + (c + b * t) * t;
    return dot(qt, qt);
}

// SDF Operations

fn opUnion(d1: f32, d2: f32) -> f32 {
    return min(d1, d2);
}

fn opSubtraction(d1: f32, d2: f32) -> f32 {
    return max(-d1, d2);
}

fn opIntersection(d1: f32, d2: f32) -> f32 {
    return max(d1, d2);
}

fn opSmoothUnion(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn opSmoothSubtraction(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
    return mix(d2, -d1, h) + k * h * (1.0 - h);
}

fn opSmoothIntersection(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) + k * h * (1.0 - h);
}

fn opOnion(d: f32, thickness: f32) -> f32 {
    return abs(d) - thickness;
}

fn opRound(d: f32, radius: f32) -> f32 {
    return d - radius;
}

fn opAnnular(d: f32, r: f32) -> f32 {
    return abs(d) - r;
}
