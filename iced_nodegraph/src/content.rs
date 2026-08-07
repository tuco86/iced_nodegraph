//! Rounded header/footer helpers for node interiors.
//!
//! Iced composites a node's interior from stacked containers, but a container
//! only rounds all four of its corners at once - so a title bar stacked on a body
//! cannot, on its own, reproduce the node's rounded silhouette (rounded only at
//! the very top and bottom). [`node_header`] and [`node_footer`] solve that: each
//! rounds only the two corners that touch the node edge, so header + body + footer
//! reconstruct the full rounded outline with a flush seam in between.

use iced_widget::core::{Border, Color, Element, Length, Theme, border};
use iced_widget::renderer::Renderer;
use iced_widget::{Container, container};

/// Which edge of the node a section sits against: a header rounds the top pair
/// of corners, a footer the bottom pair.
#[derive(Clone, Copy)]
enum ContentPosition {
    Top,
    Bottom,
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

/// Build the per-corner [`border::Radius`] for a section, rounding only the two
/// corners that touch the node edge: a header rounds the top pair, a footer the
/// bottom pair. Stacked, they reconstruct the node's full rounded silhouette.
fn section_border_radius(radii: EdgeRadii, position: ContentPosition) -> border::Radius {
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
    }
}

/// Wraps `content` in a `Length::Fill` header: a rounded box with its top two
/// corners rounded to `radii` and filled with `background`.
///
/// `radii` accepts a single value (both corners equal) or a `(left, right)`
/// tuple. To match a node's silhouette exactly, pass the node's `corner_radius`.
/// The returned [`Container`] can be laid out further by the caller.
///
/// # Example
/// ```rust
/// use iced_nodegraph::node_header;
/// use iced::{widget::text, Color};
///
/// # #[derive(Clone)]
/// # enum Message {}
/// let flush: iced::widget::Container<'_, Message> =
///     node_header(text("Title"), Color::from_rgb(0.2, 0.3, 0.4), 5.0);
/// let asymmetric: iced::widget::Container<'_, Message> =
///     node_header(text("Title"), Color::BLACK, (4.0, 8.0));
/// ```
pub fn node_header<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    background: Color,
    radii: impl Into<EdgeRadii>,
) -> Container<'a, Message, Theme, Renderer>
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
/// ```rust
/// use iced_nodegraph::node_footer;
/// use iced::{widget::text, Color};
///
/// # #[derive(Clone)]
/// # enum Message {}
/// let footer: iced::widget::Container<'_, Message> =
///     node_footer(text("Footer"), Color::from_rgb(0.15, 0.15, 0.15), 5.0);
/// ```
pub fn node_footer<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    background: Color,
    radii: impl Into<EdgeRadii>,
) -> Container<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    node_section(content, background, radii.into(), ContentPosition::Bottom)
}

/// Shared rounded-box section for header/footer: fills `background` and rounds
/// the corners at `position` to `radii`, at `Length::Fill` width.
fn node_section<'a, Message>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    background: Color,
    radii: EdgeRadii,
    position: ContentPosition,
) -> Container<'a, Message, Theme, Renderer>
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

        assert_eq!((header.top_left, header.top_right), (6.0, 6.0));
        assert_eq!((header.bottom_left, header.bottom_right), (0.0, 0.0));
        assert_eq!((footer.bottom_left, footer.bottom_right), (6.0, 6.0));
        assert_eq!((footer.top_left, footer.top_right), (0.0, 0.0));
    }
}
