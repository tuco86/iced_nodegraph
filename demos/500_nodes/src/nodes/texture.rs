use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self, SPACING_PIN};

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
        container(pin!(
            Left,
            0usize,
            text("uv"),
            Input,
            ::std::any::TypeId::of::<colors::Vec2>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            1usize,
            text("rgba"),
            Output,
            ::std::any::TypeId::of::<colors::Vec4>()
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
            container(pin!(
                Left,
                0usize,
                text("A"),
                Input,
                ::std::any::TypeId::of::<colors::Vec4>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text("out"),
                Output,
                ::std::any::TypeId::of::<colors::Vec4>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(
            Left,
            2usize,
            text("B"),
            Input,
            ::std::any::TypeId::of::<colors::Vec4>()
        ))
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
        container(pin!(
            Left,
            0usize,
            text("t"),
            Input,
            ::std::any::TypeId::of::<colors::Float>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            1usize,
            text("rgba"),
            Output,
            ::std::any::TypeId::of::<colors::Vec4>()
        ))
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
