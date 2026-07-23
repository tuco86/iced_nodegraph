//! Position-free shape recipes and their content-addressed hashes (the v3 dedup
//! foundation, Improvement A).
//!
//! A `ShapeExpr` is the DEFINITION of a shape - the authored primitives and
//! boolean ops, expressed in a LOCAL frame (centered on the shape's declared
//! intrinsic origin), with placement carried separately as a per-instance
//! translate (the keystone). Hashing the definition, never the evaluated arcs,
//! is what lets 500 identical nodes at 500 positions share one cache slot: their
//! recipes are byte-identical and differ only in the translate.
//!
//! Two disciplines the hash must honour (risk register):
//! - Hash the recipe (primitive params + op codes + sub-hashes), NEVER the
//!   evaluated geometry: arcs collide under translation and differ by 1 ULP
//!   native-vs-wasm, while the recipe is the only placement-stable key.
//! - Canonicalize float operands so `-0.0 == 0.0` and all NaNs collapse, and use
//!   a fixed deterministic hash (FNV-1a over little-endian bytes) so the same
//!   recipe hashes identically on native and wasm.
//!
//! Hashes COMPOSE: a shape's hash is a pure function of its sub-expression
//! hashes, so `base - union(cuts)` shared across nodes shares a cache slot.
//!
//! Hashing happens INCREMENTALLY: each constructor and operator runs one
//! FNV-1a pass over its own opcode/operands and folds in the already-computed
//! `hash` of every child `Shape`, so `Shape::hash` is an O(1) field read, never
//! a tree walk.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use crate::boolean;
use crate::curve::Curve;
use crate::drawable::Drawable;
use crate::tiling::Tiling;

/// A position-free geometry recipe: an expression tree of primitives
/// (`RoundedBox`, `Circle`, the open strokes `Line`/`Bezier`/`Arc`, the
/// degenerate `Point`, and `Tiling`) and operations (`Translate`, and the
/// booleans `Difference`, `Union`, `Intersection`), built in a LOCAL frame.
/// Every operand of an operation variant is a [`Shape`] (never a bare
/// `ShapeExpr`), so its already-computed hash is available to fold in
/// without re-walking the subtree - see [`Shape`] for the incremental
/// hashing scheme.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ShapeExpr {
    /// Rounded box centred on the local origin (spanning `-size/2 .. size/2`).
    /// `radii` are the four corner radii: `[top_left, top_right, bottom_right,
    /// bottom_left]`.
    RoundedBox { size: [f32; 2], radii: [f32; 4] },
    /// Circle centred on the local origin.
    Circle { radius: f32 },
    /// Open straight segment from `a` to `b` (a stroke, never an interior).
    Line { a: [f32; 2], b: [f32; 2] },
    /// Open cubic bezier (materialised as an arc-spline; a stroke).
    Bezier {
        p0: [f32; 2],
        p1: [f32; 2],
        p2: [f32; 2],
        p3: [f32; 2],
    },
    /// Open circular arc stroke: `sweep` radians of a circle of `radius` about
    /// `center`, starting at angle `start`. Unlike the centred primitives this
    /// carries its `center` directly (like [`ShapeExpr::Line`]).
    Arc {
        center: [f32; 2],
        radius: f32,
        start: f32,
        sweep: f32,
    },
    /// A single oriented point (a degenerate zero-length segment) at the local
    /// origin; `heading` orients its distance field. Place it with `translate`.
    Point { heading: f32 },
    /// An infinite analytic background field (grid/dots/triangles/hex). A leaf
    /// primitive: pushed standalone, not a boolean operand (it has no arcs).
    Tiling(Tiling),
    /// `inner` shifted by `offset` - an operation like any other, so a pin is
    /// `Shape::circle(r).translate([x, y])`.
    Translate(Box<Shape>, [f32; 2]),
    /// `0 - 1`: the second shape subtracted from the first (`a - b`).
    Difference(Box<Shape>, Box<Shape>),
    /// `0 | 1`: the union of two shapes (`a | b`).
    Union(Box<Shape>, Box<Shape>),
    /// `0 & 1`: the intersection of two shapes (`a & b`).
    Intersection(Box<Shape>, Box<Shape>),
}

/// A position-free geometry definition: the single input to the renderer. A
/// `Shape` pairs a `ShapeExpr` recipe with its content-addressed `hash`,
/// folded INCREMENTALLY as the recipe is built - every constructor and
/// operator below runs one FNV-1a pass over its own opcode/operands plus its
/// children's already-computed hashes, so [`Shape::hash`] is an O(1) field
/// read, never a tree walk. World placement is a SEPARATE per-instance
/// translate passed to `push` - so two identical shapes at different
/// positions share one cache slot (they hash equal).
///
/// Build with constructors + operators, exactly as authored:
/// ```
/// use iced_nodegraph_sdf::Shape;
/// let body = Shape::rounded_box([200.0, 120.0], [8.0; 4]);
/// let pin0 = Shape::circle(5.0).translate([0.0, 30.0]);
/// let pin1 = Shape::circle(5.0).translate([0.0, 90.0]);
/// let node = body - pin0 - pin1; // `-` = Difference, left-associative
/// ```
///
/// Origins: every primitive is centred on the local origin (`RoundedBox` spans
/// `-size/2 .. size/2`, `Circle` is centred) - placement and pin offsets are then
/// symmetric, which keeps coordinates small and float-precise.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    hash: u64,
    expr: ShapeExpr,
}

impl Shape {
    /// The recipe tree. `pub(crate)` so `evaluate`/`is_cacheable` (and any
    /// future in-crate consumer) can match on it; never exposed outside the
    /// crate, so external code stays limited to constructors/operators/the
    /// `hash`/`evaluate`/`is_cacheable`/`translate` methods.
    pub(crate) fn expr(&self) -> &ShapeExpr {
        &self.expr
    }

    /// Rounded box with its top-left corner at the local origin and per-corner
    /// `radii` `[top_left, top_right, bottom_right, bottom_left]`.
    pub fn rounded_box(size: impl Into<[f32; 2]>, radii: impl Into<[f32; 4]>) -> Self {
        let size = size.into();
        let radii = radii.into();
        let mut h = Fnv::new();
        h.write_u32(OP_ROUNDED_BOX);
        h.write_f32(size[0]);
        h.write_f32(size[1]);
        for r in radii {
            h.write_f32(r);
        }
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::RoundedBox { size, radii },
        }
    }
    /// Circle of `radius`, centred on the local origin. Place it with `translate`.
    pub fn circle(radius: f32) -> Self {
        let mut h = Fnv::new();
        h.write_u32(OP_CIRCLE);
        h.write_f32(radius);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Circle { radius },
        }
    }
    /// Open straight segment from `a` to `b`.
    pub fn line(a: impl Into<[f32; 2]>, b: impl Into<[f32; 2]>) -> Self {
        let a = a.into();
        let b = b.into();
        let mut h = Fnv::new();
        h.write_u32(OP_LINE);
        h.write_f32(a[0]);
        h.write_f32(a[1]);
        h.write_f32(b[0]);
        h.write_f32(b[1]);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Line { a, b },
        }
    }
    /// Open cubic bezier through the four control points.
    pub fn bezier(
        p0: impl Into<[f32; 2]>,
        p1: impl Into<[f32; 2]>,
        p2: impl Into<[f32; 2]>,
        p3: impl Into<[f32; 2]>,
    ) -> Self {
        let p0 = p0.into();
        let p1 = p1.into();
        let p2 = p2.into();
        let p3 = p3.into();
        let mut h = Fnv::new();
        h.write_u32(OP_BEZIER);
        for p in [p0, p1, p2, p3] {
            h.write_f32(p[0]);
            h.write_f32(p[1]);
        }
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Bezier { p0, p1, p2, p3 },
        }
    }
    /// Open circular arc stroke: `sweep` radians of a circle of `radius` about
    /// `center`, starting at angle `start`.
    pub fn arc(center: impl Into<[f32; 2]>, radius: f32, start: f32, sweep: f32) -> Self {
        let center = center.into();
        let mut h = Fnv::new();
        h.write_u32(OP_ARC);
        h.write_f32(center[0]);
        h.write_f32(center[1]);
        h.write_f32(radius);
        h.write_f32(start);
        h.write_f32(sweep);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Arc {
                center,
                radius,
                start,
                sweep,
            },
        }
    }
    /// A single oriented point at the local origin (place it with `translate`);
    /// `heading` orients its distance field.
    pub fn point(heading: f32) -> Self {
        let mut h = Fnv::new();
        h.write_u32(OP_POINT);
        h.write_f32(heading);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Point { heading },
        }
    }
    /// An infinite analytic background tiling (grid/dots/triangles/hex).
    pub fn tiling(tiling: Tiling) -> Self {
        let mut h = Fnv::new();
        h.write_u32(OP_TILING);
        let (tt, params) = tiling.to_gpu();
        h.write_u32(tt as u32);
        for p in params {
            h.write_f32(p);
        }
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Tiling(tiling),
        }
    }
    /// This shape shifted by `offset` (an operation, returns a new `Shape`).
    pub fn translate(self, offset: impl Into<[f32; 2]>) -> Self {
        let offset = offset.into();
        let mut h = Fnv::new();
        h.write_u32(OP_TRANSLATE);
        h.write_f32(offset[0]);
        h.write_f32(offset[1]);
        // Fold in the child's hash BEFORE moving it into the box.
        h.write_u64(self.hash);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Translate(Box::new(self), offset),
        }
    }
}

impl std::ops::Sub for Shape {
    type Output = Shape;
    /// `a - b` = subtract `b` from `a`.
    fn sub(self, rhs: Shape) -> Shape {
        let mut h = Fnv::new();
        h.write_u32(OP_DIFFERENCE);
        h.write_u64(self.hash);
        h.write_u64(rhs.hash);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Difference(Box::new(self), Box::new(rhs)),
        }
    }
}
impl std::ops::BitOr for Shape {
    type Output = Shape;
    /// `a | b` = the union of `a` and `b` (set algebra).
    fn bitor(self, rhs: Shape) -> Shape {
        let mut h = Fnv::new();
        h.write_u32(OP_UNION);
        h.write_u64(self.hash);
        h.write_u64(rhs.hash);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Union(Box::new(self), Box::new(rhs)),
        }
    }
}
impl std::ops::BitAnd for Shape {
    type Output = Shape;
    /// `a & b` = the intersection of `a` and `b`.
    fn bitand(self, rhs: Shape) -> Shape {
        let mut h = Fnv::new();
        h.write_u32(OP_INTERSECTION);
        h.write_u64(self.hash);
        h.write_u64(rhs.hash);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Intersection(Box::new(self), Box::new(rhs)),
        }
    }
}

/// Op-code discriminants mixed into the hash so structurally different shapes
/// with coincidentally-equal operands cannot collide.
const OP_ROUNDED_BOX: u32 = 1;
const OP_CIRCLE: u32 = 2;
const OP_LINE: u32 = 3;
const OP_BEZIER: u32 = 4;
const OP_TRANSLATE: u32 = 5;
const OP_DIFFERENCE: u32 = 6;
const OP_UNION: u32 = 7;
const OP_INTERSECTION: u32 = 8;
const OP_TILING: u32 = 9;
const OP_ARC: u32 = 10;
const OP_POINT: u32 = 11;

/// Canonical bit pattern of an `f32`: `-0.0` collapses to `+0.0` and every NaN
/// to one quiet NaN, so semantically-equal operands hash equal across platforms.
fn canon_bits(x: f32) -> u32 {
    if x.is_nan() {
        0x7fc0_0000
    } else if x == 0.0 {
        0
    } else {
        x.to_bits()
    }
}

/// FNV-1a hasher over little-endian bytes: deterministic and identical on native
/// and wasm (unlike `std`'s `DefaultHasher`, which is only stable in-process).
struct Fnv(u64);

impl Fnv {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }
    fn write_u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.write_u32((v & 0xffff_ffff) as u32);
        self.write_u32((v >> 32) as u32);
    }
    fn write_f32(&mut self, x: f32) {
        self.write_u32(canon_bits(x));
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

impl Shape {
    /// Whether this shape is worth caching across frames. Only the expensive
    /// boolean re-stitch (`Difference`/`Union`/`Intersection`) is cached; bare
    /// primitives and open strokes evaluate cheaply and - for edges - change
    /// every frame, so they bypass the frame-surviving cache and never churn its
    /// LRU. `Translate` inherits its inner shape's cacheability.
    pub fn is_cacheable(&self) -> bool {
        match self.expr() {
            ShapeExpr::Difference(..) | ShapeExpr::Union(..) | ShapeExpr::Intersection(..) => true,
            ShapeExpr::Translate(inner, _) => inner.is_cacheable(),
            _ => false,
        }
    }

    /// Content-addressed hash of the DEFINITION (not the evaluated arcs),
    /// folded incrementally by the constructors and operator impls above.
    /// Placement-stable: equal for two identical shapes at different
    /// positions. O(1): a cached field read, not a tree walk.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Materialize the shape to local-frame geometry (the expensive step the
    /// cache stores). A left-associative `a - b - c` is flattened into one
    /// `difference_many` for a single clean re-stitch.
    pub fn evaluate(&self) -> Drawable {
        match self.expr() {
            ShapeExpr::RoundedBox { size, radii } => eval_rounded_box(*size, *radii),
            ShapeExpr::Circle { radius } => Curve::circle([0.0, 0.0], *radius),
            ShapeExpr::Line { a, b } => Curve::line(*a, *b),
            ShapeExpr::Bezier { p0, p1, p2, p3 } => Curve::bezier(*p0, *p1, *p2, *p3),
            ShapeExpr::Arc {
                center,
                radius,
                start,
                sweep,
            } => Curve::arc_segment(*center, *radius, *start, *sweep),
            ShapeExpr::Point { heading } => Curve::point([0.0, 0.0], *heading),
            ShapeExpr::Tiling(t) => {
                let (tt, params) = t.to_gpu();
                Drawable::new_tiling(tt, params)
            }
            ShapeExpr::Translate(inner, off) => inner.evaluate().translated(off[0], off[1]),
            ShapeExpr::Difference(_, _) => {
                // Flatten the left-nested difference chain into base + cuts.
                let mut cuts = Vec::new();
                let mut node = self;
                while let ShapeExpr::Difference(base, cut) = node.expr() {
                    cuts.push(cut.evaluate());
                    node = base;
                }
                cuts.reverse();
                boolean::difference_many(&node.evaluate(), &cuts)
            }
            ShapeExpr::Union(a, b) => boolean::union(&a.evaluate(), &b.evaluate()),
            ShapeExpr::Intersection(a, b) => boolean::intersection(&a.evaluate(), &b.evaluate()),
        }
    }
}

/// Build a rounded box with per-corner radii, CENTRED on the local origin
/// (spanning `-size/2 .. size/2`). The contour walks clockwise from just past the
/// top-left corner, one arc per corner - mirroring `Curve::rounded_rect` but with
/// four independent radii. Each radius is clamped to half the shorter side.
fn eval_rounded_box(size: [f32; 2], radii: [f32; 4]) -> Drawable {
    let [w, h] = size;
    let rmax = (w.min(h)) * 0.5;
    let tl = radii[0].clamp(0.0, rmax);
    let tr = radii[1].clamp(0.0, rmax);
    let br = radii[2].clamp(0.0, rmax);
    let bl = radii[3].clamp(0.0, rmax);
    Curve::shape([-w * 0.5 + tl, -h * 0.5], FRAC_PI_2)
        .line((w - tl - tr).max(0.0))
        .arc(tr, FRAC_PI_2)
        .line((h - tr - br).max(0.0))
        .arc(br, FRAC_PI_2)
        .line((w - br - bl).max(0.0))
        .arc(bl, FRAC_PI_2)
        .line((h - bl - tl).max(0.0))
        .arc(tl, FRAC_PI_2)
        .close()
}

/// One cached, evaluated shape: the expensive local-frame arcs, plus the frame
/// tick it was last used on (for LRU eviction).
struct CachedShape {
    drawable: Drawable,
    last_used: u64,
}

/// Frame-surviving cache of evaluated shapes, keyed by recipe hash (Improvement
/// A). A unique shape's boolean->arcs runs once and is reused on every later
/// frame; only the per-instance translate changes. An LRU bound caps memory.
///
/// Only STABLE shapes (node bodies) are fed here. Ephemeral geometry - edges,
/// whose arcs change whenever an endpoint moves - is never a `ShapeExpr`, so it
/// structurally bypasses the cache and cannot churn the LRU.
pub struct ShapeCache {
    map: HashMap<u64, CachedShape>,
    capacity: usize,
    tick: u64,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            capacity: capacity.max(1),
            tick: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Local-frame geometry for `recipe`, evaluating and caching on a miss and
    /// reusing the cached arcs on a hit. The returned drawable is position-free;
    /// the caller places it with the per-instance translate.
    pub fn get_or_eval(&mut self, recipe: &Shape) -> &Drawable {
        let h = recipe.hash();
        self.tick += 1;
        let tick = self.tick;
        if self.map.contains_key(&h) {
            self.hits += 1;
            self.map.get_mut(&h).unwrap().last_used = tick;
        } else {
            self.misses += 1;
            let drawable = recipe.evaluate();
            // Evict before insert so capacity is a hard bound. Never evicts the
            // entry being inserted (it is not in the map yet).
            self.evict_to_capacity(self.capacity - 1);
            self.map.insert(
                h,
                CachedShape {
                    drawable,
                    last_used: tick,
                },
            );
        }
        &self.map.get(&h).unwrap().drawable
    }

    /// Evict least-recently-used entries until at most `target` remain.
    fn evict_to_capacity(&mut self, target: usize) {
        while self.map.len() > target {
            let Some((&victim, _)) = self.map.iter().min_by_key(|(_, c)| c.last_used) else {
                break;
            };
            self.map.remove(&victim);
        }
    }

    /// Number of distinct shapes currently cached.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Fraction of `get_or_eval` calls that hit the cache, over the cache's
    /// lifetime. ~1.0 on a static graph is the R4 contract.
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_body() -> Shape {
        // `box - pin0 - pin1`, pins at LOCAL offsets relative to the body centre.
        Shape::rounded_box([140.0, 88.0], [10.0; 4])
            - Shape::circle(4.0).translate([-70.0, -20.0])
            - Shape::circle(4.0).translate([70.0, 20.0])
    }

    #[test]
    fn identical_shapes_hash_equal() {
        // Two independently-built shapes for the SAME geometry: the dedup property.
        assert_eq!(node_body().hash(), node_body().hash());
    }

    #[test]
    fn differing_operands_hash_differently() {
        let a = Shape::rounded_box([140.0, 88.0], [10.0; 4]);
        let b = Shape::rounded_box([140.0, 88.0], [12.0; 4]);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn neg_zero_and_zero_hash_equal() {
        let a = Shape::circle(5.0).translate([0.0, 0.0]);
        let b = Shape::circle(5.0).translate([-0.0, -0.0]);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn nan_operands_hash_equal() {
        let a = Shape::circle(5.0).translate([f32::NAN, 0.0]);
        let b = Shape::circle(5.0).translate([f32::NAN, 0.0]);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn structurally_different_same_operands_differ() {
        // Same float operands, different op: must not collide.
        let rect = Shape::rounded_box([10.0, 10.0], [5.0; 4]);
        let circ = Shape::circle(5.0).translate([5.0, 5.0]);
        assert_ne!(rect.hash(), circ.hash());
    }

    #[test]
    fn hash_excludes_placement() {
        // The shape is position-free, so two identical bodies hash equal; their
        // world positions live in the `push` placement, NOT the shape.
        assert_eq!(node_body().hash(), node_body().hash());
    }

    #[test]
    fn difference_matches_boolean_difference_many() {
        // `box - c0 - c1` evaluates to the same geometry as the direct
        // `difference_many` over the equivalent world drawables.
        let cuts_local = [[-70.0, -20.0], [70.0, 20.0]];
        let from_shape = node_body().evaluate();

        let body = Curve::rounded_rect([0.0, 0.0], [70.0, 44.0], 10.0);
        let cuts: Vec<Drawable> = cuts_local.iter().map(|p| Curve::circle(*p, 4.0)).collect();
        let world = boolean::difference_many(&body, &cuts);
        assert_eq!(from_shape.segment_count(), world.segment_count());
    }

    #[test]
    fn evaluate_matches_direct_construction() {
        // RoundedBox (centred, size = 2*half) evaluates to the same local geometry
        // as the centred Curve::rounded_rect builder.
        let from_shape = Shape::rounded_box([140.0, 88.0], [10.0; 4]).evaluate();
        let direct = Curve::rounded_rect([0.0, 0.0], [70.0, 44.0], 10.0);
        assert_eq!(from_shape.segment_count(), direct.segment_count());
        let a = from_shape.bounds();
        let b = direct.bounds();
        for i in 0..4 {
            assert!(
                (a[i] - b[i]).abs() < 1e-4,
                "bounds differ at {i}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn zero_corner_radius_evaluates_as_sharp_rectangle() {
        // A zero corner radius (the selection box / toggle indicator) must not
        // emit degenerate zero-radius arcs: those trip `from_center_arc`'s
        // positive-radius invariant. Each corner is a sharp turn, so the box is
        // four lines and the bounds match the requested size.
        let d = Shape::rounded_box([80.0, 40.0], [0.0; 4]).evaluate();
        assert_eq!(d.segment_count(), 4);
        let b = d.bounds();
        let expected = [-40.0, -20.0, 40.0, 20.0];
        for i in 0..4 {
            assert!(
                (b[i] - expected[i]).abs() < 1e-4,
                "bounds differ at {i}: {b:?} vs {expected:?}"
            );
        }
    }

    #[test]
    fn arc_and_point_evaluate_to_their_curve_primitives() {
        let arc = Shape::arc([10.0, -5.0], 40.0, -FRAC_PI_2, FRAC_PI_2).evaluate();
        let arc_direct = Curve::arc_segment([10.0, -5.0], 40.0, -FRAC_PI_2, FRAC_PI_2);
        assert_eq!(arc.segment_count(), arc_direct.segment_count());

        let point = Shape::point(FRAC_PI_2).evaluate();
        let point_direct = Curve::point([0.0, 0.0], FRAC_PI_2);
        assert_eq!(point.segment_count(), point_direct.segment_count());
    }

    #[test]
    fn arc_and_point_hash_distinctly() {
        // Different ops with overlapping operands must not collide, and the new
        // leaves must be placement-stable (hash equal for an independent rebuild).
        let arc = Shape::arc([0.0, 0.0], 5.0, 0.0, FRAC_PI_2);
        let point = Shape::point(0.0);
        assert_eq!(
            arc.hash(),
            Shape::arc([0.0, 0.0], 5.0, 0.0, FRAC_PI_2).hash()
        );
        assert_ne!(arc.hash(), point.hash());
        assert_ne!(arc.hash(), Shape::circle(5.0).hash());
        assert_ne!(
            arc.hash(),
            Shape::arc([0.0, 0.0], 5.0, 0.0, FRAC_PI_2 + 0.1).hash()
        );
    }

    #[test]
    fn cache_reuses_identical_shapes() {
        // The headline: N identical nodes pay for ONE boolean evaluation.
        let mut cache = ShapeCache::new(64);
        for _ in 0..500 {
            let _ = cache.get_or_eval(&node_body());
        }
        assert_eq!(cache.len(), 1, "500 identical shapes must occupy one slot");
        assert_eq!(cache.misses(), 1, "the boolean evaluates exactly once");
        assert_eq!(cache.hits(), 499);
        assert!((cache.hit_rate() - 499.0 / 500.0).abs() < 1e-6);
    }

    #[test]
    fn cache_evicts_least_recently_used() {
        // Capacity 2. Insert A, B; touch A (now B is LRU); insert C -> B evicted.
        let mut cache = ShapeCache::new(2);
        let a = Shape::circle(1.0);
        let b = Shape::circle(2.0);
        let c = Shape::circle(3.0);
        cache.get_or_eval(&a);
        cache.get_or_eval(&b);
        cache.get_or_eval(&a); // touch A so B becomes least-recently-used
        cache.get_or_eval(&c); // inserts C, must evict B
        assert_eq!(cache.len(), 2);

        let misses_before = cache.misses();
        cache.get_or_eval(&a);
        cache.get_or_eval(&c);
        assert_eq!(
            cache.misses(),
            misses_before,
            "A and C should still be cached"
        );
        cache.get_or_eval(&b);
        assert_eq!(
            cache.misses(),
            misses_before + 1,
            "B should have been evicted"
        );
    }

    #[test]
    fn local_evaluate_plus_translate_equals_world() {
        // Evaluating local then translating by the placement reproduces the world
        // shape - the bridge to `compile_local_at`.
        let (cx, cy) = (300.0, -150.0);
        let local = Shape::rounded_box([140.0, 88.0], [10.0; 4]).evaluate();
        let placed = local.translated(cx, cy);
        let world = Curve::rounded_rect([cx, cy], [70.0, 44.0], 10.0);
        assert_eq!(placed.segment_count(), world.segment_count());
        for (ps, ws) in placed.segments.iter().zip(world.segments.iter()) {
            assert!(
                (ps.start - ws.start).length() < 1e-3 && (ps.end - ws.end).length() < 1e-3,
                "endpoints differ: ({:?},{:?}) vs ({:?},{:?})",
                ps.start,
                ps.end,
                ws.start,
                ws.end,
            );
            assert!(
                (ps.curvature - ws.curvature).abs() < 1e-3,
                "curvature differs"
            );
            assert!((ps.heading - ws.heading).abs() < 1e-3, "heading differs");
        }
    }
}
