//! Integer Slider Input Node
//!
//! Outputs a configurable integer value via slider.

use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, slider, text},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Integer slider configuration
#[derive(Debug, Clone, PartialEq)]
pub struct IntSliderConfig {
    pub min: i32,
    pub max: i32,
    pub label: String,
}

impl Default for IntSliderConfig {
    fn default() -> Self {
        Self {
            min: 0,
            max: 100,
            label: "Value".to_string(),
        }
    }
}

impl IntSliderConfig {
    /// Creates a config for node index selection (0-99)
    pub fn node_index() -> Self {
        Self {
            min: 0,
            max: 99,
            label: "Node Index".to_string(),
        }
    }
}

/// Creates an integer slider node with interactive slider widget
pub fn int_slider_node<'a, Message>(
    theme: &'a iced::Theme,
    value: i32,
    config: &IntSliderConfig,
    on_change: impl Fn(i32) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let value_display = text(format!("{}", value)).size(12);

    // Slider uses f32 internally, we convert
    let slider_widget = slider(
        (config.min as f32)..=(config.max as f32),
        value as f32,
        move |v| on_change(v.round() as i32),
    )
    .step(1.0)
    .width(Length::Fill);

    let output_pin = container(pin!(
        Right,
        "value",
        Output,
        "int",
        colors::PIN_NUMBER
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar(&config.label, style),
        container(
            column![
                row![slider_widget, value_display,]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                output_pin,
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(200.0)
    .into()
}
