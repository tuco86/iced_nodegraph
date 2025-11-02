struct Uniforms {
    os_scale_factor: f32,       // e.g. 1.0, 1.5
    camera_zoom: f32,
    camera_position: vec2<f32>,

    border_color: vec4<f32>,     // RGBA for node border
    fill_color: vec4<f32>,       // RGBA for node fill
    edge_color: vec4<f32>,       // RGBA for edges
    background_color: vec4<f32>, // RGBA for background
    drag_edge_color: vec4<f32>,  // RGBA for dragging edge
    drag_edge_valid_color: vec4<f32>, // RGBA for valid connection

    cursor_position: vec2<f32>,

    num_nodes: u32,
    num_pins: u32,
    num_edges: u32,
    time: f32,                  // Time in seconds for animations
    
    dragging: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    dragging_edge_from_node: u32,
    dragging_edge_from_pin: u32,
    dragging_edge_from_origin: vec2<f32>,
    dragging_edge_to_node: u32,
    dragging_edge_to_pin: u32,
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
    position: vec2<f32>,       // position from top-left (8 bytes)
    side: u32,                 // 0 = top, 1 = right, 2 = bottom, 3 = left, 4 = row (4 bytes)
    radius: f32,               // 4 bytes (total 16 bytes)
    color: vec4<f32>,          // RGBA color for pin type indicator (16 bytes, total 32)
    direction: u32,            // 0 = Input, 1 = Output, 2 = Both (4 bytes)
    flags: u32,                // Future use (4 bytes)
    _pad0: u32,                // Alignment (4 bytes)
    _pad1: u32,                // Alignment (4 bytes, total 48)
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

// Helper function to render edges - called from background to put edges behind nodes
fn render_edges(uv: vec2<f32>, base_color: vec3<f32>) -> vec3<f32> {
    var col = base_color;
    let edge_thickness = 2.0 / uniforms.camera_zoom;
    let endpoint_radius = 4.0 / uniforms.camera_zoom;
    
    // Render static edges
    for (var i = 0u; i < uniforms.num_edges; i++) {
        let edge = edges[i];
        let from_node = nodes[edge.from_node];
        let from_pin = pins[from_node.pin_start + edge.from_pin];
        let to_node = nodes[edge.to_node];
        let to_pin = pins[to_node.pin_start + edge.to_pin];

        // Calculate direction from pin side
        var dir_from = vec2<f32>(0.0, 0.0);
        switch (from_pin.side) {
            case 0u: { dir_from = vec2<f32>(-1.0, 0.0); }
            case 1u: { dir_from = vec2<f32>(1.0, 0.0); }
            case 2u: { dir_from = vec2<f32>(0.0, -1.0); }
            case 3u: { dir_from = vec2<f32>(0.0, 1.0); }
            default: { dir_from = vec2<f32>(1.0, 0.0); }
        }

        var dir_to = vec2<f32>(0.0, 0.0);
        switch (to_pin.side) {
            case 0u: { dir_to = vec2<f32>(-1.0, 0.0); }
            case 1u: { dir_to = vec2<f32>(1.0, 0.0); }
            case 2u: { dir_to = vec2<f32>(0.0, -1.0); }
            case 3u: { dir_to = vec2<f32>(0.0, 1.0); }
            default: { dir_to = vec2<f32>(-1.0, 0.0); }
        }

        let seg_len = 80.0 / uniforms.camera_zoom;
        let p0 = from_pin.position;
        let p1 = from_pin.position + dir_from * seg_len;
        let p3 = to_pin.position;
        let p2 = to_pin.position + dir_to * seg_len;

        // Determine edge color based on pin directions and colors
        // Priority: Output > Input > Both
        var edge_color = vec3<f32>(0.5, 0.5, 0.5); // Fallback gray
        
        if (from_pin.direction == 1u) {
            // From pin is Output - use its color (highest priority)
            edge_color = from_pin.color.xyz;
        } else if (to_pin.direction == 1u) {
            // To pin is Output - use its color
            edge_color = to_pin.color.xyz;
        } else if (from_pin.direction == 0u) {
            // From pin is Input - use its color (second priority)
            edge_color = from_pin.color.xyz;
        } else if (to_pin.direction == 0u) {
            // To pin is Input - use its color
            edge_color = to_pin.color.xyz;
        } else {
            // Both pins are Both direction - use from pin color
            edge_color = from_pin.color.xyz;
        }

        let dist = sdCubicBezier(uv, p0, p1, p2, p3);
        let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + 1.0, dist);
        col = mix(col, edge_color, alpha);
        
        // Add solid dots at endpoints with matching color
        let dist_start = length(uv - p0);
        let dist_end = length(uv - p3);
        let dot_alpha_start = 1.0 - smoothstep(endpoint_radius, endpoint_radius + 1.0, dist_start);
        let dot_alpha_end = 1.0 - smoothstep(endpoint_radius, endpoint_radius + 1.0, dist_end);
        col = mix(col, edge_color, max(dot_alpha_start, dot_alpha_end));
    }
    
    return col;
}

@fragment
fn fs_background(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
// Adjust UV coordinates based on camera zoom and position.
    // Original shader formula (this is correct for rendering!)
    let uv = (frag_coord.xy / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;

    // Start with theme background color and add grid pattern
    let grid_intensity = grid_pattern(uv, 100.0, 1000.0, uniforms.camera_zoom);
    // Grid lines: lighter version of border color for theme consistency
    let grid_color = uniforms.border_color.xyz * 1.3;  // Slightly lighter than border
    var col = mix(uniforms.background_color.xyz, grid_color, grid_intensity);

    // Render edges BEFORE nodes (so they appear behind)
    col = render_edges(uv, col);

    let aa = 0.5 / uniforms.camera_zoom;  // Tighter anti-aliasing for smooth edges
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

    // Render colored pin type indicators
    // Output (1) = filled circle, Input (0) = hollow circle (ring), Both (2) = filled
    for (var i = 0u; i < uniforms.num_nodes; i++) {
        let node = nodes[i];
        for (var j = 0u; j < node.pin_count; j++) {
            let pin = pins[node.pin_start + j];
            let pin_center = uv - pin.position;
            let indicator_radius = pin.radius * 0.4; // 40% of pin radius
            
            if (pin.direction == 0u) {
                // Input: Hollow circle (ring)
                let ring_thickness = indicator_radius * 0.4; // Ring thickness
                let outer_d = sd_circle(pin_center, indicator_radius);
                let inner_d = sd_circle(pin_center, indicator_radius - ring_thickness);
                // Ring is where outer is inside but inner is outside
                let ring_alpha = (1.0 - smoothstep(0.0, aa, outer_d)) * smoothstep(0.0, aa, inner_d);
                col = mix(col, pin.color.xyz, ring_alpha * pin.color.w);
            } else {
                // Output or Both: Filled circle
                let indicator_d = sd_circle(pin_center, indicator_radius);
                let indicator_alpha = 1.0 - smoothstep(0.0, aa, indicator_d);
                col = mix(col, pin.color.xyz, indicator_alpha * pin.color.w);
            }
        }
    }

    // Render nodes with clean anti-aliasing (UE5-style: thin border)
    let border_width = 1.0 / uniforms.camera_zoom;  // Thinner border (1px instead of 2px)
    let node_opacity = 0.75;  // 75% opacity = 25% transparent
    
    // Inside node (d < 0)
    if (d < 0.0) {
        // We're inside the node
        if (d > -border_width) {
            // Inside border region - blend with background using transparency
            col = mix(col, uniforms.border_color.xyz, node_opacity);
        } else {
            // Inside fill region - blend with background using transparency
            col = mix(col, uniforms.fill_color.xyz, node_opacity);
        }
    } else if (d < aa) {
        // Anti-aliasing on the outer edge
        let alpha = (1.0 - smoothstep(0.0, aa, d)) * node_opacity;
        col = mix(col, uniforms.border_color.xyz, alpha);
    }

    return vec4(col, 1.0);
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

// Grid pattern: returns intensity of grid lines
fn grid_pattern(uv: vec2<f32>, minor_spacing: f32, major_spacing: f32, zoom: f32) -> f32 {
    // Simple approach: use modulo to find distance to nearest grid line
    let coord_minor = abs(uv % minor_spacing);
    let dist_minor_x = min(coord_minor.x, minor_spacing - coord_minor.x);
    let dist_minor_y = min(coord_minor.y, minor_spacing - coord_minor.y);
    
    let coord_major = abs(uv % major_spacing);
    let dist_major_x = min(coord_major.x, major_spacing - coord_major.x);
    let dist_major_y = min(coord_major.y, major_spacing - coord_major.y);
    
    // Line thickness in world space
    let minor_width = 1.0;
    let major_width = 2.0;
    
    var intensity = 0.0;
    
    // Major grid lines (every 1000px)
    if (dist_major_x < major_width || dist_major_y < major_width) {
        intensity = 0.7;
    }
    // Minor grid lines (every 100px)
    else if (dist_minor_x < minor_width || dist_minor_y < minor_width) {
        intensity = 0.35;
    }
    
    return intensity;
}

// Analytical signed distance to cubic Bezier curve (p0, p1, p2, p3)
// Based on Inigo Quilez's approach - finds closest point using iterative refinement
fn sdCubicBezier(pos: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    // Transform to polynomial form: P(t) = A*t³ + B*t² + C*t + D
    let A = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let B = 3.0 * p0 - 6.0 * p1 + 3.0 * p2;
    let C = -3.0 * p0 + 3.0 * p1;
    let D = p0;
    
    // Iterate to find parameter t of closest point on curve
    // Start with a few seed points and refine the best one
    var min_dist = dot2(pos - p0);
    var best_t = 0.0;
    
    // Check multiple starting points along the curve
    for (var i = 0; i <= 8; i = i + 1) {
        var t = f32(i) / 8.0;
        
        // Newton-Raphson refinement (few iterations)
        for (var iter = 0; iter < 4; iter = iter + 1) {
            let t2 = t * t;
            let t3 = t2 * t;
            
            // Point on curve: P(t)
            let point = A * t3 + B * t2 + C * t + D;
            
            // First derivative: P'(t) = 3A*t² + 2B*t + C
            let deriv = 3.0 * A * t2 + 2.0 * B * t + C;
            
            // Second derivative: P''(t) = 6A*t + 2B
            let deriv2 = 6.0 * A * t + 2.0 * B;
            
            // Vector from curve to query point
            let diff = point - pos;
            
            // Newton-Raphson step: t_new = t - f(t)/f'(t)
            // where f(t) = dot(P(t)-pos, P'(t))
            let f = dot(diff, deriv);
            let fp = dot(deriv, deriv) + dot(diff, deriv2);
            
            if (abs(fp) > 0.00001) {
                t = t - f / fp;
            }
            
            t = clamp(t, 0.0, 1.0);
        }
        
        // Evaluate distance at refined t
        let t2 = t * t;
        let t3 = t2 * t;
        let point = A * t3 + B * t2 + C * t + D;
        let dist = dot2(pos - point);
        
        if (dist < min_dist) {
            min_dist = dist;
            best_t = t;
        }
    }
    
    // Also check endpoints
    min_dist = min(min_dist, dot2(pos - p3));
    
    return sqrt(min_dist);
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
    // Original shader formula (this is correct for rendering!)
    let uv = (frag_coord.xy / (uniforms.os_scale_factor * uniforms.camera_zoom)) - uniforms.camera_position;
    var color = vec4<f32>(0.0);
    let edge_thickness = 2.0 / uniforms.camera_zoom;
    let endpoint_radius = 4.0 / uniforms.camera_zoom;

    // Static edges are now rendered in background shader (behind nodes)
    // Only render dragging edge here (Edge or EdgeOver state)
    if (uniforms.dragging == 3u || uniforms.dragging == 4u) {
        let from_node = nodes[uniforms.dragging_edge_from_node];
        let from_pin = pins[from_node.pin_start + uniforms.dragging_edge_from_pin];

        var dir_from = vec2<f32>(0.0, 0.0);
        switch (from_pin.side) {
            case 0u: { dir_from = vec2<f32>(-1.0, 0.0); }
            case 1u: { dir_from = vec2<f32>(1.0, 0.0); }
            case 2u: { dir_from = vec2<f32>(0.0, -1.0); }
            case 3u: { dir_from = vec2<f32>(0.0, 1.0); }
            default: { dir_from = normalize(uv - from_pin.position); }
        }

        let seg_len = 80.0 / uniforms.camera_zoom;  // Longer segments for curvier bezier
        let p0 = from_pin.position;
        let p1 = from_pin.position + dir_from * seg_len;
        
        var p3 = uniforms.cursor_position;
        var dir_to = -dir_from;
        
        // If in EdgeOver state, use the actual target pin position and direction
        if (uniforms.dragging == 4u) {
            let to_node = nodes[uniforms.dragging_edge_to_node];
            let to_pin = pins[to_node.pin_start + uniforms.dragging_edge_to_pin];
            p3 = to_pin.position;
            
            // Get proper direction based on target pin side
            switch (to_pin.side) {
                case 0u: { dir_to = vec2<f32>(-1.0, 0.0); }
                case 1u: { dir_to = vec2<f32>(1.0, 0.0); }
                case 2u: { dir_to = vec2<f32>(0.0, -1.0); }
                case 3u: { dir_to = vec2<f32>(0.0, 1.0); }
                default: { dir_to = -dir_from; }
            }
        }
        
        let p2 = p3 + dir_to * seg_len;

        // Determine drag edge color based on source pin
        // Use pin color when valid connection, theme color for invalid
        var drag_color = vec4<f32>(0.0);
        if (uniforms.dragging == 4u) {
            // EdgeOver - valid connection, use source pin color
            drag_color = from_pin.color;
        } else {
            // Edge - dragging without valid target, use warning theme color
            drag_color = uniforms.drag_edge_color;
        }

        // Render entire dragging edge as cubic bezier curve
        let dist = sdCubicBezier(uv, p0, p1, p2, p3);
        let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + 1.0, dist);
        color = mix(color, drag_color, alpha);
        
        // Add solid dots at endpoints for dragging edge
        let dist_start_drag = length(uv - p0);
        let dist_end_drag = length(uv - p3);
        let dot_alpha_start_drag = 1.0 - smoothstep(endpoint_radius, endpoint_radius + 1.0, dist_start_drag);
        let dot_alpha_end_drag = 1.0 - smoothstep(endpoint_radius, endpoint_radius + 1.0, dist_end_drag);
        color = mix(color, drag_color, max(dot_alpha_start_drag, dot_alpha_end_drag));
    }

    return color;
}
