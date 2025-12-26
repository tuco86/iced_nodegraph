//! Node content styling helpers.
//!
//! Provides theme-aware helper functions for creating consistent node interiors.
//! These helpers extract colors from Iced's theme system to ensure nodes look good
//! across all built-in themes.

use iced::{
    Border, Color, Element, Length, Padding, Theme, border,
    widget::{Container, column, container, text},
};

/// Position of content within a node, determines which corners get rounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentPosition {
    /// Top section - rounded corners at top only
    Top,
    /// Middle section - no rounded corners
    #[default]
    Middle,
    /// Bottom section - rounded corners at bottom only
    Bottom,
    /// Full node - all corners rounded
    Full,
}

/// Style presets for different node categories.
///
/// Provides color palettes derived from the current theme for consistent
/// node interior styling, plus geometry values for title bars and content containers.
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
    /// Corner radius of the node (for title bar and content container)
    pub corner_radius: f32,
    /// Border width of the node (for inset calculations)
    pub border_width: f32,
}

/// Default corner radius for nodes (used when no resolved style is provided)
const DEFAULT_CORNER_RADIUS: f32 = 8.0;
/// Default border width for nodes (used when no resolved style is provided)
const DEFAULT_BORDER_WIDTH: f32 = 1.0;

impl NodeContentStyle {
    /// Sets the corner radius for this style.
    pub fn with_geometry(mut self, corner_radius: f32, border_width: f32) -> Self {
        self.corner_radius = corner_radius;
        self.border_width = border_width;
        self
    }

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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
            corner_radius: DEFAULT_CORNER_RADIUS,
            border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
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
                corner_radius: DEFAULT_CORNER_RADIUS,
                border_width: DEFAULT_BORDER_WIDTH,
            }
        }
    }
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

/// Creates a container for node content with proper rounded corners.
///
/// Automatically calculates the inner radius and padding based on the node's
/// geometry to ensure content fits precisely within the clipped area.
///
/// # Arguments
/// * `content` - The content to wrap
/// * `corner_radius` - The node's corner radius (typically 5.0)
/// * `border_width` - The node's border width (typically 1.0)
/// * `position` - Which corners should be rounded
///
/// # Example
/// ```ignore
/// let body = node_content_container(
///     my_widgets,
///     5.0,
///     1.0,
///     ContentPosition::Bottom,
/// );
/// ```
pub fn node_content_container<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    corner_radius: f32,
    border_width: f32,
    position: ContentPosition,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    // Inner radius fits inside the node border
    let inner_radius = (corner_radius - border_width).max(0.0);

    // Radius based on position
    let radius = match position {
        ContentPosition::Top => border::top(inner_radius),
        ContentPosition::Bottom => border::bottom(inner_radius),
        ContentPosition::Full => border::radius(inner_radius),
        ContentPosition::Middle => border::radius(0.0),
    };

    container(content)
        .padding(Padding {
            top: 0.0,
            bottom: 0.0,
            left: border_width,
            right: border_width,
        })
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            border: Border {
                radius,
                ..Default::default()
            },
            ..Default::default()
        })
}

/// Creates a simple node with title bar and content area.
///
/// This is a convenience function for building common node structures.
/// Uses default node geometry (corner_radius=5.0, border_width=1.0).
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
    let corner_radius = style.corner_radius;
    let border_width = style.border_width;

    // Title bar using node_header
    let title_text = text(title.into()).size(13).color(style.title_text);
    let title_bar = node_header(
        container(title_text).padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 8.0,
            right: 8.0,
        }),
        style.title_background,
        corner_radius,
        border_width,
    );

    column![
        title_bar,
        container(content)
            .padding(Padding {
                top: 6.0,
                bottom: 6.0,
                left: 8.0 + border_width,
                right: 8.0 + border_width,
            })
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

/// Creates a header container for nodes with top rounded corners.
///
/// Uses the same corner radius as the parent node for consistent appearance.
/// Padding is applied on left/right to account for the node's border.
///
/// # Arguments
/// * `content` - The content to wrap in the header
/// * `background` - Background color for the header
/// * `corner_radius` - The node's corner radius
/// * `border_width` - The node's border width
///
/// # Example
/// ```ignore
/// use iced_nodegraph::{node_header, NodeStyle};
/// use iced::widget::text;
/// use iced::Color;
///
/// // Get geometry from node style
/// let node_style = NodeStyle::default();
/// let header = node_header(
///     text("Header Content"),
///     Color::from_rgb(0.2, 0.3, 0.4),
///     node_style.corner_radius,
///     node_style.border_width,
/// );
/// ```
pub fn node_header<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    background: Color,
    corner_radius: f32,
    border_width: f32,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    container(content)
        .padding(Padding {
            top: 0.0,
            bottom: 0.0,
            left: border_width,
            right: border_width,
        })
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(background.into()),
            border: Border {
                radius: border::top(corner_radius),
                width: border_width,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
}

/// Creates a footer container for nodes with bottom rounded corners.
///
/// Uses the same corner radius as the parent node for consistent appearance.
/// Padding is applied on left/right to account for the node's border.
///
/// # Arguments
/// * `content` - The content to wrap in the footer
/// * `background` - Background color for the footer
/// * `corner_radius` - The node's corner radius
/// * `border_width` - The node's border width
///
/// # Example
/// ```ignore
/// use iced_nodegraph::{node_footer, NodeStyle};
/// use iced::widget::text;
/// use iced::Color;
///
/// // Get geometry from node style
/// let node_style = NodeStyle::default();
/// let footer = node_footer(
///     text("Footer Content"),
///     Color::from_rgb(0.15, 0.15, 0.15),
///     node_style.corner_radius,
///     node_style.border_width,
/// );
/// ```
pub fn node_footer<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    background: Color,
    corner_radius: f32,
    border_width: f32,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    container(content)
        .padding(Padding {
            top: 0.0,
            bottom: 0.0,
            left: border_width,
            right: border_width,
        })
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(background.into()),
            border: Border {
                radius: border::bottom(corner_radius),
                width: border_width,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
}
