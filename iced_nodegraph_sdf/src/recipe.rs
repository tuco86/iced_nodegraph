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

use std::collections::HashMap;

use crate::boolean;
use crate::curve::Curve;
use crate::drawable::Drawable;

/// A position-free shape definition. Coordinates are in the shape's local frame
/// (its declared intrinsic origin at `(0,0)`); world placement is a separate
/// per-instance translate. `evaluate` materializes the arcs; `recipe_hash` keys
/// the cache on the definition.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeExpr {
    /// Rounded rectangle centered on the local origin.
    RoundedRect { half: [f32; 2], radius: f32 },
    /// Circle at a local offset (e.g. a pin cutout relative to the body center).
    Circle { center: [f32; 2], radius: f32 },
    /// `base` with each shape in `cuts` subtracted (the pin-punched node body).
    Difference {
        base: Box<ShapeExpr>,
        cuts: Vec<ShapeExpr>,
    },
}

/// Op-code discriminants mixed into the hash so structurally different recipes
/// with coincidentally-equal operands cannot collide.
const OP_ROUNDED_RECT: u32 = 1;
const OP_CIRCLE: u32 = 2;
const OP_DIFFERENCE: u32 = 3;

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

impl ShapeExpr {
    /// Mix this recipe's op code, operands, and sub-hashes into `h`.
    fn hash_into(&self, h: &mut Fnv) {
        match self {
            ShapeExpr::RoundedRect { half, radius } => {
                h.write_u32(OP_ROUNDED_RECT);
                h.write_f32(half[0]);
                h.write_f32(half[1]);
                h.write_f32(*radius);
            }
            ShapeExpr::Circle { center, radius } => {
                h.write_u32(OP_CIRCLE);
                h.write_f32(center[0]);
                h.write_f32(center[1]);
                h.write_f32(*radius);
            }
            ShapeExpr::Difference { base, cuts } => {
                h.write_u32(OP_DIFFERENCE);
                // Compose: fold sub-hashes (each a pure function of its recipe).
                h.write_u64(base.recipe_hash());
                h.write_u32(cuts.len() as u32);
                for c in cuts {
                    h.write_u64(c.recipe_hash());
                }
            }
        }
    }

    /// Content-addressed hash of the DEFINITION (not the evaluated arcs).
    /// Placement-stable: equal for two identical shapes at different positions.
    pub fn recipe_hash(&self) -> u64 {
        let mut h = Fnv::new();
        self.hash_into(&mut h);
        h.finish()
    }

    /// Materialize the recipe to local-frame geometry (the expensive step the
    /// cache stores). Delegates to the existing evaluators.
    pub fn evaluate(&self) -> Drawable {
        match self {
            ShapeExpr::RoundedRect { half, radius } => {
                Curve::rounded_rect([0.0, 0.0], *half, *radius)
            }
            ShapeExpr::Circle { center, radius } => Curve::circle(*center, *radius),
            ShapeExpr::Difference { base, cuts } => {
                let base_d = base.evaluate();
                let cut_ds: Vec<Drawable> = cuts.iter().map(ShapeExpr::evaluate).collect();
                boolean::difference_many(&base_d, &cut_ds)
            }
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
    pub fn get_or_eval(&mut self, recipe: &ShapeExpr) -> &Drawable {
        let h = recipe.recipe_hash();
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

    fn node_body() -> ShapeExpr {
        ShapeExpr::Difference {
            base: Box::new(ShapeExpr::RoundedRect {
                half: [70.0, 44.0],
                radius: 10.0,
            }),
            cuts: vec![
                ShapeExpr::Circle {
                    center: [-70.0, -20.0],
                    radius: 4.0,
                },
                ShapeExpr::Circle {
                    center: [70.0, 20.0],
                    radius: 4.0,
                },
            ],
        }
    }

    #[test]
    fn identical_recipes_hash_equal() {
        // Two independently-built recipes for the SAME shape: the dedup property.
        assert_eq!(node_body().recipe_hash(), node_body().recipe_hash());
    }

    #[test]
    fn differing_operands_hash_differently() {
        let a = ShapeExpr::RoundedRect {
            half: [70.0, 44.0],
            radius: 10.0,
        };
        let b = ShapeExpr::RoundedRect {
            half: [70.0, 44.0],
            radius: 12.0,
        };
        assert_ne!(a.recipe_hash(), b.recipe_hash());
    }

    #[test]
    fn neg_zero_and_zero_hash_equal() {
        let a = ShapeExpr::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        };
        let b = ShapeExpr::Circle {
            center: [-0.0, -0.0],
            radius: 5.0,
        };
        assert_eq!(a.recipe_hash(), b.recipe_hash());
    }

    #[test]
    fn nan_operands_hash_equal() {
        let a = ShapeExpr::Circle {
            center: [f32::NAN, 0.0],
            radius: 5.0,
        };
        let b = ShapeExpr::Circle {
            center: [f32::NAN, 0.0],
            radius: 5.0,
        };
        assert_eq!(a.recipe_hash(), b.recipe_hash());
    }

    #[test]
    fn structurally_different_same_operands_differ() {
        // Same float operands, different op: must not collide.
        let rect = ShapeExpr::RoundedRect {
            half: [5.0, 5.0],
            radius: 5.0,
        };
        let circ = ShapeExpr::Circle {
            center: [5.0, 5.0],
            radius: 5.0,
        };
        assert_ne!(rect.recipe_hash(), circ.recipe_hash());
    }

    #[test]
    fn hash_excludes_placement() {
        // The recipe is position-free, so two identical bodies hash equal; their
        // world positions live in the translate, NOT the recipe. Build the same
        // body twice (placement is never an operand here) and confirm equality
        // survives an extra cut reordering-free structure.
        let a = node_body();
        let b = node_body();
        assert_eq!(a.recipe_hash(), b.recipe_hash());
    }

    #[test]
    fn evaluate_matches_direct_construction() {
        // The recipe evaluates to the SAME local geometry as the direct builder,
        // so a cached recipe + translate reproduces the world-baked shape.
        let recipe = ShapeExpr::RoundedRect {
            half: [70.0, 44.0],
            radius: 10.0,
        };
        let from_recipe = recipe.evaluate();
        let direct = Curve::rounded_rect([0.0, 0.0], [70.0, 44.0], 10.0);
        assert_eq!(from_recipe.segment_count(), direct.segment_count());
        let a = from_recipe.bounds();
        let b = direct.bounds();
        for i in 0..4 {
            assert!(
                (a[i] - b[i]).abs() < 1e-4,
                "bounds differ at {i}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn cache_reuses_identical_shapes() {
        // The headline: N identical nodes pay for ONE boolean evaluation. A
        // 5-node and a 500-node graph hit the same floor at the shape level.
        let mut cache = ShapeCache::new(64);
        for _ in 0..500 {
            let _ = cache.get_or_eval(&node_body());
        }
        assert_eq!(cache.len(), 1, "500 identical recipes must occupy one slot");
        assert_eq!(cache.misses(), 1, "the boolean evaluates exactly once");
        assert_eq!(cache.hits(), 499);
        assert!((cache.hit_rate() - 499.0 / 500.0).abs() < 1e-6);
    }

    #[test]
    fn cache_miss_then_hit_same_geometry() {
        let mut cache = ShapeCache::new(64);
        let first = cache.get_or_eval(&node_body()).clone();
        let second = cache.get_or_eval(&node_body()).clone();
        assert_eq!(first.segment_count(), second.segment_count());
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn cache_evicts_least_recently_used() {
        // Capacity 2. Insert A, B; touch A (now B is LRU); insert C -> B evicted.
        let mut cache = ShapeCache::new(2);
        let a = ShapeExpr::Circle {
            center: [0.0, 0.0],
            radius: 1.0,
        };
        let b = ShapeExpr::Circle {
            center: [0.0, 0.0],
            radius: 2.0,
        };
        let c = ShapeExpr::Circle {
            center: [0.0, 0.0],
            radius: 3.0,
        };
        cache.get_or_eval(&a);
        cache.get_or_eval(&b);
        cache.get_or_eval(&a); // touch A so B becomes least-recently-used
        cache.get_or_eval(&c); // inserts C, must evict B
        assert_eq!(cache.len(), 2);

        // A and C are present (hits, no new miss); B is gone (a fresh miss).
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
    fn node_body_recipe_matches_world_difference() {
        // Mirrors the widget's v3 emission: a body recipe (RoundedRect minus pin
        // circles at LOCAL offsets) evaluated and translated to the body center
        // must equal the v2 world-baked `difference_many`. This validates the
        // widget builds the right recipe; the cache + translate then render it.
        let half = [80.0, 45.0];
        let radius = 8.0;
        let center = [300.0, -120.0];
        let pin_world = [
            [center[0] - half[0], center[1] - 20.0],
            [center[0] + half[0], center[1] + 15.0],
        ];
        let pin_r = 5.0;

        let recipe = ShapeExpr::Difference {
            base: Box::new(ShapeExpr::RoundedRect { half, radius }),
            cuts: pin_world
                .iter()
                .map(|p| ShapeExpr::Circle {
                    center: [p[0] - center[0], p[1] - center[1]],
                    radius: pin_r,
                })
                .collect(),
        };
        let from_recipe = recipe.evaluate().translated(center[0], center[1]);

        let body = Curve::rounded_rect(center, half, radius);
        let cuts: Vec<Drawable> = pin_world.iter().map(|p| Curve::circle(*p, pin_r)).collect();
        let world = boolean::difference_many(&body, &cuts);

        assert_eq!(from_recipe.segment_count(), world.segment_count());
        for (a, b) in from_recipe.segments.iter().zip(world.segments.iter()) {
            assert_eq!(a.segment_type, b.segment_type);
            for i in 0..4 {
                assert!(
                    (a.geom0[i] - b.geom0[i]).abs() < 1e-2,
                    "geom0 {:?} vs {:?}",
                    a.geom0,
                    b.geom0,
                );
            }
        }
    }

    #[test]
    fn local_evaluate_plus_translate_equals_world() {
        // Evaluating local then translating by the declared origin reproduces the
        // world shape - the bridge to A1's `compile_drawable_at`.
        let half = [70.0, 44.0];
        let (cx, cy) = (300.0, -150.0);
        let local = ShapeExpr::RoundedRect { half, radius: 10.0 }.evaluate();
        let placed = local.translated(cx, cy);
        let world = Curve::rounded_rect([cx, cy], half, 10.0);
        assert_eq!(placed.segment_count(), world.segment_count());
        for (ps, ws) in placed.segments.iter().zip(world.segments.iter()) {
            for i in 0..4 {
                assert!(
                    (ps.geom0[i] - ws.geom0[i]).abs() < 1e-3,
                    "geom0 differs: {:?} vs {:?}",
                    ps.geom0,
                    ws.geom0,
                );
            }
        }
    }
}
