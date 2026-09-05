//! Frame Node
//!
//! A titled, empty region that groups the nodes laid over it. The graph moves
//! the contents with the frame (`iced_nodegraph::Node::frame`), so the boot rig
//! uses one frame per Catalog class.

use demo_common::NodeContentStyle;
use iced::{
    Length, Size,
    widget::{Space, column, container},
};

use crate::nodes::node_title_bar;

/// Creates a frame node: a title bar over an empty body at the given size.
pub fn frame_node<'a, Message>(
    theme: &'a iced::Theme,
    label: &'a str,
    size: Size,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    container(column![
        node_title_bar(label, NodeContentStyle::comment(theme)),
        Space::new().width(Length::Fill).height(Length::Fill),
    ])
    .width(size.width)
    .height(size.height)
    .into()
}
