use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector, keyboard};
use iced_widget::core::{
    Clipboard, Layout, Shell, layout, mouse, renderer,
    widget::{self, Tree, tree},
};
use std::hash::Hasher;
use web_time::Instant;

use super::{
    DragInfo, NodeGraph, NodeGraphMessage,
    effects::{
        self, EdgeRenderData, EdgesPrimitive, GridPrimitive, Layer, NodeLayer, NodePrimitive,
        PinRenderData,
    },
    euclid::{IntoIced, WorldVector},
    state::{Dragging, NodeGraphState},
};
use crate::{
    PinDirection, PinRef, PinSide,
    ids::{EdgeId, NodeId, PinId},
    node_graph::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::NodePinState,
    style::{EdgeConfig, EdgeStyle, GraphStyle, NodeConfig, NodeStyle, PinConfig, PinStyle},
};

// Click detection threshold (in world-space pixels)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary)
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

/// Computes a hash for any PinId type.
/// Used to match pin_id_hash in NodePinState.
fn compute_pin_hash<P: PinId>(pin_id: &P) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(pin_id, &mut hasher);
    hasher.finish()
}

/// Resolves a NodeConfig to a complete NodeStyle using theme defaults.
fn resolve_node_style(config: &NodeConfig, theme: &Theme) -> NodeStyle {
    let base = NodeStyle::from_theme(theme);
    NodeStyle {
        fill_color: config.fill_color.unwrap_or(base.fill_color),
        border_color: config.border_color.unwrap_or(base.border_color),
        border_width: config.border_width.unwrap_or(base.border_width),
        corner_radius: config.corner_radius.unwrap_or(base.corner_radius),
        opacity: config.opacity.unwrap_or(base.opacity),
        shadow: config
            .shadow
            .as_ref()
            .filter(|sc| sc.enabled.unwrap_or(true)) // Only if enabled (default true)
            .map(|sc| crate::style::ShadowStyle {
                offset: sc.offset.unwrap_or((4.0, 4.0)),
                blur_radius: sc.blur_radius.unwrap_or(8.0),
                color: sc
                    .color
                    .unwrap_or(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3)),
            })
            .or_else(|| {
                // Fall back to base.shadow only if config.shadow is None
                if config.shadow.is_none() {
                    base.shadow
                } else {
                    None // config.shadow exists but enabled=false
                }
            }),
    }
}

/// Resolves an EdgeConfig to a complete EdgeStyle using theme defaults.
fn resolve_edge_style(config: &EdgeConfig, theme: &Theme) -> EdgeStyle {
    let base = EdgeStyle::from_theme(theme);
    base.with_config(config)
}

/// Resolves a GraphStyle or uses theme defaults.
fn resolve_graph_style(style: Option<&GraphStyle>, theme: &Theme) -> GraphStyle {
    style
        .cloned()
        .unwrap_or_else(|| GraphStyle::from_theme(theme))
}

/// Resolves pin style by merging PinConfig overrides with theme defaults.
fn resolve_pin_style(config: Option<&PinConfig>, theme: &Theme) -> PinStyle {
    let base = PinStyle::from_theme(theme);
    let Some(config) = config else { return base };
    PinStyle {
        color: config.color.unwrap_or(base.color),
        radius: config.radius.unwrap_or(base.radius),
        shape: config.shape.unwrap_or(base.shape),
        border_color: config.border_color.or(base.border_color),
        border_width: config.border_width.unwrap_or(base.border_width),
    }
}

impl<N, P, E, Message, Renderer> iced_widget::core::Widget<Message, iced::Theme, Renderer>
    for NodeGraph<'_, N, P, E, Message, iced::Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    Renderer: iced_widget::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
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
            .map(|((position, element, _style), node_tree)| {
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
        let mut camera = state.camera;

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

        // Handle panning when dragging the graph
        if let Dragging::Graph(origin) = state.dragging
            && let Some(cursor_position) = cursor.position() {
                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                let cursor_position: WorldPoint = state
                    .camera
                    .screen_to_world()
                    .transform_point(cursor_position);
                camera = camera.move_by(cursor_position - origin);
            }

        // Resolve styles
        let resolved_graph = resolve_graph_style(self.graph_style.as_ref(), theme);
        let resolved_pin_defaults = resolve_pin_style(self.pin_defaults.as_ref(), theme);
        let resolved_edge_defaults = EdgeStyle::from_theme(theme);

        let selection_style = &resolved_graph.selection_style;
        let selection_border_color = selection_style.selected_border_color;
        let selection_border_width = selection_style.selected_border_width;
        let selected_edge_color = glam::vec4(
            selection_border_color.r,
            selection_border_color.g,
            selection_border_color.b,
            selection_border_color.a,
        );

        // Check if we're edge dragging
        let is_edge_dragging = matches!(
            state.dragging,
            Dragging::Edge(_, _, _) | Dragging::EdgeOver(_, _, _, _)
        );

        // ========================================
        // Layer 1: Grid (behind everything)
        // ========================================
        renderer.with_layer(layout.bounds(), |renderer| {
            renderer.draw_primitive(
                layout.bounds(),
                GridPrimitive {
                    camera_zoom: camera.zoom(),
                    camera_position: camera.position(),
                    background_style: resolved_graph.background.clone(),
                },
            );
        });

        // ========================================
        // Collect edge data with resolved positions
        // ========================================
        // Helper to compute drag offset for a node
        let compute_node_offset = |node_idx: usize| -> WorldVector {
            let mut offset = WorldVector::zero();
            let is_selected = state.selected_nodes.contains(&node_idx);

            // Single node drag
            if let (Dragging::Node(drag_idx, origin), Some(cursor_pos)) =
                (state.dragging.clone(), cursor.position())
                && drag_idx == node_idx {
                    let cursor_world: WorldPoint = camera
                        .screen_to_world()
                        .transform_point(cursor_pos.into_euclid());
                    offset = cursor_world - origin;
                }

            // Group move
            if let (Dragging::GroupMove(origin), Some(cursor_pos)) =
                (state.dragging.clone(), cursor.position())
                && is_selected {
                    let cursor_world: WorldPoint = camera
                        .screen_to_world()
                        .transform_point(cursor_pos.into_euclid());
                    offset = cursor_world - origin;
                }

            offset
        };

        let static_edges: Vec<EdgeRenderData> = self
            .edges
            .iter()
            .filter_map(|(from, to, edge_config)| {
                // Resolve node IDs to indices
                let from_node_idx = self.id_maps.nodes.index(&from.node_id)?;
                let to_node_idx = self.id_maps.nodes.index(&to.node_id)?;

                // Get tree/layout for each node
                let from_node_tree = tree.children.get(from_node_idx)?;
                let from_node_layout = layout.children().nth(from_node_idx)?;
                let to_node_tree = tree.children.get(to_node_idx)?;
                let to_node_layout = layout.children().nth(to_node_idx)?;

                // Compute drag offsets for both nodes
                let from_offset = compute_node_offset(from_node_idx);
                let to_offset = compute_node_offset(to_node_idx);

                // Find pins by matching pin_id hash
                let from_pin_hash = compute_pin_hash(&from.pin_id);
                let from_pins = find_pins(from_node_tree, from_node_layout);
                let (_, from_pin_state, (from_pin_pos, _)) = from_pins
                    .iter()
                    .find(|(_, state, _)| state.pin_id_hash == from_pin_hash)?;

                let to_pin_hash = compute_pin_hash(&to.pin_id);
                let to_pins = find_pins(to_node_tree, to_node_layout);
                let (_, to_pin_state, (to_pin_pos, _)) = to_pins
                    .iter()
                    .find(|(_, state, _)| state.pin_id_hash == to_pin_hash)?;

                let is_selected = state.selected_nodes.contains(&from_node_idx)
                    && state.selected_nodes.contains(&to_node_idx);

                // Apply drag offsets to pin positions
                let start_pos = (from_pin_pos.into_euclid().to_vector() + from_offset).to_point();
                let end_pos = (to_pin_pos.into_euclid().to_vector() + to_offset).to_point();

                Some(EdgeRenderData {
                    start_pos,
                    end_pos,
                    start_side: from_pin_state.side.into(),
                    end_side: to_pin_state.side.into(),
                    start_direction: from_pin_state.direction,
                    end_direction: to_pin_state.direction,
                    style: resolve_edge_style(edge_config, theme),
                    is_selected,
                    is_pending_cut: false,
                    start_pin_color: from_pin_state.color,
                    end_pin_color: to_pin_state.color,
                })
            })
            .collect();

        // Add dragging edge if actively dragging (but not when snapped - EdgeOver)
        let mut all_edges = static_edges;
        if let Dragging::Edge(from_node_idx, from_pin_idx, _) = &state.dragging
            && let Some(cursor_pos) = cursor.position() {
                // Get source pin info
                if let (Some(from_tree), Some(from_layout)) = (
                    tree.children.get(*from_node_idx),
                    layout.children().nth(*from_node_idx),
                ) {
                    let from_pins = find_pins(from_tree, from_layout);
                    if let Some((_, from_pin_state, (from_pin_pos, _))) =
                        from_pins.get(*from_pin_idx)
                    {
                        // Apply drag offset to source pin
                        let from_offset = compute_node_offset(*from_node_idx);
                        let start_pos =
                            (from_pin_pos.into_euclid().to_vector() + from_offset).to_point();

                        // End position is cursor in world coordinates
                        let end_pos: WorldPoint = camera
                            .screen_to_world()
                            .transform_point(cursor_pos.into_euclid());

                        // Use global edge_defaults if set, otherwise fall back to EdgeConfig::default()
                        let drag_edge_config = self.edge_defaults.clone().unwrap_or_default();
                        let drag_edge_style = resolve_edge_style(&drag_edge_config, theme);

                        // Compute opposite side for end
                        let end_side: u32 = match from_pin_state.side {
                            PinSide::Left => 1,   // Right
                            PinSide::Right => 0,  // Left
                            PinSide::Top => 3,    // Bottom
                            PinSide::Bottom => 2, // Top
                            PinSide::Row => 1,    // Default to Right
                        };

                        // Compute opposite direction for end
                        let end_direction = match from_pin_state.direction {
                            PinDirection::Input => PinDirection::Output,
                            PinDirection::Output => PinDirection::Input,
                            PinDirection::Both => PinDirection::Both,
                        };

                        all_edges.push(EdgeRenderData {
                            start_pos,
                            end_pos,
                            start_side: from_pin_state.side.into(),
                            end_side,
                            start_direction: from_pin_state.direction,
                            end_direction,
                            style: drag_edge_style,
                            is_selected: false,
                            is_pending_cut: false,
                            start_pin_color: from_pin_state.color,
                            end_pin_color: from_pin_state.color,
                        });
                    }
                }
            }

        // ========================================
        // Layer 2: Static Edges (behind nodes)
        // ========================================
        renderer.with_layer(layout.bounds(), |renderer| {
            renderer.draw_primitive(
                layout.bounds(),
                EdgesPrimitive {
                    edges: all_edges,
                    camera_zoom: camera.zoom(),
                    camera_position: camera.position(),
                    time,
                    selected_edge_color,
                },
            );
        });

        // ========================================
        // Layers 3..N: Nodes (each node gets 3 sub-layers)
        // For each node: Background → Widgets → Foreground
        // ========================================
        for (node_index, (((_position, element, node_style), node_tree), node_layout)) in self
            .nodes
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let is_selected = state.selected_nodes.contains(&node_index);

            // Compute drag offset
            let offset = {
                let mut offset = WorldVector::zero();

                // Single node drag
                if let (Dragging::Node(drag_idx, origin), Some(cursor_pos)) =
                    (state.dragging.clone(), cursor.position())
                    && drag_idx == node_index {
                        let cursor_world: WorldPoint = camera
                            .screen_to_world()
                            .transform_point(cursor_pos.into_euclid());
                        offset = cursor_world - origin;
                    }

                // Group move
                if let (Dragging::GroupMove(origin), Some(cursor_pos)) =
                    (state.dragging.clone(), cursor.position())
                    && is_selected {
                        let cursor_world: WorldPoint = camera
                            .screen_to_world()
                            .transform_point(cursor_pos.into_euclid());
                        offset = cursor_world - origin;
                    }

                offset
            };

            // Resolve node style
            let resolved = resolve_node_style(node_style, theme);
            let (shadow_offset, shadow_blur, shadow_color) = if let Some(shadow) = &resolved.shadow
            {
                (shadow.offset, shadow.blur_radius, shadow.color)
            } else {
                ((0.0, 0.0), 0.0, iced::Color::TRANSPARENT)
            };

            let (border_color, border_width) = if is_selected {
                (selection_border_color, selection_border_width)
            } else {
                (resolved.border_color, resolved.border_width)
            };

            // Collect pins for this node
            let pins: Vec<PinRenderData> = find_pins(node_tree, node_layout)
                .iter()
                .enumerate()
                .map(|(pin_idx, (_pin_index, pin_state, (pin_pos, _)))| {
                    let is_valid_target = is_edge_dragging
                        && state.valid_drop_targets.contains(&(node_index, pin_idx));
                    PinRenderData {
                        offset: (pin_pos.into_euclid().to_vector() + offset).to_point(),
                        side: pin_state.side.into(),
                        radius: resolved_pin_defaults.radius,
                        color: pin_state.color,
                        direction: pin_state.direction,
                        shape: resolved_pin_defaults.shape,
                        border_color: resolved_pin_defaults
                            .border_color
                            .unwrap_or(iced::Color::TRANSPARENT),
                        border_width: resolved_pin_defaults.border_width,
                        is_valid_target,
                    }
                })
                .collect();

            let node_position: WorldPoint =
                (node_layout.bounds().position().into_euclid().to_vector() + offset).to_point();
            let node_size = node_layout.bounds().size();

            // Build NodePrimitive data (will be used for both layers)
            let node_primitive = NodePrimitive {
                layer: NodeLayer::Background, // Will be overwritten
                position: node_position,
                size: node_size,
                corner_radius: resolved.corner_radius,
                border_width,
                opacity: resolved.opacity,
                fill_color: resolved.fill_color,
                border_color,
                shadow_offset,
                shadow_blur,
                shadow_color,
                is_selected,
                pins: pins.clone(),
                camera_zoom: camera.zoom(),
                camera_position: camera.position(),
                time,
            };

            // Layer 3a: Node Background (fill + shadow)
            renderer.with_layer(layout.bounds(), |renderer| {
                renderer.draw_primitive(
                    layout.bounds(),
                    NodePrimitive {
                        layer: NodeLayer::Background,
                        ..node_primitive.clone()
                    },
                );
            });

            // Layer 3b: Node Widgets
            renderer.with_layer(layout.bounds(), |renderer| {
                camera.draw_with::<_, Renderer>(
                    renderer,
                    viewport,
                    cursor,
                    |renderer, viewport, cursor| {
                        let bounds = node_layout.bounds();
                        let screen_offset: Vector = offset.into_iced();
                        let clip_bounds = Rectangle {
                            x: bounds.x + screen_offset.x + border_width,
                            y: bounds.y + screen_offset.y + border_width,
                            width: (bounds.width - 2.0 * border_width).max(0.0),
                            height: (bounds.height - 2.0 * border_width).max(0.0),
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
                                    viewport,
                                );
                            });
                        });
                    },
                );
            });

            // Layer 3c: Node Foreground (border + pins)
            renderer.with_layer(layout.bounds(), |renderer| {
                renderer.draw_primitive(
                    layout.bounds(),
                    NodePrimitive {
                        layer: NodeLayer::Foreground,
                        ..node_primitive
                    },
                );
            });
        }

        // ========================================
        // Layer N+1: Dragging Edge / Selection Box / Edge Cutting
        // Use old primitive for these overlay elements for now
        // ========================================
        if matches!(
            state.dragging,
            Dragging::Edge(_, _, _)
                | Dragging::EdgeOver(_, _, _, _)
                | Dragging::BoxSelect(_, _)
                | Dragging::EdgeCutting { .. }
        ) {
            // For dragging overlay, fall back to the old primitive approach
            // This handles the dragging edge, selection box, and edge cutting visuals
            let drag_primitive = effects::NodeGraphPrimitive {
                layer: Layer::Foreground,
                camera_zoom: camera.zoom(),
                camera_position: camera.position(),
                cursor_position: camera
                    .screen_to_world()
                    .transform_point(cursor.position().unwrap_or(Point::ORIGIN).into_euclid()),
                time,
                dragging: state.dragging.clone(),
                nodes: self
                    .nodes
                    .iter()
                    .zip(&tree.children)
                    .zip(layout.children())
                    .enumerate()
                    .map(|(node_index, ((_, node_tree), node_layout))| {
                        let is_selected = state.selected_nodes.contains(&node_index);
                        let offset = {
                            let mut offset = WorldVector::zero();
                            if let (Dragging::Node(drag_idx, origin), Some(cursor_pos)) =
                                (state.dragging.clone(), cursor.position())
                                && drag_idx == node_index {
                                    let cursor_world: WorldPoint = camera
                                        .screen_to_world()
                                        .transform_point(cursor_pos.into_euclid());
                                    offset = cursor_world - origin;
                                }
                            if let (Dragging::GroupMove(origin), Some(cursor_pos)) =
                                (state.dragging.clone(), cursor.position())
                                && is_selected {
                                    let cursor_world: WorldPoint = camera
                                        .screen_to_world()
                                        .transform_point(cursor_pos.into_euclid());
                                    offset = cursor_world - origin;
                                }
                            offset
                        };

                        effects::Node {
                            position: node_layout.bounds().position().into_euclid().to_vector()
                                + offset,
                            size: node_layout.bounds().size().into_euclid(),
                            corner_radius: 8.0,
                            border_width: 2.0,
                            opacity: 1.0,
                            fill_color: iced::Color::TRANSPARENT,
                            border_color: iced::Color::TRANSPARENT,
                            pins: find_pins(node_tree, node_layout)
                                .iter()
                                .map(|(_, pin_state, (a, _))| effects::Pin {
                                    side: pin_state.side.into(),
                                    offset: a.into_euclid().to_vector() + offset,
                                    radius: resolved_pin_defaults.radius,
                                    color: pin_state.color,
                                    direction: pin_state.direction,
                                    shape: resolved_pin_defaults.shape,
                                    border_color: iced::Color::TRANSPARENT,
                                    border_width: 0.0,
                                })
                                .collect(),
                            shadow_offset: (0.0, 0.0),
                            shadow_blur: 0.0,
                            shadow_color: iced::Color::TRANSPARENT,
                            flags: 0,
                        }
                    })
                    .collect(),
                edges: vec![],
                edge_color: glam::Vec4::ZERO,
                background_color: glam::Vec4::ZERO,
                border_color: glam::Vec4::ZERO,
                fill_color: glam::Vec4::ZERO,
                drag_edge_color: glam::vec4(
                    resolved_graph.drag_edge_color.r,
                    resolved_graph.drag_edge_color.g,
                    resolved_graph.drag_edge_color.b,
                    resolved_graph.drag_edge_color.a,
                ),
                drag_edge_valid_color: glam::vec4(
                    resolved_graph.drag_edge_valid_color.r,
                    resolved_graph.drag_edge_valid_color.g,
                    resolved_graph.drag_edge_valid_color.b,
                    resolved_graph.drag_edge_valid_color.a,
                ),
                selected_nodes: state.selected_nodes.clone(),
                selected_edge_color,
                edge_thickness: resolved_edge_defaults.get_width(),
                valid_drop_targets: state.valid_drop_targets.clone(),
                background_style: resolved_graph.background.clone(),
            };

            renderer.with_layer(layout.bounds(), |renderer| {
                renderer.draw_primitive(layout.bounds(), drag_primitive);
            });
        }
    }

    fn size_hint(&self) -> Size<Length> {
        self.size()
    }

    fn children(&self) -> Vec<Tree> {
        self.elements_iter()
            .map(|(_, element, _)| Tree::new(element))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let children: Vec<&Element<'_, Message, iced::Theme, Renderer>> =
            self.elements_iter().map(|(_, e, _)| e).collect();
        tree.diff_children(&children);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for (((_, element, _style), node_tree), node_layout) in self
            .elements_iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            element
                .as_widget_mut()
                .operate(node_tree, node_layout, renderer, operation);
        }
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

        // Synchronize external selection with internal state
        if let Some(external) = self.get_external_selection()
            && state.selected_nodes != *external {
                state.selected_nodes = external.clone();
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

        // Track keyboard modifiers for Shift/Ctrl selection
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }

        // Handle keyboard shortcuts
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
            match key {
                // Ctrl+D: Clone selected nodes
                keyboard::Key::Character(c) if c.as_str() == "d" && modifiers.command() => {
                    if !state.selected_nodes.is_empty() {
                        let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        let node_ids = self.translate_node_ids(&indices);
                        if let Some(handler) = self.on_clone_handler() {
                            shell.publish(handler(node_ids.clone()));
                        }
                        if let Some(handler) = self.get_on_event() {
                            shell.publish(handler(NodeGraphMessage::CloneRequested { node_ids }));
                        }
                        shell.capture_event();
                    }
                }
                // Ctrl+A: Select all nodes
                keyboard::Key::Character(c) if c.as_str() == "a" && modifiers.command() => {
                    let count = self.nodes.len();
                    state.selected_nodes = (0..count).collect();
                    let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                    let selected = self.translate_node_ids(&indices);
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(selected.clone()));
                    }
                    if let Some(handler) = self.get_on_event() {
                        shell.publish(handler(NodeGraphMessage::SelectionChanged { selected }));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                // Escape: Clear selection
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    if !state.selected_nodes.is_empty() {
                        state.selected_nodes.clear();
                        if let Some(handler) = self.on_select_handler() {
                            shell.publish(handler(vec![]));
                        }
                        if let Some(handler) = self.get_on_event() {
                            shell.publish(handler(NodeGraphMessage::SelectionChanged {
                                selected: vec![],
                            }));
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }
                // Delete/Backspace handled AFTER child widgets to let text inputs consume it first
                _ => {}
            }
        }

        // Track left mouse button state globally (for Fruit Ninja edge cutting)
        if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
            state.left_mouse_down = false;
        }

        if let Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) = event {
            if let Some(cursor_pos) = screen_cursor.position() {
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

                // Emit camera change event
                if let Some(handler) = self.on_camera_change_handler() {
                    let pos = state.camera.position();
                    shell.publish(handler(Point::new(pos.x, pos.y), state.camera.zoom()));
                }
            }
            shell.capture_event();
            shell.request_redraw();
        }

        let graph_move_offset = if let Dragging::Graph(origin) = state.dragging {
            screen_cursor
                .position()
                .map(|cursor_position| cursor_position - origin.into_iced())
        } else {
            None
        }
        .unwrap_or(Vector::ZERO);
        state
            .camera
            .move_by(graph_move_offset.into_euclid())
            .update_with(viewport, screen_cursor, |viewport, world_cursor| {
                let state = tree.state.downcast_mut::<NodeGraphState>();

                if state.dragging != Dragging::None {
                    if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
                        // Emit drag update event with current cursor position
                        if let Some(cursor_position) = world_cursor.position()
                            && let Some(handler) = self.on_drag_update_handler() {
                                shell.publish(handler(cursor_position.x, cursor_position.y));
                            }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }

                // Update hover state when cursor moves
                if let Some(cursor_point) = world_cursor.position() {
                    let mut found_hover = None;

                    // Check nodes in reverse order (top-most first)
                    for (node_index, node_layout) in layout.children().enumerate().rev() {
                        let bounds = node_layout.bounds();
                        if bounds.contains(cursor_point) {
                            found_hover = Some(node_index);
                            break;
                        }
                    }

                    // Update hover state if changed
                    if state.hovered_node != found_hover {
                        state.hovered_node = found_hover;
                        shell.request_redraw();
                    }
                } else {
                    // Cursor left the widget, clear hover
                    if state.hovered_node.is_some() {
                        state.hovered_node = None;
                        shell.request_redraw();
                    }
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
                                    for (edge_idx, (from_ref, to_ref, _style)) in
                                        self.edges.iter().enumerate()
                                    {
                                        // Resolve user IDs to indices
                                        let from_node_idx =
                                            match self.id_maps.nodes.index(&from_ref.node_id) {
                                                Some(idx) => idx,
                                                None => continue,
                                            };
                                        let to_node_idx =
                                            match self.id_maps.nodes.index(&to_ref.node_id) {
                                                Some(idx) => idx,
                                                None => continue,
                                            };

                                        // Get pin positions and sides for bezier calculation
                                        let from_pin_hash = compute_pin_hash(&from_ref.pin_id);
                                        let from_pin_data = layout
                                            .children()
                                            .nth(from_node_idx)
                                            .and_then(|node_layout| {
                                                tree.children.get(from_node_idx).and_then(
                                                    |node_tree| {
                                                        let pins =
                                                            find_pins(node_tree, node_layout);
                                                        pins.iter()
                                                            .find(|(_, state, _)| {
                                                                state.pin_id_hash == from_pin_hash
                                                            })
                                                            .map(|(_, state, (pos, _))| {
                                                                (*pos, state.side)
                                                            })
                                                    },
                                                )
                                            });
                                        let to_pin_hash = compute_pin_hash(&to_ref.pin_id);
                                        let to_pin_data = layout
                                            .children()
                                            .nth(to_node_idx)
                                            .and_then(|node_layout| {
                                                tree.children.get(to_node_idx).and_then(
                                                    |node_tree| {
                                                        let pins =
                                                            find_pins(node_tree, node_layout);
                                                        pins.iter()
                                                            .find(|(_, state, _)| {
                                                                state.pin_id_hash == to_pin_hash
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
                                            // Calculate bezier control points (same as shader)
                                            let seg_len = 80.0;
                                            let dir_from = pin_side_to_direction(from_side);
                                            let dir_to = pin_side_to_direction(to_side);
                                            let p1 = Point::new(
                                                p0.x + dir_from.0 * seg_len,
                                                p0.y + dir_from.1 * seg_len,
                                            );
                                            let p2 = Point::new(
                                                p3.x + dir_to.0 * seg_len,
                                                p3.y + dir_to.1 * seg_len,
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
                            if let Dragging::EdgeCutting { pending_cuts, .. } = &state.dragging {
                                #[cfg(debug_assertions)]
                                println!("Edge cutting complete: {} edges cut", pending_cuts.len());

                                for &edge_idx in pending_cuts.iter() {
                                    if let Some((from_ref, to_ref, _)) = self.edges.get(edge_idx) {
                                        // Edges already store user IDs (PinRef<N, P>)
                                        if let Some(handler) = self.on_disconnect_handler() {
                                            shell
                                                .publish(handler(from_ref.clone(), to_ref.clone()));
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
                    Dragging::Graph(origin) => if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) = event {
                        if let Some(cursor_position) = screen_cursor.position() {
                            let screen_to_world = state.camera.screen_to_world();
                            let cursor_position: ScreenPoint = cursor_position.into_euclid();
                            let cursor_position: WorldPoint =
                                screen_to_world.transform_point(cursor_position);
                            let offset = cursor_position - origin;
                            state.camera = state.camera.move_by(offset);

                            // Emit camera change event
                            if let Some(handler) = self.on_camera_change_handler() {
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
                    },
                    Dragging::Node(node_index, origin) => if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
                        if let Some(cursor_position) = world_cursor.position() {
                            let cursor_position = cursor_position.into_euclid();
                            let offset = cursor_position - origin;
                            let new_position = self.nodes[node_index].0 + offset.into_iced();

                            // Translate internal index to user ID
                            if let Some(node_id) = self.index_to_node_id(node_index) {
                                // Call on_move handler if set
                                if let Some(handler) = self.on_move_handler() {
                                    shell.publish(handler(node_id.clone(), new_position));
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphMessage::NodeMoved {
                                        node_id,
                                        position: new_position,
                                    }));
                                }
                            }
                        }
                        state.dragging = Dragging::None;
                        // Emit drag end event
                        if let Some(handler) = self.on_drag_end_handler() {
                            shell.publish(handler());
                        }
                        shell.capture_event();
                        shell.invalidate_layout();
                        shell.request_redraw();
                    },
                    Dragging::Edge(from_node, from_pin, _) => match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            // Check if cursor is over a valid target pin to transition to EdgeOver
                            if let Some(cursor_position) = world_cursor.position() {
                                // Copy valid_drop_targets before iterating over tree.children
                                let valid_targets = state.valid_drop_targets.clone();

                                // Extract from_pin_id while iterating (need access to tree.children)
                                let mut from_pin_id: Option<P> = None;
                                let mut target_info: Option<(usize, usize, P)> = None;

                                // Check all pins for proximity and validity (use SNAP_THRESHOLD to enter)
                                for (node_index, (node_layout, node_tree)) in
                                    layout.children().zip(&tree.children).enumerate()
                                {
                                    for (pin_index, pin_state, (a, b)) in
                                        find_pins(node_tree, node_layout)
                                    {
                                        // Extract from_pin_id when we find the source pin
                                        if node_index == from_node && pin_index == from_pin {
                                            from_pin_id =
                                                pin_state.pin_id.downcast_ref::<P>().cloned();
                                        }

                                        // Pin positions are already in world space (from layout)
                                        let distance = a
                                            .distance(cursor_position)
                                            .min(b.distance(cursor_position));

                                        // Use SNAP_THRESHOLD for entering snap zone
                                        if distance < SNAP_THRESHOLD && target_info.is_none() {
                                            // Check if this pin is in valid_drop_targets
                                            if valid_targets.contains(&(node_index, pin_index))
                                                && let Some(pid) =
                                                    pin_state.pin_id.downcast_ref::<P>().cloned()
                                                {
                                                    target_info =
                                                        Some((node_index, pin_index, pid));
                                                }
                                        }
                                    }
                                }

                                if let Some((to_node, to_pin, to_pin_id)) = target_info {
                                    // Fire EdgeConnected event immediately on snap (plug behavior)
                                    let from_node_id = self.index_to_node_id(from_node);
                                    let to_node_id = self.index_to_node_id(to_node);

                                    if let (Some(from_nid), Some(to_nid), Some(from_pid)) =
                                        (from_node_id, to_node_id, from_pin_id)
                                    {
                                        let from_ref = PinRef::new(from_nid.clone(), from_pid);
                                        let to_ref = PinRef::new(to_nid.clone(), to_pin_id);

                                        if let Some(handler) = self.on_connect_handler() {
                                            shell
                                                .publish(handler(from_ref.clone(), to_ref.clone()));
                                        }
                                        if let Some(handler) = self.get_on_event() {
                                            // For on_event, we need edge_id - use edge count
                                            use std::any::Any;
                                            let edge_id: Option<E> = if std::any::TypeId::of::<E>()
                                                == std::any::TypeId::of::<usize>()
                                            {
                                                let boxed: Box<dyn Any> =
                                                    Box::new(self.edges.len());
                                                boxed.downcast::<E>().ok().map(|b| *b)
                                            } else {
                                                None
                                            };
                                            if let Some(eid) = edge_id {
                                                shell.publish(handler(
                                                    NodeGraphMessage::EdgeConnected {
                                                        edge_id: eid,
                                                        from: from_ref,
                                                        to: to_ref,
                                                    },
                                                ));
                                            }
                                        }
                                    }

                                    state.dragging =
                                        Dragging::EdgeOver(from_node, from_pin, to_node, to_pin);
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

                                for (node_index, (node_layout, node_tree)) in
                                    layout.children().zip(&tree.children).enumerate()
                                {
                                    for (pin_index, pin_state, (a, b)) in
                                        find_pins(node_tree, node_layout)
                                    {
                                        // Extract from_pin_id
                                        if node_index == from_node && pin_index == from_pin {
                                            from_pin_id =
                                                pin_state.pin_id.downcast_ref::<P>().cloned();
                                        }
                                        // Extract to_pin_id and check distance
                                        if node_index == to_node && pin_index == to_pin {
                                            to_pin_id =
                                                pin_state.pin_id.downcast_ref::<P>().cloned();
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
                                        let from_ref = PinRef::new(from_nid.clone(), from_pid);
                                        let to_ref = PinRef::new(to_nid.clone(), to_pid);

                                        if let Some(handler) = self.on_disconnect_handler() {
                                            shell
                                                .publish(handler(from_ref.clone(), to_ref.clone()));
                                        }
                                        if let Some(handler) = self.get_on_event() {
                                            // For on_event, we need edge_id
                                            use std::any::Any;
                                            let edge_id: Option<E> = if std::any::TypeId::of::<E>()
                                                == std::any::TypeId::of::<usize>()
                                            {
                                                let boxed: Box<dyn Any> = Box::new(0usize);
                                                boxed.downcast::<E>().ok().map(|b| *b)
                                            } else {
                                                None
                                            };
                                            if let Some(eid) = edge_id {
                                                shell.publish(handler(
                                                    NodeGraphMessage::EdgeDisconnected {
                                                        edge_id: eid,
                                                        from: from_ref,
                                                        to: to_ref,
                                                    },
                                                ));
                                            }
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
                                    shell.publish(handler(selected.clone()));
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphMessage::SelectionChanged {
                                        selected,
                                    }));
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
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position: WorldPoint = cursor_position.into_euclid();
                                let offset = cursor_position - origin;

                                // Translate internal indices to user IDs
                                let indices: Vec<usize> =
                                    state.selected_nodes.iter().copied().collect();
                                let node_ids = self.translate_node_ids(&indices);
                                let delta = offset.into_iced();
                                if let Some(handler) = self.on_group_move_handler() {
                                    shell.publish(handler(node_ids.clone(), delta));
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphMessage::GroupMoved {
                                        node_ids,
                                        delta,
                                    }));
                                }
                            }
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

                for (((_, element, _style), tree), layout) in self
                    .elements_iter_mut()
                    .zip(&mut tree.children)
                    .zip(layout.children())
                {
                    element.as_widget_mut().update(
                        tree,
                        event,
                        layout,
                        world_cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                }

                if shell.is_event_captured() {
                    return;
                }

                // Delete/Backspace: Delete selected nodes
                // Handled AFTER child widgets so text inputs can consume the event first
                if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
                    && matches!(
                        key,
                        keyboard::Key::Named(keyboard::key::Named::Delete)
                            | keyboard::Key::Named(keyboard::key::Named::Backspace)
                    )
                        && !state.selected_nodes.is_empty() {
                            let indices: Vec<usize> =
                                state.selected_nodes.iter().copied().collect();
                            let node_ids = self.translate_node_ids(&indices);
                            if let Some(handler) = self.on_delete_handler() {
                                shell.publish(handler(node_ids.clone()));
                            }
                            if let Some(handler) = self.get_on_event() {
                                shell.publish(handler(NodeGraphMessage::DeleteRequested {
                                    node_ids,
                                }));
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
                    Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) => {
                        if let Some(cursor_pos) = screen_cursor.position() {
                            let cursor_pos: ScreenPoint = cursor_pos.into_euclid();

                            let scroll_amount = match delta {
                                mouse::ScrollDelta::Pixels { y, .. } => *y,
                                mouse::ScrollDelta::Lines { y, .. } => *y * 10.0,
                            };

                            let zoom_delta = scroll_amount / 100.0;

                            state.camera = state.camera.zoom_at(cursor_pos, zoom_delta);

                            // Emit camera change event
                            if let Some(handler) = self.on_camera_change_handler() {
                                let pos = state.camera.position();
                                shell.publish(handler(
                                    Point::new(pos.x, pos.y),
                                    state.camera.zoom(),
                                ));
                            }
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                        // Track left mouse button state for Fruit Ninja edge cutting
                        state.left_mouse_down = true;

                        // Ctrl+Click: Edge cut tool
                        if state.modifiers.command()
                            && let Some(cursor_position) = world_cursor.position() {
                                // Check if click is near any edge
                                for (from_ref, to_ref, _style) in &self.edges {
                                    // Resolve user IDs to indices
                                    let from_node_idx =
                                        match self.id_maps.nodes.index(&from_ref.node_id) {
                                            Some(idx) => idx,
                                            None => continue,
                                        };
                                    let to_node_idx =
                                        match self.id_maps.nodes.index(&to_ref.node_id) {
                                            Some(idx) => idx,
                                            None => continue,
                                        };

                                    // Get pin positions for both ends of the edge
                                    let from_pin_hash = compute_pin_hash(&from_ref.pin_id);
                                    let from_pin_pos = layout
                                        .children()
                                        .nth(from_node_idx)
                                        .and_then(|node_layout| {
                                            tree.children.get(from_node_idx).and_then(|node_tree| {
                                                let pins = find_pins(node_tree, node_layout);
                                                pins.iter()
                                                    .find(|(_, state, _)| {
                                                        state.pin_id_hash == from_pin_hash
                                                    })
                                                    .map(|(_, _, (a, _))| *a)
                                            })
                                        });
                                    let to_pin_hash = compute_pin_hash(&to_ref.pin_id);
                                    let to_pin_pos = layout.children().nth(to_node_idx).and_then(
                                        |node_layout| {
                                            tree.children.get(to_node_idx).and_then(|node_tree| {
                                                let pins = find_pins(node_tree, node_layout);
                                                pins.iter()
                                                    .find(|(_, state, _)| {
                                                        state.pin_id_hash == to_pin_hash
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
                            // check bounds for pins
                            for (node_index, (node_layout, node_tree)) in
                                layout.children().zip(&tree.children).enumerate()
                            {
                                let pins = find_pins(node_tree, node_layout);
                                // Get node_id for this node_index
                                let current_node_id =
                                    match self.id_maps.nodes.id(node_index).cloned() {
                                        Some(id) => id,
                                        None => continue,
                                    };

                                for (pin_index, pin_state, (a, b)) in pins {
                                    // Pin positions from layout are ALREADY in world space
                                    // because layout was created with .move_to(world_position)
                                    let distance = a
                                        .distance(cursor_position)
                                        .min(b.distance(cursor_position));

                                    if distance < PIN_CLICK_THRESHOLD {
                                        // Check if this pin has existing connections
                                        // If it does, "unplug" the clicked end (like pulling a cable)
                                        for (from_ref, to_ref, _style) in &self.edges {
                                            // Compare using hash-based matching
                                            let from_pin_hash = compute_pin_hash(&from_ref.pin_id);
                                            let to_pin_hash = compute_pin_hash(&to_ref.pin_id);

                                            // If we clicked the "from" pin, unplug FROM and drag it
                                            // Keep TO pin connected, drag away from it
                                            if from_ref.node_id == current_node_id
                                                && from_pin_hash == pin_state.pin_id_hash
                                            {
                                                // Disconnect the edge - already have user IDs
                                                if let Some(handler) = self.on_disconnect_handler()
                                                {
                                                    shell.publish(handler(
                                                        from_ref.clone(),
                                                        to_ref.clone(),
                                                    ));
                                                }
                                                // Note: EdgeDisconnected message not fired here

                                                // Start dragging FROM the TO pin (the end that stays connected)
                                                // We're now dragging back towards the TO pin
                                                // Resolve to_ref to indices for internal Dragging state
                                                let to_node_idx =
                                                    match self.id_maps.nodes.index(&to_ref.node_id)
                                                    {
                                                        Some(idx) => idx,
                                                        None => continue,
                                                    };
                                                let to_pin_idx = {
                                                    let to_tree =
                                                        match tree.children.get(to_node_idx) {
                                                            Some(t) => t,
                                                            None => continue,
                                                        };
                                                    let to_layout =
                                                        match layout.children().nth(to_node_idx) {
                                                            Some(l) => l,
                                                            None => continue,
                                                        };
                                                    let to_pins = find_pins(to_tree, to_layout);
                                                    match to_pins.iter().position(|(_, s, _)| {
                                                        s.pin_id_hash == to_pin_hash
                                                    }) {
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
                                                );
                                                let state =
                                                    tree.state.downcast_mut::<NodeGraphState>();
                                                state.valid_drop_targets = valid_targets;
                                                state.dragging = Dragging::Edge(
                                                    to_node_idx,
                                                    to_pin_idx,
                                                    cursor_position.into_euclid(),
                                                );
                                                shell.capture_event();
                                                return;
                                            }
                                            // If we clicked the "to" pin, unplug TO and drag it
                                            // Keep FROM pin connected, drag away from it
                                            else if to_ref.node_id == current_node_id
                                                && to_pin_hash == pin_state.pin_id_hash
                                            {
                                                // Disconnect the edge - already have user IDs
                                                if let Some(handler) = self.on_disconnect_handler()
                                                {
                                                    shell.publish(handler(
                                                        from_ref.clone(),
                                                        to_ref.clone(),
                                                    ));
                                                }
                                                // Note: EdgeDisconnected message not fired here

                                                // Start dragging FROM the FROM pin (the end that stays connected)
                                                // We're now dragging away from the FROM pin
                                                // Resolve from_ref to indices for internal Dragging state
                                                let from_node_idx = match self
                                                    .id_maps
                                                    .nodes
                                                    .index(&from_ref.node_id)
                                                {
                                                    Some(idx) => idx,
                                                    None => continue,
                                                };
                                                let from_pin_idx = {
                                                    let from_tree =
                                                        match tree.children.get(from_node_idx) {
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
                                                    let from_pins =
                                                        find_pins(from_tree, from_layout);
                                                    match from_pins.iter().position(|(_, s, _)| {
                                                        s.pin_id_hash == from_pin_hash
                                                    }) {
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
                                                );
                                                let state =
                                                    tree.state.downcast_mut::<NodeGraphState>();
                                                state.valid_drop_targets = valid_targets;
                                                state.dragging = Dragging::Edge(
                                                    from_node_idx,
                                                    from_pin_idx,
                                                    cursor_position.into_euclid(),
                                                );
                                                shell.capture_event();
                                                return;
                                            }
                                        }

                                        // If no existing connection, start a new drag
                                        // Compute valid targets ONCE at drag-start
                                        let valid_targets = compute_valid_targets(
                                            self, tree, layout, node_index, pin_index,
                                        );
                                        let state = tree.state.downcast_mut::<NodeGraphState>();
                                        state.valid_drop_targets = valid_targets;
                                        state.dragging = Dragging::Edge(
                                            node_index,
                                            pin_index,
                                            cursor_position.into_euclid(),
                                        );
                                        // Emit drag start event
                                        if let Some(handler) = self.on_drag_start_handler() {
                                            shell.publish(handler(DragInfo::Edge {
                                                from_node: node_index,
                                                from_pin: pin_index,
                                            }));
                                        }
                                        shell.capture_event();
                                        return;
                                    }
                                }
                            }

                            // check bounds for nodes
                            for (node_index, node_layout) in layout.children().enumerate() {
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

                                    // Decide between single node drag or group move
                                    if state.selected_nodes.len() > 1
                                        && state.selected_nodes.contains(&node_index)
                                    {
                                        // Multiple nodes selected, start group move
                                        let selected: Vec<usize> =
                                            state.selected_nodes.iter().copied().collect();
                                        state.dragging =
                                            Dragging::GroupMove(cursor_position.into_euclid());
                                        // Emit drag start event for group
                                        if let Some(handler) = self.on_drag_start_handler() {
                                            shell.publish(handler(DragInfo::Group {
                                                node_ids: selected,
                                            }));
                                        }
                                    } else {
                                        // Single node drag
                                        state.dragging = Dragging::Node(
                                            node_index,
                                            cursor_position.into_euclid(),
                                        );
                                        // Emit drag start event for single node
                                        if let Some(handler) = self.on_drag_start_handler() {
                                            shell.publish(handler(DragInfo::Node {
                                                node_id: node_index,
                                            }));
                                        }
                                    }

                                    // Notify selection change
                                    if selection_changed {
                                        let selected = self.translate_node_ids(&new_selection);
                                        if let Some(handler) = self.on_select_handler() {
                                            shell.publish(handler(selected.clone()));
                                        }
                                        if let Some(handler) = self.get_on_event() {
                                            shell.publish(handler(
                                                NodeGraphMessage::SelectionChanged { selected },
                                            ));
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

                            state.dragging = Dragging::BoxSelect(cursor_position, cursor_position);
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
            });
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

impl<'a, N, P, E, Message, Renderer> From<NodeGraph<'a, N, P, E, Message, iced::Theme, Renderer>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    Renderer: iced_widget::core::renderer::Renderer + 'a + iced_wgpu::primitive::Renderer,
    Message: 'static,
{
    fn from(graph: NodeGraph<'a, N, P, E, Message, iced::Theme, Renderer>) -> Self {
        Element::new(graph)
    }
}

/// Creates a new NodeGraph with default usize-based IDs.
///
/// For custom ID types, use `NodeGraph::<N, P, E, Message, Theme, Renderer>::default()`.
pub fn node_graph<'a, Message, Theme, Renderer>()
-> NodeGraph<'a, usize, usize, usize, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    NodeGraph::default()
}

/// Helper function to find all NodePin elements in the tree of a Node.
/// Returns: Vec of (pin_index, &NodePinState, (Point, Point) positions)
/// NodePinState is non-generic and contains pin_id_hash for matching.
fn find_pins<'a>(
    tree: &'a Tree,
    layout: Layout<'a>,
) -> Vec<(usize, &'a NodePinState, (Point, Point))> {
    let mut flat = Vec::new();
    let mut pin_index = 0;
    inner_find_pins(&mut flat, &mut pin_index, layout, tree);
    flat
}

fn inner_find_pins<'a>(
    flat: &mut Vec<(usize, &'a NodePinState, (Point, Point))>,
    pin_index: &mut usize,
    node_layout: Layout<'a>,
    pin_tree: &'a Tree,
) {
    // NodePinState is non-generic, so tree::Tag is always the same
    if pin_tree.tag == tree::Tag::of::<NodePinState>() {
        let pin_state = pin_tree.state.downcast_ref::<NodePinState>();
        let node_bounds = node_layout.bounds();
        let pin_positions = pin_positions(pin_state, node_bounds);
        flat.push((*pin_index, pin_state, pin_positions));
        *pin_index += 1;
    }

    for child_tree in &pin_tree.children {
        inner_find_pins(flat, pin_index, node_layout, child_tree);
    }
}

/// Validates if two pins can be connected based on direction.
/// Only checks direction compatibility - type/custom logic is handled by can_connect callback.
fn validate_pin_direction(from_pin: &NodePinState, to_pin: &NodePinState) -> bool {
    use crate::node_pin::PinDirection;

    // Check direction compatibility:
    // - Output can connect to Input or Both
    // - Input can connect to Output or Both
    // - Both can connect to anything
    match (from_pin.direction, to_pin.direction) {
        // Both can connect to anything
        (PinDirection::Both, _) | (_, PinDirection::Both) => true,
        // Output -> Input or Input -> Output is valid
        (PinDirection::Output, PinDirection::Input)
        | (PinDirection::Input, PinDirection::Output) => true,
        // Same direction is not allowed (Output->Output or Input->Input)
        _ => false,
    }
}

/// Computes valid drop targets for edge dragging.
///
/// Called ONCE at drag-start to determine which pins are valid connection targets.
/// Results are stored in state.valid_drop_targets for efficient lookup during drag.
///
/// A pin is a valid target if:
/// 1. It's not the source pin (can't connect to self)
/// 2. Direction is compatible (Output->Input, etc.)
/// 3. TypeId matches (same data type)
fn compute_valid_targets<N, P, E, Message, Renderer>(
    _graph: &NodeGraph<'_, N, P, E, Message, iced::Theme, Renderer>,
    tree: &Tree,
    layout: Layout<'_>,
    from_node: usize,
    from_pin: usize,
) -> std::collections::HashSet<(usize, usize)>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    Renderer: iced_widget::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let mut valid_targets = std::collections::HashSet::new();

    // Get the source pin state for direction and type validation
    let from_pin_state = tree.children.get(from_node).and_then(|node_tree| {
        layout.children().nth(from_node).and_then(|node_layout| {
            find_pins(node_tree, node_layout)
                .into_iter()
                .nth(from_pin)
                .map(|(_, state, _)| state.clone())
        })
    });

    let Some(from_state) = from_pin_state else {
        return valid_targets;
    };

    // Iterate all pins in all nodes
    for (node_index, (node_layout, node_tree)) in layout.children().zip(&tree.children).enumerate()
    {
        for (pin_index, pin_state, _) in find_pins(node_tree, node_layout) {
            // Skip source pin
            if node_index == from_node && pin_index == from_pin {
                continue;
            }

            // Check direction compatibility (Input<->Output, etc.)
            if !validate_pin_direction(&from_state, pin_state) {
                continue;
            }

            // Check TypeId compatibility - only same types can connect
            if from_state.data_type != pin_state.data_type {
                continue;
            }

            valid_targets.insert((node_index, pin_index));
        }
    }

    valid_targets
}

fn pin_positions(state: &NodePinState, node_bounds: Rectangle) -> (Point, Point) {
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
