use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self, SPACING_PIN};
use super::title_bar;

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
                ::std::any::TypeId::of::<colors::Float>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                ::std::any::TypeId::of::<colors::Float>()
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
            ::std::any::TypeId::of::<colors::Float>()
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
                ::std::any::TypeId::of::<colors::Float>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                ::std::any::TypeId::of::<colors::Float>()
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
            ::std::any::TypeId::of::<colors::Float>()
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
                ::std::any::TypeId::of::<colors::Float>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                ::std::any::TypeId::of::<colors::Float>()
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
            ::std::any::TypeId::of::<colors::Float>()
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
                ::std::any::TypeId::of::<colors::Float>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                ::std::any::TypeId::of::<colors::Float>()
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
            ::std::any::TypeId::of::<colors::Float>()
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
                ::std::any::TypeId::of::<colors::Float>()
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                1usize,
                text(""),
                Output,
                ::std::any::TypeId::of::<colors::Float>()
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
            ::std::any::TypeId::of::<colors::Float>()
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(SPACING_PIN);

    column![title_bar("Power", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
