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

pub fn vector_split_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "vec", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Right, "x", Output, "float", Color::from_rgb(0.9, 0.3, 0.3)),
        pin!(Right, "y", Output, "float", Color::from_rgb(0.3, 0.9, 0.3)),
        pin!(Right, "z", Output, "float", Color::from_rgb(0.3, 0.3, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Split", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn vector_combine_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "x", Input, "float", Color::from_rgb(0.9, 0.3, 0.3)),
        pin!(Left, "y", Input, "float", Color::from_rgb(0.3, 0.9, 0.3)),
        pin!(Left, "z", Input, "float", Color::from_rgb(0.3, 0.3, 0.9)),
        pin!(Right, "vec", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Combine", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn normalize_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "in", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Right, "out", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Normalize", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn dot_product_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "A", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Left, "B", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Dot", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}

pub fn cross_product_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        pin!(Left, "A", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Left, "B", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
        pin!(Right, "out", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)),
    ]
    .spacing(1);

    column![title_bar("Cross", theme), container(pins).padding([4, 0])]
        .width(120.0)
        .into()
}
