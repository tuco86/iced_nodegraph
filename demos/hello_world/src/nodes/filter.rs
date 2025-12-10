use iced::{
    widget::{column, container, row},
    alignment::Horizontal,
    Color, Length,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle};

/// Filter Node - Input + output
pub fn filter_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = row![
        container(pin!(Left, "input", Input, "string", Color::from_rgb(0.9, 0.7, 0.3)))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
        container(pin!(Right, "matches", Output, "string", Color::from_rgb(0.9, 0.7, 0.3)))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right),
    ]
    .width(Length::Fill);

    column![
        node_title_bar("Filter", style),
        container(pin_list).padding([6, 0])
    ]
    .width(160.0)
    .into()
}
