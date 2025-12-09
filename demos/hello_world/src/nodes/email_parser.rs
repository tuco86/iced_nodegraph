use iced::{
    widget::{column, container},
    Color,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

/// Email Parser Node - Input + multiple outputs
pub fn email_parser_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = column![
        pin!(Left, "email", Input, "email", Color::from_rgb(0.3, 0.7, 0.9)),
        pin!(Right, "subject", Output, "string", Color::from_rgb(0.9, 0.7, 0.3)),
        pin!(Right, "datetime", Output, "datetime", Color::from_rgb(0.7, 0.3, 0.9)),
        pin!(Right, "body", Output, "string", Color::from_rgb(0.9, 0.7, 0.3)),
    ]
    .spacing(2);

    column![
        node_title_bar("Email Parser", style),
        container(pin_list).padding([6, 0])
    ]
    .width(160.0)
    .into()
}
