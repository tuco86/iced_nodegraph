//! Edge Configuration Node
//!
//! Builds an EdgeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{EdgeConfig, EdgeType, NodeContentStyle, node_title_bar, pin};

use crate::nodes::colors;

/// Collected inputs for EdgeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<EdgeConfig>,
    /// Individual field overrides
    pub start_color: Option<Color>,
    pub end_color: Option<Color>,
    pub thickness: Option<f32>,
    pub edge_type: Option<EdgeType>,
}

impl EdgeConfigInputs {
    /// Builds the final EdgeConfig by merging with parent
    pub fn build(&self) -> EdgeConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        EdgeConfig {
            start_color: self.start_color.or(parent.start_color),
            end_color: self.end_color.or(parent.end_color),
            thickness: self.thickness.or(parent.thickness),
            edge_type: self.edge_type.or(parent.edge_type),
            dash_pattern: parent.dash_pattern,
            animation: parent.animation,
        }
    }
}

/// Creates an EdgeConfig configuration node with all field inputs
pub fn edge_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &EdgeConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let result = inputs.build();

    // Config row: input left, output right
    let config_row = row![
        pin!(Left, text("config").size(10), Input, "edge_config", colors::PIN_CONFIG),
        container(text("")).width(Length::Fill),
        pin!(Right, text("config").size(10), Output, "edge_config", colors::PIN_CONFIG),
    ].align_y(iced::Alignment::Center);

    // Separator line
    let separator = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_: &_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.1))),
            ..Default::default()
        });

    // Start color row
    let start_display: iced::Element<'a, Message> = if let Some(c) = result.start_color {
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
    let start_row = row![
        pin!(Left, text("start").size(10), Input, "color", colors::PIN_COLOR),
        container(start_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // End color row
    let end_display: iced::Element<'a, Message> = if let Some(c) = result.end_color {
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
    let end_row = row![
        pin!(Left, text("end").size(10), Input, "color", colors::PIN_COLOR),
        container(end_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Thickness row
    let thick_row = row![
        pin!(Left, text("thick").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.thickness.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Edge type row
    let edge_type_label = match result.edge_type {
        Some(EdgeType::Bezier) => "bezier",
        Some(EdgeType::Straight) => "straight",
        Some(EdgeType::Step) => "step",
        Some(EdgeType::SmoothStep) => "smooth",
        None => "--",
    };
    let type_row = row![
        pin!(Left, text("type").size(10), Input, "edge_type", colors::PIN_ANY),
        container(text(edge_type_label).size(9)).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        separator,
        start_row,
        end_row,
        thick_row,
        type_row,
    ].spacing(4);

    column![
        node_title_bar("Edge Config", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
