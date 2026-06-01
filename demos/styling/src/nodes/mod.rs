use iced::{
    Length, Padding,
    alignment::Horizontal,
    widget::{Container, column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, NodeStyle, Resolved, node_header, pin};

/// Marker type for generic data pins
pub struct Data;

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
    )
}

/// Creates a styled node with input and output pins.
///
/// The node's visual appearance is determined by the `NodeStyle`, while
/// the title bar color is derived from the style's fill color.
pub fn styled_node<'a, Message>(
    name: &str,
    style: &NodeStyle<Resolved>,
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
                container(pin!(Left, 0usize, text(""), Input, Data))
                    .width(Length::FillPortion(1))
                    .align_x(Horizontal::Left),
                container(pin!(Right, 1usize, text(""), Output, Data))
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
fn determine_content_style(style: &NodeStyle<Resolved>, theme: &iced::Theme) -> NodeContentStyle {
    // Pick the content preset from the body's representative (near-start) fill.
    let fill = style.fill_color.near_start;
    let base = if fill.b > fill.r && fill.b > fill.g {
        NodeContentStyle::input(theme)
    } else if fill.g > fill.r && fill.g > fill.b {
        NodeContentStyle::process(theme)
    } else if fill.r > fill.g {
        NodeContentStyle::output(theme)
    } else {
        NodeContentStyle::comment(theme)
    };
    // Apply the actual node geometry for correct title bar corners
    base.with_geometry(style.corner_radius, style.border_pattern.thickness)
}
