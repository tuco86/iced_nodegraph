//! The crate's one content hasher.
//!
//! Every cache key in the renderer - shape recipes, style records, geometry
//! blocks, the per-draw cull key - is a `u64` produced here. They are all
//! per-process keys, never persisted, so the value itself does not matter; what
//! must hold is that equal inputs hash equal on every target, which `std`'s
//! `DefaultHasher` does not promise across a native/wasm split.

use iced_wgpu::core::Color;

/// FNV-1a over little-endian bytes: deterministic and identical on native and
/// wasm.
pub(crate) struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub(crate) fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub(crate) fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    pub(crate) fn u64(&mut self, v: u64) {
        self.u32(v as u32);
        self.u32((v >> 32) as u32);
    }

    /// Folds `v` through its canonical bit pattern: `-0.0` collapses to `+0.0`
    /// and every NaN to one quiet NaN, so semantically-equal operands hash
    /// equal.
    pub(crate) fn f32(&mut self, v: f32) {
        let bits = if v.is_nan() {
            0x7fc0_0000
        } else if v == 0.0 {
            0
        } else {
            v.to_bits()
        };
        self.u32(bits);
    }

    pub(crate) fn color(&mut self, c: Color) {
        self.f32(c.r);
        self.f32(c.g);
        self.f32(c.b);
        self.f32(c.a);
    }

    pub(crate) fn finish(&self) -> u64 {
        self.0
    }
}
