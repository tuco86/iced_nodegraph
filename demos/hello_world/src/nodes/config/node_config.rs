//! Node Configuration Node
//!
//! Builds a NodeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, Pattern, pin};

use crate::nodes::{colors, node_title_bar, pins, section_header_with_pins};
use crate::style_overlay::NodeOverlay;

/// Section expansion state for NodeConfig nodes
#[derive(Debug, Clone, Default)]
pub struct NodeSections {
    pub fill: bool,
    pub border: bool,
}

impl NodeSections {
    pub fn new_all_expanded() -> Self {
        Self {
            fill: true,
            border: true,
        }
    }
}

/// Identifies which section to toggle in NodeConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSection {
    Fill,
    Border,
}

/// Collected inputs for NodeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<NodeOverlay>,
    /// Individual field overrides
    pub fill_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub corner_radius: Option<f32>,
    pub opacity: Option<f32>,
}

impl NodeConfigInputs {
    /// Builds the final overlay by merging this node's fields over the parent.
    pub fn build(&self) -> NodeOverlay {
        let mut p = NodeOverlay::new();
        if let Some(c) = self.fill_color {
            p = p.fill_color(c);
        }
        if let Some(c) = self.border_color {
            p = p.border_color(c);
        }
        if let Some(w) = self.border_width {
            p = p.border_pattern(Pattern::solid(w));
        }
        if let Some(r) = self.corner_radius {
            p = p.corner_radius(r);
        }
        if let Some(o) = self.opacity {
            p = p.opacity(o);
        }
        match &self.config_in {
            Some(parent) => p.merge(parent),
            None => p,
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
            ::std::any::TypeId::of::<pins::NodeConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::config::NODE_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::NodeConfigData>()
        ),
    ]
    .align_y(iced::Alignment::Center);

    // Fill color row
    let fill_display: iced::Element<'a, Message> =
        if let Some(c) = result.fill_color.map(|q| q.near_start) {
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
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
        container(fill_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border color row
    let border_color = result.border_color.map(|q| q.near_start);
    let border_display: iced::Element<'a, Message> = if let Some(c) = border_color {
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
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
        container(border_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border width row
    let border_width = result.border_pattern.map(|p| p.thickness);
    let width_row = row![
        pin!(
            Left,
            pins::config::WIDTH,
            text("width").size(10),
            Input,
            ::std::any::TypeId::of::<pins::Float>()
        ),
        container(text(border_width.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
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
            ::std::any::TypeId::of::<pins::Float>()
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
            ::std::any::TypeId::of::<pins::Float>()
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

    // Build content with collapsible sections
    let mut content = column![config_row].spacing(4);

    // Fill section - pins inline when collapsed
    let fill_collapsed_pins: Option<iced::Element<'_, Message>> = if !sections.fill {
        Some(
            row![
                pin!(
                    Left,
                    pins::config::BG_COLOR,
                    text("").size(1),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                )
                .disable_interactions(),
                pin!(
                    Left,
                    pins::config::RADIUS,
                    text("").size(1),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                )
                .disable_interactions(),
                pin!(
                    Left,
                    pins::config::OPACITY,
                    text("").size(1),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                )
                .disable_interactions(),
            ]
            .spacing(2)
            .into(),
        )
    } else {
        None
    };
    content = content.push(section_header_with_pins(
        "Fill",
        sections.fill,
        on_toggle(NodeSection::Fill),
        fill_collapsed_pins,
    ));
    if sections.fill {
        content = content.push(fill_row);
        content = content.push(radius_row);
        content = content.push(opacity_row);
    }

    // Border section - pins inline when collapsed
    let border_collapsed_pins: Option<iced::Element<'_, Message>> = if !sections.border {
        Some(
            row![
                pin!(
                    Left,
                    pins::config::COLOR,
                    text("").size(1),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                )
                .disable_interactions(),
                pin!(
                    Left,
                    pins::config::WIDTH,
                    text("").size(1),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                )
                .disable_interactions(),
            ]
            .spacing(2)
            .into(),
        )
    } else {
        None
    };
    content = content.push(section_header_with_pins(
        "Border",
        sections.border,
        on_toggle(NodeSection::Border),
        border_collapsed_pins,
    ));
    if sections.border {
        content = content.push(border_row);
        content = content.push(width_row);
    }

    column![
        node_title_bar("Node Config", style),
        container(content).padding([8, 10])
    ]
    .width(160.0)
    .into()
}
