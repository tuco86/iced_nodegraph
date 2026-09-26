//! The iced themes matching the page themes the demos are embedded in, so an
//! embedded demo follows the page it sits in: rustdoc's three built-in themes
//! and the project site's phosphor storage-tube theme.

use iced::Theme;
use iced::theme::Palette;

/// Every page theme name [`page_theme`] accepts. The first three are the names
/// rustdoc puts in `html[data-theme]`; `phosphor` is the project site's.
pub const PAGE_THEMES: [&str; 4] = ["light", "dark", "ayu", "phosphor"];

/// The iced theme matching the page theme `name`; `None` for any name not in
/// [`PAGE_THEMES`].
///
/// For rustdoc's themes, background, text, primary and warning come from the
/// page theme's `--main-background-color`, `--main-color`, `--link-color` and
/// `--warning-border-color`. Rustdoc has no success or danger colours, so
/// those are iced's own for the matching brightness. `phosphor` is the site's
/// storage-tube faceplate: a dark green-grey background with phosphor-green
/// text and accents.
pub fn page_theme(name: &str) -> Option<Theme> {
    let (name, palette) = match name {
        "light" => (
            "Rustdoc Light",
            rustdoc_palette(
                iced::color!(0xffffff),
                iced::color!(0x000000),
                iced::color!(0x3873ad),
                Palette::LIGHT,
            ),
        ),
        "dark" => (
            "Rustdoc Dark",
            rustdoc_palette(
                iced::color!(0x353535),
                iced::color!(0xdddddd),
                iced::color!(0xd2991d),
                Palette::DARK,
            ),
        ),
        "ayu" => (
            "Rustdoc Ayu",
            rustdoc_palette(
                iced::color!(0x0f1419),
                iced::color!(0xc5c5c5),
                iced::color!(0x39afd7),
                Palette::DARK,
            ),
        ),
        "phosphor" => (
            "Phosphor",
            Palette {
                background: iced::color!(0x0e1712),
                text: iced::color!(0xa8f5bd),
                primary: iced::color!(0x5fe38a),
                success: iced::color!(0x5fe38a),
                warning: iced::color!(0xe3d36a),
                danger: iced::color!(0xff8a5c),
            },
        ),
        _ => return None,
    };

    Some(Theme::custom(name.to_owned(), palette))
}

/// A rustdoc page theme's palette: its own background, text and primary, the
/// warning all three rustdoc themes share, and `base` for the rest.
fn rustdoc_palette(
    background: iced::Color,
    text: iced::Color,
    primary: iced::Color,
    base: Palette,
) -> Palette {
    Palette {
        background,
        text,
        primary,
        warning: iced::color!(0xff8e00),
        ..base
    }
}
