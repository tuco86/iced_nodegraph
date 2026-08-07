use iced::{
    Theme,
    widget::{column, container, text},
};
use iced_nodegraph::pin;

use super::colors::{self, SPACING_PIN};
use super::title_bar;

pub fn time_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Right,
        0usize,
        text("t"),
        Output,
        ::std::any::TypeId::of::<colors::Float>()
    ),]
    .spacing(SPACING_PIN);

    column![title_bar("Time", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn uv_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Right,
        0usize,
        text("uv"),
        Output,
        ::std::any::TypeId::of::<colors::Vec2>()
    ),]
    .spacing(SPACING_PIN);

    column![title_bar("UV", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn normal_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Right,
        0usize,
        text("N"),
        Output,
        ::std::any::TypeId::of::<colors::Vec3>()
    ),]
    .spacing(SPACING_PIN);

    column![title_bar("Normal", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn position_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(
        Right,
        0usize,
        text("P"),
        Output,
        ::std::any::TypeId::of::<colors::Vec3>()
    ),]
    .spacing(SPACING_PIN);

    column![
        title_bar("Position", theme),
        container(pins).padding([4, 0])
    ]
    .width(100.0)
    .into()
}
