//! Float Slider Input Node
//!
//! Outputs a configurable float value via slider.
//! Industrial Precision design: clean track, responsive handle.

use iced::{Color, Length, widget::{column, container, row, slider, text}};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar};

/// Float slider node configuration
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
            max: 20.0,
            step: 0.1,
            label: "Float".to_string(),
        }
    }
}

/// Creates a float slider node with modern styling
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

    // Value display with monospace-style formatting
    let value_display = text(format!("{:.1}", value))
        .size(11)
        .color(colors::TEXT_MUTED);

    // Slider with Industrial Precision styling
    let slider_widget = slider(config.min..=config.max, value, on_change)
        .step(config.step)
        .width(Length::Fixed(100.0))
        .height(16.0)
        .style(|_, status| {
            let (handle_bg, handle_border) = match status {
                slider::Status::Active => (Color::WHITE, colors::PIN_NUMBER),
                slider::Status::Hovered => (colors::PIN_NUMBER, Color::WHITE),
                slider::Status::Dragged => (colors::PIN_NUMBER, Color::WHITE),
            };
            slider::Style {
                rail: slider::Rail {
                    backgrounds: (
                        colors::PIN_NUMBER.into(),
                        colors::SURFACE_ELEVATED.into(),
                    ),
                    width: 6.0,
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                },
                handle: slider::Handle {
                    shape: slider::HandleShape::Circle { radius: 7.0 },
                    background: handle_bg.into(),
                    border_width: 2.0,
                    border_color: handle_border,
                },
            }
        });

    // Output pin
    let output_pin = pin!(Right, text("value").size(10), Output, "float", colors::PIN_NUMBER);

    // Use border_width from style for padding
    let border_width = style.border_width;

    column![
        node_title_bar(&config.label, style),
        container(
            column![
                row![slider_widget, value_display]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                container(output_pin)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right),
            ].spacing(6)
        ).padding([10, 12 + border_width as u16])
    ]
    .width(180.0)
    .into()
}
