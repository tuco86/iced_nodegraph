//! Node content styling helpers.
//!
//! Provides theme-aware helper functions for creating consistent node interiors.
//! These helpers extract colors from Iced's theme system to ensure nodes look good
//! across all built-in themes.

use iced::{
    Color, Element, Length, Theme,
    widget::{Container, column, container, text},
};

/// Style presets for different node categories.
///
/// Provides color palettes derived from the current theme for consistent
/// node interior styling.
#[derive(Debug, Clone)]
pub struct NodeContentStyle {
    /// Background color for the title bar area
    pub title_background: Color,
    /// Text color for the title
    pub title_text: Color,
    /// Background color for the node body
    pub body_background: Color,
    /// Text color for body content
    pub body_text: Color,
    /// Accent color for highlights and decorations
    pub accent: Color,
}

impl NodeContentStyle {
    /// Creates an input node style derived from theme's primary color.
    pub fn input(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let primary = palette.primary.base.color;

        if palette.is_dark {
            Self {
                title_background: Color::from_rgba(
                    primary.r * 0.35,
                    primary.g * 0.35,
                    primary.b * 0.35,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: primary,
            }
        } else {
            Self {
                title_background: Color::from_rgba(
                    1.0 - (1.0 - primary.r) * 0.15,
                    1.0 - (1.0 - primary.g) * 0.15,
                    1.0 - (1.0 - primary.b) * 0.15,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: primary,
            }
        }
    }

    /// Creates a process node style derived from theme's success color.
    pub fn process(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let success = palette.success.base.color;

        if palette.is_dark {
            Self {
                title_background: Color::from_rgba(
                    success.r * 0.35,
                    success.g * 0.35,
                    success.b * 0.35,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: success,
            }
        } else {
            Self {
                title_background: Color::from_rgba(
                    1.0 - (1.0 - success.r) * 0.15,
                    1.0 - (1.0 - success.g) * 0.15,
                    1.0 - (1.0 - success.b) * 0.15,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: success,
            }
        }
    }

    /// Creates an output node style derived from theme's secondary color.
    pub fn output(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let secondary = palette.secondary.base.color;

        if palette.is_dark {
            Self {
                title_background: Color::from_rgba(
                    secondary.r * 0.35,
                    secondary.g * 0.35,
                    secondary.b * 0.35,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: secondary,
            }
        } else {
            Self {
                title_background: Color::from_rgba(
                    1.0 - (1.0 - secondary.r) * 0.15,
                    1.0 - (1.0 - secondary.g) * 0.15,
                    1.0 - (1.0 - secondary.b) * 0.15,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent: secondary,
            }
        }
    }

    /// Creates a comment node style from theme's background weak color.
    pub fn comment(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let weak = palette.background.weak.color;
        let weak_text = palette.background.weak.text;

        Self {
            title_background: Color::from_rgba(weak.r, weak.g, weak.b, 0.7),
            title_text: weak_text,
            body_background: Color::TRANSPARENT,
            body_text: Color::from_rgba(weak_text.r, weak_text.g, weak_text.b, 0.8),
            accent: weak,
        }
    }

    /// Creates a custom style with the specified accent color.
    pub fn custom(theme: &Theme, accent: Color) -> Self {
        let palette = theme.extended_palette();
        let is_dark = palette.is_dark;

        if is_dark {
            Self {
                title_background: Color::from_rgba(
                    accent.r * 0.4,
                    accent.g * 0.4,
                    accent.b * 0.4,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent,
            }
        } else {
            Self {
                title_background: Color::from_rgba(
                    1.0 - (1.0 - accent.r) * 0.3,
                    1.0 - (1.0 - accent.g) * 0.3,
                    1.0 - (1.0 - accent.b) * 0.3,
                    0.9,
                ),
                title_text: palette.background.base.text,
                body_background: Color::TRANSPARENT,
                body_text: palette.background.base.text,
                accent,
            }
        }
    }
}

/// Creates a themed title bar container for nodes.
///
/// # Example
/// ```ignore
/// let title = node_title_bar("My Node", NodeContentStyle::process(theme));
/// ```
pub fn node_title_bar<'a, Message>(
    title: impl Into<String>,
    style: NodeContentStyle,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    let title_text = text(title.into()).size(13).color(style.title_text);

    container(title_text)
        .padding([4, 8])
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(style.title_background.into()),
            text_color: Some(style.title_text),
            ..Default::default()
        })
}

/// Creates a themed label row for node content.
///
/// # Example
/// ```ignore
/// let label = node_label("Parameter:", NodeContentStyle::input(theme));
/// ```
pub fn node_label<'a, Message>(
    label: impl Into<String>,
    style: NodeContentStyle,
) -> Element<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    text(label.into()).size(12).color(style.body_text).into()
}

/// Creates a themed horizontal separator for nodes.
///
/// Note: This is a simple container-based separator since horizontal_rule
/// may not be available in all Iced versions.
pub fn node_separator<'a, Message>(
    style: NodeContentStyle,
) -> Element<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(style.accent.into()),
            ..Default::default()
        })
        .into()
}

/// Creates a simple node with title bar and content area.
///
/// This is a convenience function for building common node structures.
///
/// # Example
/// ```ignore
/// let node = simple_node(
///     "Email Parser",
///     NodeContentStyle::process(theme),
///     column![
///         node_pin(PinSide::Left, text!("input")),
///         node_pin(PinSide::Right, text!("output")),
///     ]
/// );
/// ```
pub fn simple_node<'a, Message>(
    title: impl Into<String>,
    style: NodeContentStyle,
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
) -> Element<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    column![
        node_title_bar(title, style.clone()),
        container(content)
            .padding([6, 8])
            .style(move |_theme: &Theme| container::Style {
                background: Some(style.body_background.into()),
                text_color: Some(style.body_text),
                ..Default::default()
            })
    ]
    .into()
}

/// Returns theme-aware text color for node content.
pub fn get_text_color(theme: &Theme) -> Color {
    theme.extended_palette().background.base.text
}

/// Returns whether the current theme is dark.
pub fn is_theme_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}
