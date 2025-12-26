// ============================================================================
// UNIFORMS AND STORAGE BUFFERS
// ============================================================================

struct Uniforms {
    os_scale_factor: f32,
    camera_zoom: f32,
    camera_position: vec2<f32>,

    border_color: vec4<f32>,
    fill_color: vec4<f32>,
    edge_color: vec4<f32>,
    background_color: vec4<f32>,
    drag_edge_color: vec4<f32>,
    drag_edge_valid_color: vec4<f32>,

    cursor_position: vec2<f32>,

    num_nodes: u32,
    num_pins: u32,
    num_edges: u32,
    time: f32,

    dragging: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    dragging_edge_from_node: u32,
    dragging_edge_from_pin: u32,
    dragging_edge_from_origin: vec2<f32>,
    dragging_edge_to_node: u32,
    dragging_edge_to_pin: u32,

    // Dragging edge gradient colors (resolved in Rust from pin colors)
    dragging_edge_start_color: vec4<f32>,  // Color at source pin end
    dragging_edge_end_color: vec4<f32>,    // Color at cursor/target end

    // Theme-derived visual parameters (computed in Rust, no hardcodes in shader)
    grid_color: vec4<f32>,           // Pre-computed grid line color
    hover_glow_color: vec4<f32>,     // Node hover glow color
    selection_box_color: vec4<f32>,  // Box selection fill/border color
    edge_cutting_color: vec4<f32>,   // Edge cutting line color
    hover_glow_radius: f32,          // Node hover glow radius in world units
    edge_thickness: f32,             // Default edge thickness for dragging
    render_mode: u32,                // 0=background (fill only), 1=foreground (border only)
    _pad_theme1: u32,

    viewport_size: vec2<f32>,
    bounds_origin: vec2<f32>,  // widget bounds origin in physical pixels
    bounds_size: vec2<f32>,    // widget bounds size in physical pixels
};

struct Node {
    position: vec2<f32>,
    size: vec2<f32>,
    corner_radius: f32,
    border_width: f32,
    opacity: f32,
    pin_start: u32,
    pin_count: u32,
    shadow_blur: f32,
    shadow_offset: vec2<f32>,
    fill_color: vec4<f32>,
    border_color: vec4<f32>,
    shadow_color: vec4<f32>,
    flags: u32,        // bit 0: hovered, bit 1: selected
    _pad_flags0: u32,
    _pad_flags1: u32,
    _pad_flags2: u32,
};

// Node flag constants
const NODE_FLAG_HOVERED: u32 = 1u;
const NODE_FLAG_SELECTED: u32 = 2u;

// Pin flag constants (computed in Rust)
const PIN_FLAG_VALID_TARGET: u32 = 1u;

struct Pin {
    position: vec2<f32>,
    side: u32,
    radius: f32,
    color: vec4<f32>,
    border_color: vec4<f32>,
    direction: u32,
    shape: u32,          // 0=Circle, 1=Square, 2=Diamond, 3=Triangle
    border_width: f32,
    flags: u32,
};

// Edge with resolved world positions (no index lookups needed)
struct Edge {
    // Positions (resolved in Rust from node + pin offset)
    start: vec2<f32>,           // World position of source pin
    end: vec2<f32>,             // World position of target pin
    start_direction: u32,       // PinSide: 0=Left, 1=Right, 2=Top, 3=Bottom
    end_direction: u32,         // PinSide for target pin
    _pad_align0: u32,           // Padding for vec4 alignment
    _pad_align1: u32,

    // Colors (already resolved from pin colors if needed)
    start_color: vec4<f32>,     // Color at source (t=0)
    end_color: vec4<f32>,       // Color at target (t=1)

    // Style parameters
    thickness: f32,
    edge_type: u32,             // 0=Bezier, 1=Straight, 2=SmoothStep, 3=Step
    dash_length: f32,           // 0.0 = solid line
    gap_length: f32,
    flow_speed: f32,            // pixels per second
    flags: u32,                 // bit 0: animated dash, bit 1: glow, bit 2: pulse, bit 3: pending cut
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var<storage, read> nodes: array<Node>;

@group(0) @binding(2)
var<storage, read> pins: array<Pin>;

@group(0) @binding(3)
var<storage, read> edges: array<Edge>;

// ============================================================================
// VERTEX OUTPUT STRUCTS
// ============================================================================

struct EdgeVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_id: u32,
}

struct NodeVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_id: u32,
}

struct PinVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_id: u32,
    @location(2) @interpolate(flat) node_id: u32,
    @location(3) @interpolate(flat) pin_index: u32,
}

// ============================================================================
// SDF FUNCTIONS
// ============================================================================

fn sd_rounded_box(center: vec2<f32>, half_size: vec2<f32>, r: vec4<f32>) -> f32 {
    let rxz = select(r.zw, r.xy, center.x > 0.0);
    let rxy = select(vec2<f32>(rxz.y, rxz.y), vec2<f32>(rxz.x, rxz.x), center.y > 0.0);
    let corner_radius = rxy.x;

    let q = abs(center) - half_size + vec2<f32>(corner_radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - corner_radius;
}

fn sd_circle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

// SDF for square (axis-aligned box)
fn sd_box(p: vec2<f32>, half_size: f32) -> f32 {
    let d = abs(p) - vec2(half_size);
    return length(max(d, vec2(0.0))) + min(max(d.x, d.y), 0.0);
}

// SDF for diamond (45-degree rotated square)
fn sd_diamond(p: vec2<f32>, half_size: f32) -> f32 {
    // Rotate by 45 degrees
    let rot_p = vec2(p.x + p.y, p.y - p.x) * 0.7071067811865476; // 1/sqrt(2)
    return sd_box(rot_p, half_size);
}

// SDF for equilateral triangle pointing right
fn sd_triangle(p: vec2<f32>, r: f32) -> f32 {
    // Equilateral triangle pointing to the right (along +x axis)
    let k = 1.73205080757; // sqrt(3)
    var q = p;
    q.y = abs(q.y) - r;
    q.x = q.x + r / k;
    if (q.x + k * q.y > 0.0) {
        q = vec2(q.x - k * q.y, -k * q.x - q.y) * 0.5;
    }
    q.x -= clamp(q.x, -2.0 * r / k, 0.0);
    return -length(q) * sign(q.y);
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

fn sdCubicBezier(pos: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    let result = sdCubicBezierWithT(pos, p0, p1, p2, p3);
    return result.x;
}

// Returns vec2(distance, t) where t is the parameter along the curve [0,1]
fn sdCubicBezierWithT(pos: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let A = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let B = 3.0 * p0 - 6.0 * p1 + 3.0 * p2;
    let C = -3.0 * p0 + 3.0 * p1;
    let D = p0;

    var min_dist = dot2(pos - p0);
    var best_t = 0.0;

    for (var i = 0; i <= 8; i = i + 1) {
        var t = f32(i) / 8.0;

        for (var iter = 0; iter < 4; iter = iter + 1) {
            let t2 = t * t;
            let t3 = t2 * t;

            let point = A * t3 + B * t2 + C * t + D;
            let deriv = 3.0 * A * t2 + 2.0 * B * t + C;
            let deriv2 = 6.0 * A * t + 2.0 * B;
            let diff = point - pos;

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
        let dist = dot2(pos - point);

        if (dist < min_dist) {
            min_dist = dist;
            best_t = t;
        }
    }

    // Check endpoint
    let end_dist = dot2(pos - p3);
    if (end_dist < min_dist) {
        min_dist = end_dist;
        best_t = 1.0;
    }

    return vec2<f32>(sqrt(min_dist), best_t);
}

// Approximate bezier curve length using chord + control polygon method
fn estimateBezierLength(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    let chord = length(p3 - p0);
    let control_net = length(p1 - p0) + length(p2 - p1) + length(p3 - p2);
    return (chord + control_net) * 0.5;
}

// Straight line SDF with t parameter
fn sdStraightLine(pos: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> vec2<f32> {
    let pa = pos - p0;
    let ba = p1 - p0;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    let dist = length(pa - ba * h);
    return vec2<f32>(dist, h);
}

// Step path SDF (orthogonal with sharp corners)
// Returns vec2(distance, t) where t is normalized position along the path [0,1]
fn sdStepPath(pos: vec2<f32>, p0: vec2<f32>, p3: vec2<f32>, dir_from: vec2<f32>, dir_to: vec2<f32>) -> vec2<f32> {
    // Determine path layout based on pin directions
    // Horizontal pins (Left/Right) go horizontal first, vertical pins go vertical first
    let horizontal_first = abs(dir_from.x) > 0.5;

    var corner1: vec2<f32>;
    var corner2: vec2<f32>;

    if (horizontal_first) {
        // Horizontal -> Vertical -> Horizontal
        let mid_x = (p0.x + p3.x) * 0.5;
        corner1 = vec2(mid_x, p0.y);
        corner2 = vec2(mid_x, p3.y);
    } else {
        // Vertical -> Horizontal -> Vertical
        let mid_y = (p0.y + p3.y) * 0.5;
        corner1 = vec2(p0.x, mid_y);
        corner2 = vec2(p3.x, mid_y);
    }

    // Calculate segment lengths for t parameter
    let len1 = length(corner1 - p0);
    let len2 = length(corner2 - corner1);
    let len3 = length(p3 - corner2);
    let total_len = len1 + len2 + len3;

    // Avoid division by zero
    if (total_len < 0.001) {
        return vec2(length(pos - p0), 0.0);
    }

    // Find closest point on each segment
    var min_dist = 1e10;
    var best_t = 0.0;

    // Segment 1: p0 to corner1
    let result1 = sdStraightLine(pos, p0, corner1);
    if (result1.x < min_dist) {
        min_dist = result1.x;
        best_t = result1.y * len1 / total_len;
    }

    // Segment 2: corner1 to corner2
    let result2 = sdStraightLine(pos, corner1, corner2);
    if (result2.x < min_dist) {
        min_dist = result2.x;
        best_t = (len1 + result2.y * len2) / total_len;
    }

    // Segment 3: corner2 to p3
    let result3 = sdStraightLine(pos, corner2, p3);
    if (result3.x < min_dist) {
        min_dist = result3.x;
        best_t = (len1 + len2 + result3.y * len3) / total_len;
    }

    return vec2(min_dist, best_t);
}

// Compute step path length
fn stepPathLength(p0: vec2<f32>, p3: vec2<f32>, dir_from: vec2<f32>) -> f32 {
    let horizontal_first = abs(dir_from.x) > 0.5;

    if (horizontal_first) {
        let mid_x = (p0.x + p3.x) * 0.5;
        return abs(mid_x - p0.x) + abs(p3.y - p0.y) + abs(p3.x - mid_x);
    } else {
        let mid_y = (p0.y + p3.y) * 0.5;
        return abs(mid_y - p0.y) + abs(p3.x - p0.x) + abs(p3.y - mid_y);
    }
}

// Compute bounding box for step path
fn stepPathBounds(p0: vec2<f32>, p3: vec2<f32>, dir_from: vec2<f32>) -> vec4<f32> {
    let horizontal_first = abs(dir_from.x) > 0.5;

    var corner1: vec2<f32>;
    var corner2: vec2<f32>;

    if (horizontal_first) {
        let mid_x = (p0.x + p3.x) * 0.5;
        corner1 = vec2(mid_x, p0.y);
        corner2 = vec2(mid_x, p3.y);
    } else {
        let mid_y = (p0.y + p3.y) * 0.5;
        corner1 = vec2(p0.x, mid_y);
        corner2 = vec2(p3.x, mid_y);
    }

    let bbox_min = min(min(p0, p3), min(corner1, corner2));
    let bbox_max = max(max(p0, p3), max(corner1, corner2));
    return vec4(bbox_min, bbox_max);
}

// SmoothStep path SDF (orthogonal with rounded corners)
// Uses circular arcs at corners for smooth transitions
fn sdSmoothStepPath(pos: vec2<f32>, p0: vec2<f32>, p3: vec2<f32>, dir_from: vec2<f32>, dir_to: vec2<f32>, corner_radius: f32) -> vec2<f32> {
    let horizontal_first = abs(dir_from.x) > 0.5;

    var corner1: vec2<f32>;
    var corner2: vec2<f32>;

    if (horizontal_first) {
        let mid_x = (p0.x + p3.x) * 0.5;
        corner1 = vec2(mid_x, p0.y);
        corner2 = vec2(mid_x, p3.y);
    } else {
        let mid_y = (p0.y + p3.y) * 0.5;
        corner1 = vec2(p0.x, mid_y);
        corner2 = vec2(p3.x, mid_y);
    }

    // Calculate segment lengths
    let len1 = length(corner1 - p0);
    let len2 = length(corner2 - corner1);
    let len3 = length(p3 - corner2);

    // Clamp radius to not exceed half the shortest segment
    let max_radius = min(min(len1, len3), len2 * 0.5) * 0.9;
    let r = min(corner_radius, max_radius);

    // Arc length for 90-degree turn
    let arc_len = r * 1.5707963;  // PI/2

    // Total path length
    let total_len = (len1 - r) + arc_len + (len2 - 2.0 * r) + arc_len + (len3 - r);

    if (total_len < 0.001) {
        return vec2(length(pos - p0), 0.0);
    }

    // Compute arc centers and adjusted segment endpoints
    let dir1 = normalize(corner1 - p0);
    let dir2 = normalize(corner2 - corner1);
    let dir3 = normalize(p3 - corner2);

    // First arc center (at corner1)
    let arc1_center = corner1 - dir1 * r + vec2(-dir2.y, dir2.x) * r * sign(dir1.x * dir2.y - dir1.y * dir2.x);
    // Second arc center (at corner2)
    let arc2_center = corner2 - dir2 * r + vec2(-dir3.y, dir3.x) * r * sign(dir2.x * dir3.y - dir2.y * dir3.x);

    // Adjusted segment endpoints
    let seg1_end = corner1 - dir1 * r;
    let seg2_start = corner1 + dir2 * r;
    let seg2_end = corner2 - dir2 * r;
    let seg3_start = corner2 + dir3 * r;

    var min_dist = 1e10;
    var best_t = 0.0;
    var cumulative = 0.0;

    // Segment 1: p0 to seg1_end
    let seg1_len = len1 - r;
    if (seg1_len > 0.001) {
        let result1 = sdStraightLine(pos, p0, seg1_end);
        if (result1.x < min_dist) {
            min_dist = result1.x;
            best_t = (cumulative + result1.y * seg1_len) / total_len;
        }
    }
    cumulative = cumulative + seg1_len;

    // Arc 1 at corner1
    let dist_to_arc1 = abs(length(pos - arc1_center) - r);
    // Check if we're in the arc's angular range
    let to_pos1 = pos - arc1_center;
    let arc1_valid = (dot(to_pos1, dir1) <= 0.0 || length(to_pos1) < r * 0.5) &&
                     (dot(to_pos1, -dir2) <= 0.0 || length(to_pos1) < r * 0.5);
    if (dist_to_arc1 < min_dist) {
        // Approximate t along arc
        let angle_to_pos = atan2(to_pos1.y, to_pos1.x);
        let angle_start = atan2(-dir1.y, -dir1.x);
        var angle_diff = angle_to_pos - angle_start;
        if (angle_diff < 0.0) { angle_diff = angle_diff + 6.2831853; }
        if (angle_diff > 3.1415926) { angle_diff = 6.2831853 - angle_diff; }
        let arc_t = clamp(angle_diff / 1.5707963, 0.0, 1.0);

        min_dist = dist_to_arc1;
        best_t = (cumulative + arc_t * arc_len) / total_len;
    }
    cumulative = cumulative + arc_len;

    // Segment 2: seg2_start to seg2_end
    let seg2_len = len2 - 2.0 * r;
    if (seg2_len > 0.001) {
        let result2 = sdStraightLine(pos, seg2_start, seg2_end);
        if (result2.x < min_dist) {
            min_dist = result2.x;
            best_t = (cumulative + result2.y * seg2_len) / total_len;
        }
    }
    cumulative = cumulative + seg2_len;

    // Arc 2 at corner2
    let dist_to_arc2 = abs(length(pos - arc2_center) - r);
    if (dist_to_arc2 < min_dist) {
        let to_pos2 = pos - arc2_center;
        let angle_to_pos2 = atan2(to_pos2.y, to_pos2.x);
        let angle_start2 = atan2(-dir2.y, -dir2.x);
        var angle_diff2 = angle_to_pos2 - angle_start2;
        if (angle_diff2 < 0.0) { angle_diff2 = angle_diff2 + 6.2831853; }
        if (angle_diff2 > 3.1415926) { angle_diff2 = 6.2831853 - angle_diff2; }
        let arc_t2 = clamp(angle_diff2 / 1.5707963, 0.0, 1.0);

        min_dist = dist_to_arc2;
        best_t = (cumulative + arc_t2 * arc_len) / total_len;
    }
    cumulative = cumulative + arc_len;

    // Segment 3: seg3_start to p3
    let seg3_len = len3 - r;
    if (seg3_len > 0.001) {
        let result3 = sdStraightLine(pos, seg3_start, p3);
        if (result3.x < min_dist) {
            min_dist = result3.x;
            best_t = (cumulative + result3.y * seg3_len) / total_len;
        }
    }

    return vec2(min_dist, best_t);
}

// Compute smooth step path length
fn smoothStepPathLength(p0: vec2<f32>, p3: vec2<f32>, dir_from: vec2<f32>, corner_radius: f32) -> f32 {
    let horizontal_first = abs(dir_from.x) > 0.5;

    var len1: f32;
    var len2: f32;
    var len3: f32;

    if (horizontal_first) {
        let mid_x = (p0.x + p3.x) * 0.5;
        len1 = abs(mid_x - p0.x);
        len2 = abs(p3.y - p0.y);
        len3 = abs(p3.x - mid_x);
    } else {
        let mid_y = (p0.y + p3.y) * 0.5;
        len1 = abs(mid_y - p0.y);
        len2 = abs(p3.x - p0.x);
        len3 = abs(p3.y - mid_y);
    }

    let max_radius = min(min(len1, len3), len2 * 0.5) * 0.9;
    let r = min(corner_radius, max_radius);
    let arc_len = r * 1.5707963;

    return (len1 - r) + arc_len + (len2 - 2.0 * r) + arc_len + (len3 - r);
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn get_pin_direction(side: u32) -> vec2<f32> {
    switch (side) {
        case 0u: { return vec2<f32>(-1.0, 0.0); }
        case 1u: { return vec2<f32>(1.0, 0.0); }
        case 2u: { return vec2<f32>(0.0, -1.0); }
        case 3u: { return vec2<f32>(0.0, 1.0); }
        default: { return vec2<f32>(1.0, 0.0); }
    }
}

// Valid drop target is now computed in Rust and stored in pin.flags
fn is_valid_drop_target(pin: Pin) -> bool {
    return (pin.flags & PIN_FLAG_VALID_TARGET) != 0u;
}

// Convert world position to clip space, accounting for widget bounds offset
fn world_to_clip(world_pos: vec2<f32>) -> vec4<f32> {
    let screen = (world_pos + uniforms.camera_position) * uniforms.camera_zoom * uniforms.os_scale_factor;
    // Transform relative to widget bounds, not full viewport
    let ndc = (screen - uniforms.bounds_origin) / uniforms.bounds_size * 2.0 - 1.0;
    return vec4(ndc.x, -ndc.y, 0.0, 1.0);
}

fn grid_pattern(uv: vec2<f32>, minor_spacing: f32, major_spacing: f32, zoom: f32) -> f32 {
    let coord_minor = abs(uv % minor_spacing);
    let dist_minor_x = min(coord_minor.x, minor_spacing - coord_minor.x);
    let dist_minor_y = min(coord_minor.y, minor_spacing - coord_minor.y);

    let coord_major = abs(uv % major_spacing);
    let dist_major_x = min(coord_major.x, major_spacing - coord_major.x);
    let dist_major_y = min(coord_major.y, major_spacing - coord_major.y);

    let minor_width = 1.0;
    let major_width = 2.0;

    var intensity = 0.0;

    if (dist_major_x < major_width || dist_major_y < major_width) {
        intensity = 0.7;
    }
    else if (dist_minor_x < minor_width || dist_minor_y < minor_width) {
        intensity = 0.35;
    }

    return intensity;
}

// ============================================================================
// BACKGROUND SHADER (Fullscreen grid)
// ============================================================================

@vertex
fn vs_background(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // Fullscreen triangle in NDC space relative to widget bounds
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_background(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    // Offset frag_coord by bounds origin to get position relative to widget
    let local_coord = frag_coord.xy - uniforms.bounds_origin;
    let uv = (local_coord / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;

    let grid_intensity = grid_pattern(uv, 100.0, 1000.0, uniforms.camera_zoom);
    let col = mix(uniforms.background_color.xyz, uniforms.grid_color.rgb, grid_intensity);

    return vec4(col, 1.0);
}

// ============================================================================
// EDGE INSTANCE SHADER
// ============================================================================

@vertex
fn vs_edge(@builtin(instance_index) instance: u32,
           @builtin(vertex_index) vertex: u32) -> EdgeVertexOutput {
    let edge = edges[instance];

    // Use resolved positions directly (no more index lookups)
    let dir_from = get_pin_direction(edge.start_direction);
    let dir_to = get_pin_direction(edge.end_direction);
    let seg_len = 80.0;
    let p0 = edge.start;
    let p1 = p0 + dir_from * seg_len;
    let p3 = edge.end;
    let p2 = p3 + dir_to * seg_len;

    var bbox_min: vec2<f32>;
    var bbox_max: vec2<f32>;

    switch (edge.edge_type) {
        case 1u: {  // Straight
            bbox_min = min(p0, p3);
            bbox_max = max(p0, p3);
        }
        case 2u, 3u: {  // SmoothStep or Step
            let bounds = stepPathBounds(p0, p3, dir_from);
            bbox_min = bounds.xy;
            bbox_max = bounds.zw;
        }
        default: {  // Bezier (0) or fallback
            bbox_min = min(min(p0, p1), min(p2, p3));
            bbox_max = max(max(p0, p1), max(p2, p3));
        }
    }

    // Use actual edge thickness (world space) plus AA padding (screen space converted to world)
    let edge_padding = edge.thickness + 2.0 / uniforms.camera_zoom;
    let bbox = vec4(bbox_min - vec2(edge_padding), bbox_max + vec2(edge_padding));

    let corners = array<vec2<f32>, 4>(
        bbox.xy,
        vec2(bbox.z, bbox.y),
        vec2(bbox.x, bbox.w),
        bbox.zw
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let clip = world_to_clip(world_pos);

    return EdgeVertexOutput(clip, world_pos, instance);
}

@fragment
fn fs_edge(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    let edge = edges[in.instance_id];

    // Use resolved positions directly (no more index lookups)
    let dir_from = get_pin_direction(edge.start_direction);
    let dir_to = get_pin_direction(edge.end_direction);
    let seg_len = 80.0;
    let p0 = edge.start;
    let p1 = p0 + dir_from * seg_len;
    let p3 = edge.end;
    let p2 = p3 + dir_to * seg_len;

    // Calculate distance and t parameter based on edge type
    var dist_and_t: vec2<f32>;
    var curve_length: f32;

    switch (edge.edge_type) {
        case 1u: {  // Straight
            dist_and_t = sdStraightLine(in.world_uv, p0, p3);
            curve_length = length(p3 - p0);
        }
        case 2u: {  // SmoothStep (rounded orthogonal corners)
            let corner_radius = 15.0;  // Default corner radius
            dist_and_t = sdSmoothStepPath(in.world_uv, p0, p3, dir_from, dir_to, corner_radius);
            curve_length = smoothStepPathLength(p0, p3, dir_from, corner_radius);
        }
        case 3u: {  // Step (sharp orthogonal corners)
            dist_and_t = sdStepPath(in.world_uv, p0, p3, dir_from, dir_to);
            curve_length = stepPathLength(p0, p3, dir_from);
        }
        default: {  // Bezier (0) or fallback
            dist_and_t = sdCubicBezierWithT(in.world_uv, p0, p1, p2, p3);
            curve_length = estimateBezierLength(p0, p1, p2, p3);
        }
    }

    let dist = dist_and_t.x;
    let t = dist_and_t.y;

    // Gradient from start_color to end_color based on position along edge
    var edge_color = mix(edge.start_color.rgb, edge.end_color.rgb, t);

    let edge_thickness = edge.thickness;
    let aa = 1.0 / uniforms.camera_zoom;

    // Base alpha from distance
    var alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + aa, dist);

    // === DASH PATTERN ===
    if (edge.dash_length > 0.0) {
        let pattern_size = edge.dash_length + edge.gap_length;

        // Position along curve in world units
        var curve_pos = t * curve_length;

        // Animation: shift pattern with time (bit 0 = animated dash)
        if ((edge.flags & 1u) != 0u) {
            curve_pos = curve_pos + uniforms.time * edge.flow_speed;
        }

        // Dash or gap?
        let pattern_t = fract(curve_pos / pattern_size);
        let dash_ratio = edge.dash_length / pattern_size;

        if (pattern_t > dash_ratio) {
            // In gap - transparent
            alpha = 0.0;
        }
    }

    // === GLOW EFFECT (bit 1) ===
    if ((edge.flags & 2u) != 0u) {
        let flow_t = fract(t - uniforms.time * 0.5);
        let glow = smoothstep(0.0, 0.2, flow_t) * smoothstep(0.5, 0.3, flow_t);
        // Additive glow
        return vec4(edge_color + vec3(glow * 0.3), alpha);
    }

    // === PULSE EFFECT (bit 2) ===
    if ((edge.flags & 4u) != 0u) {
        let pulse = sin(uniforms.time * 3.0) * 0.5 + 0.5;
        alpha = alpha * (0.5 + pulse * 0.5);
    }

    // === PARTICLES EFFECT (bit 3) ===
    // Creates multiple flowing dots along the edge
    if ((edge.flags & 8u) != 0u) {
        // Check if this is a pending cut (during edge cutting mode)
        // If dragging type is edge cutting (7), show red pulsing
        if (uniforms.dragging == 7u) {
            // Red pulsing for pending cut
            let pulse = sin(uniforms.time * 6.0) * 0.3 + 0.7;
            return vec4(1.0, 0.2, 0.2, alpha * pulse);
        }
        // Otherwise show particle flow effect
        let num_particles = 5.0;
        let particle_spacing = 1.0 / num_particles;
        var particle_intensity = 0.0;

        for (var i = 0.0; i < num_particles; i = i + 1.0) {
            let particle_t = fract(t * num_particles - i - uniforms.time * edge.flow_speed * 0.01);
            let particle = smoothstep(0.0, 0.1, particle_t) * smoothstep(0.3, 0.1, particle_t);
            particle_intensity = max(particle_intensity, particle);
        }

        // Brighter particles on top of edge
        edge_color = mix(edge_color, vec3(1.0), particle_intensity * 0.5);
        alpha = alpha * (0.7 + particle_intensity * 0.3);
    }

    // === RAINBOW EFFECT (bit 4) ===
    // HSV-based color cycling along the edge
    if ((edge.flags & 16u) != 0u) {
        // Hue shifts along the edge and with time
        let hue = fract(t + uniforms.time * 0.1);

        // HSV to RGB conversion (simplified)
        let h = hue * 6.0;
        let hi = floor(h);
        let f = h - hi;
        let q = 1.0 - f;
        let ti = u32(hi) % 6u;

        var rgb: vec3<f32>;
        switch (ti) {
            case 0u: { rgb = vec3(1.0, f, 0.0); }
            case 1u: { rgb = vec3(q, 1.0, 0.0); }
            case 2u: { rgb = vec3(0.0, 1.0, f); }
            case 3u: { rgb = vec3(0.0, q, 1.0); }
            case 4u: { rgb = vec3(f, 0.0, 1.0); }
            default: { rgb = vec3(1.0, 0.0, q); }
        }

        // Mix rainbow with original edge color
        edge_color = mix(edge_color, rgb, 0.7);
    }

    return vec4(edge_color, alpha);
}

// ============================================================================
// NODE INSTANCE SHADER
// ============================================================================

@vertex
fn vs_node(@builtin(instance_index) instance: u32,
           @builtin(vertex_index) vertex: u32) -> NodeVertexOutput {
    let node = nodes[instance];

    // Border in world space + AA padding in screen space
    let border_padding = node.border_width + 2.0 / uniforms.camera_zoom;

    // Shadow extends the bounding box by offset + blur radius
    let shadow_extend = max(
        abs(node.shadow_offset) + vec2(node.shadow_blur),
        vec2(0.0)
    );

    // Hover glow adds extra padding
    let is_hovered = (node.flags & NODE_FLAG_HOVERED) != 0u;
    var glow_padding = 0.0;
    if (is_hovered) {
        glow_padding = uniforms.hover_glow_radius;
    }

    let total_padding = border_padding + glow_padding;
    let bbox_min = node.position - vec2(total_padding) - max(shadow_extend, vec2(0.0)) + min(node.shadow_offset, vec2(0.0));
    let bbox_max = node.position + node.size + vec2(total_padding) + shadow_extend;

    let corners = array<vec2<f32>, 4>(
        bbox_min,
        vec2(bbox_max.x, bbox_min.y),
        vec2(bbox_min.x, bbox_max.y),
        bbox_max
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let clip = world_to_clip(world_pos);

    return NodeVertexOutput(clip, world_pos, instance);
}

// Shared node SDF computation
fn compute_node_sdf(in: NodeVertexOutput) -> f32 {
    let node = nodes[in.instance_id];
    let node_half_size = node.size * 0.5;
    let node_center = in.world_uv - (node.position + node_half_size);

    var d = sd_rounded_box(node_center, node_half_size, vec4(node.corner_radius));

    for (var j = 0u; j < node.pin_count; j++) {
        let pin = pins[node.pin_start + j];
        let pin_center = in.world_uv - pin.position;
        let pin_d = sd_circle(pin_center, pin.radius);
        d = max(d, -pin_d);
    }

    return d;
}

// Background layer: renders shadow, hover glow, and fill (no border)
@fragment
fn fs_node_fill(in: NodeVertexOutput) -> @location(0) vec4<f32> {
    let node = nodes[in.instance_id];
    let d = compute_node_sdf(in);

    let aa = 0.5 / uniforms.camera_zoom;
    let node_opacity = node.opacity;
    let is_hovered = (node.flags & NODE_FLAG_HOVERED) != 0u;

    var col = vec3(0.0);
    var alpha = 0.0;

    // Render hover glow (subtle outer glow when hovered)
    if (is_hovered && d > 0.0) {
        let glow_alpha = (1.0 - smoothstep(0.0, uniforms.hover_glow_radius, d)) * 0.3 * node_opacity;
        if (glow_alpha > alpha) {
            col = uniforms.hover_glow_color.rgb;
            alpha = glow_alpha;
        }
    }

    // Render shadow (if enabled)
    let node_half_size = node.size * 0.5;
    let node_center = in.world_uv - (node.position + node_half_size);
    if (node.shadow_color.a > 0.0 && node.shadow_blur > 0.0) {
        let shadow_center = node_center - node.shadow_offset;
        let shadow_d = sd_rounded_box(shadow_center, node_half_size, vec4(node.corner_radius));
        let shadow_softness = node.shadow_blur;
        let shadow_alpha = (1.0 - smoothstep(-shadow_softness * 0.5, shadow_softness, shadow_d))
                           * node.shadow_color.a * node_opacity;

        if (shadow_alpha > alpha) {
            col = node.shadow_color.xyz;
            alpha = shadow_alpha;
        }
    }

    // Render fill for entire node interior
    if (d < 0.0) {
        col = node.fill_color.xyz;
        alpha = node_opacity;
    } else if (d < aa) {
        col = node.fill_color.xyz;
        alpha = (1.0 - smoothstep(0.0, aa, d)) * node_opacity;
    }

    return vec4(col, alpha);
}

// Foreground layer: renders border only
@fragment
fn fs_node(in: NodeVertexOutput) -> @location(0) vec4<f32> {
    let node = nodes[in.instance_id];
    let d = compute_node_sdf(in);

    let aa = 0.5 / uniforms.camera_zoom;
    let border_width = node.border_width;
    let node_opacity = node.opacity;

    var col = vec3(0.0);
    var alpha = 0.0;

    // Render border only (the ring between d=0 and d=-border_width)
    if (d < 0.0 && d > -border_width) {
        col = node.border_color.xyz;
        alpha = node_opacity;
    } else if (d >= 0.0 && d < aa) {
        // Anti-aliased outer edge of border
        col = node.border_color.xyz;
        alpha = (1.0 - smoothstep(0.0, aa, d)) * node_opacity;
    }

    return vec4(col, alpha);
}

// ============================================================================
// PIN INSTANCE SHADER
// ============================================================================

@vertex
fn vs_pin(@builtin(instance_index) instance: u32,
          @builtin(vertex_index) vertex: u32) -> PinVertexOutput {
    let pin = pins[instance];

    var node_id = 0u;
    var pin_index = 0u;

    for (var i = 0u; i < uniforms.num_nodes; i++) {
        let node = nodes[i];
        if (instance >= node.pin_start && instance < node.pin_start + node.pin_count) {
            node_id = i;
            pin_index = instance - node.pin_start;
            break;
        }
    }

    var is_valid_target = false;
    if (uniforms.dragging == 3u || uniforms.dragging == 4u) {
        is_valid_target = is_valid_drop_target(pin);
    }

    var anim_scale = 1.0;
    if (is_valid_target) {
        let pulse = sin(uniforms.time * 6.0) * 0.5 + 0.5;
        anim_scale = 1.0 + pulse * 0.5;
    }

    let indicator_radius = pin.radius * 0.4 * anim_scale;
    let padding = indicator_radius + 2.0 / uniforms.camera_zoom;

    let bbox_min = pin.position - vec2(padding);
    let bbox_max = pin.position + vec2(padding);

    let corners = array<vec2<f32>, 4>(
        bbox_min,
        vec2(bbox_max.x, bbox_min.y),
        vec2(bbox_min.x, bbox_max.y),
        bbox_max
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let clip = world_to_clip(world_pos);

    return PinVertexOutput(clip, world_pos, instance, node_id, pin_index);
}

// Helper to get SDF for a pin shape
fn sd_pin_shape(p: vec2<f32>, r: f32, shape: u32, side: u32) -> f32 {
    switch (shape) {
        case 1u: {  // Square
            return sd_box(p, r * 0.7);
        }
        case 2u: {  // Diamond
            return sd_diamond(p, r * 0.8);
        }
        case 3u: {  // Triangle - point in direction based on side
            var rotated_p = p;
            // Rotate based on pin side: Left(0)->point left, Right(1)->point right, etc
            switch (side) {
                case 0u: { rotated_p = vec2(-p.x, p.y); }  // Left - point left
                case 1u: { rotated_p = p; }                 // Right - point right
                case 2u: { rotated_p = vec2(p.y, -p.x); }  // Top - point up
                case 3u: { rotated_p = vec2(-p.y, p.x); }  // Bottom - point down
                default: { rotated_p = p; }                 // Row - point right
            }
            return sd_triangle(rotated_p, r * 0.6);
        }
        default: {  // Circle (0) or fallback
            return sd_circle(p, r);
        }
    }
}

@fragment
fn fs_pin(in: PinVertexOutput) -> @location(0) vec4<f32> {
    let pin = pins[in.instance_id];
    let pin_center = in.world_uv - pin.position;

    var is_valid_target = false;
    if (uniforms.dragging == 3u || uniforms.dragging == 4u) {
        is_valid_target = is_valid_drop_target(pin);
    }

    var anim_scale = 1.0;
    if (is_valid_target) {
        let pulse = sin(uniforms.time * 6.0) * 0.5 + 0.5;
        anim_scale = 1.0 + pulse * 0.5;
    }

    let indicator_radius = pin.radius * 0.4 * anim_scale;
    let aa = 0.5 / uniforms.camera_zoom;

    var fill_alpha = 0.0;
    var border_alpha = 0.0;

    // Get distance to pin shape
    let d = sd_pin_shape(pin_center, indicator_radius, pin.shape, pin.side);

    // For input pins (direction 0), render as ring/hollow shape
    if (pin.direction == 0u) {
        let ring_thickness = indicator_radius * 0.4;
        let inner_d = sd_pin_shape(pin_center, indicator_radius - ring_thickness, pin.shape, pin.side);
        fill_alpha = (1.0 - smoothstep(0.0, aa, d)) * smoothstep(0.0, aa, inner_d);
    } else {
        // Output or bidirectional pins are filled
        fill_alpha = 1.0 - smoothstep(0.0, aa, d);
    }

    // Render border if border_color has alpha > 0
    if (pin.border_color.w > 0.0 && pin.border_width > 0.0) {
        let border_outer = d;
        let border_inner = d + pin.border_width;
        border_alpha = (1.0 - smoothstep(0.0, aa, border_outer)) * smoothstep(-aa, 0.0, border_inner);
    }

    // Composite: border behind, fill on top
    let fill_color = vec4(pin.color.xyz, fill_alpha * pin.color.w);
    let border_color = vec4(pin.border_color.xyz, border_alpha * pin.border_color.w);

    // Alpha blend: fill over border
    let result_alpha = fill_color.w + border_color.w * (1.0 - fill_color.w);
    if (result_alpha < 0.001) {
        return vec4(0.0);
    }
    let result_rgb = (fill_color.xyz * fill_color.w + border_color.xyz * border_color.w * (1.0 - fill_color.w)) / result_alpha;

    return vec4(result_rgb, result_alpha);
}

// ============================================================================
// DRAGGING SHADER (Edge dragging, Box Selection, Edge Cutting)
// Dragging types: 3=Edge, 4=EdgeOver, 5=BoxSelect, 7=EdgeCutting
// ============================================================================

@vertex
fn vs_dragging(@builtin(vertex_index) vertex: u32) -> EdgeVertexOutput {
    // Build a bounding box for the dragging operation
    var p0 = vec2(0.0);
    var p3 = vec2(0.0);

    // Edge dragging (3, 4)
    if (uniforms.dragging == 3u || uniforms.dragging == 4u) {
        let from_node = nodes[uniforms.dragging_edge_from_node];
        let from_pin = pins[from_node.pin_start + uniforms.dragging_edge_from_pin];
        p0 = from_pin.position;

        if (uniforms.dragging == 4u) {
            let to_node = nodes[uniforms.dragging_edge_to_node];
            let to_pin = pins[to_node.pin_start + uniforms.dragging_edge_to_pin];
            p3 = to_pin.position;
        } else {
            p3 = uniforms.cursor_position;
        }
    }
    // BoxSelect (5) or EdgeCutting (7): from_origin to cursor
    else if (uniforms.dragging == 5u || uniforms.dragging == 7u) {
        p0 = uniforms.dragging_edge_from_origin;
        p3 = uniforms.cursor_position;
    }

    // EdgeCutting (7) uses world-space padding for thick cutting line
    // Other dragging modes use screen-space padding
    var padding = 100.0 / uniforms.camera_zoom;
    if (uniforms.dragging == 7u) {
        padding = 50.0;  // World-space padding for cutting line
    }
    let bbox_min = min(p0, p3) - vec2(padding);
    let bbox_max = max(p0, p3) + vec2(padding);

    let corners = array<vec2<f32>, 4>(
        bbox_min,
        vec2(bbox_max.x, bbox_min.y),
        vec2(bbox_min.x, bbox_max.y),
        bbox_max
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let clip = world_to_clip(world_pos);

    return EdgeVertexOutput(clip, world_pos, 0u);
}

@fragment
fn fs_dragging(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    let aa = 1.0 / uniforms.camera_zoom;

    // === BoxSelect (5): Draw selection rectangle ===
    if (uniforms.dragging == 5u) {
        let box_min = min(uniforms.dragging_edge_from_origin, uniforms.cursor_position);
        let box_max = max(uniforms.dragging_edge_from_origin, uniforms.cursor_position);

        let p = in.world_uv;

        // Distance to rectangle edges
        let dx = max(box_min.x - p.x, p.x - box_max.x);
        let dy = max(box_min.y - p.y, p.y - box_max.y);
        let dist_outside = length(max(vec2(dx, dy), vec2(0.0)));
        let dist_inside = min(max(dx, dy), 0.0);
        let dist = dist_outside + dist_inside;

        // Border
        let border_width = 1.5 / uniforms.camera_zoom;
        let border_alpha = 1.0 - smoothstep(-border_width, -border_width + aa, dist);
        let border_color = vec4(uniforms.selection_box_color.rgb, 0.8);

        // Fill (inside the rectangle)
        let fill_alpha = 1.0 - smoothstep(-aa, 0.0, dist);
        let fill_color = vec4(uniforms.selection_box_color.rgb, 0.15);

        // Combine: fill inside, border on edge
        if (dist < 0.0) {
            // Inside: show fill + border near edge
            let edge_dist = -dist;
            if (edge_dist < border_width + aa) {
                return vec4(border_color.rgb, border_alpha * border_color.a);
            }
            return vec4(fill_color.rgb, fill_alpha * fill_color.a);
        }
        return vec4(0.0);
    }

    // === EdgeCutting (7): Draw cutting line ===
    if (uniforms.dragging == 7u) {
        let p0 = uniforms.dragging_edge_from_origin;
        let p1 = uniforms.cursor_position;
        let p = in.world_uv;

        // Distance to line segment
        let pa = p - p0;
        let ba = p1 - p0;
        let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
        let dist = length(pa - ba * h);

        let line_width = 3.0;  // World space - scales with zoom
        let alpha = 1.0 - smoothstep(line_width, line_width + aa, dist);

        // Edge cutting line
        return vec4(uniforms.edge_cutting_color.rgb, alpha * 0.8);
    }

    // === Edge dragging (3, 4) ===
    if (uniforms.dragging != 3u && uniforms.dragging != 4u) {
        return vec4(0.0);
    }

    let from_node = nodes[uniforms.dragging_edge_from_node];
    let from_pin = pins[from_node.pin_start + uniforms.dragging_edge_from_pin];

    let dir_from = get_pin_direction(from_pin.side);
    let seg_len = 80.0;
    let p0 = from_pin.position;
    let p1 = p0 + dir_from * seg_len;

    var p3 = uniforms.cursor_position;
    var dir_to = -dir_from;

    if (uniforms.dragging == 4u) {
        let to_node = nodes[uniforms.dragging_edge_to_node];
        let to_pin = pins[to_node.pin_start + uniforms.dragging_edge_to_pin];
        p3 = to_pin.position;
        dir_to = get_pin_direction(to_pin.side);
    }

    let p2 = p3 + dir_to * seg_len;

    // Compute distance and t parameter for gradient
    let dist_and_t = sdCubicBezierWithT(in.world_uv, p0, p1, p2, p3);
    let dist = dist_and_t.x;
    let t = dist_and_t.y;

    // Gradient from start_color (pin end) to end_color (cursor/target end)
    let edge_color = mix(
        uniforms.dragging_edge_start_color.rgb,
        uniforms.dragging_edge_end_color.rgb,
        t
    );

    let edge_thickness = uniforms.edge_thickness;  // From resolved edge defaults
    let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + aa, dist);

    return vec4(edge_color, alpha);
}

// ============================================================================
// LEGACY SHADERS (Keep for compatibility, but won't be used)
// ============================================================================

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_foreground(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    // Legacy - use fs_dragging instead
    return vec4(0.0);
}
