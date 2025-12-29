//! Background Configuration Node
//!
//! Builds a BackgroundConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{BackgroundConfig, BackgroundPattern, NodeContentStyle, pin};

use crate::nodes::{colors, node_title_bar, pins};

/// Pattern type for UI display
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PatternTypeSelection {
    #[default]
    None,
    Grid,
    Hex,
    Triangle,
    Dots,
    Lines,
    Crosshatch,
}

impl PatternTypeSelection {
    pub fn to_background_pattern(self) -> BackgroundPattern {
        match self {
            Self::None => BackgroundPattern::None,
            Self::Grid => BackgroundPattern::Grid,
            Self::Hex => BackgroundPattern::Hex,
            Self::Triangle => BackgroundPattern::Triangle,
            Self::Dots => BackgroundPattern::Dots,
            Self::Lines => BackgroundPattern::Lines,
            Self::Crosshatch => BackgroundPattern::Crosshatch,
        }
    }
}

/// Collected inputs for BackgroundConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<BackgroundConfig>,

    // Pattern
    pub pattern: Option<PatternTypeSelection>,

    // Colors
    pub background_color: Option<Color>,
    pub primary_color: Option<Color>,
    pub secondary_color: Option<Color>,

    // Spacing
    pub minor_spacing: Option<f32>,
    pub major_spacing: Option<f32>,

    // Line properties
    pub minor_width: Option<f32>,
    pub major_width: Option<f32>,
    pub minor_opacity: Option<f32>,
    pub major_opacity: Option<f32>,

    // Pattern-specific
    pub dot_radius: Option<f32>,
    pub line_angle: Option<f32>,

    // Adaptive zoom
    pub adaptive_zoom: Option<bool>,
}

impl BackgroundConfigInputs {
    /// Builds the final BackgroundConfig by merging with parent
    pub fn build(&self) -> BackgroundConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        BackgroundConfig {
            pattern: self
                .pattern
                .map(|p| p.to_background_pattern())
                .or(parent.pattern),
            background_color: self.background_color.or(parent.background_color),
            primary_color: self.primary_color.or(parent.primary_color),
            secondary_color: self.secondary_color.or(parent.secondary_color),
            minor_spacing: self.minor_spacing.or(parent.minor_spacing),
            major_spacing: self.major_spacing.map(Some).or(parent.major_spacing),
            minor_width: self.minor_width.or(parent.minor_width),
            major_width: self.major_width.or(parent.major_width),
            minor_opacity: self.minor_opacity.or(parent.minor_opacity),
            major_opacity: self.major_opacity.or(parent.major_opacity),
            dot_radius: self.dot_radius.or(parent.dot_radius),
            line_angle: self.line_angle.or(parent.line_angle),
            adaptive_zoom: self.adaptive_zoom.or(parent.adaptive_zoom),
            ..parent
        }
    }
}

/// Creates a BackgroundConfig configuration node with essential field inputs
pub fn background_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &BackgroundConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    use iced::alignment::Horizontal;

    let style = NodeContentStyle::process(theme);
    let result = inputs.build();

    // Config row: input left, typed output right
    let config_row = row![
        pin!(
            Left,
            pins::config::CONFIG,
            text("in").size(10),
            Input,
            pins::BackgroundConfigData,
            colors::PIN_CONFIG
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::config::BACKGROUND_OUT,
            text("out").size(10),
            Output,
            pins::BackgroundConfigData,
            colors::PIN_CONFIG
        ),
    ]
    .align_y(iced::Alignment::Center);

    // Helper to create separator lines
    let make_separator = || {
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_: &_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.1,
                ))),
                ..Default::default()
            })
    };

    // Pattern type row
    let pattern_name = result
        .pattern
        .map(|p| match p {
            BackgroundPattern::None => "None",
            BackgroundPattern::Grid => "Grid",
            BackgroundPattern::Hex => "Hex",
            BackgroundPattern::Triangle => "Triangle",
            BackgroundPattern::Dots => "Dots",
            BackgroundPattern::Lines => "Lines",
            BackgroundPattern::Crosshatch => "Crosshatch",
        })
        .unwrap_or("--");
    let pattern_row = row![
        pin!(
            Left,
            pins::config::PATTERN,
            text("pattern").size(10),
            Input,
            pins::PatternTypeData,
            colors::PIN_STRING
        ),
        container(text(pattern_name).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Background color row
    let bg_display: iced::Element<'a, Message> = if let Some(c) = result.background_color {
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
    let bg_color_row = row![
        pin!(
            Left,
            pins::config::BACKGROUND_COLOR,
            text("bg").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(bg_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Primary color row
    let primary_display: iced::Element<'a, Message> = if let Some(c) = result.primary_color {
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
    let primary_row = row![
        pin!(
            Left,
            pins::config::PRIMARY_COLOR,
            text("primary").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(primary_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Minor spacing row
    let spacing_display = result
        .minor_spacing
        .map(|s| format!("{:.0}", s))
        .unwrap_or_else(|| "--".to_string());
    let spacing_row = row![
        pin!(
            Left,
            pins::config::MINOR_SPACING,
            text("spacing").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(text(spacing_display).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Adaptive zoom row
    let adaptive_display = result
        .adaptive_zoom
        .map(|a| if a { "yes" } else { "no" })
        .unwrap_or("--");
    let adaptive_row = row![
        pin!(
            Left,
            pins::config::ADAPTIVE_ZOOM,
            text("adaptive").size(10),
            Input,
            pins::Bool,
            colors::PIN_BOOL
        ),
        container(text(adaptive_display).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        make_separator(),
        pattern_row,
        bg_color_row,
        primary_row,
        spacing_row,
        adaptive_row,
    ]
    .spacing(4);

    column![
        node_title_bar("Background", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
