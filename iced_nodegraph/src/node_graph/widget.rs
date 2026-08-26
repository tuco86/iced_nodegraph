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
//!
//! ## Interface contract
//!
//! What each `Widget` method forwards to the node elements, and in which
//! space. Every forward maps the cursor and viewport through
//! [`Camera2D`](super::camera::Camera2D) into the widget's layout-absolute
//! space, because that is the space the child layouts were produced in.
//!
//! - `layout` measures every node with loose limits and positions it at the
//!   host's world coordinate, so the layout tree is layout-absolute and the
//!   camera applies at draw and input time only.
//! - `update` and `operate` reach every node, offscreen ones included. An
//!   unfocus-on-outside-click, a child drag still in flight, and a focus
//!   traversal or scroll_to target all live on nodes the viewport does not
//!   cover.
//! - `draw` bounds-culls only the SDF layers it builds itself. Node content is
//!   walked for every node: the per-node pre-passes must run regardless, since
//!   edges anchor to pins of offscreen nodes and the background batch has to
//!   stay entry-stable, and an offscreen node's content records into a
//!   zero-area clip. Culling the child walk would buy little and make what is
//!   walked diverge from what is drawn.
//! - `mouse_interaction` recurses into the topmost node under the cursor only.
//!   One cursor wins, so an occluded node must not claim it.
//! - `overlay` collects each node's pop-out and wraps the group in
//!   `CameraOverlay`, which applies the same transform as the node content
//!   beneath it.

use iced_wgpu::core::{
    Clipboard, Layout, Shell, layout, mouse, overlay, renderer,
    widget::{self, Tree, tree},
};
use iced_widget::core::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector, keyboard};
use web_time::Instant;

use super::{
    ANCHOR_GRAB_THRESHOLD, CableGeometry, Counts, DragInfo, EDGE_END_GRAB_LENGTH,
    EDGE_GRAB_THRESHOLD, Edge, GraphInfo, MIN_NODE_SIZE, NodeGraph, OpTiming, PendingRoute,
    PhantomKind, RESIZE_GRIP_SIDE, RenderContext, RoutePhantom, Station,
    euclid::{IntoIced, LayoutVector},
    state::{CameraTween, Dragging, NodeGraphState, PressTarget, z_render_indices},
};
use super::{AnchorStyleFn, EdgeStyleFn, NodeStyleFn, PinStyleFn};
use crate::{
    PinDirection, PinRef, PinSide,
    ids::{AnchorId, EdgeId, NodeId, PinId},
    node_graph::euclid::{IntoEuclid, LayoutPoint, ScreenPoint, WorldPoint},
    node_pin::{NodePinState, PinEnd, PinInfo},
    style::{
        AnchorStatus, AnchorStyle, EdgeGeometry, EdgeStatus, EdgeStyle, GraphStyle, NodeStatus,
        NodeStyle, PinStatus, PinStyle, TilingKind, default_anchor_style,
        default_cutting_tool_style, default_selection_box_style,
    },
};
use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style, Tiling};

mod camera_overlay;
mod draw;
pub(super) mod edge_path;
pub(crate) mod update;

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

impl<N, P, E, A, UI, Message, Renderer> iced_wgpu::core::Widget<Message, Theme, Renderer>
    for NodeGraph<'_, N, P, E, A, UI, Message, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    A: AnchorId + 'static,
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
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
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
        let children: Vec<&Element<'_, Message, Theme, Renderer>> =
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
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        // Iced collects pop-out widgets (combo box menus, tooltips, vanilla
        // `menu`) only through `Widget::overlay`. Without forwarding it to the
        // node elements, their underlying widgets draw fine but the pop-out
        // never appears. Mirror the camera the draw/update paths use so the
        // pop-out anchors and scales with the node content beneath it.
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let camera = state.camera_for(layout);

        // Collect each node's overlay (most yield None). Child layouts are in
        // the widget's layout-absolute space; `CameraOverlay` applies the
        // world->screen transform, so the child anchors in that space (zero
        // extra translation) just as it does during draw.
        let children: Vec<overlay::Element<'b, Message, Theme, Renderer>> = self
            .nodes
            .iter_mut()
            .map(|node| &mut node.element)
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

    /// The cursor the graph claims, in precedence order: an in-flight gesture,
    /// then a node's resize grip or content, then an anchor core or cable below
    /// the nodes.
    ///
    /// Recursion is gated on node bounds instead of forwarded to every child:
    /// only one cursor can win, so an occluded node must not claim it. The
    /// topmost node under the cursor consumes the query even when its subtree
    /// reports [`mouse::Interaction::None`], which is what keeps a node body
    /// from showing a cursor set by something behind it.
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<NodeGraphState>();
        // A gesture in flight owns the cursor wherever it has been dragged to,
        // grip or node bounds left behind: the drag is still going.
        match &state.dragging {
            Dragging::Resize { .. } => return mouse::Interaction::ResizingDiagonallyDown,
            Dragging::Graph(_)
            | Dragging::Node { .. }
            | Dragging::GroupMove(_)
            | Dragging::Anchor { .. }
            | Dragging::Route { .. }
            | Dragging::RouteOver { .. }
            | Dragging::PressPending { .. } => return mouse::Interaction::Grabbing,
            Dragging::Edge { .. }
            | Dragging::EdgeOver { .. }
            | Dragging::EdgeCutting { .. }
            | Dragging::SelectionBox(..) => return mouse::Interaction::Crosshair,
            Dragging::None => {}
        }
        // Outside the graph - or levitating, because a sibling in a `stack`
        // claimed the event - nothing here may claim the cursor.
        if cursor.position_over(layout.bounds()).is_none() {
            return mouse::Interaction::None;
        }
        let camera = state.camera_for(layout);
        // The spaces the child `update` walk uses: cursor and viewport
        // camera-inverted into layout-absolute space, the viewport first
        // clipped to the graph as `draw` clips it.
        let clipped_viewport = layout
            .bounds()
            .intersection(viewport)
            .unwrap_or(Rectangle::new(layout.bounds().position(), Size::ZERO));
        camera.update_with(
            &clipped_viewport,
            cursor,
            |child_viewport, layout_cursor| {
                let Some(position) = layout_cursor.position() else {
                    return mouse::Interaction::None;
                };
                let selection = self.resolved_selection(state);
                let z_indices =
                    z_render_indices(state, self.nodes.len(), |i| selection.contains(&i));
                // Top-first, like the press hit-test: a node covering another
                // node's corner takes the cursor with it.
                for &node_index in z_indices.iter().rev() {
                    let Some(node) = self.nodes.get(node_index) else {
                        continue;
                    };
                    let Some(child_tree) = tree.children.get(node_index) else {
                        continue;
                    };
                    let Some(node_layout) = layout.children().nth(node_index) else {
                        continue;
                    };
                    if !node_layout.bounds().contains(position) {
                        continue;
                    }
                    // Grips live in layout-absolute space, the space `update`
                    // hit-tests in, and sit above the node's own content.
                    if self.on_resize.is_some()
                        && node.resizable
                        && resize_grip_zone(node_layout.bounds(), camera.zoom()).contains(position)
                    {
                        return mouse::Interaction::ResizingDiagonallyDown;
                    }
                    return node.element.as_widget().mouse_interaction(
                        child_tree,
                        node_layout,
                        layout_cursor,
                        child_viewport,
                        renderer,
                    );
                }
                let at: LayoutPoint = position.into_euclid();
                if self.anchor_core_at(tree, at).is_some()
                    || self.cable_hit_at(tree, layout, at).is_some()
                {
                    mouse::Interaction::Grab
                } else {
                    mouse::Interaction::None
                }
            },
        )
    }
}

impl<'a, N, P, E, A, UI, Message, Renderer> From<NodeGraph<'a, N, P, E, A, UI, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    A: AnchorId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + 'a + iced_wgpu::primitive::Renderer,
    Message: 'static,
{
    fn from(graph: NodeGraph<'a, N, P, E, A, UI, Message, Renderer>) -> Self {
        Element::new(graph)
    }
}

/// Creates a new NodeGraph with default usize-based IDs and no pin user info.
///
/// For custom types, use
/// `NodeGraph::<N, P, E, A, UI, Message, Renderer>::default()`.
pub fn node_graph<'a, Message, Renderer>()
-> NodeGraph<'a, usize, usize, (), usize, (), Message, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    NodeGraph::default()
}

/// A pin as found in the laid-out widget tree: its positional index within the
/// owning node, its state, and the `(start, end)` anchors an edge attaches to.
///
/// The index is the pin's position in `find_pins` walk order, which is the
/// `pin_index` the [`Dragging`] states carry.
pub(super) type PinLayout<'a, P, UI> = (usize, &'a NodePinState<P, UI>, (Point, Point));

/// Every pin in a node's subtree, in depth-first layout order.
///
/// Within one graph all pins share the same `P` and `UI`, so the `tree::Tag`
/// match resolves a single concrete `NodePinState<P, UI>`.
fn find_pins<'a, P: 'static, UI: 'static>(
    tree: &'a Tree,
    layout: Layout<'a>,
) -> Vec<PinLayout<'a, P, UI>> {
    let mut flat = Vec::new();
    let mut pin_index = 0;
    inner_find_pins::<P, UI>(&mut flat, &mut pin_index, layout, tree);
    flat
}

fn inner_find_pins<'a, P: 'static, UI: 'static>(
    flat: &mut Vec<PinLayout<'a, P, UI>>,
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

/// Where an edge attaches to a pin, as (start, end) anchors on the node border.
///
/// Both anchors coincide for a pin on one side; a [`PinSide::Row`] pin spans the
/// node and offers a left and a right anchor, so an edge can arrive on either
/// side of it.
fn pin_positions<P, UI>(state: &NodePinState<P, UI>, node_bounds: Rectangle) -> (Point, Point) {
    let Point { x, y } = state.position;
    let (left, right) = (node_bounds.x, node_bounds.x + node_bounds.width);
    let (top, bottom) = (node_bounds.y, node_bounds.y + node_bounds.height);
    let both = |p: Point| (p, p);
    match state.side {
        PinSide::Row => (Point::new(left, y), Point::new(right, y)),
        PinSide::Left => both(Point::new(left, y)),
        PinSide::Right => both(Point::new(right, y)),
        PinSide::Top => both(Point::new(x, top)),
        PinSide::Bottom => both(Point::new(x, bottom)),
    }
}

/// The bottom-right square a resizable node is grabbed by, in the same
/// layout-absolute space as `bounds`.
///
/// [`RESIZE_GRIP_SIDE`] is a screen-pixel size divided by zoom here, like
/// [`PIN_CLICK_THRESHOLD`](crate::node_graph::PIN_CLICK_THRESHOLD), so the grip
/// covers the same pixels at every zoom. Capped at half the node in each axis:
/// a node at [`MIN_NODE_SIZE`](crate::node_graph::MIN_NODE_SIZE) must still
/// have a body left to drag it by.
///
/// One source for both the hit test and the drawn affordance, so the grip can
/// never be painted somewhere the press does not accept.
fn resize_grip_zone(bounds: Rectangle, zoom: f32) -> Rectangle {
    let side = (RESIZE_GRIP_SIDE / zoom)
        .min(bounds.width * 0.5)
        .min(bounds.height * 0.5);
    Rectangle {
        x: bounds.x + bounds.width - side,
        y: bounds.y + bounds.height - side,
        width: side,
        height: side,
    }
}

#[cfg(test)]
mod grip_tests {
    use super::{MIN_NODE_SIZE, resize_grip_zone};
    use iced_widget::core::{Point, Rectangle, Size};

    fn zone(size: Size, zoom: f32) -> Rectangle {
        resize_grip_zone(Rectangle::new(Point::new(10.0, 20.0), size), zoom)
    }

    #[test]
    fn sits_in_the_bottom_right_corner() {
        assert_eq!(
            zone(Size::new(100.0, 60.0), 1.0),
            Rectangle::new(Point::new(98.0, 68.0), Size::new(12.0, 12.0)),
        );
    }

    // The side scales with 1/zoom, so the grip covers the same screen pixels
    // however far the camera is zoomed in or out.
    #[test]
    fn scales_inversely_with_zoom() {
        assert_eq!(zone(Size::new(100.0, 60.0), 2.0).width, 6.0);
        assert_eq!(zone(Size::new(100.0, 60.0), 0.5).width, 24.0);
    }

    // Half the node is the ceiling in each axis: a node at the minimum size
    // keeps a body to drag it by, and a zoomed-out node is not all grip.
    #[test]
    fn never_covers_more_than_half_the_node() {
        assert_eq!(zone(MIN_NODE_SIZE, 1.0).width, 12.0);
        assert_eq!(zone(Size::new(100.0, 60.0), 0.25).width, 30.0);
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
