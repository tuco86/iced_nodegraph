//! Minimap Configuration Node
//!
//! Builds a [`MinimapOverlay`] from individual field inputs with inheritance
//! support. Color inputs are `ColorQuad`s whose near corner is taken, since
//! `MinimapStyle` holds plain `Color`s.

use demo_common::NodeContentStyle;
use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, pin};

use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use crate::style_overlay::MinimapOverlay;

/// Collected inputs for the Minimap Config node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MinimapConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<MinimapOverlay>,
    /// Individual field overrides
    pub background: Option<ColorQuad>,
    pub border_color: Option<ColorQuad>,
    pub border_width: Option<f32>,
    pub node_color: Option<ColorQuad>,
    pub selected_node_color: Option<ColorQuad>,
    pub viewport_fill: Option<ColorQuad>,
    pub viewport_border_color: Option<ColorQuad>,
    pub viewport_border_width: Option<f32>,
}

impl MinimapConfigInputs {
    /// Builds the overlay by setting this node's fields, then merging over the parent.
    pub fn build(&self) -> MinimapOverlay {
        let mut m = MinimapOverlay::new();
        if let Some(v) = self.background {
            m = m.background(v);
        }
        if let Some(v) = self.border_color {
            m = m.border_color(v);
        }
        if let Some(v) = self.border_width {
            m = m.border_width(v);
        }
        if let Some(v) = self.node_color {
            m = m.node_color(v);
        }
        if let Some(v) = self.selected_node_color {
            m = m.selected_node_color(v);
        }
        if let Some(v) = self.viewport_fill {
            m = m.viewport_fill(v);
        }
        if let Some(v) = self.viewport_border_color {
            m = m.viewport_border_color(v);
        }
        if let Some(v) = self.viewport_border_width {
            m = m.viewport_border_width(v);
        }
        match &self.config_in {
            Some(parent) => m.merge(parent),
            None => m,
        }
    }
}

/// Creates a Minimap Config node with all field inputs.
pub fn minimap_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &MinimapConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);
    let result = inputs.build();

    let config_row = row![
        pin!(
            Left,
            pins::cfg::CONFIG,
            text("in").size(10),
            Input,
            ::std::any::TypeId::of::<pins::MinimapConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::MINIMAP_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::MinimapConfigData>()
        ),
    ]
    .align_y(iced::Alignment::Center);

    let separator = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_: &_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                1.0, 1.0, 1.0, 0.1,
            ))),
            ..Default::default()
        });

    let content = column![
        config_row,
        separator,
        pin_row(
            pin!(
                Left,
                pins::minimap::BACKGROUND,
                text("background").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.background),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::BORDER_COLOR,
                text("border color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.border_color),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::BORDER_WIDTH,
                text("border width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.border_width, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::NODE_COLOR,
                text("node color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.node_color),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::SELECTED_NODE_COLOR,
                text("selected node").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.selected_node_color),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::VIEWPORT_FILL,
                text("viewport fill").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.viewport_fill),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::VIEWPORT_BORDER_COLOR,
                text("viewport border").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.viewport_border_color),
        ),
        pin_row(
            pin!(
                Left,
                pins::minimap::VIEWPORT_BORDER_WIDTH,
                text("viewport width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.viewport_border_width, 1)),
        ),
    ]
    .spacing(4);

    column![
        node_title_bar("Minimap Config", style),
        container(content).padding([8, 10])
    ]
    .width(180.0)
    .into()
}
