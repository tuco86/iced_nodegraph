//! Enum Selector Input Nodes
//!
//! Pill-style selection for EdgeType and PinShape enums.
//! Industrial Precision design: compact pills, clear selection state.

use iced::{
    Border, Color, Length,
    alignment::Horizontal,
    widget::{button, column, container, row, text},
};
use iced_nodegraph::{EdgeType, NodeContentStyle, PinShape, pin};

use super::{colors, node_title_bar};

/// Creates a pill button for enum selection
fn pill_button<'a, T, Message>(
    label: &'a str,
    value: T,
    selected: T,
    on_click: Message,
    accent_color: Color,
) -> iced::Element<'a, Message>
where
    T: PartialEq + 'a,
    Message: Clone + 'a,
{
    let is_selected = value == selected;

    button(
        text(label)
            .size(10)
            .color(if is_selected { Color::BLACK } else { colors::TEXT_PRIMARY })
    )
    .padding([4, 8])
    .on_press(on_click)
    .style(move |_, status| {
        let (bg, border_color) = match status {
            button::Status::Active => {
                if is_selected {
                    (accent_color, accent_color)
                } else {
                    (Color::TRANSPARENT, colors::BORDER_SUBTLE)
                }
            }
            button::Status::Hovered => {
                if is_selected {
                    (accent_color, Color::WHITE)
                } else {
                    (colors::SURFACE_ELEVATED, accent_color)
                }
            }
            button::Status::Pressed => {
                (accent_color, accent_color)
            }
            button::Status::Disabled => {
                (colors::SURFACE_ELEVATED, colors::BORDER_SUBTLE)
            }
        };

        button::Style {
            background: Some(bg.into()),
            text_color: if is_selected { Color::BLACK } else { colors::TEXT_PRIMARY },
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    })
    .into()
}

/// Creates an EdgeType selector node with pill buttons
pub fn edge_type_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: EdgeType,
    on_change: impl Fn(EdgeType) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);
    let accent = colors::PIN_ANY;

    // Create pills directly for clean layout
    let on_change1 = on_change.clone();
    let on_change2 = on_change.clone();
    let on_change3 = on_change.clone();
    let on_change4 = on_change.clone();

    let row1 = row![
        pill_button("Bezier", EdgeType::Bezier, selected, on_change1(EdgeType::Bezier), accent),
        pill_button("Line", EdgeType::Straight, selected, on_change2(EdgeType::Straight), accent),
    ].spacing(4);

    let row2 = row![
        pill_button("Step", EdgeType::Step, selected, on_change3(EdgeType::Step), accent),
        pill_button("Smooth", EdgeType::SmoothStep, selected, on_change4(EdgeType::SmoothStep), accent),
    ].spacing(4);

    let output_pin = container(pin!(
        Right,
        text("value").size(10),
        Output,
        "edge_type",
        colors::PIN_ANY
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Edge Type", style),
        container(
            column![
                column![row1, row2].spacing(4),
                output_pin,
            ]
            .spacing(8)
        )
        .padding([10, 12])
    ]
    .width(160.0)
    .into()
}

/// Creates a PinShape selector node with pill buttons
pub fn pin_shape_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: PinShape,
    on_change: impl Fn(PinShape) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);
    let accent = colors::PIN_ANY;

    // Create pills directly for clean layout
    let on_change1 = on_change.clone();
    let on_change2 = on_change.clone();
    let on_change3 = on_change.clone();
    let on_change4 = on_change.clone();

    let row1 = row![
        pill_button("Circle", PinShape::Circle, selected, on_change1(PinShape::Circle), accent),
        pill_button("Square", PinShape::Square, selected, on_change2(PinShape::Square), accent),
    ].spacing(4);

    let row2 = row![
        pill_button("Diamond", PinShape::Diamond, selected, on_change3(PinShape::Diamond), accent),
        pill_button("Triangle", PinShape::Triangle, selected, on_change4(PinShape::Triangle), accent),
    ].spacing(4);

    let output_pin = container(pin!(
        Right,
        text("value").size(10),
        Output,
        "pin_shape",
        colors::PIN_ANY
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Pin Shape", style),
        container(
            column![
                column![row1, row2].spacing(4),
                output_pin,
            ]
            .spacing(8)
        )
        .padding([10, 12])
    ]
    .width(160.0)
    .into()
}
