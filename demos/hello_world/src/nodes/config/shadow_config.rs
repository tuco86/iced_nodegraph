//! Shadow Configuration Node
//!
//! Builds a ShadowConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, ShadowConfig, pin};

use crate::nodes::{colors, node_title_bar, pins};

/// Collected inputs for ShadowConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShadowConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<ShadowConfig>,
    /// Individual field overrides
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub blur_radius: Option<f32>,
    pub color: Option<Color>,
    pub enabled: Option<bool>,
}

impl ShadowConfigInputs {
    /// Builds the final ShadowConfig by merging with parent
    pub fn build(&self) -> ShadowConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        // Merge offset: if either x or y is set, create a new tuple
        let offset = match (self.offset_x, self.offset_y, parent.offset) {
            (Some(x), Some(y), _) => Some((x, y)),
            (Some(x), None, Some((_, py))) => Some((x, py)),
            (None, Some(y), Some((px, _))) => Some((px, y)),
            (Some(x), None, None) => Some((x, 0.0)),
            (None, Some(y), None) => Some((0.0, y)),
            (None, None, parent_offset) => parent_offset,
        };

        ShadowConfig {
            offset,
            blur_radius: self.blur_radius.or(parent.blur_radius),
            color: self.color.or(parent.color),
            enabled: self.enabled.or(parent.enabled),
        }
    }
}

/// Creates a ShadowConfig configuration node with all field inputs
pub fn shadow_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &ShadowConfigInputs,
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
            pins::ShadowConfigData,
            colors::PIN_CONFIG
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::config::SHADOW_OUT,
            text("out").size(10),
            Output,
            pins::ShadowConfigData,
            colors::PIN_CONFIG
        ),
    ]
    .align_y(iced::Alignment::Center);

    // Separator line
    let separator = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_: &_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                1.0, 1.0, 1.0, 0.1,
            ))),
            ..Default::default()
        });

    // Offset X row
    let offset_x_row = row![
        pin!(
            Left,
            pins::config::SHADOW_OFFSET_X,
            text("off x").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .offset
                    .map_or("--".to_string(), |(x, _)| format!("{:.0}", x))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Offset Y row
    let offset_y_row = row![
        pin!(
            Left,
            pins::config::SHADOW_OFFSET_Y,
            text("off y").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .offset
                    .map_or("--".to_string(), |(_, y)| format!("{:.0}", y))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Blur radius row
    let blur_row = row![
        pin!(
            Left,
            pins::config::SHADOW_BLUR,
            text("blur").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                result
                    .blur_radius
                    .map_or("--".to_string(), |v| format!("{:.0}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

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
        pin!(
            Left,
            pins::config::SHADOW_COLOR,
            text("color").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(color_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Enabled row
    let enabled_label = match result.enabled {
        Some(true) => "yes",
        Some(false) => "no",
        None => "--",
    };
    let enabled_row = row![
        pin!(
            Left,
            pins::config::ON,
            text("on").size(10),
            Input,
            pins::Bool,
            colors::PIN_BOOL
        ),
        container(text(enabled_label).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        separator,
        offset_x_row,
        offset_y_row,
        blur_row,
        color_row,
        enabled_row,
    ]
    .spacing(4);

    column![
        node_title_bar("Shadow Config", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
