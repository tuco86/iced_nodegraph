use iced::widget::{column, container};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

use super::colors;

/// Calendar Node - Only inputs
pub fn calendar_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let pin_list = column![
        pin!(
            Left,
            "datetime",
            Input,
            "datetime",
            colors::PIN_DATETIME
        ),
        pin!(
            Left,
            "title",
            Input,
            "string",
            colors::PIN_STRING
        ),
        pin!(
            Left,
            "description",
            Input,
            "string",
            colors::PIN_STRING
        ),
    ]
    .spacing(4);

    column![
        node_title_bar("Create Event", style),
        container(pin_list).padding([10, 12])
    ]
    .width(180.0)
    .into()
}
