use iced::{
    widget::{column, container},
    Color,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

/// Filter Node - Input + output
pub fn filter_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = column![
        pin!(Left, "input", Input, "string", Color::from_rgb(0.9, 0.7, 0.3)),
        pin!(Right, "matches", Output, "string", Color::from_rgb(0.9, 0.7, 0.3)),
    ]
    .spacing(2);

    column![
        node_title_bar("Filter", style),
        container(pin_list).padding([6, 0])
    ]
    .width(140.0)
    .into()
}
