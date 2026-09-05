//! Builder Nodes
//!
//! Small combinator nodes that assemble primitive inputs into the richer values
//! the config nodes expect: a [`ColorQuad`](iced_nodegraph::ColorQuad) from four
//! corner colors, a 2D vector from two scalars, and a color with a chosen alpha
//! (the palette nodes emit opaque colors, so every translucent style field goes
//! through it). Their inputs are filled from incoming connections during value
//! propagation; the output pin carries the assembled value.

use demo_common::NodeContentStyle;
use iced::widget::{column, container, text};

use super::{AlphaNode, ColorQuadNode, Vec2Node};
use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use iced_nodegraph::pin;

/// Creates a ColorQuad builder node: four corner color inputs -> one quad output.
pub fn color_quad_node<'a, Message>(
    theme: &'a iced::Theme,
    state: &ColorQuadNode,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let quad = state.quad();

    // Output row: resolved quad swatch + typed output pin.
    let out_row = pin_row(
        color_swatch(Some(quad.near_start)),
        pin!(
            Right,
            pins::build::QUAD_OUT,
            text("quad").size(10),
            Output,
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
    );

    let corner = |label: &'static str, id: &'static str, color: Option<iced::Color>| {
        pin_row(
            pin!(
                Left,
                id,
                text(label).size(10),
                Input,
                ::std::any::TypeId::of::<pins::ColorData>()
            ),
            color_swatch(color),
        )
    };

    let content = column![
        out_row,
        corner("near start", pins::build::NEAR_START, state.near_start),
        corner("near end", pins::build::NEAR_END, state.near_end),
        corner("far start", pins::build::FAR_START, state.far_start),
        corner("far end", pins::build::FAR_END, state.far_end),
    ]
    .spacing(4);

    column![
        node_title_bar("Color Quad", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}

/// Creates a Vec2 builder node: two scalar inputs -> one vector output.
pub fn vec2_node<'a, Message>(
    theme: &'a iced::Theme,
    state: &Vec2Node,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let (x, y) = state.vec2();

    let out_row = pin_row(
        value_display(format!("{:.1}, {:.1}", x, y)),
        pin!(
            Right,
            pins::build::VEC2_OUT,
            text("vec2").size(10),
            Output,
            ::std::any::TypeId::of::<pins::Vec2Data>()
        ),
    );

    let component = |label: &'static str, id: &'static str, value: Option<f32>| {
        pin_row(
            pin!(
                Left,
                id,
                text(label).size(10),
                Input,
                ::std::any::TypeId::of::<pins::Float>()
            ),
            value_display(fmt_float(value, 1)),
        )
    };

    let content = column![
        out_row,
        component("x", pins::build::X, state.x),
        component("y", pins::build::Y, state.y),
    ]
    .spacing(4);

    column![
        node_title_bar("Vec2", style),
        container(content).padding([8, 10])
    ]
    .width(130.0)
    .into()
}

/// Creates an Alpha builder node: color + alpha inputs -> one color output.
pub fn alpha_node<'a, Message>(
    theme: &'a iced::Theme,
    state: &AlphaNode,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);

    let out_row = pin_row(
        color_swatch(Some(state.color())),
        pin!(
            Right,
            pins::build::ALPHA_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
    );

    let color_row = pin_row(
        pin!(
            Left,
            pins::build::ALPHA_COLOR,
            text("color").size(10),
            Input,
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
        color_swatch(state.color),
    );

    let alpha_row = pin_row(
        pin!(
            Left,
            pins::build::ALPHA,
            text("alpha").size(10),
            Input,
            ::std::any::TypeId::of::<pins::Float>()
        ),
        value_display(fmt_float(state.alpha, 2)),
    );

    let content = column![out_row, color_row, alpha_row].spacing(4);

    column![
        node_title_bar("Alpha", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
