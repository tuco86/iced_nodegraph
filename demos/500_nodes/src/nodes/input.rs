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

pub fn time_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "t", Output, "float", Color::from_rgb(0.9, 0.5, 0.2)),].spacing(1);

    column![title_bar("Time", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn uv_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "uv", Output, "vec2", Color::from_rgb(0.9, 0.7, 0.3)),].spacing(1);

    column![title_bar("UV", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn normal_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "N", Output, "vec3", Color::from_rgb(0.5, 0.7, 0.9)),].spacing(1);

    column![title_bar("Normal", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}

pub fn position_input_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let pins = column![pin!(Right, "P", Output, "vec3", Color::from_rgb(0.3, 0.9, 0.5)),].spacing(1);

    column![title_bar("Position", theme), container(pins).padding([4, 0])]
        .width(100.0)
        .into()
}
