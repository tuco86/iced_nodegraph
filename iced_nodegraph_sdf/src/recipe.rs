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

#![allow(dead_code)]

use crate::boolean;
use crate::curve::Curve;
use crate::drawable::Drawable;

/// A position-free shape definition. Coordinates are in the shape's local frame
/// (its declared intrinsic origin at `(0,0)`); world placement is a separate
/// per-instance translate. `evaluate` materializes the arcs; `recipe_hash` keys
/// the cache on the definition.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ShapeExpr {
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
    pub(crate) fn recipe_hash(&self) -> u64 {
        let mut h = Fnv::new();
        self.hash_into(&mut h);
        h.finish()
    }

    /// Materialize the recipe to local-frame geometry (the expensive step the
    /// cache stores). Delegates to the existing evaluators.
    pub(crate) fn evaluate(&self) -> Drawable {
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
