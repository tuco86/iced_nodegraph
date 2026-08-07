use demo_common::{NodeContentStyle, node_title_bar};
use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{NodeStyle, pin};

/// Marker type for generic data pins
pub struct Data;

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
                    0usize,
                    text(""),
                    Input,
                    ::std::any::TypeId::of::<Data>()
                ))
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Left),
                container(pin!(
                    Right,
                    1usize,
                    text(""),
                    Output,
                    ::std::any::TypeId::of::<Data>()
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
