//! CPU-side per-frame work benchmark.
//!
//! Measures the SDF geometry preparation that dominates `NodeGraph`'s
//! per-frame CPU cost: building each node silhouette (a rounded-rect body with
//! the pin cutouts subtracted via the boolean `difference_many`, the operation
//! the widget's own comments flag as the expensive one), layering its
//! shadow/fill/border, and stroking the edge beziers - all batched into one
//! `SdfPrimitive`, exactly as `NodeGraph::draw` assembles a frame.
//!
//! This is the same work the `info()` callback times at runtime, isolated from
//! the GPU/present path so it runs headlessly and reproducibly. It does NOT
//! include iced's `layout()` pass (cheap relative to the geometry) or the
//! per-pixel tile culling, which happens on the GPU (a compute shader), not the
//! CPU.
//!
//! Run with: `cargo bench -p iced_nodegraph --bench frame_prep`
//! (target the bench explicitly; a plain `cargo bench` also compiles the lib
//! unit tests, where an unrelated mock currently fails under the bench profile.)
//!
//! v2 BASELINE (frozen for the SDF v3 rewrite, STEP 0). 500 nodes / 640 edges,
//! post pin-pulse removal, this machine: CPU prepare ~9.35 ms
//! (range [9.18, 9.54] ms), dominated by the `difference_many` boolean. This is
//! the CPU half of the order-of-magnitude target (cut to ~1 ms). The GPU half
//! (fragment + compute via timestamp queries, R3) is recorded separately when
//! the GPU-path benchmark lands.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use iced::Color;
use iced_nodegraph_sdf::{Curve, Pattern, SdfPrimitive, Style, boolean};
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

/// The per-frame CPU work: rebuild every node silhouette and edge curve and
/// batch them into one primitive, as `draw` does each frame.
fn prep_frame(scene: &Scene) -> SdfPrimitive {
    let fill = Style::solid(Color::from_rgb(0.2, 0.2, 0.25));
    let border = Style::stroke(Color::from_rgb(0.6, 0.6, 0.7), Pattern::solid(2.0));
    let shadow = Style::shadow(Color::from_rgba(0.0, 0.0, 0.0, 0.5), 12.0);
    let edge_style = Style::stroke(Color::from_rgb(0.5, 0.7, 0.9), Pattern::solid(2.0));

    let mut prim = SdfPrimitive::with_capacity(scene.nodes.len() * 3 + scene.edges.len());

    for node in &scene.nodes {
        let body = Curve::rounded_rect(
            [node.cx, node.cy],
            [NODE_W * 0.5, NODE_H * 0.5],
            CORNER_RADIUS,
        );
        // Pin cutouts: three on the left edge, two on the right.
        let cuts = [
            Curve::circle([node.cx - NODE_W * 0.5, node.cy - 25.0], PIN_RADIUS),
            Curve::circle([node.cx - NODE_W * 0.5, node.cy], PIN_RADIUS),
            Curve::circle([node.cx - NODE_W * 0.5, node.cy + 25.0], PIN_RADIUS),
            Curve::circle([node.cx + NODE_W * 0.5, node.cy - 15.0], PIN_RADIUS),
            Curve::circle([node.cx + NODE_W * 0.5, node.cy + 15.0], PIN_RADIUS),
        ];
        let outline = boolean::difference_many(&body, &cuts);

        // Shadow reuses the silhouette shifted by the shadow offset (as the
        // widget does), then fill and border paint over it.
        prim.push(&outline.translated(4.0, 4.0), &shadow);
        prim.push(&outline, &fill);
        prim.push(&outline, &border);
    }

    for &(from, to) in &scene.edges {
        let a = &scene.nodes[from];
        let b = &scene.nodes[to];
        // Horizontal-tangent bezier, the edge shape the widget renders.
        let dx = (b.cx - a.cx).abs().max(NODE_W);
        let curve = Curve::bezier(
            [a.cx + NODE_W * 0.5, a.cy],
            [a.cx + NODE_W * 0.5 + dx * 0.5, a.cy],
            [b.cx - NODE_W * 0.5 - dx * 0.5, b.cy],
            [b.cx - NODE_W * 0.5, b.cy],
        );
        prim.push(&curve, &edge_style);
    }

    prim
}

fn bench_frame_prep(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_prep");
    for &n in &[100usize, 500, 2000] {
        let scene = build_scene(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &scene, |b, scene| {
            b.iter(|| black_box(prep_frame(black_box(scene))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_frame_prep);
criterion_main!(benches);
