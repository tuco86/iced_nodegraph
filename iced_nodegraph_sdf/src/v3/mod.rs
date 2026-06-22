//! Normalized / instanced SDF backend (v3), gated behind the `sdf-v3` feature.
//!
//! v3 decomposes every drawn thing into three content-addressed axes (shape,
//! band, pattern) plus an instance/command table, so identical shapes are
//! composed and uploaded once and per-frame cost tracks visible geometry rather
//! than screen area x layer count. See `plan/sdf-normalized-renderer.md` for the
//! full architecture, decisions, and phase plan.
//!
//! Status: under construction behind the default-off `sdf-v3` feature. v2 stays
//! the shipping default until v3 meets the acceptance bar (golden
//! pixel-equivalence + no per-frame cost regression) on every corpus scene.
//! Selecting the backend happens behind the unchanged `SdfPrimitive` / widget
//! API, so enabling the feature is not a public-API break.

/// Whether the v3 backend is compiled in. Lets the widget and tests branch on
/// the active backend without `cfg` plumbing at every call site.
pub const ENABLED: bool = true;
