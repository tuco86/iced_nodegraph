//! Style Configuration Nodes
//!
//! These nodes receive values from input nodes and apply them to graph styling.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Node that configures corner radius for all nodes
pub fn corner_radius_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<f32>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let value_display = match current_value {
        Some(v) => text(format!("{:.1}px", v)).size(11),
        None => text("--").size(11),
    };

    let input_pin = container(pin!(
        Left,
        "radius",
        Input,
        "float",
        colors::PIN_NUMBER
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Corner Radius").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Node Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(value_display)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}

/// Node that configures opacity for all nodes
pub fn opacity_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<f32>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let value_display = match current_value {
        Some(v) => text(format!("{:.0}%", v * 100.0)).size(11),
        None => text("--").size(11),
    };

    let input_pin = container(pin!(
        Left,
        "opacity",
        Input,
        "float",
        colors::PIN_NUMBER
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Opacity").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Node Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(value_display)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}

/// Node that configures border width for all nodes
pub fn border_width_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<f32>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let value_display = match current_value {
        Some(v) => text(format!("{:.1}px", v)).size(11),
        None => text("--").size(11),
    };

    let input_pin = container(pin!(
        Left,
        "width",
        Input,
        "float",
        colors::PIN_NUMBER
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Border Width").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Node Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(value_display)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}

/// Node that configures fill color for all nodes
pub fn fill_color_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<Color>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let color_preview = match current_value {
        Some(c) => container(text(""))
            .width(44)
            .height(20)
            .style(move |_theme| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        None => container(text("--").size(10)).width(44).height(20),
    };

    let input_pin = container(pin!(
        Left,
        "color",
        Input,
        "color",
        colors::PIN_COLOR
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Fill Color").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Node Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(color_preview)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}

/// Node that configures edge thickness
pub fn edge_thickness_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<f32>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let value_display = match current_value {
        Some(v) => text(format!("{:.1}px", v)).size(11),
        None => text("--").size(11),
    };

    let input_pin = container(pin!(
        Left,
        "thickness",
        Input,
        "float",
        colors::PIN_NUMBER
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Thickness").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Edge Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(value_display)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}

/// Node that configures edge color
pub fn edge_color_config_node<'a, Message>(
    theme: &'a iced::Theme,
    current_value: Option<Color>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let color_preview = match current_value {
        Some(c) => container(text(""))
            .width(44)
            .height(20)
            .style(move |_theme| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        None => container(text("--").size(10)).width(44).height(20),
    };

    let input_pin = container(pin!(
        Left,
        "color",
        Input,
        "color",
        colors::PIN_COLOR
    ))
    .width(Length::FillPortion(1))
    .align_x(Horizontal::Left);

    let label = container(text("Edge Color").size(11))
        .width(Length::FillPortion(2))
        .align_x(Horizontal::Right);

    column![
        node_title_bar("Edge Style", style),
        container(
            column![
                row![input_pin, label].width(Length::Fill),
                container(color_preview)
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}
