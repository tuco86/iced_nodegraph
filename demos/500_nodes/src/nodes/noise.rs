use iced::{
    Color, Length, Theme,
    alignment::Horizontal,
    widget::{column, container, row, text},
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

pub fn perlin_noise_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(
            Left,
            "in",
            Input,
            "vec2",
            Color::from_rgb(0.9, 0.7, 0.3)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "out",
            Output,
            "float",
            Color::from_rgb(0.7, 0.9, 0.7)
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
            "in",
            Input,
            "vec2",
            Color::from_rgb(0.9, 0.7, 0.3)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "out",
            Output,
            "float",
            Color::from_rgb(0.7, 0.9, 0.7)
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
            "in",
            Input,
            "vec2",
            Color::from_rgb(0.9, 0.7, 0.3)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "out",
            Output,
            "float",
            Color::from_rgb(0.7, 0.9, 0.7)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Simplex", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
