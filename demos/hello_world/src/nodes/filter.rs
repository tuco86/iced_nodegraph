use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Filter Node - Input + output
pub fn filter_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = row![
        container(pin!(
            Left,
            "input",
            Input,
            "string",
            colors::PIN_STRING
        ))
        .width(Length::FillPortion(1))
        .align_x(Horizontal::Left),
        container(pin!(
            Right,
            "matches",
            Output,
            "string",
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
