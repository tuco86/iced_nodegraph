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
//! (range [9.18, 9.54] ms), dominated by the `difference_many` boolean.
//!
//! NODE-BODY dedup in ISOLATION (`v2` vs `v3_cached` measured BEFORE A4 wired
//! arc-spline edges, when `Curve::bezier` just stored 4 control points): v3
//! fetches the node silhouette from the frame-surviving `ShapeCache` (one boolean
//! for all identical bodies) and places it by translate, instead of re-running
//! the boolean per node:
//!   - 500 nodes:  ~9.56 ms -> ~0.48 ms  (~20x), 2000 nodes ~18x.
//! For the NODE BOOLEAN this is the CPU half of the order-of-magnitude and meets
//! it. But it is NOT the full-scene number - see the post-A4 correction below.
//!
//! POST-A4 CORRECTION (this benchmark, WITH edges, current measurement). A4 made
//! `Curve::bezier` fit an arc-spline (biarc subdivision) on the CPU to delete the
//! per-pixel cubic solver from the shader - moving edge cost from GPU-per-pixel to
//! CPU-per-frame-build. The scene here has 640 edges (1.28/node), so the edge
//! build now DOMINATES the v3 prepare and is a shared floor for both paths:
//!   - 500 nodes:  v2 ~14.2 ms -> v3 ~4.34 ms  (~3.3x)
//!   - of v3's 4.34 ms, ~3.8 ms is the 640-edge biarc build (~6 us/edge); the
//!     deduped node bodies are <0.5 ms (the 20x above, now a small slice).
//! So a realistic edged graph sees ~3x on CPU prepare, NOT 10x: the node-boolean
//! dedup win is real but the arc-spline edge BUILD became the new bottleneck (the
//! plan assumed edges stay cheap to rebuild; A4 invalidated that premise - the
//! plan's "look again" signal). The order-of-magnitude holds on the edgeless
//! node-only scene (`frame_time_v3_not_slower_than_v2`, ~12x) and on GPU memory
//! (instancing). Cutting the edge floor (cache static-edge arc-splines by
//! endpoint, or cheapen biarc) is the open follow-up.
//!
//! FULL-FRAME order-of-magnitude MET (`frame_time_v3_not_slower_than_v2` in the
//! sdf crate drives the real `SdfPipeline`: build + upload + cull + render +
//! GPU fence). Once the per-entry uploads are batched into one bulk write per
//! buffer (the per-entry `queue.write_buffer` submission overhead, not the
//! boolean, was the last cost), 500 nodes this machine:
//!   - per-frame WORK (build+prepare, no fence): v2 ~7.7 ms -> v3 ~0.61 ms (~12.7x)
//!   - full frame wall-clock (with GPU fence):   v2 ~7.9 ms -> v3 ~0.87 ms (~9x)
//!   - cull GPU-only (R3 timestamps):            ~0.05 ms either way (negligible)
//!
//! The R3 timestamps prove the GPU cull was never the bottleneck; the win is the
//! deduped boolean + batched upload. The remaining fragment cost is the
//! fullscreen background tiling (per-node prims are scissored), addressed by the
//! static-background texture cache - not by dedup.
//!
//! GPU MEMORY half (instancing, proven by `gpu_instancing_shares_segment_range`):
//! with the per-instance translate on the command, N identical node bodies upload
//! ONE shape's segments instead of N copies. For 500 identical nodes that is a
//! ~500x reduction in per-frame segment-buffer upload - the memory-bandwidth axis
//! the field-reported iGPU bottleneck is bound on. The GPU fragment/compute TIME
//! half is unchanged by instancing (same tiles) and needs Phase C (two-level
//! tiling + layer collapse) plus R3 timestamps to reduce and record.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use iced::Color;
use iced_nodegraph_sdf::{Curve, Pattern, SdfPrimitive, ShapeCache, ShapeExpr, Style, boolean};
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

/// The position-free node-body recipe (v3 keystone): identical for every node,
/// so the boolean evaluates ONCE and every other node is a cache hit. Pin cuts
/// sit at LOCAL offsets relative to the body center.
fn node_body_recipe() -> ShapeExpr {
    ShapeExpr::Difference {
        base: Box::new(ShapeExpr::RoundedRect {
            half: [NODE_W * 0.5, NODE_H * 0.5],
            radius: CORNER_RADIUS,
        }),
        cuts: vec![
            ShapeExpr::Circle {
                center: [-NODE_W * 0.5, -25.0],
                radius: PIN_RADIUS,
            },
            ShapeExpr::Circle {
                center: [-NODE_W * 0.5, 0.0],
                radius: PIN_RADIUS,
            },
            ShapeExpr::Circle {
                center: [-NODE_W * 0.5, 25.0],
                radius: PIN_RADIUS,
            },
            ShapeExpr::Circle {
                center: [NODE_W * 0.5, -15.0],
                radius: PIN_RADIUS,
            },
            ShapeExpr::Circle {
                center: [NODE_W * 0.5, 15.0],
                radius: PIN_RADIUS,
            },
        ],
    }
}

/// v3 per-frame CPU work: the node silhouette is fetched from the shape cache
/// (one boolean for all identical bodies) and placed by translate, instead of
/// re-running `difference_many` per node. Edges remain per-frame (ephemeral).
fn prep_frame_cached(scene: &Scene, cache: &mut ShapeCache) -> SdfPrimitive {
    let fill = Style::solid(Color::from_rgb(0.2, 0.2, 0.25));
    let border = Style::stroke(Color::from_rgb(0.6, 0.6, 0.7), Pattern::solid(2.0));
    let shadow = Style::shadow(Color::from_rgba(0.0, 0.0, 0.0, 0.5), 12.0);
    let edge_style = Style::stroke(Color::from_rgb(0.5, 0.7, 0.9), Pattern::solid(2.0));

    let recipe = node_body_recipe();
    let mut prim = SdfPrimitive::with_capacity(scene.nodes.len() * 3 + scene.edges.len());

    for node in &scene.nodes {
        // Cached local silhouette (one boolean for all nodes) placed by translate.
        let outline = cache.get_or_eval(&recipe).translated(node.cx, node.cy);
        prim.push(&outline.translated(4.0, 4.0), &shadow);
        prim.push(&outline, &fill);
        prim.push(&outline, &border);
    }

    for &(from, to) in &scene.edges {
        let a = &scene.nodes[from];
        let b = &scene.nodes[to];
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
        // v2: world-baked, `difference_many` per node every frame.
        group.bench_with_input(BenchmarkId::new("v2", n), &scene, |b, scene| {
            b.iter(|| black_box(prep_frame(black_box(scene))));
        });
        // v3: node bodies deduped through the shape cache (one boolean total).
        group.bench_with_input(BenchmarkId::new("v3_cached", n), &scene, |b, scene| {
            let mut cache = ShapeCache::new(64);
            b.iter(|| black_box(prep_frame_cached(black_box(scene), &mut cache)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_frame_prep);
criterion_main!(benches);
