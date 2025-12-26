//! Math Nodes
//!
//! Nodes that perform mathematical operations on float values.
//! Supports Add, Subtract, Multiply, and Divide operations.
//! Outputs can be chained to other Math nodes or Config nodes.

use iced::{
    Length,
    widget::{Space, column, container, row, text},
};
use iced_nodegraph::{NodeContentStyle, pin};

use super::{MathNodeState, colors, node_title_bar};

/// Creates a math operation node with horizontal pin pairing
pub fn math_node<'a, Message>(
    theme: &'a iced::Theme,
    state: &MathNodeState,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let border_width = style.border_width;

    // Format input values
    let a_display = state
        .input_a
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "?".to_string());
    let b_display = state
        .input_b
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "?".to_string());

    // Format result with operation symbol
    let result_display = state
        .result()
        .map(|v| {
            if v.is_infinite() {
                "INF".to_string()
            } else if v.is_nan() {
                "NaN".to_string()
            } else {
                format!("{:.1}", v)
            }
        })
        .unwrap_or_else(|| "?".to_string());

    let result_color = if state.result().is_some() {
        colors::PIN_NUMBER
    } else {
        colors::TEXT_MUTED
    };

    // Row 1: Input A + Result output (horizontal pin pairing)
    let result_text = text(format!("{} {}", state.operation.symbol(), result_display))
        .size(11)
        .color(result_color);

    let row_a = row![
        pin!(Left, "A", Input, "float", colors::PIN_NUMBER),
        text(a_display).size(10).color(colors::TEXT_MUTED),
        Space::new().width(Length::Fill),
        pin!(Right, result_text, Output, "float", colors::PIN_NUMBER),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    // Row 2: Input B only
    let row_b = row![
        pin!(Left, "B", Input, "float", colors::PIN_NUMBER),
        text(b_display).size(10).color(colors::TEXT_MUTED),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    // Build compact node
    let title = state.operation.name();

    column![
        node_title_bar(title, style),
        container(column![row_a, row_b].spacing(6)).padding([10, 12 + border_width as u16])
    ]
    .width(160.0)
    .into()
}
