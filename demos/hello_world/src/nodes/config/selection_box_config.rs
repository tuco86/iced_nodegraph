//! Selection Box Configuration Node
//!
//! Builds a [`SelectionBoxOverlay`] from individual field inputs with
//! inheritance support. Color inputs are `ColorQuad`s whose near corner is
//! taken, since `SelectionBoxStyle` holds plain `Color`s.

use demo_common::NodeContentStyle;
use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, pin};

use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use crate::style_overlay::SelectionBoxOverlay;

/// Collected inputs for the Selection Box Config node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionBoxConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<SelectionBoxOverlay>,
    /// Individual field overrides
    pub fill: Option<ColorQuad>,
    pub border_color: Option<ColorQuad>,
    pub border_width: Option<f32>,
}

impl SelectionBoxConfigInputs {
    /// Builds the overlay by setting this node's fields, then merging over the parent.
    pub fn build(&self) -> SelectionBoxOverlay {
        let mut s = SelectionBoxOverlay::new();
        if let Some(v) = self.fill {
            s = s.fill(v);
        }
        if let Some(v) = self.border_color {
            s = s.border_color(v);
        }
        if let Some(v) = self.border_width {
            s = s.border_width(v);
        }
        match &self.config_in {
            Some(parent) => s.merge(parent),
            None => s,
        }
    }
}

/// Creates a Selection Box Config node with all field inputs.
pub fn selection_box_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &SelectionBoxConfigInputs,
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
            ::std::any::TypeId::of::<pins::SelectionBoxConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::SELECTION_BOX_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::SelectionBoxConfigData>()
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
                pins::selection_box::FILL,
                text("fill").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.fill),
        ),
        pin_row(
            pin!(
                Left,
                pins::selection_box::BORDER_COLOR,
                text("border color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.border_color),
        ),
        pin_row(
            pin!(
                Left,
                pins::selection_box::BORDER_WIDTH,
                text("border width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.border_width, 1)),
        ),
    ]
    .spacing(4);

    column![
        node_title_bar("Selection Box Config", style),
        container(content).padding([8, 10])
    ]
    .width(170.0)
    .into()
}
