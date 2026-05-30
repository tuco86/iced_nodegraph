//! Typestate markers for style field-wrapping.
//!
//! A style struct generic over [`StyleMode`] has each field wrapped by
//! `S::Wrap<T>`: `Option<T>` in [`Partial`] (the user-facing overlay, where
//! `None` means "inherit") and `T` in [`Resolved`] (the fully populated form the
//! renderer consumes). Shared by all style structs (node, edge, pin, ...).

/// Selects per-field wrapping for a style struct.
pub trait StyleMode {
    /// `Option<T>` for [`Partial`], `T` for [`Resolved`].
    type Wrap<T>;
}

/// Overlay mode: every field is `Option<T>` (`None` = inherit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Partial;
impl StyleMode for Partial {
    type Wrap<T> = Option<T>;
}

/// Resolved mode: every field is a concrete `T`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resolved;
impl StyleMode for Resolved {
    type Wrap<T> = T;
}
