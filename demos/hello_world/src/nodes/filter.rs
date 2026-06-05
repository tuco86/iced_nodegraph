use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, NodeStatus, default_node_style, pin};

use super::{node_title_bar, pins};

/// Filter Node - Input + output
///
/// Uses the default NodeStyle geometry to ensure the title bar corners
/// match the actual node's rounded corners.
pub fn filter_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    // Use the theme-resolved NodeStyle for precise corner calculation
    let base = default_node_style(theme, NodeStatus::Idle);
    let border_width = base.border_pattern.thickness;
    let style = NodeContentStyle::process(theme).with_geometry(base.corner_radius, border_width);

    let pin_list = row![
        container(pin!(
            Left,
            "input",
            text("input"),
            Input,
            ::std::any::TypeId::of::<pins::StringData>()
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "matches",
            text("matches"),
            Output,
            ::std::any::TypeId::of::<pins::StringData>()
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
