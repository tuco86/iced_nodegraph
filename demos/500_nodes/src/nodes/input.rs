use iced::{
    Length, Theme,
    widget::{column, container, text},
};
use iced_nodegraph::pin;

use super::colors::{PIN_FLOAT, PIN_NORMAL, PIN_POSITION, PIN_VEC2, SPACING_PIN};

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

pub fn time_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "t", Output, "float", PIN_FLOAT),].spacing(SPACING_PIN);

    column![title_bar("Time", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn uv_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "uv", Output, "vec2", PIN_VEC2),].spacing(SPACING_PIN);

    column![title_bar("UV", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn normal_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "N", Output, "vec3", PIN_NORMAL),].spacing(SPACING_PIN);

    column![title_bar("Normal", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn position_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "P", Output, "vec3", PIN_POSITION),].spacing(SPACING_PIN);

    column![
        title_bar("Position", theme),
        container(pins).padding([4, 0])
    ]
    .width(100.0)
    .into()
}
