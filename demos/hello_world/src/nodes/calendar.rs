use iced::{
    widget::{column, container},
    Color,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

/// Calendar Node - Only inputs
pub fn calendar_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let pin_list = column![
        pin!(Left, "datetime", Input, "datetime", Color::from_rgb(0.7, 0.3, 0.9)),
        pin!(Left, "title", Input, "string", Color::from_rgb(0.9, 0.7, 0.3)),
        pin!(Left, "description", Input, "string", Color::from_rgb(0.9, 0.7, 0.3)),
    ]
    .spacing(2);

    column![
        node_title_bar("Create Event", style),
        container(pin_list).padding([6, 0])
    ]
    .width(160.0)
    .into()
}
