struct Uniforms {
    os_scale_factor: f32,       // e.g. 1.0, 1.5
    camera_zoom: f32,
    camera_position: vec2<f32>,

    border_color: vec4<f32>,  // RGBA for node border
    fill_color: vec4<f32>,    // RGBA for node fill

    cursor_position: vec2<f32>,

    num_nodes: u32,
    num_pins: u32,
    num_edges: u32,
    
    dragging: u32,
    dragging_edge_from_node: u32,
    dragging_edge_from_pin: u32,
    dragging_edge_to_x: f32,
    dragging_edge_to_y: f32,
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
// Adjust UV coordinates based on camera zoom and position.
    let uv = (frag_coord.xy / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;

    var d = 1e5;

// Iterate over nodes and apply transformations.
    for (var i = 0u; i < uniforms.num_nodes; i++) {
        let node = nodes[i];
        let node_half_size = node.size * 0.5;
        let node_center = uv - (node.position + node_half_size);
        let node_d = sd_rounded_box(node_center, node_half_size, vec4(node.corner_radius));
        d = min(node_d, d);

        // Iterate over pins and apply transformations.
        for (var j = 0u; j < node.pin_count; j++) {
            let pin = pins[node.pin_start + j];
            let pin_center = uv - pin.position;
            let pin_d = sd_circle(pin_center, pin.radius);
            d = max(d, -pin_d); // Subtract pin from box.
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

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

// Signed distance to quadratic Bezier curve (A, B, C)
fn sdBezier(pos: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {
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
    var res = 0.0;
    if (h >= 0.0) {
        let h_sqrt = sqrt(h);
        let x1 = (h_sqrt - q) / 2.0;
        let x2 = (-h_sqrt - q) / 2.0;
        let uv1 = sign(x1) * pow(abs(x1), 1.0 / 3.0);
        let uv2 = sign(x2) * pow(abs(x2), 1.0 / 3.0);
        let t = clamp(uv1 + uv2 - kx, 0.0, 1.0);
        res = dot2(d + (c + b * t) * t);
    } else {
        let z = sqrt(-p);
        let v = acos(q / (2.0 * p * z)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let t1 = clamp(m + m, 0.0, 1.0) * z - kx;
        let t2 = clamp(-n - m, 0.0, 1.0) * z - kx;
        let t3 = clamp(n - m, 0.0, 1.0) * z - kx;
        let t1c = clamp(t1, 0.0, 1.0);
        let t2c = clamp(t2, 0.0, 1.0);
        res = min(
            dot2(d + (c + b * t1c) * t1c),
            dot2(d + (c + b * t2c) * t2c)
        );
        // The third root cannot be the closest
        // res = min(res, dot2(d + (c + b * t3c) * t3c));
    }
    return sqrt(res);
}

// Signed distance to a line segment AB
fn sdSegment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

@fragment
fn fs_foreground(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;
    var color = vec4<f32>(0.0);

    if (uniforms.dragging == 3u) {
        // Pin-Side Mapping: 0=Left, 1=Right, 2=Top, 3=Bottom, 4=Row
        let from_node = nodes[uniforms.dragging_edge_from_node];
        let from_pin = pins[from_node.pin_start + uniforms.dragging_edge_from_pin];
        // let to_pos = vec2<f32>(uniforms.dragging_edge_to_x, uniforms.dragging_edge_to_y);

        // Segment 1: Gerade aus dem Pin heraus
        var dir_from = vec2<f32>(0.0, 0.0);
        switch (from_pin.side) {
            case 0u: { dir_from = vec2<f32>(-1.0, 0.0); }  // Left: nach außen (rechts)
            case 1u: { dir_from = vec2<f32>(1.0, 0.0); } // Right: nach außen (links)
            case 2u: { dir_from = vec2<f32>(0.0, -1.0); }  // Top: nach außen (unten)
            case 3u: { dir_from = vec2<f32>(0.0, 1.0); } // Bottom: nach außen (oben)
            default: { dir_from = normalize(uv - from_pin.position); }
        }
        var dir_to = -dir_from;

        let seg_len = 24.0 / uniforms.camera_zoom;
        let p0 = from_pin.position;
        let p1 = from_pin.position + dir_from * seg_len;

        // Segment 4: Gerade in den Zielpin (angenommen gegenüberliegende Seite)
        let p3 = uniforms.cursor_position;
        let p2 = uniforms.cursor_position + dir_to * seg_len;

        // Use a fixed control point offset for a visually pleasing curve
        let ctrl = (p1 + p2) * 0.5 + vec2<f32>(0.0, 0.0); // try offsetting here if you want more curve

        // SDF for start segment, bezier, end segment
        var dist = sdSegment(uv, p0, p1);
        dist = min(dist, sdSegment(uv, p1, p2));
        dist = min(dist, sdSegment(uv, p2, p3));

        let edge_thickness = 4.0 / uniforms.camera_zoom;
        let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + 1.5, dist);
        color = mix(vec4<f32>(0.0), vec4<f32>(uniforms.border_color.xyz, 1.0), alpha);
    }

    return color;
}
