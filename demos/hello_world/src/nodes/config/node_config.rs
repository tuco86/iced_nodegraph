//! Node Configuration Node
//!
//! Builds a NodeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeConfig, NodeContentStyle, ShadowConfig, node_title_bar, pin};

use crate::nodes::colors;

/// Collected inputs for NodeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<NodeConfig>,
    /// Individual field overrides
    pub fill_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub corner_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub shadow: Option<ShadowConfig>,
}

impl NodeConfigInputs {
    /// Builds the final NodeConfig by merging with parent
    pub fn build(&self) -> NodeConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        NodeConfig {
            fill_color: self.fill_color.or(parent.fill_color),
            border_color: self.border_color.or(parent.border_color),
            border_width: self.border_width.or(parent.border_width),
            corner_radius: self.corner_radius.or(parent.corner_radius),
            opacity: self.opacity.or(parent.opacity),
            shadow: self.shadow.clone().or(parent.shadow),
        }
    }
}

/// Creates a NodeConfig configuration node with all field inputs
pub fn node_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &NodeConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);
    let result = inputs.build();

    // Config row: input left, output right
    let config_row = row![
        pin!(Left, text("config").size(10), Input, "node_config", colors::PIN_CONFIG),
        container(text("")).width(Length::Fill),
        pin!(Right, text("config").size(10), Output, "node_config", colors::PIN_CONFIG),
    ].align_y(iced::Alignment::Center);

    // Separator line
    let separator = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_: &_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.1))),
            ..Default::default()
        });

    // Fill color row
    let fill_display: iced::Element<'a, Message> = if let Some(c) = result.fill_color {
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
    let fill_row = row![
        pin!(Left, text("fill").size(10), Input, "color", colors::PIN_COLOR),
        container(fill_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Border color row
    let border_display: iced::Element<'a, Message> = if let Some(c) = result.border_color {
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
    let border_row = row![
        pin!(Left, text("border").size(10), Input, "color", colors::PIN_COLOR),
        container(border_display).width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Border width row
    let width_row = row![
        pin!(Left, text("width").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.border_width.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Corner radius row
    let radius_row = row![
        pin!(Left, text("radius").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.corner_radius.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Opacity row
    let opacity_row = row![
        pin!(Left, text("opacity").size(10), Input, "float", colors::PIN_NUMBER),
        container(text(result.opacity.map_or("--".to_string(), |v| format!("{:.0}%", v * 100.0))).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    // Shadow row
    let shadow_row = row![
        pin!(Left, text("shadow").size(10), Input, "shadow_config", colors::PIN_CONFIG),
        container(text(if result.shadow.is_some() { "set" } else { "--" }).size(9))
            .width(Length::Fill).align_x(Horizontal::Right),
    ].align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        separator,
        fill_row,
        border_row,
        width_row,
        radius_row,
        opacity_row,
        shadow_row,
    ].spacing(4);

    column![
        node_title_bar("Node Config", style),
        container(content).padding([8, 10])
    ]
    .width(160.0)
    .into()
}
