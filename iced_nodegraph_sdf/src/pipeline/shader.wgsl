// Segment-based SDF renderer with a scatter-built two-level tile index.
// Coarse 64px tiles store (segment_idx_or_tiling, entry_idx) pairs as 2x u32,
// sorted by entry; 16px fine tiles store 16-bit indices into their parent
// coarse list. The fragment walks its fine tile's references per pixel.

// --- Constants ---

const TILE_SIZE: f32 = 16.0;
// Two-level spatial index. COARSE tiles are 4x4 fine tiles (64x64 px). The coarse
// level materializes the (segment, entry) cull result once per 64px tile (few
// tiles, fat slots); each 16px FINE tile then stores only compact 16-bit indices
// into its parent coarse tile's result, paying one indirection for a much smaller
// fine buffer (16x more fine tiles).
const COARSE_FACTOR: u32 = 4u;        // fine tiles per coarse tile, per axis
// Coarse: up to 512 (segment_idx, entry_idx) results per 64px tile. Scatter
// appends first-come; K3 clamps, reserving 4 slots for tilings (see plan doc).
const MAX_COARSE_SLOTS: u32 = 512u;
const COARSE_STRIDE: u32 = 1024u;     // MAX_COARSE_SLOTS * 2 u32 per coarse tile
// Fine: up to 128 16-bit indices into the parent coarse list, packed 2 per u32
// (16 bit because the coarse list is 512 deep, past a u8).
const MAX_FINE_SLOTS: u32 = 128u;
const FINE_STRIDE: u32 = 64u;         // MAX_FINE_SLOTS / 2 (u16 packed 2 per u32)

const ENTRY_TILING: u32 = 2u;
// Marker bit: slot segment_idx with this bit set = tiling (segment_idx = entry_idx)
const TILING_BIT: u32 = 0x80000000u;

// entry.flags
const FLAG_CLOSED: u32 = 1u;

// style.flags
const STYLE_FLAG_HAS_PATTERN: u32 = 1u;

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
    entry_count: u32,
    entry_start: u32,
    grid_cols: u32,
    grid_rows: u32,
    tile_base: u32,
    coarse_cols: u32,
    coarse_rows: u32,
    coarse_base: u32,
    // The draw's tiling entry ids (up to TILING_RESERVE, CULL_SENTINEL-padded),
    // carried per draw so the sort/fine kernel needs no extra storage binding
    // (the compute stage must stay within 8 storage buffers per stage - the
    // WebGPU spec default - for wasm).
    tiling0: u32,
    tiling1: u32,
    tiling2: u32,
    tiling3: u32,
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
// Fine level: per 16px tile, a count and a run of 16-bit indices (packed 2/u32)
// into the parent coarse tile's result list.
@group(0) @binding(4) var<storage, read> fine_counts: array<u32>;
@group(0) @binding(5) var<storage, read> fine_slots: array<u32>;
// Coarse level: per 64px tile, COARSE_STRIDE u32s = MAX_COARSE_SLOTS pairs of
// (segment_idx_or_tiling, entry_idx). The fine 16-bit index addresses these.
@group(0) @binding(6) var<storage, read> coarse_slots: array<u32>;

// --- Compute bindings ---

@group(0) @binding(0) var<storage, read> cs_draws: array<DrawData>;
@group(0) @binding(1) var<storage, read> cs_entries: array<GpuDrawEntry>;
@group(0) @binding(2) var<storage, read> cs_segments: array<GpuSegment>;
@group(0) @binding(3) var<storage, read> cs_styles: array<GpuStyle>;

// draw_index is carried by the dispatch z-axis (workgroup_id.z), not a uniform.
//
// The compute stage must stay within the WebGPU spec-default limit of 8
// storage buffers per stage (group 0 already binds 4), so group 1 holds at
// most 4 bindings per PIPELINE. Bindings (1,2)/(1,3) are declared twice with
// different names; that is valid WGSL as long as no single entry point
// references both declarations - the scatter kernels use the work list + meta,
// the sort/fine kernel uses the fine buffers.
@group(1) @binding(0) var<storage, read_write> cs_coarse_counts: array<atomic<u32>>;
@group(1) @binding(1) var<storage, read_write> cs_coarse_slots: array<u32>;
// Sort/fine kernel only: the fine-tile outputs.
@group(1) @binding(2) var<storage, read_write> cs_fine_counts: array<u32>;
@group(1) @binding(3) var<storage, read_write> cs_fine_slots: array<u32>;
// Scatter kernels only: the flat work list, built on the CPU alongside the
// entry batch and reused under the same slot-reuse discipline (ABSOLUTE
// indices). The bind group selects the content per kernel: (draw, entry,
// segment) triples of OPEN entries for cs_scatter_open, (draw, entry) pairs
// of CLOSED entries for cs_scatter_closed.
@group(1) @binding(2) var<storage, read> cs_cull_list: array<u32>;
// Live element counts: [0] = open triples, [1] = closed pairs. Needed because
// `arrayLength` reports buffer CAPACITY, not this frame's live length.
@group(1) @binding(3) var<storage, read> cs_cull_meta: array<u32>;

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
// (tile_col, tile_row). The fine tile's 16-bit slots index into this run.
fn coarse_base_for_fine(draw: DrawData, tile_col: u32, tile_row: u32) -> u32 {
    let cc = tile_col / COARSE_FACTOR;
    let cr = tile_row / COARSE_FACTOR;
    return (draw.coarse_base + cr * draw.coarse_cols + cc) * COARSE_STRIDE;
}

// Unpack the k-th 16-bit slot (coarse-list index) of a fine tile.
fn fine_coarse_index(fine_base: u32, k: u32) -> u32 {
    let word = fine_slots[fine_base + (k >> 1u)];
    return (word >> ((k & 1u) * 16u)) & 0xFFFFu;
}

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
        let count = fine_counts[tile_idx];

        if count == 0u {
            discard;
        }

        // Each fine slot is a 16-bit index into this tile's parent coarse list,
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

    if acc.a < 0.001 { discard; }
    return acc;
}

// ============================================================================
// Compute Shader - Per-Segment Spatial Index Builder
// ============================================================================


// --- Workgroup state for the sort+fine kernel (one workgroup per coarse tile) ---
// The coarse result list: (segment_idx_or_tiling, entry_idx) pairs, loaded from
// the scattered slots, sorted in place, read by all threads in the fine phase.
var<workgroup> wg_cseg: array<u32, 512>;
var<workgroup> wg_centry: array<u32, 512>;
var<workgroup> wg_ccount: u32;
// Contour bbox of a closed entry, folded by thread 0 in cs_scatter_closed.
var<workgroup> wg_bbox: vec4<f32>;

// Sentinel for unused DrawData tiling slots and sort padding.
const CULL_SENTINEL: u32 = 0xFFFFFFFFu;
// Coarse slots K3 reserves for the draw's tilings when clamping the scattered
// contour slots, so an overflowing tile can never drop the background.
const TILING_RESERVE: u32 = 4u;

// Style reach band half-width for the cull: the outermost stop distance plus
// the pattern's perpendicular reach plus the AA safety margin; dashed/arrowed
// caps tilt with the pattern angle, widening with the tile half-diagonal.
fn style_reach(style: GpuStyle, tile_thd: f32) -> f32 {
    var reach = style_max_dist(style) + pattern_perp_reach(style) + 0.5;
    if style.pattern_type == PATTERN_DASHED || style.pattern_type == PATTERN_ARROWED {
        reach = reach + tile_thd * abs(tan(style.pattern_param2));
    }
    return reach;
}

// Conservative bbox (min.xy, max.xy) of one segment in its LOCAL frame.
// Line/point: endpoint box. Arc: bbox of the 30-degree sub-chord endpoints
// inflated by the per-piece sagitta (the split of `arc_box_interval`), so the
// bound hugs the curve instead of swallowing the chord's whole bulge side.
fn seg_bbox(seg: GpuSegment) -> vec4<f32> {
    let s = seg.endpoints.xy;
    let e = seg.endpoints.zw;
    let curvature = seg.params.x;
    var lo = min(s, e);
    var hi = max(s, e);
    if length(e - s) >= POINT_EPS && abs(curvature) >= LINE_EPS {
        let ap = arc_from_endpoints(s, e, curvature);
        var k = u32(ceil(abs(ap.sweep) / 0.5235988)); // PI/6 per sub-chord
        k = clamp(k, 1u, 8u);
        let step = ap.sweep / f32(k);
        let sag = ap.radius * (1.0 - cos(0.5 * abs(step)));
        var ang = ap.start_angle;
        for (var j = 0u; j <= k; j = j + 1u) {
            let p = ap.center + ap.radius * vec2(cos(ang), sin(ang));
            lo = min(lo, p - vec2(sag, sag));
            hi = max(hi, p + vec2(sag, sag));
            ang = ang + step;
        }
    }
    return vec4(lo, hi);
}

// Inclusive coarse-tile range of a draw covered by the world bbox [lo, hi];
// empty (x0 > x1) when the bbox misses the grid or the draw is a fallback
// (coarse_cols == 0). Grid pixels are local: px = (world + camera) * zoom * scale.
struct TileRange {
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
}
fn coarse_range(draw: DrawData, lo: vec2<f32>, hi: vec2<f32>) -> TileRange {
    var r: TileRange;
    r.x0 = 1u; r.x1 = 0u; r.y0 = 1u; r.y1 = 0u;
    if draw.coarse_cols == 0u || draw.coarse_rows == 0u {
        return r;
    }
    let cs = draw.camera_zoom * draw.scale_factor;
    let coarse_px = TILE_SIZE * f32(COARSE_FACTOR);
    let px_lo = (lo + draw.camera_position) * cs / coarse_px;
    let px_hi = (hi + draw.camera_position) * cs / coarse_px;
    let x0f = floor(px_lo.x);
    let y0f = floor(px_lo.y);
    let x1f = floor(px_hi.x);
    let y1f = floor(px_hi.y);
    if x1f < 0.0 || y1f < 0.0
        || x0f >= f32(draw.coarse_cols) || y0f >= f32(draw.coarse_rows) {
        return r;
    }
    r.x0 = u32(max(x0f, 0.0));
    r.y0 = u32(max(y0f, 0.0));
    r.x1 = u32(min(x1f, f32(draw.coarse_cols - 1u)));
    r.y1 = u32(min(y1f, f32(draw.coarse_rows - 1u)));
    return r;
}

// World-space box (min.xy, max.xy) of coarse tile (tx, ty) of a draw.
fn coarse_tile_box(draw: DrawData, tx: u32, ty: u32) -> vec4<f32> {
    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let coarse_px = TILE_SIZE * f32(COARSE_FACTOR);
    let cmin_px = vec2(f32(tx), f32(ty)) * coarse_px;
    let lo = cmin_px * inv_cs - draw.camera_position;
    let hi = (cmin_px + vec2(coarse_px, coarse_px)) * inv_cs - draw.camera_position;
    return vec4(lo, hi);
}

// Reserve one coarse slot of `coarse_global` and write the (seg, entry) pair.
// Beyond the cap the pair is DROPPED first-come (the scatter cannot rank by
// distance like the old single-threaded keep-nearest did); the count keeps
// rising past the cap, so between the scatter and the sort it holds TRUE
// demand - the overflow telemetry snapshots it in that window (the sort then
// overwrites it with the clamped render list length). See
// plan/scatter-binning.md and plan/exact-slot-allocation.md.
fn coarse_append(coarse_global: u32, seg_field: u32, entry_idx: u32) {
    let idx = atomicAdd(&cs_coarse_counts[coarse_global], 1u);
    if idx < MAX_COARSE_SLOTS {
        let base = coarse_global * COARSE_STRIDE;
        cs_coarse_slots[base + idx * 2u] = seg_field;
        cs_coarse_slots[base + idx * 2u + 1u] = entry_idx;
    }
}

// Write a 16-bit coarse-list index into fine slot `slot` (2 indices per u32).
// Mask-write preserves the word's already-written half; halves past the fine
// count are never read, so stale data there is harmless.
fn fine_set(fine_base: u32, slot: u32, coarse_idx: u32) {
    let word = fine_base + (slot >> 1u);
    let shift = (slot & 1u) * 16u;
    cs_fine_slots[word] = (cs_fine_slots[word] & ~(0xFFFFu << shift))
        | ((coarse_idx & 0xFFFFu) << shift);
}

// Read the k-th 16-bit fine slot back (compute-side mirror of the fragment's
// `fine_coarse_index`).
fn cs_fine_get(fine_base: u32, k: u32) -> u32 {
    let word = cs_fine_slots[fine_base + (k >> 1u)];
    return (word >> ((k & 1u) * 16u)) & 0xFFFFu;
}

// Append (or keep-nearest replace) a coarse-list index into a fine tile.
// Single-threaded per fine tile (each thread owns one), so no atomics.
// Returns true when an existing slot was REPLACED: the list is then no longer
// index-ascending and the caller must re-sort it, because the fragment folds
// CONSECUTIVE same-entry references into one contour (a scrambled list splits
// an entry into multiple runs, compositing it twice).
fn fine_push(
    fine_base: u32,
    count: ptr<function, u32>,
    slot_dist: ptr<function, array<f32, MAX_FINE_SLOTS>>,
    coarse_idx: u32,
    prio: f32,
) -> bool {
    if *count < MAX_FINE_SLOTS {
        fine_set(fine_base, *count, coarse_idx);
        (*slot_dist)[*count] = prio;
        *count += 1u;
        return false;
    }
    var maxi = 0u;
    var maxd = (*slot_dist)[0];
    for (var k = 1u; k < MAX_FINE_SLOTS; k = k + 1u) {
        if (*slot_dist)[k] > maxd { maxd = (*slot_dist)[k]; maxi = k; }
    }
    if prio < maxd {
        fine_set(fine_base, maxi, coarse_idx);
        (*slot_dist)[maxi] = prio;
        return true;
    }
    return false;
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

// ============================================================================
// Scatter cull (see plan/scatter-binning.md)
//
// The gather build scanned EVERY entry x segment from EVERY coarse tile -
// O(tiles x segments) regardless of visibility. The scatter flips the
// iteration: each (entry, segment) pair visits only the coarse tiles inside
// its reach-inflated bbox and appends itself where the EXACT interval test
// passes. Work is proportional to actual segment-tile overlaps. The cull TEST
// is unchanged (`seg_box_interval` interval vs the style reach band), so the
// conservative-over-approximation contract holds as before.
//
// cs_scatter_open   - one thread per (draw, entry, segment) triple (open entries)
// cs_scatter_closed - one workgroup per closed entry (interior-aware)
// cs_sort_fine      - one workgroup per coarse tile: sort + fine re-cull
// ============================================================================

// Flat work-item index for the scatter kernels: the x dimension is capped at
// 65535 workgroups, so y extends it (see the dispatch in run_deferred_compute).
fn scatter_flat_id(wid: vec3<u32>, lidx: u32) -> u32 {
    return (wid.y * 65535u + wid.x) * 64u + lidx;
}

// One thread per (draw, entry, segment) triple of an OPEN entry. Kbound
// tightening (only potential-nearest segments) needs cross-segment state and
// is left to the fine phase; extra coarse slots are safe, the fragment folds
// the nearest segment anyway.
@compute @workgroup_size(64, 1, 1)
fn cs_scatter_open(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) lidx: u32,
) {
    let i = scatter_flat_id(wid, lidx);
    if i >= cs_cull_meta[0] { return; }
    let draw_idx = cs_cull_list[i * 3u];
    let entry_idx = cs_cull_list[i * 3u + 1u];
    let seg_idx = cs_cull_list[i * 3u + 2u];
    let draw = cs_draws[draw_idx];
    let entry = cs_entries[entry_idx];
    let style = cs_styles[entry.style_idx];

    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let coarse_thd = TILE_SIZE * f32(COARSE_FACTOR) * 0.70710678 * inv_cs;
    let reach = style_reach(style, coarse_thd);

    let seg = cs_segments[seg_idx];
    let bb = seg_bbox(seg);
    let lo = bb.xy + entry.translate - vec2(reach, reach);
    let hi = bb.zw + entry.translate + vec2(reach, reach);
    let r = coarse_range(draw, lo, hi);
    if r.x0 > r.x1 || r.y0 > r.y1 { return; }

    for (var ty = r.y0; ty <= r.y1; ty++) {
        for (var tx = r.x0; tx <= r.x1; tx++) {
            let tb = coarse_tile_box(draw, tx, ty);
            let bmin = tb.xy - entry.translate;
            let bmax = tb.zw - entry.translate;
            if seg_box_interval(seg, bmin, bmax).x <= reach {
                coarse_append(draw.coarse_base + ty * draw.coarse_cols + tx, seg_idx, entry_idx);
            }
        }
    }
}

// One 64-thread workgroup per CLOSED entry. Thread 0 folds the contour bbox
// (the interior is inside it, so bbox iteration cannot miss interior-only
// tiles); the reach-inflated, grid-clipped tile range is strided across the
// threads, each running the exact per-entry gather test per tile: keep the
// tile when the contour band reaches it (`mu <= reach`) OR the tile centre is
// inside the fill (`center_signed < 0`), then push every segment that can be
// the per-pixel nearest anywhere in the tile (`iv.x <= kbound`).
@compute @workgroup_size(64, 1, 1)
fn cs_scatter_closed(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) lidx: u32,
) {
    let pair_i = wid.y * 65535u + wid.x;
    // wid is uniform across the workgroup, so this return is uniform and safe
    // before the barrier below.
    if pair_i >= cs_cull_meta[1] { return; }
    let draw_idx = cs_cull_list[pair_i * 2u];
    let entry_idx = cs_cull_list[pair_i * 2u + 1u];
    let draw = cs_draws[draw_idx];
    let entry = cs_entries[entry_idx];
    let style = cs_styles[entry.style_idx];

    if lidx == 0u {
        var lo = vec2(1e30, 1e30);
        var hi = vec2(-1e30, -1e30);
        for (var s = 0u; s < entry.segment_count; s++) {
            let bb = seg_bbox(cs_segments[entry.segment_start + s]);
            lo = min(lo, bb.xy);
            hi = max(hi, bb.zw);
        }
        wg_bbox = vec4(lo, hi);
    }
    workgroupBarrier();

    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let coarse_thd = TILE_SIZE * f32(COARSE_FACTOR) * 0.70710678 * inv_cs;
    let reach = style_reach(style, coarse_thd);
    let lo = wg_bbox.xy + entry.translate - vec2(reach, reach);
    let hi = wg_bbox.zw + entry.translate + vec2(reach, reach);
    let r = coarse_range(draw, lo, hi);
    if r.x0 > r.x1 || r.y0 > r.y1 { return; }

    let ncols = r.x1 - r.x0 + 1u;
    let ntiles = ncols * (r.y1 - r.y0 + 1u);
    for (var t = lidx; t < ntiles; t += 64u) {
        let tx = r.x0 + t % ncols;
        let ty = r.y0 + t / ncols;
        let tb = coarse_tile_box(draw, tx, ty);
        let bmin = tb.xy - entry.translate;
        let bmax = tb.zw - entry.translate;
        let lp_center = (bmin + bmax) * 0.5;

        var mu = 1e30;
        var kbound = 1e30;
        var center_signed = 1e30;
        var center_abs = 1e30;
        for (var s = 0u; s < entry.segment_count; s++) {
            let seg = cs_segments[entry.segment_start + s];
            let iv = seg_box_interval(seg, bmin, bmax);
            mu = min(mu, iv.x);
            kbound = min(kbound, iv.y);
            let rc = eval_segment(lp_center, seg);
            if abs(rc.dist) < center_abs { center_abs = abs(rc.dist); center_signed = rc.dist; }
        }
        var visible = mu <= reach;
        if !visible && center_signed < 0.0 { visible = true; }
        if !visible { continue; }

        let cg = draw.coarse_base + ty * draw.coarse_cols + tx;
        for (var s = 0u; s < entry.segment_count; s++) {
            let seg_idx = entry.segment_start + s;
            if seg_box_interval(cs_segments[seg_idx], bmin, bmax).x <= kbound {
                coarse_append(cg, seg_idx, entry_idx);
            }
        }
    }
}

// One 64-thread workgroup per coarse tile (dispatch x/y = coarse grid, z =
// draw index). Loads the scattered slots (clamped, TILING_RESERVE spare),
// appends the draw's tilings, bitonic-sorts by (entry, seg) - a UNIQUE total
// order, so the frame is deterministic regardless of atomic append order -
// writes the sorted list back, then threads 0..15 run the fine re-cull
// (unchanged 16px logic, keep-nearest, 16-bit refs).
@compute @workgroup_size(64, 1, 1)
fn cs_sort_fine(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) lindex: u32,
) {
    let draw = cs_draws[wid.z];
    // Uniform abort for surplus workgroups and fallback draws (coarse_cols == 0).
    if wid.x >= draw.coarse_cols || wid.y >= draw.coarse_rows { return; }

    let inv_cs = 1.0 / (draw.camera_zoom * draw.scale_factor);
    let thd = TILE_SIZE * 0.70710678 * inv_cs;           // fine half diagonal
    let coarse_px = TILE_SIZE * f32(COARSE_FACTOR);       // 64
    let coarse_thd = coarse_px * 0.70710678 * inv_cs;     // coarse half diagonal
    let coarse_global = draw.coarse_base + wid.y * draw.coarse_cols + wid.x;
    let cbase = coarse_global * COARSE_STRIDE;

    // Load the scattered slots; pad the tail with sort sentinels.
    let demand = atomicLoad(&cs_coarse_counts[coarse_global]);
    let loaded = min(demand, MAX_COARSE_SLOTS - TILING_RESERVE);
    for (var s = lindex; s < MAX_COARSE_SLOTS; s += 64u) {
        if s < loaded {
            wg_cseg[s] = cs_coarse_slots[cbase + s * 2u];
            wg_centry[s] = cs_coarse_slots[cbase + s * 2u + 1u];
        } else {
            wg_cseg[s] = CULL_SENTINEL;
            wg_centry[s] = CULL_SENTINEL;
        }
    }
    workgroupBarrier();

    // Thread 0 appends the draw's tilings (reach-tested at coarse resolution).
    if lindex == 0u {
        let cmin_px = vec2(f32(wid.x), f32(wid.y)) * coarse_px;
        let coarse_min = cmin_px * inv_cs - draw.camera_position;
        let coarse_max = (cmin_px + vec2(coarse_px, coarse_px)) * inv_cs - draw.camera_position;
        let coarse_center = (coarse_min + coarse_max) * 0.5;
        let coarse_half = (coarse_max - coarse_min) * 0.5;
        var cnt = loaded;
        var draw_tilings = array<u32, 4>(draw.tiling0, draw.tiling1, draw.tiling2, draw.tiling3);
        for (var k = 0u; k < TILING_RESERVE; k++) {
            let te = draw_tilings[k];
            if te == CULL_SENTINEL { continue; }
            let entry = cs_entries[te];
            let style = cs_styles[entry.style_idx];
            let td = tiling_box_dist(entry.tiling_type, entry.tiling_params,
                coarse_center, coarse_half);
            if td - coarse_thd <= style_max_dist(style) + 0.5 {
                wg_cseg[cnt] = te | TILING_BIT;
                wg_centry[cnt] = te;
                cnt++;
            }
        }
        wg_ccount = cnt;
    }
    // `workgroupUniformLoad` both synchronizes and yields a PROVABLY uniform
    // value, so the barrier inside the count-dependent sort loop below is
    // legal under WGSL uniformity analysis.
    let ccount = workgroupUniformLoad(&wg_ccount);

    // Bitonic sort, ascending (entry, seg); sentinel pairs sink to the tail.
    // Entries were pushed in z order, so entry-ascending is front-to-back and
    // the fragment sees contiguous runs. The network is sized to the next
    // power of two covering the LIVE slots: a near-empty tile (the common
    // case for small clipped node draws) pays ~log2(4)^2 stages instead of
    // the fixed 512-wide network's 45. Elements past `n` are all sentinels
    // (maximal keys), so leaving them out keeps the array globally sorted.
    var n = 2u;
    while n < ccount { n = n << 1u; }
    for (var k = 2u; k <= n; k = k << 1u) {
        for (var j = k >> 1u; j > 0u; j = j >> 1u) {
            for (var i = lindex; i < n; i += 64u) {
                let ixj = i ^ j;
                if ixj > i {
                    let e_a = wg_centry[i];
                    let s_a = wg_cseg[i];
                    let e_b = wg_centry[ixj];
                    let s_b = wg_cseg[ixj];
                    let a_gt_b = e_a > e_b || (e_a == e_b && s_a > s_b);
                    let ascending = (i & k) == 0u;
                    if ascending == a_gt_b {
                        wg_centry[i] = e_b;
                        wg_cseg[i] = s_b;
                        wg_centry[ixj] = e_a;
                        wg_cseg[ixj] = s_a;
                    }
                }
            }
            workgroupBarrier();
        }
    }

    // Write the sorted list back for the fragment shader.
    for (var s = lindex; s < ccount; s += 64u) {
        cs_coarse_slots[cbase + s * 2u] = wg_cseg[s];
        cs_coarse_slots[cbase + s * 2u + 1u] = wg_centry[s];
    }
    if lindex == 0u {
        atomicStore(&cs_coarse_counts[coarse_global], ccount);
    }

    // --- Fine phase: threads 0..15, one 16px tile each (unchanged logic) ---
    // Per-thread (non-uniform) returns; no barrier follows, so they are allowed.
    if lindex >= 16u { return; }
    let fine_col = wid.x * COARSE_FACTOR + (lindex % COARSE_FACTOR);
    let fine_row = wid.y * COARSE_FACTOR + (lindex / COARSE_FACTOR);
    if fine_col >= draw.grid_cols || fine_row >= draw.grid_rows { return; }
    let fine_tile_idx = draw.tile_base + fine_row * draw.grid_cols + fine_col;
    let fine_base = fine_tile_idx * FINE_STRIDE;

    let fcenter = vec2((f32(fine_col) + 0.5) * TILE_SIZE, (f32(fine_row) + 0.5) * TILE_SIZE);
    let fworld = fcenter * inv_cs - draw.camera_position;
    let fhw = TILE_SIZE * 0.5 * inv_cs;
    let fhalf = vec2(fhw, fhw);

    var fcount = 0u;
    var fdist: array<f32, MAX_FINE_SLOTS>;
    var freplaced = false;
    var j = 0u;
    while j < ccount {
        let raw = wg_cseg[j];
        let e_idx = wg_centry[j];
        let entry = cs_entries[e_idx];
        let style = cs_styles[entry.style_idx];

        if (raw & TILING_BIT) != 0u {
            let td = tiling_box_dist(entry.tiling_type, entry.tiling_params, fworld, fhalf);
            if td - thd <= style_max_dist(style) + 0.5 {
                freplaced = fine_push(fine_base, &fcount, &fdist, j, td) || freplaced;
            }
            j++;
            continue;
        }

        let is_closed = (entry.flags & FLAG_CLOSED) != 0u;
        let bmin = fworld - fhalf - entry.translate;
        let bmax = fworld + fhalf - entry.translate;
        let reach = style_reach(style, thd);

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
        var visible = (mu <= reach);
        if !visible && is_closed && center_signed < 0.0 { visible = true; }

        // Pass 2: reference (16-bit) every coarse slot of the run that can be
        // the per-pixel nearest in this fine tile.
        if visible {
            for (var t: u32 = j; t < k; t++) {
                let iv = seg_box_interval(cs_segments[wg_cseg[t]], bmin, bmax);
                if iv.x <= kbound {
                    // Priority = |distance| at the fine tile centre: ranks by
                    // dominance, and breaks the iv.x = 0 tie among segments
                    // inside the tile.
                    let cd = abs(eval_segment(lp_center, cs_segments[wg_cseg[t]]).dist);
                    freplaced = fine_push(fine_base, &fcount, &fdist, t, cd) || freplaced;
                }
            }
        }
        j = k;
    }

    // Keep-nearest replacement scrambles the reference order; ascending
    // indices into the SORTED coarse list restore entry-contiguous,
    // front-to-back runs for the fragment fold. Only overflowing tiles
    // (>MAX_FINE_SLOTS surviving candidates) ever pay this.
    if freplaced {
        for (var a = 1u; a < fcount; a++) {
            let v = cs_fine_get(fine_base, a);
            var b = a;
            while b > 0u && cs_fine_get(fine_base, b - 1u) > v {
                fine_set(fine_base, b, cs_fine_get(fine_base, b - 1u));
                b--;
            }
            fine_set(fine_base, b, v);
        }
    }

    cs_fine_counts[fine_tile_idx] = fcount;
}
