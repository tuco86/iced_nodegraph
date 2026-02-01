// SDF Renderer Shader
// Evaluates SDF operations in RPN format and renders with layer styling.

// ============================================================================
// Constants
// ============================================================================

// Operation types (must match compile.rs OpType enum)
const OP_CIRCLE: u32 = 0u;
const OP_BOX: u32 = 1u;
const OP_ROUNDED_BOX: u32 = 2u;
const OP_LINE: u32 = 3u;
const OP_BEZIER: u32 = 4u;

const OP_UNION: u32 = 16u;
const OP_SUBTRACT: u32 = 17u;
const OP_INTERSECT: u32 = 18u;
const OP_SMOOTH_UNION: u32 = 19u;
const OP_SMOOTH_SUBTRACT: u32 = 20u;

const OP_ROUND: u32 = 32u;
const OP_ONION: u32 = 33u;

// Layer flags
const LAYER_FLAG_GRADIENT: u32 = 1u;
const LAYER_FLAG_GRADIENT_U: u32 = 2u;
const LAYER_FLAG_HAS_PATTERN: u32 = 4u;

// Pattern types
const PATTERN_SOLID: u32 = 0u;
const PATTERN_DASHED: u32 = 1u;
const PATTERN_ARROWED: u32 = 2u;
const PATTERN_DOTTED: u32 = 3u;

const PI: f32 = 3.14159265359;
const MAX_STACK: u32 = 16u;

// ============================================================================
// Data Structures
// ============================================================================

struct Uniforms {
    viewport_size: vec2<f32>,
    camera_position: vec2<f32>,
    camera_zoom: f32,
    time: f32,
    num_ops: u32,
    num_layers: u32,
}

struct SdfOp {
    op_type: u32,
    flags: u32,
    _pad0: u32,
    _pad1: u32,
    param0: vec4<f32>,
    param1: vec4<f32>,
    param2: vec4<f32>,
}

struct SdfLayer {
    color: vec4<f32>,
    gradient_color: vec4<f32>,
    expand: f32,
    blur: f32,
    gradient_angle: f32,
    flags: u32,
    pattern_type: u32,
    thickness: f32,
    pattern_param0: f32,  // dash/segment/spacing
    pattern_param1: f32,  // gap/radius
    pattern_param2: f32,  // angle (arrowed)
    flow_speed: f32,
}

// SDF result: distance and arc-length parameter
struct SdfResult {
    dist: f32,
    u: f32,
}

// ============================================================================
// Bindings
// ============================================================================

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> ops: array<SdfOp>;
@group(0) @binding(2) var<storage, read> layers: array<SdfLayer>;

// ============================================================================
// SDF Primitives
// ============================================================================

// Circle SDF with arc-length parameter
fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> SdfResult {
    let d = length(p - center) - radius;
    let angle = atan2(p.y - center.y, p.x - center.x);
    let u = (angle + PI) * radius; // arc-length along circumference
    return SdfResult(d, u);
}

// Box SDF with perimeter parameter
fn sd_box(p: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>) -> SdfResult {
    let q = abs(p - center) - half_size;
    let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);

    // Compute perimeter position (clockwise from top-left)
    let rel = p - center;
    var u: f32 = 0.0;
    let w = half_size.x;
    let h = half_size.y;

    if abs(rel.y + h) < 0.001 && abs(rel.x) <= w {
        // Bottom edge
        u = 2.0 * w + 2.0 * h + (w - rel.x);
    } else if abs(rel.x - w) < 0.001 && abs(rel.y) <= h {
        // Right edge
        u = w + (h - rel.y);
    } else if abs(rel.y - h) < 0.001 && abs(rel.x) <= w {
        // Top edge
        u = (w + rel.x);
    } else {
        // Left edge
        u = 2.0 * w + h + (h + rel.y);
    }

    return SdfResult(d, u);
}

// Rounded box SDF
fn sd_rounded_box(p: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>, r: f32) -> SdfResult {
    let q = abs(p - center) - half_size + r;
    let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;

    // Approximate perimeter position
    let base = sd_box(p, center, half_size);
    return SdfResult(d, base.u);
}

// Line segment SDF
fn sd_line(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> SdfResult {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    let d = length(pa - ba * h);
    let u = h * length(ba); // arc-length along line
    return SdfResult(d, u);
}

// Cubic bezier SDF (approximation using sampling)
fn sd_bezier(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> SdfResult {
    var min_dist: f32 = 1e10;
    var best_t: f32 = 0.0;

    // Sample the curve to find closest point
    let num_samples: i32 = 16;
    for (var i: i32 = 0; i <= num_samples; i++) {
        let t = f32(i) / f32(num_samples);
        let mt = 1.0 - t;

        // Cubic bezier evaluation
        let pos = mt * mt * mt * p0
                + 3.0 * mt * mt * t * p1
                + 3.0 * mt * t * t * p2
                + t * t * t * p3;

        let dist = length(p - pos);
        if dist < min_dist {
            min_dist = dist;
            best_t = t;
        }
    }

    // Refine with a few more samples around best_t
    let dt = 1.0 / f32(num_samples);
    for (var i: i32 = -4; i <= 4; i++) {
        let t = clamp(best_t + f32(i) * dt * 0.25, 0.0, 1.0);
        let mt = 1.0 - t;
        let pos = mt * mt * mt * p0
                + 3.0 * mt * mt * t * p1
                + 3.0 * mt * t * t * p2
                + t * t * t * p3;
        let dist = length(p - pos);
        if dist < min_dist {
            min_dist = dist;
            best_t = t;
        }
    }

    // Approximate arc length (simple chord approximation)
    let curve_length = length(p1 - p0) + length(p2 - p1) + length(p3 - p2);
    let u = best_t * curve_length;

    return SdfResult(min_dist, u);
}

// ============================================================================
// CSG Operations
// ============================================================================

fn op_union(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist < b.dist {
        return a;
    }
    return b;
}

fn op_subtract(a: SdfResult, b: SdfResult) -> SdfResult {
    // a - b = max(a, -b)
    if a.dist > -b.dist {
        return SdfResult(a.dist, a.u);
    }
    return SdfResult(-b.dist, b.u);
}

fn op_intersect(a: SdfResult, b: SdfResult) -> SdfResult {
    if a.dist > b.dist {
        return a;
    }
    return b;
}

fn op_smooth_union(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = clamp(0.5 + 0.5 * (b.dist - a.dist) / k, 0.0, 1.0);
    let d = mix(b.dist, a.dist, h) - k * h * (1.0 - h);
    let u = mix(b.u, a.u, h);
    return SdfResult(d, u);
}

fn op_smooth_subtract(a: SdfResult, b: SdfResult, k: f32) -> SdfResult {
    let h = clamp(0.5 - 0.5 * (a.dist + b.dist) / k, 0.0, 1.0);
    let d = mix(a.dist, -b.dist, h) + k * h * (1.0 - h);
    let u = mix(a.u, b.u, h);
    return SdfResult(d, u);
}

// ============================================================================
// Modifiers
// ============================================================================

fn op_round(a: SdfResult, r: f32) -> SdfResult {
    return SdfResult(a.dist - r, a.u);
}

fn op_onion(a: SdfResult, thickness: f32) -> SdfResult {
    return SdfResult(abs(a.dist) - thickness, a.u);
}

// ============================================================================
// Pattern Evaluation
// ============================================================================

// Apply pattern to SDF result, returns new distance
fn apply_pattern(sdf: SdfResult, layer: SdfLayer) -> f32 {
    let dist = sdf.dist;
    let thickness = layer.thickness;

    // Apply flow animation
    var u = sdf.u;
    if layer.flow_speed != 0.0 {
        u = u + uniforms.time * layer.flow_speed;
    }

    switch layer.pattern_type {
        case PATTERN_SOLID: {
            // Solid stroke: just apply thickness
            return abs(dist) - thickness * 0.5;
        }
        case PATTERN_DASHED: {
            // Dashed pattern with round caps
            let dash = layer.pattern_param0;
            let gap = layer.pattern_param1;
            let period = dash + gap;

            // Find nearest dash center
            let nearest = round(u / period) * period;
            let dist_along = u - nearest;
            let half_dash = dash * 0.5;

            // Clamp to dash bounds
            let clamped = clamp(dist_along, -half_dash, half_dash);
            let cap_dist = dist_along - clamped;

            // Round cap: combine perpendicular distance with cap distance
            return length(vec2(cap_dist, dist)) - thickness * 0.5;
        }
        case PATTERN_ARROWED: {
            // Arrowed pattern (sheared dashes)
            let segment = layer.pattern_param0;
            let gap = layer.pattern_param1;
            let angle = layer.pattern_param2;
            let period = segment + gap;

            // Shear the u coordinate based on perpendicular distance
            let shifted_u = u - dist * tan(angle);

            // Same as dashed, but with shifted u
            let nearest = round(shifted_u / period) * period;
            let dist_along = shifted_u - nearest;
            let half_seg = segment * 0.5;
            let clamped = clamp(dist_along, -half_seg, half_seg);
            let cap_dist = dist_along - clamped;

            return length(vec2(cap_dist, dist)) - thickness * 0.5;
        }
        case PATTERN_DOTTED: {
            // Dotted pattern
            let spacing = layer.pattern_param0;
            let radius = layer.pattern_param1;

            // Find nearest dot center
            let nearest = round(u / spacing) * spacing;
            let dist_to_center = abs(u - nearest);

            // Distance to dot (circular)
            return length(vec2(dist_to_center, dist)) - radius;
        }
        default: {
            return dist;
        }
    }
}

// ============================================================================
// SDF Evaluation (Stack-based RPN)
// ============================================================================

fn evaluate_sdf(p: vec2<f32>) -> SdfResult {
    var stack: array<SdfResult, MAX_STACK>;
    var sp: u32 = 0u;

    for (var i: u32 = 0u; i < uniforms.num_ops; i++) {
        let op = ops[i];

        switch op.op_type {
            // Primitives - push result
            case OP_CIRCLE: {
                let center = op.param0.xy;
                let radius = op.param0.z;
                stack[sp] = sd_circle(p, center, radius);
                sp++;
            }
            case OP_BOX: {
                let center = op.param0.xy;
                let half_size = op.param0.zw;
                stack[sp] = sd_box(p, center, half_size);
                sp++;
            }
            case OP_ROUNDED_BOX: {
                let center = op.param0.xy;
                let half_size = op.param0.zw;
                let corner = op.param1.x;
                stack[sp] = sd_rounded_box(p, center, half_size, corner);
                sp++;
            }
            case OP_LINE: {
                let a = op.param0.xy;
                let b = op.param0.zw;
                stack[sp] = sd_line(p, a, b);
                sp++;
            }
            case OP_BEZIER: {
                let p0 = op.param0.xy;
                let p1 = op.param0.zw;
                let p2 = op.param1.xy;
                let p3 = op.param1.zw;
                stack[sp] = sd_bezier(p, p0, p1, p2, p3);
                sp++;
            }

            // Binary operations - pop 2, push 1
            case OP_UNION: {
                sp--;
                let b = stack[sp];
                sp--;
                let a = stack[sp];
                stack[sp] = op_union(a, b);
                sp++;
            }
            case OP_SUBTRACT: {
                sp--;
                let b = stack[sp];
                sp--;
                let a = stack[sp];
                stack[sp] = op_subtract(a, b);
                sp++;
            }
            case OP_INTERSECT: {
                sp--;
                let b = stack[sp];
                sp--;
                let a = stack[sp];
                stack[sp] = op_intersect(a, b);
                sp++;
            }
            case OP_SMOOTH_UNION: {
                sp--;
                let b = stack[sp];
                sp--;
                let a = stack[sp];
                let k = op.param0.x;
                stack[sp] = op_smooth_union(a, b, k);
                sp++;
            }
            case OP_SMOOTH_SUBTRACT: {
                sp--;
                let b = stack[sp];
                sp--;
                let a = stack[sp];
                let k = op.param0.x;
                stack[sp] = op_smooth_subtract(a, b, k);
                sp++;
            }

            // Unary modifiers - pop 1, push 1
            case OP_ROUND: {
                sp--;
                let a = stack[sp];
                let r = op.param0.x;
                stack[sp] = op_round(a, r);
                sp++;
            }
            case OP_ONION: {
                sp--;
                let a = stack[sp];
                let t = op.param0.x;
                stack[sp] = op_onion(a, t);
                sp++;
            }

            default: {}
        }
    }

    // Return top of stack (or zero if empty)
    if sp > 0u {
        return stack[sp - 1u];
    }
    return SdfResult(1e10, 0.0);
}

// ============================================================================
// Layer Rendering
// ============================================================================

fn render_layer(sdf: SdfResult, layer: SdfLayer) -> vec4<f32> {
    var d: f32;

    // Check if layer has a pattern
    if (layer.flags & LAYER_FLAG_HAS_PATTERN) != 0u {
        d = apply_pattern(sdf, layer);
    } else {
        // Apply expand for fill layers
        d = sdf.dist - layer.expand;
    }

    // Apply blur (smooth alpha transition)
    var alpha: f32;
    if layer.blur > 0.0 {
        alpha = 1.0 - smoothstep(-layer.blur, layer.blur, d);
    } else {
        alpha = select(0.0, 1.0, d < 0.0);
    }

    // Determine color
    var color = layer.color;

    if (layer.flags & LAYER_FLAG_GRADIENT) != 0u {
        var t: f32;
        if (layer.flags & LAYER_FLAG_GRADIENT_U) != 0u {
            // Gradient along arc-length
            t = fract(sdf.u * 0.01); // Normalize by some factor
        } else {
            // Gradient along angle
            let angle = layer.gradient_angle;
            // This would need position, simplified for now
            t = 0.5;
        }
        color = mix(layer.color, layer.gradient_color, t);
    }

    return vec4(color.rgb, color.a * alpha);
}

// ============================================================================
// Vertex/Fragment Shaders
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );

    var out: VertexOutput;
    out.position = vec4(positions[vertex_index], 0.0, 1.0);
    out.uv = positions[vertex_index] * 0.5 + 0.5;
    out.uv.y = 1.0 - out.uv.y; // Flip Y for screen coordinates
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Convert UV to world coordinates
    let screen_pos = in.uv * uniforms.viewport_size;
    let world_pos = screen_pos / uniforms.camera_zoom - uniforms.camera_position;

    // Evaluate SDF
    let sdf = evaluate_sdf(world_pos);

    // Composite layers (back to front)
    var color = vec4(0.0);

    for (var i: u32 = 0u; i < uniforms.num_layers; i++) {
        let layer = layers[i];
        let layer_color = render_layer(sdf, layer);

        // Alpha blend (premultiplied)
        color = color * (1.0 - layer_color.a) + layer_color;
    }

    return color;
}
