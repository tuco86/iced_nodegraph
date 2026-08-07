//! Themed node-interior helpers for the demos.
//!
//! [`NodeContentStyle`] derives title/body colors from the active iced theme so
//! demo nodes look consistent across every built-in theme, and
//! [`node_title_bar`] renders a node's header in those colors. These are demo
//! conveniences, not part of the `iced_nodegraph` widget: the widget renders
//! the node fill/border/pins; the app decides what goes inside. The title bar
//! is built with the library's [`node_header`](iced_nodegraph::node_header) so
//! its rounded top corners match the rendered node silhouette.

use iced::{
    Color, Padding, Theme,
    widget::{Container, container, text},
};
use iced_nodegraph::node_header;

/// Default node corner radius. Kept in sync with the widget's `default_node_style`
/// so a `simple_node` lines up with the rendered fill (asserted in tests).
const DEFAULT_CORNER_RADIUS: f32 = 5.0;
/// Default node border width. Kept in sync with `default_node_style`.
const DEFAULT_BORDER_WIDTH: f32 = 1.0;

/// Theme-derived color palette for a node's interior.
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

/// Creates a themed title bar container for nodes.
///
/// Built on the library's [`node_header`], so its rounded top corners match the
/// silhouette the widget renders behind it.
pub fn node_title_bar<'a, Message>(
    title: impl Into<String>,
    style: NodeContentStyle,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    let title_text = text(title.into()).size(13).color(style.title_text);

    node_header(
        container(title_text).padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 8.0,
            right: 8.0,
        }),
        style.title_background,
        style.corner_radius,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_nodegraph::{NodeStatus, default_node_style};

    /// The default `NodeContentStyle` geometry must match the node the widget
    /// actually renders: the fill uses `NodeStyle::corner_radius`, and the
    /// content style defaults to the same value, so header and fill line up flush.
    #[test]
    fn default_content_matches_rendered_node() {
        let theme = Theme::Dark;
        let fill = default_node_style(&theme, NodeStatus::Idle).corner_radius;
        let content = NodeContentStyle::input(&theme).corner_radius;
        assert_eq!(
            content, fill,
            "NodeContentStyle default radius must equal the rendered fill radius"
        );
    }
}
