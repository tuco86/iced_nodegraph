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
const STYLE_FLAG_CLOSED: u32 = 4u;

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
    _pad1: u32,
    _pad2: u32,
}

struct GpuSegment {
    segment_type: u32,
    _pad0: u32,
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
}

struct GpuStyle {
    near_start: vec4<f32>,
    near_end: vec4<f32>,
    far_start: vec4<f32>,
    far_end: vec4<f32>,
    dist_from: f32,
    dist_to: f32,
    flags: u32,
    pattern_type: u32,
    pattern_thickness: f32,
    pattern_param0: f32,
    pattern_param1: f32,
    pattern_param2: f32,
    flow_speed: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

struct ComputeUniforms {
    draw_index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct SdfResult {
    dist: f32,
    u: f32,
    v: f32,
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
    let n = vec2<f32>(-ba.y, ba.x);
    var v_val = 0.0;
    if len_sq > 0.0 {
        v_val = dot(pa, n) / sqrt(len_sq);
    }
    return SdfResult(dist, t, v_val);
}

fn sd_bezier(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> SdfResult {
    var best_t = 0.0;
    var best_dist = 1e20;
    const SAMPLES: u32 = 16u;
    for (var i = 0u; i <= SAMPLES; i++) {
        let t = f32(i) / f32(SAMPLES);
        let bp = bezier_point(p0, p1, p2, p3, t);
        let d = length(p - bp);
        if d < best_dist { best_dist = d; best_t = t; }
    }
    for (var iter = 0u; iter < 4u; iter++) {
        let bp = bezier_point(p0, p1, p2, p3, best_t);
        let bd = bezier_deriv(p0, p1, p2, p3, best_t);
        let bdd = bezier_deriv2(p0, p1, p2, p3, best_t);
        let diff = bp - p;
        let num = dot(diff, bd);
        let den = dot(bd, bd) + dot(diff, bdd);
        if abs(den) > 1e-8 { best_t = clamp(best_t - num / den, 0.0, 1.0); }
    }
    let closest = bezier_point(p0, p1, p2, p3, best_t);
    let dist = length(p - closest);
    let tangent = bezier_deriv(p0, p1, p2, p3, best_t);
    let normal = vec2<f32>(-tangent.y, tangent.x);
    let diff = p - closest;
    var v_val = 0.0;
    let n_len = length(normal);
    if n_len > 1e-8 { v_val = dot(diff, normal) / n_len; }
    let arc_to_t = bezier_arc_length_to(p0, p1, p2, p3, best_t);
    let total_arc = bezier_total_arc_length(p0, p1, p2, p3);
    var u_frac = 0.0;
    if total_arc > 1e-6 { u_frac = arc_to_t / total_arc; }
    return SdfResult(dist, u_frac, v_val);
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

    // Normalize angle relative to arc start, wrapping to [-PI, PI]
    var rel = angle - start;
    rel = rel - round(rel / 6.2831853) * 6.2831853;

    let on_arc = (sweep > 0.0 && rel >= 0.0 && rel <= sweep)
              || (sweep < 0.0 && rel <= 0.0 && rel >= sweep);

    var dist: f32;
    var u_frac: f32;
    var v_val: f32;

    if on_arc {
        dist = abs(dist_to_center - radius);
        u_frac = rel / sweep;
        // On the arc: radial v (perpendicular to curve)
        v_val = select(-1.0, 1.0, sweep > 0.0) * (radius - dist_to_center);
    } else {
        // At endpoints: tangent-based v (sign boundary extends straight)
        let end_angle = start + sweep;
        let p_start = center + vec2(cos(start), sin(start)) * radius;
        let p_end = center + vec2(cos(end_angle), sin(end_angle)) * radius;
        let d_start = length(p - p_start);
        let d_end = length(p - p_end);

        if d_start < d_end {
            dist = d_start;
            u_frac = 0.0;
            // Tangent at start in travel direction
            let tangent = vec2(-sin(start), cos(start)) * sign(sweep);
            let n = vec2(-tangent.y, tangent.x);
            let nlen = length(n);
            if nlen > 0.0 { v_val = dot(p - p_start, n) / nlen; }
            else { v_val = 0.0; }
        } else {
            dist = d_end;
            u_frac = 1.0;
            let tangent = vec2(-sin(end_angle), cos(end_angle)) * sign(sweep);
            let n = vec2(-tangent.y, tangent.x);
            let nlen = length(n);
            if nlen > 0.0 { v_val = dot(p - p_end, n) / nlen; }
            else { v_val = 0.0; }
        }
    }

    return SdfResult(dist, u_frac, v_val);
}

// Junction point between segments. Owns the corner, defines sign via heading.
// geom0 = (px, py, heading, 0)
fn sd_point(p: vec2<f32>, pos: vec2<f32>, heading: f32) -> SdfResult {
    // Tiny distance advantage so Point always wins over adjacent segment endpoints
    // at the junction. Ensures correct v from bisector heading.
    let dist = max(0.0, length(p - pos) - 0.01);
    let right = vec2(cos(heading), sin(heading));
    let v_val = dot(p - pos, right);
    return SdfResult(dist, 0.0, v_val);
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
            return SdfResult(min(mx, my), 0.0, 0.0);
        }
        case TILING_DOTS: {
            let radius = params.z;
            // Distance to nearest dot center (modulo spacing)
            let cell = round(p / spacing) * spacing;
            let dist = length(p - cell) - radius;
            return SdfResult(dist, 0.0, 0.0);
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
            return SdfResult(min(min(m1, m2), m3), 0.0, 0.0);
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
            return SdfResult(abs(length(d) * sign(d.y)), 0.0, 0.0);
        }
        default: {
            return SdfResult(1e10, 0.0, 0.0);
        }
    }
}

fn eval_segment(p: vec2<f32>, seg: GpuSegment) -> SdfResult {
    switch seg.segment_type {
        case SEG_LINE: { return sd_line(p, seg.geom0.xy, seg.geom0.zw); }
        case SEG_ARC: { return sd_arc_segment(p, seg.geom0.xy, seg.geom0.z, seg.geom0.w, seg.geom1.x); }
        case SEG_CUBIC: { return sd_bezier(p, seg.geom0.xy, seg.geom0.zw, seg.geom1.xy, seg.geom1.zw); }
        case SEG_POINT: { return sd_point(p, seg.geom0.xy, seg.geom0.z); }
        default: { return SdfResult(1e10, 0.0, 0.0); }
    }
}

// Evaluate a single segment, map u through arc_range, apply sign.
fn eval_single_segment(p: vec2<f32>, seg_idx: u32, style: GpuStyle) -> SdfResult {
    let seg = segments[seg_idx];
    var r = eval_segment(p, seg);
    // Map parametric u (0..1) to world-space arc-length
    // Patterns need world units; render_style normalizes for gradients
    let arc_start = seg.arc_range.x;
    let arc_end = seg.arc_range.y;
    r.u = arc_start + r.u * (arc_end - arc_start);
    // Store total arc length in v_unused... no, we need v for sign.
    // Instead, store total in arc_range.z and pass through via a trick:
    // We'll normalize in render_style using dist_from/dist_to context.
    // For now: u = world-space arc-length, v = perpendicular
    r.dist = r.dist * select(1.0, -1.0, r.v > 0.0);
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
    if (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u {
        return render_distance_field(sdf.dist, style, draw);
    }

    // Normalize u from world-space to 0..1 for color gradient
    var arc_t = 0.0;
    if total_arc > 0.0 { arc_t = clamp(sdf.u / total_arc, 0.0, 1.0); }

    var dist = sdf.dist;

    if (style.flags & STYLE_FLAG_HAS_PATTERN) != 0u {
        // Pattern uses world-space u (sdf.u) for dash layout
        dist = apply_pattern(dist, sdf, style, draw.time);

        let color = mix(style.near_start, style.near_end, arc_t);
        let aa = fwidth(dist) * 0.75;
        let alpha = color.a * (1.0 - smoothstep(-aa, aa, dist));
        if alpha < 0.001 { return vec4(0.0); }
        return vec4(color.rgb * alpha, alpha);
    }

    let near = mix(style.near_start, style.near_end, arc_t);
    let far = mix(style.far_start, style.far_end, arc_t);

    let range = style.dist_to - style.dist_from;
    var color: vec4<f32>;
    if range > 0.001 {
        let dist_t = clamp((dist - style.dist_from) / range, 0.0, 1.0);
        color = mix(near, far, dist_t);
    } else {
        color = near;
    }

    // fwidth-based AA at distance boundaries
    let aa = fwidth(dist) * 0.75;
    let alpha_from = smoothstep(style.dist_from - aa, style.dist_from + aa, dist);
    let alpha_to = 1.0 - smoothstep(style.dist_to - aa, style.dist_to + aa, dist);
    let alpha = color.a * alpha_from * alpha_to;

    if alpha < 0.001 { return vec4(0.0); }
    return vec4(color.rgb * alpha, alpha);
}

fn render_distance_field(d: f32, style: GpuStyle, draw: DrawData) -> vec4<f32> {
    let outside_col = style.near_start.rgb;
    let inside_col = style.far_start.rgb;
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
            let shifted_u = u + sdf.v * tan(angle);
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let dd = abs(vec2(dist_along, dist)) - vec2(dash * 0.5, half_t);
            return length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
        }
        case PATTERN_ARROWED: {
            let segment = style.pattern_param0;
            let gap = style.pattern_param1;
            let angle = style.pattern_param2;
            let period = segment + gap;
            let shifted_u = u + abs(sdf.v) * tan(angle);
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let dd = abs(vec2(dist_along, dist)) - vec2(segment * 0.5, half_t);
            return length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0);
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
            let shifted_local = local_u + abs(sdf.v) * tan(angle);
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let draw = draws[in.draw_idx];
    let pixel = in.position.xy;
    let local_px = pixel - draw.bounds_origin;
    if local_px.x < 0.0 || local_px.y < 0.0 { discard; }

    let cs = draw.camera_zoom * draw.scale_factor;
    let world_p = local_px / cs - draw.camera_position;

    var acc = vec4(0.0);

    if draw.grid_cols > 0u {
        let tile_col = u32(local_px.x / TILE_SIZE);
        let tile_row = u32(local_px.y / TILE_SIZE);
        if tile_col >= draw.grid_cols || tile_row >= draw.grid_rows { discard; }

        let tile_idx = draw.tile_base + tile_row * draw.grid_cols + tile_col;
        let count = tile_counts[tile_idx];

        if count == 0u {
            if (draw.debug_flags & 1u) != 0u {
                return vec4(0.0, 0.05, 0.0, 0.1);
            }
            discard;
        }

        // Each slot = 2 u32s: (segment_idx_or_tiling, style_idx)
        // Group by style: for same style, find nearest segment per pixel.
        let slot_base = tile_idx * SLOT_STRIDE;
        var i = 0u;
        while i < count {
            if acc.a >= 0.999 { break; }
            let raw_seg = tile_slots[slot_base + i * 2u];
            let first_sty = tile_slots[slot_base + i * 2u + 1u];
            let style = styles[first_sty];

            // Check for tiling marker
            if (raw_seg & TILING_BIT) != 0u {
                let entry_idx = raw_seg & ~TILING_BIT;
                let entry = draw_entries[entry_idx];
                let sdf = sd_tiling(world_p, entry.tiling_type, entry.tiling_params);
                let frag = render_style(sdf, style, draw, 0.0);
                acc = acc + frag * (1.0 - acc.a);
                i++;
                continue;
            }

            // Regular segments: find nearest among consecutive same-style slots
            var best_sdf = eval_single_segment(world_p, raw_seg, style);
            var best_abs = abs(best_sdf.dist);
            var best_seg = raw_seg;
            i++;

            while i < count && tile_slots[slot_base + i * 2u + 1u] == first_sty {
                let next_seg = tile_slots[slot_base + i * 2u];
                if (next_seg & TILING_BIT) != 0u { break; }
                let sdf = eval_single_segment(world_p, next_seg, style);
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
                for (var s = 0u; s < entry.segment_count; s++) {
                    let seg_idx = entry.segment_start + s;
                    let sdf = eval_single_segment(world_p, seg_idx, style);
                    let frag = render_style(sdf, style, draw, segment_total_arc(seg_idx));
                    acc = acc + frag * (1.0 - acc.a);
                }
            }
        }
    }

    // Debug: tile borders with slot count heat map
    if (draw.debug_flags & 1u) != 0u && draw.grid_cols > 0u {
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
    // World-space arc-length (patterns need world units)
    let arc_start = seg.arc_range.x;
    let arc_end = seg.arc_range.y;
    r.u = arc_start + r.u * (arc_end - arc_start);
    r.dist = r.dist * select(1.0, -1.0, r.v > 0.0);
    return r;
}

fn cs_push_slot(base: u32, count: ptr<function, u32>, seg_idx: u32, style_idx: u32) {
    if *count < MAX_SLOTS_PER_TILE {
        cs_tile_slots[base + *count * 2u] = seg_idx;
        cs_tile_slots[base + *count * 2u + 1u] = style_idx;
        *count += 1u;
    }
}

@compute @workgroup_size(16, 16, 1)
fn cs_build_index(@builtin(global_invocation_id) gid: vec3<u32>) {
    let draw = cs_draws[cs_uniforms.draw_index];
    let col = gid.x;
    let row = gid.y;

    if col >= draw.grid_cols || row >= draw.grid_rows { return; }

    let local_tile_idx = row * draw.grid_cols + col;
    let global_tile_idx = draw.tile_base + local_tile_idx;

    let local_center = vec2(
        (f32(col) + 0.5) * TILE_SIZE,
        (f32(row) + 0.5) * TILE_SIZE,
    );
    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let world_pos = local_center * inv_cs - draw.camera_position;
    let thd = TILE_SIZE * 0.70710678 * inv_cs; // tile half diagonal in world

    var count: u32 = 0u;
    let slot_base = global_tile_idx * SLOT_STRIDE;

    let entry_end = draw.entry_start + draw.entry_count;
    for (var i: u32 = draw.entry_start; i < entry_end; i++) {
        let entry = cs_entries[i];
        let style = cs_styles[entry.style_idx];
        let sty_idx = entry.style_idx;

        // Tilings: always include, store entry_idx with TILING_BIT marker
        if entry.entry_type == ENTRY_TILING {
            cs_push_slot(slot_base, &count, i | TILING_BIT, sty_idx);
            continue;
        }

        let has_pattern = (style.flags & STYLE_FLAG_HAS_PATTERN) != 0u;

        // Pass 1: find nearest segment to determine inside/outside at tile center
        var min_unsigned = 1e10;
        var best_v = 0.0;
        for (var s: u32 = 0u; s < entry.segment_count; s++) {
            let seg = cs_segments[entry.segment_start + s];
            let r = eval_segment(world_pos, seg);
            if r.dist < min_unsigned {
                min_unsigned = r.dist;
                best_v = r.v;
            }
        }

        // Shape-level signed distance (correct: from nearest segment across ALL)
        let signed_dist = min_unsigned * select(1.0, -1.0, best_v > 0.0);
        // Proximity: segments that could be nearest at any pixel in tile
        let proximity = min_unsigned + thd * 2.0;

        // Determine if this (entry, style) is visible at all in this tile
        var entry_visible = false;
        if (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u {
            entry_visible = true;
        } else if has_pattern {
            // For patterns: visible if any nearby segment has visible pattern
            entry_visible = true; // conservative, per-segment check below
        } else if (style.flags & STYLE_FLAG_CLOSED) != 0u {
            // Closed fill: visible if any pixel in tile could be inside
            entry_visible = (signed_dist - thd) < style.dist_to + 0.5;
        } else {
            // Open curve fill: use unsigned distance (no "inside" concept)
            entry_visible = (min_unsigned - thd) < style.dist_to + 0.5;
        }

        if !entry_visible { continue; }

        // Pass 2: push nearby segments for per-pixel accuracy
        for (var s: u32 = 0u; s < entry.segment_count; s++) {
            let seg_idx = entry.segment_start + s;
            let seg = cs_segments[seg_idx];
            let r = eval_segment(world_pos, seg);

            if has_pattern {
                // Pattern: per-segment culling with pattern evaluation
                let sdf = cs_eval_segment(world_pos, seg_idx);
                let eff = apply_pattern(sdf.dist, sdf, style, draw.time);
                // Angled patterns shift by v*tan(angle), so margin grows with angle
                let angle_margin = thd * (1.0 + abs(tan(style.pattern_param2)));
                if eff <= angle_margin {
                    cs_push_slot(slot_base, &count, seg_idx, sty_idx);
                }
            } else {
                // Fill/DF: push segments that could be nearest at any pixel
                if r.dist <= proximity {
                    cs_push_slot(slot_base, &count, seg_idx, sty_idx);
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
