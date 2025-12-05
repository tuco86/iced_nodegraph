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

    viewport_size: vec2<f32>,
};

struct Node {
    position: vec2<f32>,
    size: vec2<f32>,
    corner_radius: f32,
    border_width: f32,
    opacity: f32,
    pin_start: u32,
    pin_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    fill_color: vec4<f32>,
    border_color: vec4<f32>,
};

struct Pin {
    position: vec2<f32>,
    side: u32,
    radius: f32,
    color: vec4<f32>,
    direction: u32,
    flags: u32,
    _pad0: u32,
    _pad1: u32,
};

struct Edge {
    from_node: u32,
    from_pin: u32,
    to_node: u32,
    to_pin: u32,
    color: vec4<f32>,
    thickness: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

fn sdCubicBezier(pos: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
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

    min_dist = min(min_dist, dot2(pos - p3));

    return sqrt(min_dist);
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

fn select_edge_color(from_pin: Pin, to_pin: Pin) -> vec3<f32> {
    if (from_pin.direction == 1u) {
        return from_pin.color.xyz;
    } else if (to_pin.direction == 1u) {
        return to_pin.color.xyz;
    } else if (from_pin.direction == 0u) {
        return from_pin.color.xyz;
    } else if (to_pin.direction == 0u) {
        return to_pin.color.xyz;
    } else {
        return from_pin.color.xyz;
    }
}

fn check_valid_drop_target(node_id: u32, pin_index: u32) -> bool {
    let from_node = nodes[uniforms.dragging_edge_from_node];
    let from_pin = pins[from_node.pin_start + uniforms.dragging_edge_from_pin];
    let pin = pins[nodes[node_id].pin_start + pin_index];

    var direction_valid = false;
    if (from_pin.direction == 2u || pin.direction == 2u) {
        direction_valid = true;
    } else if ((from_pin.direction == 1u && pin.direction == 0u) ||
               (from_pin.direction == 0u && pin.direction == 1u)) {
        direction_valid = true;
    }

    let color_diff = length(from_pin.color.xyz - pin.color.xyz);
    let type_valid = color_diff < 0.1;

    let is_source = (node_id == uniforms.dragging_edge_from_node) &&
                    (pin_index == uniforms.dragging_edge_from_pin);

    return direction_valid && type_valid && !is_source;
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
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_background(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;

    let grid_intensity = grid_pattern(uv, 100.0, 1000.0, uniforms.camera_zoom);
    let grid_color = uniforms.border_color.xyz * 1.3;
    let col = mix(uniforms.background_color.xyz, grid_color, grid_intensity);

    return vec4(col, 1.0);
}

// ============================================================================
// EDGE INSTANCE SHADER
// ============================================================================

@vertex
fn vs_edge(@builtin(instance_index) instance: u32,
           @builtin(vertex_index) vertex: u32) -> EdgeVertexOutput {
    let edge = edges[instance];
    let from_node = nodes[edge.from_node];
    let from_pin = pins[from_node.pin_start + edge.from_pin];
    let to_node = nodes[edge.to_node];
    let to_pin = pins[to_node.pin_start + edge.to_pin];

    let dir_from = get_pin_direction(from_pin.side);
    let dir_to = get_pin_direction(to_pin.side);
    let seg_len = 80.0;
    let p0 = from_pin.position;
    let p1 = p0 + dir_from * seg_len;
    let p3 = to_pin.position;
    let p2 = p3 + dir_to * seg_len;

    let bbox_min = min(min(p0, p1), min(p2, p3));
    let bbox_max = max(max(p0, p1), max(p2, p3));
    let edge_thickness = 4.0 / uniforms.camera_zoom;
    let bbox = vec4(bbox_min - vec2(edge_thickness), bbox_max + vec2(edge_thickness));

    let corners = array<vec2<f32>, 4>(
        bbox.xy,
        vec2(bbox.z, bbox.y),
        vec2(bbox.x, bbox.w),
        bbox.zw
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let screen = (world_pos + uniforms.camera_position) * uniforms.camera_zoom * uniforms.os_scale_factor;
    let ndc = screen / uniforms.viewport_size * 2.0 - 1.0;
    let clip = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    return EdgeVertexOutput(clip, world_pos, instance);
}

@fragment
fn fs_edge(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    let edge = edges[in.instance_id];
    let from_node = nodes[edge.from_node];
    let from_pin = pins[from_node.pin_start + edge.from_pin];
    let to_node = nodes[edge.to_node];
    let to_pin = pins[to_node.pin_start + edge.to_pin];

    let dir_from = get_pin_direction(from_pin.side);
    let dir_to = get_pin_direction(to_pin.side);
    let seg_len = 80.0;
    let p0 = from_pin.position;
    let p1 = p0 + dir_from * seg_len;
    let p3 = to_pin.position;
    let p2 = p3 + dir_to * seg_len;

    let edge_color = select_edge_color(from_pin, to_pin);

    let dist = sdCubicBezier(in.world_uv, p0, p1, p2, p3);
    let edge_thickness = 2.0 / uniforms.camera_zoom;
    let aa = 1.0 / uniforms.camera_zoom;

    let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + aa, dist);

    return vec4(edge_color, alpha);
}

// ============================================================================
// NODE INSTANCE SHADER
// ============================================================================

@vertex
fn vs_node(@builtin(instance_index) instance: u32,
           @builtin(vertex_index) vertex: u32) -> NodeVertexOutput {
    let node = nodes[instance];

    // Use per-node border_width for bounding box padding
    let border_padding = max(node.border_width, 2.0) / uniforms.camera_zoom;
    let bbox_min = node.position - vec2(border_padding);
    let bbox_max = node.position + node.size + vec2(border_padding);

    let corners = array<vec2<f32>, 4>(
        bbox_min,
        vec2(bbox_max.x, bbox_min.y),
        vec2(bbox_min.x, bbox_max.y),
        bbox_max
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let screen = (world_pos + uniforms.camera_position) * uniforms.camera_zoom * uniforms.os_scale_factor;
    let ndc = screen / uniforms.viewport_size * 2.0 - 1.0;
    let clip = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    return NodeVertexOutput(clip, world_pos, instance);
}

@fragment
fn fs_node(in: NodeVertexOutput) -> @location(0) vec4<f32> {
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

    let aa = 0.5 / uniforms.camera_zoom;
    // Use per-node border_width and opacity
    let border_width = node.border_width / uniforms.camera_zoom;
    let node_opacity = node.opacity;

    var col = vec3(0.0);
    var alpha = 0.0;

    if (d < 0.0) {
        if (d > -border_width) {
            // Use per-node border_color
            col = node.border_color.xyz;
        } else {
            // Use per-node fill_color
            col = node.fill_color.xyz;
        }
        alpha = node_opacity;
    } else if (d < aa) {
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
    if (uniforms.dragging == 3u) {
        is_valid_target = check_valid_drop_target(node_id, pin_index);
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

    let screen = (world_pos + uniforms.camera_position) * uniforms.camera_zoom * uniforms.os_scale_factor;
    let ndc = screen / uniforms.viewport_size * 2.0 - 1.0;
    let clip = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    return PinVertexOutput(clip, world_pos, instance, node_id, pin_index);
}

@fragment
fn fs_pin(in: PinVertexOutput) -> @location(0) vec4<f32> {
    let pin = pins[in.instance_id];
    let pin_center = in.world_uv - pin.position;

    var is_valid_target = false;
    if (uniforms.dragging == 3u) {
        is_valid_target = check_valid_drop_target(in.node_id, in.pin_index);
    }

    var anim_scale = 1.0;
    if (is_valid_target) {
        let pulse = sin(uniforms.time * 6.0) * 0.5 + 0.5;
        anim_scale = 1.0 + pulse * 0.5;
    }

    let indicator_radius = pin.radius * 0.4 * anim_scale;
    let aa = 0.5 / uniforms.camera_zoom;

    var alpha = 0.0;

    if (pin.direction == 0u) {
        let ring_thickness = indicator_radius * 0.4;
        let outer_d = sd_circle(pin_center, indicator_radius);
        let inner_d = sd_circle(pin_center, indicator_radius - ring_thickness);
        alpha = (1.0 - smoothstep(0.0, aa, outer_d)) * smoothstep(0.0, aa, inner_d);
    } else {
        let d = sd_circle(pin_center, indicator_radius);
        alpha = 1.0 - smoothstep(0.0, aa, d);
    }

    return vec4(pin.color.xyz, alpha * pin.color.w);
}

// ============================================================================
// DRAGGING EDGE SHADER (Foreground - reuses edge shader logic)
// ============================================================================

@vertex
fn vs_dragging(@builtin(vertex_index) vertex: u32) -> EdgeVertexOutput {
    // Build a bounding box for the dragging edge
    var p0 = vec2(0.0);
    var p3 = vec2(0.0);

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

    let bbox_min = min(p0, p3) - vec2(100.0 / uniforms.camera_zoom);
    let bbox_max = max(p0, p3) + vec2(100.0 / uniforms.camera_zoom);

    let corners = array<vec2<f32>, 4>(
        bbox_min,
        vec2(bbox_max.x, bbox_min.y),
        vec2(bbox_min.x, bbox_max.y),
        bbox_max
    );
    let indices = array<u32, 6>(0, 1, 2, 1, 3, 2);
    let world_pos = corners[indices[vertex]];

    let screen = (world_pos + uniforms.camera_position) * uniforms.camera_zoom * uniforms.os_scale_factor;
    let ndc = screen / uniforms.viewport_size * 2.0 - 1.0;
    let clip = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    return EdgeVertexOutput(clip, world_pos, 0u);
}

@fragment
fn fs_dragging(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
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

    var drag_color = vec4(0.0);
    if (uniforms.dragging == 4u) {
        drag_color = from_pin.color;
    } else {
        drag_color = uniforms.drag_edge_color;
    }

    let dist = sdCubicBezier(in.world_uv, p0, p1, p2, p3);
    let edge_thickness = 2.0 / uniforms.camera_zoom;
    let aa = 1.0 / uniforms.camera_zoom;

    let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + aa, dist);

    return vec4(drag_color.xyz, alpha);
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
