//! Anchor Configuration Node
//!
//! Builds an [`AnchorOverlay`] from individual field inputs with inheritance
//! support. Color inputs are `ColorQuad`s, matching `AnchorStyle`.

use demo_common::NodeContentStyle;
use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, pin};

use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use crate::style_overlay::AnchorOverlay;

/// Collected inputs for the Anchor Config node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnchorConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<AnchorOverlay>,
    /// Individual field overrides
    pub core_size: Option<f32>,
    pub core_radius: Option<f32>,
    pub core_color: Option<ColorQuad>,
    pub core_border_color: Option<ColorQuad>,
    pub core_border_width: Option<f32>,
    pub orbit_offset: Option<f32>,
    pub orbit_spacing: Option<f32>,
    pub ring_color: Option<ColorQuad>,
    pub ring_width: Option<f32>,
    pub offered_ring_color: Option<ColorQuad>,
}

impl AnchorConfigInputs {
    /// Builds the overlay by setting this node's fields, then merging over the parent.
    pub fn build(&self) -> AnchorOverlay {
        let mut a = AnchorOverlay::new();
        if let Some(v) = self.core_size {
            a = a.core_size(v);
        }
        if let Some(v) = self.core_radius {
            a = a.core_radius(v);
        }
        if let Some(v) = self.core_color {
            a = a.core_color(v);
        }
        if let Some(v) = self.core_border_color {
            a = a.core_border_color(v);
        }
        if let Some(v) = self.core_border_width {
            a = a.core_border_width(v);
        }
        if let Some(v) = self.orbit_offset {
            a = a.orbit_offset(v);
        }
        if let Some(v) = self.orbit_spacing {
            a = a.orbit_spacing(v);
        }
        if let Some(v) = self.ring_color {
            a = a.ring_color(v);
        }
        if let Some(v) = self.ring_width {
            a = a.ring_width(v);
        }
        if let Some(v) = self.offered_ring_color {
            a = a.offered_ring_color(v);
        }
        match &self.config_in {
            Some(parent) => a.merge(parent),
            None => a,
        }
    }
}

/// Creates an Anchor Config node with all field inputs.
pub fn anchor_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &AnchorConfigInputs,
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
            ::std::any::TypeId::of::<pins::AnchorConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::ANCHOR_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::AnchorConfigData>()
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
                pins::anchor::CORE_SIZE,
                text("core size").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.core_size, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::CORE_RADIUS,
                text("core radius").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.core_radius, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::CORE_COLOR,
                text("core color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.core_color.map(|q| q.near_start)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::CORE_BORDER_COLOR,
                text("border color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.core_border_color.map(|q| q.near_start)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::CORE_BORDER_WIDTH,
                text("border width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.core_border_width, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::ORBIT_OFFSET,
                text("orbit offset").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.orbit_offset, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::ORBIT_SPACING,
                text("orbit spacing").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.orbit_spacing, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::RING_COLOR,
                text("ring color").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.ring_color.map(|q| q.near_start)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::RING_WIDTH,
                text("ring width").size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(result.ring_width, 1)),
        ),
        pin_row(
            pin!(
                Left,
                pins::anchor::OFFERED_RING_COLOR,
                text("offered ring").size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(result.offered_ring_color.map(|q| q.near_start)),
        ),
    ]
    .spacing(4);

    column![
        node_title_bar("Anchor Config", style),
        container(content).padding([8, 10])
    ]
    .width(170.0)
    .into()
}
