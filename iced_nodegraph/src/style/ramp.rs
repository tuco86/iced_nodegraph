//! Perceptual color ramp the theme defaults are built from.
//!
//! Two ladders carry the whole default mapping, and both are defined in Oklab so
//! one step means the same thing in every palette:
//!
//! - an ELEVATION ladder ([`shift`]), which moves a surface away from the canvas
//!   in lightness while holding hue and chroma exactly - the node body and the
//!   grid are rungs on it;
//! - a LEGIBILITY ladder ([`blend`]), which places a mark at a fixed fraction of
//!   the distance between the canvas and the theme's own foreground - the edge
//!   stroke and the pin indicator are rungs on it.
//!
//! [`separate`] guards the two semantic accents (`primary`, `danger`), which are
//! authored per theme and therefore cannot be trusted to contrast with the
//! canvas at all: `Theme::KanagawaDragon` pairs a `#181616` background with a
//! `#223249` primary.
//!
//! Working in Oklab rather than iced's `palette::deviate` is deliberate. That
//! helper multiplies chroma as it moves lightness (`c *= 1 + 2 * amount / l`), so
//! on a dark, saturated canvas one ramp step also doubles saturation. A widget
//! that tints a handful of small controls absorbs that; a node graph tiles the
//! whole viewport with canvas, grid and node bodies, where it reads as a colored
//! slab under a colored lattice.

use iced_widget::core::Color;

/// A color in Oklab: perceptual lightness plus the two opponent-color axes.
///
/// Holding `a`/`b` while moving `l` preserves both hue and chroma, which is what
/// makes an elevation step theme-independent.
struct Oklab {
    l: f32,
    a: f32,
    b: f32,
    alpha: f32,
}

// https://en.wikipedia.org/wiki/Oklab_color_space#Conversions_between_color_spaces
fn to_oklab(color: Color) -> Oklab {
    let [r, g, b, alpha] = color.into_linear();

    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;

    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());

    Oklab {
        l: 0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        a: 1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        b: 0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
        alpha,
    }
}

/// Converts back to sRGB, clamping into gamut.
///
/// A lightness shift on an already saturated color can leave the sRGB gamut; the
/// clamp then trades a little chroma for the requested lightness, which is the
/// right direction for a contrast guard.
fn from_oklab(color: Oklab) -> Color {
    let Oklab { l, a, b, alpha } = color;

    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;

    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    Color::from_linear_rgba(
        (4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s).clamp(0.0, 1.0),
        (-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s).clamp(0.0, 1.0),
        (-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s).clamp(0.0, 1.0),
        alpha,
    )
}

/// Perceptual lightness, 0 at black and 1 at white.
pub(crate) fn lightness(color: Color) -> f32 {
    to_oklab(color).l
}

/// Moves `color` `delta` along perceptual lightness, holding hue and chroma.
///
/// The elevation ladder: positive raises (a surface over a dark canvas),
/// negative lowers (a surface over a light one).
pub(crate) fn shift(color: Color, delta: f32) -> Color {
    let base = to_oklab(color);
    from_oklab(Oklab {
        l: (base.l + delta).clamp(0.0, 1.0),
        ..base
    })
}

/// The point `t` of the way from `from` to `to` along a straight line in Oklab.
///
/// The legibility ladder: `blend(canvas, foreground, t)` places a mark at the
/// same perceptual fraction of every theme's own contrast range, so it can
/// neither vanish into the canvas nor outshine the text.
pub(crate) fn blend(from: Color, to: Color, t: f32) -> Color {
    let (a, b) = (to_oklab(from), to_oklab(to));
    from_oklab(Oklab {
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
        b: a.b + (b.b - a.b) * t,
        alpha: a.alpha + (b.alpha - a.alpha) * t,
    })
}

/// Pushes `color` away from `reference` until the two differ by at least
/// `min_delta` in perceptual lightness, keeping hue and chroma.
///
/// Applied to the authored accents, whose contrast against the canvas is a
/// property of the theme rather than of this crate. It moves in the direction
/// `color` already leans, and flips when that direction has run out of range.
pub(crate) fn separate(color: Color, reference: Color, min_delta: f32) -> Color {
    let base = to_oklab(color);
    let anchor = lightness(reference);

    if (base.l - anchor).abs() >= min_delta {
        return color;
    }

    let up = anchor + min_delta;
    let down = anchor - min_delta;
    let target = if base.l >= anchor {
        if up <= 1.0 { up } else { down }
    } else if down >= 0.0 {
        down
    } else {
        up
    };

    from_oklab(Oklab {
        l: target.clamp(0.0, 1.0),
        ..base
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-tripping must not drift, or every ramp step would accumulate error.
    #[test]
    fn oklab_round_trips() {
        for c in [
            Color::BLACK,
            Color::WHITE,
            Color::from_rgb(0.1, 0.2, 0.35),
            Color::from_rgb(0.95, 0.3, 0.1),
        ] {
            let back = from_oklab(to_oklab(c));
            for (a, b) in [(c.r, back.r), (c.g, back.g), (c.b, back.b)] {
                assert!((a - b).abs() < 1e-3, "{c:?} round-tripped to {back:?}");
            }
        }
    }

    /// The point of working in Oklab: a step keeps the hue and the saturation it
    /// started with. iced's `palette::lighten` deliberately does not - it scales
    /// chroma by `1 + 2 * amount / l`, which is what this ladder exists to avoid.
    #[test]
    fn a_lightness_step_holds_hue_and_chroma() {
        let navy = Color::from_rgb8(0x01, 0x16, 0x27);
        let raised = shift(navy, 0.1);

        let (before, after) = (to_oklab(navy), to_oklab(raised));
        assert!(
            (after.l - before.l - 0.1).abs() < 1e-3,
            "lightness moved by 0.1"
        );
        assert!(
            (after.a - before.a).abs() < 1e-3,
            "hue/chroma axis a is held"
        );
        assert!(
            (after.b - before.b).abs() < 1e-3,
            "hue/chroma axis b is held"
        );

        let iced_step = iced_widget::core::theme::palette::lighten(navy, 0.1);
        let iced_chroma = to_oklab(iced_step);
        let chroma = |o: &Oklab| (o.a * o.a + o.b * o.b).sqrt();
        assert!(
            chroma(&iced_chroma) > chroma(&before) * 1.5,
            "iced's step is expected to amplify chroma; this ladder must not"
        );
    }

    /// A ladder step must stay a step at both ends of the lightness range: a
    /// near-black and a near-white canvas get the same perceptual elevation.
    #[test]
    fn elevation_is_uniform_across_the_range() {
        let near_black = Color::from_rgb8(0x08, 0x08, 0x08);
        let near_white = Color::from_rgb8(0xef, 0xf1, 0xf5);

        let dark_step = lightness(shift(near_black, 0.06)) - lightness(near_black);
        let light_step = lightness(near_white) - lightness(shift(near_white, -0.06));

        assert!((dark_step - 0.06).abs() < 1e-3);
        assert!((light_step - 0.06).abs() < 1e-3);
    }

    /// The guard is the reason `Theme::KanagawaDragon` has a visible selection at
    /// all: its authored primary sits 0.09 in lightness from its own background.
    #[test]
    fn separate_lifts_an_accent_that_collides_with_its_canvas() {
        let canvas = Color::from_rgb8(0x18, 0x16, 0x16);
        let accent = Color::from_rgb8(0x22, 0x32, 0x49);
        assert!(
            (lightness(accent) - lightness(canvas)).abs() < 0.3,
            "precondition"
        );

        let guarded = separate(accent, canvas, 0.32);
        assert!((lightness(guarded) - lightness(canvas)).abs() >= 0.315);
    }

    /// An accent that already clears the floor must come back untouched, so a
    /// theme that got its accent right keeps it exactly.
    #[test]
    fn separate_leaves_a_contrasting_accent_alone() {
        let canvas = Color::from_rgb8(0x1e, 0x1e, 0x2e);
        let accent = Color::from_rgb8(0x89, 0xb4, 0xfa);
        assert_eq!(separate(accent, canvas, 0.32), accent);
    }

    /// Pushing away from a near-white reference must flip downward rather than
    /// clamp at white and silently return an invisible accent.
    #[test]
    fn separate_flips_when_the_leaning_direction_has_no_room() {
        let canvas = Color::WHITE;
        let accent = Color::from_rgb8(0xf0, 0xf0, 0xf0);
        let guarded = separate(accent, canvas, 0.32);
        assert!(lightness(guarded) <= lightness(canvas) - 0.315);
    }
}
