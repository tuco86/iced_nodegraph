use iced::{
    widget::{column, container},
    Color,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

/// Email Trigger Node - Only outputs
pub fn email_trigger_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let pin_list = column![pin!(Right, "on email", Output, "email", Color::from_rgb(0.3, 0.7, 0.9)),]
        .spacing(2);

    column![
        node_title_bar("Email Trigger", style),
        container(pin_list).padding([6, 0])
    ]
    .width(160.0)
    .into()
}
