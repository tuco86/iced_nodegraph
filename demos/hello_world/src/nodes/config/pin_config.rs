//! Pin Configuration Node
//!
//! Builds a PinConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, PinConfig, PinShape, node_title_bar, pin};

use crate::nodes::colors;

/// Collected inputs for PinConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<PinConfig>,
    /// Individual field overrides
    pub color: Option<Color>,
    pub radius: Option<f32>,
    pub shape: Option<PinShape>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
}

impl PinConfigInputs {
    /// Builds the final PinConfig by merging with parent
    pub fn build(&self) -> PinConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        PinConfig {
            color: self.color.or(parent.color),
            radius: self.radius.or(parent.radius),
            shape: self.shape.or(parent.shape),
            border_color: self.border_color.or(parent.border_color),
            border_width: self.border_width.or(parent.border_width),
        }
    }
}

/// Creates a PinConfig configuration node with all field inputs
pub fn pin_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &PinConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);
    let result = inputs.build();

    // Config row: input left, output right
    let config_row = row![
        pin!(Left, text("config").size(10), Input, "pin_config", colors::PIN_CONFIG),
        container(text("")).width(Length::Fill),
        pin!(Right, text("config").size(10), Output, "pin_config", colors::PIN_CONFIG),
    ].align_y(iced::Alignment::Center);

    // Separator line
    let separator = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_: &_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.1))),
            ..Default::default()
        });

    // Color row
    let color_display: iced::Element<'a, Message> = if let Some(c) = result.color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let color_row = row![
        pin!(Left, text("color").size(10), Input, "color", colors::PIN_COLOR),
        container(color_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Radius row
    let radius_row = row![
        pin!(Left, text("radius").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.radius.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Shape row
    let shape_label = match result.shape {
        Some(PinShape::Circle) => "circle",
        Some(PinShape::Square) => "square",
        Some(PinShape::Diamond) => "diamond",
        Some(PinShape::Triangle) => "triangle",
        None => "--",
    };
    let shape_row = row![
        pin!(Left, text("shape").size(10), Input, "pin_shape", colors::PIN_ANY),
        container(text(shape_label).size(9)).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Border color row
    let border_color_display: iced::Element<'a, Message> = if let Some(c) = result.border_color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let border_color_row = row![
        pin!(Left, text("border").size(10), Input, "color", colors::PIN_COLOR),
        container(border_color_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Border width row
    let border_width_row = row![
        pin!(Left, text("width").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.border_width.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        separator,
        color_row,
        radius_row,
        shape_row,
        border_color_row,
        border_width_row,
    ].spacing(4);

    column![
        node_title_bar("Pin Config", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
