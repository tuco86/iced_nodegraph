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

/// Default node corner radius. Kept in sync with `default_node_style` so a
/// `simple_node` built on the theme base lines up with the rendered fill.
const DEFAULT_CORNER_RADIUS: f32 = 5.0;
/// Default node border width. Kept in sync with `default_node_style`.
const DEFAULT_BORDER_WIDTH: f32 = 1.0;

impl NodeContentStyle {
    /// Sets the corner radius for this style.
    pub fn with_geometry(mut self, corner_radius: f32, border_width: f32) -> Self {
        self.corner_radius = corner_radius;
        self.border_width = border_width;
        self
    }

    /// Creates a themed node content style from an accent color.
    ///
    /// Dark themes tint the title background by darkening the accent color.
    /// Light themes tint it by lightening towards white.
    fn from_accent(accent: Color, theme: &Theme) -> Self {
        const DARK_TINT: f32 = 0.35;
        const LIGHT_TINT: f32 = 0.15;

        let palette = theme.extended_palette();
        let title_background = if palette.is_dark {
            Color::from_rgba(
                accent.r * DARK_TINT,
                accent.g * DARK_TINT,
                accent.b * DARK_TINT,
                0.9,
            )
        } else {
            Color::from_rgba(
                1.0 - (1.0 - accent.r) * LIGHT_TINT,
                1.0 - (1.0 - accent.g) * LIGHT_TINT,
                1.0 - (1.0 - accent.b) * LIGHT_TINT,
                0.9,
            )
        };

        Self {
            title_background,
            title_text: palette.background.base.text,
            body_background: Color::TRANSPARENT,
            body_text: palette.background.base.text,
            accent,
            corner_radius: DEFAULT_CORNER_RADIUS,
            border_width: DEFAULT_BORDER_WIDTH,
        }
    }

    /// Creates an input node style derived from theme's primary color.
    pub fn input(theme: &Theme) -> Self {
        Self::from_accent(theme.extended_palette().primary.base.color, theme)
    }

    /// Creates a process node style derived from theme's success color.
    pub fn process(theme: &Theme) -> Self {
        Self::from_accent(theme.extended_palette().success.base.color, theme)
    }

    /// Creates an output node style derived from theme's secondary color.
    pub fn output(theme: &Theme) -> Self {
        Self::from_accent(theme.extended_palette().secondary.base.color, theme)
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

/// Corner radii for the two corners along one node edge.
///
/// Accepts a single value (both corners equal) or a `(left, right)` tuple via
/// `impl Into<EdgeRadii>`, so a header/footer can match the node's rounded
/// corners with one number or round each corner differently.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeRadii {
    /// The left (or top-left/bottom-left) corner radius.
    pub left: f32,
    /// The right (or top-right/bottom-right) corner radius.
    pub right: f32,
}

impl From<f32> for EdgeRadii {
    fn from(value: f32) -> Self {
        Self {
            left: value,
            right: value,
        }
    }
}

impl From<(f32, f32)> for EdgeRadii {
    fn from((left, right): (f32, f32)) -> Self {
        Self { left, right }
    }
}

/// Build the per-corner [`border::Radius`] for a section, rounding only the
/// corners that touch the node edge: a header rounds the top pair, a footer the
/// bottom, `Full` all four, `Middle` none. Stacked header + footer reconstruct
/// the node's full rounded silhouette.
pub(crate) fn section_border_radius(radii: EdgeRadii, position: ContentPosition) -> border::Radius {
    let (l, r) = (radii.left, radii.right);
    match position {
        ContentPosition::Top => border::Radius {
            top_left: l,
            top_right: r,
            bottom_right: 0.0,
            bottom_left: 0.0,
        },
        ContentPosition::Bottom => border::Radius {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: r,
            bottom_left: l,
        },
        ContentPosition::Full => border::Radius {
            top_left: l,
            top_right: r,
            bottom_right: r,
            bottom_left: l,
        },
        ContentPosition::Middle => border::Radius::from(0.0),
    }
}

/// Creates a simple node with title bar and content area.
///
/// This is a convenience function for building common node structures.
/// Uses default node geometry (corner_radius=5.0, border_width=1.0).
///
/// The returned element is `Length::Fill` in width so the title bar and body
/// stay aligned with the rendered node fill. Constrain it with a fixed-width
/// parent, e.g. `container(simple_node(..)).width(160.0)`.
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
    );

    // The body fills the node width to match the header (node_header is
    // Length::Fill); otherwise it would shrink to its content and sit
    // misaligned inside a wider node, offsetting the rendered node fill.
    column![
        title_bar,
        container(content)
            .width(Length::Fill)
            .padding(Padding {
                top: 6.0,
                bottom: 6.0,
                left: 8.0,
                right: 8.0,
            })
            .style(move |_theme: &Theme| container::Style {
                background: Some(style.body_background.into()),
                text_color: Some(style.body_text),
                ..Default::default()
            })
    ]
    .width(Length::Fill)
    .into()
}

/// Wraps `content` in a `Length::Fill` header: a rounded box with its top two
/// corners rounded to `radii` and filled with `background`.
///
/// `radii` accepts a single value (both corners equal) or a `(left, right)`
/// tuple. To match a node's silhouette exactly, pass the node's `corner_radius`.
/// The returned [`Container`] can be laid out further by the caller.
///
/// # Example
/// ```ignore
/// use iced_nodegraph::node_header;
/// use iced::{widget::text, Color};
///
/// let header = node_header(text("Title"), Color::from_rgb(0.2, 0.3, 0.4), 5.0);
/// let header = node_header(text("Title"), Color::BLACK, (4.0, 8.0));
/// ```
pub fn node_header<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    background: Color,
    radii: impl Into<EdgeRadii>,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    node_section(content, background, radii.into(), ContentPosition::Top)
}

/// Wraps `content` in a `Length::Fill` footer: a rounded box with its bottom two
/// corners rounded to `radii` and filled with `background`.
///
/// `radii` accepts a single value or a `(left, right)` tuple; pass the node's
/// `corner_radius` to match its silhouette. The returned [`Container`] can be
/// laid out further by the caller.
///
/// # Example
/// ```ignore
/// use iced_nodegraph::node_footer;
/// use iced::{widget::text, Color};
///
/// let footer = node_footer(text("Footer"), Color::from_rgb(0.15, 0.15, 0.15), 5.0);
/// ```
pub fn node_footer<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    background: Color,
    radii: impl Into<EdgeRadii>,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    node_section(content, background, radii.into(), ContentPosition::Bottom)
}

/// Shared rounded-box section for header/footer: fills `background` and rounds
/// the corners at `position` to `radii`, at `Length::Fill` width.
fn node_section<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    background: Color,
    radii: EdgeRadii,
    position: ContentPosition,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    let radius = section_border_radius(radii, position);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(background.into()),
            border: Border {
                radius,
                ..Default::default()
            },
            ..Default::default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{NodeStatus, default_node_style};

    /// A single radius rounds both corners of the edge equally.
    #[test]
    fn single_radius_rounds_both_corners() {
        let r = section_border_radius(6.0.into(), ContentPosition::Top);
        assert_eq!((r.top_left, r.top_right), (6.0, 6.0));
        assert_eq!((r.bottom_left, r.bottom_right), (0.0, 0.0));
    }

    /// A `(left, right)` tuple rounds the two corners independently.
    #[test]
    fn tuple_radius_rounds_corners_independently() {
        let top = section_border_radius((4.0, 8.0).into(), ContentPosition::Top);
        assert_eq!((top.top_left, top.top_right), (4.0, 8.0));

        let bottom = section_border_radius((4.0, 8.0).into(), ContentPosition::Bottom);
        assert_eq!((bottom.bottom_left, bottom.bottom_right), (4.0, 8.0));
    }

    /// Header rounds only the top corners, footer only the bottom; stacked they
    /// reconstruct the node's full rounded silhouette with no double or missing
    /// rounding at the seam.
    #[test]
    fn header_and_footer_round_complementary_corners() {
        let cr: EdgeRadii = 6.0.into();
        let header = section_border_radius(cr, ContentPosition::Top);
        let footer = section_border_radius(cr, ContentPosition::Bottom);
        let full = section_border_radius(cr, ContentPosition::Full);

        assert_eq!((header.top_left, header.top_right), (6.0, 6.0));
        assert_eq!((header.bottom_left, header.bottom_right), (0.0, 0.0));
        assert_eq!((footer.bottom_left, footer.bottom_right), (6.0, 6.0));
        assert_eq!((footer.top_left, footer.top_right), (0.0, 0.0));

        // Header top + footer bottom equal the all-around full rounding.
        assert_eq!(header.top_left, full.top_left);
        assert_eq!(footer.bottom_right, full.bottom_right);
    }

    /// The default `simple_node`/header geometry matches the node the widget
    /// actually renders: the fill uses `NodeStyle::corner_radius` directly, and
    /// `NodeContentStyle` defaults to the same value, so they line up flush.
    #[test]
    fn default_content_matches_rendered_node() {
        let theme = iced::Theme::Dark;
        let fill = default_node_style(&theme, NodeStatus::Idle).corner_radius;
        let content = NodeContentStyle::input(&theme).corner_radius;
        assert_eq!(
            content, fill,
            "NodeContentStyle default radius must equal the rendered fill radius"
        );

        let header = section_border_radius(content.into(), ContentPosition::Top);
        assert_eq!(header.top_left, fill);
    }
}
