// Segment-based SDF renderer with per-segment tile spatial index.
// Tile slots store (segment_idx, style_idx) pairs as 2x u32.
// Compute evaluates individual segments, fragment just iterates tile slots.

// --- Constants ---

const TILE_SIZE: f32 = 16.0;
// Two-level spatial index. COARSE tiles are 4x4 fine tiles (64x64 px). The coarse
// level materializes the (segment, entry) cull result once per 64px tile (few
// tiles, fat slots); each 16px FINE tile then stores only compact 8-bit indices
// into its parent coarse tile's result, paying one indirection for a much smaller
// fine buffer (16x more fine tiles).
const COARSE_FACTOR: u32 = 4u;        // fine tiles per coarse tile, per axis
// Coarse: up to 256 (segment_idx, entry_idx) results per 64px tile (8bit-addressable).
const MAX_COARSE_SLOTS: u32 = 256u;
const COARSE_STRIDE: u32 = 512u;      // MAX_COARSE_SLOTS * 2 u32 per coarse tile
// Fine: up to 128 8-bit indices into the parent coarse list, packed 4 per u32.
const MAX_FINE_SLOTS: u32 = 128u;
const FINE_STRIDE: u32 = 32u;         // MAX_FINE_SLOTS / 4 (u8 packed 4 per u32)

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

// segment.flags
const SEG_FLAG_SIGNED: u32 = 1u;

// Arc-only segment thresholds (mirror crate::segment LINE_EPS / POINT_EPS).
const LINE_EPS: f32 = 1e-6;
const POINT_EPS: f32 = 1e-5;
const SEG_PI: f32 = 3.14159265;
const SEG_TAU: f32 = 6.2831853;

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
    coarse_cols: u32,
    coarse_rows: u32,
    coarse_base: u32,
    mouse_px: vec2<f32>,
    _pad0: u32,
    _pad1: u32,
}

struct GpuSegment {
    flags: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // Endpoints: (start.x, start.y, end.x, end.y).
    endpoints: vec4<f32>,
    // Arc encoding: (curvature, heading, 0, 0). curvature=0 -> line;
    // start==end -> point (sign from heading); else minor arc r = 1/|curvature|.
    params: vec4<f32>,
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

struct SdfResult {
    dist: f32,  // signed distance (positive = right side of curve)
    u: f32,     // parametric position along curve [0..1]
}

// --- Render bindings (group 0) ---

@group(0) @binding(0) var<storage, read> draws: array<DrawData>;
@group(0) @binding(1) var<storage, read> draw_entries: array<GpuDrawEntry>;
@group(0) @binding(2) var<storage, read> segments: array<GpuSegment>;
@group(0) @binding(3) var<storage, read> styles: array<GpuStyle>;
// Fine level: per 16px tile, a count and a run of 8-bit indices (packed 4/u32)
// into the parent coarse tile's result list.
@group(0) @binding(4) var<storage, read> fine_counts: array<u32>;
@group(0) @binding(5) var<storage, read> fine_slots: array<u32>;
// Coarse level: per 64px tile, COARSE_STRIDE u32s = MAX_COARSE_SLOTS pairs of
// (segment_idx_or_tiling, entry_idx). The fine 8-bit index addresses these.
@group(0) @binding(6) var<storage, read> coarse_slots: array<u32>;

// --- Compute bindings ---

@group(0) @binding(0) var<storage, read> cs_draws: array<DrawData>;
@group(0) @binding(1) var<storage, read> cs_entries: array<GpuDrawEntry>;
@group(0) @binding(2) var<storage, read> cs_segments: array<GpuSegment>;
@group(0) @binding(3) var<storage, read> cs_styles: array<GpuStyle>;

// draw_index is carried by the dispatch z-axis (workgroup_id.z), not a uniform.
@group(1) @binding(0) var<storage, read_write> cs_coarse_counts: array<u32>;
@group(1) @binding(1) var<storage, read_write> cs_coarse_slots: array<u32>;
@group(1) @binding(2) var<storage, read_write> cs_fine_counts: array<u32>;
@group(1) @binding(3) var<storage, read_write> cs_fine_slots: array<u32>;

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

// v3 is arc-only: the per-pixel cubic-bezier SDF (Newton refinement +
// Gauss-Legendre arc-length quadrature) is GONE. Cubics are approximated by an
// arc-spline on the CPU (crate::biarc) before reaching the GPU, so a line or
// minor arc is the only thing the shader ever evaluates.

// Reconstruct (center, radius, start_angle, sweep) of the minor arc from its
// endpoints and signed curvature - the GPU twin of crate::segment::arc_center +
// arc_minor_sweep. Caller guarantees a true arc (|curvature| >= LINE_EPS and
// start != end).
struct ArcParams {
    center: vec2<f32>,
    radius: f32,
    start_angle: f32,
    sweep: f32,
}

fn arc_from_endpoints(start: vec2<f32>, end: vec2<f32>, curvature: f32) -> ArcParams {
    let d = end - start;
    let l = length(d);
    let r = 1.0 / abs(curvature);
    let u = d / l;
    let n = vec2<f32>(-u.y, u.x);
    let h = sqrt(max(r * r - (l * 0.5) * (l * 0.5), 0.0));
    let center = (start + end) * 0.5 + n * (sign(curvature) * h);
    let a_start = atan2(start.y - center.y, start.x - center.x);
    let a_end = atan2(end.y - center.y, end.x - center.x);
    var sweep = a_end - a_start;
    if sweep <= -SEG_PI { sweep = sweep + SEG_TAU; }
    else if sweep > SEG_PI { sweep = sweep - SEG_TAU; }
    return ArcParams(center, r, a_start, sweep);
}

// Exact signed distance to a circular arc segment.
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
    // One arc primitive: the geometry (not a type tag) selects the branch.
    let start = seg.endpoints.xy;
    let end = seg.endpoints.zw;
    let curvature = seg.params.x;
    let heading = seg.params.y;
    if length(end - start) < POINT_EPS { return sd_point(p, start, heading); }
    if abs(curvature) < LINE_EPS { return sd_line(p, start, end); }
    let ap = arc_from_endpoints(start, end, curvature);
    return sd_arc_segment(p, ap.center, ap.radius, ap.start_angle, ap.sweep);
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
fn render_style(sdf: SdfResult, style: GpuStyle, draw: DrawData, total_arc: f32, is_closed: bool) -> vec4<f32> {
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
        dist = apply_pattern(dist, sdf, style, draw.time, is_closed);

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

fn apply_pattern(dist: f32, sdf: SdfResult, style: GpuStyle, time: f32, is_closed: bool) -> f32 {
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
            let feature = length(vec2(dist_to_center, dist)) - radius;
            if is_closed {
                // Sign-aware composition (A3): on a CLOSED contour the interior
                // is negative, so a full dot bulging inward breaks the inner
                // edge. Union the dots' OUTER half (clip to dist>=0) with a plain
                // inner closed line: min(plain_band, max(feature, -dist)). Both
                // min/max of 1-Lipschitz fields stay 1-Lipschitz, so AA is free.
                // TUNING (called out per plan, not baked silently): the feature
                // sits on the centerline and the inner line is a thin symmetric
                // band (<=2px), NOT the full dot radius - which would swallow the
                // bumps. Open contours keep the mirrored symmetric dot below.
                let inner_half = min(half_t, 2.0);
                let plain_band = abs(dist) - inner_half;
                return min(plain_band, max(feature, -dist));
            }
            return feature;
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

// Coarse-tile base (offset into coarse_slots) for the fine tile at
// (tile_col, tile_row). The fine tile's 8-bit slots index into this run.
fn coarse_base_for_fine(draw: DrawData, tile_col: u32, tile_row: u32) -> u32 {
    let cc = tile_col / COARSE_FACTOR;
    let cr = tile_row / COARSE_FACTOR;
    return (draw.coarse_base + cr * draw.coarse_cols + cc) * COARSE_STRIDE;
}

// Unpack the k-th 8-bit slot (coarse-list index) of a fine tile.
fn fine_coarse_index(fine_base: u32, k: u32) -> u32 {
    let word = fine_slots[fine_base + (k >> 2u)];
    return (word >> ((k & 3u) * 8u)) & 0xFFu;
}

// Hovered-tile inspector: render the IQ distance field built from ONLY the
// segments held by the tile under the cursor, plus an occupancy readout. Makes
// a single fine tile's slot buffer (and any overflow) directly visible.
fn render_hovered_tile(draw: DrawData, local_px: vec2<f32>, world_p: vec2<f32>) -> vec4<f32> {
    let mcol = u32(draw.mouse_px.x / TILE_SIZE);
    let mrow = u32(draw.mouse_px.y / TILE_SIZE);
    if draw.mouse_px.x < 0.0 || draw.mouse_px.y < 0.0
        || mcol >= draw.grid_cols || mrow >= draw.grid_rows {
        // Cursor outside this layer's grid: contribute nothing (discarded).
        return vec4(0.0);
    }

    let htile = draw.tile_base + mrow * draw.grid_cols + mcol;
    let hcount = fine_counts[htile];
    let hbase = htile * FINE_STRIDE;
    let cbase = coarse_base_for_fine(draw, mcol, mrow);

    // Nearest segment among the hovered tile's slots (regular segments only).
    var best_abs = 1e30;
    var best_signed = 1e30;
    var best_style = 0u;
    var found = false;
    var k = 0u;
    while k < hcount {
        let ci = fine_coarse_index(hbase, k);
        let rs = coarse_slots[cbase + ci * 2u];
        if (rs & TILING_BIT) == 0u {
            // The fine slot dereferences to the coarse (segment, entry) result.
            // Segments are LOCAL, so evaluate at world_p minus the entry's
            // translate, and resolve the style through entry.style_idx.
            let e_idx = coarse_slots[cbase + ci * 2u + 1u];
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
    let frac = f32(hcount) / f32(MAX_FINE_SLOTS);
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
        let count = fine_counts[tile_idx];

        if count == 0u {
            if (draw.debug_flags & DEBUG_TILE_HEATMAP) != 0u {
                return vec4(0.0, 0.05, 0.0, 0.1);
            }
            discard;
        }

        // Each fine slot is an 8-bit index into this tile's parent coarse list,
        // whose entries are (segment_idx_or_tiling, entry_idx). Dereference once,
        // then group by entry: for one shape, find the nearest segment per pixel.
        // The entry (command) carries the per-instance translate and the style
        // index, so identical shapes share one segment range and differ only here.
        let fine_base = tile_idx * FINE_STRIDE;
        let cbase = coarse_base_for_fine(draw, tile_col, tile_row);
        var i = 0u;
        while i < count {
            if acc.a >= 0.999 { break; }
            let ci0 = fine_coarse_index(fine_base, i);
            let raw_seg = coarse_slots[cbase + ci0 * 2u];
            let first_entry = coarse_slots[cbase + ci0 * 2u + 1u];
            let entry = draw_entries[first_entry];
            let style = styles[entry.style_idx];

            // Check for tiling marker
            if (raw_seg & TILING_BIT) != 0u {
                let sdf = sd_tiling(world_p, entry.tiling_type, entry.tiling_params);
                let frag = render_style(sdf, style, draw, 0.0, false);
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

            loop {
                if i >= count { break; }
                let ci = fine_coarse_index(fine_base, i);
                if coarse_slots[cbase + ci * 2u + 1u] != first_entry { break; }
                let next_seg = coarse_slots[cbase + ci * 2u];
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

            let frag = render_style(best_sdf, style, draw, segment_total_arc(best_seg), (entry.flags & FLAG_CLOSED) != 0u);
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
                let frag = render_style(sdf, style, draw, 0.0, false);
                acc = acc + frag * (1.0 - acc.a);
            } else {
                let lp = world_p - entry.translate;
                // Fold to the NEAREST segment (as the tiled path does) so a
                // multi-segment entry renders as ONE contour, not N overlapping
                // per-segment strokes. Without this the untiled fallback double-AAs
                // every join, which a multi-segment edge (e.g. an arc-spline
                // bezier) makes visible as wobble.
                var best_sdf = eval_single_segment(lp, entry.segment_start);
                var best_abs = abs(best_sdf.dist);
                var best_seg = entry.segment_start;
                for (var s = 1u; s < entry.segment_count; s++) {
                    let seg_idx = entry.segment_start + s;
                    let sdf = eval_single_segment(lp, seg_idx);
                    if abs(sdf.dist) < best_abs {
                        best_abs = abs(sdf.dist);
                        best_sdf = sdf;
                        best_seg = seg_idx;
                    }
                }
                let frag = render_style(best_sdf, style, draw, segment_total_arc(best_seg), (entry.flags & FLAG_CLOSED) != 0u);
                acc = acc + frag * (1.0 - acc.a);
            }
        }
    }

    // Debug: tile borders with slot count heat map
    if (draw.debug_flags & DEBUG_TILE_HEATMAP) != 0u && draw.grid_cols > 0u {
        let tile_col = u32(local_px.x / TILE_SIZE);
        let tile_row = u32(local_px.y / TILE_SIZE);
        let tile_idx = draw.tile_base + tile_row * draw.grid_cols + tile_col;
        let count = fine_counts[tile_idx];

        let lx = local_px.x - f32(tile_col) * TILE_SIZE;
        let ly = local_px.y - f32(tile_row) * TILE_SIZE;
        let edge = min(min(lx, ly), min(TILE_SIZE - lx, TILE_SIZE - ly));
        if edge < 1.0 && count > 0u {
            // Log scale so 1 slot is clearly visible: log2(1+count)/log2(1+max)
            let t = log2(1.0 + f32(count)) / log2(1.0 + f32(MAX_FINE_SLOTS));
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

// --- Workgroup state for the two-level build (one workgroup per coarse tile) ---
// Entry candidates whose AABB reaches the 64px coarse tile (cooperative bin).
const MAX_WG_CANDIDATES: u32 = 256u;
var<workgroup> wg_candidates: array<u32, 256>;
var<workgroup> wg_cand_count: atomic<u32>;
// The coarse result list: (segment_idx_or_tiling, entry_idx) pairs, built by
// thread 0, read by all threads in the fine phase. Mirrored to cs_coarse_slots.
var<workgroup> wg_cseg: array<u32, 256>;
var<workgroup> wg_centry: array<u32, 256>;
var<workgroup> wg_ccount: u32;

// Append (or, when full, keep-nearest replace) into the coarse list. Single-
// threaded (thread 0 only), so no atomics. `prio` = closest box distance.
fn coarse_push(
    count: ptr<function, u32>,
    slot_dist: ptr<function, array<f32, MAX_COARSE_SLOTS>>,
    seg_idx: u32,
    entry_idx: u32,
    prio: f32,
) {
    if *count < MAX_COARSE_SLOTS {
        wg_cseg[*count] = seg_idx;
        wg_centry[*count] = entry_idx;
        (*slot_dist)[*count] = prio;
        *count += 1u;
    } else {
        // Coarse tile full: keep the NEAREST MAX_COARSE_SLOTS - replace the
        // farthest if this one is nearer (so a crowded tile keeps the segments
        // that dominate its pixels, not an arbitrary first 256 by scan order).
        var maxi = 0u;
        var maxd = (*slot_dist)[0];
        for (var k = 1u; k < MAX_COARSE_SLOTS; k = k + 1u) {
            if (*slot_dist)[k] > maxd { maxd = (*slot_dist)[k]; maxi = k; }
        }
        if prio < maxd {
            wg_cseg[maxi] = seg_idx;
            wg_centry[maxi] = entry_idx;
            (*slot_dist)[maxi] = prio;
        }
    }
}

// Write an 8-bit coarse-list index into fine slot `slot` (4 indices per u32).
// Mask-write preserves the word's already-written bytes; bytes past the fine
// count are never read, so stale data there is harmless.
fn fine_set(fine_base: u32, slot: u32, coarse_idx: u32) {
    let word = fine_base + (slot >> 2u);
    let shift = (slot & 3u) * 8u;
    cs_fine_slots[word] = (cs_fine_slots[word] & ~(0xFFu << shift))
        | ((coarse_idx & 0xFFu) << shift);
}

// Append (or keep-nearest replace) a coarse-list index into a fine tile. Single-
// threaded per fine tile (each thread owns one), so no atomics.
fn fine_push(
    fine_base: u32,
    count: ptr<function, u32>,
    slot_dist: ptr<function, array<f32, MAX_FINE_SLOTS>>,
    coarse_idx: u32,
    prio: f32,
) {
    if *count < MAX_FINE_SLOTS {
        fine_set(fine_base, *count, coarse_idx);
        (*slot_dist)[*count] = prio;
        *count += 1u;
    } else {
        var maxi = 0u;
        var maxd = (*slot_dist)[0];
        for (var k = 1u; k < MAX_FINE_SLOTS; k = k + 1u) {
            if (*slot_dist)[k] > maxd { maxd = (*slot_dist)[k]; maxi = k; }
        }
        if prio < maxd {
            fine_set(fine_base, maxi, coarse_idx);
            (*slot_dist)[maxi] = prio;
        }
    }
}

// ============================================================================
// Exact segment <-> tile-box distance intervals (cull geometry)
//
// The cull asks one question per (segment, tile): does any STYLE BAND of this
// segment touch this tile square? The old cull answered it by sampling the SDF
// at the tile CENTRE and padding with the tile half-diagonal - a point sample of
// a function that varies across the tile, so diagonal curves and reflex corners
// slipped through the margin and dropped tiles (holes) or kept the wrong sign
// (filled boxes). Instead compute the EXACT range [m, M] the segment's distance
// takes over the whole box: the band [lo, hi] touches the tile iff the intervals
// overlap. The cull must be a conservative OVER-approximation - over-inclusion
// is free (a far segment renders alpha 0 per pixel), under-inclusion is a hole -
// so for the one non-convex primitive (arc) m is a lower bound and M an upper
// bound; line and point are exact (distance to a convex set is convex, so its
// max over the box is attained at a corner).
// ============================================================================

// Closest / farthest distance from a POINT to the axis-aligned box [bmin,bmax].
fn pt_box_min(c: vec2<f32>, bmin: vec2<f32>, bmax: vec2<f32>) -> f32 {
    return length(max(max(bmin - c, c - bmax), vec2(0.0, 0.0)));
}
fn pt_box_max(c: vec2<f32>, bmin: vec2<f32>, bmax: vec2<f32>) -> f32 {
    return length(max(abs(c - bmin), abs(c - bmax)));
}

// Unsigned distance from point p to segment a->b (no sign, cull only needs |d|).
fn pt_seg_dist(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ba = b - a;
    let pa = p - a;
    let l2 = dot(ba, ba);
    var t = 0.0;
    if l2 > 0.0 { t = clamp(dot(pa, ba) / l2, 0.0, 1.0); }
    return length(pa - ba * t);
}

// [min, max] distance from segment a->b to the box. Exact: max at a box corner
// (convex), min is 0 when the segment overlaps the box (4-axis SAT: the box's x
// and y axes plus the segment's tangent and normal) else the nearest
// vertex/corner pair.
fn line_box_interval(a: vec2<f32>, b: vec2<f32>, bmin: vec2<f32>, bmax: vec2<f32>) -> vec2<f32> {
    let c00 = bmin;
    let c10 = vec2(bmax.x, bmin.y);
    let c01 = vec2(bmin.x, bmax.y);
    let c11 = bmax;

    let big = max(max(pt_seg_dist(c00, a, b), pt_seg_dist(c10, a, b)),
                  max(pt_seg_dist(c01, a, b), pt_seg_dist(c11, a, b)));

    let sep_x = max(a.x, b.x) < bmin.x || min(a.x, b.x) > bmax.x;
    let sep_y = max(a.y, b.y) < bmin.y || min(a.y, b.y) > bmax.y;
    // Segment normal axis: the segment projects to the single value 0.
    let n = vec2(-(b.y - a.y), b.x - a.x);
    let n0 = dot(c00 - a, n); let n1 = dot(c10 - a, n);
    let n2 = dot(c01 - a, n); let n3 = dot(c11 - a, n);
    let sep_n = (n0 > 0.0 && n1 > 0.0 && n2 > 0.0 && n3 > 0.0)
             || (n0 < 0.0 && n1 < 0.0 && n2 < 0.0 && n3 < 0.0);
    // Segment tangent axis: the segment projects to [0, |ba|^2].
    let ba = b - a;
    let l2 = dot(ba, ba);
    let t0 = dot(c00 - a, ba); let t1 = dot(c10 - a, ba);
    let t2 = dot(c01 - a, ba); let t3 = dot(c11 - a, ba);
    let sep_t = max(max(t0, t1), max(t2, t3)) < 0.0 || min(min(t0, t1), min(t2, t3)) > l2;

    var small = 0.0;
    if sep_x || sep_y || sep_n || sep_t {
        small = min(min(pt_box_min(a, bmin, bmax), pt_box_min(b, bmin, bmax)),
                    min(min(pt_seg_dist(c00, a, b), pt_seg_dist(c10, a, b)),
                        min(pt_seg_dist(c01, a, b), pt_seg_dist(c11, a, b))));
    }
    return vec2(small, big);
}

// [min, max] distance from a circular ARC to the box. A single chord+sagitta
// bound inflates a WIDE arc (the biarc fitter allows up to ~171 deg) by sag =
// R*(1 - cos(sweep/2)) ~ 0.9R on BOTH sides, swallowing the whole concave side -
// a large false region in the cull. Instead split the arc into sub-chords each
// spanning <= ~30 deg, so the per-piece sagitta is < ~3.5% of R: the bound then
// hugs the arc and the concave side stays empty. Endpoints of every sub-chord
// lie ON the arc and the arc never leaves its sub-chord by more than that tiny
// sag, so m = min(piece) - sag is a safe lower bound and M = max(piece) + sag a
// safe upper bound. Shallow arcs (node corners) collapse to k = 1, unchanged.
fn arc_box_interval(
    center: vec2<f32>, radius: f32, start: f32, sweep: f32,
    bmin: vec2<f32>, bmax: vec2<f32>,
) -> vec2<f32> {
    let aswp = abs(sweep);
    var k = u32(ceil(aswp / 0.5235988)); // PI/6 = 30 deg per sub-chord
    k = clamp(k, 1u, 8u);
    let step = sweep / f32(k);
    let sag = radius * (1.0 - cos(0.5 * abs(step)));
    var a = start;
    var pa = center + radius * vec2(cos(a), sin(a));
    var m = 1e30;
    var big = 0.0;
    for (var j = 0u; j < k; j = j + 1u) {
        let b = a + step;
        let pb = center + radius * vec2(cos(b), sin(b));
        let iv = line_box_interval(pa, pb, bmin, bmax);
        m = min(m, iv.x - sag);
        big = max(big, iv.y + sag);
        a = b;
        pa = pb;
    }
    return vec2(max(m, 0.0), big);
}

// [min, max] distance from the arc primitive to the box. The geometry selects
// the branch (point / line / arc); the arc case reconstructs center+sweep so the
// sub-chord bound above keeps the concave interior empty.
fn seg_box_interval(seg: GpuSegment, bmin: vec2<f32>, bmax: vec2<f32>) -> vec2<f32> {
    let start = seg.endpoints.xy;
    let end = seg.endpoints.zw;
    let curvature = seg.params.x;
    if length(end - start) < POINT_EPS {
        return vec2(pt_box_min(start, bmin, bmax), pt_box_max(start, bmin, bmax));
    }
    if abs(curvature) < LINE_EPS {
        return line_box_interval(start, end, bmin, bmax);
    }
    let ap = arc_from_endpoints(start, end, curvature);
    return arc_box_interval(ap.center, ap.radius, ap.start_angle, ap.sweep, bmin, bmax);
}

// Distance from a box [bmin,bmax] (center cc, half ch) to a tiling's nearest
// feature, sampled at the center + four corners. Corners restore conservativeness
// for HEX, whose round-to-nearest field is not 1-Lipschitz across cell seams.
fn tiling_box_dist(tt: u32, params: vec4<f32>, cc: vec2<f32>, ch: vec2<f32>) -> f32 {
    var td = sd_tiling(cc, tt, params).dist;
    td = min(td, sd_tiling(cc + vec2(ch.x, ch.y), tt, params).dist);
    td = min(td, sd_tiling(cc + vec2(-ch.x, ch.y), tt, params).dist);
    td = min(td, sd_tiling(cc + vec2(ch.x, -ch.y), tt, params).dist);
    td = min(td, sd_tiling(cc + vec2(-ch.x, -ch.y), tt, params).dist);
    return td;
}

// Two-level spatial index. ONE workgroup per 64px coarse tile (4x4 = 16 threads,
// one per fine tile). Phase 0: cooperatively bin entry candidates of the coarse
// tile. Phase 1: thread 0 builds the coarse (segment, entry) result list (the
// expensive cull, at 64px). Phase 2: each thread re-culls that list at 16px and
// writes compact 8-bit indices into its fine tile - the memory win (16x more fine
// tiles, 1 byte/slot vs 8) for one indirection at shade time.
@compute @workgroup_size(4, 4, 1)
fn cs_build_index(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
    @builtin(local_invocation_index) lindex: u32,
) {
    let draw = cs_draws[wid.z];
    // wid is uniform across the workgroup, so this early return is uniform and
    // safe before the barriers below. Aborts surplus workgroups (and every
    // workgroup of a fallback draw with coarse_cols == 0).
    if wid.x >= draw.coarse_cols || wid.y >= draw.coarse_rows { return; }

    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let thd = TILE_SIZE * 0.70710678 * inv_cs;           // fine half diagonal
    let coarse_px = TILE_SIZE * f32(COARSE_FACTOR);       // 64
    let coarse_thd = coarse_px * 0.70710678 * inv_cs;     // coarse half diagonal

    // Coarse tile box in world.
    let cmin_px = vec2(f32(wid.x) * coarse_px, f32(wid.y) * coarse_px);
    let coarse_min = cmin_px * inv_cs - draw.camera_position;
    let coarse_max = (cmin_px + vec2(coarse_px, coarse_px)) * inv_cs - draw.camera_position;
    let coarse_center = (coarse_min + coarse_max) * 0.5;
    let coarse_half = (coarse_max - coarse_min) * 0.5;

    // --- Phase 0: cooperative entry binning (16 threads) ---
    if lindex == 0u { atomicStore(&wg_cand_count, 0u); }
    workgroupBarrier();
    let entry_end = draw.entry_start + draw.entry_count;
    for (var bi: u32 = draw.entry_start + lindex; bi < entry_end; bi = bi + 16u) {
        let e = cs_entries[bi];
        let st = cs_styles[e.style_idx];
        let er = style_max_dist(st) + pattern_perp_reach(st) + coarse_thd + 1.0;
        let eb = e.bounds;
        if eb.z + er >= coarse_min.x && eb.x - er <= coarse_max.x
            && eb.w + er >= coarse_min.y && eb.y - er <= coarse_max.y {
            let slot = atomicAdd(&wg_cand_count, 1u);
            if slot < MAX_WG_CANDIDATES { wg_candidates[slot] = bi; }
        }
    }
    workgroupBarrier();
    let total_cand = atomicLoad(&wg_cand_count);
    let overflow = total_cand > MAX_WG_CANDIDATES;
    let cand_count = min(total_cand, MAX_WG_CANDIDATES);

    // --- Phase 1: thread 0 builds the coarse result list over the 64px box ---
    if lindex == 0u {
        var ccount = 0u;
        var cdist: array<f32, MAX_COARSE_SLOTS>;
        let scan_count = select(cand_count, draw.entry_count, overflow);
        for (var ci: u32 = 0u; ci < scan_count; ci = ci + 1u) {
            var i: u32;
            if overflow { i = draw.entry_start + ci; } else { i = wg_candidates[ci]; }
            let entry = cs_entries[i];
            let style = cs_styles[entry.style_idx];

            if entry.entry_type == ENTRY_TILING {
                let td = tiling_box_dist(entry.tiling_type, entry.tiling_params,
                    coarse_center, coarse_half);
                if td - coarse_thd <= style_max_dist(style) + 0.5 {
                    coarse_push(&ccount, &cdist, i | TILING_BIT, i, td);
                }
                continue;
            }

            let is_closed = (entry.flags & FLAG_CLOSED) != 0u;
            let is_df = (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u;
            let bmin = coarse_min - entry.translate;
            let bmax = coarse_max - entry.translate;
            var reach = style_max_dist(style) + pattern_perp_reach(style) + 0.5;
            if style.pattern_type == PATTERN_DASHED || style.pattern_type == PATTERN_ARROWED {
                reach = reach + coarse_thd * abs(tan(style.pattern_param2));
            }

            var mu = 1e30;
            var kbound = 1e30;
            var center_signed = 1e30;
            var center_abs = 1e30;
            let lp_center = coarse_center - entry.translate;
            for (var s: u32 = 0u; s < entry.segment_count; s++) {
                let seg = cs_segments[entry.segment_start + s];
                let iv = seg_box_interval(seg, bmin, bmax);
                mu = min(mu, iv.x);
                kbound = min(kbound, iv.y);
                let rc = eval_segment(lp_center, seg);
                if abs(rc.dist) < center_abs { center_abs = abs(rc.dist); center_signed = rc.dist; }
            }
            var visible = is_df || (mu <= reach);
            if !visible && is_closed && center_signed < 0.0 { visible = true; }
            if !visible { continue; }

            for (var s: u32 = 0u; s < entry.segment_count; s++) {
                let seg_idx = entry.segment_start + s;
                let iv = seg_box_interval(cs_segments[seg_idx], bmin, bmax);
                // Priority = |distance| at the box centre, not iv.x: segments
                // inside the box all have iv.x = 0 (a tie that strands the
                // last-pushed one on overflow), whereas the centre distance ranks
                // by how much each dominates the tile.
                if iv.x <= kbound {
                    let cd = abs(eval_segment(lp_center, cs_segments[seg_idx]).dist);
                    coarse_push(&ccount, &cdist, seg_idx, i, cd);
                }
            }
        }

        // Sort by entry index (entries are pushed in z_order), so the fine phase
        // and the fragment see contiguous, front-to-back entry runs.
        for (var si: u32 = 1u; si < ccount; si++) {
            let s_seg = wg_cseg[si];
            let s_ent = wg_centry[si];
            var sj = si;
            while sj > 0u && wg_centry[sj - 1u] > s_ent {
                wg_cseg[sj] = wg_cseg[sj - 1u];
                wg_centry[sj] = wg_centry[sj - 1u];
                sj--;
            }
            wg_cseg[sj] = s_seg;
            wg_centry[sj] = s_ent;
        }

        wg_ccount = ccount;
        let coarse_global = draw.coarse_base + wid.y * draw.coarse_cols + wid.x;
        let cbase = coarse_global * COARSE_STRIDE;
        for (var j: u32 = 0u; j < ccount; j++) {
            cs_coarse_slots[cbase + j * 2u] = wg_cseg[j];
            cs_coarse_slots[cbase + j * 2u + 1u] = wg_centry[j];
        }
        cs_coarse_counts[coarse_global] = ccount;
    }
    workgroupBarrier();
    let ccount = wg_ccount;

    // --- Phase 2: each thread finalizes its 16px fine tile ---
    let fine_col = gid.x;
    let fine_row = gid.y;
    // Per-thread (non-uniform) return, but no barrier follows, so it is allowed.
    if fine_col >= draw.grid_cols || fine_row >= draw.grid_rows { return; }
    let fine_tile_idx = draw.tile_base + fine_row * draw.grid_cols + fine_col;
    let fine_base = fine_tile_idx * FINE_STRIDE;

    let fcenter = vec2((f32(fine_col) + 0.5) * TILE_SIZE, (f32(fine_row) + 0.5) * TILE_SIZE);
    let fworld = fcenter * inv_cs - draw.camera_position;
    let fhw = TILE_SIZE * 0.5 * inv_cs;
    let fhalf = vec2(fhw, fhw);

    var fcount = 0u;
    var fdist: array<f32, MAX_FINE_SLOTS>;
    var j = 0u;
    while j < ccount {
        let raw = wg_cseg[j];
        let e_idx = wg_centry[j];
        let entry = cs_entries[e_idx];
        let style = cs_styles[entry.style_idx];

        if (raw & TILING_BIT) != 0u {
            let td = tiling_box_dist(entry.tiling_type, entry.tiling_params, fworld, fhalf);
            if td - thd <= style_max_dist(style) + 0.5 {
                fine_push(fine_base, &fcount, &fdist, j, td);
            }
            j++;
            continue;
        }

        let is_closed = (entry.flags & FLAG_CLOSED) != 0u;
        let is_df = (style.flags & STYLE_FLAG_DISTANCE_FIELD) != 0u;
        let bmin = fworld - fhalf - entry.translate;
        let bmax = fworld + fhalf - entry.translate;
        var reach = style_max_dist(style) + pattern_perp_reach(style) + 0.5;
        if style.pattern_type == PATTERN_DASHED || style.pattern_type == PATTERN_ARROWED {
            reach = reach + thd * abs(tan(style.pattern_param2));
        }

        // The entry's contiguous run in the coarse list.
        var k = j;
        loop {
            if k >= ccount { break; }
            if wg_centry[k] != e_idx { break; }
            if (wg_cseg[k] & TILING_BIT) != 0u { break; }
            k++;
        }

        // Pass 1 over the run, at fine resolution.
        var mu = 1e30;
        var kbound = 1e30;
        var center_signed = 1e30;
        var center_abs = 1e30;
        let lp_center = fworld - entry.translate;
        for (var t: u32 = j; t < k; t++) {
            let seg = cs_segments[wg_cseg[t]];
            let iv = seg_box_interval(seg, bmin, bmax);
            mu = min(mu, iv.x);
            kbound = min(kbound, iv.y);
            let rc = eval_segment(lp_center, seg);
            if abs(rc.dist) < center_abs { center_abs = abs(rc.dist); center_signed = rc.dist; }
        }
        var visible = is_df || (mu <= reach);
        if !visible && is_closed && center_signed < 0.0 { visible = true; }

        // Pass 2: reference (8-bit) every coarse slot of the run that can be the
        // per-pixel nearest in this fine tile.
        if visible {
            for (var t: u32 = j; t < k; t++) {
                let iv = seg_box_interval(cs_segments[wg_cseg[t]], bmin, bmax);
                if iv.x <= kbound {
                    // Priority = |distance| at the fine tile centre (see the
                    // coarse pass): ranks by dominance, and breaks the iv.x = 0
                    // tie among segments inside the tile.
                    let cd = abs(eval_segment(lp_center, cs_segments[wg_cseg[t]]).dist);
                    fine_push(fine_base, &fcount, &fdist, t, cd);
                }
            }
        }
        j = k;
    }

    cs_fine_counts[fine_tile_idx] = fcount;
}
