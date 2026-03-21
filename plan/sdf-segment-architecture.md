# Segment-Based SDF Architecture

## Problem

CSG operations (`min`, `max`) on SDFs produce inexact distance fields. The distance is correct at the surface (`dist = 0`) but increasingly wrong further away. This affects blur, gradients, expand, and any effect that depends on accurate distance values.

Example: `rounded_box - circle` (node with pin cutouts) has distorted blur falloff near the cutouts because `max(dist_box, -dist_circle)` underestimates the true distance to the compound surface.

## Solution

Replace CSG-based compound shapes with **contours**: ordered lists of geometric segments (lines, arcs, cubic beziers). Each segment has an exact distance function. `min()` over non-overlapping segments produces exact distances everywhere.

## Architecture

### Contour Type

A contour is an ordered list of segments with metadata:

```rust
struct Contour {
    segments: Vec<Segment>,
    closed: bool,
}

enum Segment {
    Line { a: Vec2, b: Vec2 },
    Arc { center: Vec2, radius: f32, start_angle: f32, end_angle: f32 },
    CubicBezier { p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2 },
}
```

A shape is one or more contours (multiple contours for shapes with holes):

```rust
struct Shape {
    contours: Vec<Contour>,
}
```

### Distance Evaluation

Two modes, determined by the rendering layer (not the shape):

- **Unsigned**: `min(dist_to_each_segment)` -- always >= 0, for strokes/outlines
- **Signed**: `winding_sign * min(dist_to_each_segment)` -- negative inside, for fills

The winding number is computed via ray casting against all segments. A GPU flag per shape controls which mode the shader uses.

### Turtle-Graphics Shape Builder

Shapes are constructed with a cursor-and-heading builder (like Logo/turtle graphics). All coordinates are relative to the current cursor position and heading direction.

**Builder state:**

```rust
struct ShapeBuilder {
    segments: Vec<Segment>,
    cursor: Vec2,      // current position
    heading: f32,      // current direction (radians)
    origin: Vec2,      // start position (for close check)
}
```

**API:**

```rust
let node = Sdf::shape()
    .start([-60.0, -40.0], FRAC_PI_2)  // position, heading
    .line(80.0)                          // straight line, length
    .arc(5.0, FRAC_PI_2)                // radius, sweep (radians)
    .line(120.0)
    .arc(5.0, FRAC_PI_2)
    .line(30.0)
    .angle(FRAC_PI_2)                   // turn heading without drawing
    .arc(2.5, PI)                       // pin cutout (semicircle)
    .angle(-FRAC_PI_2)                  // turn back
    .line(25.0)
    .arc(5.0, FRAC_PI_2)
    .line(120.0)
    .arc(5.0, FRAC_PI_2)
    .close();                            // close path + debug_assert cursor ~ origin
```

**Operations:**

| Method | Draws | Moves Cursor | Changes Heading |
|--------|-------|-------------|-----------------|
| `start(pos, heading)` | -- | yes | yes |
| `line(length)` | Line segment | yes | -- |
| `arc(radius, sweep)` | Circular arc | yes | yes (+sweep) |
| `cubic(exit_angle, entry_angle, distance)` | Cubic bezier | yes | yes (end tangent) |
| `angle(delta)` | -- | -- | yes (+delta) |
| `close()` | Line to start if needed | -- | -- (asserts closed) |
| `end()` | -- | -- | -- (open path) |

**Angles**: All angles in **radians**. No performance difference (conversion is build-time only), but consistent with Rust math ecosystem (`f32::sin`, `std::f32::consts::PI`). Helper constant `DEG: f32 = PI / 180.0` for readability where needed.

**Arc behavior**: Positive sweep = left turn (CCW), negative sweep = right turn (CW). Center is computed perpendicular to heading.

**Cubic behavior**: `exit_angle` is deviation from heading at start, `entry_angle` at end. `distance` is straight-line distance to endpoint. Heading updates to end tangent direction.

**Close**: `close()` adds a closing line segment from cursor to origin if they don't coincide, then asserts `debug_assert!` that the contour is closed. `end()` leaves the path open.

### Segment Primitives

The **industry-standard minimal set** (supported by all major 2D renderers: Skia, Vello, Cairo, SVG, Direct2D, CoreGraphics):

| Segment | Use Case | Distance Function |
|---------|----------|-------------------|
| Line | Straight edges | Exact, trivial |
| Arc | Rounded corners, circles, pin cutouts | Exact, analytical |
| Cubic Bezier | Edges between nodes, organic curves | Exact (polynomial root finding) |

Quadratic Bezier is optional (nice-to-have). All other curve types (conics, elliptical arcs, NURBS) are unnecessary for our use cases.

### What This Replaces

The existing CSG-based approach for compound shapes:

```
Current (CSG tree):
    rounded_box - circle - circle - circle
    → RPN: [Box, Circle, Subtract, Circle, Subtract, Circle, Subtract]
    → GPU: 7 ops, stack evaluation, inexact distances

New (contour):
    Sdf::shape().line().arc().line().arc()...close()
    → Segments: [Line, Arc, Line, Arc, Line, Arc, ...]
    → GPU: min() over segments, exact distances
```

The existing `Sdf` primitives (circle, box, rounded_box, line, bezier) and CSG operations (union, subtract) remain available for cases where exact distance is not required.

### Spatial Index Integration

Each segment is an independent entry in the spatial index:

```
Tile near pin cutout:
    Current: [CompoundShape] -> evaluate full CSG tree (6 SDFs + 5 subtracts)
    New:     [Arc_cutout, LineSeg_7] -> evaluate 2 simple primitives
```

Fewer evaluations per tile AND exact distance. Both correctness and performance improve.

## Nodegraph Use Cases

Everything needed for the node graph editor maps cleanly to contours:

| Element | Construction | Type |
|---------|-------------|------|
| Node body | Turtle builder (Line + Arc) | Closed contour, signed distance |
| Edges | Cubic Bezier (existing `Sdf::bezier`) | Open path, unsigned distance |
| Pins | Single Arc (full circle) | Closed contour, signed distance |
| Selection box | 4 Lines | Closed contour, signed distance |

No boolean operations needed. Node shapes with pin cutouts are drawn directly using the turtle builder (`.angle()` + `.arc()` for each cutout). This avoids CSG entirely.

## Implementation Phases

### Phase 1: Contour Data Type + Turtle Builder

- `Contour` struct: `Vec<Segment>`, `closed: bool`
- `Shape` struct: `Vec<Contour>` (single contour for now)
- `ShapeBuilder` with `start`, `line`, `arc`, `angle`, `close`, `end`
- `Segment` enum: `Line`, `Arc` (Cubic Bezier deferred to Phase 2)
- CPU-side distance evaluation for hit testing
- Unit tests for builder geometry (cursor/heading math)

### Phase 2: GPU Rendering

- New `SdfNode::Contour` variant in shape.rs
- New `OpType` entries for contour evaluation in compile.rs
- Shader support: `min()` over segment distances + winding number for sign
- Spatial index: register individual segments with bounding boxes
- Integration into existing Layer/Pattern system

### Phase 3: Cubic Bezier in Builder

- Add `.cubic(exit_angle, entry_angle, distance)` to turtle builder
- Cubic segment distance evaluation (CPU + GPU)
- Use for edge rendering via contour system (replacing direct `Sdf::bezier`)

### Phase 4: Boolean Operations (Future, Optional)

If general-purpose CSG on contours is needed later, it can be built on top:

- Intersection finding: Line-Line, Line-Arc, Arc-Arc (all analytical)
- Segment splitting at intersection points
- Inside/outside classification via winding number
- Segment selection per operation (union, subtract, intersect, XOR)
- Simplify pass: merge collinear lines, co-circular arcs, remove degenerates
- Self-intersection resolution (= union with self)

This is ~700-900 lines of geometry code. All Line+Arc intersections are analytically exact (linear/quadratic equations). Cubic Bezier intersections are harder (degree 6-9 polynomials, numerical methods) and would only be needed if cubics participate in boolean ops.

**Constraint**: Boolean ops work cleanly for Line+Arc shapes. Degenerate cases (tangential contacts, coincident edges, self-intersections from aggressive offsets) require a Simplify pass. For well-formed nodegraph shapes, these degeneracies don't occur.

## Performance Considerations

- Contour construction is CPU-side, once per shape change (not per frame)
- Typical node segment count: ~13 (4 edges + 4 corner arcs + ~5 pin cutout arcs)
- Per-tile: spatial index reduces to 2-3 relevant segments
- Per-pixel: 2-3 simple distance evaluations vs 11+ SDFs with CSG ops
- Net effect: **faster AND more correct**

## Decided Against

- **Domain-specific builders** (e.g. `NodeShape::rounded_rect().cutout()`): Too specific, the turtle builder is general enough and equally ergonomic for the nodegraph case.
- **Automatic CSG decomposition**: Complex (intersection algorithms for every primitive pair), unnecessary when shapes can be constructed directly as contours.
- **Degree input for angles**: Radians chosen for consistency with Rust ecosystem. `DEG` constant available for readability.
- **Smooth Union/Subtract on contours**: No clean segment decomposition exists. Keep CSG-based SmoothUnion for cases that need it.

## References

- Skia (Google): SkPath with Line/Quad/Cubic/Conic + SkPathOps for boolean operations
- Vello/kurbo (Linebender): PathEl enum (Line/Quad/Cubic), arc-to-cubic approximation
- Cairo: Line/Cubic/Arc (center-parameterized), rel_* turtle-style variants
- SVG Path spec: Most complete relative-coordinate model, smooth continuity shortcuts (S/T)
- Direct2D: Line/Quad/Cubic/Arc + CombineWithGeometry for boolean ops
- CoreGraphics: Line/Quad/Cubic/Arc (center + tangent parameterized)
- IQ 2D SDF functions: exact distance primitives for all segment types
