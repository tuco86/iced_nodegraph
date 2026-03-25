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

// entry.flags
const FLAG_CLOSED: u32 = 1u;

// style.flags
const STYLE_FLAG_GRADIENT: u32 = 1u;
const STYLE_FLAG_ARC_GRADIENT: u32 = 2u;
const STYLE_FLAG_HAS_PATTERN: u32 = 4u;
const STYLE_FLAG_DISTANCE_FIELD: u32 = 8u;
const STYLE_FLAG_CLOSED: u32 = 16u;

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
    color: vec4<f32>,
    gradient_color: vec4<f32>,
    gradient_angle: f32,
    flags: u32,
    expand: f32,
    blur: f32,
    pattern_type: u32,
    pattern_thickness: f32,
    pattern_param0: f32,
    pattern_param1: f32,
    pattern_param2: f32,
    flow_speed: f32,
    outline_thickness: f32,
    _pad0: f32,
    outline_color: vec4<f32>,
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
    // Map parametric u (0..1) to arc-length range
    let arc_start = seg.arc_range.x;
    let arc_end = seg.arc_range.y;
    r.u = arc_start + r.u * (arc_end - arc_start);
    // Sign from perpendicular: v > 0 = right side in screen Y-down = negative
    r.dist = r.dist * select(1.0, -1.0, r.v > 0.0);
    return r;
}

// ============================================================================
// Style Rendering
// ============================================================================

fn render_style(sdf: SdfResult, style: GpuStyle, draw: DrawData) -> vec4<f32> {
    if (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u {
        return render_distance_field(sdf.dist, style, draw);
    }

    var dist = sdf.dist;
    dist -= style.expand;

    if (style.flags & STYLE_FLAG_HAS_PATTERN) != 0u {
        dist = apply_pattern(dist, sdf, style, draw.time);
    }

    var alpha = 0.0;
    if style.blur > 0.0 {
        alpha = 1.0 - smoothstep(-style.blur, style.blur, dist);
    } else {
        alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
    }
    if alpha < 0.001 { return vec4(0.0); }

    var color = style.color;

    if (style.flags & STYLE_FLAG_GRADIENT) != 0u {
        var t = 0.0;
        if (style.flags & STYLE_FLAG_ARC_GRADIENT) != 0u {
            t = clamp(sdf.u, 0.0, 1.0);
        }
        color = mix(color, style.gradient_color, t);
    }

    if style.outline_thickness > 0.0 {
        let outline_dist = abs(sdf.dist) - style.outline_thickness * 0.5;
        let outline_alpha = 1.0 - smoothstep(-0.5, 0.5, outline_dist);
        if outline_alpha > 0.001 {
            let oc = style.outline_color;
            let oa = oc.a * outline_alpha;
            color = vec4(mix(color.rgb, oc.rgb, oa), max(color.a, oa));
        }
    }

    let final_alpha = color.a * alpha;
    return vec4(color.rgb * final_alpha, final_alpha);
}

fn render_distance_field(d: f32, style: GpuStyle, draw: DrawData) -> vec4<f32> {
    let outside_col = style.color.rgb;
    let inside_col = style.gradient_color.rgb;
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

        // Each slot = 2 u32s: (segment_idx, style_idx)
        // Group by style: for same style, find nearest segment per pixel.
        let slot_base = tile_idx * SLOT_STRIDE;
        var i = 0u;
        while i < count {
            if acc.a >= 0.999 { break; }
            let first_sty = tile_slots[slot_base + i * 2u + 1u];
            let style = styles[first_sty];

            // Find nearest segment among consecutive slots with same style
            var best_sdf = eval_single_segment(world_p, tile_slots[slot_base + i * 2u], style);
            var best_abs = abs(best_sdf.dist);
            i++;

            while i < count && tile_slots[slot_base + i * 2u + 1u] == first_sty {
                let sdf = eval_single_segment(world_p, tile_slots[slot_base + i * 2u], style);
                let ad = abs(sdf.dist);
                if ad < best_abs {
                    best_abs = ad;
                    best_sdf = sdf;
                }
                i++;
            }

            let frag = render_style(best_sdf, style, draw);
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
            // Evaluate all segments of this entry
            for (var s = 0u; s < entry.segment_count; s++) {
                let sdf = eval_single_segment(world_p, entry.segment_start + s, style);
                let frag = render_style(sdf, style, draw);
                acc = acc + frag * (1.0 - acc.a);
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
        } else {
            // Fill: visible if any pixel in tile could be inside the fill region
            entry_visible = (signed_dist - thd - style.expand) < style.blur + 0.5;
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
                if eff <= thd + style.blur {
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
