//! Color Picker Input Node
//!
//! Outputs a configurable color value via RGB sliders or presets.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{button, column, container, row, slider, text},
};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar};

/// Predefined color presets
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorPreset {
    Red,
    Green,
    Blue,
    Yellow,
    Purple,
    Cyan,
    White,
    Gray,
}

impl ColorPreset {
    pub fn to_color(self) -> Color {
        match self {
            Self::Red => Color::from_rgb(0.9, 0.2, 0.2),
            Self::Green => Color::from_rgb(0.2, 0.8, 0.3),
            Self::Blue => Color::from_rgb(0.2, 0.4, 0.9),
            Self::Yellow => Color::from_rgb(0.9, 0.8, 0.2),
            Self::Purple => Color::from_rgb(0.7, 0.3, 0.9),
            Self::Cyan => Color::from_rgb(0.2, 0.8, 0.9),
            Self::White => Color::from_rgb(0.9, 0.9, 0.9),
            Self::Gray => Color::from_rgb(0.5, 0.5, 0.5),
        }
    }

    pub fn all() -> &'static [ColorPreset] {
        &[
            Self::Red,
            Self::Green,
            Self::Blue,
            Self::Yellow,
            Self::Purple,
            Self::Cyan,
            Self::White,
            Self::Gray,
        ]
    }
}

/// Creates a color picker node with RGB sliders
pub fn color_picker_node<'a, Message>(
    theme: &'a iced::Theme,
    color: Color,
    on_change: impl Fn(Color) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    // Preview box showing current color (larger for better visibility)
    let preview = container(text(""))
        .width(44)
        .height(32)
        .style(move |_theme| container::Style {
            background: Some(iced::Background::Color(color)),
            border: iced::Border {
                color: colors::PIN_ANY,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

    // RGB value display
    let rgb_display = text(format!(
        "R:{:.0} G:{:.0} B:{:.0}",
        color.r * 255.0,
        color.g * 255.0,
        color.b * 255.0
    ))
    .size(10);

    // RGB sliders
    let on_change_r = on_change.clone();
    let on_change_g = on_change.clone();
    let on_change_b = on_change.clone();

    let r_slider = row![
        text("R").size(10).width(12),
        slider(0.0..=1.0, color.r, move |r| {
            on_change_r(Color::from_rgb(r, color.g, color.b))
        })
        .step(0.01)
        .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let g_slider = row![
        text("G").size(10).width(12),
        slider(0.0..=1.0, color.g, move |g| {
            on_change_g(Color::from_rgb(color.r, g, color.b))
        })
        .step(0.01)
        .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let b_slider = row![
        text("B").size(10).width(12),
        slider(0.0..=1.0, color.b, move |b| {
            on_change_b(Color::from_rgb(color.r, color.g, b))
        })
        .step(0.01)
        .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let output_pin = container(pin!(
        Right,
        text("color").size(10),
        Output,
        "color",
        colors::PIN_COLOR
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Color", style),
        container(
            column![
                row![preview, rgb_display]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                r_slider,
                g_slider,
                b_slider,
                output_pin,
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(200.0)
    .into()
}

/// Creates a simpler color picker with preset buttons
pub fn color_preset_node<'a, Message>(
    theme: &'a iced::Theme,
    current: Color,
    on_select: impl Fn(Color) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    // Preview box (full-width bar)
    let preview = container(text(""))
        .width(Length::Fill)
        .height(24)
        .style(move |_theme| container::Style {
            background: Some(iced::Background::Color(current)),
            border: iced::Border {
                color: colors::PIN_ANY,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    // Color preset buttons (2 rows of 4) - larger buttons
    let presets = ColorPreset::all();
    let row1: Vec<_> = presets[0..4]
        .iter()
        .map(|preset| {
            let color = preset.to_color();
            let on_select = on_select.clone();
            button(text(""))
                .width(24)
                .height(24)
                .style(move |_theme, _status| button::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        color: colors::PIN_ANY,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(on_select(color))
                .into()
        })
        .collect();

    let row2: Vec<_> = presets[4..8]
        .iter()
        .map(|preset| {
            let color = preset.to_color();
            let on_select = on_select.clone();
            button(text(""))
                .width(24)
                .height(24)
                .style(move |_theme, _status| button::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        color: colors::PIN_ANY,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(on_select(color))
                .into()
        })
        .collect();

    let output_pin = container(pin!(
        Right,
        text("color").size(10),
        Output,
        "color",
        colors::PIN_COLOR
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Color Preset", style),
        container(
            column![
                preview,
                row(row1).spacing(6),
                row(row2).spacing(6),
                output_pin,
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(160.0)
    .into()
}
