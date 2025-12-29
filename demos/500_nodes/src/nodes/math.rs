use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self, PIN_GENERIC_IN, PIN_GENERIC_OUT, SPACING_PIN};

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

pub fn add_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
                colors::Float,
                PIN_GENERIC_IN
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
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
            colors::Float,
            PIN_GENERIC_IN
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Add", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn multiply_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
                colors::Float,
                PIN_GENERIC_IN
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
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
            colors::Float,
            PIN_GENERIC_IN
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![
        title_bar("Multiply", theme),
        container(pins).padding([4, 0])
    ]
    .width(130.0)
    .into()
}

pub fn divide_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
                colors::Float,
                PIN_GENERIC_IN
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
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
            colors::Float,
            PIN_GENERIC_IN
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Divide", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn subtract_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
                colors::Float,
                PIN_GENERIC_IN
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
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
            colors::Float,
            PIN_GENERIC_IN
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![
        title_bar("Subtract", theme),
        container(pins).padding([4, 0])
    ]
    .width(130.0)
    .into()
}

pub fn power_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(
                Left,
                0usize,
                text("val"),
                Input,
                colors::Float,
                PIN_GENERIC_IN
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                colors::Float,
                PIN_GENERIC_OUT
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(
            Left,
            2usize,
            text("exp"),
            Input,
            colors::Float,
            PIN_GENERIC_IN
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Power", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
