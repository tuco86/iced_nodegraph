//! `PinStyle`: per-pin visual style.
//!
//! A flat, concrete struct the renderer consumes directly. See [`super::node`]
//! for the override-via-struct-update pattern over [`default_pin_style`](crate::default_pin_style).
//! Color fields are [`ColorQuad`]s; a plain `Color` coerces to a solid quad.
//! Border on/off is the `border_width` sentinel (0 = no border).
//!
use super::ColorQuad;
use super::PinShape;

/// Visual style for a pin indicator, and of the hole it punches in the node
/// body.
///
/// A node styles its own pins ([`Node::pin_style`](crate::Node::pin_style)); the
/// pin widget carries no style of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct PinStyle {
    // Indicator
    /// Pin indicator color.
    pub color: ColorQuad,
    /// Radius of the drawn indicator, in world units. This is the size on
    /// screen: nothing scales it.
    pub radius: f32,
    /// Indicator shape.
    pub shape: PinShape,

    // Cutout
    /// Radius of the circular hole this pin punches in the node body, in world
    /// units. 0 leaves the body intact.
    ///
    /// Independent of [`radius`](Self::radius) on purpose: the well and the mark
    /// in it answer different questions - how far the body opens up for an
    /// arriving edge, and how big a target the pin is - and tying one to the
    /// other means neither can be set without disturbing the other.
    ///
    /// The hole is geometry, not paint: the node silhouette (and its shadow) is
    /// rebuilt when it changes, and the result is cached by shape. Varying it
    /// per [`PinStatus`](crate::PinStatus) therefore costs a cache entry per
    /// state and is the one field to leave alone across statuses.
    pub cutout_radius: f32,

    // Border (width 0 = no border)
    /// Border color.
    pub border_color: ColorQuad,
    /// Border width in world-space pixels. 0 = no border. The border is drawn
    /// OUTSIDE the indicator, so it may reach past the cutout and over the node
    /// body - which is what makes it usable as a halo.
    pub border_width: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_widget::core::Theme;

    /// The well is the node's, the mark is the pin's. Resizing one must leave
    /// the other exactly where it was.
    #[test]
    fn the_cutout_is_not_derived_from_the_indicator() {
        use crate::style::{PinStatus, default_pin_style};
        let base = default_pin_style(&Theme::Dark, PinStatus::Idle);
        let bigger = PinStyle {
            radius: base.radius * 3.0,
            ..base.clone()
        };
        assert_eq!(bigger.cutout_radius, base.cutout_radius);
    }
}
