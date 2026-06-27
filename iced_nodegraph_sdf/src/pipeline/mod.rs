//! GPU pipeline infrastructure for SDF rendering.

pub(crate) mod buffer;
pub(crate) mod types;

/// Static-background texture cache: blits a repeated static background instead of
/// re-rendering it, while a changing background still renders direct.
pub(crate) mod bg_cache;

#[cfg(test)]
mod pixel_tests;
