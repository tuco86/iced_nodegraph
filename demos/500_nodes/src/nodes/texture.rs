use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self, PIN_FLOAT, PIN_VEC2, PIN_VEC4, SPACING_PIN};

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

pub fn sampler2d_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(Left, "uv", text(""), Input, colors::Vec2, PIN_VEC2))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "rgba",
            text(""),
            Output,
            colors::Vec4,
            PIN_VEC4
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Texture", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn color_mix_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "A", text(""), Input, colors::Vec4, PIN_VEC4))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "out", text(""), Output, colors::Vec4, PIN_VEC4))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "B", text(""), Input, colors::Vec4, PIN_VEC4))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Mix", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn gradient_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(Left, "t", text(""), Input, colors::Float, PIN_FLOAT))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
        container(pin!(Right, "col", text(""), Output, colors::Vec4, PIN_VEC4))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![
        title_bar("Gradient", theme),
        container(pins).padding([4, 0])
    ]
    .width(130.0)
    .into()
}
