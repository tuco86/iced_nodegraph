//! Float Slider Input Node
//!
//! Outputs a configurable float value via slider.

use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, slider, text},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Float slider node message for internal value changes
#[derive(Debug, Clone, PartialEq)]
pub struct FloatSliderConfig {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub label: String,
}

impl Default for FloatSliderConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 10.0,
            step: 0.1,
            label: "Value".to_string(),
        }
    }
}

impl FloatSliderConfig {
    pub fn corner_radius() -> Self {
        Self {
            min: 0.0,
            max: 20.0,
            step: 0.5,
            label: "Corner Radius".to_string(),
        }
    }

    pub fn opacity() -> Self {
        Self {
            min: 0.1,
            max: 1.0,
            step: 0.05,
            label: "Opacity".to_string(),
        }
    }

    pub fn border_width() -> Self {
        Self {
            min: 0.5,
            max: 5.0,
            step: 0.5,
            label: "Border Width".to_string(),
        }
    }

    pub fn thickness() -> Self {
        Self {
            min: 0.5,
            max: 8.0,
            step: 0.1,
            label: "Thickness".to_string(),
        }
    }
}

/// Creates a float slider node with interactive slider widget
pub fn float_slider_node<'a, Message>(
    theme: &'a iced::Theme,
    value: f32,
    config: &FloatSliderConfig,
    on_change: impl Fn(f32) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let value_display = text(format!("{:.1}", value)).size(12);

    let slider_widget = slider(config.min..=config.max, value, on_change)
        .step(config.step)
        .width(Length::Fill);

    let output_pin = container(pin!(
        Right,
        "value",
        Output,
        "float",
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
