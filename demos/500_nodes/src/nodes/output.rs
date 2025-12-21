use iced::{
    Color, Length, Theme,
    widget::{column, container, text},
};
use iced_nodegraph::pin;

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
    let pins = column![pin!(
        Left,
        "col",
        Input,
        "vec4",
        Color::from_rgb(0.9, 0.5, 0.9)
    ),]
    .spacing(1);

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
        Input,
        "float",
        Color::from_rgb(0.9, 0.9, 0.9)
    ),]
    .spacing(1);

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
        Input,
        "float",
        Color::from_rgb(0.9, 0.9, 0.9)
    ),]
    .spacing(1);

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
        Input,
        "vec4",
        Color::from_rgb(0.9, 0.9, 0.3)
    ),]
    .spacing(1);

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
    let pins = column![pin!(
        Left,
        "N",
        Input,
        "vec3",
        Color::from_rgb(0.5, 0.7, 0.9)
    ),]
    .spacing(1);

    column![title_bar("Normal", theme), container(pins).padding([4, 0])]
        .width(140.0)
        .into()
}
