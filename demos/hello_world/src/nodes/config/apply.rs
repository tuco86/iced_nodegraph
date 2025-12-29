//! Apply Nodes
//!
//! Nodes that apply configurations to the graph or specific nodes.

use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, pin};

use crate::nodes::{colors, node_title_bar, pins};

/// Creates an ApplyToGraph node that receives configs and applies them globally
pub fn apply_to_graph_node<'a, Message>(
    theme: &'a iced::Theme,
    has_node_config: bool,
    has_edge_config: bool,
    has_pin_config: bool,
    has_background_config: bool,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    // Node config row
    let node_status = if has_node_config { "ok" } else { "--" };
    let node_config_row = row![
        pin!(
            Left,
            pins::config::NODE_CONFIG,
            text("nodes").size(10),
            Input,
            pins::NodeConfigData,
            colors::PIN_CONFIG
        ),
        container(text(node_status).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Edge config row
    let edge_status = if has_edge_config { "ok" } else { "--" };
    let edge_config_row = row![
        pin!(
            Left,
            pins::config::EDGE_CONFIG,
            text("edges").size(10),
            Input,
            pins::EdgeConfigData,
            colors::PIN_CONFIG
        ),
        container(text(edge_status).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Pin config row
    let pin_status = if has_pin_config { "ok" } else { "--" };
    let pin_config_row = row![
        pin!(
            Left,
            pins::config::PIN_CONFIG,
            text("pins").size(10),
            Input,
            pins::PinConfigData,
            colors::PIN_CONFIG
        ),
        container(text(pin_status).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Background config row
    let bg_status = if has_background_config { "ok" } else { "--" };
    let bg_config_row = row![
        pin!(
            Left,
            pins::config::BACKGROUND_CONFIG,
            text("background").size(10),
            Input,
            pins::BackgroundConfigData,
            colors::PIN_CONFIG
        ),
        container(text(bg_status).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![
        node_config_row,
        edge_config_row,
        pin_config_row,
        bg_config_row,
    ]
    .spacing(4);

    column![
        node_title_bar("Apply to Graph", style),
        container(content).padding([8, 10])
    ]
    .width(180.0)
    .into()
}

/// Creates an ApplyToNode node that applies config to a specific node by ID
pub fn apply_to_node_node<'a, Message>(
    theme: &'a iced::Theme,
    has_node_config: bool,
    target_id: Option<i32>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    // Config input row
    let config_status = if has_node_config { "ok" } else { "--" };
    let config_row = row![
        pin!(
            Left,
            pins::config::NODE_CONFIG,
            text("config").size(10),
            Input,
            pins::NodeConfigData,
            colors::PIN_CONFIG
        ),
        container(text(config_status).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Target ID row
    let id_display = target_id.map_or("--".to_string(), |id| format!("#{}", id));
    let target_row = row![
        pin!(
            Left,
            pins::config::TARGET,
            text("target").size(10),
            Input,
            pins::Int,
            colors::PIN_NUMBER
        ),
        container(text(id_display).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![config_row, target_row,].spacing(4);

    column![
        node_title_bar("Apply to Node", style),
        container(content).padding([8, 10])
    ]
    .width(170.0)
    .into()
}
