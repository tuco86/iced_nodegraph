//! Boolean Toggle Input Node
//!
//! Outputs a boolean value via checkbox toggle.

use iced::{
    Length,
    alignment::Horizontal,
    widget::{checkbox, column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Boolean toggle configuration
#[derive(Debug, Clone, PartialEq)]
pub struct BoolToggleConfig {
    pub label: String,
    pub toggle_label: String,
}

impl Default for BoolToggleConfig {
    fn default() -> Self {
        Self {
            label: "Toggle".to_string(),
            toggle_label: "Enabled".to_string(),
        }
    }
}

impl BoolToggleConfig {
    /// Creates a config for shadow enabled toggle
    pub fn shadow_enabled() -> Self {
        Self {
            label: "Shadow".to_string(),
            toggle_label: "Enabled".to_string(),
        }
    }
}

/// Creates a boolean toggle node with checkbox widget
pub fn bool_toggle_node<'a, Message>(
    theme: &'a iced::Theme,
    value: bool,
    config: &'a BoolToggleConfig,
    on_change: impl Fn(bool) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let checkbox_widget = checkbox(value)
        .label(&config.toggle_label)
        .on_toggle(on_change)
        .size(16)
        .text_size(11);

    let output_pin = container(pin!(
        Right,
        "value",
        Output,
        "bool",
        colors::PIN_BOOL
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar(&config.label, style),
        container(
            column![
                row![
                    checkbox_widget,
                    container(text(if value { "true" } else { "false" }).size(10))
                        .width(Length::Fill)
                        .align_x(Horizontal::Right)
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                output_pin,
            ]
            .spacing(6)
        )
        .padding([10, 12])
    ]
    .width(180.0)
    .into()
}
