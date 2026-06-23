//! GPU pipeline infrastructure for SDF rendering.

pub(crate) mod buffer;
pub(crate) mod types;

/// Static-background texture cache (Phase C). Only the v3 backend populates it;
/// gated so the v2 shipping path is byte-identical.
#[cfg(feature = "sdf-v3")]
pub(crate) mod bg_cache;

#[cfg(test)]
mod pixel_tests;
