//! Float Slider Input Node
//!
//! Outputs a configurable float value via slider.
//! Industrial Precision design: clean track, responsive handle.
//! Supports expandable options for configuring min/max/step.

use iced::{
    Color, Length,
    widget::{button, column, container, row, slider, text, text_input},
};
use iced_nodegraph::{NodeContentStyle, node_footer, pin};

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
            label: "Float Slider".to_string(),
        }
    }
}

/// Creates a float slider node with modern styling and optional expanded options
pub fn float_slider_node<'a, Message>(
    theme: &'a iced::Theme,
    value: f32,
    config: &FloatSliderConfig,
    expanded: bool,
    on_change: impl Fn(f32) -> Message + 'a,
    on_config_change: impl Fn(FloatSliderConfig) -> Message + Clone + 'a,
    on_expand_toggle: Message,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);
    let corner_radius = style.corner_radius;
    let border_width = style.border_width;

    // Value display with monospace-style formatting
    let value_display = text(format!("{:.1}", value))
        .size(11)
        .color(colors::TEXT_MUTED);

    // Main slider with Industrial Precision styling
    let slider_widget = slider(config.min..=config.max, value, on_change)
        .step(config.step)
        .width(Length::Fixed(100.0))
        .height(16.0)
        .style(slider_style);

    // Expand/collapse button - minimal height
    let expand_icon = if expanded { "−" } else { "···" };
    let expand_button = button(
        text(expand_icon)
            .size(8)
            .color(colors::TEXT_MUTED)
            .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(on_expand_toggle)
    .padding([0, 12])
    .style(|_, status| {
        let bg = match status {
            button::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.15),
            button::Status::Pressed => Color::from_rgba(1.0, 1.0, 1.0, 0.2),
            _ => Color::TRANSPARENT,
        };
        button::Style {
            background: Some(bg.into()),
            text_color: colors::TEXT_MUTED,
            border: iced::Border::default(),
            ..Default::default()
        }
    });

    // Build the main content - slider and pin-wrapped value on same row
    let main_content = row![
        slider_widget,
        pin!(Right, value_display, Output, "float", colors::PIN_NUMBER)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    // Build expanded options if needed
    let body_content: iced::Element<'a, Message> = if expanded {
        let config_clone = config.clone();

        // Min input - only update on valid parse, ensure min < max
        let min_input = {
            let cfg = config_clone.clone();
            let on_cfg = on_config_change.clone();
            row![
                text("Min").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &format!("{}", config.min))
                    .on_input(move |s| {
                        if let Ok(v) = s.parse::<f32>() {
                            if v < cfg.max {
                                return on_cfg(FloatSliderConfig {
                                    min: v,
                                    ..cfg.clone()
                                });
                            }
                        }
                        on_cfg(cfg.clone())
                    })
                    .size(10)
                    .width(Length::Fixed(50.0))
                    .padding(4)
                    .style(config_input_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Max input - only update on valid parse, ensure max > min
        let max_input = {
            let cfg = config_clone.clone();
            let on_cfg = on_config_change.clone();
            row![
                text("Max").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &format!("{}", config.max))
                    .on_input(move |s| {
                        if let Ok(v) = s.parse::<f32>() {
                            if v > cfg.min {
                                return on_cfg(FloatSliderConfig {
                                    max: v,
                                    ..cfg.clone()
                                });
                            }
                        }
                        on_cfg(cfg.clone())
                    })
                    .size(10)
                    .width(Length::Fixed(50.0))
                    .padding(4)
                    .style(config_input_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Step input - only update on valid parse, ensure step > 0
        let step_input = {
            let cfg = config_clone;
            let on_cfg = on_config_change;
            row![
                text("Step").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &format!("{}", config.step))
                    .on_input(move |s| {
                        if let Ok(v) = s.parse::<f32>() {
                            if v > 0.0 {
                                return on_cfg(FloatSliderConfig {
                                    step: v,
                                    ..cfg.clone()
                                });
                            }
                        }
                        on_cfg(cfg.clone())
                    })
                    .size(10)
                    .width(Length::Fixed(50.0))
                    .padding(4)
                    .style(config_input_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Separator line
        let separator = container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_| container::Style {
                background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.1).into()),
                ..Default::default()
            });

        column![
            main_content,
            separator,
            column![min_input, max_input, step_input].spacing(4)
        ]
        .spacing(8)
        .into()
    } else {
        main_content.into()
    };

    // Footer background - slightly darker than body
    let footer_bg = Color::from_rgba(0.0, 0.0, 0.0, 0.15);

    column![
        node_title_bar(&config.label, style),
        container(body_content).padding([10, 12 + border_width as u16]),
        node_footer(
            container(expand_button)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
            footer_bg,
            corner_radius,
            border_width,
        )
    ]
    .width(180.0)
    .into()
}

/// Slider style for the main value slider
fn slider_style(_: &iced::Theme, status: slider::Status) -> slider::Style {
    let (handle_bg, handle_border) = match status {
        slider::Status::Active => (Color::WHITE, colors::PIN_NUMBER),
        slider::Status::Hovered => (colors::PIN_NUMBER, Color::WHITE),
        slider::Status::Dragged => (colors::PIN_NUMBER, Color::WHITE),
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (colors::PIN_NUMBER.into(), colors::SURFACE_ELEVATED.into()),
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
}

/// Text input style for config fields
fn config_input_style(_: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let (bg, border_color) = match status {
        text_input::Status::Active => (
            Color::from_rgba(1.0, 1.0, 1.0, 0.05),
            Color::from_rgba(1.0, 1.0, 1.0, 0.1),
        ),
        text_input::Status::Hovered => (
            Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            Color::from_rgba(1.0, 1.0, 1.0, 0.2),
        ),
        text_input::Status::Focused { .. } => {
            (Color::from_rgba(1.0, 1.0, 1.0, 0.1), colors::PIN_NUMBER)
        }
        text_input::Status::Disabled => (
            Color::from_rgba(1.0, 1.0, 1.0, 0.02),
            Color::from_rgba(1.0, 1.0, 1.0, 0.05),
        ),
    };
    text_input::Style {
        background: bg.into(),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 3.0.into(),
        },
        icon: colors::TEXT_MUTED,
        placeholder: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
        value: Color::WHITE,
        selection: colors::PIN_NUMBER,
    }
}
