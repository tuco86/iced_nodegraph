//! The one place an iced [`Theme`] becomes node-graph colors.
//!
//! Every `default_*_style` and [`GraphStyle::from_theme`] reads its colors from
//! [`Roles`], so the mapping is defined once for all themes instead of once per
//! element. What each role means, and the ladder it sits on, is the contract; the
//! constants below are the whole tuning surface.
//!
//! [`GraphStyle::from_theme`]: crate::GraphStyle::from_theme

use iced_widget::core::{Color, Theme};

use super::ramp;

/// Perceptual lightness between the canvas and the grid lines drawn on it.
///
/// The grid is an orientation aid, not content: it must resolve when looked for
/// and disappear when not.
const GRID_ELEVATION: f32 = 0.045;

/// Perceptual lightness between the canvas and a node body.
///
/// Small on purpose. Node content is hosted iced widgets, which the application
/// colors for the theme's *window* background; keeping the body within a hair of
/// the canvas is what makes that text readable on it. The silhouette is carried
/// by the border and the shadow, not by the fill.
const BODY_ELEVATION: f32 = 0.06;

/// Perceptual lightness between the canvas and a node border.
///
/// Large enough to hold the silhouette on its own, since nodes overlap each other
/// and a shadow alone cannot separate two stacked bodies.
const BORDER_ELEVATION: f32 = 0.17;

/// Where an edge sits between the canvas and the theme's foreground.
///
/// Edges are the graph's structure and cross both canvas and node bodies, so they
/// need real contrast - but they must stay under the text they run past.
const WIRE_LEGIBILITY: f32 = 0.58;

/// Where a pin indicator sits between the canvas and the theme's foreground.
///
/// Above the wire it terminates: a pin is a target you aim at, the wire is not.
const TERMINAL_LEGIBILITY: f32 = 0.78;

/// Minimum perceptual lightness between an authored accent and the canvas.
///
/// `primary` and `danger` are chosen by the theme author with no promise about
/// the background they land on - `Theme::KanagawaDragon` puts `#223249` on
/// `#181616` - so the accents are floored rather than trusted.
const ACCENT_SEPARATION: f32 = 0.34;

/// The node-graph colors of one theme.
///
/// Three groups, and the group decides how a color is derived:
///
/// - SURFACES ([`canvas`](Self::canvas), [`grid`](Self::grid),
///   [`body`](Self::body), [`border`](Self::border)) step away from the window
///   background in perceptual lightness, holding its hue and chroma. They read as
///   one material at different elevations.
/// - MARKS ([`wire`](Self::wire), [`terminal`](Self::terminal)) sit at a fixed
///   fraction of the distance from the canvas to the theme's foreground, so every
///   theme places them the same way inside its own contrast range.
/// - ACCENTS ([`accent`](Self::accent), [`valid`](Self::valid),
///   [`danger`](Self::danger)) keep the theme's authored hue, floored to a
///   minimum separation from the canvas.
///
/// Each accent means exactly one thing: [`accent`](Self::accent) is selection,
/// [`valid`](Self::valid) is a connection that would be accepted, and
/// [`danger`](Self::danger) is destruction. Nothing else may borrow them - which
/// is why an idle pin is a mark and not an accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Roles {
    /// The infinite plane. The theme's window background, untouched.
    pub canvas: Color,
    /// The tiling drawn over the canvas.
    pub grid: Color,
    /// A node body.
    pub body: Color,
    /// A node's silhouette against the canvas.
    pub border: Color,
    /// An edge stroke.
    pub wire: Color,
    /// A pin indicator: the wire's endpoint, one rung brighter.
    pub terminal: Color,
    /// Selection, and nothing else.
    pub accent: Color,
    /// A pin the in-flight edge would connect to, and nothing else.
    pub valid: Color,
    /// Destruction (the cutting trail), and nothing else.
    pub danger: Color,
    /// Whether the canvas is dark, and therefore which way the surfaces step.
    pub is_dark: bool,
}

impl Roles {
    /// Derives the node-graph colors of `theme`.
    pub fn of(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let canvas = palette.background.base.color;
        // `Pair::text` is iced's readability-corrected foreground for this exact
        // background, so the legibility ladder is anchored on a color already
        // known to resolve against the canvas.
        let foreground = palette.background.base.text;
        let up = if palette.is_dark { 1.0 } else { -1.0 };

        Self {
            canvas,
            grid: ramp::shift(canvas, up * GRID_ELEVATION),
            body: ramp::shift(canvas, up * BODY_ELEVATION),
            border: ramp::shift(canvas, up * BORDER_ELEVATION),
            wire: ramp::blend(canvas, foreground, WIRE_LEGIBILITY),
            terminal: ramp::blend(canvas, foreground, TERMINAL_LEGIBILITY),
            accent: ramp::separate(palette.primary.base.color, canvas, ACCENT_SEPARATION),
            valid: ramp::separate(palette.success.base.color, canvas, ACCENT_SEPARATION),
            danger: ramp::separate(palette.danger.base.color, canvas, ACCENT_SEPARATION),
            is_dark: palette.is_dark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Perceptual lightness distance, the unit both ladders are measured in.
    fn gap(a: Color, b: Color) -> f32 {
        (ramp::lightness(a) - ramp::lightness(b)).abs()
    }

    /// Signed lightness distance from the canvas: positive means "away from the
    /// canvas in the direction the theme elevates".
    fn rise(roles: &Roles, color: Color) -> f32 {
        let signed = ramp::lightness(color) - ramp::lightness(roles.canvas);
        if roles.is_dark { signed } else { -signed }
    }

    /// The surfaces form one ordered ladder in every theme, and every rung is the
    /// same perceptual height everywhere. A theme whose background sits near an
    /// end of the lightness range must not get a collapsed step: that is exactly
    /// where the elevation would silently disappear.
    #[test]
    fn the_elevation_ladder_holds_in_every_theme() {
        for theme in Theme::ALL {
            let roles = Roles::of(theme);
            let (grid, body, border) = (
                rise(&roles, roles.grid),
                rise(&roles, roles.body),
                rise(&roles, roles.border),
            );

            assert!(
                grid < body && body < border,
                "{theme}: surfaces out of order (grid {grid:.3}, body {body:.3}, \
                 border {border:.3})",
            );
            for (name, got, want) in [
                ("grid", grid, GRID_ELEVATION),
                ("body", body, BODY_ELEVATION),
                ("border", border, BORDER_ELEVATION),
            ] {
                assert!(
                    (got - want).abs() < 0.01,
                    "{theme}: {name} rose {got:.3} instead of {want:.3}",
                );
            }
        }
    }

    /// A node body carries hosted iced widgets, which the application colors for
    /// the theme's WINDOW background - the widget never gets to restyle them. So
    /// every step of elevation is paid for in text contrast, and the bill must be
    /// the same in all 22 themes rather than falling on the two that have the
    /// least to give (`SolarizedLight` and `KanagawaLotus` clear iced's own
    /// readability bar by 0.2 on their own background).
    ///
    /// Retention, not an absolute floor, is therefore the contract: whatever
    /// contrast a theme has, a node body keeps at least three quarters of it.
    /// Across the built-in set the realized figure lands between 79% and 92%,
    /// which is the evidence that one elevation constant costs every palette
    /// roughly the same.
    #[test]
    fn a_node_body_keeps_most_of_the_canvas_text_contrast() {
        for theme in Theme::ALL {
            let roles = Roles::of(theme);
            let text = theme.extended_palette().background.base.text;
            let retained =
                roles.body.relative_contrast(text) / roles.canvas.relative_contrast(text);
            assert!(
                retained >= 0.78,
                "{theme}: a node body keeps only {:.0}% of the canvas text contrast",
                retained * 100.0,
            );
        }
    }

    /// Marks must resolve against BOTH surfaces they cross - a pin straddles the
    /// node silhouette and an edge runs over canvas and bodies alike - and the pin
    /// must stay the brighter of the two, since it is the thing you aim at.
    #[test]
    fn marks_resolve_against_canvas_and_body_in_every_theme() {
        for theme in Theme::ALL {
            let roles = Roles::of(theme);

            for (name, mark, floor) in [
                ("wire", roles.wire, 0.25),
                ("terminal", roles.terminal, 0.35),
            ] {
                assert!(
                    gap(mark, roles.canvas) >= floor,
                    "{theme}: {name} is {:.3} from the canvas, under {floor}",
                    gap(mark, roles.canvas),
                );
                assert!(
                    gap(mark, roles.body) >= floor - 0.05,
                    "{theme}: {name} is {:.3} from a node body, under {:.2}",
                    gap(mark, roles.body),
                    floor - 0.05,
                );
            }

            assert!(
                rise(&roles, roles.terminal) > rise(&roles, roles.wire),
                "{theme}: a pin must read above the wire it terminates",
            );
        }
    }

    /// The authored accents are floored against the canvas AND against a node
    /// body, because that is what a selection border is drawn around. Without the
    /// guard `Theme::KanagawaDragon` selects a node in a color 0.09 from its own
    /// background, and `Theme::Ferra` cuts edges in one barely off its own text.
    #[test]
    fn accents_separate_from_both_surfaces_in_every_theme() {
        for theme in Theme::ALL {
            let roles = Roles::of(theme);
            for (name, accent) in [
                ("accent", roles.accent),
                ("valid", roles.valid),
                ("danger", roles.danger),
            ] {
                assert!(
                    gap(accent, roles.canvas) >= 0.30,
                    "{theme}: {name} is {:.3} from the canvas",
                    gap(accent, roles.canvas),
                );
                assert!(
                    gap(accent, roles.body) >= 0.20,
                    "{theme}: {name} is {:.3} from a node body",
                    gap(accent, roles.body),
                );
            }
        }
    }

    /// An accent means one thing. Selection, an accepting drop target and
    /// destruction must never resolve to the same paint, whatever a theme does
    /// with `primary`, `success` and `danger`.
    #[test]
    fn the_three_accents_never_collide() {
        for theme in Theme::ALL {
            let roles = Roles::of(theme);
            for (a, an, b, bn) in [
                (roles.accent, "selection", roles.danger, "destruction"),
                (roles.accent, "selection", roles.valid, "a valid target"),
                (roles.valid, "a valid target", roles.danger, "destruction"),
            ] {
                assert_ne!(a, b, "{theme}: {an} and {bn} resolve to one color");
            }
        }
    }
}
