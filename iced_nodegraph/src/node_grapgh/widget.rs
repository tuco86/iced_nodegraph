use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector, border, keyboard,
};
use iced_widget::core::{
    Clipboard, Layout, Shell, layout, mouse, renderer,
    widget::{self, Tree, tree},
};
use web_time::Instant;

use super::{
    DragInfo, NodeGraph, NodeGraphEvent,
    effects::{self, EdgeData, Layer},
    euclid::{IntoIced, WorldVector},
    state::{Dragging, NodeGraphState},
};
use crate::{
    PinReference, PinSide,
    node_grapgh::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::NodePinState,
    style::StyleResolver,
};

// Click detection threshold (in world-space pixels)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

impl<Message, Renderer> iced_widget::core::Widget<Message, iced::Theme, Renderer>
    for NodeGraph<'_, Message, iced::Theme, Renderer>
where
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
                state.time + delta
            } else {
                state.time
            }
        };

        // Handle panning when dragging the graph.
        if let Dragging::Graph(origin) = state.dragging {
            if let Some(cursor_position) = cursor.position() {
                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                let cursor_position: WorldPoint = state
                    .camera
                    .screen_to_world()
                    .transform_point(cursor_position);
                camera = camera.move_by(cursor_position - origin);
            }
        }

        // Create StyleResolver for cascading style system
        // Theme Defaults -> Graph Defaults -> Item Config
        // Uses iced::Theme directly for proper palette access
        let resolver = StyleResolver::new(theme, self.graph_defaults.as_ref());

        // Resolve graph-level styles through cascade
        let resolved_graph = resolver.resolve_graph();
        let resolved_node_defaults = resolver.resolve_node(None);
        let resolved_edge_defaults = resolver.resolve_edge(None);
        let resolved_pin_defaults = resolver.resolve_pin(None);

        // Convert resolved styles to GPU-compatible formats
        let bg_color = glam::vec4(
            resolved_graph.background_color.r,
            resolved_graph.background_color.g,
            resolved_graph.background_color.b,
            resolved_graph.background_color.a,
        );
        let border_color = glam::vec4(
            resolved_graph.grid_color.r,
            resolved_graph.grid_color.g,
            resolved_graph.grid_color.b,
            resolved_graph.grid_color.a,
        );
        let fill_color = glam::vec4(
            resolved_node_defaults.fill_color.r,
            resolved_node_defaults.fill_color.g,
            resolved_node_defaults.fill_color.b,
            resolved_node_defaults.fill_color.a,
        );
        let edge_color = glam::vec4(
            resolved_edge_defaults.start_color.r,
            resolved_edge_defaults.start_color.g,
            resolved_edge_defaults.start_color.b,
            resolved_edge_defaults.start_color.a,
        );
        let drag_edge_color = glam::vec4(
            resolved_graph.drag_edge_color.r,
            resolved_graph.drag_edge_color.g,
            resolved_graph.drag_edge_color.b,
            resolved_graph.drag_edge_color.a,
        );
        let drag_valid_color = glam::vec4(
            resolved_graph.drag_edge_valid_color.r,
            resolved_graph.drag_edge_valid_color.g,
            resolved_graph.drag_edge_valid_color.b,
            resolved_graph.drag_edge_valid_color.a,
        );

        // Get selection style from resolved graph style
        let selection_style = &resolved_graph.selection_style;
        let selection_border_color = selection_style.selected_border_color;
        let selection_border_width = selection_style.selected_border_width;

        let primitive_background = effects::NodeGraphPrimitive {
            layer: Layer::Background,
            camera_zoom: camera.zoom(),
            camera_position: camera.position(),
            cursor_position: camera.screen_to_world().transform_point(
                cursor
                    .position()
                    .unwrap_or(Point::new(0.0, 0.0))
                    .into_euclid(),
            ),
            time,
            dragging: state.dragging.clone(),
            nodes: {
                self.nodes
                    .iter()
                    .zip(&tree.children)
                    .zip(layout.children())
                    .enumerate()
                    .map(
                        |(
                            node_index,
                            (((_position, _element, node_style), node_tree), node_layout),
                        )| {
                            let is_selected = state.selected_nodes.contains(&node_index);
                            let mut offset = WorldVector::zero();

                            // Handle single node drag offset
                            if let (
                                Dragging::Node(drag_node_index, origin),
                                Some(cursor_position),
                            ) = (state.dragging.clone(), cursor.position())
                            {
                                if drag_node_index == node_index {
                                    let cursor_position: ScreenPoint =
                                        cursor_position.into_euclid();
                                    let cursor_position: WorldPoint =
                                        camera.screen_to_world().transform_point(cursor_position);
                                    offset = cursor_position - origin
                                }
                            }

                            // Handle group move offset for all selected nodes
                            if let (Dragging::GroupMove(origin), Some(cursor_position)) =
                                (state.dragging.clone(), cursor.position())
                            {
                                if is_selected {
                                    let cursor_position: ScreenPoint =
                                        cursor_position.into_euclid();
                                    let cursor_position: WorldPoint =
                                        camera.screen_to_world().transform_point(cursor_position);
                                    offset = cursor_position - origin
                                }
                            }

                            // Resolve node style through cascade:
                            // Theme Defaults -> Graph Defaults -> Per-Node Config
                            // node_style is now Option<NodeConfig> (partial overrides)
                            let resolved = resolver.resolve_node(node_style.as_ref());

                            // Extract shadow properties
                            let (shadow_offset, shadow_blur, shadow_color) =
                                if let Some(shadow) = &resolved.shadow {
                                    (shadow.offset, shadow.blur_radius, shadow.color)
                                } else {
                                    ((0.0, 0.0), 0.0, iced::Color::TRANSPARENT)
                                };

                            let node_fill = resolved.fill_color;
                            let mut node_border = resolved.border_color;
                            let corner_rad = resolved.corner_radius;
                            let mut border_w = resolved.border_width;
                            let opacity = resolved.opacity;

                            // Apply selection highlighting
                            if is_selected {
                                node_border = selection_border_color;
                                border_w = selection_border_width;
                            }

                            // Compute state flags
                            // Note: Hover glow disabled for cleaner UX
                            let mut flags = 0u32;
                            if is_selected {
                                flags |= effects::NodeFlags::SELECTED;
                            }

                            effects::Node {
                                position: node_layout.bounds().position().into_euclid().to_vector()
                                    + offset,
                                size: node_layout.bounds().size().into_euclid(),
                                corner_radius: corner_rad,
                                border_width: border_w,
                                opacity,
                                fill_color: node_fill,
                                border_color: node_border,
                                pins: find_pins(node_tree, node_layout)
                                    .iter()
                                    .map(|(_pin_index, pin_state, (a, _b))| effects::Pin {
                                        side: pin_state.side.into(),
                                        offset: a.into_euclid().to_vector() + offset,
                                        // Use resolved pin defaults, but pin color comes from the pin widget
                                        radius: resolved_pin_defaults.radius,
                                        color: pin_state.color,
                                        direction: pin_state.direction,
                                        shape: resolved_pin_defaults.shape,
                                        border_color: resolved_pin_defaults.border_color.unwrap_or(iced::Color::TRANSPARENT),
                                        border_width: resolved_pin_defaults.border_width,
                                    })
                                    .collect(),
                                shadow_offset,
                                shadow_blur,
                                shadow_color,
                                flags,
                            }
                        },
                    )
                    .collect()
            },
            // Extract edge connectivity with style resolved through cascade
            edges: self
                .edges
                .iter()
                .map(|(from, to, edge_style)| {
                    // Resolve edge style through cascade:
                    // Theme Defaults -> Graph Defaults -> Per-Edge Style
                    let edge_config = edge_style.as_ref().map(|s| crate::style::EdgeConfig::from(s.clone()));
                    let resolved_edge = resolver.resolve_edge(edge_config.as_ref());
                    EdgeData {
                        from_node: from.node_id,
                        from_pin: from.pin_id,
                        to_node: to.node_id,
                        to_pin: to.pin_id,
                        style: resolved_edge,
                    }
                })
                .collect(),
            edge_color,
            background_color: bg_color,
            border_color,
            fill_color,
            drag_edge_color,
            drag_edge_valid_color: drag_valid_color,
            selected_nodes: state.selected_nodes.clone(),
            selected_edge_color: glam::vec4(
                selection_border_color.r,
                selection_border_color.g,
                selection_border_color.b,
                selection_border_color.a,
            ),
            edge_thickness: resolved_edge_defaults.thickness,
        };
        let mut primitive_foreground = primitive_background.clone();
        primitive_foreground.layer = Layer::Foreground;

        // Draw the background primitive
        renderer.draw_primitive(layout.bounds(), primitive_background);

        // Draw child elements with camera transformation
        camera.draw_with::<_, Renderer>(
            renderer,
            viewport,
            cursor,
            |renderer, viewport, cursor| {
                for (node_index, (((_position, element, _style), tree), layout)) in self
                    .elements_iter()
                    .zip(&tree.children)
                    .zip(layout.children())
                    .enumerate()
                {
                    let is_selected = state.selected_nodes.contains(&node_index);

                    // Calculate offset for single node drag
                    let single_node_offset =
                        if let Dragging::Node(dragging_node_index, origin) = state.dragging {
                            cursor
                                .position()
                                .filter(|_| dragging_node_index == node_index)
                                .map(|cursor_position| cursor_position - origin.into_iced())
                        } else {
                            None
                        };

                    // Calculate offset for group move (all selected nodes)
                    let group_move_offset = if let Dragging::GroupMove(origin) = state.dragging {
                        if is_selected {
                            cursor
                                .position()
                                .map(|cursor_position| cursor_position - origin.into_iced())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let node_move_offset = single_node_offset
                        .or(group_move_offset)
                        .unwrap_or(Vector::ZERO);

                    renderer.with_translation(node_move_offset, |renderer| {
                        element
                            .as_widget()
                            .draw(tree, renderer, theme, style, layout, cursor, &viewport);
                    });
                }
            },
        );

        // Draw the foreground primitive (includes BoxSelect and EdgeCutting via shader)
        renderer.draw_primitive(layout.bounds(), primitive_foreground);
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
        if let Some(external) = self.get_external_selection() {
            if state.selected_nodes != *external {
                state.selected_nodes = external.clone();
            }
        }

        // Update time for animations
        let now = Instant::now();

        if let Some(last_update) = state.last_update {
            let delta = now.duration_since(last_update).as_secs_f32();
            state.time += delta;
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
                        let node_ids: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        if let Some(handler) = self.on_clone_handler() {
                            shell.publish(handler(node_ids.clone()));
                        }
                        if let Some(handler) = self.get_on_event() {
                            shell.publish(handler(NodeGraphEvent::CloneRequested { node_ids }));
                        }
                        shell.capture_event();
                    }
                }
                // Ctrl+A: Select all nodes
                keyboard::Key::Character(c) if c.as_str() == "a" && modifiers.command() => {
                    let count = self.nodes.len();
                    state.selected_nodes = (0..count).collect();
                    let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(selected.clone()));
                    }
                    if let Some(handler) = self.get_on_event() {
                        shell.publish(handler(NodeGraphEvent::SelectionChanged { selected }));
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
                            shell.publish(handler(NodeGraphEvent::SelectionChanged {
                                selected: vec![],
                            }));
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }
                // Delete/Backspace: Delete selected nodes
                keyboard::Key::Named(keyboard::key::Named::Delete)
                | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                    if !state.selected_nodes.is_empty() {
                        let node_ids: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        if let Some(handler) = self.on_delete_handler() {
                            shell.publish(handler(node_ids.clone()));
                        }
                        if let Some(handler) = self.get_on_event() {
                            shell.publish(handler(NodeGraphEvent::DeleteRequested { node_ids }));
                        }
                        state.selected_nodes.clear();
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }
                _ => {}
            }
        }

        // Track left mouse button state globally (for Fruit Ninja edge cutting)
        match event {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.left_mouse_down = false;
            }
            _ => {}
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) => {
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

                    #[cfg(debug_assertions)]
                    println!(
                        "\n=== ZOOM: {:.2} + delta={:.2} at screen={:?} ===",
                        state.camera.zoom(),
                        zoom_delta,
                        cursor_pos
                    );

                    state.camera = state.camera.zoom_at(cursor_pos, zoom_delta);

                    #[cfg(debug_assertions)]
                    println!(
                        "  New camera: zoom={:.2}, position={:?}",
                        state.camera.zoom(),
                        state.camera.position()
                    );
                }
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
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
                // println!("camera: {:?}", state.camera);
                // println!("cursor: {:?}", cursor);
                // println!("viewport: {:?}", viewport);
                // println!("state.offset: {:?}", state.offset);
                // println!("state.zoom: {:?}", state.zoom);

                if state.dragging != Dragging::None {
                    match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            // Emit drag update event with current cursor position
                            if let Some(cursor_position) = world_cursor.position() {
                                if let Some(handler) = self.on_drag_update_handler() {
                                    shell.publish(handler(cursor_position.x, cursor_position.y));
                                }
                            }
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
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
                                if let Dragging::EdgeCutting { ref mut trail, ref mut pending_cuts } = state.dragging {
                                    trail.push(cursor_position);

                                    // Get cutting line: from start point to current cursor
                                    let cut_start = trail.first().copied().unwrap_or(cursor_position);
                                    let cut_end = cursor_position;

                                    // Clear and recalculate - only edges intersecting cutting line are highlighted
                                    pending_cuts.clear();

                                    // Check each edge for intersection with the cutting line
                                    for (edge_idx, (from_ref, to_ref, _style)) in self.edges.iter().enumerate() {
                                        let (from_node, from_pin) = (from_ref.node_id, from_ref.pin_id);
                                        let (to_node, to_pin) = (to_ref.node_id, to_ref.pin_id);

                                        // Get pin positions and sides for bezier calculation
                                        let from_pin_data = layout.children().nth(from_node).and_then(|node_layout| {
                                            tree.children.get(from_node).and_then(|node_tree| {
                                                find_pins(node_tree, node_layout).get(from_pin).map(|(_, state, (pos, _))| (*pos, state.side))
                                            })
                                        });
                                        let to_pin_data = layout.children().nth(to_node).and_then(|node_layout| {
                                            tree.children.get(to_node).and_then(|node_tree| {
                                                find_pins(node_tree, node_layout).get(to_pin).map(|(_, state, (pos, _))| (*pos, state.side))
                                            })
                                        });

                                        if let (Some((p0, from_side)), Some((p3, to_side))) = (from_pin_data, to_pin_data) {
                                            // Calculate bezier control points (same as shader)
                                            let seg_len = 80.0;
                                            let dir_from = pin_side_to_direction(from_side);
                                            let dir_to = pin_side_to_direction(to_side);
                                            let p1 = Point::new(p0.x + dir_from.0 * seg_len, p0.y + dir_from.1 * seg_len);
                                            let p2 = Point::new(p3.x + dir_to.0 * seg_len, p3.y + dir_to.1 * seg_len);

                                            // Check if cutting line intersects this bezier edge
                                            if line_intersects_bezier(
                                                cut_start.into_iced(),
                                                cut_end.into_iced(),
                                                p0, p1, p2, p3,
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
                                        if let Some(handler) = self.on_disconnect_handler() {
                                            shell.publish(handler(*from_ref, *to_ref));
                                        }
                                        if let Some(handler) = self.get_on_event() {
                                            shell.publish(handler(NodeGraphEvent::EdgeDisconnected {
                                                from: *from_ref,
                                                to: *to_ref,
                                            }));
                                        }
                                    }
                                }
                            }
                            state.dragging = Dragging::None;
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                    Dragging::Graph(origin) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                            if let Some(cursor_position) = screen_cursor.position() {
                                let screen_to_world = state.camera.screen_to_world();
                                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                                let cursor_position: WorldPoint = screen_to_world.transform_point(cursor_position);
                                let offset = cursor_position - origin;
                                state.camera = state.camera.move_by(offset);
                            }
                            state.dragging = Dragging::None;
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                    Dragging::Node(node_index, origin) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position = cursor_position.into_euclid();
                                let offset = cursor_position - origin;
                                let new_position = self.nodes[node_index].0 + offset.into_iced();

                                // Call on_move handler if set
                                if let Some(handler) = self.on_move_handler() {
                                    let message = handler(node_index, new_position);
                                    shell.publish(message);
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphEvent::NodeMoved {
                                        node_id: node_index,
                                        position: new_position,
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
                    Dragging::Edge(from_node, from_pin, _) => match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            // Check if cursor is over a pin to transition to EdgeOver
                            if let Some(cursor_position) = world_cursor.position() {
                                let mut target_pin: Option<(usize, usize)> = None;

                                // Get the source pin state for validation
                                let from_pin_state = find_pins(&tree.children[from_node], layout.children().nth(from_node).unwrap())
                                    .get(from_pin)
                                    .map(|(_, state, _)| *state);

                                for (node_index, (node_layout, node_tree)) in
                                    layout.children().zip(&tree.children).enumerate()
                                {
                                    for (pin_index, pin_state, (a, b)) in find_pins(node_tree, node_layout) {
                                        // Pin positions are already in world space (from layout)
                                        let distance = a
                                            .distance(cursor_position)
                                            .min(b.distance(cursor_position));
                                        if distance < PIN_CLICK_THRESHOLD {
                                            // Don't connect to the same pin we're dragging from
                                            if node_index != from_node || pin_index != from_pin {
                                                // Validate pin connection (direction and type compatibility)
                                                if let Some(from_state) = from_pin_state {
                                                    if validate_pin_connection(from_state, pin_state) {
                                                        target_pin = Some((node_index, pin_index));
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if target_pin.is_some() {
                                        break;
                                    }
                                }

                                if let Some((to_node, to_pin)) = target_pin {
                                    #[cfg(debug_assertions)]
                                    println!("  ✓ SNAP TO PIN: node={}, pin={}", to_node, to_pin);

                                    // Fire EdgeConnected event immediately on snap (plug behavior)
                                    let from_ref = PinReference::new(from_node, from_pin);
                                    let to_ref = PinReference::new(to_node, to_pin);
                                    if let Some(handler) = self.on_connect_handler() {
                                        shell.publish(handler(from_ref, to_ref));
                                    }
                                    if let Some(handler) = self.get_on_event() {
                                        shell.publish(handler(NodeGraphEvent::EdgeConnected {
                                            from: from_ref,
                                            to: to_ref,
                                        }));
                                    }

                                    state.dragging = Dragging::EdgeOver(
                                        from_node,
                                        from_pin,
                                        to_node,
                                        to_pin,
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
                            if let Some(cursor_position) = world_cursor.position() {
                                let mut still_over_pin = false;
                                if let Some((node_layout, node_tree)) = layout
                                    .children()
                                    .zip(&tree.children)
                                    .nth(to_node)
                                {
                                    if let Some((_, _, (a, b))) = find_pins(node_tree, node_layout)
                                        .into_iter()
                                        .nth(to_pin)
                                    {
                                        // Pin positions are already in world space (from layout)
                                        let distance = a
                                            .distance(cursor_position)
                                            .min(b.distance(cursor_position));
                                        still_over_pin = distance < PIN_CLICK_THRESHOLD;
                                    }
                                }

                                if !still_over_pin {
                                    // Fire EdgeDisconnected event when leaving snap (plug behavior)
                                    let from_ref = PinReference::new(from_node, from_pin);
                                    let to_ref = PinReference::new(to_node, to_pin);
                                    if let Some(handler) = self.on_disconnect_handler() {
                                        shell.publish(handler(from_ref, to_ref));
                                    }
                                    if let Some(handler) = self.get_on_event() {
                                        shell.publish(handler(NodeGraphEvent::EdgeDisconnected {
                                            from: from_ref,
                                            to: to_ref,
                                        }));
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
                            #[cfg(debug_assertions)]
                            println!("  ✓ DRAG COMPLETE (edge already connected): node {} pin {} -> node {} pin {}\n", from_node, from_pin, to_node, to_pin);

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
                                state.dragging = Dragging::BoxSelect(start, cursor_position.into_euclid());
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

                                #[cfg(debug_assertions)]
                                println!("Box selection complete: {} nodes selected", state.selected_nodes.len());

                                // Notify selection change
                                let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                                if let Some(handler) = self.on_select_handler() {
                                    shell.publish(handler(selected.clone()));
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphEvent::SelectionChanged { selected }));
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

                                #[cfg(debug_assertions)]
                                println!("Group move complete: offset={:?}", offset);

                                // Call on_group_move handler with selected nodes and offset
                                let node_ids: Vec<usize> = state.selected_nodes.iter().copied().collect();
                                let delta = offset.into_iced();
                                if let Some(handler) = self.on_group_move_handler() {
                                    shell.publish(handler(node_ids.clone(), delta));
                                }
                                if let Some(handler) = self.get_on_event() {
                                    shell.publish(handler(NodeGraphEvent::GroupMoved { node_ids, delta }));
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
                    // Edge vertex dragging (for physics wire simulation)
                    Dragging::EdgeVertex { edge_index, vertex_index, origin } => match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            // TODO: Implement vertex drag with physics impulse
                            // For now, just mark the edge as dirty
                            let _ = (edge_index, vertex_index, origin);
                            shell.request_redraw();
                        }
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            state.dragging = Dragging::None;
                            if let Some(handler) = self.on_drag_end_handler() {
                                shell.publish(handler());
                            }
                            shell.capture_event();
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
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                        // Track left mouse button state for Fruit Ninja edge cutting
                        state.left_mouse_down = true;

                        // === MEASUREMENT POINT: Mouse Click ===
                        #[cfg(debug_assertions)]
                        {
                            if let Some(screen_pos) = screen_cursor.position() {
                                let screen_pos_euclid: ScreenPoint = screen_pos.into_euclid();
                                let world_pos = state.camera.screen_to_world().transform_point(screen_pos_euclid);
                                println!(
                                    "\n=== CLICK: screen={:?}, world={:?}, zoom={:.2}, cam_pos={:?} ===",
                                    screen_pos, world_pos, state.camera.zoom(), state.camera.position()
                                );
                            }
                        }

                        // Ctrl+Click: Edge cut tool
                        if state.modifiers.command() {
                            if let Some(cursor_position) = world_cursor.position() {
                                // Check if click is near any edge
                                for (from_ref, to_ref, _style) in &self.edges {
                                    let (from_node, from_pin) = (from_ref.node_id, from_ref.pin_id);
                                    let (to_node, to_pin) = (to_ref.node_id, to_ref.pin_id);
                                    // Get pin positions for both ends of the edge
                                    let from_pin_pos = layout.children().nth(from_node).and_then(|node_layout| {
                                        tree.children.get(from_node).and_then(|node_tree| {
                                            find_pins(node_tree, node_layout).get(from_pin).map(|(_, _, (a, _))| *a)
                                        })
                                    });
                                    let to_pin_pos = layout.children().nth(to_node).and_then(|node_layout| {
                                        tree.children.get(to_node).and_then(|node_tree| {
                                            find_pins(node_tree, node_layout).get(to_pin).map(|(_, _, (a, _))| *a)
                                        })
                                    });

                                    if let (Some(from_pos), Some(to_pos)) = (from_pin_pos, to_pin_pos) {
                                        // Check if cursor is near the edge line (using simple distance to line segment)
                                        let distance = point_to_line_distance(cursor_position, from_pos, to_pos);
                                        const EDGE_CUT_THRESHOLD: f32 = 10.0;

                                        if distance < EDGE_CUT_THRESHOLD {
                                            #[cfg(debug_assertions)]
                                            println!("Edge cut: disconnecting {} pin {} -> {} pin {}", from_node, from_pin, to_node, to_pin);

                                            if let Some(handler) = self.on_disconnect_handler() {
                                                shell.publish(handler(*from_ref, *to_ref));
                                            }
                                            if let Some(handler) = self.get_on_event() {
                                                shell.publish(handler(NodeGraphEvent::EdgeDisconnected {
                                                    from: *from_ref,
                                                    to: *to_ref,
                                                }));
                                            }
                                            shell.capture_event();
                                            shell.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(cursor_position) = world_cursor.position() {
                            // check bounds for pins
                            for (node_index, (node_layout, node_tree)) in
                                layout.children().zip(&mut tree.children).enumerate()
                            {
                                let pins = find_pins(node_tree, node_layout);
                                #[cfg(debug_assertions)]
                                if !pins.is_empty() {
                                    println!("  Node {} has {} pins at node_bounds={:?}", node_index, pins.len(), node_layout.bounds());
                                    for (idx, _, (pin_pos, _)) in &pins {
                                        println!("    Pin {} at world position: {:?}", idx, pin_pos);
                                    }
                                }

                                for (pin_index, _, (a, b)) in pins {
                                    // Pin positions from layout are ALREADY in world space
                                    // because layout was created with .move_to(world_position)
                                    let distance = a
                                        .distance(cursor_position)
                                        .min(b.distance(cursor_position));

                                    #[cfg(debug_assertions)]
                                    if distance < 10.0 {  // Log if we're anywhere near (increased threshold for visibility)
                                        println!(
                                            "  PIN CHECK: node={}, pin={}, pin_world={:?}, cursor_world={:?}, distance={:.2}",
                                            node_index, pin_index, a, cursor_position, distance
                                        );
                                    }

                                    if distance < PIN_CLICK_THRESHOLD {
                                        #[cfg(debug_assertions)]
                                        println!("  ✓ PIN HIT!");

                                        // Check if this pin has existing connections
                                        // If it does, "unplug" the clicked end (like pulling a cable)
                                        for (from_ref, to_ref, _style) in &self.edges {
                                    let (from_node, from_pin) = (from_ref.node_id, from_ref.pin_id);
                                    let (to_node, to_pin) = (to_ref.node_id, to_ref.pin_id);
                                            // If we clicked the "from" pin, unplug FROM and drag it
                                            // Keep TO pin connected, drag away from it
                                            if from_node == node_index && from_pin == pin_index {
                                                #[cfg(debug_assertions)]
                                                println!(
                                                    "  Unplugging FROM pin - keep TO pin at node {} pin {}, drag FROM end",
                                                    to_node, to_pin
                                                );

                                                // Disconnect the edge
                                                if let Some(handler) = self.on_disconnect_handler() {
                                                    shell.publish(handler(*from_ref, *to_ref));
                                                }
                                                if let Some(handler) = self.get_on_event() {
                                                    shell.publish(handler(NodeGraphEvent::EdgeDisconnected {
                                                        from: *from_ref,
                                                        to: *to_ref,
                                                    }));
                                                }

                                                // Start dragging FROM the TO pin (the end that stays connected)
                                                // We're now dragging back towards the TO pin
                                                let state = tree.state.downcast_mut::<NodeGraphState>();
                                                state.dragging = Dragging::Edge(
                                                    to_node,
                                                    to_pin,
                                                    cursor_position.into_euclid(),
                                                );
                                                shell.capture_event();
                                                return;
                                            }
                                            // If we clicked the "to" pin, unplug TO and drag it
                                            // Keep FROM pin connected, drag away from it
                                            else if to_node == node_index && to_pin == pin_index {
                                                #[cfg(debug_assertions)]
                                                println!(
                                                    "  Unplugging TO pin - keep FROM pin at node {} pin {}, drag TO end",
                                                    from_node, from_pin
                                                );

                                                // Disconnect the edge
                                                if let Some(handler) = self.on_disconnect_handler() {
                                                    shell.publish(handler(*from_ref, *to_ref));
                                                }
                                                if let Some(handler) = self.get_on_event() {
                                                    shell.publish(handler(NodeGraphEvent::EdgeDisconnected {
                                                        from: *from_ref,
                                                        to: *to_ref,
                                                    }));
                                                }

                                                // Start dragging FROM the FROM pin (the end that stays connected)
                                                // We're now dragging away from the FROM pin
                                                let state = tree.state.downcast_mut::<NodeGraphState>();
                                                state.dragging = Dragging::Edge(
                                                    from_node,
                                                    from_pin,
                                                    cursor_position.into_euclid(),
                                                );
                                                shell.capture_event();
                                                return;
                                            }
                                        }

                                        // If no existing connection, start a new drag
                                        let state = tree.state.downcast_mut::<NodeGraphState>();
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
                                    let already_selected = state.selected_nodes.contains(&node_index);
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
                                    let new_selection: Vec<usize> = state.selected_nodes.iter().copied().collect();

                                    #[cfg(debug_assertions)]
                                    println!("node {:?} clicked, selected: {:?}", node_index, state.selected_nodes);

                                    // Decide between single node drag or group move
                                    if state.selected_nodes.len() > 1 && state.selected_nodes.contains(&node_index) {
                                        // Multiple nodes selected, start group move
                                        let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                                        state.dragging = Dragging::GroupMove(cursor_position.into_euclid());
                                        // Emit drag start event for group
                                        if let Some(handler) = self.on_drag_start_handler() {
                                            shell.publish(handler(DragInfo::Group { node_ids: selected }));
                                        }
                                    } else {
                                        // Single node drag
                                        state.dragging = Dragging::Node(node_index, cursor_position.into_euclid());
                                        // Emit drag start event for single node
                                        if let Some(handler) = self.on_drag_start_handler() {
                                            shell.publish(handler(DragInfo::Node { node_id: node_index }));
                                        }
                                    }

                                    // Notify selection change
                                    if selection_changed {
                                        if let Some(handler) = self.on_select_handler() {
                                            shell.publish(handler(new_selection.clone()));
                                        }
                                        if let Some(handler) = self.get_on_event() {
                                            shell.publish(handler(NodeGraphEvent::SelectionChanged { selected: new_selection }));
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
                                #[cfg(debug_assertions)]
                                println!("Starting edge cutting from {:?}", cursor_position);
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

                            #[cfg(debug_assertions)]
                            println!("starting box selection from {:?}", cursor_position);
                            state.dragging = Dragging::BoxSelect(cursor_position, cursor_position);
                            // Emit drag start event for box select
                            if let Some(handler) = self.on_drag_start_handler() {
                                shell.publish(handler(DragInfo::BoxSelect {
                                    start_x: cursor_position.x,
                                    start_y: cursor_position.y,
                                }));
                            }
                            shell.capture_event();
                            return;
                        }
                    }
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                        // Right-click: start graph panning
                        if let Some(cursor_position) = screen_cursor.position() {
                            let cursor_position: ScreenPoint = cursor_position.into_euclid();
                            let cursor_position: WorldPoint = state.camera.screen_to_world().transform_point(cursor_position);
                            #[cfg(debug_assertions)]
                            println!("dragging graph from {:?}", cursor_position);
                            let state = tree.state.downcast_mut::<NodeGraphState>();
                            state.dragging = Dragging::Graph(cursor_position.into_euclid());
                            shell.capture_event();
                            return;
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

impl<'a, Message, Renderer> From<NodeGraph<'a, Message, iced::Theme, Renderer>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer + 'a + iced_wgpu::primitive::Renderer,
    Message: 'static,
{
    fn from(graph: NodeGraph<'a, Message, iced::Theme, Renderer>) -> Self {
        Element::new(graph)
    }
}

pub fn node_graph<'a, Message, Theme, Renderer>() -> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    NodeGraph::default()
}

//// Helper function to find all NodePin elements in the tree - OF A Node!!!
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

/// Validates if two pins can be connected based on direction and type.
/// Returns true if connection is valid.
fn validate_pin_connection(from_pin: &NodePinState, to_pin: &NodePinState) -> bool {
    use crate::node_pin::PinDirection;

    // Check direction compatibility:
    // - Output can connect to Input or Both
    // - Input can connect to Output or Both
    // - Both can connect to anything
    let direction_valid = match (from_pin.direction, to_pin.direction) {
        // Both can connect to anything
        (PinDirection::Both, _) | (_, PinDirection::Both) => true,
        // Output -> Input or Input -> Output is valid
        (PinDirection::Output, PinDirection::Input)
        | (PinDirection::Input, PinDirection::Output) => true,
        // Same direction is not allowed (Output->Output or Input->Input)
        _ => false,
    };

    // Check type compatibility (empty string or "any" matches everything)
    let type_valid = from_pin.pin_type == to_pin.pin_type
        || from_pin.pin_type == "any"
        || to_pin.pin_type == "any"
        || from_pin.pin_type.is_empty()
        || to_pin.pin_type.is_empty();

    direction_valid && type_valid
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

// Helper function to draw a simple line between two points
#[allow(dead_code)]
fn draw_line<Renderer>(renderer: &mut Renderer, from: Point, to: Point, width: f32, color: Color)
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    // Simple line drawing using small rectangles
    let distance = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
    if distance < 0.1 {
        return; // Too short to draw
    }

    // Draw line as series of small rectangles
    let steps = (distance / 3.0).ceil() as usize; // Smaller step size for smoother lines
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let point = Point::new(from.x + t * (to.x - from.x), from.y + t * (to.y - from.y));

        let segment_bounds = Rectangle::new(
            Point::new(point.x - width / 2.0, point.y - width / 2.0),
            Size::new(width, width),
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: segment_bounds,
                border: border::Border::default(),
                ..Default::default()
            },
            Background::Color(color),
        );
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

/// Checks if two line segments intersect.
/// Returns true if segments (a1,a2) and (b1,b2) cross each other.
fn line_segments_intersect(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    // Using cross product method for line segment intersection
    fn cross(o: Point, a: Point, b: Point) -> f32 {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    }

    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);

    // Check if segments straddle each other
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }

    // Check for collinear cases (endpoint on segment)
    fn on_segment(p: Point, q: Point, r: Point) -> bool {
        q.x <= p.x.max(r.x) && q.x >= p.x.min(r.x) && q.y <= p.y.max(r.y) && q.y >= p.y.min(r.y)
    }

    if d1.abs() < 0.0001 && on_segment(b1, a1, b2) {
        return true;
    }
    if d2.abs() < 0.0001 && on_segment(b1, a2, b2) {
        return true;
    }
    if d3.abs() < 0.0001 && on_segment(a1, b1, a2) {
        return true;
    }
    if d4.abs() < 0.0001 && on_segment(a1, b2, a2) {
        return true;
    }

    false
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
        if t >= 0.0 && t <= 1.0 {
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
