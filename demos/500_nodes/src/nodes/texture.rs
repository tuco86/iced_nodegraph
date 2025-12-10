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

pub fn sampler2d_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(
            Left,
            "uv",
            Input,
            "vec2",
            Color::from_rgb(0.9, 0.7, 0.3)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "rgba",
            Output,
            "vec4",
            Color::from_rgb(0.9, 0.5, 0.9)
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
                "A",
                Input,
                "vec4",
                Color::from_rgb(0.9, 0.5, 0.9)
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                "out",
                Output,
                "vec4",
                Color::from_rgb(0.9, 0.5, 0.9)
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(
            Left,
            "B",
            Input,
            "vec4",
            Color::from_rgb(0.9, 0.5, 0.9)
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Left),
    ]
    .spacing(1);

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
            "t",
            Input,
            "float",
            Color::from_rgb(0.9, 0.5, 0.2)
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "col",
            Output,
            "vec4",
            Color::from_rgb(0.9, 0.5, 0.9)
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
