use iced::{
    Length, Theme,
    widget::{column, container, text},
};
use iced_nodegraph::pin;

use super::colors::{self, PIN_EMISSION, PIN_GENERIC_OUT, PIN_NORMAL, PIN_VEC4, SPACING_PIN};

fn title_bar<'a, Message>(
    title: &'a str,
    theme: &'a Theme,
) -> iced::widget::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    let palette = theme.extended_palette();
    container(text(title).size(12).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_: &Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        })
}

pub fn base_color_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins =
        column![pin!(Left, "col", text(""), Input, colors::Vec4, PIN_VEC4),].spacing(SPACING_PIN);

    column![
        title_bar("Base Color", theme),
        container(pins).padding([4, 0])
    ]
    .width(140.0)
    .into()
}

pub fn roughness_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Left,
        "val",
        text(""),
        Input,
        colors::Float,
        PIN_GENERIC_OUT
    ),]
    .spacing(SPACING_PIN);

    column![
        title_bar("Roughness", theme),
        container(pins).padding([4, 0])
    ]
    .width(140.0)
    .into()
}

pub fn metallic_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Left,
        "val",
        text(""),
        Input,
        colors::Float,
        PIN_GENERIC_OUT
    ),]
    .spacing(SPACING_PIN);

    column![
        title_bar("Metallic", theme),
        container(pins).padding([4, 0])
    ]
    .width(140.0)
    .into()
}

pub fn emission_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Left,
        "col",
        text(""),
        Input,
        colors::Vec4,
        PIN_EMISSION
    ),]
    .spacing(SPACING_PIN);

    column![
        title_bar("Emission", theme),
        container(pins).padding([4, 0])
    ]
    .width(140.0)
    .into()
}

pub fn normal_output_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins =
        column![pin!(Left, "N", text(""), Input, colors::Vec3, PIN_NORMAL),].spacing(SPACING_PIN);

    column![title_bar("Normal", theme), container(pins).padding([4, 0])]
        .width(140.0)
        .into()
}
