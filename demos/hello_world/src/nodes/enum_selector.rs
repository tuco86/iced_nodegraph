//! Enum Selector Input Nodes
//!
//! Radio-button selection for EdgeType and PinShape enums.

use iced::{
    Length,
    alignment::Horizontal,
    widget::{column, container, radio, text},
};
use iced_nodegraph::{EdgeType, NodeContentStyle, PinShape, node_title_bar, pin};

use super::colors;

/// Creates an EdgeType selector node
pub fn edge_type_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: EdgeType,
    on_change: impl Fn(EdgeType) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let edge_types = [
        (EdgeType::Bezier, "Bezier"),
        (EdgeType::Straight, "Straight"),
        (EdgeType::Step, "Step"),
        (EdgeType::SmoothStep, "Smooth Step"),
    ];

    let radios: Vec<iced::Element<'a, Message>> = edge_types
        .iter()
        .map(|(edge_type, label)| {
            let on_change = on_change.clone();
            let et = *edge_type;
            radio(*label, et, Some(selected), move |_| on_change(et))
                .size(14)
                .text_size(10)
                .into()
        })
        .collect();

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
                column(radios).spacing(4),
                output_pin,
            ]
            .spacing(8)
        )
        .padding([10, 12])
    ]
    .width(160.0)
    .into()
}

/// Creates a PinShape selector node
pub fn pin_shape_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: PinShape,
    on_change: impl Fn(PinShape) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);

    let pin_shapes = [
        (PinShape::Circle, "Circle"),
        (PinShape::Square, "Square"),
        (PinShape::Diamond, "Diamond"),
        (PinShape::Triangle, "Triangle"),
    ];

    let radios: Vec<iced::Element<'a, Message>> = pin_shapes
        .iter()
        .map(|(shape, label)| {
            let on_change = on_change.clone();
            let s = *shape;
            radio(*label, s, Some(selected), move |_| on_change(s))
                .size(14)
                .text_size(10)
                .into()
        })
        .collect();

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
                column(radios).spacing(4),
                output_pin,
            ]
            .spacing(8)
        )
        .padding([10, 12])
    ]
    .width(160.0)
    .into()
}
