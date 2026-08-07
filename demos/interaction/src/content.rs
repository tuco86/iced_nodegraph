//! The node interior this demo builds: a themed title bar over a padded body.
//!
//! Local to this demo - the other demos compose their own interiors from
//! [`demo_common::node_title_bar`] instead.

use demo_common::NodeContentStyle;
use iced::{
    Element, Length, Padding, Theme,
    widget::{column, container, text},
};
use iced_nodegraph::node_header;

/// Creates a simple node with title bar and content area.
///
/// This is a convenience function for building common node structures.
/// Uses default node geometry (corner_radius=5.0, border_width=1.0).
///
/// The returned element is `Length::Fill` in width so the title bar and body
/// stay aligned with the rendered node fill. Constrain it with a fixed-width
/// parent, e.g. `container(simple_node(..)).width(160.0)`.
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
