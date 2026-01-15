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

use super::{colors, node_title_bar, pins};

/// Float slider node configuration
#[derive(Debug, Clone, PartialEq)]
pub struct FloatSliderConfig {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub label: String,
    // Temporary edit buffers for text inputs (only used during editing)
    #[allow(dead_code)]
    pub min_edit: Option<String>,
    #[allow(dead_code)]
    pub max_edit: Option<String>,
    #[allow(dead_code)]
    pub step_edit: Option<String>,
}

impl Default for FloatSliderConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 20.0,
            step: 0.1,
            label: "Float Slider".to_string(),
            min_edit: None,
            max_edit: None,
            step_edit: None,
        }
    }
}

#[allow(dead_code)]
impl FloatSliderConfig {
    /// Creates a config with the given label
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }

    /// Sets the range
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Named constructor for corner radius config
    pub fn corner_radius() -> Self {
        Self::new("Corner Radius").range(0.0, 30.0)
    }

    /// Named constructor for opacity config
    pub fn opacity() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            step: 0.01,
            label: "Opacity".to_string(),
            ..Default::default()
        }
    }

    /// Named constructor for border width config
    pub fn border_width() -> Self {
        Self::new("Border Width").range(0.0, 10.0)
    }

    /// Named constructor for blur radius config
    pub fn blur_radius() -> Self {
        Self::new("Blur Radius").range(0.0, 50.0)
    }

    /// Named constructor for offset X config (supports negative)
    pub fn offset_x() -> Self {
        Self {
            min: -50.0,
            max: 50.0,
            step: 1.0,
            label: "Offset X".to_string(),
            ..Default::default()
        }
    }

    /// Named constructor for offset Y config (supports negative)
    pub fn offset_y() -> Self {
        Self {
            min: -50.0,
            max: 50.0,
            step: 1.0,
            label: "Offset Y".to_string(),
            ..Default::default()
        }
    }

    /// Named constructor for pattern angle config (degrees)
    pub fn pattern_angle() -> Self {
        Self {
            min: -90.0,
            max: 90.0,
            step: 5.0,
            label: "Pattern Angle".to_string(),
            ..Default::default()
        }
    }

    /// Named constructor for thickness config
    pub fn thickness() -> Self {
        Self::new("Thickness").range(0.5, 10.0)
    }

    /// Named constructor for pin radius config
    pub fn pin_radius() -> Self {
        Self::new("Pin Radius").range(2.0, 20.0)
    }

    /// Named constructor for dash length
    pub fn dash_length() -> Self {
        Self {
            min: 1.0,
            max: 50.0,
            step: 1.0,
            label: "Dash".to_string(),
            ..Default::default()
        }
    }

    /// Named constructor for gap length
    pub fn gap_length() -> Self {
        Self {
            min: 1.0,
            max: 50.0,
            step: 1.0,
            label: "Gap".to_string(),
            ..Default::default()
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
        pin!(
            Right,
            "value",
            value_display,
            Output,
            pins::Float,
            colors::PIN_NUMBER
        )
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    // Build expanded options if needed
    let body_content: iced::Element<'a, Message> = if expanded {
        // Get display values from edit buffers or actual values
        let min_display = config
            .min_edit
            .clone()
            .unwrap_or_else(|| format!("{}", config.min));
        let max_display = config
            .max_edit
            .clone()
            .unwrap_or_else(|| format!("{}", config.max));
        let step_display = config
            .step_edit
            .clone()
            .unwrap_or_else(|| format!("{}", config.step));

        // Min input - track edits, apply on Enter
        let min_input = {
            let cfg = config.clone();
            let on_cfg = on_config_change.clone();
            let on_cfg2 = on_config_change.clone();
            row![
                text("Min").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &min_display)
                    .on_input(move |s| {
                        // Just store the edit, don't validate yet
                        on_cfg(FloatSliderConfig {
                            min_edit: Some(s),
                            ..cfg.clone()
                        })
                    })
                    .on_submit({
                        let cfg = config.clone();
                        // On Enter: parse, validate, and apply
                        if let Some(ref edit) = config.min_edit {
                            if let Ok(v) = edit.parse::<f32>() {
                                if v.is_finite() && v < cfg.max {
                                    on_cfg2(FloatSliderConfig {
                                        min: v,
                                        min_edit: None,
                                        ..cfg
                                    })
                                } else {
                                    // Invalid: reset to current value
                                    on_cfg2(FloatSliderConfig {
                                        min_edit: None,
                                        ..cfg
                                    })
                                }
                            } else {
                                // Parse failed: reset
                                on_cfg2(FloatSliderConfig {
                                    min_edit: None,
                                    ..cfg
                                })
                            }
                        } else {
                            on_cfg2(cfg)
                        }
                    })
                    .size(10)
                    .width(Length::Fixed(60.0))
                    .padding(4)
                    .style(config_input_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Max input - track edits, apply on Enter
        let max_input = {
            let cfg = config.clone();
            let on_cfg = on_config_change.clone();
            let on_cfg2 = on_config_change.clone();
            row![
                text("Max").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &max_display)
                    .on_input(move |s| {
                        on_cfg(FloatSliderConfig {
                            max_edit: Some(s),
                            ..cfg.clone()
                        })
                    })
                    .on_submit({
                        let cfg = config.clone();
                        if let Some(ref edit) = config.max_edit {
                            if let Ok(v) = edit.parse::<f32>() {
                                if v.is_finite() && v > cfg.min {
                                    on_cfg2(FloatSliderConfig {
                                        max: v,
                                        max_edit: None,
                                        ..cfg
                                    })
                                } else {
                                    on_cfg2(FloatSliderConfig {
                                        max_edit: None,
                                        ..cfg
                                    })
                                }
                            } else {
                                on_cfg2(FloatSliderConfig {
                                    max_edit: None,
                                    ..cfg
                                })
                            }
                        } else {
                            on_cfg2(cfg)
                        }
                    })
                    .size(10)
                    .width(Length::Fixed(60.0))
                    .padding(4)
                    .style(config_input_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Step input - track edits, apply on Enter
        let step_input = {
            let cfg = config.clone();
            let on_cfg = on_config_change.clone();
            let on_cfg2 = on_config_change;
            row![
                text("Step").size(10).color(colors::TEXT_MUTED).width(30),
                text_input("", &step_display)
                    .on_input(move |s| {
                        on_cfg(FloatSliderConfig {
                            step_edit: Some(s),
                            ..cfg.clone()
                        })
                    })
                    .on_submit({
                        let cfg = config.clone();
                        if let Some(ref edit) = config.step_edit {
                            if let Ok(v) = edit.parse::<f32>() {
                                if v.is_finite() && v > 0.0 {
                                    on_cfg2(FloatSliderConfig {
                                        step: v,
                                        step_edit: None,
                                        ..cfg
                                    })
                                } else {
                                    on_cfg2(FloatSliderConfig {
                                        step_edit: None,
                                        ..cfg
                                    })
                                }
                            } else {
                                on_cfg2(FloatSliderConfig {
                                    step_edit: None,
                                    ..cfg
                                })
                            }
                        } else {
                            on_cfg2(cfg)
                        }
                    })
                    .size(10)
                    .width(Length::Fixed(60.0))
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
