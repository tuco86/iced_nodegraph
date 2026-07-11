//! The `draw` render path of [`NodeGraph`] and its draw-exclusive helpers.
//!
//! Split out of `widget.rs` mechanically; see the module docs there for the
//! rendering-layer overview.

use super::*;

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

impl<N, P, E, UI, Message, Renderer> NodeGraph<'_, N, P, UI, Message, iced::Theme, Renderer, E>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    /// Signature mirrors the corresponding `Widget` trait method it backs.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_impl(
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
}
