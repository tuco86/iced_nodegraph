//! Integer Slider Input Node
//!
//! Outputs a configurable integer value via slider.
//! Supports expandable options for configuring min/max.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{button, column, container, row, slider, text},
};
use iced_nodegraph::{NodeContentStyle, node_footer, pin};

use super::{colors, node_title_bar};

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
            label: "Int Slider".to_string(),
        }
    }
}

impl IntSliderConfig {
    /// Creates a config for node index selection (0-99)
    #[allow(dead_code)]
    pub fn node_index() -> Self {
        Self {
            min: 0,
            max: 99,
            label: "Node Index".to_string(),
        }
    }
}

/// Creates an integer slider node with interactive slider widget and optional expanded options
pub fn int_slider_node<'a, Message>(
    theme: &'a iced::Theme,
    value: i32,
    config: &IntSliderConfig,
    expanded: bool,
    on_change: impl Fn(i32) -> Message + 'a,
    on_config_change: impl Fn(IntSliderConfig) -> Message + Clone + 'a,
    on_expand_toggle: Message,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);
    let corner_radius = style.corner_radius;
    let border_width = style.border_width;

    let value_display = text(format!("{}", value)).size(12);

    // Slider uses f32 internally, we convert
    let slider_widget = slider(
        (config.min as f32)..=(config.max as f32),
        value as f32,
        move |v| on_change(v.round() as i32),
    )
    .step(1.0)
    .width(Length::Fill);

    // Expand/collapse button - minimal height
    let expand_icon = if expanded { "−" } else { "···" };
    let expand_button = button(
        text(expand_icon)
            .size(8)
            .color(colors::TEXT_MUTED)
            .align_x(Horizontal::Center),
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
        pin!(Right, value_display, Output, "int", colors::PIN_NUMBER)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    // Build expanded options if needed
    let body_content: iced::Element<'a, Message> = if expanded {
        let config_clone = config.clone();

        // Min slider
        let min_slider = {
            let cfg = config_clone.clone();
            let on_cfg = on_config_change.clone();
            row![
                text("Min").size(10).color(colors::TEXT_MUTED).width(30),
                slider(-1000i32..=(config.max - 1), config.min, move |v| {
                    on_cfg(IntSliderConfig {
                        min: v,
                        ..cfg.clone()
                    })
                })
                .step(1)
                .width(Length::Fixed(100.0))
                .height(12.0)
                .style(config_slider_style),
                text(format!("{}", config.min))
                    .size(9)
                    .color(colors::TEXT_MUTED),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        };

        // Max slider
        let max_slider = {
            let cfg = config_clone;
            let on_cfg = on_config_change;
            row![
                text("Max").size(10).color(colors::TEXT_MUTED).width(30),
                slider((config.min + 1)..=10000i32, config.max, move |v| {
                    on_cfg(IntSliderConfig {
                        max: v,
                        ..cfg.clone()
                    })
                })
                .step(1)
                .width(Length::Fixed(100.0))
                .height(12.0)
                .style(config_slider_style),
                text(format!("{}", config.max))
                    .size(9)
                    .color(colors::TEXT_MUTED),
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
            column![min_slider, max_slider].spacing(4)
        ]
        .spacing(8)
        .into()
    } else {
        main_content.into()
    };

    // Footer background
    let footer_bg = Color::from_rgba(0.0, 0.0, 0.0, 0.15);

    column![
        node_title_bar(&config.label, style),
        container(body_content).padding([10, 12 + border_width as u16]),
        node_footer(
            container(expand_button)
                .width(Length::Fill)
                .align_x(Horizontal::Center),
            footer_bg,
            corner_radius,
            border_width,
        )
    ]
    .width(200.0)
    .into()
}

/// Slider style for the config sliders (smaller, more subtle)
fn config_slider_style(_: &iced::Theme, status: slider::Status) -> slider::Style {
    let handle_color = match status {
        slider::Status::Active => colors::TEXT_MUTED,
        slider::Status::Hovered | slider::Status::Dragged => Color::WHITE,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Color::from_rgba(1.0, 1.0, 1.0, 0.3).into(),
                Color::from_rgba(1.0, 1.0, 1.0, 0.1).into(),
            ),
            width: 4.0,
            border: iced::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 5.0 },
            background: handle_color.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}
