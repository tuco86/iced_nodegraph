//! `AnchorStyle`: the core an anchor is grabbed by, plus the orbit geometry
//! cables wrap it on.
//!
//! A flat, concrete struct the renderer consumes directly, following the
//! override-via-struct-update pattern over
//! [`default_anchor_style`](super::default_anchor_style) that every other
//! style type uses. Color fields are [`ColorQuad`]s; a plain `Color` coerces
//! to a solid quad. Border on/off is the `core_border_width` sentinel (0 = no
//! border).
//!
//! `orbit_offset`/`orbit_spacing` are GEOMETRY, not paint: they are the radii
//! a cable is laid tangent to, resolved once per frame before the path is
//! built, so changing them moves the cable rather than recoloring it. Orbit
//! `k` has radius `orbit_offset + k * orbit_spacing`.

use super::ColorQuad;

/// Visual style of an anchor and the radii of its orbits.
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorStyle {
    /// Core side length in world units.
    pub core_size: f32,
    /// Corner radius of the core.
    pub core_radius: f32,
    /// Core fill color.
    pub core_color: ColorQuad,
    /// Core border color.
    pub core_border_color: ColorQuad,
    /// Core border width. 0 = no border.
    pub core_border_width: f32,
    /// Radius of orbit 0.
    pub orbit_offset: f32,
    /// Additional radius per orbit.
    pub orbit_spacing: f32,
    /// Ring color of an orbit a cable is attached to. 0-width ring = off.
    pub ring_color: ColorQuad,
    /// Ring stroke width.
    pub ring_width: f32,
}

impl AnchorStyle {
    /// Radius of orbit `k`.
    pub fn orbit_radius(&self, orbit: u8) -> f32 {
        self.orbit_offset + orbit as f32 * self.orbit_spacing
    }
}
