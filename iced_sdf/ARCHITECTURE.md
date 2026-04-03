# iced_sdf Architecture

## Purpose

Render 2D signed distance fields (SDF) on the GPU via a tile-based spatial index. The pipeline takes geometric primitives (lines, arcs, beziers) with styles (colors, patterns, distance ranges) and produces pixel-accurate anti-aliased output.

## Data Model

### Primitives

There are exactly three drawable types:

| Type | Description | Segments |
|------|-------------|----------|
| **CurveSegment** | Single disconnected curve (line, arc, bezier) | 1 |
| **Shape** | Connected contour of segments, optionally closed | N |
| **Tiling** | Infinite repeating pattern (grid, dots, triangles, hex) | 0 (parametric) |

### Segments

Each segment is one of four types:

| Type | Geometry | SDF returns |
|------|----------|-------------|
| **Line** | Two endpoints (a, b) | Unsigned dist, parametric u [0,1], signed perpendicular v |
| **Arc** | Center, radius, start angle, sweep | Unsigned dist, parametric u [0,1], signed perpendicular v |
| **CubicBezier** | Four control points (p0..p3) | Unsigned dist, parametric u [0,1], signed perpendicular v |
| **Point** | Position, heading angle | Unsigned dist from point, heading-based sign |

All SDF functions return `SdfResult { dist: f32, u: f32, v: f32 }`:
- `dist`: Unsigned distance to the curve (always >= 0)
- `u`: Parametric position along the curve [0, 1]
- `v`: Signed perpendicular distance (positive = right side of curve in travel direction)

### Styles

A style defines a 2D color field mapped over (arc-length, distance):

```
              arc=0          arc=1
dist_from:  near_start     near_end
dist_to:    far_start      far_end
```

Plus an optional pattern and special modes:

| Field | Purpose |
|-------|---------|
| `near_start/end, far_start/end` | 4-corner color gradient |
| `dist_from, dist_to` | Distance range (negative = inside for closed shapes) |
| `pattern` | Optional: modifies effective distance (stroke, dash, arrow, dot) |
| `distance_field` | Special: IQ visualization mode, ignores color/pattern |

### Patterns

Patterns transform the raw SDF distance into a stroke-space distance:

| Pattern | Parameters | Effect |
|---------|-----------|--------|
| Solid | thickness | `abs(dist) - thickness/2` |
| Dashed | thickness, dash, gap, angle | 2D box SDF with angle shear |
| Arrowed | thickness, segment, gap, angle | Like dashed, symmetric angle |
| Dotted | spacing, radius | Circular dots along curve |
| DashDotted | thickness, dash, gap, dot_radius | Alternating dashes and dots |
| ArrowDotted | thickness, segment, gap, dot_radius | Alternating arrows and dots |

## GPU Pipeline

### Stage 1: Compile (CPU)

`compile_drawable()` transforms Rust data to GPU structs:

```
Drawable + Style --> GpuDrawEntry + GpuStyle + [GpuSegment]
```

No logic, pure data mapping. Each drawable becomes one entry pointing to a contiguous range of segments in the segment buffer.

**Flags set at compile time:**
- `FLAG_CLOSED` (entry): drawable is a closed contour
- `STYLE_FLAG_HAS_PATTERN` (style): pattern present
- `STYLE_FLAG_DISTANCE_FIELD` (style): IQ visualization mode
- `STYLE_FLAG_CLOSED` (style): copied from entry (for fragment shader convenience)

### Stage 2: Compute Shader (GPU) -- Tile Spatial Index

`cs_build_index` runs per tile (16x16 pixels). For each tile:

1. **Evaluate all entries** at tile center in world space
2. **Determine visibility** per entry:
   - Distance field: always visible
   - Pattern: always visible (per-segment culling below)
   - Closed fill: `signed_dist - thd < dist_to`
   - Open curve: `unsigned_dist - thd < dist_to`
3. **Push segments** into tile slot array:
   - Pattern entries: evaluate `apply_pattern()` at tile center, include if `eff <= margin`
   - Non-pattern entries: include if `unsigned_dist <= proximity`
4. **Sort slots** by style_idx (preserves z-order)

**Tile culling contract:**
- `thd` = tile half-diagonal in world space = `TILE_SIZE * sqrt(2)/2 / (zoom * scale)`
- A segment MUST be included if it could affect ANY pixel in the tile
- The margin must account for the maximum SDF variation from tile center to any pixel
- For patterns: `margin = thd * (1 + |tan(angle)|)` (arc-length + angle shear variation)
- Violation of this contract produces tile-boundary artifacts

### Stage 3: Fragment Shader (GPU) -- Per-Pixel Rendering

`fs_main` runs per pixel:

1. **Transform** pixel position to world coordinates
2. **Look up tile** from spatial index
3. **For each style group** in the tile's slot list:
   a. Find the **nearest segment** (minimum `abs(dist)`)
   b. Apply **sign**: `dist *= select(1, -1, v > 0)` (always, for all segments)
   c. Call `render_style()` with the nearest segment's SDF result
4. **Accumulate** fragments with premultiplied alpha blending (front to back)

**Signed distance convention:**
- `eval_single_segment()` always applies sign from `v`
- For closed shapes: negative = interior, positive = exterior
- For open curves: sign has no geometric meaning but must be consistent between compute and fragment shaders
- `render_style()` receives signed distance

### render_style() Contract

For **pattern styles** (`STYLE_FLAG_HAS_PATTERN`):
1. `apply_pattern(signed_dist, sdf, style, time)` transforms dist to pattern-space
2. All pattern functions use `abs(dist)` internally -- sign-invariant
3. Color from arc-length gradient (`near_start` to `near_end`)
4. Anti-aliasing via `fwidth()` smoothstep at pattern boundary

For **non-pattern styles** (fills, shadows, blur):
1. 4-corner bilinear color interpolation over (arc_t, dist_t)
2. `dist_t = (dist - dist_from) / (dist_to - dist_from)`
3. Anti-aliasing at `dist_from` and `dist_to` boundaries
4. **Note**: signed distance means non-pattern styles on open curves are asymmetric (one side only). This is a known property, not a bug.

For **distance field** (`STYLE_FLAG_DISTANCE_FIELD`):
1. IQ visualization: color from sign, contour lines from `cos()`
2. White highlight at `dist = 0`

## Invariants

1. **Segment SDF functions return unsigned distance.** Sign is applied later in `eval_single_segment`.
2. **`eval_single_segment` always applies sign.** No conditional. `cs_eval_segment` does the same.
3. **Pattern functions are sign-invariant.** They use `abs(dist)` internally.
4. **Tile culling is conservative.** A segment is included if it MIGHT affect any pixel. False positives are acceptable. False negatives produce artifacts.
5. **Style rendering is tile-independent.** Given the same set of segments, the result is identical regardless of which tile the pixel belongs to.
6. **No special-case flags in eval_single_segment.** The function is the same for all drawable types. Behavioral differences come from the style, not from the segment evaluation.

## What This Pipeline Does NOT Do

- No per-drawable sign control. All segments get signed distance.
- No unsigned rendering mode for open curves. (Non-pattern styles are inherently one-sided on open curves due to sign.)
- No special handling for overlapping drawables. Each drawable is independent; compositing is purely alpha-based.
- No anti-aliasing beyond `fwidth`-based smoothstep. No MSAA, no temporal AA.

## File Map

| File | Responsibility |
|------|---------------|
| `src/curve.rs` | Curve/ShapeBuilder API, segment creation |
| `src/drawable.rs` | Drawable struct, segment storage, bounds |
| `src/tiling.rs` | Tiling factory functions |
| `src/style.rs` | Style struct, constructors |
| `src/pattern.rs` | Pattern enum, GPU parameter encoding |
| `src/compile.rs` | Rust-to-GPU data transformation |
| `src/shared.rs` | Shared GPU resources (shader, pipelines, layouts) |
| `src/primitive.rs` | SdfPrimitive (prepare + draw), SdfPipeline |
| `src/pipeline/shader.wgsl` | All GPU code (vertex, fragment, compute) |
| `src/pipeline/types.rs` | GPU struct definitions (must match WGSL) |
| `src/pipeline/buffer.rs` | Dynamic GPU buffer wrapper |
| `src/pipeline/pixel_tests.rs` | Headless pixel-level rendering tests |
