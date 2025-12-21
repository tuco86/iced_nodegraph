use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row},
};
use iced_nodegraph::{NodeContentStyle, node_title_bar, pin};

/// Email Parser Node - Input + multiple outputs
pub fn email_parser_node<'a, Message>(theme: &'a iced::Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let pin_list = column![
        // Row 1: email input + subject output
        row![
            container(pin!(
                Left,
                "email",
                Input,
                "email",
                Color::from_rgb(0.3, 0.7, 0.9)
            ))
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Left),
            container(pin!(
                Right,
                "subject",
                Output,
                "string",
                Color::from_rgb(0.9, 0.7, 0.3)
            ))
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
            Color::from_rgb(0.7, 0.3, 0.9)
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Right),
        // Row 3: body output
        container(pin!(
            Right,
            "body",
            Output,
            "string",
            Color::from_rgb(0.9, 0.7, 0.3)
        ))
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .spacing(2);

    column![
        node_title_bar("Email Parser", style),
        container(pin_list).padding([6, 0])
    ]
    .width(180.0)
    .into()
}
