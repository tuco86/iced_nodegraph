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
    // Force parameters
    gravity: f32,
    bending_stiffness: f32,
    pin_suction: f32,
    path_attraction: f32,
    // Improved segment model parameters
    contraction_strength: f32,
    curvature_contraction: f32,
    node_wrap_distance: f32,
    edge_bundle_distance: f32,
    edge_attraction_range: f32,
    min_segment_length: f32,
    edge_attraction: f32,
    _pad0: u32,
    _pad1: u32,
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
    // Anchor positions for pin suction and path attraction
    start_anchor: vec2<f32>,
    end_anchor: vec2<f32>,
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

/// Calculate bending stiffness force.
/// Pulls vertex toward midpoint of prev/next neighbors for smooth curves.
fn bending_force(prev: vec2<f32>, current: vec2<f32>, next: vec2<f32>) -> vec2<f32> {
    let midpoint = (prev + next) * 0.5;
    return (midpoint - current) * physics.bending_stiffness;
}

/// Gravity force (constant downward).
fn gravity_force() -> vec2<f32> {
    return vec2<f32>(0.0, physics.gravity);
}

/// Pin suction force - pulls vertex toward nearest anchor.
/// Simulates pins "slurping up" the edge like spaghetti.
fn pin_suction_force(
    pos: vec2<f32>,
    start_anchor: vec2<f32>,
    end_anchor: vec2<f32>,
    vertex_t: f32,  // 0.0 = at start, 1.0 = at end
) -> vec2<f32> {
    // Determine which anchor to pull toward based on position
    var anchor_pos: vec2<f32>;
    var pull_strength: f32;

    if (vertex_t < 0.5) {
        // Closer to start - pull toward start
        let t = 1.0 - vertex_t * 2.0;  // 1.0 at start, 0.0 at middle
        anchor_pos = start_anchor;
        pull_strength = physics.pin_suction * t * t;
    } else {
        // Closer to end - pull toward end
        let t = (vertex_t - 0.5) * 2.0;  // 0.0 at middle, 1.0 at end
        anchor_pos = end_anchor;
        pull_strength = physics.pin_suction * t * t;
    }

    let delta = anchor_pos - pos;
    let dist = length(delta);

    if (dist < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    return (delta / dist) * pull_strength;
}

/// Path attraction force - pulls vertex toward direct line between pins.
fn path_attraction_force(
    pos: vec2<f32>,
    start_anchor: vec2<f32>,
    end_anchor: vec2<f32>,
) -> vec2<f32> {
    let line = end_anchor - start_anchor;
    let line_len_sq = dot(line, line);

    if (line_len_sq < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    // Project vertex onto line segment
    let t = clamp(dot(pos - start_anchor, line) / line_len_sq, 0.0, 1.0);
    let closest = start_anchor + t * line;

    let delta = closest - pos;
    let dist = length(delta);

    if (dist < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    // Quadratic falloff - stronger when far from path
    let force_magnitude = physics.path_attraction * min(dist / 100.0, 1.0);
    return (delta / dist) * force_magnitude;
}

// ============================================================================
// IMPROVED SEGMENT MODEL FORCES
// ============================================================================

/// Contraction force - segments try to become shorter.
/// Force is proportional to current segment length (longer = stronger pull).
fn contraction_force(current: vec2<f32>, neighbor: vec2<f32>) -> vec2<f32> {
    let delta = neighbor - current;
    let dist = length(delta);

    if (dist < physics.min_segment_length || dist < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    let direction = delta / dist;

    // Force proportional to length (longer segments pull harder)
    let length_factor = dist / physics.rest_length;
    let force_magnitude = physics.contraction_strength * length_factor;

    return direction * force_magnitude;
}

/// Calculate curvature at a vertex (how bent the edge is here).
/// Returns 0.0 for straight, 1.0 for 90 degree bend, 2.0 for hairpin.
fn calculate_curvature(prev: vec2<f32>, current: vec2<f32>, next: vec2<f32>) -> f32 {
    let v1 = current - prev;
    let v2 = next - current;
    let len1 = length(v1);
    let len2 = length(v2);

    if (len1 < 0.001 || len2 < 0.001) {
        return 0.0;
    }

    let d = dot(v1 / len1, v2 / len2);
    // curvature = 1 - cos(angle)
    // Straight: d ≈ 1 → curvature ≈ 0
    // 90° bend: d ≈ 0 → curvature ≈ 1
    // Hairpin: d ≈ -1 → curvature ≈ 2
    return 1.0 - d;
}

/// Extra contraction force in curved regions.
/// Creates more detail in curves, less vertices in straight sections.
fn curvature_contraction_force(
    prev: vec2<f32>,
    current: vec2<f32>,
    next: vec2<f32>,
) -> vec2<f32> {
    let curvature = calculate_curvature(prev, current, next);
    let midpoint = (prev + next) * 0.5;
    let to_midpoint = midpoint - current;
    let dist = length(to_midpoint);

    if (dist < 0.001) {
        return vec2<f32>(0.0, 0.0);
    }

    // Stronger pull toward midpoint at curves
    let force_magnitude = physics.curvature_contraction * curvature;
    return (to_midpoint / dist) * force_magnitude;
}

/// Calculate SDF gradient (direction pointing outward from node surface).
fn sd_gradient(pos: vec2<f32>, node: Node) -> vec2<f32> {
    let node_center = node.position + node.size * 0.5;
    let local = pos - node_center;
    let half_size = node.size * 0.5 - vec2<f32>(node.corner_radius);

    // Distance to box edges
    let q = abs(local) - half_size;
    let outside_dist = length(max(q, vec2<f32>(0.0)));
    let inside_dist = min(max(q.x, q.y), 0.0);

    // Gradient calculation
    if (q.x > 0.0 && q.y > 0.0) {
        // Corner region
        return normalize(sign(local) * max(q, vec2<f32>(0.0)));
    } else if (q.x > q.y) {
        // Left/right edge
        return vec2<f32>(sign(local.x), 0.0);
    } else {
        // Top/bottom edge
        return vec2<f32>(0.0, sign(local.y));
    }
}

/// Calculate SDF to node (negative = inside).
fn sd_rounded_box(pos: vec2<f32>, node: Node) -> f32 {
    let node_center = node.position + node.size * 0.5;
    let local = pos - node_center;
    let half_size = node.size * 0.5 - vec2<f32>(node.corner_radius);

    let q = abs(local) - half_size;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - node.corner_radius;
}

/// Apply hard node collision constraint.
/// Returns corrected position that maintains minimum distance from all nodes.
fn apply_node_collision(pos: vec2<f32>) -> vec2<f32> {
    var corrected = pos;

    for (var i = 0u; i < physics.num_nodes; i++) {
        let node = nodes[i];
        let sdf = sd_rounded_box(corrected, node);

        if (sdf < physics.node_wrap_distance) {
            // Vertex is too close or inside - push it out
            let gradient = sd_gradient(corrected, node);
            let penetration = physics.node_wrap_distance - sdf;
            corrected = corrected + gradient * penetration;
        }
    }

    return corrected;
}

/// Edge bundling force - attract to bundle_distance, repel if closer.
fn edge_bundle_force(vertex_pos: vec2<f32>, other_pos: vec2<f32>) -> vec2<f32> {
    let delta = other_pos - vertex_pos;
    let dist = length(delta);

    if (dist > physics.edge_attraction_range || dist < 0.001) {
        return vec2<f32>(0.0, 0.0);  // Too far or same position
    }

    let direction = delta / dist;

    if (dist < physics.edge_bundle_distance) {
        // Too close: repel
        let repel_factor = (physics.edge_bundle_distance - dist) / physics.edge_bundle_distance;
        return -direction * repel_factor * physics.edge_repulsion;
    } else {
        // Attract toward bundle distance
        let attract_range = physics.edge_attraction_range - physics.edge_bundle_distance;
        let attract_factor = (dist - physics.edge_bundle_distance) / attract_range;
        return direction * attract_factor * physics.edge_attraction;
    }
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

    // Calculate vertex_t (0.0 at start, 1.0 at end) for position-dependent forces
    let vertex_t = f32(local_index) / f32(max(vertex_count - 1u, 1u));

    // --- Get neighbor positions ---
    var prev_pos = vertex.position;
    var next_pos = vertex.position;
    var has_prev = false;
    var has_next = false;

    if (local_index > 0u) {
        let prev_idx = vertex_start + local_index - 1u;
        prev_pos = vertices_in[prev_idx].position;
        has_prev = true;
    }

    if (local_index < vertex_count - 1u) {
        let next_idx = vertex_start + local_index + 1u;
        next_pos = vertices_in[next_idx].position;
        has_next = true;
    }

    // --- Contraction forces (segments try to become shorter) ---
    if (has_prev) {
        force += contraction_force(vertex.position, prev_pos);
    }
    if (has_next) {
        force += contraction_force(vertex.position, next_pos);
    }

    // --- Curvature-dependent contraction (more detail in curves) ---
    if (has_prev && has_next) {
        force += curvature_contraction_force(prev_pos, vertex.position, next_pos);
    }

    // --- Bending stiffness (smooth curves) ---
    if (has_prev && has_next) {
        force += bending_force(prev_pos, vertex.position, next_pos);
    }

    // --- Gravity ---
    force += gravity_force();

    // --- Path attraction (toward direct line between pins) ---
    force += path_attraction_force(vertex.position, edge.start_anchor, edge.end_anchor);

    // --- Pin suction (pull toward nearest anchor) ---
    force += pin_suction_force(vertex.position, edge.start_anchor, edge.end_anchor, vertex_t);

    // --- Edge bundling (attract/repel based on distance) ---
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
            force += edge_bundle_force(vertex.position, other.position);
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
    var new_position = vertex.position + new_velocity * physics.dt;

    // --- Apply hard node collision constraint (AFTER integration) ---
    // This is a constraint, not a force - vertices cannot penetrate nodes
    new_position = apply_node_collision(new_position);

    // Write output
    var out_vertex = vertex;
    out_vertex.position = new_position;
    out_vertex.velocity = new_velocity;
    vertices_out[vertex_id] = out_vertex;
}
