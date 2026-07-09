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
    state::{Dragging, NodeGraphState, z_render_indices},
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

// Click detection threshold (in world-space pixels)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

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

/// Line width for the edge cutting overlay (in world-space pixels).
const EDGE_CUT_LINE_WIDTH: f32 = 3.0;

/// Convert a world-space bounding box to screen-space bounds for SdfPrimitive.
///
/// Formula: screen = (world + camera_position) * zoom
/// Returns [x, y, width, height] in screen pixels.
/// Clip shape screen bounds to a layout rectangle.
/// Returns `None` if the shape is entirely off-screen (no intersection).
fn clipped_shape_bounds(b: [f32; 4], clip: Rectangle) -> Option<Rectangle> {
    let x0 = b[0].max(clip.x);
    let y0 = b[1].max(clip.y);
    let x1 = (b[0] + b[2]).min(clip.x + clip.width);
    let y1 = (b[1] + b[3]).min(clip.y + clip.height);
    if x1 <= x0 || y1 <= y0 {
        return None; // fully off-screen
    }
    Some(Rectangle::new(
        Point::new(x0, y0),
        Size::new(x1 - x0, y1 - y0),
    ))
}

/// Camera offset for an SDF layer drawn into the sub-rectangle `clip`.
///
/// The shader uses `clip` as its `bounds_origin`, so the world->screen mapping
/// must shift the camera to compensate for both the clip origin and the
/// widget's own screen offset. Reduces to `camera_position - widget_origin`
/// when `clip` covers the full widget bounds.
fn layer_camera(
    camera_position: WorldPoint,
    zoom: f32,
    widget_origin: Point,
    clip: Rectangle,
) -> (f32, f32) {
    let cx = camera_position.x + (widget_origin.x * (1.0 - zoom) - clip.x) / zoom;
    let cy = camera_position.y + (widget_origin.y * (1.0 - zoom) - clip.y) / zoom;
    (cx, cy)
}

/// Submits an SDF primitive and records whether it animates into `animated`.
///
/// Routing every primitive through one boundary keeps the on-demand redraw flag
/// complete across all layers - edges, fills, node borders, pins, overlays - rather
/// than edges only. `update()` reads the flag to keep an animated `.flow()` pattern
/// redrawing without the host driving a frame clock. Detection must live here on the
/// widget side: the GPU `prepare` step sees the primitive but has no `shell` to
/// request a redraw.
fn draw_sdf<Renderer>(
    renderer: &mut Renderer,
    animated: &std::cell::Cell<bool>,
    clip: Rectangle,
    primitive: SdfPrimitive,
) where
    Renderer: iced_wgpu::primitive::Renderer,
{
    if primitive.has_animations() {
        animated.set(true);
    }
    renderer.draw_primitive(clip, primitive);
}

fn world_bbox_to_screen_bounds(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    padding: f32,
    ctx: &RenderContext,
) -> [f32; 4] {
    let min_x = x0.min(x1) - padding;
    let min_y = y0.min(y1) - padding;
    let max_x = x0.max(x1) + padding;
    let max_y = y0.max(y1) + padding;

    // Node geometry is expressed in absolute layout coordinates (widget origin
    // + world). The screen mapping is `origin + (world + camera) * zoom`, which
    // for an absolute coordinate `a = origin + world` becomes
    // `(a + camera) * zoom + origin * (1 - zoom)`. The `origin * (1 - zoom)`
    // term keeps the bounds aligned with the widget when it is not at the
    // window origin.
    let ox = ctx.viewport_origin.x * (1.0 - ctx.camera_zoom);
    let oy = ctx.viewport_origin.y * (1.0 - ctx.camera_zoom);
    let screen_min_x = (min_x + ctx.camera_position.x) * ctx.camera_zoom + ox;
    let screen_min_y = (min_y + ctx.camera_position.y) * ctx.camera_zoom + oy;
    let screen_max_x = (max_x + ctx.camera_position.x) * ctx.camera_zoom + ox;
    let screen_max_y = (max_y + ctx.camera_position.y) * ctx.camera_zoom + oy;

    [
        screen_min_x,
        screen_min_y,
        screen_max_x - screen_min_x,
        screen_max_y - screen_min_y,
    ]
}

/// Returns the tangent direction vector for a pin side.
/// Left=(-1,0), Right=(1,0), Top=(0,-1), Bottom=(0,1)
fn pin_side_direction(side: u32) -> [f32; 2] {
    match side {
        0 => [-1.0, 0.0], // Left
        1 => [1.0, 0.0],  // Right
        2 => [0.0, -1.0], // Top
        3 => [0.0, 1.0],  // Bottom
        _ => [1.0, 0.0],  // Default (Row)
    }
}

/// Construct the open `Shape` for an edge based on curve type and pin sides. The
/// geometry is world-space (edges are ephemeral, never deduped), so callers push
/// it with a zero placement.
fn edge_shape(
    start: &WorldPoint,
    end: &WorldPoint,
    start_side: u32,
    end_side: u32,
    curve: &crate::style::EdgeCurve,
) -> Shape {
    let p0 = [start.x, start.y];
    let p1 = [end.x, end.y];

    match curve {
        crate::style::EdgeCurve::Line => Shape::line(p0, p1),
        _ => {
            // Bezier: compute control points from pin tangent directions
            let dir_from = pin_side_direction(start_side);
            let dir_to = pin_side_direction(end_side);
            let l = adaptive_bezier_length(p0, p1);
            let cp0 = [p0[0] + dir_from[0] * l, p0[1] + dir_from[1] * l];
            let cp1 = [p1[0] + dir_to[0] * l, p1[1] + dir_to[1] * l];
            Shape::bezier(p0, cp0, cp1, p1)
        }
    }
}

/// Build the stroke `Shape` for an edge plus its shadow shape.
///
/// The shadow shares the stroke geometry, shifted by `style.shadow.offset` when
/// non-zero (otherwise it is a clone of the stroke shape).
fn edge_shapes(
    start: &WorldPoint,
    end: &WorldPoint,
    start_side: u32,
    end_side: u32,
    style: &EdgeStyle,
) -> (Shape, Shape) {
    let shape = edge_shape(start, end, start_side, end_side, &style.curve);
    let has_shadow = style.shadow_blur > 0.0
        && (style.shadow_color.near_start.a > 0.0 || style.shadow_color.near_end.a > 0.0);
    let shadow_shape = if has_shadow && style.shadow_offset != (0.0, 0.0) {
        let (ox, oy) = style.shadow_offset;
        let s_start = WorldPoint::new(start.x + ox, start.y + oy);
        let s_end = WorldPoint::new(end.x + ox, end.y + oy);
        edge_shape(&s_start, &s_end, start_side, end_side, &style.curve)
    } else {
        shape.clone()
    };
    (shape, shadow_shape)
}

/// Push the SDF layers of `style` for an edge onto `batch`, choosing the stroke
/// or shadow shape per layer. Edge geometry is world-space, so placement is zero.
/// Layer order and styling live in [`EdgeStyle::sdf_layers`].
fn push_edge_layers(
    batch: &mut SdfPrimitive,
    shape: &Shape,
    shadow_shape: &Shape,
    style: &EdgeStyle,
) {
    for layer in style.sdf_layers() {
        let shape = match layer.geometry {
            EdgeGeometry::Stroke => shape,
            EdgeGeometry::Shadow => shadow_shape,
        };
        batch.push(shape, &layer.style, [0.0, 0.0]);
    }
}

// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary)
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

/// Resolves a node's style: theme base, then the optional per-node callback.
fn resolve_node_style(
    style_fn: Option<&NodeStyleFn<'_, Theme>>,
    theme: &Theme,
    status: NodeStatus,
) -> NodeStyle {
    match style_fn {
        Some(f) => f(theme, status),
        None => crate::style::default_node_style(theme, status),
    }
}

/// Resolves an edge's style: the per-edge callback, or the built-in default.
fn resolve_edge_style<P: PinId + 'static, UI>(
    style_fn: Option<&EdgeStyleFn<'_, P, UI, Theme>>,
    theme: &Theme,
    status: EdgeStatus,
    start: Option<PinInfo<'_, P, UI>>,
    end: Option<PinInfo<'_, P, UI>>,
) -> EdgeStyle {
    match (style_fn, start, end) {
        (Some(f), Some(s), Some(e)) => f(theme, status, s, e),
        _ => crate::style::default_edge_style(theme, status),
    }
}

/// Builds the read-only [`PinInfo`] view onto a pin state.
fn pin_info<'s, P, UI>(state: &'s NodePinState<P, UI>) -> Option<PinInfo<'s, P, UI>> {
    Some(PinInfo::new(
        state.direction,
        &state.pin_id,
        &state.user_info,
    ))
}

/// Resolves a pin's drawn style: theme base merged with the per-pin overlay,
/// then the indicator fill color forced to the pin's `color`.
fn resolve_pin_style<P: PinId + 'static, UI>(
    pin_style_fn: Option<&PinStyleFn<'_, P, UI, Theme>>,
    state: &NodePinState<P, UI>,
    other: Option<&NodePinState<P, UI>>,
    theme: &Theme,
    status: PinStatus,
) -> PinStyle {
    if let (Some(f), Some(this)) = (pin_style_fn, pin_info::<P, UI>(state)) {
        let other_info = other.and_then(pin_info::<P, UI>);
        f(theme, &this, other_info.as_ref(), status)
    } else {
        crate::style::default_pin_style(theme, status)
    }
}

/// Circular pin cutouts that puncture a node body, translated by `(tx, ty)`.
///
/// Shared by the node fill (drag offset only) and the shadow (drag offset plus
/// shadow offset) so the shadow's holes line up exactly with the body's. The
/// cutout radius tracks the drawn pin indicator. `is_valid_target(pin_idx)`
/// selects the valid-target pin style; the cutout radius is static (no pulse).
/// World-space `(center, radius)` of each pin cutout - the single source for the
/// recipe cuts (`ShapeExpr::Circle` at local offsets) that punch the pin holes,
/// so the body and its shadow punch identical holes.
fn pin_cutout_params<P: PinId + 'static, UI>(
    pins: &[(usize, &NodePinState<P, UI>, (Point, Point))],
    pin_style_fn: Option<&PinStyleFn<'_, P, UI, Theme>>,
    other: Option<&NodePinState<P, UI>>,
    theme: &Theme,
    offset: WorldVector,
    mut is_valid_target: impl FnMut(usize) -> bool,
) -> Vec<([f32; 2], f32)> {
    let mut cuts = Vec::new();
    for (pin_idx, (_pin_index, pin_state, (pos_a, pos_b))) in pins.iter().enumerate() {
        let valid = is_valid_target(pin_idx);
        let pin_status = if valid {
            PinStatus::ValidTarget
        } else {
            PinStatus::Idle
        };
        let pin_style =
            resolve_pin_style::<P, UI>(pin_style_fn, pin_state, other, theme, pin_status);
        let indicator_r = pin_style.radius * 0.4;
        // Cut a hole roughly twice the drawn pin's visual extent, so pins sit in
        // a clear well rather than hugging the body edge.
        let cutout_r = (indicator_r + pin_style.border_width) * 2.0;
        if cutout_r <= 0.01 {
            continue;
        }
        // Row pins project onto two borders, yielding two cutout centers.
        let positions: &[Point] = if pin_state.side == crate::PinSide::Row {
            &[*pos_a, *pos_b]
        } else {
            std::slice::from_ref(pos_a)
        };
        for pos in positions {
            cuts.push(([pos.x + offset.x, pos.y + offset.y], cutout_r));
        }
    }
    cuts
}

/// Camera-aware wrapper for node pop-out overlays (combo box menus, tooltips).
///
/// Node elements lay out — and produce their overlays — in the widget's
/// layout-absolute space, while node content is drawn through the camera
/// transform. This wrapper applies that same transform to the pop-out so it
/// stays anchored to and scales with the node beneath it, and maps the screen
/// cursor back into layout-absolute space for the wrapped overlay's
/// hit-testing (the inverse of the draw transform, mirroring
/// [`Camera2D::cursor_screen_to_layout`]).
struct CameraOverlay<'a, Message, Renderer> {
    content: overlay::Element<'a, Message, iced::Theme, Renderer>,
    camera: super::camera::Camera2D,
}

impl<Message, Renderer> overlay::Overlay<Message, iced::Theme, Renderer>
    for CameraOverlay<'_, Message, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.content.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let cursor = self.camera.cursor_screen_to_layout(cursor);
        renderer.with_transformation(self.camera.layer_transformation(), |renderer| {
            self.content
                .as_overlay()
                .draw(renderer, theme, style, layout, cursor);
        });
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let cursor = self.camera.cursor_screen_to_layout(cursor);
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let cursor = self.camera.cursor_screen_to_layout(cursor);
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, iced::Theme, Renderer>> {
        let camera = self.camera;
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| {
                overlay::Element::new(Box::new(CameraOverlay { content, camera })
                    as Box<dyn overlay::Overlay<Message, iced::Theme, Renderer>>)
            })
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
        let state = tree.state.downcast_ref::<NodeGraphState>();
        // Recompute the animation flag from the primitives actually submitted this
        // frame (each `draw_sdf` ORs its primitive in); reset first so removing the
        // last animated style lets the redraw loop wind down.
        state.sdf_animated.set(false);
        // Refresh the camera's viewport origin from the widget's screen position
        // so SDF layers, child content, and hit-testing stay aligned when the
        // graph is not at the window origin (e.g. below a toolbar).
        let mut camera = state
            .camera
            .with_viewport_origin(layout.bounds().position().into_euclid().to_vector());
        let z_indices = z_render_indices(state, self.nodes.len());

        // Update time for animations
        let time = {
            let now = Instant::now();
            if let Some(last_update) = state.last_update {
                let delta = now.duration_since(last_update).as_secs_f32();
                let capped_delta = delta.min(0.1);
                state.time + capped_delta
            } else {
                state.time
            }
        };

        // Create RenderContext (will be finalized after camera panning is applied)
        let mut render_context = RenderContext {
            camera_zoom: state.camera.zoom(),
            camera_position: state.camera.position(),
            viewport_origin: camera.viewport_origin(),
            time,
        };

        // Handle panning when dragging the graph
        if let Dragging::Graph(origin) = state.dragging
            && let Some(cursor_position) = cursor.position()
        {
            let cursor_position: ScreenPoint = cursor_position.into_euclid();
            let cursor_position: WorldPoint =
                camera.screen_to_world().transform_point(cursor_position);
            camera = camera.move_by(cursor_position - origin);
        }

        // Update render context with final camera state
        render_context.camera_zoom = camera.zoom();
        render_context.camera_position = camera.position();

        // Resolve styles
        let resolved_graph = if let Some(ref style_fn) = self.graph_style {
            style_fn(theme)
        } else {
            GraphStyle::from_theme(theme)
        };

        // Check if we're edge dragging
        let is_edge_dragging = matches!(
            state.dragging,
            Dragging::Edge(_, _, _) | Dragging::EdgeOver(_, _, _, _)
        );

        // The pin an edge drag started from, surfaced as `other` to pin_style so
        // candidate pins can react to what is being dragged toward them.
        let drag_source: Option<NodePinState<P, UI>> = match state.dragging {
            Dragging::Edge(from_node, from_pin, _)
            | Dragging::EdgeOver(from_node, from_pin, _, _) => tree
                .children
                .get(from_node)
                .zip(layout.children().nth(from_node))
                .and_then(|(nt, nl)| {
                    find_pins::<P, UI>(nt, nl)
                        .into_iter()
                        .nth(from_pin)
                        .map(|(_, s, _)| s.clone())
                }),
            _ => None,
        };

        // ========================================
        // Layer 1: Background (solid color)
        // ========================================
        renderer.with_layer(layout.bounds(), |renderer| {
            renderer.fill_quad(
                iced_wgpu::core::renderer::Quad {
                    bounds: layout.bounds(),
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                    snap: true,
                },
                iced_wgpu::core::Background::Color(resolved_graph.background_color),
            );
        });

        // The tiling grid is folded into the single graph-background draw below
        // (z0, under the node + edge shadows and the edge strokes) so the whole
        // below-nodes layer is one fullscreen SDF pass.

        // ========================================
        // Collect edge data with resolved positions
        // ========================================
        // Helper to compute drag offset for a node
        // Node/group drag origins are captured in layout-absolute space (the
        // event closure's cursor), so the live preview must compute the cursor
        // in the same space; the `viewport_origin` term cancels in the delta.
        let vo = camera.viewport_origin();
        let cursor_layout = |cursor_pos: iced::Point| -> WorldPoint {
            let w = camera
                .screen_to_world()
                .transform_point(cursor_pos.into_euclid());
            WorldPoint::new(w.x + vo.x, w.y + vo.y)
        };
        let compute_node_offset = |node_idx: usize| -> WorldVector {
            let mut offset = WorldVector::zero();
            let is_selected = state.selected_nodes.contains(&node_idx);

            // Single node drag
            if let (Dragging::Node(drag_idx, origin), Some(cursor_pos)) =
                (&state.dragging, cursor.position())
                && *drag_idx == node_idx
            {
                offset = cursor_layout(cursor_pos) - *origin;
            }

            // Group move
            if let (Dragging::GroupMove(origin), Some(cursor_pos)) =
                (&state.dragging, cursor.position())
                && is_selected
            {
                offset = cursor_layout(cursor_pos) - *origin;
            }

            offset
        };

        // ========================================
        // Per-node geometry, built once and shared by the node shadows (below)
        // and the fill/border (Layer 4). The silhouette (body minus pin cutouts)
        // is the expensive boolean, so it is never built twice: the shadow clones
        // and shifts it by the shadow offset rather than rebuilding it.
        // ========================================
        struct NodeGeom {
            // The position-free shape (body minus pin cutouts) in its LOCAL frame
            // (centred on the origin), plus the node's world centre (the
            // per-instance placement). Two identical nodes at different positions
            // share one cache slot: the shape hashes equal, only `center` differs.
            shape: Shape,
            center: [f32; 2],
            resolved: NodeStyle,
            offset: WorldVector,
            position: WorldPoint,
            size: Size,
        }
        impl NodeGeom {
            /// Push the node silhouette to `batch` with `style`, placed at the
            /// node centre shifted by `extra` (the shadow offset, or zero for
            /// fill/border).
            fn push_body(
                &self,
                batch: &mut SdfPrimitive,
                style: &iced_nodegraph_sdf::Style,
                extra: (f32, f32),
            ) {
                batch.push(
                    &self.shape,
                    style,
                    [self.center[0] + extra.0, self.center[1] + extra.1],
                );
            }
        }
        let t_geom_start = Instant::now();
        let node_geoms: Vec<Option<NodeGeom>> = (0..self.nodes.len())
            .map(|node_index| {
                let (_id, _position, _element, node_style, node_pin_style) =
                    &self.nodes[node_index];
                let node_layout = layout.children().nth(node_index)?;
                let node_tree = tree.children.get(node_index)?;
                let status = if state.selected_nodes.contains(&node_index) {
                    NodeStatus::Selected
                } else {
                    NodeStatus::Idle
                };
                let resolved = resolve_node_style(node_style.as_ref(), theme, status);
                let offset = compute_node_offset(node_index);
                let position: WorldPoint =
                    (node_layout.bounds().position().into_euclid().to_vector() + offset).to_point();
                let size = node_layout.bounds().size();
                let pins = find_pins::<P, UI>(node_tree, node_layout);
                let center = [
                    position.x + size.width * 0.5,
                    position.y + size.height * 0.5,
                ];
                let cut_params = pin_cutout_params(
                    &pins,
                    node_pin_style.as_ref(),
                    drag_source.as_ref(),
                    theme,
                    offset,
                    |pin_idx| {
                        is_edge_dragging
                            && state.valid_drop_targets.contains(&(node_index, pin_idx))
                    },
                );

                // Body = a centre-origin rounded box; each pin cut sits at a LOCAL
                // offset relative to the body centre, so two identical nodes at
                // different positions share a recipe (the position lives entirely
                // in `center`). `box - cut0 - cut1 - ...` as authored.
                let mut shape =
                    Shape::rounded_box([size.width, size.height], [resolved.corner_radius; 4]);
                for &(c, r) in &cut_params {
                    shape =
                        shape - Shape::circle(r).translate([c[0] - center[0], c[1] - center[1]]);
                }

                Some(NodeGeom {
                    shape,
                    center,
                    resolved,
                    offset,
                    position,
                    size,
                })
            })
            .collect();
        let t_after_geom = Instant::now();

        // ========================================
        // Graph background: ONE batched SDF draw under all nodes. Within a single
        // SDF primitive the FIRST-pushed entry composites in FRONT (the cull
        // sorts slots ascending by push index and the fragment blends them
        // front-to-back), so entries are pushed FRONT-TO-BACK here: edge strokes
        // (z2, top), then all shadows (z1, edge + node), then the grid (z0,
        // bottom). Folding the grid, every node shadow and every edge into one
        // primitive collapses the whole below-nodes layer into a single
        // fullscreen fragment pass. Pushing ALL strokes before ANY shadow keeps
        // every edge line above every shadow. The node bodies (Layer 4) paint
        // over all of it. The grid is no longer marked cacheable: the dynamic
        // shadows/edges sharing this draw would never let the static-background
        // texture cache hit. Node shadows within the z1 band are pushed in
        // STABLE node-index order rather than the selection-sorted `z_indices`
        // (see below) - bg_layer is a single SdfPrimitive whose geometry hash
        // covers entry push order, so ordering shadows by `z_indices` would
        // make every selection click re-hash and rebuild the whole background
        // (all edge biarcs included) just to reshuffle translucent shadows
        // that composite the same either way.
        // ========================================
        let bg_layer = {
            let mut bg = SdfPrimitive::with_capacity(self.nodes.len() + self.edges.len() * 4 + 1);

            // Edge layers split by geometry: strokes (z2) collected to push first
            // (front), shadows (z1) collected to push behind them. Each edge's own
            // layer order is preserved within each group.
            let pending_cuts = match &state.dragging {
                Dragging::EdgeCutting { pending_cuts, .. } => Some(pending_cuts),
                _ => None,
            };
            let mut edge_strokes: Vec<(Shape, Style)> = Vec::with_capacity(self.edges.len() * 2);
            let mut edge_shadows: Vec<(Shape, Style)> = Vec::with_capacity(self.edges.len());

            for (edge_idx, (_edge_id, from, to, edge_style_fn)) in self.edges.iter().enumerate() {
                let Some(from_node_idx) = self.node_index(&from.node_id) else {
                    continue;
                };
                let Some(to_node_idx) = self.node_index(&to.node_id) else {
                    continue;
                };
                let Some(from_node_tree) = tree.children.get(from_node_idx) else {
                    continue;
                };
                let Some(from_node_layout) = layout.children().nth(from_node_idx) else {
                    continue;
                };
                let Some(to_node_tree) = tree.children.get(to_node_idx) else {
                    continue;
                };
                let Some(to_node_layout) = layout.children().nth(to_node_idx) else {
                    continue;
                };

                let from_offset = compute_node_offset(from_node_idx);
                let to_offset = compute_node_offset(to_node_idx);

                let from_pins = find_pins::<P, UI>(from_node_tree, from_node_layout);
                let Some((_, from_pin_state, (from_pin_pos, _))) = from_pins
                    .iter()
                    .find(|(_, state, _)| state.pin_id == from.pin_id)
                else {
                    continue;
                };

                let to_pins = find_pins::<P, UI>(to_node_tree, to_node_layout);
                let Some((_, to_pin_state, (to_pin_pos, _))) = to_pins
                    .iter()
                    .find(|(_, state, _)| state.pin_id == to.pin_id)
                else {
                    continue;
                };

                let from_pos = (from_pin_pos.into_euclid().to_vector() + from_offset).to_point();
                let to_pos = (to_pin_pos.into_euclid().to_vector() + to_offset).to_point();
                let from_side: u32 = from_pin_state.side.into();
                let to_side: u32 = to_pin_state.side.into();
                let from_info = pin_info::<P, UI>(from_pin_state);
                let to_info = pin_info::<P, UI>(to_pin_state);

                // Normalize orientation so the OUTPUT pin is the edge start
                // (output -> input). Gradient, arrow and flow then follow the
                // data-flow direction regardless of which side was dragged from.
                let swap = !matches!(from_pin_state.direction, PinDirection::Output)
                    && matches!(to_pin_state.direction, PinDirection::Output);
                let (start_pos, end_pos, start_side, end_side, start_info, end_info) = if swap {
                    (to_pos, from_pos, to_side, from_side, to_info, from_info)
                } else {
                    (from_pos, to_pos, from_side, to_side, from_info, to_info)
                };

                let edge_status = if pending_cuts.is_some_and(|cuts| cuts.contains(&edge_idx)) {
                    EdgeStatus::PendingCut
                } else {
                    EdgeStatus::Idle
                };
                let edge_style = resolve_edge_style(
                    edge_style_fn.as_ref(),
                    theme,
                    edge_status,
                    start_info,
                    end_info,
                );

                let (shape, shadow_shape) =
                    edge_shapes(&start_pos, &end_pos, start_side, end_side, &edge_style);

                // Collect this edge's layers by geometry; both groups are pushed
                // in z order after the loop.
                for layer in edge_style.sdf_layers() {
                    match layer.geometry {
                        EdgeGeometry::Stroke => {
                            edge_strokes.push((shape.clone(), layer.style));
                        }
                        EdgeGeometry::Shadow => {
                            edge_shadows.push((shadow_shape.clone(), layer.style));
                        }
                    }
                }
            }

            // z2: edge strokes (frontmost in the background layer).
            for (shape, style) in &edge_strokes {
                bg.push(shape, style, [0.0, 0.0]);
            }

            // z1: shadows behind the strokes - edge shadows, then node shadows
            // (same plane; both above the grid and below every edge line).
            for (shape, style) in &edge_shadows {
                bg.push(shape, style, [0.0, 0.0]);
            }
            // Node shadows are pushed in STABLE node-index order, not
            // `z_indices` (which re-sorts by (selected, z) on every selection
            // change) - see the rationale above. Shadow-over-shadow blending
            // is commutative for overlapping nodes that share the same shadow
            // color/alpha (premultiplied "over" with equal operands); with
            // differing custom shadow styles the overlap blend can shift
            // marginally, an accepted trade for not rebuilding the whole
            // bg_layer (edge biarcs included) on every selection click.
            for geom in node_geoms.iter().flatten() {
                if !geom.resolved.has_shadow() {
                    continue;
                }
                let (ox, oy) = geom.resolved.shadow_offset;
                for band in geom.resolved.shadow_sdf_layers(geom.resolved.opacity) {
                    geom.push_body(&mut bg, &band, (ox, oy));
                }
            }

            // z0: tiling grid/dots/triangles/hex (backmost).
            if let Some(tiling) = resolved_graph.tiling {
                let tiling_shape = Shape::tiling(match tiling.kind {
                    TilingKind::Grid => {
                        Tiling::grid(tiling.spacing, tiling.spacing, tiling.thickness)
                    }
                    TilingKind::Dots => {
                        Tiling::dots(tiling.spacing, tiling.spacing, tiling.thickness)
                    }
                    TilingKind::Triangles => Tiling::triangles(tiling.spacing, tiling.thickness),
                    TilingKind::Hex => Tiling::hex(tiling.spacing, tiling.thickness),
                });
                // Grid/triangle/hex give the unsigned distance to the line, so
                // their thickness comes from the style; dots bake the radius in.
                let style = match tiling.kind {
                    TilingKind::Dots => Style::solid(tiling.color),
                    _ => Style::solid(tiling.color).expand(tiling.thickness * 0.5),
                };
                bg.push(&tiling_shape, &style, [0.0, 0.0]);
            }

            bg
        };

        // Batches clipped to the full graph bounds use the bounds origin as the
        // shader's `bounds_origin`, so the camera offset compensates with
        // `camera_position - widget_origin` (the general formula reduced for a
        // full-bounds clip). No-op when the graph is at the window origin.
        if !bg_layer.is_empty() {
            let wo = layout.bounds().position();
            let (cx, cy) = layer_camera(
                render_context.camera_position,
                render_context.camera_zoom,
                wo,
                layout.bounds(),
            );
            renderer.with_layer(layout.bounds(), |renderer| {
                draw_sdf(
                    renderer,
                    &state.sdf_animated,
                    layout.bounds(),
                    bg_layer
                        .camera(cx, cy, render_context.camera_zoom)
                        .time(render_context.time),
                );
            });
        }

        // Dragging edge (single primitive, only during interaction). Kept as its
        // own draw above the background but below the nodes, matching its prior
        // z-position; it is never folded into the background batch.
        if let Dragging::Edge(from_node_idx, from_pin_idx, _) = &state.dragging
            && let Some(cursor_pos) = cursor.position()
            && let (Some(from_tree), Some(from_layout)) = (
                tree.children.get(*from_node_idx),
                layout.children().nth(*from_node_idx),
            )
        {
            let from_pins = find_pins::<P, UI>(from_tree, from_layout);
            if let Some((_, from_pin_state, (from_pin_pos, _))) = from_pins.get(*from_pin_idx) {
                let from_offset = compute_node_offset(*from_node_idx);
                let start_pos = (from_pin_pos.into_euclid().to_vector() + from_offset).to_point();
                // Loose end follows the cursor in the same layout-absolute space
                // as the pin geometry so the dragged edge stays aligned when the
                // graph is off the window origin.
                let end_pos: WorldPoint = cursor_layout(cursor_pos);

                let drag_edge_style = match (
                    self.dragging_edge_style_fn.as_ref(),
                    pin_info::<P, UI>(from_pin_state),
                ) {
                    (Some(f), Some(info)) => f(theme, info),
                    _ => crate::style::default_edge_style(theme, EdgeStatus::Idle),
                };

                let from_side: u32 = from_pin_state.side.into();
                let cursor_side: u32 = match from_pin_state.side {
                    PinSide::Left => 1,
                    PinSide::Right => 0,
                    PinSide::Top => 3,
                    PinSide::Bottom => 2,
                    PinSide::Row => 1,
                };

                // Output = start, input = end. Dragging FROM an input pin puts
                // the held pin at the END and the cursor at the START (flip);
                // from an output it stays start -> cursor end.
                let (start_pos, end_pos, start_side, end_side) =
                    if matches!(from_pin_state.direction, PinDirection::Input) {
                        (end_pos, start_pos, cursor_side, from_side)
                    } else {
                        (start_pos, end_pos, from_side, cursor_side)
                    };

                let (shape, shadow_shape) =
                    edge_shapes(&start_pos, &end_pos, start_side, end_side, &drag_edge_style);

                let mut drag_batch = SdfPrimitive::new();
                push_edge_layers(&mut drag_batch, &shape, &shadow_shape, &drag_edge_style);

                let wo = layout.bounds().position();
                let (cx, cy) = layer_camera(
                    render_context.camera_position,
                    render_context.camera_zoom,
                    wo,
                    layout.bounds(),
                );
                renderer.with_layer(layout.bounds(), |renderer| {
                    draw_sdf(
                        renderer,
                        &state.sdf_animated,
                        layout.bounds(),
                        drag_batch
                            .camera(cx, cy, render_context.camera_zoom)
                            .time(render_context.time),
                    );
                });
            }
        }
        let t_after_background = Instant::now();

        // ========================================
        // Layers 4..N: Nodes (each node gets 3 sub-layers)
        // For each node: Fill → Widgets → Foreground (border + pins batched)
        // ========================================
        for &node_index in &z_indices {
            let (_id, _position, element, _node_style, node_pin_style) = &self.nodes[node_index];
            let Some(node_tree) = tree.children.get(node_index) else {
                continue;
            };
            let Some(node_layout) = layout.children().nth(node_index) else {
                continue;
            };
            let Some(geom) = node_geoms[node_index].as_ref() else {
                continue;
            };
            let resolved = &geom.resolved;
            let offset = geom.offset;
            let node_position = geom.position;
            let node_size = geom.size;
            // The silhouette (body minus pin cutouts) was prepared once in the
            // per-node pre-pass as a cached recipe; `geom.push_body` reuses it
            // for fill and border.

            let opacity = resolved.opacity;
            let cam_zoom = render_context.camera_zoom;

            // Pins drive the foreground (border halo plus indicators); the body
            // cutouts they imply are already baked into `node_outline`.
            let pins = find_pins::<P, UI>(node_tree, node_layout);

            // Layer 4a: Node Fill
            let fill_pad = 2.0 / cam_zoom;
            let fb = world_bbox_to_screen_bounds(
                node_position.x,
                node_position.y,
                node_position.x + node_size.width,
                node_position.y + node_size.height,
                fill_pad,
                &render_context,
            );
            if let Some(fill_clip) = clipped_shape_bounds(fb, layout.bounds()) {
                let (cx, cy) = layer_camera(
                    render_context.camera_position,
                    cam_zoom,
                    layout.bounds().position(),
                    fill_clip,
                );
                renderer.with_layer(layout.bounds(), |renderer| {
                    let mut fill_batch = SdfPrimitive::new();
                    geom.push_body(
                        &mut fill_batch,
                        &resolved.fill_sdf_style(opacity),
                        (0.0, 0.0),
                    );
                    draw_sdf(
                        renderer,
                        &state.sdf_animated,
                        fill_clip,
                        fill_batch
                            .camera(cx, cy, cam_zoom)
                            .time(render_context.time),
                    );
                });
            }

            // Layer 4b: Node Widgets
            // Mirrors Container::clip(true): bound the child viewport to the
            // graph so widgets inside nodes can't paint past the graph edge.
            let clipped_viewport = layout
                .bounds()
                .intersection(viewport)
                .unwrap_or(Rectangle::new(layout.bounds().position(), Size::ZERO));
            renderer.with_layer(layout.bounds(), |renderer| {
                camera.draw_with::<_, Renderer>(
                    renderer,
                    &clipped_viewport,
                    cursor,
                    |renderer, viewport, cursor| {
                        let bounds = node_layout.bounds();
                        let screen_offset: Vector = offset.into_iced();
                        // Clip content to the full node bounds (the body edge).
                        // The border sits outside the silhouette, so it never
                        // narrows the content area: selection thickening the
                        // border no longer shrinks the node interior.
                        let node_clip = Rectangle {
                            x: bounds.x + screen_offset.x,
                            y: bounds.y + screen_offset.y,
                            width: bounds.width,
                            height: bounds.height,
                        };

                        // push_clip replaces (does not intersect) the parent
                        // clip, so intersect with the graph viewport here;
                        // otherwise a node straddling the graph edge paints its
                        // content (e.g. the title bar) past that edge.
                        let clip_bounds = node_clip
                            .intersection(viewport)
                            .unwrap_or(Rectangle::new(node_clip.position(), Size::ZERO));

                        // The child is laid out at its stored position and shifted
                        // into place by `screen_offset` during a drag. Child widgets
                        // cull their content against the viewport using that stored
                        // (pre-translation) position, so a node dragged in from off
                        // screen would have its content (e.g. text glyphs) culled as
                        // if still off screen (visible only after the next drop
                        // re-laid it out). Compensate by handing the child the
                        // viewport in its own pre-translation space.
                        let child_viewport = Rectangle {
                            x: viewport.x - screen_offset.x,
                            y: viewport.y - screen_offset.y,
                            width: viewport.width,
                            height: viewport.height,
                        };

                        renderer.with_layer(clip_bounds, |renderer| {
                            renderer.with_translation(screen_offset, |renderer| {
                                element.as_widget().draw(
                                    node_tree,
                                    renderer,
                                    theme,
                                    style,
                                    node_layout,
                                    cursor,
                                    &child_viewport,
                                );
                            });
                        });
                    },
                );
            });

            // Layer 4c: Node Foreground (border + pins batched)
            let has_border = resolved.border_pattern.thickness > 0.0;
            let has_pins = !pins.is_empty();

            if has_border || has_pins {
                let mut fg_batch = SdfPrimitive::with_capacity(pins.len() * 2 + 2);
                let mut fg_min_x = f32::MAX;
                let mut fg_min_y = f32::MAX;
                let mut fg_max_x = f32::MIN;
                let mut fg_max_y = f32::MIN;

                // Border (main stroke in front; outline pushed behind as halo).
                // Cull padding follows the actual layer extents rather than a
                // hand-tuned guess; the node body is a closed shape.
                let border_layers = resolved.border_sdf_layers(opacity);
                if !border_layers.is_empty() {
                    let border_pad = border_layers
                        .iter()
                        // The node body is always a closed shape.
                        .map(|s| s.extent(true))
                        .fold(0.0_f32, f32::max)
                        + 2.0 / cam_zoom;
                    let bb = world_bbox_to_screen_bounds(
                        node_position.x,
                        node_position.y,
                        node_position.x + node_size.width,
                        node_position.y + node_size.height,
                        border_pad,
                        &render_context,
                    );

                    for style in &border_layers {
                        geom.push_body(&mut fg_batch, style, (0.0, 0.0));
                    }

                    fg_min_x = fg_min_x.min(bb[0]);
                    fg_min_y = fg_min_y.min(bb[1]);
                    fg_max_x = fg_max_x.max(bb[0] + bb[2]);
                    fg_max_y = fg_max_y.max(bb[1] + bb[3]);
                }

                // Pins
                for (pin_idx, (_pin_index, pin_state, (pin_pos, _))) in pins.iter().enumerate() {
                    let is_valid_target = is_edge_dragging
                        && state.valid_drop_targets.contains(&(node_index, pin_idx));
                    let pin_status = if is_valid_target {
                        PinStatus::ValidTarget
                    } else {
                        PinStatus::Idle
                    };
                    let pin_style = resolve_pin_style(
                        node_pin_style.as_ref(),
                        pin_state,
                        drag_source.as_ref(),
                        theme,
                        pin_status,
                    );
                    let indicator_r = pin_style.radius * 0.4;
                    let pin_world: WorldPoint =
                        (pin_pos.into_euclid().to_vector() + offset).to_point();
                    let pw = [pin_world.x, pin_world.y];

                    // Pin shapes are centred on the pin, and so is every
                    // primitive's origin, so the placement is just the pin
                    // position - and identical pins share a recipe.
                    let (pin_shape, pin_place) = match pin_style.shape {
                        crate::style::PinShape::Square => {
                            let h = indicator_r * 0.7;
                            (Shape::rounded_box([2.0 * h, 2.0 * h], [0.0; 4]), pw)
                        }
                        _ => (Shape::circle(indicator_r), pw),
                    };

                    let pin_layers = pin_style.sdf_layers(pin_state.direction, indicator_r);
                    // Bounds: shape radius plus the largest layer extent beyond
                    // the shape boundary (input ring, border ring). Pins are
                    // closed shapes.
                    let pin_pad = indicator_r
                        + pin_layers
                            .iter()
                            .map(|s| s.extent(true))
                            .fold(0.0_f32, f32::max)
                        + 2.0 / cam_zoom;
                    let pin_bounds = world_bbox_to_screen_bounds(
                        pin_world.x - pin_pad,
                        pin_world.y - pin_pad,
                        pin_world.x + pin_pad,
                        pin_world.y + pin_pad,
                        0.0,
                        &render_context,
                    );

                    for style in &pin_layers {
                        fg_batch.push(&pin_shape, style, pin_place);
                    }

                    fg_min_x = fg_min_x.min(pin_bounds[0]);
                    fg_min_y = fg_min_y.min(pin_bounds[1]);
                    fg_max_x = fg_max_x.max(pin_bounds[0] + pin_bounds[2]);
                    fg_max_y = fg_max_y.max(pin_bounds[1] + pin_bounds[3]);
                }

                if let Some(fg_clip) = clipped_shape_bounds(
                    [fg_min_x, fg_min_y, fg_max_x - fg_min_x, fg_max_y - fg_min_y],
                    layout.bounds(),
                ) {
                    let (cx, cy) = layer_camera(
                        render_context.camera_position,
                        cam_zoom,
                        layout.bounds().position(),
                        fg_clip,
                    );
                    renderer.with_layer(layout.bounds(), |renderer| {
                        draw_sdf(
                            renderer,
                            &state.sdf_animated,
                            fg_clip,
                            fg_batch.camera(cx, cy, cam_zoom).time(render_context.time),
                        );
                    });
                }
            }
        }
        let t_after_fg = Instant::now();

        // ========================================
        // Layer N+1: Box Selection Overlay
        // ========================================
        if let Dragging::BoxSelect(start, _end) = &state.dragging {
            // `start` was captured in layout-absolute space (the event closure's
            // cursor), so the live corner must match that space.
            let cursor_world = cursor.position().map(cursor_layout).unwrap_or(*start);

            // Resolve box select colors: use callback if provided, otherwise use selection_style
            let (fill_color, border_color) = if let Some(ref style_fn) = self.box_select_style_fn {
                style_fn(theme)
            } else {
                (
                    resolved_graph.selection_style.box_select_fill,
                    resolved_graph.selection_style.box_select_border,
                )
            };

            let center = [
                (start.x + cursor_world.x) * 0.5,
                (start.y + cursor_world.y) * 0.5,
            ];
            let half_size = [
                ((cursor_world.x - start.x) * 0.5).abs(),
                ((cursor_world.y - start.y) * 0.5).abs(),
            ];
            let border_width = 1.5 / camera.zoom();

            let select_bounds = world_bbox_to_screen_bounds(
                start.x,
                start.y,
                cursor_world.x,
                cursor_world.y,
                border_width + 2.0 / camera.zoom(),
                &render_context,
            );

            if let Some(select_clip) = clipped_shape_bounds(select_bounds, layout.bounds()) {
                let select_shape =
                    Shape::rounded_box([half_size[0] * 2.0, half_size[1] * 2.0], [0.0; 4]);
                let select_place = center;
                let mut select_batch = SdfPrimitive::with_capacity(2);
                // Border (front), fill (behind)
                select_batch.push(
                    &select_shape,
                    &Style::stroke(border_color, Pattern::solid(border_width)),
                    select_place,
                );
                select_batch.push(&select_shape, &Style::solid(fill_color), select_place);

                let (cx, cy) = layer_camera(
                    render_context.camera_position,
                    render_context.camera_zoom,
                    layout.bounds().position(),
                    select_clip,
                );
                let select_primitive = select_batch
                    .camera(cx, cy, render_context.camera_zoom)
                    .time(render_context.time);

                renderer.with_layer(layout.bounds(), |renderer| {
                    draw_sdf(renderer, &state.sdf_animated, select_clip, select_primitive);
                });
            }
        }

        // ========================================
        // Layer N+2: Edge Cutting Overlay
        // ========================================
        if let Dragging::EdgeCutting { trail, .. } = &state.dragging
            && let Some(start) = trail.first()
        {
            // `start` was captured in layout-absolute space (the event closure's
            // cursor), so the live corner must match that space.
            let cursor_world = cursor.position().map(cursor_layout).unwrap_or(*start);

            // Resolve cutting tool color: use callback if provided, otherwise use selection_style
            let cutting_color = if let Some(ref style_fn) = self.cutting_tool_style_fn {
                style_fn(theme)
            } else {
                resolved_graph.selection_style.edge_cutting_color
            };

            let cutting_bounds = world_bbox_to_screen_bounds(
                start.x,
                start.y,
                cursor_world.x,
                cursor_world.y,
                EDGE_CUT_LINE_WIDTH + 2.0 / render_context.camera_zoom,
                &render_context,
            );

            if let Some(cutting_clip) = clipped_shape_bounds(cutting_bounds, layout.bounds()) {
                let mut cutting_batch = SdfPrimitive::new();
                cutting_batch.push(
                    &Shape::line([start.x, start.y], [cursor_world.x, cursor_world.y]),
                    &Style::stroke(cutting_color, Pattern::solid(EDGE_CUT_LINE_WIDTH)),
                    [0.0, 0.0],
                );
                let (cx, cy) = layer_camera(
                    render_context.camera_position,
                    render_context.camera_zoom,
                    layout.bounds().position(),
                    cutting_clip,
                );
                let cutting_primitive = cutting_batch
                    .camera(cx, cy, render_context.camera_zoom)
                    .time(render_context.time);

                renderer.with_layer(layout.bounds(), |renderer| {
                    draw_sdf(
                        renderer,
                        &state.sdf_animated,
                        cutting_clip,
                        cutting_primitive,
                    );
                });
            }
        }

        // Gather per-frame diagnostics (CPU-side) and stash them for the next
        // update() to deliver via the `on_info` callback. Only when a host asked for
        // them; cheap otherwise (a few elapsed reads + one bbox test per node).
        if self.on_info.is_some() {
            let viewport = layout.bounds();
            let mut node_in_view = vec![false; node_geoms.len()];
            let mut nodes_in = 0usize;
            let mut pins_total = 0usize;
            let mut pins_in = 0usize;
            for (i, geom) in node_geoms.iter().enumerate() {
                let Some(geom) = geom else { continue };
                let bb = world_bbox_to_screen_bounds(
                    geom.position.x,
                    geom.position.y,
                    geom.position.x + geom.size.width,
                    geom.position.y + geom.size.height,
                    0.0,
                    &render_context,
                );
                let rect = Rectangle {
                    x: bb[0],
                    y: bb[1],
                    width: bb[2],
                    height: bb[3],
                };
                let in_view = rect.intersects(&viewport);
                node_in_view[i] = in_view;
                if in_view {
                    nodes_in += 1;
                }
                if let (Some(nt), Some(nl)) = (tree.children.get(i), layout.children().nth(i)) {
                    let pin_count = find_pins::<P, UI>(nt, nl).len();
                    pins_total += pin_count;
                    if in_view {
                        pins_in += pin_count;
                    }
                }
            }
            let edges_in = self
                .edges
                .iter()
                .filter(|(_, from, to, _)| {
                    let visible = |id| self.node_index(id).is_some_and(|idx| node_in_view[idx]);
                    visible(&from.node_id) || visible(&to.node_id)
                })
                .count();

            let counts = |total: usize, in_view: usize| Counts {
                total,
                in_view,
                culled: total - in_view,
            };
            let sdf = iced_nodegraph_sdf::sdf_stats();
            let info = GraphInfo {
                nodes: counts(node_geoms.len(), nodes_in),
                pins: counts(pins_total, pins_in),
                edges: counts(self.edges.len(), edges_in),
                timings: vec![
                    OpTiming {
                        label: "geometry",
                        duration: t_after_geom - t_geom_start,
                    },
                    OpTiming {
                        label: "background",
                        duration: t_after_background - t_after_geom,
                    },
                    OpTiming {
                        label: "foreground",
                        duration: t_after_fg - t_after_background,
                    },
                    OpTiming {
                        label: "sdf_prepare",
                        duration: std::time::Duration::from_micros(sdf.prepare_cpu_us),
                    },
                ],
                sdf_entries: sdf.entry_count,
                sdf_tiles: sdf.tile_count,
            };
            state.last_info.replace(Some(info));
        }
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
        let state = tree.state.downcast_mut::<NodeGraphState>();

        // Sync the host-controlled view (`view()`) into the camera, but only when
        // the host changed it since we last synced. Comparing against the live
        // camera would also fire while the user is mid pan/zoom (before the
        // matching `on_pan` round-trips back into `view`), clobbering the
        // interaction with a stale value. Same race-avoidance as selection.
        if let Some(view) = self.view_value()
            && state.last_synced_view != Some(view)
        {
            let (position, zoom) = view;
            state.camera = super::camera::Camera2D::with_zoom_and_position(
                zoom,
                WorldPoint::new(position.x, position.y),
            );
            state.last_synced_view = Some(view);
        }

        // Refresh the viewport origin so screen->layout mapping (cursor hit-tests,
        // child event propagation) aligns when the graph is not at the window
        // origin. Drag deltas and emitted positions are relative or use stored
        // world coordinates, so this origin term cancels there.
        state.camera = state
            .camera
            .with_viewport_origin(layout.bounds().position().into_euclid().to_vector());

        // Assign z-order entries to any newly-seen node indices so freshly
        // pushed nodes spawn on top of older ones.
        state.ensure_z_entries(self.nodes.len());
        let z_indices = z_render_indices(state, self.nodes.len());

        // Sync the externally-provided selection (`.selection()`) into state
        // only when the host changed it since we last looked. Comparing
        // against `state.selected_nodes` directly would also fire when the
        // widget itself just modified the state (box-select drag, click etc.)
        // and the matching `on_select` message has not yet propagated back
        // through the host into a refreshed `external_selection` — that race
        // would clobber the new state with a stale external value, breaking
        // any host that uses `.selection()`.
        if let Some(external) = self.get_external_selection()
            && state.last_synced_external.as_ref() != Some(external)
        {
            state.selected_nodes = external.clone();
            state.last_synced_external = Some(external.clone());
        }

        // Update time for animations
        // Cap delta to prevent large time jumps when app is in background
        let now = Instant::now();

        if let Some(last_update) = state.last_update {
            let delta = now.duration_since(last_update).as_secs_f32();
            // Cap at 100ms to prevent freeze after background
            let capped_delta = delta.min(0.1);
            state.time += capped_delta;
        }
        state.last_update = Some(now);

        // On each frame, drive continuous redraws for SDF animations and deliver
        // the diagnostics measured during the previous draw().
        if let Event::Window(iced::window::Event::RedrawRequested(_)) = event {
            if state.sdf_animated.get() {
                shell.request_redraw();
            }
            // Publish the stashed GraphInfo (set during draw) one frame behind,
            // mirroring the controlled on_pan pattern. A host showing live
            // diagnostics needs a steady frame stream, so keep redraws flowing.
            if let Some(handler) = self.on_info_handler() {
                if let Some(info) = state.last_info.borrow_mut().take() {
                    shell.publish(handler(info));
                }
                shell.request_redraw();
            }
        }

        // Track keyboard modifiers for Shift/Ctrl selection
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }

        // Handle keyboard shortcuts
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
            match key {
                // Ctrl+D: Clone selected nodes. Gated on on_clone: without a handler
                // the clone cannot be persisted, so leave the shortcut unhandled and
                // let the key fall through instead of silently swallowing it.
                keyboard::Key::Character(c)
                    if c.as_str() == "d"
                        && modifiers.command()
                        && !state.selected_nodes.is_empty()
                        && self.on_clone_handler().is_some() =>
                {
                    let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                    let node_ids = self.translate_node_ids(&indices);
                    if let Some(handler) = self.on_clone_handler() {
                        shell.publish(handler(node_ids));
                    }
                    shell.capture_event();
                }
                // Ctrl+A: Select all nodes
                keyboard::Key::Character(c) if c.as_str() == "a" && modifiers.command() => {
                    let count = self.nodes.len();
                    state.selected_nodes = (0..count).collect();
                    let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                    let selected = self.translate_node_ids(&indices);
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(selected));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                // Escape: Clear selection
                keyboard::Key::Named(keyboard::key::Named::Escape)
                    if !state.selected_nodes.is_empty() =>
                {
                    state.selected_nodes.clear();
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(vec![]));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                // Delete/Backspace handled AFTER child widgets to let text inputs consume it first
                _ => {}
            }
        }

        // Track left mouse button state globally (for Fruit Ninja edge cutting)
        if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
            state.left_mouse_down = false;
        }

        // `position_over` rejects Levitating cursors (sibling above claimed the
        // event in a `stack`) and cursors outside the graph's layout bounds.
        // Without this guard, scrolling above an overlapping widget zooms the
        // graph anyway, and the event is consumed past where it should be.
        if let Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) = event
            && let Some(cursor_pos) = screen_cursor.position_over(layout.bounds())
        {
            let cursor_pos: ScreenPoint = cursor_pos.into_euclid();

            let scroll_amount = match delta {
                mouse::ScrollDelta::Pixels { y, .. } => *y,
                mouse::ScrollDelta::Lines { y, .. } => *y * 10.0,
            };

            // Different zoom speeds for WASM vs native
            #[cfg(target_arch = "wasm32")]
            let zoom_delta = scroll_amount * 0.001 * state.camera.zoom();
            #[cfg(not(target_arch = "wasm32"))]
            let zoom_delta = scroll_amount * 0.01 * state.camera.zoom();

            state.camera = state.camera.zoom_at(cursor_pos, zoom_delta);

            // Commit the new camera (zoom shifts position too).
            if let Some(handler) = self.on_pan_handler() {
                let pos = state.camera.position();
                shell.publish(handler(Point::new(pos.x, pos.y), state.camera.zoom()));
            }

            shell.capture_event();
            shell.request_redraw();
        }

        let graph_move_offset = if let Dragging::Graph(origin) = state.dragging {
            screen_cursor.position().map(|cursor_position| {
                let cursor_world: WorldPoint = state
                    .camera
                    .screen_to_world()
                    .transform_point(cursor_position.into_euclid());
                (cursor_world - origin).into_iced()
            })
        } else {
            None
        }
        .unwrap_or(Vector::ZERO);
        // Matches draw(): children see the viewport clipped to graph bounds.
        let clipped_viewport = layout
            .bounds()
            .intersection(viewport)
            .unwrap_or(Rectangle::new(layout.bounds().position(), Size::ZERO));
        state
            .camera
            .move_by(graph_move_offset.into_euclid())
            .update_with(
                &clipped_viewport,
                screen_cursor,
                |viewport, world_cursor| {
                    let state = tree.state.downcast_mut::<NodeGraphState>();

                    if state.dragging != Dragging::None
                        && let Event::Mouse(mouse::Event::CursorMoved { .. }) = event
                    {
                        // Emit drag update event with current cursor position
                        if let Some(cursor_position) = world_cursor.position()
                            && let Some(handler) = self.on_drag_update_handler()
                        {
                            shell.publish(handler(cursor_position));
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }

                    match state.dragging.clone() {
                        Dragging::None => {}
                        Dragging::EdgeCutting { .. } => match event {
                            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                                if let Some(cursor_position) = world_cursor.position() {
                                    let cursor_position: WorldPoint = cursor_position.into_euclid();

                                    // Update trail and check which edges intersect with cutting line
                                    if let Dragging::EdgeCutting {
                                        ref mut trail,
                                        ref mut pending_cuts,
                                    } = state.dragging
                                    {
                                        trail.push(cursor_position);

                                        // Get cutting line: from start point to current cursor
                                        let cut_start =
                                            trail.first().copied().unwrap_or(cursor_position);
                                        let cut_end = cursor_position;

                                        // Clear and recalculate - only edges intersecting cutting line are highlighted
                                        pending_cuts.clear();

                                        // Check each edge for intersection with the cutting line
                                        for (edge_idx, (_id, from_ref, to_ref, _style)) in
                                            self.edges.iter().enumerate()
                                        {
                                            // Resolve user IDs to indices
                                            let from_node_idx =
                                                match self.node_index(&from_ref.node_id) {
                                                    Some(idx) => idx,
                                                    None => continue,
                                                };
                                            let to_node_idx = match self.node_index(&to_ref.node_id)
                                            {
                                                Some(idx) => idx,
                                                None => continue,
                                            };

                                            // Get pin positions and sides for bezier calculation
                                            let from_pin_data = layout
                                                .children()
                                                .nth(from_node_idx)
                                                .and_then(|node_layout| {
                                                    tree.children.get(from_node_idx).and_then(
                                                        |node_tree| {
                                                            let pins = find_pins::<P, UI>(
                                                                node_tree,
                                                                node_layout,
                                                            );
                                                            pins.iter()
                                                                .find(|(_, state, _)| {
                                                                    state.pin_id == from_ref.pin_id
                                                                })
                                                                .map(|(_, state, (pos, _))| {
                                                                    (*pos, state.side)
                                                                })
                                                        },
                                                    )
                                                });
                                            let to_pin_data = layout
                                                .children()
                                                .nth(to_node_idx)
                                                .and_then(|node_layout| {
                                                    tree.children.get(to_node_idx).and_then(
                                                        |node_tree| {
                                                            let pins = find_pins::<P, UI>(
                                                                node_tree,
                                                                node_layout,
                                                            );
                                                            pins.iter()
                                                                .find(|(_, state, _)| {
                                                                    state.pin_id == to_ref.pin_id
                                                                })
                                                                .map(|(_, state, (pos, _))| {
                                                                    (*pos, state.side)
                                                                })
                                                        },
                                                    )
                                                });

                                            if let (Some((p0, from_side)), Some((p3, to_side))) =
                                                (from_pin_data, to_pin_data)
                                            {
                                                // Calculate bezier control points
                                                let dir_from = pin_side_to_direction(from_side);
                                                let dir_to = pin_side_to_direction(to_side);
                                                let l = adaptive_bezier_length(
                                                    [p0.x, p0.y],
                                                    [p3.x, p3.y],
                                                );
                                                let p1 = Point::new(
                                                    p0.x + dir_from.0 * l,
                                                    p0.y + dir_from.1 * l,
                                                );
                                                let p2 = Point::new(
                                                    p3.x + dir_to.0 * l,
                                                    p3.y + dir_to.1 * l,
                                                );

                                                // Check if cutting line intersects this bezier edge
                                                if line_intersects_bezier(
                                                    cut_start.into_iced(),
                                                    cut_end.into_iced(),
                                                    p0,
                                                    p1,
                                                    p2,
                                                    p3,
                                                ) {
                                                    pending_cuts.insert(edge_idx);
                                                }
                                            }
                                        }
                                    }
                                }
                                shell.request_redraw();
                            }
                            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                                // Delete all pending edges on release
                                if let Dragging::EdgeCutting { pending_cuts, .. } = &state.dragging
                                {
                                    for &edge_idx in pending_cuts.iter() {
                                        if let Some((_id, from_ref, to_ref, _)) =
                                            self.edges.get(edge_idx)
                                        {
                                            // Edges already store user IDs (PinRef<N, P>)
                                            if let Some(handler) = self.on_disconnect_handler() {
                                                shell.publish(handler(
                                                    from_ref.clone(),
                                                    to_ref.clone(),
                                                ));
                                            }
                                            // Note: EdgeDisconnected message not fired for edge cutting
                                            // because edges are not registered with IDs in current design
                                        }
                                    }
                                }
                                state.dragging = Dragging::None;
                                shell.capture_event();
                                shell.request_redraw();
                            }
                            _ => {}
                        },
                        Dragging::Graph(origin) => {
                            if let Event::Mouse(mouse::Event::ButtonReleased(
                                mouse::Button::Right,
                            )) = event
                            {
                                if let Some(cursor_position) = screen_cursor.position() {
                                    let screen_to_world = state.camera.screen_to_world();
                                    let cursor_position: ScreenPoint =
                                        cursor_position.into_euclid();
                                    let cursor_position: WorldPoint =
                                        screen_to_world.transform_point(cursor_position);
                                    let offset = cursor_position - origin;
                                    state.camera = state.camera.move_by(offset);

                                    // Commit the new camera position on pan release.
                                    if let Some(handler) = self.on_pan_handler() {
                                        let pos = state.camera.position();
                                        shell.publish(handler(
                                            Point::new(pos.x, pos.y),
                                            state.camera.zoom(),
                                        ));
                                    }
                                }
                                state.dragging = Dragging::None;
                                shell.capture_event();
                                shell.request_redraw();
                            }
                        }
                        Dragging::Node(node_index, origin) => {
                            if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) =
                                event
                            {
                                if let Some(cursor_position) = world_cursor.position() {
                                    let cursor_position = cursor_position.into_euclid();
                                    let offset = cursor_position - origin;

                                    // A press+release without motion is a click, not
                                    // a move: don't emit a spurious move (which would
                                    // dirty host state / undo history on a plain
                                    // selection click). Only report an actual drag.
                                    let moved = offset.x.abs() > f32::EPSILON
                                        || offset.y.abs() > f32::EPSILON;

                                    // Translate internal index to user ID
                                    if let Some(node_id) = self.index_to_node_id(node_index)
                                        && moved
                                    {
                                        // Call on_move handler if set
                                        if let Some(handler) = self.on_move_handler() {
                                            shell.publish(handler(
                                                offset.into_iced(),
                                                vec![node_id],
                                            ));
                                        }
                                    }
                                }
                                // Promote this node to the top of the z-order on drop.
                                state.promote_z(node_index);
                                state.dragging = Dragging::None;
                                // Emit drag end event
                                if let Some(handler) = self.on_drag_end_handler() {
                                    shell.publish(handler());
                                }
                                shell.capture_event();
                                shell.invalidate_layout();
                                shell.request_redraw();
                            }
                        }
                        Dragging::Edge(from_node, from_pin, _) => match event {
                            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                                // Check if cursor is over a valid target pin to transition to EdgeOver
                                if let Some(cursor_position) = world_cursor.position() {
                                    // Copy valid_drop_targets before iterating over tree.children
                                    let valid_targets = state.valid_drop_targets.clone();

                                    // Extract from_pin_id while iterating (need access to tree.children)
                                    let mut from_pin_id: Option<P> = None;
                                    let mut from_dir: Option<PinDirection> = None;
                                    let mut target_info: Option<(usize, usize, P, PinDirection)> =
                                        None;

                                    // Check all pins for proximity and validity (use SNAP_THRESHOLD to enter)
                                    for (node_index, (node_layout, node_tree)) in
                                        layout.children().zip(&tree.children).enumerate()
                                    {
                                        for (pin_index, pin_state, (a, b)) in
                                            find_pins::<P, UI>(node_tree, node_layout)
                                        {
                                            // Extract from_pin_id when we find the source pin
                                            if node_index == from_node && pin_index == from_pin {
                                                from_pin_id = Some(pin_state.pin_id.clone());
                                                from_dir = Some(pin_state.direction);
                                            }

                                            // Pin positions are already in world space (from layout)
                                            let distance = a
                                                .distance(cursor_position)
                                                .min(b.distance(cursor_position));

                                            // Use SNAP_THRESHOLD for entering snap zone
                                            if distance < SNAP_THRESHOLD && target_info.is_none() {
                                                // Check if this pin is in valid_drop_targets
                                                if valid_targets.contains(&(node_index, pin_index))
                                                {
                                                    target_info = Some((
                                                        node_index,
                                                        pin_index,
                                                        pin_state.pin_id.clone(),
                                                        pin_state.direction,
                                                    ));
                                                }
                                            }
                                        }
                                    }

                                    if let Some((to_node, to_pin, to_pin_id, to_dir)) = target_info
                                    {
                                        // Fire EdgeConnected event immediately on snap (plug behavior)
                                        let from_node_id = self.index_to_node_id(from_node);
                                        let to_node_id = self.index_to_node_id(to_node);

                                        if let (Some(from_nid), Some(to_nid), Some(from_pid)) =
                                            (from_node_id, to_node_id, from_pin_id)
                                        {
                                            // Normalize to output -> input so the reported
                                            // endpoints match the rendered data-flow direction,
                                            // independent of which pin the drag started on.
                                            let (from_ref, to_ref) = orient_connection(
                                                from_dir.unwrap_or(PinDirection::Both),
                                                to_dir,
                                                PinRef::new(from_nid.clone(), from_pid),
                                                PinRef::new(to_nid.clone(), to_pin_id),
                                            );

                                            if let Some(handler) = self.on_connect_handler() {
                                                shell.publish(handler(from_ref, to_ref));
                                            }
                                        }

                                        state.dragging = Dragging::EdgeOver(
                                            from_node, from_pin, to_node, to_pin,
                                        );
                                    }
                                }
                                shell.request_redraw();
                            }
                            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                                state.dragging = Dragging::None;
                                // Emit drag end event
                                if let Some(handler) = self.on_drag_end_handler() {
                                    shell.publish(handler());
                                }
                                shell.capture_event();
                                shell.request_redraw();
                            }
                            _ => {}
                        },
                        Dragging::EdgeOver(from_node, from_pin, to_node, to_pin) => match event {
                            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                                // Check if still over the target pin, otherwise go back to Edge state
                                // Use UNSNAP_THRESHOLD (larger than SNAP_THRESHOLD) to prevent jitter
                                if let Some(cursor_position) = world_cursor.position() {
                                    // Extract pin IDs and check distance in one pass through tree.children
                                    let mut still_over_pin = false;
                                    let mut from_pin_id: Option<P> = None;
                                    let mut to_pin_id: Option<P> = None;
                                    let mut from_dir: Option<PinDirection> = None;
                                    let mut to_dir: Option<PinDirection> = None;

                                    for (node_index, (node_layout, node_tree)) in
                                        layout.children().zip(&tree.children).enumerate()
                                    {
                                        for (pin_index, pin_state, (a, b)) in
                                            find_pins::<P, UI>(node_tree, node_layout)
                                        {
                                            // Extract from_pin_id
                                            if node_index == from_node && pin_index == from_pin {
                                                from_pin_id = Some(pin_state.pin_id.clone());
                                                from_dir = Some(pin_state.direction);
                                            }
                                            // Extract to_pin_id and check distance
                                            if node_index == to_node && pin_index == to_pin {
                                                to_pin_id = Some(pin_state.pin_id.clone());
                                                to_dir = Some(pin_state.direction);
                                                let distance = a
                                                    .distance(cursor_position)
                                                    .min(b.distance(cursor_position));
                                                still_over_pin = distance < UNSNAP_THRESHOLD;
                                            }
                                        }
                                    }

                                    if !still_over_pin {
                                        // Fire EdgeDisconnected event when leaving snap (plug behavior)
                                        let from_node_id = self.index_to_node_id(from_node);
                                        let to_node_id = self.index_to_node_id(to_node);

                                        if let (
                                            Some(from_nid),
                                            Some(to_nid),
                                            Some(from_pid),
                                            Some(to_pid),
                                        ) = (from_node_id, to_node_id, from_pin_id, to_pin_id)
                                        {
                                            // Match the output -> input order used when the
                                            // edge connected, so the user's edge list lookup
                                            // removes the same pair it inserted.
                                            let (from_ref, to_ref) = orient_connection(
                                                from_dir.unwrap_or(PinDirection::Both),
                                                to_dir.unwrap_or(PinDirection::Both),
                                                PinRef::new(from_nid.clone(), from_pid),
                                                PinRef::new(to_nid.clone(), to_pid),
                                            );

                                            if let Some(handler) = self.on_disconnect_handler() {
                                                shell.publish(handler(from_ref, to_ref));
                                            }
                                        }

                                        // Moved away from pin, go back to dragging
                                        state.dragging = Dragging::Edge(
                                            from_node,
                                            from_pin,
                                            cursor_position.into_euclid(),
                                        );
                                    }
                                }
                                shell.request_redraw();
                            }
                            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                                // Edge already connected via snap event - just end the drag
                                state.dragging = Dragging::None;
                                // Emit drag end event
                                if let Some(handler) = self.on_drag_end_handler() {
                                    shell.publish(handler());
                                }
                                shell.capture_event();
                                shell.request_redraw();
                            }
                            _ => {}
                        },
                        Dragging::BoxSelect(start, _current) => match event {
                            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                                // Update the box selection end point
                                if let Some(cursor_position) = world_cursor.position() {
                                    state.dragging =
                                        Dragging::BoxSelect(start, cursor_position.into_euclid());
                                }
                                shell.request_redraw();
                            }
                            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                                // Complete box selection - find nodes that intersect the selection rectangle
                                if let Some(cursor_position) = world_cursor.position() {
                                    let end: WorldPoint = cursor_position.into_euclid();
                                    let selection_rect = selection_rect_from_points(start, end);

                                    // Without Shift: replace selection. With Shift: add to selection.
                                    if !state.modifiers.shift() {
                                        state.selected_nodes.clear();
                                    }

                                    // Find all nodes that intersect the selection rectangle
                                    for (node_index, node_layout) in layout.children().enumerate() {
                                        if rects_intersect(&selection_rect, &node_layout.bounds()) {
                                            state.selected_nodes.insert(node_index);
                                        }
                                    }

                                    // Notify selection change
                                    let indices: Vec<usize> =
                                        state.selected_nodes.iter().copied().collect();
                                    let selected = self.translate_node_ids(&indices);
                                    if let Some(handler) = self.on_select_handler() {
                                        shell.publish(handler(selected));
                                    }
                                }
                                state.dragging = Dragging::None;
                                // Emit drag end event
                                if let Some(handler) = self.on_drag_end_handler() {
                                    shell.publish(handler());
                                }
                                shell.capture_event();
                                shell.request_redraw();
                            }
                            _ => {}
                        },
                        Dragging::GroupMove(origin) => match event {
                            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                                shell.request_redraw();
                            }
                            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                                // Complete group move - notify all selected nodes moved
                                let indices: Vec<usize> =
                                    state.selected_nodes.iter().copied().collect();
                                if let Some(cursor_position) = world_cursor.position() {
                                    let cursor_position: WorldPoint = cursor_position.into_euclid();
                                    let offset = cursor_position - origin;

                                    // Translate internal indices to user IDs
                                    let node_ids = self.translate_node_ids(&indices);
                                    let delta = offset.into_iced();
                                    if let Some(handler) = self.on_move_handler() {
                                        shell.publish(handler(delta, node_ids));
                                    }
                                }
                                // Promote moved nodes to the top of the z-order.
                                state.promote_z_many(&indices);
                                state.dragging = Dragging::None;
                                // Emit drag end event
                                if let Some(handler) = self.on_drag_end_handler() {
                                    shell.publish(handler());
                                }
                                shell.capture_event();
                                shell.invalidate_layout();
                                shell.request_redraw();
                            }
                            _ => {}
                        },
                    }

                    // Iterate top-first so the topmost node's child widgets get a
                    // chance to capture the event before nodes below them. Without
                    // this, sliders / inputs underneath a higher-z node would
                    // consume clicks meant for the visible node on top.
                    //
                    // If the event was already captured BEFORE this loop (e.g. the
                    // parent captured CursorMoved at the top of update() during a
                    // drag), still propagate to all children — that captured-but-
                    // shared mode is how snap targets receive cursor updates while
                    // an edge is being dragged. Only short-circuit when one of the
                    // children itself takes the event.
                    let pre_captured = shell.is_event_captured();
                    for &node_index in z_indices.iter().rev() {
                        let Some((_id, _pos, element, _style, _)) = self.nodes.get_mut(node_index)
                        else {
                            continue;
                        };
                        let Some(child_tree) = tree.children.get_mut(node_index) else {
                            continue;
                        };
                        let Some(child_layout) = layout.children().nth(node_index) else {
                            continue;
                        };
                        element.as_widget_mut().update(
                            child_tree,
                            event,
                            child_layout,
                            world_cursor,
                            renderer,
                            clipboard,
                            shell,
                            viewport,
                        );
                        if !pre_captured && shell.is_event_captured() {
                            break;
                        }
                    }

                    if shell.is_event_captured() {
                        return;
                    }

                    // Delete/Backspace: Delete selected nodes.
                    // Handled AFTER child widgets so text inputs can consume the event
                    // first. Gated on on_delete: without a handler the delete cannot be
                    // persisted, so don't consume the key (let it fall through).
                    if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
                        && matches!(
                            key,
                            keyboard::Key::Named(keyboard::key::Named::Delete)
                                | keyboard::Key::Named(keyboard::key::Named::Backspace)
                        )
                        && !state.selected_nodes.is_empty()
                        && self.on_delete_handler().is_some()
                    {
                        let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        let node_ids = self.translate_node_ids(&indices);
                        if let Some(handler) = self.on_delete_handler() {
                            shell.publish(handler(node_ids));
                        }
                        state.selected_nodes.clear();
                        shell.capture_event();
                        shell.request_redraw();
                    }

                    // Only process mouse events if cursor is within our bounds
                    if !screen_cursor.is_over(layout.bounds()) {
                        return;
                    }

                    match event {
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                            // Track left mouse button state for Fruit Ninja edge cutting
                            state.left_mouse_down = true;

                            // Shift+drag from an occupied pin forks a NEW edge instead
                            // of unplugging the existing one. Captured here while `state`
                            // is still borrowable, before the pin hit-test reborrows tree.
                            let shift_held = state.modifiers.shift();

                            // Ctrl+Click: Edge cut tool
                            if state.modifiers.command()
                                && let Some(cursor_position) = world_cursor.position()
                            {
                                // Check if click is near any edge
                                for (_id, from_ref, to_ref, _style) in &self.edges {
                                    // Resolve user IDs to indices
                                    let from_node_idx = match self.node_index(&from_ref.node_id) {
                                        Some(idx) => idx,
                                        None => continue,
                                    };
                                    let to_node_idx = match self.node_index(&to_ref.node_id) {
                                        Some(idx) => idx,
                                        None => continue,
                                    };

                                    // Get pin positions for both ends of the edge
                                    let from_pin_pos = layout
                                        .children()
                                        .nth(from_node_idx)
                                        .and_then(|node_layout| {
                                            tree.children.get(from_node_idx).and_then(|node_tree| {
                                                let pins =
                                                    find_pins::<P, UI>(node_tree, node_layout);
                                                pins.iter()
                                                    .find(|(_, state, _)| {
                                                        state.pin_id == from_ref.pin_id
                                                    })
                                                    .map(|(_, _, (a, _))| *a)
                                            })
                                        });
                                    let to_pin_pos = layout.children().nth(to_node_idx).and_then(
                                        |node_layout| {
                                            tree.children.get(to_node_idx).and_then(|node_tree| {
                                                let pins =
                                                    find_pins::<P, UI>(node_tree, node_layout);
                                                pins.iter()
                                                    .find(|(_, state, _)| {
                                                        state.pin_id == to_ref.pin_id
                                                    })
                                                    .map(|(_, _, (a, _))| *a)
                                            })
                                        },
                                    );

                                    if let (Some(from_pos), Some(to_pos)) =
                                        (from_pin_pos, to_pin_pos)
                                    {
                                        // Check if cursor is near the edge line (using simple distance to line segment)
                                        let distance = point_to_line_distance(
                                            cursor_position,
                                            from_pos,
                                            to_pos,
                                        );
                                        const EDGE_CUT_THRESHOLD: f32 = 10.0;

                                        if distance < EDGE_CUT_THRESHOLD {
                                            // Edges already store user IDs
                                            if let Some(handler) = self.on_disconnect_handler() {
                                                shell.publish(handler(
                                                    from_ref.clone(),
                                                    to_ref.clone(),
                                                ));
                                            }
                                            shell.capture_event();
                                            shell.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }

                            if let Some(cursor_position) = world_cursor.position() {
                                // Per-node hit-test, top-first by z-order: check this
                                // node's pins first, then its body. The first node to
                                // own the cursor — pin OR body — wins. This way a body
                                // on top blocks click-through to a pin hidden beneath
                                // (no accidental edge-drag from a covered pin), while
                                // the snap logic during an active edge drag still sees
                                // all pins regardless of cover.
                                for &node_index in z_indices.iter().rev() {
                                    let Some(node_layout) = layout.children().nth(node_index)
                                    else {
                                        continue;
                                    };
                                    let Some(node_tree) = tree.children.get(node_index) else {
                                        continue;
                                    };
                                    let pins = find_pins::<P, UI>(node_tree, node_layout);
                                    // Get node_id for this node_index
                                    let current_node_id = match self.index_to_node_id(node_index) {
                                        Some(id) => id,
                                        None => continue,
                                    };

                                    for (pin_index, pin_state, (a, b)) in pins {
                                        // Pin positions from layout are ALREADY in world space
                                        // because layout was created with .move_to(world_position)
                                        let distance = a
                                            .distance(cursor_position)
                                            .min(b.distance(cursor_position));

                                        if distance < PIN_CLICK_THRESHOLD
                                            && !pin_state.interactions_disabled
                                        {
                                            // Check if this pin has existing connections.
                                            // Without shift, "unplug" the clicked end (like
                                            // pulling a cable). With shift held, skip the
                                            // unplug entirely and fall through to start a
                                            // fresh edge, leaving existing connections intact.
                                            if !shift_held {
                                                for (_id, from_ref, to_ref, _style) in &self.edges {
                                                    // If we clicked the "from" pin, unplug FROM and drag it
                                                    // Keep TO pin connected, drag away from it
                                                    if from_ref.node_id == current_node_id
                                                        && from_ref.pin_id == pin_state.pin_id
                                                    {
                                                        // Magnetic plug: grabbing a connected pin
                                                        // does NOT disconnect yet. Enter the snapped
                                                        // EdgeOver state anchored at the OTHER (TO)
                                                        // end; the hysteresis in the EdgeOver handler
                                                        // fires on_disconnect only once the cursor
                                                        // leaves the grabbed pin by more than
                                                        // UNSNAP_THRESHOLD.
                                                        // Resolve to_ref to indices for internal Dragging state
                                                        let to_node_idx = match self
                                                            .node_index(&to_ref.node_id)
                                                        {
                                                            Some(idx) => idx,
                                                            None => continue,
                                                        };
                                                        let to_pin_idx = {
                                                            let to_tree = match tree
                                                                .children
                                                                .get(to_node_idx)
                                                            {
                                                                Some(t) => t,
                                                                None => continue,
                                                            };
                                                            let to_layout = match layout
                                                                .children()
                                                                .nth(to_node_idx)
                                                            {
                                                                Some(l) => l,
                                                                None => continue,
                                                            };
                                                            let to_pins = find_pins::<P, UI>(
                                                                to_tree, to_layout,
                                                            );
                                                            match to_pins.iter().position(
                                                                |(_, s, _)| {
                                                                    s.pin_id == to_ref.pin_id
                                                                },
                                                            ) {
                                                                Some(idx) => idx,
                                                                None => continue,
                                                            }
                                                        };
                                                        // Compute valid targets for the new drag
                                                        let valid_targets = compute_valid_targets(
                                                            self,
                                                            tree,
                                                            layout,
                                                            to_node_idx,
                                                            to_pin_idx,
                                                            Some((from_ref, to_ref)),
                                                        );
                                                        let state = tree
                                                            .state
                                                            .downcast_mut::<NodeGraphState>();
                                                        state.valid_drop_targets = valid_targets;
                                                        // Anchor at the TO pin, hold the grabbed
                                                        // FROM pin snapped (still connected).
                                                        state.dragging = Dragging::EdgeOver(
                                                            to_node_idx,
                                                            to_pin_idx,
                                                            node_index,
                                                            pin_index,
                                                        );
                                                        shell.capture_event();
                                                        return;
                                                    }
                                                    // If we clicked the "to" pin, unplug TO and drag it
                                                    // Keep FROM pin connected, drag away from it
                                                    else if to_ref.node_id == current_node_id
                                                        && to_ref.pin_id == pin_state.pin_id
                                                    {
                                                        // Magnetic plug: grabbing a connected pin
                                                        // does NOT disconnect yet. Enter the snapped
                                                        // EdgeOver state anchored at the OTHER (FROM)
                                                        // end; the hysteresis in the EdgeOver handler
                                                        // fires on_disconnect only once the cursor
                                                        // leaves the grabbed pin by more than
                                                        // UNSNAP_THRESHOLD.
                                                        // Resolve from_ref to indices for internal Dragging state
                                                        let from_node_idx = match self
                                                            .node_index(&from_ref.node_id)
                                                        {
                                                            Some(idx) => idx,
                                                            None => continue,
                                                        };
                                                        let from_pin_idx = {
                                                            let from_tree = match tree
                                                                .children
                                                                .get(from_node_idx)
                                                            {
                                                                Some(t) => t,
                                                                None => continue,
                                                            };
                                                            let from_layout = match layout
                                                                .children()
                                                                .nth(from_node_idx)
                                                            {
                                                                Some(l) => l,
                                                                None => continue,
                                                            };
                                                            let from_pins = find_pins::<P, UI>(
                                                                from_tree,
                                                                from_layout,
                                                            );
                                                            match from_pins.iter().position(
                                                                |(_, s, _)| {
                                                                    s.pin_id == from_ref.pin_id
                                                                },
                                                            ) {
                                                                Some(idx) => idx,
                                                                None => continue,
                                                            }
                                                        };
                                                        // Compute valid targets for the new drag
                                                        let valid_targets = compute_valid_targets(
                                                            self,
                                                            tree,
                                                            layout,
                                                            from_node_idx,
                                                            from_pin_idx,
                                                            Some((from_ref, to_ref)),
                                                        );
                                                        let state = tree
                                                            .state
                                                            .downcast_mut::<NodeGraphState>();
                                                        state.valid_drop_targets = valid_targets;
                                                        // Anchor at the FROM pin, hold the grabbed
                                                        // TO pin snapped (still connected).
                                                        state.dragging = Dragging::EdgeOver(
                                                            from_node_idx,
                                                            from_pin_idx,
                                                            node_index,
                                                            pin_index,
                                                        );
                                                        shell.capture_event();
                                                        return;
                                                    }
                                                }
                                            } // end if !shift_held

                                            // No existing connection (or shift held to fork a
                                            // new edge): start a fresh drag - but only if
                                            // on_connect is wired. Without it a dropped edge
                                            // cannot persist, so let the press fall through to
                                            // node selection instead.
                                            if self.on_connect_handler().is_some() {
                                                // Compute valid targets ONCE at drag-start
                                                let valid_targets = compute_valid_targets(
                                                    self, tree, layout, node_index, pin_index, None,
                                                );
                                                let state =
                                                    tree.state.downcast_mut::<NodeGraphState>();
                                                state.valid_drop_targets = valid_targets;
                                                state.dragging = Dragging::Edge(
                                                    node_index,
                                                    pin_index,
                                                    cursor_position.into_euclid(),
                                                );
                                                // Emit drag start event
                                                if let Some(handler) = self.on_drag_start_handler()
                                                {
                                                    shell.publish(handler(DragInfo::Edge {
                                                        from_node: current_node_id.clone(),
                                                        from_pin: pin_state.pin_id.clone(),
                                                    }));
                                                }
                                                shell.capture_event();
                                                return;
                                            }
                                        }
                                    }

                                    // Body check for this same node (still top-first).
                                    if world_cursor.is_over(node_layout.bounds()) {
                                        let state = tree.state.downcast_mut::<NodeGraphState>();
                                        let already_selected =
                                            state.selected_nodes.contains(&node_index);
                                        let modifiers = state.modifiers;
                                        let selection_changed;

                                        // Handle selection based on modifiers
                                        if modifiers.shift() {
                                            // Shift+Click: Toggle selection
                                            if already_selected {
                                                state.selected_nodes.remove(&node_index);
                                            } else {
                                                state.selected_nodes.insert(node_index);
                                            }
                                            selection_changed = true;
                                        } else if !already_selected {
                                            // Regular click on unselected node: clear and select only this one
                                            state.selected_nodes.clear();
                                            state.selected_nodes.insert(node_index);
                                            selection_changed = true;
                                        } else {
                                            // Clicking on already-selected node without modifier, keep selection (for group drag)
                                            selection_changed = false;
                                        }

                                        // Get the new selection for callback
                                        let new_selection: Vec<usize> =
                                            state.selected_nodes.iter().copied().collect();

                                        // Decide between single node drag or group move -
                                        // only when on_move is wired. Node positions come
                                        // from the host, so without on_move a drag would move
                                        // the node visually then snap back on the next frame;
                                        // gate it off (selection below still fires).
                                        if self.on_move_handler().is_some() {
                                            if state.selected_nodes.len() > 1
                                                && state.selected_nodes.contains(&node_index)
                                            {
                                                // Multiple nodes selected, start group move
                                                let selected: Vec<usize> =
                                                    state.selected_nodes.iter().copied().collect();
                                                state.dragging = Dragging::GroupMove(
                                                    cursor_position.into_euclid(),
                                                );
                                                // Emit drag start event for group
                                                if let Some(handler) = self.on_drag_start_handler()
                                                {
                                                    shell.publish(handler(DragInfo::Group {
                                                        node_ids: self
                                                            .translate_node_ids(&selected),
                                                    }));
                                                }
                                            } else {
                                                // Single node drag
                                                state.dragging = Dragging::Node(
                                                    node_index,
                                                    cursor_position.into_euclid(),
                                                );
                                                // Emit drag start event for single node
                                                if let Some(handler) = self.on_drag_start_handler()
                                                    && let Some(node_id) =
                                                        self.index_to_node_id(node_index)
                                                {
                                                    shell.publish(handler(DragInfo::Node {
                                                        node_id,
                                                    }));
                                                }
                                            }
                                        }

                                        // Notify selection change
                                        if selection_changed {
                                            let selected = self.translate_node_ids(&new_selection);
                                            if let Some(handler) = self.on_select_handler() {
                                                shell.publish(handler(selected));
                                            }
                                        }

                                        shell.capture_event();
                                        return;
                                    }
                                }
                            }
                            // Nothing hit - start box selection on empty space
                            // But NOT when Ctrl is held (reserved for Fruit Ninja edge cutting)
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position: WorldPoint = cursor_position.into_euclid();
                                let state = tree.state.downcast_mut::<NodeGraphState>();

                                // Ctrl+Left: Start edge cutting mode instead of box selection
                                if state.modifiers.command() {
                                    state.dragging = Dragging::EdgeCutting {
                                        trail: vec![cursor_position],
                                        pending_cuts: std::collections::HashSet::new(),
                                    };
                                    shell.capture_event();
                                    return;
                                }

                                // Clear selection unless Shift is held
                                if !state.modifiers.shift() {
                                    state.selected_nodes.clear();
                                }

                                state.dragging =
                                    Dragging::BoxSelect(cursor_position, cursor_position);
                                // Emit drag start event for box select
                                if let Some(handler) = self.on_drag_start_handler() {
                                    shell.publish(handler(DragInfo::BoxSelect {
                                        start_x: cursor_position.x,
                                        start_y: cursor_position.y,
                                    }));
                                }
                                shell.capture_event();
                            }
                        }
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                            // Right-click: start graph panning
                            if let Some(cursor_position) = screen_cursor.position() {
                                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                                let cursor_position: WorldPoint = state
                                    .camera
                                    .screen_to_world()
                                    .transform_point(cursor_position);
                                let state = tree.state.downcast_mut::<NodeGraphState>();
                                state.dragging = Dragging::Graph(cursor_position.into_euclid());
                                shell.capture_event();
                            }
                        }
                        _ => {}
                    }
                },
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

/// Computes valid drop targets for edge dragging.
///
/// Called ONCE at drag-start to determine which pins are valid connection targets.
/// Results are stored in state.valid_drop_targets for efficient lookup during drag.
///
/// A pin is a valid target if:
/// 1. It's not the source pin (can't connect to self)
/// 2. It is not interaction-disabled
/// 3. The `can_connect` closure accepts the pair (authoritative when set);
///    otherwise [`default_can_connect`](crate::connection::default_can_connect)
///    (direction + not-same-node + one-edge-per-input) accepts it.
///
/// `excluded_edge` is the edge currently being re-routed (its endpoints), left out
/// of the occupancy check so it can be dropped back onto its own input. Pass `None`
/// when starting a fresh edge.
fn compute_valid_targets<N, P, E, UI, Message, Renderer>(
    graph: &NodeGraph<'_, N, P, UI, Message, iced::Theme, Renderer, E>,
    tree: &Tree,
    layout: Layout<'_>,
    from_node: usize,
    from_pin: usize,
    excluded_edge: Option<(&PinRef<N, P>, &PinRef<N, P>)>,
) -> std::collections::HashSet<(usize, usize)>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let mut valid_targets = std::collections::HashSet::new();

    // Get the source pin state for validation.
    let from_pin_state = tree.children.get(from_node).and_then(|node_tree| {
        layout.children().nth(from_node).and_then(|node_layout| {
            find_pins::<P, UI>(node_tree, node_layout)
                .into_iter()
                .nth(from_pin)
                .map(|(_, state, _)| state.clone())
        })
    });

    let Some(from_state) = from_pin_state else {
        return valid_targets;
    };

    let from_node_id = graph.node_id_at(from_node);

    // Pins already holding an edge, consulted by `input_not_occupied`. The edge
    // currently being dragged (when re-routing an existing connection) is excluded,
    // so its own input still reads as free and can be dropped back onto.
    let occupied: std::collections::HashSet<(&N, &P)> = graph
        .edges
        .iter()
        .filter(|(_, from, to, _)| excluded_edge != Some((from, to)))
        .flat_map(|(_, from, to, _)| [(&from.node_id, &from.pin_id), (&to.node_id, &to.pin_id)])
        .collect();
    let is_occupied = |node_id: &N, pin_id: &P| occupied.contains(&(node_id, pin_id));

    // Iterate all pins in all nodes
    for (node_index, (node_layout, node_tree)) in layout.children().zip(&tree.children).enumerate()
    {
        for (pin_index, pin_state, _) in find_pins::<P, UI>(node_tree, node_layout) {
            // Skip source pin
            if node_index == from_node && pin_index == from_pin {
                continue;
            }

            // Skip pins with disabled interactions
            if pin_state.interactions_disabled {
                continue;
            }

            let (Some(fid), Some(tid)) = (from_node_id, graph.node_id_at(node_index)) else {
                continue;
            };
            let from_end = PinEnd::new(
                fid,
                &from_state.pin_id,
                from_state.direction,
                &from_state.user_info,
                is_occupied(fid, &from_state.pin_id),
            );
            let to_end = PinEnd::new(
                tid,
                &pin_state.pin_id,
                pin_state.direction,
                &pin_state.user_info,
                is_occupied(tid, &pin_state.pin_id),
            );
            // `can_connect` is authoritative when set; otherwise the built-in default
            // (direction + not-same-node + one-edge-per-input) applies.
            let accepted = match &graph.can_connect {
                Some(can_connect) => can_connect(from_end, to_end),
                None => crate::connection::default_can_connect(from_end, to_end),
            };
            if !accepted {
                continue;
            }

            valid_targets.insert((node_index, pin_index));
        }
    }

    valid_targets
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

/// Creates a selection rectangle from two corner points (handles any corner order)
fn selection_rect_from_points(a: WorldPoint, b: WorldPoint) -> Rectangle {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = a.x.max(b.x);
    let max_y = a.y.max(b.y);
    Rectangle {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// Checks if two rectangles intersect (have any overlapping area)
fn rects_intersect(a: &Rectangle, b: &Rectangle) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

/// Calculates the distance from a point to a line segment
fn point_to_line_distance(point: Point, line_start: Point, line_end: Point) -> f32 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let line_length_sq = dx * dx + dy * dy;

    if line_length_sq < 0.001 {
        // Line segment is essentially a point
        return ((point.x - line_start.x).powi(2) + (point.y - line_start.y).powi(2)).sqrt();
    }

    // Calculate projection of point onto line
    let t = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / line_length_sq;
    let t = t.clamp(0.0, 1.0);

    // Find closest point on line segment
    let closest_x = line_start.x + t * dx;
    let closest_y = line_start.y + t * dy;

    // Return distance from point to closest point on line
    ((point.x - closest_x).powi(2) + (point.y - closest_y).powi(2)).sqrt()
}

/// Checks if a line segment intersects a cubic bezier curve.
/// Uses analytical solution by substituting bezier into line equation.
fn line_intersects_bezier(
    line_start: Point,
    line_end: Point,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
) -> bool {
    // Line in implicit form: ax + by + c = 0
    let a = line_end.y - line_start.y;
    let b = line_start.x - line_end.x;
    let c = line_end.x * line_start.y - line_start.x * line_end.y;

    // Evaluate line equation at bezier control points
    let d0 = a * p0.x + b * p0.y + c;
    let d1 = a * p1.x + b * p1.y + c;
    let d2 = a * p2.x + b * p2.y + c;
    let d3 = a * p3.x + b * p3.y + c;

    // Coefficients of cubic polynomial: at³ + bt² + ct + d = 0
    // Derived from substituting bezier B(t) into line equation
    let coef_a = -d0 + 3.0 * d1 - 3.0 * d2 + d3;
    let coef_b = 3.0 * d0 - 6.0 * d1 + 3.0 * d2;
    let coef_c = -3.0 * d0 + 3.0 * d1;
    let coef_d = d0;

    // Find roots of the cubic polynomial
    let roots = solve_cubic(coef_a, coef_b, coef_c, coef_d);

    // Check if any root in [0, 1] produces a point within the line segment
    let line_len_sq = (line_end.x - line_start.x).powi(2) + (line_end.y - line_start.y).powi(2);

    for t in roots {
        if (0.0..=1.0).contains(&t) {
            // Evaluate bezier at this t
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;
            let t2 = t * t;
            let t3 = t2 * t;

            let bx = mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x;
            let by = mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y;

            // Check if this point is within the line segment bounds
            let dx = bx - line_start.x;
            let dy = by - line_start.y;
            let proj = dx * (line_end.x - line_start.x) + dy * (line_end.y - line_start.y);

            if proj >= 0.0 && proj <= line_len_sq {
                return true;
            }
        }
    }
    false
}

/// Solves cubic equation ax³ + bx² + cx + d = 0.
/// Returns up to 3 real roots.
fn solve_cubic(a: f32, b: f32, c: f32, d: f32) -> Vec<f32> {
    const EPSILON: f32 = 1e-6;

    // Handle degenerate cases
    if a.abs() < EPSILON {
        // Quadratic: bx² + cx + d = 0
        if b.abs() < EPSILON {
            // Linear: cx + d = 0
            if c.abs() < EPSILON {
                return vec![];
            }
            return vec![-d / c];
        }
        let disc = c * c - 4.0 * b * d;
        if disc < 0.0 {
            return vec![];
        }
        let sqrt_disc = disc.sqrt();
        return vec![(-c + sqrt_disc) / (2.0 * b), (-c - sqrt_disc) / (2.0 * b)];
    }

    // Normalize: x³ + px² + qx + r = 0
    let p = b / a;
    let q = c / a;
    let r = d / a;

    // Substitute x = t - p/3 to get depressed cubic: t³ + pt + q = 0
    let p_new = q - p * p / 3.0;
    let q_new = 2.0 * p * p * p / 27.0 - p * q / 3.0 + r;

    // Cardano's formula
    let disc = q_new * q_new / 4.0 + p_new * p_new * p_new / 27.0;

    let offset = -p / 3.0;

    if disc > EPSILON {
        // One real root
        let sqrt_disc = disc.sqrt();
        let u = (-q_new / 2.0 + sqrt_disc).cbrt();
        let v = (-q_new / 2.0 - sqrt_disc).cbrt();
        vec![u + v + offset]
    } else if disc < -EPSILON {
        // Three real roots (casus irreducibilis)
        let m = (-p_new / 3.0).sqrt();
        let theta = (-q_new / (2.0 * m * m * m)).acos() / 3.0;
        let pi = std::f32::consts::PI;
        vec![
            2.0 * m * theta.cos() + offset,
            2.0 * m * (theta + 2.0 * pi / 3.0).cos() + offset,
            2.0 * m * (theta + 4.0 * pi / 3.0).cos() + offset,
        ]
    } else {
        // Double or triple root
        if q_new.abs() < EPSILON {
            vec![offset]
        } else {
            let u = (-q_new / 2.0).cbrt();
            vec![2.0 * u + offset, -u + offset]
        }
    }
}

/// Converts a PinSide to a direction vector (matches shader get_pin_direction).
fn pin_side_to_direction(side: crate::node_pin::PinSide) -> (f32, f32) {
    use crate::node_pin::PinSide;
    match side {
        PinSide::Left => (-1.0, 0.0),
        PinSide::Right => (1.0, 0.0),
        PinSide::Top => (0.0, -1.0),
        PinSide::Bottom => (0.0, 1.0),
        PinSide::Row => (1.0, 0.0), // Default to right
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
