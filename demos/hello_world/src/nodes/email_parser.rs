use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row},
};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{colors, node_title_bar};

/// Email Parser Node - Input + multiple outputs
pub fn email_parser_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = column![
        // Row 1: email input + subject output
        row![
            container(pin!(Left, "email", Input, "email", colors::PIN_EMAIL))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
            container(pin!(Right, "subject", Output, "string", colors::PIN_STRING))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
        ]
        .width(Length::Fill),
        // Row 2: datetime output
        container(pin!(
            Right,
            "datetime",
            Output,
            "datetime",
            colors::PIN_DATETIME
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Right),
        // Row 3: body output
        container(pin!(Right, "body", Output, "string", colors::PIN_STRING))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .spacing(4);

    column![
        node_title_bar("Email Parser", style),
        container(pin_list).padding([10, 12])
    ]
    .width(200.0)
    .into()
}
