//! The `draw` render path of [`NodeGraph`]: turning resolved styles and pin
//! geometry into SDF layers, in the order they composite.
//!
//! The layer stack this builds is documented on the parent `widget` module.
//! Appearance decisions (which layers a style expands into) live in
//! [`crate::style`]; this module owns placement, culling and batching.

use super::update::{CableHit, CableZone, anchor_drag_offset, drag_carries, drag_delta};
use super::*;
use crate::node_graph::euclid::{WorldRect, WorldSize};
use crate::node_graph::state::AnchorGeometry;
use crate::style::{ColorQuad, EdgeCurve};
use iced_widget::core::{Border, Shadow};

/// Half-extent of a [`PinShape::Square`](crate::PinShape::Square) indicator, as
/// a fraction of [`PinStyle::radius`].
///
/// `sqrt(PI) / 2`, which gives the square the area of the circle it replaces -
/// so swapping a pin's shape changes its outline, not how heavy it looks next to
/// its neighbours.
const SQUARE_HALF_EXTENT: f32 = 0.886_226_9;

/// Alpha multiplier for the one empty orbit ring an anchor offers.
///
/// Visible enough to aim at, quiet enough that an idle anchor does not read as
/// having a cable on it.
const FREE_SHELL_OPACITY: f32 = 0.45;

/// How much wider than its cable the hover glow is stroked.
const GLOW_WIDTH_FACTOR: f32 = 3.0;

/// Alpha of the hover glow around the cable stretch under the cursor.
const GLOW_ALPHA: f32 = 0.45;

/// Intersects a shape's screen bounds with the widget's layout rectangle.
///
/// `None` means the shape is entirely off-screen, so the caller can skip it.
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

/// A world-space bounding box as SDF screen bounds `[x, y, width, height]`,
/// grown by `padding` world units on every side.
///
/// Applies `screen = (world + camera_position) * zoom` and the widget's own
/// screen offset.
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

/// Whether an edge's shadow is both visible and displaced, and therefore needs
/// its own geometry rather than a clone of the stroke.
fn edge_shadow_is_offset(style: &EdgeStyle) -> bool {
    style.shadow_blur > 0.0
        && (style.shadow_color.near_start.a > 0.0 || style.shadow_color.near_end.a > 0.0)
        && style.shadow_offset != (0.0, 0.0)
}

/// Pushes the SDF layers of `style` onto `batch`, choosing the stroke or shadow
/// shape per layer. Edge geometry is layout-absolute, so placement is zero.
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

/// Builds the read-only [`PinInfo`] view onto a pin state.
fn pin_info<'s, I: Ids>(state: &'s NodePinState<I::PinId, I::Payload>) -> PinInfo<'s, I> {
    PinInfo::new(state.direction, &state.pin_id, &state.user_info)
}

/// Resolves a pin's style through the theme, from the owning node's pin class.
fn resolve_pin_style<I: Ids, Theme: Catalog>(
    theme: &Theme,
    pin_class: &Theme::PinClass<'_, I>,
    state: &NodePinState<I::PinId, I::Payload>,
    other: Option<&NodePinState<I::PinId, I::Payload>>,
    status: PinStatus,
) -> PinStyle {
    let this = pin_info::<I>(state);
    let other = other.map(pin_info::<I>);
    theme.pin(pin_class, &this, other.as_ref(), status)
}

/// Circular pin cutouts that puncture a node body, translated by `(tx, ty)`.
///
/// Shared by the node fill (drag offset only) and the shadow (drag offset plus
/// shadow offset) so the shadow's holes line up exactly with the body's. The
/// radius is [`PinStyle::cutout_radius`] verbatim - the well is authored, not
/// derived from the mark that sits in it. `is_valid_target(pin_idx)` selects the
/// valid-target pin style, so a host that varies the cutout by status gets one
/// cached silhouette per status; the built-in default deliberately does not.
/// Layout-absolute `(center, radius)` of each pin cutout - the single source for the
/// recipe cuts (`ShapeExpr::Circle` at local offsets) that punch the pin holes,
/// so the body and its shadow punch identical holes.
fn pin_cutout_params<I: Ids, Theme: Catalog>(
    pins: &[PinLayout<'_, I>],
    theme: &Theme,
    pin_class: &Theme::PinClass<'_, I>,
    other: Option<&NodePinState<I::PinId, I::Payload>>,
    offset: LayoutVector,
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
            resolve_pin_style::<I, Theme>(theme, pin_class, pin_state, other, pin_status);
        let cutout_r = pin_style.cutout_radius;
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

impl<I, Message, Theme, Renderer> NodeGraph<'_, I, Message, Theme, Renderer>
where
    I: Ids,
    Theme: Catalog,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    /// Signature mirrors the corresponding `Widget` trait method it backs.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_impl(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
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
        let mut camera = state.camera_for(layout);
        // One selection read per frame, shared by the z-order and every node's
        // status, so nothing can disagree about what is selected.
        let selection = self.resolved_selection(state);
        let z_indices = z_render_indices(
            state,
            self.nodes.len(),
            |i| selection.contains(&i),
            |i| self.nodes[i].frame,
        );

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

        let resolved_graph = theme.graph(&self.graph_class);

        // Check if we're edge dragging
        let is_edge_dragging = matches!(
            state.dragging,
            Dragging::Edge {
                from_node: _,
                from_pin: _,
                origin: _
            } | Dragging::EdgeOver {
                from_node: _,
                from_pin: _,
                to_node: _,
                to_pin: _
            }
        );

        // Per-frame pin table, built ONCE per node: `find_pins` is a
        // Vec-allocating tree walk, and the drag preview, node geometry,
        // edge loop, foreground and info passes all need pin positions.
        // Indexed by node index; padded so lookups never go out of bounds.
        let mut node_pins: Vec<Vec<PinLayout<'_, I>>> = layout
            .children()
            .zip(&tree.children)
            .map(|(node_layout, node_tree)| find_pins::<I>(node_tree, node_layout))
            .collect();
        node_pins.resize_with(self.nodes.len(), Vec::new);

        // The pin an edge drag started from, surfaced as `other` to pin_style so
        // candidate pins can react to what is being dragged toward them.
        let drag_source: Option<NodePinState<I::PinId, I::Payload>> = match state.dragging {
            Dragging::Edge {
                from_node,
                from_pin,
                origin: _,
            }
            | Dragging::EdgeOver {
                from_node,
                from_pin,
                to_node: _,
                to_pin: _,
            } => node_pins
                .get(from_node)
                .and_then(|pins| pins.get(from_pin))
                .map(|(_, s, _)| (*s).clone()),
            _ => None,
        };

        // ========================================
        // Layer 1: Background (solid color)
        // ========================================
        renderer.with_layer(layout.bounds(), |renderer| {
            renderer.fill_quad(
                iced_wgpu::core::renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border::default(),
                    shadow: Shadow::default(),
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
        let cursor_layout = |cursor_pos: Point| -> LayoutPoint {
            camera.world_to_layout(
                camera
                    .screen_to_world()
                    .transform_point(cursor_pos.into_euclid()),
            )
        };
        // One delta per frame, shared by every node the drag carries, so the
        // preview cannot disagree with the delta the release publishes.
        let shared_delta = cursor
            .position()
            .and_then(|cursor_pos| drag_delta(state, self, cursor_layout(cursor_pos)));
        let compute_node_offset = |node_idx: usize| -> LayoutVector {
            match shared_delta {
                Some(delta) if drag_carries(state, node_idx, |i| selection.contains(&i)) => delta,
                _ => LayoutVector::zero(),
            }
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
            offset: LayoutVector,
            position: LayoutPoint,
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
                let node = &self.nodes[node_index];
                let node_layout = layout.children().nth(node_index)?;
                // Gate only: a node without a tree child gets no geometry
                // (its pins are already absent from `node_pins`).
                let _node_tree = tree.children.get(node_index)?;
                let status = if selection.contains(&node_index) {
                    NodeStatus::Selected
                } else {
                    NodeStatus::Idle
                };
                let resolved = theme.node(&node.class, status);
                let offset = compute_node_offset(node_index);
                let position: LayoutPoint =
                    (node_layout.bounds().position().into_euclid().to_vector() + offset).to_point();
                let size = node_layout.bounds().size();
                let pins = &node_pins[node_index];
                let center = [
                    position.x + size.width * 0.5,
                    position.y + size.height * 0.5,
                ];
                let cut_params = pin_cutout_params::<I, Theme>(
                    pins,
                    theme,
                    &node.pin_class,
                    drag_source.as_ref(),
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

        // Anchors resolve once per frame, before any cable is built:
        // `orbit_offset`/`orbit_spacing` are the radii the paths below are laid
        // tangent to, so the style has to exist first. Host positions are
        // world-space; edge endpoints in this pass are layout-absolute, so fold
        // the viewport origin in here and nowhere else.
        //
        // A drag in flight is previewed here, so the cables attached to the
        // anchor follow it live: their tangent points are recomputed from this
        // centre every frame.
        let anchor_drag = match state.dragging {
            Dragging::Anchor { anchor, origin } => cursor.position().map(|p| {
                let raw = camera.screen_to_world().transform_point(p.into_euclid()) - origin;
                (anchor, anchor_drag_offset(state, self, anchor, raw))
            }),
            _ => None,
        };
        let anchor_layouts: Vec<[f32; 2]> = self
            .anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| {
                let mut p = camera.world_to_layout(anchor.position.into_euclid());
                if let Some((dragged, offset)) = anchor_drag
                    && dragged == index
                {
                    p += LayoutVector::new(offset.x, offset.y);
                }
                [p.x, p.y]
            })
            .collect();
        let phantom = match &state.dragging {
            Dragging::Route { edge, detached } => cursor.position().map(|p| {
                let center = cursor_layout(p);
                RoutePhantom {
                    edge: *edge,
                    exclude: *detached,
                    kind: PhantomKind::At {
                        center: [center.x, center.y],
                        radius: theme
                            .anchor(&Theme::default_anchor(), AnchorStatus::Idle)
                            .orbit_radius(0),
                    },
                }
            }),
            Dragging::RouteOver {
                edge,
                anchor,
                detached,
            } => Some(RoutePhantom {
                edge: *edge,
                exclude: *detached,
                kind: PhantomKind::Snap { anchor: *anchor },
            }),
            _ => None,
        };
        // Derived through the drag's own pending edit, so the ring a preview
        // draws and the ring the interaction path measures against are the same
        // circle.
        let rings = self.anchor_rings(phantom.as_ref().map(RoutePhantom::pending));

        // Feedback before the gesture: the anchor a press would act on renders
        // hovered, and the cable stretch under the cursor glows below.
        //
        // The hit tests read the orbit radii the LAST frame published, because
        // this frame's come from the very styles the hover decides. They differ
        // only on a frame where the host changed an anchor style.
        let hover_at = match state.dragging {
            Dragging::None => cursor.position_over(layout.bounds()).map(cursor_layout),
            _ => None,
        };
        let cable_hit: Option<CableHit> =
            hover_at.and_then(|at| self.cable_hit_at(tree, layout, at));
        let hovered_anchor = hover_at
            .and_then(|at| self.anchor_core_at(tree, at))
            .or_else(|| {
                cable_hit.as_ref().and_then(|hit| match &hit.zone {
                    CableZone::Wrap { anchor, .. } => Some(*anchor),
                    CableZone::End { .. } | CableZone::Run { .. } => None,
                })
            });
        let route_drag = match &state.dragging {
            Dragging::Route { edge, detached } => Some((*edge, *detached, None)),
            Dragging::RouteOver {
                edge,
                anchor,
                detached,
            } => Some((*edge, *detached, Some(*anchor))),
            _ => None,
        };
        let snap_eligible = route_drag
            .map(|(edge, detached, _)| self.route_snap_eligible(edge, detached))
            .unwrap_or_default();
        // The anchor a snapped route drag is currently attached to.
        let snapped_anchor = route_drag.and_then(|(_, _, anchor)| anchor);
        let anchor_styles: Vec<AnchorStyle> = self
            .anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| {
                let status = if snap_eligible.contains(&index) {
                    AnchorStatus::ValidTarget
                } else if hovered_anchor == Some(index) {
                    AnchorStatus::Hovered
                } else {
                    AnchorStatus::Idle
                };
                theme.anchor(&anchor.class, status)
            })
            .collect();
        // Hand the resolved core and radii to the interaction path, which has no
        // theme to resolve them from itself.
        state.anchor_geometry.replace(
            anchor_styles
                .iter()
                .map(|style| AnchorGeometry {
                    core_size: style.core_size,
                    orbit_offset: style.orbit_offset,
                    orbit_spacing: style.orbit_spacing,
                })
                .collect(),
        );

        let phantom_visual = match &state.dragging {
            Dragging::Route { .. } => cursor.position().map(|p| {
                let center = cursor_layout(p);
                (
                    [center.x, center.y],
                    theme.anchor(&Theme::default_anchor(), AnchorStatus::Hovered),
                )
            }),
            _ => None,
        };

        // Graph background: ONE batched SDF draw under all nodes. Within a single
        // SDF primitive the FIRST-pushed entry composites in FRONT (the cull
        // sorts slots ascending by push index and the fragment blends them
        // front-to-back), so entries are pushed FRONT-TO-BACK here: anchors
        // (z3, top), edge strokes (z2), all shadows (z1, edge + node), then the
        // grid (z0, bottom). Folding the grid, every node
        // shadow, every edge and every anchor into one primitive collapses the
        // whole below-nodes layer into a single fullscreen fragment pass.
        // Pushing ALL strokes before ANY shadow keeps
        // every edge line above every shadow. The node bodies (Layer 4) paint
        // over all of it. The grid is not marked cacheable: it shares this draw
        // with the dynamic shadows and edges, so the static-background texture
        // cache could never hit. Node shadows within the z1 band are pushed in
        // STABLE node-index order rather than the selection-sorted `z_indices`
        // (see below) - bg_layer is a single SdfPrimitive whose geometry hash
        // covers entry push order, so ordering shadows by `z_indices` would
        // make every selection click re-hash and rebuild the whole background
        // (all edge biarcs included) just to reshuffle translucent shadows
        // that composite the same either way.
        // For the same reason the batch is not viewport-culled: its entries are
        // world-anchored, so under pan and zoom only the camera uniform moves
        // and the geometry hash stays put, letting the prepare step reuse the
        // compiled buffers. Dropping offscreen edges or shadows would make the
        // entry set camera-dependent and re-upload every biarc each pan frame,
        // while the shading cost it would save is already bounded by the
        // shader's per-tile segment cull and the primitive's own scissor.
        // ========================================
        // The hover glow is deliberately NOT part of `bg_layer`: its slice
        // window tracks the cursor, so folding it in would change the batch's
        // geometry hash on every mouse-move frame and re-upload every biarc,
        // grid tile and node shadow with it - a strictly larger rebuild than
        // the selection-click case the batch is arranged to avoid. It rides
        // above the background instead, like the dragged-edge preview.
        let mut hover_glow: Option<(Shape, Style)> = None;
        let bg_layer = {
            let occupied_rings: usize = rings.iter().sum();
            // One dimmed shell per anchor the drag could still take. The anchor
            // it already sits on offers none, and `snap_eligible` is empty
            // without a route drag.
            let offered_rings = snap_eligible
                .iter()
                .filter(|anchor| snapped_anchor != Some(**anchor))
                .count();
            let phantom_layers = if phantom_visual.is_some() { 3 } else { 0 };
            let mut bg = SdfPrimitive::with_capacity(
                self.nodes.len()
                    + self.edges.len() * 4
                    + self.anchors.len() * 3
                    + occupied_rings
                    + offered_rings
                    + phantom_layers
                    + 1,
            );
            // Edge layers split by geometry: strokes (z2) collected to push first
            // (front), shadows (z1) collected to push behind them. Each edge's own
            // layer order is preserved within each group.
            let pending_cuts = match &state.dragging {
                Dragging::EdgeCutting { pending_cuts, .. } => Some(pending_cuts),
                _ => None,
            };
            let mut edge_strokes: Vec<(Shape, Style)> = Vec::with_capacity(self.edges.len() * 2);
            let mut edge_shadows: Vec<(Shape, Style)> = Vec::with_capacity(self.edges.len());

            let pin = |pin: &PinRef<I>| -> Option<Station> {
                let node_idx = self.node_index(&pin.node_id)?;
                let (_, pin_state, (pin_pos, _)) = node_pins[node_idx]
                    .iter()
                    .find(|(_, state, _)| state.pin_id == pin.pin_id)?;
                let point =
                    (pin_pos.into_euclid().to_vector() + compute_node_offset(node_idx)).to_point();
                Some(Station {
                    point: [point.x, point.y],
                    side: pin_state.side.into(),
                    direction: Some(pin_state.direction),
                })
            };
            let ring = |anchor: usize, orbit: u8| -> Option<edge_path::Orbit> {
                Some(edge_path::Orbit {
                    center: *anchor_layouts.get(anchor)?,
                    radius: anchor_styles.get(anchor)?.orbit_radius(orbit),
                })
            };
            let end_info = |pin: &PinRef<I>| -> Option<PinInfo<'_, I>> {
                let node_idx = self.node_index(&pin.node_id)?;
                let (_, pin_state, _) = node_pins[node_idx]
                    .iter()
                    .find(|(_, state, _)| state.pin_id == pin.pin_id)?;
                Some(pin_info::<I>(pin_state))
            };
            let hit_edge = cable_hit.as_ref().map(|hit| match &hit.zone {
                CableZone::End { edge, .. }
                | CableZone::Wrap { edge, .. }
                | CableZone::Run { edge } => *edge,
            });

            // `edge_hops` skips any edge whose endpoint pin does not resolve, so
            // the published curves are placed BY edge index, not appended in
            // iteration order; a skipped edge keeps the default.
            let mut edge_curves = vec![EdgeCurve::default(); self.edges.len()];
            // The orbit assignment builds candidate arrangements to count their
            // crossings, so it needs each leg's shape - and it reads the curve
            // the LAST frame published rather than one resolved here. The style
            // closure takes the two endpoint `PinInfo`s in ORIENTED order and
            // that orientation is derived inside `edge_hops`, so resolving ahead
            // of the call would either duplicate the derivation or hand it the
            // wrong order; and the hit test reads this same vector, which is
            // what makes the ring order it measures against the ring order that
            // gets stroked. A host switching an edge's curve therefore settles
            // the ring assignment one frame later. The stroke itself still uses
            // this frame's resolved curve. The borrow ends with this block, well
            // before the vector is replaced below.
            let cables = {
                let published = state.edge_curves.borrow();
                let curve = |edge: usize| published.get(edge).copied().unwrap_or_default();
                self.edge_hops(&pin, &ring, &curve, phantom.as_ref())
            };
            for geometry in cables {
                let edge = &self.edges[geometry.edge];
                let edge_status = if pending_cuts.is_some_and(|cuts| cuts.contains(&geometry.edge))
                {
                    EdgeStatus::PendingCut
                } else {
                    EdgeStatus::Idle
                };
                // `edge_hops` already drops an edge whose endpoint does not
                // resolve, so this never skips in practice.
                let Some((from, to)) = end_info(geometry.ends.0).zip(end_info(geometry.ends.1))
                else {
                    continue;
                };
                let edge_style = theme.edge(&edge.class, edge_status, from, to);

                let built = edge_path::build(&geometry.hops, &edge_style.curve);
                edge_curves[geometry.edge] = edge_style.curve;
                if hit_edge == Some(geometry.edge)
                    && let Some(hit) = cable_hit.as_ref()
                {
                    let glow_path = built.path.slice(hit.window.0, hit.window.1);
                    if !glow_path.segs.is_empty() {
                        hover_glow = Some((
                            glow_path.into_shape(),
                            Style::quad_stroke(
                                &edge_style.stroke_color.with_opacity(GLOW_ALPHA),
                                Pattern::solid(edge_style.pattern.thickness * GLOW_WIDTH_FACTOR),
                            ),
                        ));
                    }
                }
                let shape = built.path.into_shape();
                // Translation commutes with the path construction, so shifting
                // the finished cable is the same geometry as building it from
                // shifted endpoints.
                let shadow_shape = if edge_shadow_is_offset(&edge_style) {
                    let (ox, oy) = edge_style.shadow_offset;
                    shape.clone().translate([ox, oy])
                } else {
                    shape.clone()
                };

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
            // Hand the resolved curves to the hit test and to next frame's orbit
            // assignment, neither of which has a theme to resolve a style with.
            state.edge_curves.replace(edge_curves);

            // z3: anchor cores and orbit rings, in front of the cables wrapping
            // them (the node bodies of Layer 4 still cover both). The core is an
            // axis-aligned rounded box: the SDF has no rotate op, so a diamond
            // would need a primitive that does not exist.
            if let Some((center, style)) = &phantom_visual {
                if style.ring_width > 0.0 {
                    let ring = Shape::circle(style.orbit_radius(0)).translate(*center);
                    bg.push(
                        &ring,
                        &Style::quad_stroke(&style.ring_color, Pattern::solid(style.ring_width)),
                        [0.0, 0.0],
                    );
                }
                let core = Shape::rounded_box([style.core_size; 2], [style.core_radius; 4])
                    .translate(*center);
                bg.push(
                    &core,
                    &Style::quad_band(&style.core_color, -1e6, 0.0),
                    [0.0, 0.0],
                );
                if style.core_border_width > 0.0 {
                    bg.push(
                        &core,
                        &Style::quad_band(&style.core_border_color, -1e6, 0.0)
                            .expand(style.core_border_width),
                        [0.0, 0.0],
                    );
                }
            }
            for (anchor_idx, style) in anchor_styles.iter().enumerate() {
                let center = anchor_layouts[anchor_idx];

                if style.ring_width > 0.0 {
                    let occupied = rings.get(anchor_idx).copied().unwrap_or(0);
                    for orbit in 0..occupied {
                        let Ok(orbit) = u8::try_from(orbit) else {
                            break;
                        };
                        let ring = Shape::circle(style.orbit_radius(orbit)).translate(center);
                        bg.push(
                            &ring,
                            &Style::quad_stroke(
                                &style.ring_color,
                                Pattern::solid(style.ring_width),
                            ),
                            [0.0, 0.0],
                        );
                    }
                    // An anchor the drag could still take shows the ring the
                    // cable would land on, dimmed and one step outside every
                    // ring it already carries. The anchor the drag is ALREADY on
                    // gets none: `pending` folds the drag into that anchor's
                    // occupancy, so the loop above has just stroked the very
                    // ring the cable rides.
                    if snapped_anchor != Some(anchor_idx)
                        && snap_eligible.contains(&anchor_idx)
                        && let Ok(offered) = u8::try_from(occupied)
                    {
                        let ring = Shape::circle(style.orbit_radius(offered)).translate(center);
                        bg.push(
                            &ring,
                            &Style::quad_stroke(
                                &style.ring_color.with_opacity(FREE_SHELL_OPACITY),
                                Pattern::solid(style.ring_width),
                            ),
                            [0.0, 0.0],
                        );
                    }
                }

                let core = Shape::rounded_box([style.core_size; 2], [style.core_radius; 4])
                    .translate(center);
                bg.push(
                    &core,
                    &Style::quad_band(&style.core_color, -1e6, 0.0),
                    [0.0, 0.0],
                );
                if style.core_border_width > 0.0 {
                    bg.push(
                        &core,
                        &Style::quad_band(&style.core_border_color, -1e6, 0.0)
                            .expand(style.core_border_width),
                        [0.0, 0.0],
                    );
                }
            }

            // z2: edge strokes (frontmost in the background layer).
            for (shape, style) in &edge_strokes {
                bg.push(shape, style, [0.0, 0.0]);
            }

            // z1: shadows behind the strokes - edge layers, then node shadows
            // (all above the grid and below every edge line).
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
                // Thickness is baked into the tiling SDF (params.z) for all
                // kinds: grid/triangle/hex subtract half the line thickness,
                // dots bake the radius in.
                let style = Style::solid(tiling.color);
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

        // Hover glow: its own small primitive above the background, so a cursor
        // move re-uploads one sliced stroke instead of the whole batch. It
        // therefore paints OVER the cable rather than under it, which is not a
        // visible change: the glow carries the stroke's own color, so the line
        // gains a wash of itself and no hue shift.
        if let Some((shape, style)) = &hover_glow {
            let wo = layout.bounds().position();
            let (cx, cy) = layer_camera(
                render_context.camera_position,
                render_context.camera_zoom,
                wo,
                layout.bounds(),
            );
            let mut glow_batch = SdfPrimitive::new();
            glow_batch.push(shape, style, [0.0, 0.0]);
            renderer.with_layer(layout.bounds(), |renderer| {
                draw_sdf(
                    renderer,
                    &state.sdf_animated,
                    layout.bounds(),
                    glow_batch
                        .camera(cx, cy, render_context.camera_zoom)
                        .time(render_context.time),
                );
            });
        }

        // Dragging edge (single primitive, only during interaction). Kept as its
        // own draw above the background but below the nodes, matching its prior
        // z-position; it is never folded into the background batch.
        if let Dragging::Edge {
            from_node: from_node_idx,
            from_pin: from_pin_idx,
            origin: _,
        } = &state.dragging
            && let Some(cursor_pos) = cursor.position()
        {
            let from_pins = &node_pins[*from_node_idx];
            if let Some((_, from_pin_state, (from_pin_pos, _))) = from_pins.get(*from_pin_idx) {
                let from_offset = compute_node_offset(*from_node_idx);
                let start_pos = (from_pin_pos.into_euclid().to_vector() + from_offset).to_point();
                // Loose end follows the cursor in the same layout-absolute space
                // as the pin geometry so the dragged edge stays aligned when the
                // graph is off the window origin.
                let end_pos: LayoutPoint = cursor_layout(cursor_pos);

                let drag_edge_style =
                    theme.drag_edge(&self.drag_edge_class, pin_info::<I>(from_pin_state));

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
                let hops = [
                    edge_path::Hop::Pin {
                        point: [start_pos.x, start_pos.y],
                        side: start_side,
                    },
                    edge_path::Hop::Pin {
                        point: [end_pos.x, end_pos.y],
                        side: end_side,
                    },
                ];
                let shape = edge_path::build(&hops, &drag_edge_style.curve)
                    .path
                    .into_shape();
                let shadow_shape = if edge_shadow_is_offset(&drag_edge_style) {
                    let (ox, oy) = drag_edge_style.shadow_offset;
                    shape.clone().translate([ox, oy])
                } else {
                    shape.clone()
                };

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
            let node = &self.nodes[node_index];
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
            let pins = &node_pins[node_index];

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
                        // The border grows outward from the silhouette, so the
                        // content area is the same width at any border
                        // thickness: selecting a node cannot reflow its
                        // interior.
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
                                node.element.as_widget().draw(
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

            // Layer 4c: Node Foreground (border + pins + resize grip batched)
            let has_border = resolved.border_pattern.thickness > 0.0;
            let has_pins = !pins.is_empty();
            // Drawn on exactly the condition the press honours, so the
            // affordance never promises an interaction that is not wired.
            let has_grip = node.resizable && self.on_resize.is_some();

            if has_border || has_pins || has_grip {
                let mut fg_batch = SdfPrimitive::with_capacity(pins.len() * 2 + 4);
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
                    let pin_style = resolve_pin_style::<I, Theme>(
                        theme,
                        &node.pin_class,
                        pin_state,
                        drag_source.as_ref(),
                        pin_status,
                    );
                    let indicator_r = pin_style.radius;
                    let pin_layout: LayoutPoint =
                        (pin_pos.into_euclid().to_vector() + offset).to_point();
                    let pw = [pin_layout.x, pin_layout.y];

                    // Pin shapes are centred on the pin, and so is every
                    // primitive's origin, so the placement is just the pin
                    // position - and identical pins share a recipe.
                    let (pin_shape, pin_place) = match pin_style.shape {
                        crate::style::PinShape::Square => {
                            let h = indicator_r * SQUARE_HALF_EXTENT;
                            (Shape::rounded_box([2.0 * h, 2.0 * h], [0.0; 4]), pw)
                        }
                        crate::style::PinShape::Circle => (Shape::circle(indicator_r), pw),
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
                        pin_layout.x - pin_pad,
                        pin_layout.y - pin_pad,
                        pin_layout.x + pin_pad,
                        pin_layout.y + pin_pad,
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

                // Resize grip: two ticks parallel to the corner bevel, filling
                // the same square `resize_grip_zone` hit-tests. It borrows the
                // node's border color rather than adding a style knob - the
                // grip is part of the node's outline, not a separate element.
                if has_grip {
                    let zone = resize_grip_zone(
                        Rectangle::new(Point::new(node_position.x, node_position.y), node_size),
                        cam_zoom,
                    );
                    // Screen-pixel width like every other outline the widget
                    // draws itself, so the ticks stay legible at any zoom.
                    let tick_width = 1.5 / cam_zoom;
                    let grip_style = Style::quad_stroke(
                        &resolved.border_color.with_opacity(opacity),
                        Pattern::solid(tick_width),
                    );
                    // Shapes are local to the corner, so every resizable node
                    // of the same size shares one recipe.
                    let corner = [zone.x + zone.width, zone.y + zone.height];
                    let inset = zone.width * 0.2;
                    for reach in [zone.width * 0.9, zone.width * 0.5] {
                        fg_batch.push(
                            &Shape::line([-reach, -inset], [-inset, -reach]),
                            &grip_style,
                            corner,
                        );
                    }

                    // Ticks are open strokes: half the width spills to each side.
                    let grip_pad = tick_width * 0.5 + 2.0 / cam_zoom;
                    let grip_bounds = world_bbox_to_screen_bounds(
                        zone.x,
                        zone.y,
                        zone.x + zone.width,
                        zone.y + zone.height,
                        grip_pad,
                        &render_context,
                    );
                    fg_min_x = fg_min_x.min(grip_bounds[0]);
                    fg_min_y = fg_min_y.min(grip_bounds[1]);
                    fg_max_x = fg_max_x.max(grip_bounds[0] + grip_bounds[2]);
                    fg_max_y = fg_max_y.max(grip_bounds[1] + grip_bounds[3]);
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
        // Layer N+1: the selection box
        // ========================================
        if let Dragging::SelectionBox(start, _end) = &state.dragging {
            // `start` was captured in layout-absolute space (the event closure's
            // cursor), so the live corner must match that space.
            let cursor_world = cursor.position().map(cursor_layout).unwrap_or(*start);

            let selection_box = theme.selection_box(&self.selection_box_class);

            let center = [
                (start.x + cursor_world.x) * 0.5,
                (start.y + cursor_world.y) * 0.5,
            ];
            let half_size = [
                ((cursor_world.x - start.x) * 0.5).abs(),
                ((cursor_world.y - start.y) * 0.5).abs(),
            ];
            // Declared in screen pixels, so the outline reads the same at any zoom.
            let border_width = selection_box.border_width / camera.zoom();

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
                    &Style::stroke(selection_box.border_color, Pattern::solid(border_width)),
                    select_place,
                );
                select_batch.push(
                    &select_shape,
                    &Style::solid(selection_box.fill),
                    select_place,
                );

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

            let cut_style = theme.cutting_tool(&self.cutting_tool_class);
            // Screen pixels, like the selection box outline.
            let cut_width = cut_style.width / render_context.camera_zoom;

            let cutting_bounds = world_bbox_to_screen_bounds(
                start.x,
                start.y,
                cursor_world.x,
                cursor_world.y,
                cut_width + 2.0 / render_context.camera_zoom,
                &render_context,
            );

            if let Some(cutting_clip) = clipped_shape_bounds(cutting_bounds, layout.bounds()) {
                let mut cutting_batch = SdfPrimitive::new();
                cutting_batch.push(
                    &Shape::line([start.x, start.y], [cursor_world.x, cursor_world.y]),
                    &Style::stroke(cut_style.color, Pattern::solid(cut_width)),
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
                let pin_count = node_pins[i].len();
                pins_total += pin_count;
                if in_view {
                    pins_in += pin_count;
                }
            }
            // An anchor's footprint is its core, grown to the outermost occupied
            // orbit. Offered rings are transient interaction feedback, not graph
            // contents counted by the diagnostics.
            let anchor_in_view: Vec<bool> = (0..self.anchors.len())
                .map(|index| {
                    let style = &anchor_styles[index];
                    let half = rings[index]
                        .checked_sub(1)
                        .and_then(|orbit| u8::try_from(orbit).ok())
                        .map_or(style.core_size * 0.5, |orbit| {
                            style.orbit_radius(orbit) + style.ring_width
                        });
                    let center = anchor_layouts[index];
                    let bb = world_bbox_to_screen_bounds(
                        center[0] - half,
                        center[1] - half,
                        center[0] + half,
                        center[1] + half,
                        0.0,
                        &render_context,
                    );
                    Rectangle {
                        x: bb[0],
                        y: bb[1],
                        width: bb[2],
                        height: bb[3],
                    }
                    .intersects(&viewport)
                })
                .collect();
            let anchors_in = anchor_in_view.iter().filter(|in_view| **in_view).count();
            let edges_in = self
                .edges
                .iter()
                .filter(|edge| {
                    let visible = |id| self.node_index(id).is_some_and(|idx| node_in_view[idx]);
                    visible(&edge.from.node_id) || visible(&edge.to.node_id)
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
                anchors: counts(self.anchors.len(), anchors_in),
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
                sdf_upload_bytes: sdf.upload_bytes,
                sdf_gpu_bytes: sdf.gpu_bytes,
                sdf_index_bytes: sdf.index_bytes,
                sdf_draws: sdf.sdf_draws,
                sdf_shaded_px: sdf.shaded_px,
                sdf_segment_evals: sdf.segment_evals,
                sdf_fine_slots_max: sdf.fine_slots_max,
                sdf_fine_evicted_tiles: sdf.fine_evicted_tiles,
                sdf_index_traffic_bytes: sdf.index_traffic_bytes,
                sdf_cull_skipped: sdf.cull_skipped,
            };
            state.last_info.replace(Some(info));
        }

        // ========================================
        // Layer N+3: Minimap overlay
        // ========================================
        // Screen space, on top of everything, no camera transform: the map is
        // chrome pinned to a corner of the widget, not content on the canvas.
        if let Some(minimap) = self.minimap.as_ref() {
            let style = theme.minimap(&self.minimap_class);
            // Node geometry is layout-absolute and carries this frame's drag
            // offset, so a dragged node moves on the map with its body.
            let node_world = |geom: &NodeGeom| {
                WorldRect::new(
                    camera.layout_to_world(geom.position),
                    WorldSize::new(geom.size.width, geom.size.height),
                )
            };
            let visible = camera.visible_world_rect(layout.bounds());
            let map = minimap::rect(minimap, layout.bounds());
            let projection = minimap::Projection::new(
                map,
                minimap::world_bounds(node_geoms.iter().flatten().map(node_world), visible),
            );
            renderer.with_layer(layout.bounds(), |renderer| {
                minimap::draw(
                    renderer,
                    map,
                    &projection,
                    &style,
                    node_geoms.iter().enumerate().filter_map(|(index, geom)| {
                        Some((node_world(geom.as_ref()?), selection.contains(&index)))
                    }),
                    visible,
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use iced_wgpu::core::widget::Widget;
    use iced_widget::core::{Background, Color, Element, Theme, Transformation, image};

    use crate::style::EdgeStyle;
    use crate::{PinDirection, PinRef, PinSide, default_edge_style, edge, node, node_pin};

    /// Satisfies the renderer bounds [`NodeGraph`] imposes and keeps nothing:
    /// what these tests read is widget state, not renderer output.
    #[derive(Debug)]
    struct NullRenderer;

    impl renderer::Renderer for NullRenderer {
        fn start_layer(&mut self, _bounds: Rectangle) {}
        fn end_layer(&mut self) {}
        fn start_transformation(&mut self, _transformation: Transformation) {}
        fn end_transformation(&mut self) {}
        fn fill_quad(&mut self, _quad: renderer::Quad, _background: impl Into<Background>) {}
        fn reset(&mut self, _new_bounds: Rectangle) {}
        fn allocate_image(
            &mut self,
            _handle: &image::Handle,
            _callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
        ) {
        }
    }

    impl iced_wgpu::primitive::Renderer for NullRenderer {
        fn draw_primitive(&mut self, _bounds: Rectangle, _primitive: impl iced_wgpu::Primitive) {}
    }

    /// A pin needs an element to wrap; nothing here depends on which.
    struct Blank;

    impl<Message> Widget<Message, Theme, NullRenderer> for Blank {
        fn size(&self) -> Size<Length> {
            Size::new(Length::Fixed(8.0), Length::Fixed(8.0))
        }

        fn layout(
            &mut self,
            _tree: &mut Tree,
            _renderer: &NullRenderer,
            limits: &layout::Limits,
        ) -> layout::Node {
            layout::Node::new(limits.resolve(Length::Fixed(8.0), Length::Fixed(8.0), Size::ZERO))
        }

        fn draw(
            &self,
            _tree: &Tree,
            _renderer: &mut NullRenderer,
            _theme: &Theme,
            _style: &renderer::Style,
            _layout: layout::Layout<'_>,
            _cursor: mouse::Cursor,
            _viewport: &Rectangle,
        ) {
        }
    }

    impl<'a, Message: 'a> From<Blank> for Element<'a, Message, Theme, NullRenderer> {
        fn from(blank: Blank) -> Self {
            Element::new(blank)
        }
    }

    fn styled_edge(from: PinRef, to: PinRef, curve: EdgeCurve) -> Edge<'static> {
        edge((), from, to).style(move |theme, status, _from, _to| EdgeStyle {
            curve,
            ..default_edge_style(theme, status)
        })
    }

    /// `draw` resolves the theme-dependent curve per edge; the interaction path
    /// and the orbit assignment have no theme, so both read what `draw`
    /// published. `edge_hops` yields only the edges whose endpoint pins resolve,
    /// so the vector has to be keyed by edge index rather than filled in
    /// iteration order. Both ways an endpoint fails to resolve are here: edge 0
    /// names a pin id its node does not own, edge 2 names a node that was never
    /// pushed, and the two survivors around them carry different curves.
    #[test]
    fn draw_publishes_every_edge_curve_by_edge_index() {
        const SIZE: Size = Size {
            width: 400.0,
            height: 300.0,
        };

        let mut graph: NodeGraph<'static, Indexed, (), Theme, NullRenderer> = NodeGraph::new()
            .width(Length::Fixed(SIZE.width))
            .height(Length::Fixed(SIZE.height))
            .push_node(node(
                0,
                Point::new(20.0, 40.0),
                node_pin(PinSide::Right, 0usize, Blank).direction(PinDirection::Output),
            ))
            .push_node(node(
                1,
                Point::new(220.0, 40.0),
                node_pin(PinSide::Left, 1usize, Blank).direction(PinDirection::Input),
            ))
            .push_edge(styled_edge(
                PinRef::new(0, 9),
                PinRef::new(1, 1),
                EdgeCurve::Line,
            ))
            .push_edge(styled_edge(
                PinRef::new(0, 0),
                PinRef::new(1, 1),
                EdgeCurve::Line,
            ))
            .push_edge(styled_edge(
                PinRef::new(7, 0),
                PinRef::new(1, 1),
                EdgeCurve::Line,
            ))
            .push_edge(styled_edge(
                PinRef::new(0, 0),
                PinRef::new(1, 1),
                EdgeCurve::BezierCubic,
            ));

        let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, NullRenderer>);
        let mut renderer = NullRenderer;
        let layout_node =
            graph.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, SIZE));
        graph.draw(
            &tree,
            &mut renderer,
            &Theme::Dark,
            &renderer::Style {
                text_color: Color::BLACK,
            },
            layout::Layout::new(&layout_node),
            mouse::Cursor::Unavailable,
            &Rectangle::new(Point::ORIGIN, SIZE),
        );

        let state = tree.state.downcast_ref::<NodeGraphState>();
        let curves = state.edge_curves.borrow();
        assert_eq!(
            curves.as_slice(),
            [
                EdgeCurve::default(),
                EdgeCurve::Line,
                EdgeCurve::default(),
                EdgeCurve::BezierCubic
            ],
            "edges 0 and 2 never resolved, so they keep the default; edges 1 and \
             3 keep the curve they were drawn with, at their own index",
        );
    }
}
