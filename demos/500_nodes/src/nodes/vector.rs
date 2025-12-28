use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self, PIN_GENERIC_OUT, PIN_VEC3, PIN_X, PIN_Y, PIN_Z, SPACING_PIN};

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

pub fn vector_split_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "vec", text(""), Input, colors::Vec3, PIN_VEC3))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "x", text(""), Output, colors::Float, PIN_X))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Right, "y", text(""), Output, colors::Float, PIN_Y))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
        container(pin!(Right, "z", text(""), Output, colors::Float, PIN_Z))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Split", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn vector_combine_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "x", text(""), Input, colors::Float, PIN_X))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "vec", text(""), Output, colors::Vec3, PIN_VEC3))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "y", text(""), Input, colors::Float, PIN_Y))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
        container(pin!(Left, "z", text(""), Input, colors::Float, PIN_Z))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Combine", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn normalize_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(Left, "in", text(""), Input, colors::Vec3, PIN_VEC3))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
        container(pin!(Right, "out", text(""), Output, colors::Vec3, PIN_VEC3))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![
        title_bar("Normalize", theme),
        container(pins).padding([4, 0])
    ]
    .width(130.0)
    .into()
}

pub fn dot_product_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "A", text(""), Input, colors::Vec3, PIN_VEC3))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(
                Right,
                "out",
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "B", text(""), Input, colors::Vec3, PIN_VEC3))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Dot", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn cross_product_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "A", text(""), Input, colors::Vec3, PIN_VEC3))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "out", text(""), Output, colors::Vec3, PIN_VEC3))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "B", text(""), Input, colors::Vec3, PIN_VEC3))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Cross", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
