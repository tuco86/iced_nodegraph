//! The iced themes matching rustdoc's three page themes, so an embedded demo
//! follows the page it sits in.

use iced::Theme;
use iced::theme::Palette;

/// The names rustdoc puts in `html[data-theme]`.
pub const RUSTDOC_THEMES: [&str; 3] = ["light", "dark", "ayu"];

/// The iced theme matching one of rustdoc's built-in page themes, by the name
/// rustdoc puts in `html[data-theme]`; `None` for any other name.
///
/// Background, text, primary and warning come from the page theme's
/// `--main-background-color`, `--main-color`, `--link-color` and
/// `--warning-border-color`. Rustdoc has no success or danger colours, so
/// those are iced's own for the matching brightness.
pub fn rustdoc_theme(name: &str) -> Option<Theme> {
    let (name, background, text, primary, base) = match name {
        "light" => (
            "Rustdoc Light",
            iced::color!(0xffffff),
            iced::color!(0x000000),
            iced::color!(0x3873ad),
            Palette::LIGHT,
        ),
        "dark" => (
            "Rustdoc Dark",
            iced::color!(0x353535),
            iced::color!(0xdddddd),
            iced::color!(0xd2991d),
            Palette::DARK,
        ),
        "ayu" => (
            "Rustdoc Ayu",
            iced::color!(0x0f1419),
            iced::color!(0xc5c5c5),
            iced::color!(0x39afd7),
            Palette::DARK,
        ),
        _ => return None,
    };
    Some(Theme::custom(
        name.to_owned(),
        Palette {
            background,
            text,
            primary,
            warning: iced::color!(0xff8e00),
            ..base
        },
    ))
}
