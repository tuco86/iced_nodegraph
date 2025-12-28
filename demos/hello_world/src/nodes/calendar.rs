use iced::widget::{column, container, text};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar, pins};

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
            text("datetime"),
            Input,
            pins::DateTime,
            colors::PIN_DATETIME
        ),
        pin!(
            Left,
            "title",
            text("title"),
            Input,
            pins::StringData,
            colors::PIN_STRING
        ),
        pin!(
            Left,
            "description",
            text("description"),
            Input,
            pins::StringData,
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
