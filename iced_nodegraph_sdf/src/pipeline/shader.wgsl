// Segment-based SDF renderer with per-segment tile spatial index.
// Tile slots store (segment_idx, style_idx) pairs as 2x u32.
// Compute evaluates individual segments, fragment just iterates tile slots.

// --- Constants ---

const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 32u;
// Each slot = 2 u32s (segment_idx, style_idx), so buffer stride = MAX_SLOTS * 2
const SLOT_STRIDE: u32 = 64u; // MAX_SLOTS_PER_TILE * 2

const SEG_LINE: u32 = 0u;
const SEG_ARC: u32 = 1u;
const SEG_CUBIC: u32 = 2u;
const SEG_POINT: u32 = 3u;

const ENTRY_CURVE: u32 = 0u;
const ENTRY_SHAPE: u32 = 1u;
const ENTRY_TILING: u32 = 2u;
// Marker bit: slot segment_idx with this bit set = tiling (segment_idx = entry_idx)
const TILING_BIT: u32 = 0x80000000u;

// entry.flags
const FLAG_CLOSED: u32 = 1u;

// style.flags
const STYLE_FLAG_HAS_PATTERN: u32 = 1u;
const STYLE_FLAG_DISTANCE_FIELD: u32 = 2u;

// Debug visualization modes (DrawData.debug_flags). Mirror `DebugFlags` in
// primitive.rs.
const DEBUG_TILE_HEATMAP: u32 = 1u;
const DEBUG_DISTANCE_FIELD: u32 = 2u;
const DEBUG_HOVERED_TILE: u32 = 4u;

// segment.flags (in _pad0 slot)
const SEG_FLAG_SIGNED: u32 = 1u;

const PATTERN_SOLID: u32 = 0u;
const PATTERN_DASHED: u32 = 1u;
const PATTERN_ARROWED: u32 = 2u;
const PATTERN_DOTTED: u32 = 3u;
const PATTERN_DASH_DOTTED: u32 = 4u;
const PATTERN_ARROW_DOTTED: u32 = 5u;

// --- Data structures ---

struct DrawData {
    bounds_origin: vec2<f32>,
    camera_position: vec2<f32>,
    camera_zoom: f32,
    scale_factor: f32,
    time: f32,
    debug_flags: u32,
    entry_count: u32,
    entry_start: u32,
    grid_cols: u32,
    grid_rows: u32,
    tile_base: u32,
    _pad0: u32,
    mouse_px: vec2<f32>,
}

struct GpuSegment {
    segment_type: u32,
    flags: u32,
    _pad1: u32,
    _pad2: u32,
    geom0: vec4<f32>,
    geom1: vec4<f32>,
    arc_range: vec4<f32>,
}

struct GpuDrawEntry {
    entry_type: u32,
    style_idx: u32,
    z_order: u32,
    flags: u32,
    bounds: vec4<f32>,
    segment_start: u32,
    segment_count: u32,
    tiling_type: u32,
    _pad: u32,
    tiling_params: vec4<f32>,
    // Per-INSTANCE placement (D1): the entry's segments are local; evaluate at
    // `world_p - translate`. `(0,0)` (v2 default) leaves geometry world-baked.
    translate: vec2<f32>,
    _translate_pad: vec2<f32>,
}

// MAX_STOPS must match `style::MAX_STOPS` on the Rust side.
const MAX_STOPS: u32 = 8u;

struct GpuStyle {
    stop_start: array<vec4<f32>, 8>,
    stop_end: array<vec4<f32>, 8>,
    stop_dist: array<vec4<f32>, 2>,
    stop_count: u32,
    flags: u32,
    pattern_type: u32,
    pattern_thickness: f32,
    pattern_param0: f32,
    pattern_param1: f32,
    pattern_param2: f32,
    flow_speed: f32,
    transfer_type: u32,
    transfer_param: f32,
    _transfer_pad0: u32,
    _transfer_pad1: u32,
}

// A3 transfer (variant B): a color-domain warp on the post-smoothstep blend t.
// 0=linear (identity), 1=smoothstep, 2=gamma(param). Never touches `dist`.
fn apply_transfer(t: f32, kind: u32, param: f32) -> f32 {
    if kind == 1u {
        return t * t * (3.0 - 2.0 * t);
    } else if kind == 2u {
        return pow(t, max(param, 1e-4));
    }
    return t;
}

// Signed distance of stop `i` (distances are packed 4 per vec4).
fn stop_dist_at(style: GpuStyle, i: u32) -> f32 {
    return style.stop_dist[i / 4u][i % 4u];
}

// Largest active stop distance (outermost extent on the distance axis).
fn style_max_dist(style: GpuStyle) -> f32 {
    var m = stop_dist_at(style, 0u);
    for (var i = 1u; i < style.stop_count; i++) {
        m = max(m, stop_dist_at(style, i));
    }
    return m;
}

// Perpendicular half-reach of a pattern's stroke: the widest distance a feature
// can occupy ACROSS the contour, independent of `time` and the along-u dash/dot
// layout (C1). Half thickness for line-like patterns; the dot radius can exceed
// it for the dotted families.
fn pattern_perp_reach(style: GpuStyle) -> f32 {
    let half_t = style.pattern_thickness * 0.5;
    switch style.pattern_type {
        case PATTERN_DOTTED: { return max(half_t, style.pattern_param1); }
        case PATTERN_DASH_DOTTED, PATTERN_ARROW_DOTTED: { return max(half_t, style.pattern_param2); }
        default: { return half_t; }
    }
}

struct ComputeUniforms {
    draw_index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct SdfResult {
    dist: f32,  // signed distance (positive = right side of curve)
    u: f32,     // parametric position along curve [0..1]
}

// --- Render bindings (group 0) ---

@group(0) @binding(0) var<storage, read> draws: array<DrawData>;
@group(0) @binding(1) var<storage, read> draw_entries: array<GpuDrawEntry>;
@group(0) @binding(2) var<storage, read> segments: array<GpuSegment>;
@group(0) @binding(3) var<storage, read> styles: array<GpuStyle>;
@group(0) @binding(4) var<storage, read> tile_counts: array<u32>;
@group(0) @binding(5) var<storage, read> tile_slots: array<u32>;

// --- Compute bindings ---

@group(0) @binding(0) var<storage, read> cs_draws: array<DrawData>;
@group(0) @binding(1) var<storage, read> cs_entries: array<GpuDrawEntry>;
@group(0) @binding(2) var<storage, read> cs_segments: array<GpuSegment>;
@group(0) @binding(3) var<storage, read> cs_styles: array<GpuStyle>;

@group(1) @binding(0) var<uniform> cs_uniforms: ComputeUniforms;
@group(1) @binding(1) var<storage, read_write> cs_tile_counts: array<u32>;
@group(1) @binding(2) var<storage, read_write> cs_tile_slots: array<u32>;

// ============================================================================
// SDF Distance Functions
// ============================================================================

fn sd_line(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> SdfResult {
    let ba = b - a;
    let pa = p - a;
    let len_sq = dot(ba, ba);
    var t = 0.0;
    if len_sq > 0.0 {
        t = clamp(dot(pa, ba) / len_sq, 0.0, 1.0);
    }
    let proj = a + ba * t;
    let dist = length(p - proj);
    // Sign from perpendicular: positive = right side of a→b
    let n = vec2<f32>(-ba.y, ba.x);
    var sign = 1.0;
    if len_sq > 0.0 && dot(pa, n) > 0.0 { sign = -1.0; }
    return SdfResult(dist * sign, t);
}

// Newton refinement of a single Bezier parameter. Returns the converged t.
fn bezier_newton(
    p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t_init: f32,
) -> f32 {
    var t = t_init;
    for (var iter = 0u; iter < 4u; iter++) {
        let bp = bezier_point(p0, p1, p2, p3, t);
        let bd = bezier_deriv(p0, p1, p2, p3, t);
        let bdd = bezier_deriv2(p0, p1, p2, p3, t);
        let diff = bp - p;
        let num = dot(diff, bd);
        let den = dot(bd, bd) + dot(diff, bdd);
        if abs(den) > 1e-8 { t = clamp(t - num / den, 0.0, 1.0); }
    }
    return t;
}

fn sd_bezier(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> SdfResult {
    // Tight or self-overshooting beziers (e.g. an edge being dragged near its
    // origin) have multiple local distance minima. A single 16-sample seed +
    // Newton run frequently snaps to the wrong one, leaving "armpit" tiles
    // with a far-away point as their "nearest" — the culling then drops the
    // tile and the fragment renders garbage. Densely sample and refine the
    // best AND second-best local minimum, then keep the global winner.
    const SAMPLES: u32 = 32u;
    var best1_t = 0.0; var best1_d = 1e20;
    var best2_t = 0.0; var best2_d = 1e20;
    var prev_d = 1e20;
    var prev_t = 0.0;
    var prev_decreasing = true;
    for (var i = 0u; i <= SAMPLES; i++) {
        let t = f32(i) / f32(SAMPLES);
        let bp = bezier_point(p0, p1, p2, p3, t);
        let d = length(p - bp);
        // Track every basin (local min). A basin ends when distance starts
        // increasing again; record that prior sample as a candidate.
        if i > 0u && prev_decreasing && d > prev_d {
            if prev_d < best1_d {
                best2_d = best1_d; best2_t = best1_t;
                best1_d = prev_d;  best1_t = prev_t;
            } else if prev_d < best2_d {
                best2_d = prev_d; best2_t = prev_t;
            }
        }
        prev_decreasing = d < prev_d;
        prev_d = d; prev_t = t;
    }
    // Always also consider the final sample (descending into endpoint).
    if prev_d < best1_d {
        best2_d = best1_d; best2_t = best1_t;
        best1_d = prev_d;  best1_t = prev_t;
    } else if prev_d < best2_d {
        best2_d = prev_d; best2_t = prev_t;
    }

    let t_a = bezier_newton(p, p0, p1, p2, p3, best1_t);
    let bp_a = bezier_point(p0, p1, p2, p3, t_a);
    let d_a = length(p - bp_a);

    var best_t = t_a;
    var closest = bp_a;
    var dist = d_a;
    if best2_d < 1e19 {
        let t_b = bezier_newton(p, p0, p1, p2, p3, best2_t);
        let bp_b = bezier_point(p0, p1, p2, p3, t_b);
        let d_b = length(p - bp_b);
        if d_b < dist {
            best_t = t_b; closest = bp_b; dist = d_b;
        }
    }

    let tangent = bezier_deriv(p0, p1, p2, p3, best_t);
    let normal = vec2<f32>(-tangent.y, tangent.x);
    let diff = p - closest;
    var sign = 1.0;
    let n_len = length(normal);
    if n_len > 1e-8 && dot(diff, normal) > 0.0 { sign = -1.0; }
    let arc_to_t = bezier_arc_length_to(p0, p1, p2, p3, best_t);
    let total_arc = bezier_total_arc_length(p0, p1, p2, p3);
    var u_frac = 0.0;
    if total_arc > 1e-6 { u_frac = arc_to_t / total_arc; }
    return SdfResult(dist * sign, u_frac);
}

fn bezier_point(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3;
}

fn bezier_deriv(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return 3.0 * u * u * (p1 - p0) + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (p3 - p2);
}

fn bezier_deriv2(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return 6.0 * u * (p2 - 2.0 * p1 + p0) + 6.0 * t * (p3 - 2.0 * p2 + p1);
}

fn bezier_arc_length_to(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t_end: f32) -> f32 {
    let w0 = 0.2369268850;
    let w1 = 0.4786286705;
    let w2 = 0.5688888889;
    let a0 = 0.9061798459;
    let a1 = 0.5384693101;
    let half_t = t_end * 0.5;
    var len = 0.0;
    len += w0 * length(bezier_deriv(p0, p1, p2, p3, half_t * (1.0 - a0)));
    len += w1 * length(bezier_deriv(p0, p1, p2, p3, half_t * (1.0 - a1)));
    len += w2 * length(bezier_deriv(p0, p1, p2, p3, half_t));
    len += w1 * length(bezier_deriv(p0, p1, p2, p3, half_t * (1.0 + a1)));
    len += w0 * length(bezier_deriv(p0, p1, p2, p3, half_t * (1.0 + a0)));
    return len * half_t;
}

fn bezier_total_arc_length(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    return bezier_arc_length_to(p0, p1, p2, p3, 1.0);
}

// Exact signed distance to a circular arc segment.
// geom0 = (cx, cy, radius, start_angle), geom1 = (sweep_angle, 0, 0, 0)
fn sd_arc_segment(p: vec2<f32>, center: vec2<f32>, radius: f32, start: f32, sweep: f32) -> SdfResult {
    let offset = p - center;
    let dist_to_center = length(offset);
    let angle = atan2(offset.y, offset.x);

    // Normalize angle relative to arc start. Wrap into the half-turn that
    // matches the sweep direction so the on_arc test below works for arcs
    // wider than PI (full circles in particular — without this the
    // wrap-around half is wrongly classified as off-arc and gets a flipped
    // sign in the else branch, leaking the closed-shape fill outside).
    var rel = angle - start;
    if sweep > 0.0 {
        rel = rel - floor(rel / 6.2831853) * 6.2831853; // → [0, TAU)
    } else {
        rel = rel - ceil(rel / 6.2831853) * 6.2831853;  // → (-TAU, 0]
    }

    let on_arc = (sweep > 0.0 && rel <= sweep)
              || (sweep < 0.0 && rel >= sweep);

    var dist: f32;
    var u_frac: f32;
    var v_sign = 1.0;

    if on_arc {
        dist = abs(dist_to_center - radius);
        u_frac = rel / sweep;
        let v_val = select(-1.0, 1.0, sweep > 0.0) * (radius - dist_to_center);
        if v_val > 0.0 { v_sign = -1.0; }
    } else {
        let end_angle = start + sweep;
        let p_start = center + vec2(cos(start), sin(start)) * radius;
        let p_end = center + vec2(cos(end_angle), sin(end_angle)) * radius;
        let d_start = length(p - p_start);
        let d_end = length(p - p_end);

        if d_start < d_end {
            dist = d_start;
            u_frac = 0.0;
            let tangent = vec2(-sin(start), cos(start)) * sign(sweep);
            let n = vec2(-tangent.y, tangent.x);
            if dot(p - p_start, n) > 0.0 { v_sign = -1.0; }
        } else {
            dist = d_end;
            u_frac = 1.0;
            let tangent = vec2(-sin(end_angle), cos(end_angle)) * sign(sweep);
            let n = vec2(-tangent.y, tangent.x);
            if dot(p - p_end, n) > 0.0 { v_sign = -1.0; }
        }
    }

    return SdfResult(dist * v_sign, u_frac);
}

// Junction point between segments. Owns the corner, defines sign via heading.
// geom0 = (px, py, heading, 0)
fn sd_point(p: vec2<f32>, pos: vec2<f32>, heading: f32) -> SdfResult {
    // Tiny distance advantage so Point always wins over adjacent segment endpoints
    // at the junction. Ensures correct v from bisector heading.
    let dist = max(0.0, length(p - pos) - 0.01);
    let right = vec2(cos(heading), sin(heading));
    var sign = 1.0;
    if dot(p - pos, right) > 0.0 { sign = -1.0; }
    return SdfResult(dist * sign, 0.0);
}

// --- Tiling SDF functions ---

const TILING_GRID: u32 = 0u;
const TILING_DOTS: u32 = 1u;
const TILING_TRIANGLES: u32 = 2u;
const TILING_HEX: u32 = 3u;

fn sd_tiling(p: vec2<f32>, tiling_type: u32, params: vec4<f32>) -> SdfResult {
    let spacing = params.xy;
    switch tiling_type {
        case TILING_GRID: {
            // Unsigned distance to nearest grid line
            let fx = ((p.x % spacing.x) + spacing.x) % spacing.x;
            let mx = min(fx, spacing.x - fx);
            let fy = ((p.y % spacing.y) + spacing.y) % spacing.y;
            let my = min(fy, spacing.y - fy);
            return SdfResult(min(mx, my), 0.0);
        }
        case TILING_DOTS: {
            let radius = params.z;
            // Distance to nearest dot center (modulo spacing)
            let cell = round(p / spacing) * spacing;
            let dist = length(p - cell) - radius;
            return SdfResult(dist, 0.0);
        }
        case TILING_TRIANGLES: {
            let s = params.x;
            let h = s * 0.866025404;  // sqrt(3)/2 * edge length
            // Three line-family projections (normals at 0, 60, -60 degrees)
            let d1 = p.y;
            let d2 = 0.866025404 * p.x + 0.5 * p.y;
            let d3 = 0.866025404 * p.x - 0.5 * p.y;
            // Unsigned distance to nearest line in each family
            let f1 = ((d1 % h) + h) % h;
            let m1 = min(f1, h - f1);
            let f2 = ((d2 % h) + h) % h;
            let m2 = min(f2, h - f2);
            let f3 = ((d3 % h) + h) % h;
            let m3 = min(f3, h - f3);
            return SdfResult(min(min(m1, m2), m3), 0.0);
        }
        case TILING_HEX: {
            let size = params.x * 0.5;  // apothem = flat-to-flat / 2
            let s3 = 1.732050808;
            let edge = 2.0 * size / s3;
            // Pixel to axial hex coordinates (flat-top)
            let q = 2.0 / (3.0 * edge) * p.x;
            let r = (-p.x + s3 * p.y) / (3.0 * edge);
            let s_ax = -q - r;
            // Cube round to nearest hex center
            var qi = round(q);
            var ri = round(r);
            var si = round(s_ax);
            let dq = abs(qi - q);
            let dr = abs(ri - r);
            let ds = abs(si - s_ax);
            if dq > dr && dq > ds { qi = -ri - si; }
            else if dr > ds { ri = -qi - si; }
            // Axial to pixel (flat-top)
            let cx = edge * 1.5 * qi;
            let cy = edge * s3 * (0.5 * qi + ri);
            let delta = p - vec2(cx, cy);
            // Unsigned distance to nearest hex edge (IQ's sdHexagon)
            let k = vec3(-0.866025404, 0.5, 0.577350269);
            var d = abs(delta);
            d -= 2.0 * min(dot(k.xy, d), 0.0) * k.xy;
            d -= vec2(clamp(d.x, -k.z * size, k.z * size), size);
            return SdfResult(abs(length(d) * sign(d.y)), 0.0);
        }
        default: {
            return SdfResult(1e10, 0.0);
        }
    }
}

fn eval_segment(p: vec2<f32>, seg: GpuSegment) -> SdfResult {
    // Segments are stored in the entry's local frame; the caller passes
    // `world_p - entry.translate`. Distance is translation-invariant.
    switch seg.segment_type {
        case SEG_LINE: { return sd_line(p, seg.geom0.xy, seg.geom0.zw); }
        case SEG_ARC: { return sd_arc_segment(p, seg.geom0.xy, seg.geom0.z, seg.geom0.w, seg.geom1.x); }
        case SEG_CUBIC: { return sd_bezier(p, seg.geom0.xy, seg.geom0.zw, seg.geom1.xy, seg.geom1.zw); }
        case SEG_POINT: { return sd_point(p, seg.geom0.xy, seg.geom0.z); }
        default: { return SdfResult(1e10, 0.0); }
    }
}

// Evaluate a single segment. Maps parametric u to world-space arc-length.
// dist is already signed from the SDF function.
fn eval_single_segment(p: vec2<f32>, seg_idx: u32) -> SdfResult {
    let seg = segments[seg_idx];
    var r = eval_segment(p, seg);
    let arc_start = seg.arc_range.x;
    let arc_end = seg.arc_range.y;
    r.u = arc_start + r.u * (arc_end - arc_start);
    return r;
}

// Total arc length for a segment (for normalizing u to 0..1 in render_style)
fn segment_total_arc(seg_idx: u32) -> f32 {
    return segments[seg_idx].arc_range.z;
}

// ============================================================================
// Style Rendering
// ============================================================================

// total_arc: total arc-length of the contour (for normalizing u to 0..1)
fn render_style(sdf: SdfResult, style: GpuStyle, draw: DrawData, total_arc: f32) -> vec4<f32> {
    if (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u
        || (draw.debug_flags & DEBUG_DISTANCE_FIELD) != 0u {
        return render_distance_field(sdf.dist, style, draw);
    }

    // Antialiasing half-width, in world units. The contour SDF has |grad|=1 in
    // world space, so one screen pixel spans `1/(zoom*scale)` world units. We
    // derive the AA band analytically instead of with `fwidth(dist)`: the tile
    // loop above is data-dependent (different tiles per pixel), so a derivative
    // quad straddling a tile boundary runs in non-uniform control flow, where
    // screen-space derivatives are undefined per the WGSL spec. On some GPUs
    // that produced a 1px seam at every tile boundary; the analytic form is
    // deterministic everywhere.
    let aa = 1.1 / max(draw.camera_zoom * draw.scale_factor, 1e-6);

    // Normalize u from world-space to 0..1 for color gradient
    var arc_t = 0.0;
    if total_arc > 0.0 { arc_t = clamp(sdf.u / total_arc, 0.0, 1.0); }

    var dist = sdf.dist;

    if (style.flags & STYLE_FLAG_HAS_PATTERN) != 0u {
        // Pattern uses world-space u (sdf.u) for dash layout
        dist = apply_pattern(dist, sdf, style, draw.time);

        let color = mix(style.stop_start[0], style.stop_end[0], arc_t);
        let alpha = color.a * (1.0 - smoothstep(-aa, aa, dist));
        if alpha < 0.001 { return vec4(0.0); }
        return vec4(color.rgb * alpha, alpha);
    }

    // Distance-stop chain: hold the first stop below it, blend each consecutive
    // pair with smoothstep over their interval (widened to at least one pixel so
    // a zero-width step is a crisp AA edge), hold the last stop above it. One
    // continuous evaluation - no per-band compositing, so abutting bands never
    // seam.
    // Blend the stop chain in PREMULTIPLIED space (A3 band-fold fix). Mixing
    // straight-alpha RGBA toward a stop with different alpha pulls RGB toward the
    // (near-)transparent stop's RGB and fringes the falloff - visible on soft
    // shadows/glows where a transparent outer stop meets an opaque one.
    // Premultiplied mixing avoids it. For stops at equal alpha the result is
    // identical to a straight-space mix, so opaque/abutting bands are unchanged.
    let c0 = mix(style.stop_start[0], style.stop_end[0], arc_t);
    var acc = vec4(c0.rgb * c0.a, c0.a);
    for (var i = 0u; i + 1u < style.stop_count; i++) {
        let cj = mix(style.stop_start[i + 1u], style.stop_end[i + 1u], arc_t);
        let pcj = vec4(cj.rgb * cj.a, cj.a);
        var lo = stop_dist_at(style, i);
        var hi = stop_dist_at(style, i + 1u);
        if hi - lo < aa {
            let m = (lo + hi) * 0.5;
            var nlo = m - aa * 0.5;
            var nhi = m + aa * 0.5;
            // A3 widening clamp: a sub-aa interval is widened to >= aa for AA,
            // but if two adjacent intervals both widen and OVERLAP, the thin band
            // between them is attenuated or vanishes. Cap the expansion at the
            // midpoint to each neighbouring stop so widened intervals abut
            // instead of overlapping. (No effect when stops are >~aa apart.)
            if i > 0u {
                nlo = max(nlo, (stop_dist_at(style, i - 1u) + lo) * 0.5);
            }
            if i + 2u < style.stop_count {
                nhi = min(nhi, (hi + stop_dist_at(style, i + 2u)) * 0.5);
            }
            lo = nlo;
            hi = nhi;
        }
        let t = apply_transfer(smoothstep(lo, hi, dist), style.transfer_type, style.transfer_param);
        acc = mix(acc, pcj, t);
    }

    let alpha = acc.a;
    if alpha < 0.001 { return vec4(0.0); }
    // `acc.rgb` is already premultiplied.
    return vec4(acc.rgb, alpha);
}

fn render_distance_field(d: f32, style: GpuStyle, draw: DrawData) -> vec4<f32> {
    let outside_col = style.stop_start[0].rgb;
    let inside_col = style.stop_end[0].rgb;
    let dn = d * draw.camera_zoom * draw.scale_factor * 0.003;
    var col = select(inside_col, outside_col, d > 0.0);
    col *= 1.0 - exp(-6.0 * abs(dn));
    col *= 0.8 + 0.2 * cos(150.0 * dn);
    let pixel_dist = abs(d) * draw.camera_zoom * draw.scale_factor;
    col = mix(col, vec3(1.0), 1.0 - smoothstep(0.0, 1.5, pixel_dist));
    return vec4(col, 1.0);
}

fn apply_pattern(dist: f32, sdf: SdfResult, style: GpuStyle, time: f32) -> f32 {
    let thickness = style.pattern_thickness;
    let half_t = thickness * 0.5;
    var u = sdf.u;
    if style.flow_speed != 0.0 { u = u - time * style.flow_speed; }

    switch style.pattern_type {
        case PATTERN_SOLID: { return abs(dist) - half_t; }
        case PATTERN_DASHED: {
            let dash = style.pattern_param0;
            let gap = style.pattern_param1;
            let angle = style.pattern_param2;
            let period = dash + gap;
            let shifted_u = u + dist * tan(angle);
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let dd = abs(vec2(dist_along, dist)) - vec2(dash * 0.5, half_t);
            let box_d = length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
            // Lipschitz correction (A3): the box is measured in the sheared
            // (u,dist) frame (|grad|=sec(angle)), over-estimating distance by
            // 1/cos(angle). Multiply by cos(angle) to restore |grad|=1 so the
            // analytic AA band on diagonal dash ends is the right width.
            return box_d * cos(angle);
        }
        case PATTERN_ARROWED: {
            let segment = style.pattern_param0;
            let gap = style.pattern_param1;
            let angle = style.pattern_param2;
            let period = segment + gap;
            let shifted_u = u + abs(dist) * tan(angle);
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let dd = abs(vec2(dist_along, dist)) - vec2(segment * 0.5, half_t);
            let box_d = length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
            return box_d * cos(angle); // Lipschitz correction (A3), see PATTERN_DASHED.
        }
        case PATTERN_DOTTED: {
            let spacing = style.pattern_param0;
            let radius = style.pattern_param1;
            let nearest = round(u / spacing) * spacing;
            let dist_to_center = abs(u - nearest);
            return length(vec2(dist_to_center, dist)) - radius;
        }
        case PATTERN_DASH_DOTTED: {
            let dash = style.pattern_param0;
            let gap = style.pattern_param1;
            let dot_radius = style.pattern_param2;
            let period = dash + gap + dot_radius * 2.0 + gap;
            let nearest = round(u / period) * period;
            let local_u = u - nearest;
            let dot_center = dash * 0.5 + gap + dot_radius;
            let dd = abs(vec2(local_u, dist)) - vec2(dash * 0.5, half_t);
            let d_dash = length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
            let d_dot_pos = length(vec2(local_u - dot_center, dist)) - dot_radius;
            let d_dot_neg = length(vec2(local_u + dot_center, dist)) - dot_radius;
            return min(d_dash, min(d_dot_pos, d_dot_neg));
        }
        case PATTERN_ARROW_DOTTED: {
            let segment = style.pattern_param0;
            let gap = style.pattern_param1;
            let dot_radius = style.pattern_param2;
            let angle = 0.5816;
            let period = segment + gap + dot_radius * 2.0 + gap;
            let nearest = round(u / period) * period;
            let local_u = u - nearest;
            let shifted_local = local_u + abs(dist) * tan(angle);
            let dot_center = segment * 0.5 + gap + dot_radius;
            let dd = abs(vec2(shifted_local, dist)) - vec2(segment * 0.5, half_t);
            let d_seg = length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
            let d_dot_pos = length(vec2(local_u - dot_center, dist)) - dot_radius;
            let d_dot_neg = length(vec2(local_u + dot_center, dist)) - dot_radius;
            return min(d_seg, min(d_dot_pos, d_dot_neg));
        }
        default: { return abs(dist) - half_t; }
    }
}

// ============================================================================
// Vertex Shader
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) draw_idx: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VertexOutput {
    let x = f32(i32(vid & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vid >> 1u)) * 4.0 - 1.0;
    return VertexOutput(vec4<f32>(x, y, 0.0, 1.0), iid);
}

// ============================================================================
// Fragment Shader
// ============================================================================

// Hovered-tile inspector: render the IQ distance field built from ONLY the
// segments held by the tile under the cursor, plus an occupancy readout. Makes
// a single tile's 32-slot buffer (and any overflow) directly visible.
fn render_hovered_tile(draw: DrawData, local_px: vec2<f32>, world_p: vec2<f32>) -> vec4<f32> {
    let mcol = u32(draw.mouse_px.x / TILE_SIZE);
    let mrow = u32(draw.mouse_px.y / TILE_SIZE);
    if draw.mouse_px.x < 0.0 || draw.mouse_px.y < 0.0
        || mcol >= draw.grid_cols || mrow >= draw.grid_rows {
        // Cursor outside this layer's grid: contribute nothing (discarded).
        return vec4(0.0);
    }

    let htile = draw.tile_base + mrow * draw.grid_cols + mcol;
    let hcount = tile_counts[htile];
    let hbase = htile * SLOT_STRIDE;

    // Nearest segment among the hovered tile's slots (regular segments only).
    var best_abs = 1e30;
    var best_signed = 1e30;
    var best_style = 0u;
    var found = false;
    var k = 0u;
    while k < hcount {
        let rs = tile_slots[hbase + k * 2u];
        if (rs & TILING_BIT) == 0u {
            // Slot value 2 is the ENTRY index (matching the main render path):
            // segments are LOCAL, so evaluate at world_p minus the entry's
            // translate, and resolve the style through entry.style_idx. (The old
            // code evaluated raw world_p and read the entry index as a style
            // index - broken since the keystone translate + batched styles.)
            let e_idx = tile_slots[hbase + k * 2u + 1u];
            let entry = draw_entries[e_idx];
            let sdf = eval_single_segment(world_p - entry.translate, rs);
            let ad = abs(sdf.dist);
            if ad < best_abs {
                best_abs = ad;
                best_signed = sdf.dist;
                best_style = entry.style_idx;
                found = true;
            }
        }
        k++;
    }

    var col: vec4<f32>;
    if found {
        col = render_distance_field(best_signed, styles[best_style], draw);
    } else {
        col = vec4(0.02, 0.02, 0.04, 1.0);
    }

    // Outline the hovered tile cell so its position is unambiguous.
    if u32(local_px.x / TILE_SIZE) == mcol && u32(local_px.y / TILE_SIZE) == mrow {
        let lx = local_px.x - f32(mcol) * TILE_SIZE;
        let ly = local_px.y - f32(mrow) * TILE_SIZE;
        let edge = min(min(lx, ly), min(TILE_SIZE - lx, TILE_SIZE - ly));
        if edge < 1.0 {
            col = mix(col, vec4(1.0, 1.0, 0.0, 1.0), 0.9);
        }
    }

    // Occupancy readout: a heat bar along the bottom edge. Width tracks
    // count / MAX_SLOTS; full red means the tile is saturated (overflow risk).
    let grid_w = f32(draw.grid_cols) * TILE_SIZE;
    let grid_h = f32(draw.grid_rows) * TILE_SIZE;
    let frac = f32(hcount) / f32(MAX_SLOTS_PER_TILE);
    if local_px.y > grid_h - 6.0 && local_px.x < grid_w * frac {
        col = vec4(frac, 1.0 - frac, 0.0, 1.0);
    }

    return col;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let draw = draws[in.draw_idx];
    let pixel = in.position.xy;
    let local_px = pixel - draw.bounds_origin;
    if local_px.x < 0.0 || local_px.y < 0.0 { discard; }

    let cs = draw.camera_zoom * draw.scale_factor;
    let world_p = local_px / cs - draw.camera_position;

    if (draw.debug_flags & DEBUG_HOVERED_TILE) != 0u && draw.grid_cols > 0u {
        let hovered = render_hovered_tile(draw, local_px, world_p);
        if hovered.a < 0.001 { discard; }
        return hovered;
    }

    var acc = vec4(0.0);

    if draw.grid_cols > 0u {
        let tile_col = u32(local_px.x / TILE_SIZE);
        let tile_row = u32(local_px.y / TILE_SIZE);
        if tile_col >= draw.grid_cols || tile_row >= draw.grid_rows { discard; }

        let tile_idx = draw.tile_base + tile_row * draw.grid_cols + tile_col;
        let count = tile_counts[tile_idx];

        if count == 0u {
            if (draw.debug_flags & DEBUG_TILE_HEATMAP) != 0u {
                return vec4(0.0, 0.05, 0.0, 0.1);
            }
            discard;
        }

        // Each slot = 2 u32s: (segment_idx_or_tiling, entry_idx). Group by
        // entry: for one shape, find the nearest segment per pixel. The entry
        // (command) carries the per-instance translate and the style index, so
        // identical shapes can share one segment range and differ only here.
        let slot_base = tile_idx * SLOT_STRIDE;
        var i = 0u;
        while i < count {
            if acc.a >= 0.999 { break; }
            let raw_seg = tile_slots[slot_base + i * 2u];
            let first_entry = tile_slots[slot_base + i * 2u + 1u];
            let entry = draw_entries[first_entry];
            let style = styles[entry.style_idx];

            // Check for tiling marker
            if (raw_seg & TILING_BIT) != 0u {
                let sdf = sd_tiling(world_p, entry.tiling_type, entry.tiling_params);
                let frag = render_style(sdf, style, draw, 0.0);
                acc = acc + frag * (1.0 - acc.a);
                i++;
                continue;
            }

            // Segments are stored local; shift the eval point by the instance
            // translate. Find the nearest among this entry's consecutive slots.
            let lp = world_p - entry.translate;
            var best_sdf = eval_single_segment(lp, raw_seg);
            var best_abs = abs(best_sdf.dist);
            var best_seg = raw_seg;
            i++;

            while i < count && tile_slots[slot_base + i * 2u + 1u] == first_entry {
                let next_seg = tile_slots[slot_base + i * 2u];
                if (next_seg & TILING_BIT) != 0u { break; }
                let sdf = eval_single_segment(lp, next_seg);
                let ad = abs(sdf.dist);
                if ad < best_abs {
                    best_abs = ad;
                    best_sdf = sdf;
                    best_seg = next_seg;
                }
                i++;
            }

            let frag = render_style(best_sdf, style, draw, segment_total_arc(best_seg));
            acc = acc + frag * (1.0 - acc.a);
        }
    } else {
        // Fallback: iterate all entries (no spatial index)
        let start = draw.entry_start;
        let end = start + draw.entry_count;
        for (var i = start; i < end; i++) {
            if acc.a >= 0.999 { break; }
            let entry = draw_entries[i];
            let style = styles[entry.style_idx];
            if entry.entry_type == ENTRY_TILING {
                let sdf = sd_tiling(world_p, entry.tiling_type, entry.tiling_params);
                let frag = render_style(sdf, style, draw, 0.0);
                acc = acc + frag * (1.0 - acc.a);
            } else {
                let lp = world_p - entry.translate;
                for (var s = 0u; s < entry.segment_count; s++) {
                    let seg_idx = entry.segment_start + s;
                    let sdf = eval_single_segment(lp, seg_idx);
                    let frag = render_style(sdf, style, draw, segment_total_arc(seg_idx));
                    acc = acc + frag * (1.0 - acc.a);
                }
            }
        }
    }

    // Debug: tile borders with slot count heat map
    if (draw.debug_flags & DEBUG_TILE_HEATMAP) != 0u && draw.grid_cols > 0u {
        let tile_col = u32(local_px.x / TILE_SIZE);
        let tile_row = u32(local_px.y / TILE_SIZE);
        let tile_idx = draw.tile_base + tile_row * draw.grid_cols + tile_col;
        let count = tile_counts[tile_idx];

        let lx = local_px.x - f32(tile_col) * TILE_SIZE;
        let ly = local_px.y - f32(tile_row) * TILE_SIZE;
        let edge = min(min(lx, ly), min(TILE_SIZE - lx, TILE_SIZE - ly));
        if edge < 1.0 && count > 0u {
            // Log scale so 1 slot is clearly visible: log2(1+count)/log2(1+max)
            let t = log2(1.0 + f32(count)) / log2(1.0 + f32(MAX_SLOTS_PER_TILE));
            let ba = (1.0 - edge) * 0.7;
            let bc = vec4(t, 1.0 - t, 0.0, ba);
            acc = acc * (1.0 - bc.a) + bc;
        }
    }

    if acc.a < 0.001 { discard; }
    return acc;
}

// ============================================================================
// Compute Shader - Per-Segment Spatial Index Builder
// ============================================================================

// Evaluate a single segment using compute bindings.
fn cs_eval_segment(p: vec2<f32>, seg_idx: u32) -> SdfResult {
    let seg = cs_segments[seg_idx];
    var r = eval_segment(p, seg);
    let arc_start = seg.arc_range.x;
    let arc_end = seg.arc_range.y;
    r.u = arc_start + r.u * (arc_end - arc_start);
    return r;
}

fn cs_push_slot(
    base: u32,
    count: ptr<function, u32>,
    slot_dist: ptr<function, array<f32, 32>>,
    seg_idx: u32,
    style_idx: u32,
    prio: f32,
) {
    if *count < MAX_SLOTS_PER_TILE {
        cs_tile_slots[base + *count * 2u] = seg_idx;
        cs_tile_slots[base + *count * 2u + 1u] = style_idx;
        (*slot_dist)[*count] = prio;
        *count += 1u;
    } else {
        // Tile full: keep the NEAREST MAX_SLOTS_PER_TILE entries - replace the
        // farthest slot if this one is nearer. Without this a crowded tile kept
        // an arbitrary first 32 by scan order, which could drop near segments
        // (the ones that dominate the tile's pixels) in favour of far ones.
        var maxi = 0u;
        var maxd = (*slot_dist)[0];
        for (var k = 1u; k < MAX_SLOTS_PER_TILE; k = k + 1u) {
            if (*slot_dist)[k] > maxd {
                maxd = (*slot_dist)[k];
                maxi = k;
            }
        }
        if prio < maxd {
            cs_tile_slots[base + maxi * 2u] = seg_idx;
            cs_tile_slots[base + maxi * 2u + 1u] = style_idx;
            (*slot_dist)[maxi] = prio;
        }
    }
}

// Two-level (regional) cull: candidates of one 16x16-tile workgroup, gathered
// in workgroup memory so each fine tile scans the region's candidates instead of
// every entry. 256 = the workgroup's thread count and a generous regional cap.
const MAX_WG_CANDIDATES: u32 = 256u;
var<workgroup> wg_candidates: array<u32, 256>;
var<workgroup> wg_count: atomic<u32>;

@compute @workgroup_size(16, 16, 1)
fn cs_build_index(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    let draw = cs_draws[cs_uniforms.draw_index];
    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let thd = TILE_SIZE * 0.70710678 * inv_cs; // tile half diagonal in world

    // --- Regional candidate binning (two-level cull). EVERY thread participates
    // (no early return before the barrier - WGSL requires uniform control flow
    // there): cooperatively gather entries whose world AABB reaches this
    // workgroup's 16x16-tile (256px) region, so the per-fine-tile loop below
    // scans only the region candidates, not all entries.
    let wg_col0 = wid.x * 16u;
    let wg_row0 = wid.y * 16u;
    let wg_min = vec2(f32(wg_col0) * TILE_SIZE, f32(wg_row0) * TILE_SIZE) * inv_cs
        - draw.camera_position;
    let wg_max = vec2(f32(wg_col0 + 16u) * TILE_SIZE, f32(wg_row0 + 16u) * TILE_SIZE) * inv_cs
        - draw.camera_position;

    if lid == 0u { atomicStore(&wg_count, 0u); }
    workgroupBarrier();

    let entry_end = draw.entry_start + draw.entry_count;
    for (var bi: u32 = draw.entry_start + lid; bi < entry_end; bi = bi + 256u) {
        let e = cs_entries[bi];
        let st = cs_styles[e.style_idx];
        let er = style_max_dist(st) + pattern_perp_reach(st) + thd + 1.0;
        let eb = e.bounds;
        if eb.z + er >= wg_min.x && eb.x - er <= wg_max.x
            && eb.w + er >= wg_min.y && eb.y - er <= wg_max.y {
            let slot = atomicAdd(&wg_count, 1u);
            if slot < MAX_WG_CANDIDATES { wg_candidates[slot] = bi; }
        }
    }
    workgroupBarrier();
    // If the region produced MORE candidates than the bin holds, the binning is
    // incomplete - fall back to scanning every entry for this tile so no entry is
    // ever dropped (correctness over the fast path). This triggers when many
    // entries crowd one 256px region, e.g. zoomed far out; without it the excess
    // candidates (often edges) silently vanished. The fast candidate path stays
    // for the common, non-crowded case.
    let total_cand = atomicLoad(&wg_count);
    let overflow = total_cand > MAX_WG_CANDIDATES;
    let cand_count = min(total_cand, MAX_WG_CANDIDATES);

    // --- Per-fine-tile cull over the region candidates ---
    let col = gid.x;
    let row = gid.y;
    if col >= draw.grid_cols || row >= draw.grid_rows { return; }

    let local_tile_idx = row * draw.grid_cols + col;
    let global_tile_idx = draw.tile_base + local_tile_idx;
    let local_center = vec2(
        (f32(col) + 0.5) * TILE_SIZE,
        (f32(row) + 0.5) * TILE_SIZE,
    );
    let world_pos = local_center * inv_cs - draw.camera_position;

    var count: u32 = 0u;
    // Parallel to the tile slots: each pushed entry's priority (|dist| to the
    // tile centre) so an overflowing tile keeps the NEAREST entries (see
    // cs_push_slot). Only read when a tile exceeds MAX_SLOTS_PER_TILE.
    var slot_dist: array<f32, 32>;
    let slot_base = global_tile_idx * SLOT_STRIDE;

    let scan_count = select(cand_count, draw.entry_count, overflow);
    for (var ci: u32 = 0u; ci < scan_count; ci = ci + 1u) {
        var i: u32;
        if overflow { i = draw.entry_start + ci; } else { i = wg_candidates[ci]; }
        let entry = cs_entries[i];
        let style = cs_styles[entry.style_idx];
        // Segments are local; evaluate at the tile center shifted by the
        // instance translate. The slot key is the entry (command) index.
        let lp = world_pos - entry.translate;

        // Tilings: cull by EVALUATION like any SDF (D7), not "always include".
        // Sample `sd_tiling` at the tile center plus its four corners (corners
        // restore conservativeness for HEX, whose round-to-nearest field is not
        // 1-Lipschitz across cell seams) and include only tiles a feature
        // reaches. Opaque / fine-spacing tilings still reach every tile; a
        // transparent-gap tiling auto-prunes its empty tiles.
        if entry.entry_type == ENTRY_TILING {
            let ht = TILE_SIZE * 0.5 * inv_cs;
            var td = sd_tiling(world_pos, entry.tiling_type, entry.tiling_params).dist;
            td = min(td, sd_tiling(world_pos + vec2(ht, ht), entry.tiling_type, entry.tiling_params).dist);
            td = min(td, sd_tiling(world_pos + vec2(-ht, ht), entry.tiling_type, entry.tiling_params).dist);
            td = min(td, sd_tiling(world_pos + vec2(ht, -ht), entry.tiling_type, entry.tiling_params).dist);
            td = min(td, sd_tiling(world_pos + vec2(-ht, -ht), entry.tiling_type, entry.tiling_params).dist);
            if td - thd <= style_max_dist(style) + 0.5 {
                // Tilings usually cover the whole tile; prioritise them (td is
                // small/negative inside the feature) so they survive overflow.
                cs_push_slot(slot_base, &count, &slot_dist, i | TILING_BIT, i, td);
            }
            continue;
        }

        // Spatial pre-cull (shapes): skip entries whose world AABB - expanded by
        // the tile half-diagonal and the style/pattern outward reach - does not
        // contain this tile center. A cheap bounds test that avoids per-segment
        // evaluation for the many far entries, so the cull is NOT O(tiles x all
        // entries). Over-inclusion only (the margin is conservative).
        let er = thd + style_max_dist(style) + pattern_perp_reach(style) + 0.5;
        let eb = entry.bounds;
        if world_pos.x < eb.x - er || world_pos.x > eb.z + er
            || world_pos.y < eb.y - er || world_pos.y > eb.w + er {
            continue;
        }

        let has_pattern = (style.flags & STYLE_FLAG_HAS_PATTERN) != 0u;

        // Pass 1: find nearest segment (by absolute distance) at tile center
        var min_abs_dist = 1e10;
        var best_signed_dist = 0.0;
        for (var s: u32 = 0u; s < entry.segment_count; s++) {
            let seg = cs_segments[entry.segment_start + s];
            let r = eval_segment(lp, seg);
            let ad = abs(r.dist);
            if ad < min_abs_dist {
                min_abs_dist = ad;
                best_signed_dist = r.dist;
            }
        }

        let proximity = min_abs_dist + thd * 2.0;

        // Determine if this (entry, style) is visible at all in this tile
        var entry_visible = false;
        if (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u {
            entry_visible = true;
        } else if has_pattern {
            entry_visible = true; // conservative, per-segment check below
        } else if (entry.flags & FLAG_CLOSED) != 0u {
            // Closed fill: signed dist from nearest segment
            entry_visible = (best_signed_dist - thd) < style_max_dist(style) + 0.5;
        } else {
            // Open curve: unsigned distance
            entry_visible = (min_abs_dist - thd) < style_max_dist(style) + 0.5;
        }

        if !entry_visible { continue; }

        // Pass 2: push nearby segments for per-pixel accuracy
        for (var s: u32 = 0u; s < entry.segment_count; s++) {
            let seg_idx = entry.segment_start + s;
            let seg = cs_segments[seg_idx];
            let r = eval_segment(lp, seg);

            if has_pattern {
                // C1: cull against the pattern's PERPENDICULAR envelope (as if
                // SOLID), ignoring the along-u dash/dot/flow structure - so the
                // cull is TIME-INDEPENDENT. Conservative over-inclusion: a gap
                // tile may get the command and render transparent. A sheared
                // dash/arrow leans by up to thd*|tan| at the open-contour end.
                var reach = pattern_perp_reach(style) + style_max_dist(style);
                if style.pattern_type == PATTERN_DASHED
                    || style.pattern_type == PATTERN_ARROWED
                {
                    reach = reach + thd * abs(tan(style.pattern_param2));
                }
                if abs(r.dist) - thd <= reach {
                    cs_push_slot(slot_base, &count, &slot_dist, seg_idx, i, abs(r.dist));
                }
            } else {
                // Fill/DF: push segments that could be nearest at any pixel
                if abs(r.dist) <= proximity {
                    cs_push_slot(slot_base, &count, &slot_dist, seg_idx, i, abs(r.dist));
                }
            }
        }
    }

    // Sort by style_idx (styles are pushed in z_order, so sorting by
    // style_idx preserves front-to-back order)
    for (var si: u32 = 1u; si < count; si++) {
        let s_seg = cs_tile_slots[slot_base + si * 2u];
        let s_sty = cs_tile_slots[slot_base + si * 2u + 1u];
        var sj = si;
        while sj > 0u {
            let p_sty = cs_tile_slots[slot_base + (sj - 1u) * 2u + 1u];
            if p_sty <= s_sty { break; }
            cs_tile_slots[slot_base + sj * 2u] = cs_tile_slots[slot_base + (sj - 1u) * 2u];
            cs_tile_slots[slot_base + sj * 2u + 1u] = p_sty;
            sj--;
        }
        cs_tile_slots[slot_base + sj * 2u] = s_seg;
        cs_tile_slots[slot_base + sj * 2u + 1u] = s_sty;
    }

    cs_tile_counts[global_tile_idx] = count;
}
