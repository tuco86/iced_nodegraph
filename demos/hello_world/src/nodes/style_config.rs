//! Style Configuration Nodes
//!
//! These nodes receive values from input nodes and apply them to graph styling.

use iced::{
    widget::{column, container, row, text},
    alignment::Horizontal,
    Color, Length,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

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

    let input_pin = container(pin!(Left, "radius", Input, "float", Color::from_rgb(0.5, 0.8, 0.5)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
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

    let input_pin = container(pin!(Left, "opacity", Input, "float", Color::from_rgb(0.5, 0.8, 0.5)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
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

    let input_pin = container(pin!(Left, "width", Input, "float", Color::from_rgb(0.5, 0.8, 0.5)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
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
            .width(40)
            .height(16)
            .style(move |_theme| {
                container::Style {
                    background: Some(iced::Background::Color(c)),
                    border: iced::Border {
                        color: Color::from_rgb(0.4, 0.4, 0.4),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..Default::default()
                }
            }),
        None => container(text("--").size(10))
            .width(40)
            .height(16),
    };

    let input_pin = container(pin!(Left, "color", Input, "color", Color::from_rgb(0.8, 0.5, 0.8)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
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

    let input_pin = container(pin!(Left, "thickness", Input, "float", Color::from_rgb(0.5, 0.8, 0.5)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
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
            .width(40)
            .height(16)
            .style(move |_theme| {
                container::Style {
                    background: Some(iced::Background::Color(c)),
                    border: iced::Border {
                        color: Color::from_rgb(0.4, 0.4, 0.4),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..Default::default()
                }
            }),
        None => container(text("--").size(10))
            .width(40)
            .height(16),
    };

    let input_pin = container(pin!(Left, "color", Input, "color", Color::from_rgb(0.8, 0.5, 0.8)))
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
            .spacing(4)
        )
        .padding([6, 8])
    ]
    .width(160.0)
    .into()
}
