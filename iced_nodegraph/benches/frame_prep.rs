//! CPU-side shape-evaluation benchmark.
//!
//! Measures the CPU geometry work that survives in the single-renderer SDF
//! pipeline: materialising a `Shape` recipe into arcs (`Shape::evaluate` - the
//! rounded-box body with its pin cutouts subtracted via `difference_many`, the
//! boolean the widget's comments flag as the expensive one), and stroking the
//! edge beziers (the biarc spline build). It contrasts evaluating every node
//! COLD against fetching it through the frame-surviving `ShapeCache`, so the
//! delta is exactly the dedup win: N identical node bodies pay for ONE boolean.
//!
//! Scope. In the current architecture `SdfPrimitive::push` only stores the
//! position-free `Shape`; the boolean/biarc evaluation runs inside the GPU
//! pipeline's `prepare`, deduped per frame by recipe hash. This bench isolates
//! that CPU evaluation headlessly (no device) by calling `evaluate` /
//! `ShapeCache::get_or_eval` directly. It does NOT cover iced's `layout` pass,
//! the GPU tile cull (a compute shader), or upload/present - the full-frame
//! wall-clock story lives in the `iced_nodegraph_sdf` pipeline tests.
//!
//! Recorded context (this machine, 500 nodes / 640 edges). The node-body dedup
//! collapses the per-frame boolean from one-per-node to one total (~20x on the
//! node bodies in isolation). Post-A4 the biarc edge build (~6 us/edge) became
//! the dominant CPU term for an edged scene, so a realistic graph sees ~3x on
//! CPU evaluation, not 10x - the dedup win is real but bounded by the edge
//! floor. Caching static-edge arc-splines by endpoint is the open follow-up.
//!
//! Run with: `cargo bench -p iced_nodegraph --bench frame_prep`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use iced_nodegraph_sdf::{Shape, ShapeCache};
use std::hint::black_box;

/// Edges per node, matching the README's "500 nodes, 640 edges" demo (~1.28).
const EDGES_PER_NODE: usize = 128;
const NODE_W: f32 = 160.0;
const NODE_H: f32 = 90.0;
const CORNER_RADIUS: f32 = 8.0;
const PIN_RADIUS: f32 = 5.0;

struct NodeSpec {
    cx: f32,
    cy: f32,
}

struct Scene {
    nodes: Vec<NodeSpec>,
    edges: Vec<(usize, usize)>,
}

/// Lay nodes out on a grid and connect each to a deterministic spread of
/// others, so the scene is fixed across iterations (no RNG, no per-run drift).
fn build_scene(n: usize) -> Scene {
    let cols = (n as f32).sqrt().ceil() as usize;
    let spacing_x = NODE_W * 1.6;
    let spacing_y = NODE_H * 1.8;
    let nodes = (0..n)
        .map(|i| NodeSpec {
            cx: (i % cols) as f32 * spacing_x,
            cy: (i / cols) as f32 * spacing_y,
        })
        .collect();

    let edge_count = n * EDGES_PER_NODE / 100;
    let edges = (0..edge_count)
        .map(|e| {
            let from = e % n;
            // Deterministic, well-spread target distinct from `from`.
            let to = (from + 1 + (e * 7 + 3) % (n.max(2) - 1)) % n;
            (from, to)
        })
        .collect();

    Scene { nodes, edges }
}

/// The position-free node-body recipe (the dedup keystone): identical for every
/// node, so the boolean evaluates ONCE and every other node is a cache hit. The
/// box is centred and the pin cuts sit at LOCAL offsets from the centre.
fn node_body() -> Shape {
    Shape::rounded_box([NODE_W, NODE_H], [CORNER_RADIUS; 4])
        - Shape::circle(PIN_RADIUS).translate([-NODE_W * 0.5, -25.0])
        - Shape::circle(PIN_RADIUS).translate([-NODE_W * 0.5, 0.0])
        - Shape::circle(PIN_RADIUS).translate([-NODE_W * 0.5, 25.0])
        - Shape::circle(PIN_RADIUS).translate([NODE_W * 0.5, -15.0])
        - Shape::circle(PIN_RADIUS).translate([NODE_W * 0.5, 15.0])
}

/// Horizontal-tangent bezier between two nodes - the edge shape the widget
/// renders, rebuilt every frame because its endpoints move (ephemeral).
fn edge_shape(scene: &Scene, from: usize, to: usize) -> Shape {
    let a = &scene.nodes[from];
    let b = &scene.nodes[to];
    let dx = (b.cx - a.cx).abs().max(NODE_W);
    Shape::bezier(
        [a.cx + NODE_W * 0.5, a.cy],
        [a.cx + NODE_W * 0.5 + dx * 0.5, a.cy],
        [b.cx - NODE_W * 0.5 - dx * 0.5, b.cy],
        [b.cx - NODE_W * 0.5, b.cy],
    )
}

/// Cold per-frame CPU work: re-run the silhouette boolean for EVERY node (no
/// dedup), then build each edge spline. The returned segment tally is a sink so
/// the optimiser cannot elide the evaluations.
fn prep_frame(scene: &Scene) -> usize {
    let body = node_body();
    let mut segs = 0usize;
    for _ in &scene.nodes {
        segs += body.evaluate().segment_count();
    }
    for &(from, to) in &scene.edges {
        segs += edge_shape(scene, from, to).evaluate().segment_count();
    }
    segs
}

/// Deduped per-frame CPU work: the node silhouette comes from the shape cache
/// (one boolean for all identical bodies); edges remain per-frame (ephemeral).
fn prep_frame_cached(scene: &Scene, cache: &mut ShapeCache) -> usize {
    let body = node_body();
    let mut segs = 0usize;
    for _ in &scene.nodes {
        segs += cache.get_or_eval(&body).segment_count();
    }
    for &(from, to) in &scene.edges {
        segs += edge_shape(scene, from, to).evaluate().segment_count();
    }
    segs
}

fn bench_frame_prep(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_prep");
    for &n in &[100usize, 500, 2000] {
        let scene = build_scene(n);
        // Cold: the silhouette boolean runs per node every frame.
        group.bench_with_input(BenchmarkId::new("cold", n), &scene, |b, scene| {
            b.iter(|| black_box(prep_frame(black_box(scene))));
        });
        // Cached: node bodies deduped through the shape cache (one boolean total).
        group.bench_with_input(BenchmarkId::new("cached", n), &scene, |b, scene| {
            let mut cache = ShapeCache::new(64);
            b.iter(|| black_box(prep_frame_cached(black_box(scene), &mut cache)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_frame_prep);
criterion_main!(benches);
