//! Cutting Tool Configuration Node
//!
//! Builds a [`CuttingToolOverlay`] from individual field inputs with
//! inheritance support. The color input is a `ColorQuad` whose near corner is
//! taken, since `CuttingToolStyle` holds a plain `Color`.

use demo_common::NodeContentStyle;
use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, pin};

use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use crate::style_overlay::CuttingToolOverlay;

/// Collected inputs for the Cutting Tool Config node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CuttingToolConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<CuttingToolOverlay>,
    /// Individual field overrides
    pub color: Option<ColorQuad>,
    pub width: Option<f32>,
}

impl CuttingToolConfigInputs {
    /// Builds the overlay by setting this node's fields, then merging over the parent.
    pub fn build(&self) -> CuttingToolOverlay {
        let mut c = CuttingToolOverlay::new();
        if let Some(v) = self.color {
            c = c.color(v);
        }
        if let Some(v) = self.width {
            c = c.width(v);
        }
        match &self.config_in {
            Some(parent) => c.merge(parent),
            None => c,
        }
    }
}

/// Creates a Cutting Tool Config node with all field inputs.
pub fn cutting_tool_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &CuttingToolConfigInputs,
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
            ::std::any::TypeId::of::<pins::CuttingToolConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::CUTTING_TOOL_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::CuttingToolConfigData>()
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
                pins::cutting_tool::COLOR,
                text("color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.color),
        ),
        pin_row(
            pin!(
                Left,
                pins::cutting_tool::WIDTH,
                text("width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.width, 1)),
        ),
    ]
    .spacing(4);

    column![
        node_title_bar("Cutting Tool Config", style),
        container(content).padding([8, 10])
    ]
    .width(170.0)
    .into()
}
