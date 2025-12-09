use iced::{
    widget::{column, container, text},
    Color, Length, Theme,
};
use iced_nodegraph::pin;

fn title_bar<'a, Message>(title: &'a str, theme: &'a Theme) -> iced::widget::Container<'a, Message, Theme, iced::Renderer>
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
        pin!(Left, "A", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Left, "B", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Add", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn multiply_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "A", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Left, "B", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Multiply", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn divide_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "A", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Left, "B", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Divide", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn subtract_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "A", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Left, "B", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Subtract", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn power_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "val", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Left, "exp", Input, "float", Color::from_rgb(0.8, 0.8, 0.8)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Power", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}
