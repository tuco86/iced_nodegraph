use iced::{
    Color, Length, Padding,
    alignment::Horizontal,
    widget::{Container, column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, NodeStyle, node_header, pin};

/// Creates a themed title bar container for nodes.
fn node_title_bar<'a, Message>(
    title: impl Into<String>,
    style: NodeContentStyle,
) -> Container<'a, Message, iced::Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    let title_text = text(title.into()).size(13).color(style.title_text);

    node_header(
        container(title_text).padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 8.0,
            right: 8.0,
        }),
        style.title_background,
        style.corner_radius,
        style.border_width,
    )
}

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
                container(pin!(
                    Left,
                    "input",
                    Input,
                    "data",
                    Color::from_rgb(0.5, 0.7, 0.9)
                ))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
                container(pin!(
                    Right,
                    "output",
                    Output,
                    "data",
                    Color::from_rgb(0.9, 0.7, 0.5)
                ))
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
/// Uses the node's actual corner_radius and border_width for proper geometry.
fn determine_content_style(style: &NodeStyle, theme: &iced::Theme) -> NodeContentStyle {
    let base = if style.fill_color.b > style.fill_color.r && style.fill_color.b > style.fill_color.g {
        NodeContentStyle::input(theme)
    } else if style.fill_color.g > style.fill_color.r && style.fill_color.g > style.fill_color.b {
        NodeContentStyle::process(theme)
    } else if style.fill_color.r > style.fill_color.g {
        NodeContentStyle::output(theme)
    } else {
        NodeContentStyle::comment(theme)
    };
    // Apply the actual node geometry for correct title bar corners
    base.with_geometry(style.corner_radius, style.border_width)
}
