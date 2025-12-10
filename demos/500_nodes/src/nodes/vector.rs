use iced::{
    alignment::Horizontal,
    widget::{column, container, row, text},
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
        row![
            container(pin!(Left, "vec", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "x", Output, "float", Color::from_rgb(0.9, 0.3, 0.3)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Right, "y", Output, "float", Color::from_rgb(0.3, 0.9, 0.3)))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
        container(pin!(Right, "z", Output, "float", Color::from_rgb(0.3, 0.3, 0.9)))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .spacing(1);

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
            container(pin!(Left, "x", Input, "float", Color::from_rgb(0.9, 0.3, 0.3)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "vec", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "y", Input, "float", Color::from_rgb(0.3, 0.9, 0.3)))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
        container(pin!(Left, "z", Input, "float", Color::from_rgb(0.3, 0.3, 0.9)))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(1);

    column![title_bar("Combine", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn normalize_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = row![
        container(pin!(Left, "in", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
        container(pin!(Right, "out", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![title_bar("Normalize", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}

pub fn dot_product_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![
        row![
            container(pin!(Left, "A", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "out", Output, "float", Color::from_rgb(0.9, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "B", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(1);

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
            container(pin!(Left, "A", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "out", Output, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        container(pin!(Left, "B", Input, "vec3", Color::from_rgb(0.5, 0.9, 0.9)))
            .width(Length::Fill)
            .align_x(Horizontal::Left),
    ]
    .spacing(1);

    column![title_bar("Cross", theme), container(pins).padding([4, 0])]
        .width(130.0)
        .into()
}
