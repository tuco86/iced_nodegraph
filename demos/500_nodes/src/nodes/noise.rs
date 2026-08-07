use iced::{
    Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::pin;

use super::colors::{self};
use super::title_bar;

pub fn perlin_noise_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
            text("out"),
            Output,
            ::std::any::TypeId::of::<colors::Float>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Perlin", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn voronoi_noise_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
            text("out"),
            Output,
            ::std::any::TypeId::of::<colors::Float>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Voronoi", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn simplex_noise_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
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
            text("out"),
            Output,
            ::std::any::TypeId::of::<colors::Float>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Simplex", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
