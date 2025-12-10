use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector, border, keyboard,
};
use iced_widget::core::{
    Clipboard, Layout, Shell, layout, mouse, renderer,
    widget::{self, Tree, tree},
};
use web_time::Instant;

use super::{
    DragInfo, NodeGraph,
    effects::{self, Layer},
    euclid::{IntoIced, WorldVector},
    state::{Dragging, NodeGraphState},
};
use crate::{
    PinSide,
    node_grapgh::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::NodePinState,
    style::is_dark_theme,
};

// Click detection threshold (in world-space pixels)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

impl<Message, Theme, Renderer> iced_widget::core::Widget<Message, Theme, Renderer>
    for NodeGraph<'_, Message, Theme, Renderer>
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
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let mut camera = state.camera;

        // Update time for animations
        let (time, now) = {
            let now = Instant::now();
            let time = if let Some(last_update) = state.last_update {
                let delta = now.duration_since(last_update).as_secs_f32();
                state.time + delta
            } else {
                state.time
            };
            (time, now)
        };

        // Get fade-in opacity for smooth appearance
        let fade_opacity = state.fade_in.interpolate(0.0, 1.0, now);

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

        // Theme-aware colors from extended palette
        let text_color = style.text_color;

        // Use proper relative luminance for theme detection
        // Light text (high luminance) indicates dark background theme
        let is_dark = is_dark_theme(text_color);

        // Check for user-provided graph style
        let graph_style = self.get_graph_style();

        // Derive colors from theme or use graph style if provided
        // Apply fade_opacity to all colors
        let (bg_color, border_color, fill_color, edge_color, drag_edge_color, drag_valid_color) =
            if let Some(gs) = graph_style {
                // User provided graph style - use their colors
                let bg = glam::vec4(
                    gs.background_color.r,
                    gs.background_color.g,
                    gs.background_color.b,
                    gs.background_color.a * fade_opacity,
                );
                let border = glam::vec4(
                    gs.grid_color.r,
                    gs.grid_color.g,
                    gs.grid_color.b,
                    gs.grid_color.a * fade_opacity,
                );
                // Derive fill from background (slightly lighter/darker)
                let fill = if is_dark {
                    glam::vec4(
                        gs.background_color.r + 0.06,
                        gs.background_color.g + 0.06,
                        gs.background_color.b + 0.07,
                        fade_opacity * 0.75,
                    )
                } else {
                    glam::vec4(
                        gs.background_color.r - 0.08,
                        gs.background_color.g - 0.08,
                        gs.background_color.b - 0.07,
                        fade_opacity * 0.75,
                    )
                };
                let edge = glam::vec4(
                    text_color.r,
                    text_color.g,
                    text_color.b,
                    text_color.a * fade_opacity,
                );
                let drag = glam::vec4(
                    gs.drag_edge_color.r,
                    gs.drag_edge_color.g,
                    gs.drag_edge_color.b,
                    gs.drag_edge_color.a * fade_opacity,
                );
                let valid = glam::vec4(
                    gs.drag_edge_valid_color.r,
                    gs.drag_edge_valid_color.g,
                    gs.drag_edge_valid_color.b,
                    gs.drag_edge_valid_color.a * fade_opacity,
                );
                (bg, border, fill, edge, drag, valid)
            } else if is_dark {
                // Dark theme: use darker backgrounds with subtle highlights
                let bg = glam::vec4(0.08, 0.08, 0.09, fade_opacity);
                let border = glam::vec4(0.20, 0.20, 0.22, fade_opacity);
                let fill = glam::vec4(0.14, 0.14, 0.16, fade_opacity);
                let edge = glam::vec4(
                    text_color.r,
                    text_color.g,
                    text_color.b,
                    text_color.a * fade_opacity,
                );
                // Drag colors: warning (orange-ish) and success (green-ish)
                let drag = glam::vec4(0.9, 0.6, 0.3, fade_opacity);
                let valid = glam::vec4(0.3, 0.8, 0.5, fade_opacity);
                (bg, border, fill, edge, drag, valid)
            } else {
                // Light theme: use lighter backgrounds with more contrast
                let bg = glam::vec4(0.92, 0.92, 0.93, fade_opacity);
                let border = glam::vec4(0.70, 0.70, 0.72, fade_opacity);
                let fill = glam::vec4(0.84, 0.84, 0.86, fade_opacity);
                let edge = glam::vec4(
                    text_color.r,
                    text_color.g,
                    text_color.b,
                    text_color.a * fade_opacity,
                );
                // Drag colors: darker for light theme
                let drag = glam::vec4(0.8, 0.5, 0.2, fade_opacity);
                let valid = glam::vec4(0.2, 0.7, 0.4, fade_opacity);
                (bg, border, fill, edge, drag, valid)
            };

        // Get selection style from graph_style or use defaults
        let default_selection_style = crate::style::SelectionStyle::default();
        let selection_style = graph_style
            .map(|gs| &gs.selection_style)
            .unwrap_or(&default_selection_style);
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

                            // Use per-node style if provided, otherwise use theme defaults
                            let (node_fill, mut node_border, corner_rad, mut border_w, opacity) =
                                if let Some(style) = node_style {
                                    (
                                        style.fill_color,
                                        style.border_color,
                                        style.corner_radius,
                                        style.border_width,
                                        style.opacity * fade_opacity,
                                    )
                                } else {
                                    (
                                        iced::Color::from_rgba(
                                            fill_color.x,
                                            fill_color.y,
                                            fill_color.z,
                                            fill_color.w,
                                        ),
                                        iced::Color::from_rgba(
                                            border_color.x,
                                            border_color.y,
                                            border_color.z,
                                            border_color.w,
                                        ),
                                        5.0,
                                        1.0,
                                        fade_opacity * 0.75,
                                    )
                                };

                            // Apply selection highlighting
                            if is_selected {
                                node_border = selection_border_color;
                                border_w = selection_border_width;
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
                                        radius: 5.0,
                                        color: pin_state.color,
                                        direction: pin_state.direction,
                                    })
                                    .collect(),
                            }
                        },
                    )
                    .collect()
            },
            // Extract edge connectivity without style for GPU primitive (style used separately)
            edges: self
                .edges
                .iter()
                .map(|(from, to, _style)| ((from.node_id, from.pin_id), (to.node_id, to.pin_id)))
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
                selection_border_color.a * fade_opacity,
            ),
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
        let children: Vec<&Element<'_, Message, Theme, Renderer>> =
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

        // Start fade-in animation on first update
        if state.last_update.is_none() {
            state.fade_in.go_mut(true, now);
        }

        if let Some(last_update) = state.last_update {
            let delta = now.duration_since(last_update).as_secs_f32();
            state.time += delta;
        }
        state.last_update = Some(now);

        // Request redraw while animating
        if state.fade_in.is_animating(now) {
            shell.request_redraw();
        }

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
                        if let Some(handler) = self.on_clone_handler() {
                            let selected: Vec<usize> =
                                state.selected_nodes.iter().copied().collect();
                            shell.publish(handler(selected));
                        }
                        shell.capture_event();
                    }
                }
                // Ctrl+A: Select all nodes
                keyboard::Key::Character(c) if c.as_str() == "a" && modifiers.command() => {
                    let count = self.nodes.len();
                    state.selected_nodes = (0..count).collect();
                    if let Some(handler) = self.on_select_handler() {
                        let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        shell.publish(handler(selected));
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
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }
                // Delete/Backspace: Delete selected nodes
                keyboard::Key::Named(keyboard::key::Named::Delete)
                | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                    if !state.selected_nodes.is_empty() {
                        if let Some(handler) = self.on_delete_handler() {
                            let selected: Vec<usize> =
                                state.selected_nodes.iter().copied().collect();
                            shell.publish(handler(selected));
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

                match state.dragging.clone() {
                    Dragging::None => {}
                    Dragging::EdgeCutting(_) => match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position: WorldPoint = cursor_position.into_euclid();

                                // Add point to trail
                                if let Dragging::EdgeCutting(ref mut trail) = state.dragging {
                                    trail.push(cursor_position);
                                }

                                // Check each edge for intersection with the cutting line
                                for (from_ref, to_ref, _style) in &self.edges {
                                    let (from_node, from_pin) = (from_ref.node_id, from_ref.pin_id);
                                    let (to_node, to_pin) = (to_ref.node_id, to_ref.pin_id);
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
                                        let distance = point_to_line_distance(cursor_position.into_iced(), from_pos, to_pos);
                                        const EDGE_CUT_THRESHOLD: f32 = 10.0;

                                        if distance < EDGE_CUT_THRESHOLD {
                                            #[cfg(debug_assertions)]
                                            println!("Edge cut: {} pin {} -> {} pin {}", from_node, from_pin, to_node, to_pin);

                                            if let Some(handler) = self.on_disconnect_handler() {
                                                shell.publish(handler(from_node, from_pin, to_node, to_pin));
                                            }
                                            // Only cut one edge per frame
                                            break;
                                        }
                                    }
                                }
                            }
                            shell.request_redraw();
                        }
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            #[cfg(debug_assertions)]
                            println!("Edge cutting complete");
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
                                    println!("  ✓ HOVER OVER PIN: node={}, pin={}", to_node, to_pin);

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
                            // Connection successful! Call the on_connect handler
                            #[cfg(debug_assertions)]
                            println!("  ✓ CONNECTION COMPLETE: node {} pin {} -> node {} pin {}\n", from_node, from_pin, to_node, to_pin);

                            if let Some(handler) = self.on_connect_handler() {
                                let message = handler(from_node, from_pin, to_node, to_pin);
                                shell.publish(message);
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
                                if let Some(handler) = self.on_select_handler() {
                                    let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
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
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position: WorldPoint = cursor_position.into_euclid();
                                let offset = cursor_position - origin;

                                #[cfg(debug_assertions)]
                                println!("Group move complete: offset={:?}", offset);

                                // Call on_group_move handler with selected nodes and offset
                                if let Some(handler) = self.on_group_move_handler() {
                                    let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                                    shell.publish(handler(selected, offset.into_iced()));
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
                                                shell.publish(handler(from_node, from_pin, to_node, to_pin));
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
                                                    let message = handler(from_node, from_pin, to_node, to_pin);
                                                    shell.publish(message);
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
                                                    let message = handler(from_node, from_pin, to_node, to_pin);
                                                    shell.publish(message);
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
                                            shell.publish(handler(new_selection));
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
                                state.dragging = Dragging::EdgeCutting(vec![cursor_position]);
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

impl<'a, Message, Theme, Renderer> From<NodeGraph<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer + 'a + iced_wgpu::primitive::Renderer,
    Message: 'static,
    Theme: 'a,
{
    fn from(graph: NodeGraph<'a, Message, Theme, Renderer>) -> Self {
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
