// ============================================================================
// PHYSICS COMPUTE SHADER
// Edge wire simulation using spring-damper system with repulsion forces
// ============================================================================

struct PhysicsUniforms {
    spring_stiffness: f32,
    damping: f32,
    rest_length: f32,
    node_repulsion: f32,
    edge_repulsion: f32,
    repulsion_radius: f32,
    max_velocity: f32,
    dt: f32,
    num_vertices: u32,
    num_edges: u32,
    num_nodes: u32,
    _pad0: u32,
}

struct PhysicsVertex {
    position: vec2<f32>,
    velocity: vec2<f32>,
    mass: f32,
    flags: u32,         // bit 0 = anchored
    edge_index: u32,
    vertex_index: u32,
}

struct PhysicsEdgeMeta {
    vertex_start: u32,
    vertex_count: u32,
    from_node: u32,
    from_pin: u32,
    to_node: u32,
    to_pin: u32,
    _pad0: u32,
    _pad1: u32,
    color: vec4<f32>,
    thickness: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
}

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
}

@group(0) @binding(0)
var<uniform> physics: PhysicsUniforms;

@group(0) @binding(1)
var<storage, read> nodes: array<Node>;

@group(0) @binding(2)
var<storage, read> edges_meta: array<PhysicsEdgeMeta>;

@group(1) @binding(0)
var<storage, read> vertices_in: array<PhysicsVertex>;

@group(1) @binding(1)
var<storage, read_write> vertices_out: array<PhysicsVertex>;

// ============================================================================
// FORCE CALCULATIONS
// ============================================================================

/// Calculate spring force between two positions.
/// Returns force vector applied to position `from`.
fn spring_force(from_pos: vec2<f32>, to_pos: vec2<f32>) -> vec2<f32> {
    let delta = to_pos - from_pos;
    let distance = length(delta);

    if (distance < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    let direction = delta / distance;
    let displacement = distance - physics.rest_length;
    let force_magnitude = physics.spring_stiffness * displacement;

    return direction * force_magnitude;
}

/// Calculate repulsion force from a point.
/// Returns force vector pushing `from` away from `repulsor`.
fn repulsion_force(from_pos: vec2<f32>, repulsor_pos: vec2<f32>, strength: f32) -> vec2<f32> {
    let delta = from_pos - repulsor_pos;
    let distance_sq = dot(delta, delta);
    let distance = sqrt(distance_sq);

    if (distance > physics.repulsion_radius || distance < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    let direction = delta / distance;
    let force_magnitude = strength / (distance_sq + 1.0);

    return direction * force_magnitude;
}

/// Calculate repulsion from a node (rectangular region).
fn node_repulsion_force(vertex_pos: vec2<f32>, node: Node) -> vec2<f32> {
    let node_center = node.position + node.size * 0.5;
    let half_size = node.size * 0.5;

    // Find closest point on node boundary
    let clamped = clamp(vertex_pos, node.position, node.position + node.size);

    // If vertex is inside node, push it out
    let is_inside = all(vertex_pos >= node.position) && all(vertex_pos <= node.position + node.size);

    if (is_inside) {
        // Push toward nearest edge
        let to_left = vertex_pos.x - node.position.x;
        let to_right = (node.position.x + node.size.x) - vertex_pos.x;
        let to_top = vertex_pos.y - node.position.y;
        let to_bottom = (node.position.y + node.size.y) - vertex_pos.y;

        let min_dist = min(min(to_left, to_right), min(to_top, to_bottom));

        var push_dir = vec2<f32>(0.0, 0.0);
        if (min_dist == to_left) {
            push_dir = vec2<f32>(-1.0, 0.0);
        } else if (min_dist == to_right) {
            push_dir = vec2<f32>(1.0, 0.0);
        } else if (min_dist == to_top) {
            push_dir = vec2<f32>(0.0, -1.0);
        } else {
            push_dir = vec2<f32>(0.0, 1.0);
        }

        return push_dir * physics.node_repulsion * 2.0;
    }

    // Repulsion from closest point
    return repulsion_force(vertex_pos, clamped, physics.node_repulsion);
}

// ============================================================================
// MAIN COMPUTE SHADER
// ============================================================================

@compute @workgroup_size(64)
fn physics_step(@builtin(global_invocation_id) gid: vec3<u32>) {
    let vertex_id = gid.x;

    if (vertex_id >= physics.num_vertices) {
        return;
    }

    var vertex = vertices_in[vertex_id];

    // Anchored vertices don't move
    let is_anchored = (vertex.flags & 1u) != 0u;
    if (is_anchored) {
        vertices_out[vertex_id] = vertex;
        return;
    }

    var force = vec2<f32>(0.0, 0.0);

    // Get edge metadata for this vertex
    let edge = edges_meta[vertex.edge_index];
    let vertex_count = edge.vertex_count;
    let vertex_start = edge.vertex_start;
    let local_index = vertex.vertex_index;

    // --- Spring forces from neighbors ---

    // Force from previous vertex
    if (local_index > 0u) {
        let prev_idx = vertex_start + local_index - 1u;
        let prev = vertices_in[prev_idx];
        force += spring_force(vertex.position, prev.position);
    }

    // Force from next vertex
    if (local_index < vertex_count - 1u) {
        let next_idx = vertex_start + local_index + 1u;
        let next = vertices_in[next_idx];
        force += spring_force(vertex.position, next.position);
    }

    // --- Node repulsion ---
    for (var i = 0u; i < physics.num_nodes; i++) {
        let node = nodes[i];
        force += node_repulsion_force(vertex.position, node);
    }

    // --- Edge-edge repulsion (sampled, not N^2) ---
    // Only check every 4th vertex from other edges to reduce complexity
    for (var e = 0u; e < physics.num_edges; e++) {
        if (e == vertex.edge_index) {
            continue; // Skip own edge
        }

        let other_edge = edges_meta[e];
        let sample_step = max(1u, other_edge.vertex_count / 4u);

        for (var v = 0u; v < other_edge.vertex_count; v += sample_step) {
            let other_idx = other_edge.vertex_start + v;
            let other = vertices_in[other_idx];
            force += repulsion_force(vertex.position, other.position, physics.edge_repulsion);
        }
    }

    // --- Semi-implicit Euler integration ---
    let acceleration = force / vertex.mass;

    // Update velocity
    var new_velocity = (vertex.velocity + acceleration * physics.dt) * physics.damping;

    // Clamp velocity
    let speed = length(new_velocity);
    if (speed > physics.max_velocity) {
        new_velocity = new_velocity * (physics.max_velocity / speed);
    }

    // Update position
    let new_position = vertex.position + new_velocity * physics.dt;

    // Write output
    var out_vertex = vertex;
    out_vertex.position = new_position;
    out_vertex.velocity = new_velocity;
    vertices_out[vertex_id] = out_vertex;
}
