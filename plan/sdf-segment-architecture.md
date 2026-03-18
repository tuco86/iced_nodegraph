# Segment-Based SDF Architecture

## Problem

CSG operations (`min`, `max`) on SDFs produce inexact distance fields. The distance is correct at the surface (`dist = 0`) but increasingly wrong further away. This affects blur, gradients, expand, and any effect that depends on accurate distance values.

Example: `rounded_box - circle` (node with pin cutouts) has distorted blur falloff near the cutouts because `max(dist_box, -dist_circle)` underestimates the true distance to the compound surface.

## Insight

A compound shape's surface consists of **segments from the original shapes**. After `A - B`, the visible surface is:
- Parts of A's boundary where B doesn't intersect
- Parts of B's boundary where B intersects A

These segments are simple geometric primitives (line segments, arcs, bezier curves) with **exact** distance functions. If the compound is decomposed into its constituent segments, `min()` over non-overlapping segments produces exact distances.

## Proposed Architecture

### Two Compilation Modes

```
CSG Mode (current):
    compile(box - circles) -> [Box, Circle, Circle, ..., Subtract, Subtract, ...]
    + Fast compilation
    + Simple API
    - Inexact distance beyond surface

Segment Mode (new):
    compile(box - circles) -> [LineSeg, Arc, LineSeg, Arc, LineSeg, Arc, ...]
    + Exact distance everywhere
    + Per-segment spatial index culling (fewer evals per tile)
    - Compilation cost (surface decomposition)
    - Only works for known primitive intersections
```

### Segment Primitives

All 2D compound surfaces can be represented with:

1. **Line segment** (A to B) - already exists as `Sdf::line`
2. **Circular arc** (center, radius, start_angle, end_angle) - new primitive
3. **Cubic bezier segment** (p0, p1, p2, p3) - already exists as `Sdf::bezier`

Optional for completeness:
4. **Quadratic bezier segment** - already exists
5. **Elliptical arc** - for ellipse intersections (rare)

With line + arc + cubic bezier, virtually any 2D shape boundary can be described.

### Decomposition Pipeline

```
Sdf::rounded_box([0,0], [120,80], 8.0) - Sdf::circle([-120, 0], 5.0)

Step 1: Enumerate surface segments of each primitive
    Box  -> 4 lines + 4 arcs (rounded corners)
    Circle -> 1 full arc

Step 2: Compute intersections
    Circle intersects left edge of box at 2 points

Step 3: Clip segments
    Box left edge: split into 2 line segments (above and below circle)
    Circle arc: clip to the portion inside the box (this becomes a cutout arc)
    Box other edges: unchanged

Step 4: Emit segment list
    [LineSeg, LineSeg, Arc(corner), LineSeg, Arc(corner), LineSeg,
     Arc(corner), LineSeg, Arc(corner), Arc(cutout)]
```

### Spatial Index Integration

Each segment is an independent primitive in the spatial index:

```
Tile near pin cutout:
    Today:    [CompoundShape] -> evaluate full CSG tree (6 SDFs + 5 subtracts)
    Segments: [Arc_cutout_3, LineSeg_7] -> evaluate 2 simple primitives
```

Fewer evaluations per tile AND exact distance. Both correctness and performance improve.

### API Design

Two options:

**Option A: Automatic decomposition**
```rust
// User writes CSG as today, compiler decomposes internally
let node = Sdf::rounded_box([0,0], [120,80], 8.0)
    - Sdf::circle([-120, 0], 5.0);
// compile() detects CSG and decomposes to segments
```

**Option B: Explicit path API alongside CSG**
```rust
// User can opt into exact paths
let node = Sdf::path()
    .line_to([120, -80])
    .arc_to(center, radius, angle)  // rounded corner
    .line_to([-120, -80])
    .cutout_arc(pin_center, pin_radius, start, end)  // pin cutout
    .close();
```

**Option C: Both**
- CSG API stays for quick prototyping (inexact but easy)
- Path API for production shapes (exact, manual)
- Optional auto-decomposition as optimization pass

### Implementation Phases

**Phase 1: Arc segment primitive**
- Add `SdfNode::Arc` with center, radius, start_angle, end_angle to shape.rs
- Implement exact distance in eval.rs
- Add OpType and shader support in compile.rs / shader.wgsl
- This is useful standalone (horseshoe, pie segments)

**Phase 2: Path/contour type**
- New `SdfNode::Contour { segments: Vec<Segment> }` variant
- Segment enum: `Line(a, b)`, `Arc(center, radius, start, end)`, `Bezier(p0, p1, p2, p3)`
- Shader evaluates `min()` over all segments (exact because non-overlapping)
- Spatial index culls per-segment

**Phase 3: Automatic CSG decomposition (optional)**
- CPU-side intersection computation for known primitive pairs
- Clipping algorithm for segments
- Replaces CSG subtree with equivalent contour during compilation
- Pairs to support: box-circle, box-box, circle-circle, rounded_box-circle

### Performance Considerations

- Decomposition is CPU-side, once per shape change (not per frame)
- Segment count for typical node: ~13 (4 edges + 4 corners + 5 pin cutouts)
- Per-tile: spatial index reduces to 2-3 relevant segments
- Per-pixel: 2-3 simple distance evaluations vs 11 SDFs + 5 CSG ops
- Net effect: **faster AND more correct**

### Open Questions

- Should segments carry winding information for fill testing?
- How to handle smooth union (SmoothUnion) - no clean segment decomposition exists
- Arc segment distance function: use IQ's `sdArc` or custom clipped version?
- Memory layout: segments as individual shapes or packed contour buffer?

### References

- Vello (Google): segment-based 2D renderer with tile culling
- Pathfinder (Mozilla): GPU path rasterizer using segment decomposition
- IQ 2D SDF functions: exact distance primitives
- Green (2007): "Improved Alpha-Tested Magnification for Vector Textures and Special Effects"
