# iced_nodegraph_sdf Architecture

This is the precise reference: the data model, the GPU pipeline, and the
invariants the implementation must hold. For the narrative walkthrough of *why*
the design looks like this, with diagrams, read [`README.md`](README.md) first.

## Purpose

Render 2D signed distance fields on the GPU via a tile-based spatial index. The
pipeline takes one geometric primitive (the circular arc) with a styling profile
(a distance-stop chain plus an optional stroke pattern) and produces
pixel-accurate, antialiased, resolution-independent output.

## Data model

### One primitive: the arc

There is exactly one drawn geometric primitive, the `Segment` (see
`src/drawable.rs`), encoded by its two endpoints plus a signed curvature
`k = 1/r`. Its three forms are degenerates of the same encoding:

| Form | Condition | Geometry |
|------|-----------|----------|
| **Line** | `k == 0` | straight `start -> end` (radius is infinite) |
| **Point** | `start == end` | zero-length junction; `heading` orients its sign |
| **Arc** | otherwise | minor arc (`|sweep| < pi`), radius `1/|k|`, bulge side from `sign(k)` |

There is no separate Line / Cubic / Point type. Stored arcs are always *minor*;
a wider sweep (a full-circle pin is `2*pi`) is split into minor sub-arcs before
storage (full circle -> four quarters), so the minor-arc reconstruction in the
distance field is unambiguous.

The distance field (`src/segment.rs::seg_sdf`, mirrored in the WGSL
`eval_segment`) returns a **signed** distance: negative on the right side of the
travel direction (the interior of a clockwise contour), positive on the left.
Endpoints + curvature is chosen over center/radius/sweep because a line is a
clean degenerate (`k = 0`, not `r -> infinity`) and the encoding stays in the
segment's own coordinate range, avoiding far-from-origin precision loss.

### Drawable types

A `Drawable` (the evaluated geometry) is one of three entry types, matching the
GPU `entry_type` discriminant:

| Type | Description | Segments |
|------|-------------|----------|
| **CurveSegment** (0) | one open stroke (line / arc / arc-splined bezier) | 1..N |
| **Shape** (1) | a closed contour, optionally compound (booleans) | N |
| **Tiling** (2) | an infinite analytic background | 0 (parametric) |

Cubic beziers never reach the GPU as cubics: they are fitted to an arc-spline on
the CPU (`src/biarc.rs`) within a sub-pixel world-space tolerance, and each arc
carries its exact arc length so dash/flow parametrisation matches the cubic.

### Closed shapes via set algebra

Compound closed shapes (a node body minus its pin cutouts) are built with boolean
operations on contours (`src/boolean.rs`): the operands are clipped against each
other and the surviving boundary is re-stitched into a single clean loop of arcs
with `Point` junctions at corners. Combining is *not* `min`/`max` of fields —
that would seam and mis-sign concave corners. Only `Line` and `Arc` segments
participate as boolean operands.

### Styles: a distance-stop chain

A `Style` (see `src/style.rs`) maps signed distance to colour via a chain of up to
`MAX_STOPS` (8) `Stop`s, each placed at a signed distance and carrying an
arc-length colour pair (`start` at arc 0, `end` at arc 1). Evaluation at signed
distance `d`:

- `d <= stops[0].dist`: hold the first stop (clamped).
- between consecutive stops: `smoothstep`-blend, the transition widened to at
  least one pixel so a zero-width step is a crisp antialiased edge.
- `d >= stops[last].dist`: hold the last stop (clamped).

The whole profile is one entry, blended in premultiplied space, so abutting bands
never composite against each other and cannot seam. Fills, glows, blurs, and
bands are all expressed as stop chains (see the `Style` constructors).

| Field | Purpose |
|-------|---------|
| `stops` | the distance profile (ascending by `dist`, never empty) |
| `pattern` | optional: reshapes distance along the contour (stroke layout) |
| `transfer` | colour-domain warp on the blend parameter (linear / smoothstep / gamma) |
| `distance_field` | special IQ visualization mode (ignores the stop colours) |

### Patterns

A `Pattern` (see `src/pattern.rs`) transforms the raw distance into a stroke-space
distance before the stop lookup, using the segment's arc-length `u` for layout:

| Pattern | Effect |
|---------|--------|
| Solid | `abs(dist) - thickness/2` |
| Dashed | sheared 2D box SDF along `u` (angle tilts the caps) |
| Arrowed | symmetric-angle dashes |
| Dotted | circular dots spaced along `u` |
| DashDotted / ArrowDotted | alternating strokes and dots |

A non-zero `flow_speed` shifts `u` by `time * flow_speed` for animated flow.

## GPU pipeline

### Stage 1: Compile (CPU)

`compile_local_at` / `entry_referencing` (see `src/compile.rs`) map evaluated
arcs and styles into three flat buffers:

```
Drawable (local) + Style + translate  ->  GpuDrawEntry + GpuStyle + [GpuSegment]
```

Pure data mapping. Geometry is stored in the shape's **local** frame; the world
placement rides per-instance in `entry.translate`, evaluated as
`world_p - translate`. Buffer sizes: `GpuSegment` 64 B, `GpuDrawEntry` 80 B,
`GpuStyle` 16-byte-aligned (~340 B). `DrawData` (camera, zoom, time, debug flags,
grid dims) is separate and per-draw.

**Flags set at compile time:**
- `FLAG_CLOSED` (entry): the contour is closed (fillable).
- `SEG_FLAG_SIGNED` (segment): part of a closed contour.
- `STYLE_FLAG_HAS_PATTERN` / `STYLE_FLAG_DISTANCE_FIELD` (style).

**Three deduplications run here (see `src/primitive.rs::prepare`):**
- *Segment instancing*: the first instance of a shape this frame uploads its
  segments; identical instances emit an entry referencing the shared range.
- *Style dedup*: byte-identical compiled styles share one buffer slot.
- *Geometry-hash slot reuse*: a primitive byte-identical to last frame (shapes,
  placements, styles) skips evaluate + upload entirely and reuses resident data.

Cacheable booleans are evaluated through a frame-surviving `ShapeCache` (LRU,
content-hash keyed), so a unique node body's boolean runs once across frames.

### Stage 2: Compute shader (GPU) — tile spatial index

`cs_build_index` (workgroup `16x16`) builds, per 16x16-pixel tile, the list of
segments that can colour any of its pixels. Slots are `(segment_idx, entry_idx)`
pairs (`MAX_SLOTS_PER_TILE = 128`), sorted by entry so the fragment shader walks
one shape at a time in z-order.

**Two-level cull.** Each workgroup first cooperatively bins entries whose world
AABB reaches its 256x256-pixel region into workgroup memory
(`MAX_WG_CANDIDATES = 256`); each fine tile then scans only those candidates. If a
region overflows the candidate bin, that region falls back to scanning every entry
(correctness over the fast path).

**Cull contract (the load-bearing invariant).** For each (segment, tile) the cull
computes the exact distance **interval** `[m, M]` the segment takes over the whole
tile box (`seg_box_interval`), and keeps the segment iff that interval overlaps the
style's reach band. The cull must be a conservative **over**-approximation:

- `m` is a guaranteed lower bound, `M` a guaranteed upper bound on the distance.
- For line and point the interval is exact (distance to a convex set is convex, so
  the max over the box is at a corner). For an arc (non-convex) it is bounded by
  splitting the arc into shallow sub-chords.
- Over-inclusion is free (a far segment renders alpha 0 per pixel). Under-inclusion
  is a hole. Never under-include.
- A closed fill whose interior covers the tile but whose contour is far is kept via
  the nearest-segment sign at the tile centre, trusted only far from the contour.

The whole frame's culls run as **one** dispatch: the draw index is the dispatch
z-axis (`workgroup_id.z`), so each draw reads its own `DrawData` with no per-draw
uniform, and the frame issues one `queue.submit`.

### Stage 3: Fragment shader (GPU) — per-pixel rendering

`fs_main` runs per pixel:

1. Transform the pixel to world coordinates.
2. Look up its tile in the spatial index.
3. For each entry (shape) in the tile's slot list, front to back:
   a. fold to the **nearest segment** (minimum `abs(dist)`) over that entry's slots,
      evaluated at `world_p - entry.translate`;
   b. call `render_style` with the nearest segment's signed distance.
4. Accumulate fragments with premultiplied-alpha blending, with an early-out once
   the pixel is opaque (`acc.a >= ~1`).

When `grid_cols == 0` (a draw whose tile region would exceed the device storage
limit, e.g. many large overlapping primitives) the shader falls back to iterating
all of that draw's entries with the same nearest-segment fold.

### `render_style` contract

- **Pattern styles**: `apply_pattern` reshapes the distance to stroke-space (using
  `abs(dist)` internally, sign-invariant) before the colour lookup; the colour
  comes from the arc-length gradient of the first stop.
- **Stop-chain styles** (fills, glows, blurs): the piecewise-`smoothstep` fold over
  the stops, in premultiplied space. A closed contour's nearest-segment field is
  already signed, so the fill and its silhouette come from the same field — no
  separate fill pass, no winding count. On an *open* curve the signed field is
  one-sided; non-pattern styles on open curves are therefore asymmetric by design.
- **Distance field**: IQ visualization — colour from sign, contour lines from
  `cos`, white highlight at `dist = 0`.

**Antialiasing** is analytic: the contour field has unit gradient, so one screen
pixel is `1/(zoom * scale)` world units and the AA band is a `smoothstep` over that
width. It is computed analytically, not with `fwidth`, because the per-tile loop is
data-dependent and screen-space derivatives are undefined in non-uniform control
flow (which produced a 1px tile-boundary seam on some GPUs).

## Invariants

1. **Segment distance is signed.** Sign comes from the perpendicular side of the
   travel direction; `eval_segment` applies it unconditionally on CPU and GPU.
2. **Stored arcs are minor (`|sweep| < pi`).** Wider sweeps are split before storage.
3. **Pattern functions are sign-invariant.** They operate on `abs(dist)`.
4. **The tile cull is conservative.** Include if the segment *might* affect any
   pixel; false positives are acceptable, false negatives are holes.
5. **Style rendering is tile-independent.** Given the same segments, a pixel's
   result does not depend on which tile owns it.
6. **No special-case flags in the segment evaluator.** The geometry (curvature,
   start==end), not a type tag, selects the line / arc / point branch; behavioural
   differences come from the style.
7. **Placement is translation-only and distance-preserving.** A shape's rendered
   result is independent of its per-instance translate, which is what lets
   identical shapes share evaluated geometry.
8. **The recipe hash addresses the definition, not the output.** It is
   placement-independent and identical on native and wasm.

## What this pipeline does NOT do

- No per-drawable unsigned mode: open curves get a signed (one-sided) field.
- No special handling for overlapping drawables: each is independent; compositing
  is purely alpha, front to back.
- No GPU cubic-bezier evaluator: cubics are arc-splined on the CPU.
- No `min`/`max` field compositing for compound shapes: booleans re-stitch one
  contour.
- No antialiasing beyond the analytic `smoothstep` band: no MSAA, no temporal AA.

## File map

| File | Responsibility |
|------|----------------|
| `src/shape.rs` | `Shape` recipe tree, content hash, `ShapeCache` |
| `src/segment.rs` | the arc encoding and its reference distance field |
| `src/biarc.rs` | cubic bezier -> arc-spline fit |
| `src/curve.rs` | `Curve` / `ShapeBuilder` geometry construction |
| `src/drawable.rs` | compiled `Segment` + `Drawable`, bounds, arc-length |
| `src/boolean.rs` | union / difference / intersection on closed contours |
| `src/tiling.rs` | infinite analytic background factories |
| `src/style.rs` | the distance-stop `Style` system + `Stop` / `Transfer` |
| `src/pattern.rs` | stroke `Pattern`s and GPU parameter encoding |
| `src/color.rs` | `ColorQuad`, the four-corner colour field |
| `src/compile.rs` | arcs + styles -> GPU structs |
| `src/shared.rs` | shared GPU resources (shader module, layouts, pipelines) |
| `src/primitive.rs` | `SdfPrimitive` + `SdfPipeline` (prepare / deferred compute / draw) |
| `src/pipeline/shader.wgsl` | all GPU code (vertex, fragment, compute) |
| `src/pipeline/types.rs` | GPU struct layouts (must match the WGSL) |
| `src/pipeline/buffer.rs` | dynamic GPU buffer wrapper |
| `src/pipeline/pixel_tests.rs` | headless pixel-level rendering tests |
