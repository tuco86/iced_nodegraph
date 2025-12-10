use iced::{
    widget::{column, container, row},
    alignment::Horizontal,
    Color, Length,
};
use iced_nodegraph::{pin, node_title_bar, NodeContentStyle, NodeStyle};

/// Creates a styled node with input and output pins.
///
/// The node's visual appearance is determined by the `NodeStyle`, while
/// the title bar color is derived from the style's fill color.
pub fn styled_node<'a, Message>(
    name: &str,
    style: &NodeStyle,
    theme: &'a iced::Theme,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let content_style = determine_content_style(style, theme);

    column![
        node_title_bar(name.to_string(), content_style),
        container(
            row![
                container(pin!(Left, "input", Input, "data", Color::from_rgb(0.5, 0.7, 0.9)))
                    .width(Length::FillPortion(1))
                    .align_x(Horizontal::Left),
                container(pin!(Right, "output", Output, "data", Color::from_rgb(0.9, 0.7, 0.5)))
                    .width(Length::FillPortion(1))
                    .align_x(Horizontal::Right),
            ]
            .width(Length::Fill)
        )
        .padding([8, 10]),
    ]
    .width(160.0)
    .into()
}

/// Determines the content style based on the node's fill color.
fn determine_content_style(style: &NodeStyle, theme: &iced::Theme) -> NodeContentStyle {
    if style.fill_color.b > style.fill_color.r && style.fill_color.b > style.fill_color.g {
        NodeContentStyle::input(theme)
    } else if style.fill_color.g > style.fill_color.r && style.fill_color.g > style.fill_color.b {
        NodeContentStyle::process(theme)
    } else if style.fill_color.r > style.fill_color.g {
        NodeContentStyle::output(theme)
    } else {
        NodeContentStyle::comment(theme)
    }
}
