//! Enum Selector Input Nodes
//!
//! Pill-style selection for EdgeCurve, PinShape, and PatternType enums.
//! Industrial Precision design: compact pills, clear selection state.

use iced::{
    Border, Color, Length,
    alignment::Horizontal,
    widget::{button, column, container, row, text},
};
use iced_nodegraph::{EdgeCurve, NodeContentStyle, PinShape, pin};

use super::{PatternType, colors, node_title_bar, pins};

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

    button(text(label).size(10).color(if is_selected {
        Color::BLACK
    } else {
        colors::TEXT_PRIMARY
    }))
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
            button::Status::Pressed => (accent_color, accent_color),
            button::Status::Disabled => (colors::SURFACE_ELEVATED, colors::BORDER_SUBTLE),
        };

        button::Style {
            background: Some(bg.into()),
            text_color: if is_selected {
                Color::BLACK
            } else {
                colors::TEXT_PRIMARY
            },
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

/// Creates an EdgeCurve selector node with pill buttons
pub fn edge_curve_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: EdgeCurve,
    on_change: impl Fn(EdgeCurve) -> Message + Clone + 'a,
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
        pill_button(
            "Bezier",
            EdgeCurve::BezierCubic,
            selected,
            on_change1(EdgeCurve::BezierCubic),
            accent
        ),
        pill_button(
            "Line",
            EdgeCurve::Line,
            selected,
            on_change2(EdgeCurve::Line),
            accent
        ),
    ]
    .spacing(4);

    let row2 = row![
        pill_button(
            "Step",
            EdgeCurve::Orthogonal,
            selected,
            on_change3(EdgeCurve::Orthogonal),
            accent
        ),
        pill_button(
            "Smooth",
            EdgeCurve::OrthogonalSmooth { radius: 15.0 },
            selected,
            on_change4(EdgeCurve::OrthogonalSmooth { radius: 15.0 }),
            accent
        ),
    ]
    .spacing(4);

    let output_pin = container(pin!(
        Right,
        "value",
        text("value").size(10),
        Output,
        pins::EdgeCurveData,
        colors::PIN_ANY
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Edge Curve", style),
        container(column![column![row1, row2].spacing(4), output_pin,].spacing(8))
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
        pill_button(
            "Circle",
            PinShape::Circle,
            selected,
            on_change1(PinShape::Circle),
            accent
        ),
        pill_button(
            "Square",
            PinShape::Square,
            selected,
            on_change2(PinShape::Square),
            accent
        ),
    ]
    .spacing(4);

    let row2 = row![
        pill_button(
            "Diamond",
            PinShape::Diamond,
            selected,
            on_change3(PinShape::Diamond),
            accent
        ),
        pill_button(
            "Triangle",
            PinShape::Triangle,
            selected,
            on_change4(PinShape::Triangle),
            accent
        ),
    ]
    .spacing(4);

    let output_pin = container(pin!(
        Right,
        "value",
        text("value").size(10),
        Output,
        pins::PinShapeData,
        colors::PIN_ANY
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Pin Shape", style),
        container(column![column![row1, row2].spacing(4), output_pin,].spacing(8))
            .padding([10, 12])
    ]
    .width(160.0)
    .into()
}

/// Creates a PatternType selector node with pill buttons
pub fn pattern_type_selector_node<'a, Message>(
    theme: &'a iced::Theme,
    selected: PatternType,
    on_change: impl Fn(PatternType) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::input(theme);
    let accent = colors::PIN_ANY;

    // Create pills for pattern types
    let on_change1 = on_change.clone();
    let on_change2 = on_change.clone();
    let on_change3 = on_change.clone();
    let on_change4 = on_change.clone();
    let on_change5 = on_change.clone();
    let on_change6 = on_change.clone();

    // First row: Solid, Dashed
    let pills_row1 = row![
        pill_button(
            "Solid",
            PatternType::Solid,
            selected,
            on_change1(PatternType::Solid),
            accent
        ),
        pill_button(
            "Dashed",
            PatternType::Dashed,
            selected,
            on_change2(PatternType::Dashed),
            accent
        ),
    ]
    .spacing(4);

    // Second row: Arrowed (///), Angled (dashed with angled caps)
    let pills_row2 = row![
        pill_button(
            "Arrowed",
            PatternType::Arrowed,
            selected,
            on_change3(PatternType::Arrowed),
            accent
        ),
        pill_button(
            "Angled",
            PatternType::Angled,
            selected,
            on_change4(PatternType::Angled),
            accent
        ),
    ]
    .spacing(4);

    // Third row: Dotted, DashDot
    let pills_row3 = row![
        pill_button(
            "Dotted",
            PatternType::Dotted,
            selected,
            on_change5(PatternType::Dotted),
            accent
        ),
        pill_button(
            "Dash·Dot",
            PatternType::DashDotted,
            selected,
            on_change6(PatternType::DashDotted),
            accent
        ),
    ]
    .spacing(4);

    let output_pin = container(pin!(
        Right,
        "value",
        text("value").size(10),
        Output,
        pins::PatternTypeData,
        colors::PIN_ANY
    ))
    .width(Length::Fill)
    .align_x(Horizontal::Right);

    column![
        node_title_bar("Pattern", style),
        container(column![pills_row1, pills_row2, pills_row3, output_pin,].spacing(6))
            .padding([10, 12])
    ]
    .width(200.0)
    .into()
}
