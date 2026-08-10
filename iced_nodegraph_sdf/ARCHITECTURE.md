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

### Math types: glam here, euclid above

This crate does all vector math with `glam::Vec2`; the widget crate above does
all of its math with `euclid`'s typed spaces. That split is deliberate, not
drift. `iced_nodegraph` juggles two coordinate spaces (screen and world), and
euclid's `ScreenPoint`/`WorldPoint` make mixing them a compile error - the bug
class its `Camera2D` exists to prevent. Inside this crate there is exactly one
space (the entry's local frame), so typed spaces would protect nothing and the
point/vector ceremony would only obscure the solver math (biarc fitting, the
boolean clipper, the SDF reference field). The seam is unit-free by design:
the public API takes plain `[f32; 2]`, and neither math library appears in any
public signature. Each `Cargo.toml` declares only its own math dependency, so
a cross-use is a compile error today; keep it that way.

## GPU pipeline

### Stage 1: Compile (CPU)

`compile_local_at` / `entry_referencing` (see `src/compile.rs`) map evaluated
arcs and styles into three flat buffers:

```
Drawable (local) + Style + translate  ->  GpuDrawEntry + GpuStyle + [GpuSegment]
```

Pure data mapping. Geometry is stored in the shape's **local** frame; the world
placement rides per-instance in `entry.translate`, evaluated as
`world_p - translate`. Buffer sizes: `GpuSegment` 64 B, `GpuDrawEntry` 64 B,
`GpuStyle` 16-byte-aligned (~340 B). `DrawData` (camera, zoom, time, grid dims,
plus the world-anchored `grid_offset`) is separate and per-draw.

**Flags set at compile time:**
- `FLAG_CLOSED` (entry): the contour is closed (fillable).
- `SEG_FLAG_SIGNED` (segment): part of a closed contour.
- `STYLE_FLAG_HAS_PATTERN` (style).

**Three deduplications run here (see `src/primitive.rs::prepare`). All three are
cross-frame: the segment/entry/style buffers are persistent arenas
(`src/pipeline/arena.rs`) whose contents never move while resident:**
- *Segment instancing*: the first instance of a shape EVER uploads its segments;
  identical instances - in any primitive, any frame - emit an entry referencing
  the shared resident range (refcounted, freed at zero).
- *Style dedup*: byte-identical compiled styles share one resident buffer slot.
- *Content-keyed residency*: a primitive whose compiled bytes (shapes,
  placements, styles) hash-match a resident block reuses it WHEREVER it sits in
  the prepare order - no evaluate, no upload; a reorder or an earlier rebuild
  invalidates nothing.
  The "WHEREVER" is the whole point: reuse used to be positional AND
  cursor-coupled - the buffers were packed front-to-back each frame, so a
  primitive kept its data only if every primitive BEFORE it was byte-identical
  too. Any reorder (a selection z-resort, a node add or remove) re-evaluated
  and re-uploaded every shifted primitive, a ~2-3 ms hitch on 500 nodes. Pure
  content keying could not fix that alone, because entries reference the
  segments and styles of OTHER primitives by absolute index, so relocated
  bytes would need reference fix-ups; the arenas exist so nothing relocates.
  A reorder frame is now 0 rebuilds and 100% resident hits.
  Blocks unused for `RESIDENT_MAX_AGE` (8) frames return
  their ranges to the arenas; when an arena's high-water mark runs far ahead of
  its live count, the residency state is dropped and the next frame rebuilds
  tightly packed (`SdfStats::arena_compactions`).
  The `Shape` recipe hash is computed once at CONSTRUCTION (each constructor and
  operator folds its params and child hashes), so `hash()` is an O(1) field read.

Alongside the entry batch, `prepare` builds the scatter work lists for Stage 2:
(draw, entry, segment) triples of open entries and (draw, entry) pairs of
closed entries, in persistent buffers with the same slot-reuse lifecycle as
the geometry; the draw's tiling entry ids (4, sentinel-padded) ride inside its
`DrawData`. Each compute pipeline binds at most 8 storage buffers - the
WebGPU spec-default per-stage limit, which wasm enforces.

Cacheable booleans are evaluated through a frame-surviving `ShapeCache` (LRU,
content-hash keyed), so a unique node body's boolean runs once across frames.

**Resident-index skip:** the tile grid is **world-anchored** — `DrawData.grid_offset`
folds the camera pan into the tile lattice, so a segment's tile membership depends
only on its world position, the zoom and a **tile-quantized window base**, never on
the continuous `camera_position`. The cull result therefore depends on the geometry
buffers and on every `DrawData` field EXCEPT `time`, `camera_position`,
`bounds_origin` and `grid_offset` (the last three only shift the window
continuously). `prepare` keys each draw on that window base plus zoom, viewport,
grid geometry and entry ranges (`cull_key`); when no key changed, no slot rebuilt
and no index buffer regrew, the whole Stage-2 dispatch is skipped and the resident
index reused — idle redraws, time-only animation frames AND **sub-tile pans** recull
nothing; a pan reculls only when it crosses a 64px coarse-tile boundary
(`SdfStats::cull_skipped`). Output stays pixel-identical across the reuse.

### The world-anchored tile lattice

Scatter and fragment must map a point to a coarse tile through the SAME
formula, and that formula must not depend on the continuous camera pan - or the
resident index could not survive one.

Decompose the pan `pan_px = camera_position * cs` per axis into whole coarse
tiles plus a remainder:

```
N   = floor(-pan_px / coarse_px)         // window base, in whole tiles
off = (-pan_px).rem_euclid(coarse_px)    // remainder, in [0, coarse_px)
```

and map with `coarse_col = floor((local_px + off) / coarse_px)`. Substituting
`local_px = world_p * cs + pan_px` and the `rem_euclid` identity collapses the
pan out of the expression entirely:

```
local_px + off = world_p * cs - N * coarse_px
coarse_col     = floor(world_p * cs / coarse_px) - N
```

The right side depends only on world position, zoom and `N` - never on
`camera_position`. The index is a window `[N, N + coarse_cols)` over a world
lattice. A segment at world `s` and a pixel at world `w` with `s ~ w` therefore
resolve to the same tile, because both sides evaluate the same expression.

`N` is computed on the CPU in f64 - the large `pan_px - N * coarse_px`
subtraction happens once, precisely - and feeds only `cull_key`. `off` reaches
the GPU as `DrawData.grid_offset`, uploaded every frame like `time`. Since `N`
changes only when the pan crosses a 64px boundary, a sub-tile pan keeps
`cull_key` and reuses the index; frame B reads `worldcell(w) - N_B` with
`N_B == N_A`, which is the tile index frame A wrote, so the reuse is
pixel-identical rather than merely dispatch-skipped.

The window spans `ceil(viewport / coarse_px) + 1` cells. World anchoring means
the viewport is never cell-aligned, so it always clips one extra cell; that
apron is the minimal price of the anchoring (`local_px` in `[0, viewport)` and
`off` in `[0, coarse_px)` bound the argument below `viewport + coarse_px`). The
fragment clamps to `grid_cols - 1` rather than discarding, against float edge
cases. At `off = 0` the formula reduces exactly to the screen-anchored one it
replaced, which is what makes the origin-centred tests a regression guard.

### Stage 2: Compute shader (GPU) — scatter-built two-level tile index

Three kernels build a two-level index, both levels persisted to storage
buffers. They replaced a GATHER cull that scanned every entry x segment from
every coarse tile - `O(tiles x entries x segments)` regardless of visibility,
a 1.2-1.6 ms zoom-independent floor on the 500-node scene. Only the iteration
direction flipped; the cull TEST (`seg_box_interval` against the style reach
band) is unchanged and still exact. Measured on that scene, the lumped index
build went 4260.9 -> 958.5 us (4.4x) at pixel-identical output.

- **Coarse** 64x64-pixel tiles (`COARSE_FACTOR = 8` fine tiles per axis). Each
  holds up to `MAX_COARSE_SLOTS = 512` `(segment_idx, entry_idx)` results (two u32
  each), sorted by entry so the fragment shader walks one shape at a time in
  z-order. Tilings are marked by `TILING_BIT` on the segment field, as before.
  Past the cap, pairs drop FIRST-COME: which pairs survive depends on the atomic
  interleave, so the surviving SET is nondeterministic even though the per-tile
  sort makes the surviving ORDER deterministic. The gather kernel used to rank
  drops keep-nearest, a policy no atomic append can express. Rather than pay for
  exact allocation speculatively, the cap carries telemetry: the demand counters
  keep counting past it, and an async readback taken between the scatter and the
  sort surfaces true per-tile demand as `SdfStats::coarse_demand_max` /
  `coarse_overflow_tiles` (one frame delayed, non-blocking). The 500-node
  overview measures ~165 slots in the busiest tile against the 512 cap and no
  tested configuration has ever overflowed - only hundreds of shapes stacked
  into one 64px tile can. Should the counter ever fire in real use, the
  escalation is exact allocation (Vello-style count + prefix scan + write, no
  cap at all); it costs roughly 2x scatter ALU, a scan dispatch, a slot buffer
  sized GPU-side behind a grow-and-recull loop, and a sort that handles
  unbounded per-tile lists past the ~2k-slot workgroup-memory limit. That price
  is deliberately unpaid until evidence demands it.
- **Fine** 8x8-pixel tiles. Each holds up to `MAX_FINE_SLOTS = 64` **16-bit**
  indices into its parent coarse tile's result, packed 2 per u32
  (`FINE_STRIDE = 32`). The fragment dereferences a fine index through the coarse
  tile to recover the `(segment, entry)`. Past the cap `fine_push` drops one
  candidate per call - it evicts the resident slot furthest from the tile
  centre, or rejects the newcomer. A second, OPT-IN async readback
  (`crate::set_index_probe`) reports both halves of the packed `fine_counts`
  word: the low 16 bits are the live slot count, the high 16 bits the dropped
  count. `Sum(counts) * 64` (pixels per fine tile) is the `eval_segment` count
  shader performs per frame (`SdfStats::segment_evals` / `fine_slots_max` /
  `fine_live_tiles`), and `fine_evicted_tiles` / `fine_evicted_slots` flag tiles
  whose slot list is INCOMPLETE - a dropped segment that would have been the
  per-pixel nearest renders a wrong distance, so that counter is a correctness
  signal, not a quality knob. Off by default: it copies 4 bytes per fine tile
  per culled frame.

The split trades one indirection for memory: the fat coarse slots exist once per
(few) coarse tiles; the 64x-more-numerous fine tiles cost two bytes per slot, not
eight.

**The build SCATTERS work-proportionally instead of gathering per tile.** The
former gather kernel scanned every entry x segment from every coarse tile -
O(tiles x segments) regardless of visibility, a zoom-independent cost floor.
The scatter build's work is proportional to actual segment-tile overlaps:

1. `cs_scatter_open` - one thread per (draw, entry, segment) triple of an OPEN
   entry. The segment's conservative bbox (arc sub-chord endpoints inflated by
   sagitta), inflated by the style reach, is mapped to the draw's coarse tile
   range; each tile in range gets the exact `seg_box_interval` test and, on
   pass, an atomic append of the (segment, entry) slot. Short biarc pieces
   follow the curve, so a diagonal edge never floods its whole bbox.
2. `cs_scatter_closed` - one 64-thread workgroup per CLOSED entry. Thread 0
   folds the contour bbox (the interior lies inside it, so bbox iteration
   cannot miss interior-only tiles); the reach-inflated, grid-clipped range is
   strided across threads, each running the exact per-entry test (band reach
   OR centre-sign interior keep) and appending every segment that can be the
   per-pixel nearest anywhere in the tile.
3. `cs_sort_fine` - one 64-thread workgroup per LIVE coarse tile, dispatched
   1D-flat; each workgroup binary-searches its owning draw over the draws'
   coarse-base prefix sums, so no workgroup is dead on arrival (the old
   per-draw-grid dispatch launched the largest draw's grid for every draw).
   Loads the scattered slots, appends the draw's tilings,
   bitonic-sorts by (entry, seg) - a unique total order, so the frame is
   DETERMINISTIC regardless of atomic append order - writes the sorted list
   back, then all 64 threads re-cull one 8px fine tile each and write the
   16-bit references (keep-nearest at the 64 cap).

Coarse overflow past 512 drops slots first-come (the old single-threaded
keep-nearest ranking is not expressible with atomic appends); the doubled cap
makes that pathological-only, and the count keeps rising past the cap so true
demand stays observable. The counts buffer is cleared by
`CommandEncoder::clear_buffer` before the pass; all dispatches share one
compute pass and one `queue.submit` per frame.

**Cull contract (the load-bearing invariant), applied at BOTH levels.** For each
(segment, tile box) the cull computes the exact distance **interval** `[m, M]` the
segment takes over the whole box (`seg_box_interval`), and keeps the segment iff
that interval overlaps the style's reach band. The cull must be a conservative
**over**-approximation:

- `m` is a guaranteed lower bound, `M` a guaranteed upper bound on the distance.
- For line and point the interval is exact (distance to a convex set is convex, so
  the max over the box is at a corner). For an arc (non-convex) it is bounded by
  splitting the arc into shallow sub-chords.
- Over-inclusion is free (a far segment renders alpha 0 per pixel). Under-inclusion
  is a hole. Never under-include.
- A closed fill whose interior covers the tile but whose contour is far is kept via
  the nearest-segment sign at the tile centre, trusted only far from the contour.
- Fine-level eviction (when a fine tile exceeds its 64 references) is
  keep-nearest, ranked by `|distance|` at the tile centre, so the segments that
  dominate the tile's pixels survive.

### Stage 3: Fragment shader (GPU) — per-pixel rendering

`fs_main` runs per pixel:

1. Transform the pixel to world coordinates.
2. Look up its fine tile, and dereference each 16-bit slot through the parent
   coarse tile to recover its `(segment, entry)`.
3. For each entry (shape) in the resolved slot list, front to back:
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
- No zoom-quantized tile bands: the lattice is anchored in PAN only, so any
  zoom change reculls.
- No LOD tile hierarchy and no sparse GPU hash: the index is a single dense
  window over one world lattice.

The last two are deliberate, and the reason is worth writing down because it is
not "we never planned it". Pan anchoring is PROVABLY pixel-identical (the
substitution above). Tile SIZE is not, because both slot caps are calibrated to
it. Coarser tiles crowd more segments into a tile, pushing the coarse level
toward its first-come drop; and they weaken the fine level's keep-nearest
ranking, which ranks by distance at the tile CENTRE - a heuristic that holds
only while the tile is small, and whose failure punches a hole for a
centre-distant pixel without ever hitting the cap. A zoom band can therefore
only be cleared by MEASUREMENT: sweep `demos/500_nodes` across the zoom range
and log `coarse_demand_max` / `coarse_overflow_tiles` at the top of each band;
a band may land only if overflow stays 0 with headroom under the cap. Note that
HALVING the fine tile moves the safe way on both counts, which is why that
change needed no such gate - only the memory budget moved. A real LOD hierarchy
is a larger step again: it replaces the dense arena with an atomic-insert hash
whose collision handling must re-prove the frame determinism the bitonic sort
gives today, and it is only worth it once zoomed-out overview has to stop
falling back to `grid_cols = 0` (iterate all entries).

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
| `src/pipeline/arena.rs` | range allocator for the persistent geometry arenas |
| `src/pipeline/overflow.rs` | async index readbacks: coarse-slot demand (always on) and per-fine-tile slot counts (opt-in) |
| `src/pipeline/pixel_tests.rs` | headless pixel-level rendering tests |
