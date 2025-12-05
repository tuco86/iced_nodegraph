// Error Visualization Shader
// Displays animated red/pink pattern when shader compilation fails

@fragment
fn fs_edge_error(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.world_uv / 100.0;
    let t = uniforms.time;

    // Animated checkerboard pattern
    let checker = step(0.5, fract(uv.x * 5.0 + t * 0.5)) + step(0.5, fract(uv.y * 5.0 - t * 0.5));
    let pattern = mod(checker, 2.0);

    // Pulsing effect
    let pulse = 0.7 + 0.3 * sin(t * 3.0);

    // Error colors: red to pink
    let color1 = vec3<f32>(1.0, 0.1, 0.2);
    let color2 = vec3<f32>(1.0, 0.4, 0.6);
    let error_color = mix(color1, color2, pattern) * pulse;

    return vec4<f32>(error_color, 0.9);
}

@fragment
fn fs_node_error(in: NodeVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.world_uv / 50.0;
    let t = uniforms.time;

    // Diagonal stripes
    let stripes = sin((uv.x + uv.y) * 10.0 + t * 2.0);
    let pattern = step(0.0, stripes);

    let error_color = mix(vec3<f32>(0.8, 0.0, 0.1), vec3<f32>(1.0, 0.3, 0.4), pattern);

    return vec4<f32>(error_color, 0.7);
}

@fragment
fn fs_background_error(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / uniforms.viewport_size;
    let t = uniforms.time;

    // Pulsing background
    let pulse1 = sin(uv.x * 3.0 + t * 1.5);
    let pulse2 = sin(uv.y * 3.0 - t * 1.5);
    let pattern = pulse1 * pulse2;

    let bg_color = vec3<f32>(0.15, 0.01, 0.02);
    let error_tint = vec3<f32>(0.3, 0.05, 0.05) * (0.5 + 0.5 * pattern);

    return vec4<f32>(bg_color + error_tint, 1.0);
}
