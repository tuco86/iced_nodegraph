use iced::widget::{column, container};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar};

/// Email Trigger Node - Only outputs
pub fn email_trigger_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let pin_list = column![pin!(Right, "on email", Output, "email", colors::PIN_EMAIL),].spacing(4);

    column![
        node_title_bar("Email Trigger", style),
        container(pin_list).padding([10, 12])
    ]
    .width(180.0)
    .into()
}
