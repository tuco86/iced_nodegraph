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

use crate::boolean;
use crate::curve::Curve;
use crate::drawable::Drawable;
use crate::hash::Fnv1a;
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
        let mut h = Fnv1a::new();
        h.u32(OP_ROUNDED_BOX);
        h.f32(size[0]);
        h.f32(size[1]);
        for r in radii {
            h.f32(r);
        }
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::RoundedBox { size, radii },
        }
    }
    /// Circle of `radius`, centred on the local origin. Place it with `translate`.
    pub fn circle(radius: f32) -> Self {
        let mut h = Fnv1a::new();
        h.u32(OP_CIRCLE);
        h.f32(radius);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Circle { radius },
        }
    }
    /// Open straight segment from `a` to `b`.
    pub fn line(a: impl Into<[f32; 2]>, b: impl Into<[f32; 2]>) -> Self {
        let a = a.into();
        let b = b.into();
        let mut h = Fnv1a::new();
        h.u32(OP_LINE);
        h.f32(a[0]);
        h.f32(a[1]);
        h.f32(b[0]);
        h.f32(b[1]);
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
        let mut h = Fnv1a::new();
        h.u32(OP_BEZIER);
        for p in [p0, p1, p2, p3] {
            h.f32(p[0]);
            h.f32(p[1]);
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
        let mut h = Fnv1a::new();
        h.u32(OP_ARC);
        h.f32(center[0]);
        h.f32(center[1]);
        h.f32(radius);
        h.f32(start);
        h.f32(sweep);
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
        let mut h = Fnv1a::new();
        h.u32(OP_POINT);
        h.f32(heading);
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Point { heading },
        }
    }
    /// An infinite analytic background tiling (grid/dots/triangles/hex).
    pub fn tiling(tiling: Tiling) -> Self {
        let mut h = Fnv1a::new();
        h.u32(OP_TILING);
        let (tt, params) = tiling.to_gpu();
        h.u32(tt as u32);
        for p in params {
            h.f32(p);
        }
        Shape {
            hash: h.finish(),
            expr: ShapeExpr::Tiling(tiling),
        }
    }
    /// This shape shifted by `offset` (an operation, returns a new `Shape`).
    pub fn translate(self, offset: impl Into<[f32; 2]>) -> Self {
        let offset = offset.into();
        let mut h = Fnv1a::new();
        h.u32(OP_TRANSLATE);
        h.f32(offset[0]);
        h.f32(offset[1]);
        // Fold in the child's hash BEFORE moving it into the box.
        h.u64(self.hash);
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
        let mut h = Fnv1a::new();
        h.u32(OP_DIFFERENCE);
        h.u64(self.hash);
        h.u64(rhs.hash);
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
        let mut h = Fnv1a::new();
        h.u32(OP_UNION);
        h.u64(self.hash);
        h.u64(rhs.hash);
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
        let mut h = Fnv1a::new();
        h.u32(OP_INTERSECTION);
        h.u64(self.hash);
        h.u64(rhs.hash);
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
            ShapeExpr::RoundedBox { size, radii } => {
                Curve::rounded_rect_with_radii([0.0, 0.0], [size[0] * 0.5, size[1] * 0.5], *radii)
            }
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
    use std::f32::consts::FRAC_PI_2;

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

    /// One perturbation per recipe FIELD, produced by an exhaustive match: a
    /// `ShapeExpr` variant added without its parameters perturbed here fails to
    /// COMPILE, so the coverage cannot silently rot.
    ///
    /// Array-valued fields are perturbed in their LAST element, which also catches
    /// a fold that stops short of the end.
    fn perturbations(shape: &Shape) -> Vec<(&'static str, Shape)> {
        match shape.expr() {
            ShapeExpr::RoundedBox { size, radii } => vec![
                (
                    "RoundedBox.size",
                    Shape::rounded_box([size[0], size[1] + 1.0], *radii),
                ),
                ("RoundedBox.radii", {
                    let mut r = *radii;
                    r[3] += 1.0;
                    Shape::rounded_box(*size, r)
                }),
            ],
            ShapeExpr::Circle { radius } => {
                vec![("Circle.radius", Shape::circle(radius + 1.0))]
            }
            ShapeExpr::Line { a, b } => vec![
                ("Line.a", Shape::line([a[0], a[1] + 1.0], *b)),
                ("Line.b", Shape::line(*a, [b[0], b[1] + 1.0])),
            ],
            ShapeExpr::Bezier { p0, p1, p2, p3 } => vec![
                (
                    "Bezier.p0",
                    Shape::bezier([p0[0], p0[1] + 1.0], *p1, *p2, *p3),
                ),
                (
                    "Bezier.p1",
                    Shape::bezier(*p0, [p1[0], p1[1] + 1.0], *p2, *p3),
                ),
                (
                    "Bezier.p2",
                    Shape::bezier(*p0, *p1, [p2[0], p2[1] + 1.0], *p3),
                ),
                (
                    "Bezier.p3",
                    Shape::bezier(*p0, *p1, *p2, [p3[0], p3[1] + 1.0]),
                ),
            ],
            ShapeExpr::Arc {
                center,
                radius,
                start,
                sweep,
            } => vec![
                (
                    "Arc.center",
                    Shape::arc([center[0], center[1] + 1.0], *radius, *start, *sweep),
                ),
                (
                    "Arc.radius",
                    Shape::arc(*center, radius + 1.0, *start, *sweep),
                ),
                (
                    "Arc.start",
                    Shape::arc(*center, *radius, start + 1.0, *sweep),
                ),
                (
                    "Arc.sweep",
                    Shape::arc(*center, *radius, *start, sweep + 1.0),
                ),
            ],
            ShapeExpr::Point { heading } => {
                vec![("Point.heading", Shape::point(heading + 1.0))]
            }
            ShapeExpr::Tiling(_) => vec![(
                "Tiling",
                Shape::tiling(crate::tiling::Tiling::grid(9.0, 9.0, 9.0)),
            )],
            ShapeExpr::Translate(inner, offset) => vec![
                (
                    "Translate.offset",
                    (**inner).clone().translate([offset[0], offset[1] + 1.0]),
                ),
                ("Translate.inner", Shape::circle(123.0).translate(*offset)),
            ],
            ShapeExpr::Difference(a, b) => vec![
                ("Difference.lhs", Shape::circle(123.0) - (**b).clone()),
                ("Difference.rhs", (**a).clone() - Shape::circle(123.0)),
                ("Difference.op", (**a).clone() | (**b).clone()),
            ],
            ShapeExpr::Union(a, b) => vec![
                ("Union.lhs", Shape::circle(123.0) | (**b).clone()),
                ("Union.rhs", (**a).clone() | Shape::circle(123.0)),
                ("Union.op", (**a).clone() & (**b).clone()),
            ],
            ShapeExpr::Intersection(a, b) => vec![
                ("Intersection.lhs", Shape::circle(123.0) & (**b).clone()),
                ("Intersection.rhs", (**a).clone() & Shape::circle(123.0)),
                ("Intersection.op", (**a).clone() - (**b).clone()),
            ],
        }
    }

    /// The shape cache is keyed on nothing but [`Shape::hash`], so a recipe field
    /// the hash ignores makes two DIFFERENT shapes share one evaluated geometry.
    #[test]
    fn shape_hash_reacts_to_every_recipe_field() {
        let one = || Shape::circle(2.0);
        let other = || Shape::rounded_box([4.0, 6.0], [1.0, 2.0, 3.0, 4.0]);
        let subjects = [
            other(),
            one(),
            Shape::line([1.0, 2.0], [3.0, 4.0]),
            Shape::bezier([0.0, 0.0], [1.0, 1.0], [2.0, 1.0], [3.0, 0.0]),
            Shape::arc([1.0, 2.0], 3.0, 0.5, 1.5),
            Shape::point(0.25),
            Shape::tiling(crate::tiling::Tiling::grid(4.0, 5.0, 1.0)),
            one().translate([7.0, 8.0]),
            other() - one(),
            other() | one(),
            other() & one(),
        ];
        for subject in &subjects {
            let cases = perturbations(subject);
            assert!(
                !cases.is_empty(),
                "no perturbation covers {:?}",
                subject.expr()
            );
            for (field, perturbed) in cases {
                assert_ne!(
                    perturbed.hash(),
                    subject.hash(),
                    "Shape::hash ignores {field}: two different recipes would share \
                     one cache slot",
                );
            }
        }
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

        let body = Curve::rounded_rect_with_radii([0.0, 0.0], [70.0, 44.0], [10.0; 4]);
        let cuts: Vec<Drawable> = cuts_local.iter().map(|p| Curve::circle(*p, 4.0)).collect();
        let world = boolean::difference_many(&body, &cuts);
        assert_eq!(from_shape.segment_count(), world.segment_count());
    }

    #[test]
    fn evaluate_matches_direct_construction() {
        // RoundedBox (centred, size = 2*half) evaluates to the same local geometry
        // as the centred Curve::rounded_rect builder.
        let from_shape = Shape::rounded_box([140.0, 88.0], [10.0; 4]).evaluate();
        let direct = Curve::rounded_rect_with_radii([0.0, 0.0], [70.0, 44.0], [10.0; 4]);
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
        let world = Curve::rounded_rect_with_radii([cx, cy], [70.0, 44.0], [10.0; 4]);
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
