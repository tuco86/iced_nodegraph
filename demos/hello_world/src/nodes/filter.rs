use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, NodeStyle, pin};

use super::{colors, node_title_bar, pins};

/// Filter Node - Input + output
///
/// Uses the default NodeStyle geometry to ensure the title bar corners
/// match the actual node's rounded corners.
pub fn filter_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    // Use actual NodeStyle defaults for precise corner calculation
    let node_defaults = NodeStyle::default();
    let style = NodeContentStyle::process(theme)
        .with_geometry(node_defaults.corner_radius, node_defaults.border_width);

    let pin_list = row![
        container(pin!(
            Left,
            "input",
            text("input"),
            Input,
            pins::StringData,
            colors::PIN_STRING
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "matches",
            text("matches"),
            Output,
            pins::StringData,
            colors::PIN_STRING
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![
        node_title_bar("Filter", style),
        container(pin_list).padding([10, 12])
    ]
    .width(180.0)
    .into()
}
