//! Widget implementation for NodeGraph.
//!
//! This module implements the Iced `Widget` trait for [`NodeGraph`], handling:
//! - Layout computation for nodes and their content
//! - Event processing (mouse, keyboard)
//! - SDF-based rendering via iced_nodegraph_sdf primitives
//!
//! ## Rendering Layers
//!
//! The widget renders in three tiers for correct z-ordering:
//! 1. Solid background color.
//! 2. Graph background: ONE batched SDF draw under all nodes, internally
//!    ordered grid (z0), node + edge shadows (z1), edge strokes (z2).
//! 3. Per node, composited by Iced in z-order: node background (fill) -> node
//!    content (Iced widgets) -> node foreground (border + pins). Embedding Iced
//!    widgets between the two SDF node layers lets nodes overlap correctly.
//! 4. Graph foreground: interaction tools (selection box, edge-cutting overlay).

use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector, keyboard};
use iced_wgpu::core::{
    Clipboard, Layout, Shell, layout, mouse, overlay, renderer,
    widget::{self, Tree, tree},
};
use web_time::Instant;

use super::{
    Counts, DragInfo, GraphInfo, NodeGraph, OpTiming, RenderContext,
    euclid::{IntoIced, WorldVector},
    state::{CameraTween, Dragging, NodeGraphState, z_render_indices},
};
use super::{EdgeStyleFn, NodeStyleFn, PinStyleFn};
use crate::{
    PinDirection, PinRef, PinSide,
    ids::{EdgeId, NodeId, PinId},
    node_graph::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::{NodePinState, PinEnd, PinInfo},
    style::{
        EdgeGeometry, EdgeStatus, EdgeStyle, GraphStyle, NodeStatus, NodeStyle, PinStatus,
        PinStyle, TilingKind,
    },
};
use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style, Tiling};

mod camera_overlay;
mod draw;
mod update;

use camera_overlay::CameraOverlay;

/// Length of bezier control point segments (in world-space pixels).
/// Controls how far control points extend from pins along their tangent direction.
const BEZIER_SEGMENT_LENGTH: f32 = 80.0;

/// Adaptively pick the control-point length for an edge so the bezier never
/// overshoots the other endpoint. With a fixed 80px length, two pins placed
/// 20px apart would have control points 80px past each other, curling the
/// curve into a tight loop that the SDF cannot resolve cleanly and the cull
/// drops along the inner side. Clamp to ≈half the endpoint distance.
fn adaptive_bezier_length(start: [f32; 2], end: [f32; 2]) -> f32 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let d = (dx * dx + dy * dy).sqrt();
    BEZIER_SEGMENT_LENGTH.min(d * 0.5).max(1.0)
}

/// Returns the tangent direction vector for a pin side in the shader's `u32`
/// side encoding (matches `get_pin_direction` in the WGSL).
/// Left=(-1,0), Right=(1,0), Top=(0,-1), Bottom=(0,1); anything else (Row,
/// synthetic mirror sides) defaults to (1,0).
fn pin_side_direction(side: u32) -> [f32; 2] {
    match side {
        0 => [-1.0, 0.0], // Left
        1 => [1.0, 0.0],  // Right
        2 => [0.0, -1.0], // Top
        3 => [0.0, 1.0],  // Bottom
        _ => [1.0, 0.0],  // Default (Row)
    }
}

impl<N, P, E, UI, Message, Renderer> iced_wgpu::core::Widget<Message, iced::Theme, Renderer>
    for NodeGraph<'_, N, P, UI, Message, iced::Theme, Renderer, E>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<NodeGraphState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(NodeGraphState::default())
    }

    fn size(&self) -> Size<Length> {
        self.size
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(self.size.width).height(self.size.height);
        let size = limits.resolve(self.size.width, self.size.height, Size::ZERO);
        // Use loose limits for nodes so they can shrink-to-fit their content
        // This prevents Length::Fill children from expanding to full graph size
        let node_limits = layout::Limits::new(Size::ZERO, Size::INFINITE);
        let nodes = self
            .elements_iter_mut()
            .zip(&mut tree.children)
            .map(|((position, element), node_tree)| {
                element
                    .as_widget_mut()
                    .layout(node_tree, renderer, &node_limits)
                    .move_to(position)
            })
            .collect();
        layout::Node::with_children(size, nodes)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.draw_impl(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn size_hint(&self) -> Size<Length> {
        self.size()
    }

    fn children(&self) -> Vec<Tree> {
        self.elements_iter()
            .map(|(_, element)| Tree::new(element))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let children: Vec<&Element<'_, Message, iced::Theme, Renderer>> =
            self.elements_iter().map(|(_, e)| e).collect();
        tree.diff_children(&children);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for (((_, element), node_tree), node_layout) in self
            .elements_iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            element
                .as_widget_mut()
                .operate(node_tree, node_layout, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, Renderer>> {
        // Iced collects pop-out widgets (combo box menus, tooltips, vanilla
        // `menu`) only through `Widget::overlay`. Without forwarding it to the
        // node elements, their underlying widgets draw fine but the pop-out
        // never appears. Mirror the camera the draw/update paths use so the
        // pop-out anchors and scales with the node content beneath it.
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let camera = state
            .camera
            .with_viewport_origin(layout.bounds().position().into_euclid().to_vector());

        // Collect each node's overlay (most yield None). Child layouts are in
        // the widget's layout-absolute space; `CameraOverlay` applies the
        // world->screen transform, so the child anchors in that space (zero
        // extra translation) just as it does during draw.
        let children: Vec<overlay::Element<'b, Message, iced::Theme, Renderer>> = self
            .nodes
            .iter_mut()
            .map(|(_, _, element, _, _)| element)
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter_map(|((element, node_tree), node_layout)| {
                element.as_widget_mut().overlay(
                    node_tree,
                    node_layout,
                    renderer,
                    viewport,
                    Vector::ZERO,
                )
            })
            .collect();

        if children.is_empty() {
            return None;
        }

        let content = overlay::Group::with_children(children).overlay();
        Some(overlay::Element::new(Box::new(CameraOverlay {
            content,
            camera,
        })))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        screen_cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.update_impl(
            tree,
            event,
            layout,
            screen_cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

impl<'a, N, P, E, UI, Message, Renderer>
    From<NodeGraph<'a, N, P, UI, Message, iced::Theme, Renderer, E>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + 'a + iced_wgpu::primitive::Renderer,
    Message: 'static,
{
    fn from(graph: NodeGraph<'a, N, P, UI, Message, iced::Theme, Renderer, E>) -> Self {
        Element::new(graph)
    }
}

/// Creates a new NodeGraph with default usize-based IDs and no pin user info.
///
/// For custom types, use
/// `NodeGraph::<N, P, UI, Message, Theme, Renderer, E>::default()`.
pub fn node_graph<'a, Message, Theme, Renderer>()
-> NodeGraph<'a, usize, usize, (), Message, Theme, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    NodeGraph::default()
}

/// Helper function to find all NodePin elements in the tree of a Node.
/// Returns: Vec of (pin_index, &NodePinState, (Point, Point) positions).
/// Generic over `P` and `UI`; within one graph all pins share the same `P` and
/// `UI`, so the tag match resolves a single concrete `NodePinState<P, UI>`.
fn find_pins<'a, P: 'static, UI: 'static>(
    tree: &'a Tree,
    layout: Layout<'a>,
) -> Vec<(usize, &'a NodePinState<P, UI>, (Point, Point))> {
    let mut flat = Vec::new();
    let mut pin_index = 0;
    inner_find_pins::<P, UI>(&mut flat, &mut pin_index, layout, tree);
    flat
}

fn inner_find_pins<'a, P: 'static, UI: 'static>(
    flat: &mut Vec<(usize, &'a NodePinState<P, UI>, (Point, Point))>,
    pin_index: &mut usize,
    node_layout: Layout<'a>,
    pin_tree: &'a Tree,
) {
    if pin_tree.tag == tree::Tag::of::<NodePinState<P, UI>>() {
        let pin_state = pin_tree.state.downcast_ref::<NodePinState<P, UI>>();
        let node_bounds = node_layout.bounds();
        let pin_positions = pin_positions(pin_state, node_bounds);
        flat.push((*pin_index, pin_state, pin_positions));
        *pin_index += 1;
    }

    for child_tree in &pin_tree.children {
        inner_find_pins::<P, UI>(flat, pin_index, node_layout, child_tree);
    }
}

/// Orients a connected pair so the OUTPUT pin is `from` (output -> input),
/// independent of which side the drag started on. Mirrors the edge-rendering
/// normalization (`swap` in `draw`), so the endpoints reported to
/// `on_connect`/`on_disconnect` match the visual data-flow direction. Order is
/// only swapped when `from` is a non-output and `to` is an output.
fn orient_connection<N, P>(
    from_dir: PinDirection,
    to_dir: PinDirection,
    from: PinRef<N, P>,
    to: PinRef<N, P>,
) -> (PinRef<N, P>, PinRef<N, P>) {
    let swap = !matches!(from_dir, PinDirection::Output) && matches!(to_dir, PinDirection::Output);
    if swap { (to, from) } else { (from, to) }
}

fn pin_positions<P, UI>(state: &NodePinState<P, UI>, node_bounds: Rectangle) -> (Point, Point) {
    if state.side == PinSide::Row {
        (
            pin_position(state.position, PinSide::Left, node_bounds),
            pin_position(state.position, PinSide::Right, node_bounds),
        )
    } else {
        let position = pin_position(state.position, state.side, node_bounds);
        (position, position)
    }
}

fn pin_position(position: Point, side: PinSide, node_bounds: Rectangle) -> Point {
    match side {
        PinSide::Row => panic!("Row pin is supposed to be handled separately"),
        PinSide::Left => Point::new(node_bounds.x, position.y),
        PinSide::Right => Point::new(node_bounds.x + node_bounds.width, position.y),
        PinSide::Top => Point::new(position.x, node_bounds.y),
        PinSide::Bottom => Point::new(position.x, node_bounds.y + node_bounds.height),
    }
}

#[cfg(test)]
mod orient_tests {
    use super::orient_connection;
    use crate::PinRef;
    use crate::node_pin::PinDirection;

    // A drag from an output pin to an input pin keeps (output, input) order.
    #[test]
    fn output_to_input_keeps_order() {
        let out = PinRef::new(0usize, 0usize);
        let inp = PinRef::new(1usize, 0usize);
        let (from, to) = orient_connection(PinDirection::Output, PinDirection::Input, out, inp);
        assert_eq!(from, PinRef::new(0, 0));
        assert_eq!(to, PinRef::new(1, 0));
    }

    // A drag from an input pin to an output pin is flipped to (output, input),
    // so on_connect reports the same pair regardless of drag direction.
    #[test]
    fn input_to_output_is_flipped() {
        let inp = PinRef::new(1usize, 0usize);
        let out = PinRef::new(0usize, 0usize);
        let (from, to) = orient_connection(PinDirection::Input, PinDirection::Output, inp, out);
        assert_eq!(from, PinRef::new(0, 0));
        assert_eq!(to, PinRef::new(1, 0));
    }

    // Ambiguous pairs (Both) are left in drag order; only a non-output -> output
    // pair is swapped.
    #[test]
    fn both_keeps_drag_order() {
        let a = PinRef::new(0usize, 0usize);
        let b = PinRef::new(1usize, 0usize);
        let (from, to) = orient_connection(PinDirection::Both, PinDirection::Both, a, b);
        assert_eq!(from, PinRef::new(0, 0));
        assert_eq!(to, PinRef::new(1, 0));
    }
}
