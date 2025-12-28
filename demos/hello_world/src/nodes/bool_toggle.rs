//! Boolean Toggle Input Node
//!
//! Outputs a boolean value via modern switch toggle.
//! Industrial Precision design: clean pill track, responsive thumb.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text, toggler},
};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar, pins};

/// Boolean toggle configuration
#[derive(Debug, Clone, PartialEq)]
pub struct BoolToggleConfig {
    pub label: String,
    pub toggle_label: String,
}

impl Default for BoolToggleConfig {
    fn default() -> Self {
        Self {
            label: "Boolean Toggle".to_string(),
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

/// Creates a boolean toggle node with modern switch styling
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

    // State indicator text
    let state_text = text(if value { "ON" } else { "OFF" })
        .size(10)
        .color(if value {
            colors::PIN_BOOL
        } else {
            colors::TEXT_MUTED
        });

    // Modern switch toggle with Industrial Precision styling
    let toggle_widget = toggler(value)
        .on_toggle(on_change)
        .size(16)
        .style(move |_, status| {
            let is_on = match status {
                toggler::Status::Active { is_toggled } => is_toggled,
                toggler::Status::Hovered { is_toggled } => is_toggled,
                toggler::Status::Disabled { is_toggled } => is_toggled,
            };

            let (background, foreground, foreground_border) = match status {
                toggler::Status::Active { .. } => {
                    if is_on {
                        (colors::PIN_BOOL, Color::WHITE, colors::PIN_BOOL)
                    } else {
                        (
                            colors::SURFACE_ELEVATED,
                            Color::WHITE,
                            colors::BORDER_SUBTLE,
                        )
                    }
                }
                toggler::Status::Hovered { .. } => {
                    if is_on {
                        (colors::PIN_BOOL, colors::PIN_BOOL, Color::WHITE)
                    } else {
                        (colors::BORDER_SUBTLE, Color::WHITE, colors::TEXT_MUTED)
                    }
                }
                toggler::Status::Disabled { .. } => (
                    colors::SURFACE_ELEVATED,
                    colors::TEXT_MUTED,
                    colors::BORDER_SUBTLE,
                ),
            };

            toggler::Style {
                background: background.into(),
                background_border_width: 1.0,
                background_border_color: if is_on {
                    colors::PIN_BOOL
                } else {
                    colors::BORDER_SUBTLE
                },
                foreground: foreground.into(),
                foreground_border_width: 2.0,
                foreground_border_color: foreground_border,
                border_radius: Some(8.0.into()),
                padding_ratio: 0.12,
                text_color: Some(colors::TEXT_PRIMARY),
            }
        });

    // Output pin
    let output_pin = container(pin!(
        Right,
        "value",
        row![
            text(&config.toggle_label)
                .size(11)
                .color(colors::TEXT_PRIMARY),
            container(
                row![toggle_widget, state_text]
                    .spacing(6)
                    .align_y(iced::Alignment::Center)
            )
            .width(Length::Fill)
            .align_x(Horizontal::Right),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        Output,
        pins::Bool,
        colors::PIN_BOOL
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar(&config.label, style),
        container(output_pin).padding([10, 12])
    ]
    .width(180.0)
    .into()
}
