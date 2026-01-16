//! Node Configuration Node
//!
//! Builds a NodeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeConfig, NodeContentStyle, ShadowConfig, pin};

use crate::nodes::{colors, node_title_bar, pins, section_header};

/// Section expansion state for NodeConfig nodes
#[derive(Debug, Clone, Default)]
pub struct NodeSections {
    pub fill: bool,
    pub border: bool,
    pub shadow: bool,
}

impl NodeSections {
    pub fn new_all_expanded() -> Self {
        Self {
            fill: true,
            border: true,
            shadow: true,
        }
    }
}

/// Identifies which section to toggle in NodeConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSection {
    Fill,
    Border,
    Shadow,
}

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

/// Creates a NodeConfig configuration node with all field inputs and collapsible sections
pub fn node_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &NodeConfigInputs,
    sections: &NodeSections,
    on_toggle: impl Fn(NodeSection) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);
    let result = inputs.build();

    // Config row: input left, typed output right
    let config_row = row![
        pin!(
            Left,
            pins::config::CONFIG,
            text("in").size(10),
            Input,
            pins::NodeConfigData,
            colors::PIN_CONFIG
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::config::NODE_OUT,
            text("out").size(10),
            Output,
            pins::NodeConfigData,
            colors::PIN_CONFIG
        ),
    ]
    .align_y(iced::Alignment::Center);

    // Helper to create separator lines
    let make_separator = || {
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_: &_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.1,
                ))),
                ..Default::default()
            })
    };

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
        pin!(
            Left,
            pins::config::BG_COLOR,
            text("fill").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(fill_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

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
        pin!(
            Left,
            pins::config::COLOR,
            text("border").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(border_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border width row
    let width_row = row![
        pin!(
            Left,
            pins::config::WIDTH,
            text("width").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .border_width
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Corner radius row
    let radius_row = row![
        pin!(
            Left,
            pins::config::RADIUS,
            text("radius").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .corner_radius
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Opacity row
    let opacity_row = row![
        pin!(
            Left,
            pins::config::OPACITY,
            text("opacity").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .opacity
                    .map_or("--".to_string(), |v| format!("{:.0}%", v * 100.0))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Shadow row
    let shadow_row = row![
        pin!(
            Left,
            pins::config::SHADOW,
            text("shadow").size(10),
            Input,
            pins::ShadowConfigData,
            colors::PIN_CONFIG
        ),
        container(text(if result.shadow.is_some() { "set" } else { "--" }).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Build content with collapsible sections
    let mut content = column![config_row, make_separator()].spacing(4);

    // Fill section
    content = content.push(section_header("Fill", sections.fill, on_toggle(NodeSection::Fill)));
    if sections.fill {
        content = content.push(fill_row);
        content = content.push(radius_row);
        content = content.push(opacity_row);
    } else {
        // Collapsed: show disabled pins stacked
        content = content.push(
            row![
                pin!(Left, pins::config::BG_COLOR, text("").size(1), Input, pins::ColorData, colors::PIN_COLOR).disable_interactions(),
                pin!(Left, pins::config::RADIUS, text("").size(1), Input, pins::Float, colors::PIN_NUMBER).disable_interactions(),
                pin!(Left, pins::config::OPACITY, text("").size(1), Input, pins::Float, colors::PIN_NUMBER).disable_interactions(),
            ]
            .spacing(2),
        );
    }
    content = content.push(make_separator());

    // Border section
    content = content.push(section_header("Border", sections.border, on_toggle(NodeSection::Border)));
    if sections.border {
        content = content.push(border_row);
        content = content.push(width_row);
    } else {
        // Collapsed: show disabled pins stacked
        content = content.push(
            row![
                pin!(Left, pins::config::COLOR, text("").size(1), Input, pins::ColorData, colors::PIN_COLOR).disable_interactions(),
                pin!(Left, pins::config::WIDTH, text("").size(1), Input, pins::Float, colors::PIN_NUMBER).disable_interactions(),
            ]
            .spacing(2),
        );
    }
    content = content.push(make_separator());

    // Shadow section
    content = content.push(section_header("Shadow", sections.shadow, on_toggle(NodeSection::Shadow)));
    if sections.shadow {
        content = content.push(shadow_row);
    } else {
        // Collapsed: show disabled pin
        content = content.push(
            pin!(Left, pins::config::SHADOW, text("").size(1), Input, pins::ShadowConfigData, colors::PIN_CONFIG).disable_interactions(),
        );
    }

    column![
        node_title_bar("Node Config", style),
        container(content).padding([8, 10])
    ]
    .width(160.0)
    .into()
}
