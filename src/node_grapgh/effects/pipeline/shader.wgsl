struct Uniforms {
    os_scale_factor: f32,       // e.g. 1.0, 1.5
    camera_zoom: f32,
    camera_position: vec2<f32>,

    border_color: vec4<f32>,  // RGBA for node border
    fill_color: vec4<f32>,    // RGBA for node fill

    num_nodes: u32,
    num_pins: u32,
    num_edges: u32,
    _padding: u32,
};

struct Node {
    position: vec2<f32>,       // top-left in screen space
    size: vec2<f32>,           // width / height
    corner_radius: f32,
    pin_start: u32,
    pin_count: u32,
    _padding: u32,
};

struct Pin {
    position: vec2<f32>,         // position from top-left
    side: u32,                 // 0 = top, 1 = right, 2 = bottom, 3 = left, 4 = row
    radius: f32,
};

struct Edge {
    from_node: u32,
    from_pin: u32,
    to_node: u32,
    to_pin: u32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var<storage, read> nodes: array<Node>;

@group(0) @binding(2)
var<storage, read> pins: array<Pin>;

@group(0) @binding(3)
var<storage, read> edges: array<Edge>;

fn sdRoundedBox(p: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let d = abs(p) - size + vec2<f32>(radius);
    return length(max(d, vec2<f32>(0.0))) - radius;
}

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

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn op_round(d: f32, r: f32) -> f32 {
    return d - r;
}

fn op_onion(d: f32, r: f32) -> f32 {
    return abs(d) - r;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    let pos = positions[vertex_index];
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_background(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy / uniforms.os_scale_factor * uniforms.camera_zoom) - uniforms.camera_position;
    var d = 1e5;

    for (var i = 0u; i < uniforms.num_nodes; i++) {
        let node = nodes[i];
        let node_half_size = node.size * 0.5;
        let node_center = uv - (node.position + node_half_size);
        let node_d = sd_rounded_box(node_center, node_half_size, vec4(node.corner_radius));
        d = min(node_d, d);
        for (var j = 0u; j < node.pin_count; j++) {
            let pin = pins[node.pin_start + j];
            let pin_center = uv - pin.position;
            let pin_d = sd_circle(pin_center, pin.radius);
            d = max(d, -pin_d); // subtract pin from box
        }
    }

    // coloring
    var col = select(
        uniforms.border_color.xyz,
        uniforms.fill_color.xyz,
        d > 0.0
    );

    col *= 1.0 - exp(-6.0 * abs(d));
    col *= 0.8 + 0.2 * cos(0.1 * d);
    col = mix(col, vec3<f32>(0.0), 1.0 - smoothstep(1.2, 1.3, abs(d)));

    return vec4(col, 1.0);
}

@fragment
fn fs_foreground(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy;
    var dist = 1e5;
    return vec4(1.0, 0.0, 0.0, 0.0);
}
