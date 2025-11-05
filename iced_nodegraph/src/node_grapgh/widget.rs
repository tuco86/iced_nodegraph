use iced::{Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector, border};
use iced_widget::core::{
    Clipboard, Layout, Shell, layout, mouse, renderer,
    widget::{self, Tree, tree},
};

use super::{
    NodeGraph,
    effects::{self, Layer},
    euclid::{IntoIced, WorldVector},
    state::{Dragging, NodeGraphState},
};
use crate::{
    PinSide,
    node_grapgh::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::NodePinState,
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
        let nodes = self
            .elements_iter_mut()
            .zip(&mut tree.children)
            .map(|((position, element), node_tree)| {
                element
                    .as_widget_mut()
                    .layout(node_tree, renderer, &limits)
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
        #[cfg(not(target_arch = "wasm32"))]
        let time = {
            let now = std::time::Instant::now();
            if let Some(last_update) = state.last_update {
                let delta = now.duration_since(last_update).as_secs_f32();
                state.time + delta
            } else {
                state.time
            }
        };
        
        // On WASM, use performance.now() for animations (in milliseconds)
        #[cfg(target_arch = "wasm32")]
        let time = {
            use wasm_bindgen::prelude::*;
            
            #[wasm_bindgen]
            unsafe extern "C" {
                #[wasm_bindgen(js_namespace = performance)]
                fn now() -> f64;
            }
            
            let now_ms = now();
            if let Some(last_update_ms) = state.last_update_ms {
                let delta = (now_ms - last_update_ms) / 1000.0; // Convert to seconds
                state.time + delta as f32
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

        // Theme-aware colors from extended palette
        let text_color = style.text_color;

        // Try to get extended palette if we have iced::Theme
        // If not available, derive from text_color
        let is_dark_theme = (text_color.r + text_color.g + text_color.b) > 1.5;

        // Use simple color derivation that adapts to dark/light themes
        let (bg_color, border_color, fill_color, edge_color, drag_edge_color, drag_valid_color) =
            if is_dark_theme {
                // Dark theme: use darker backgrounds with subtle highlights
                let bg = glam::vec4(0.08, 0.08, 0.09, 1.0);
                let border = glam::vec4(0.20, 0.20, 0.22, 1.0);
                let fill = glam::vec4(0.14, 0.14, 0.16, 1.0);
                let edge = glam::vec4(text_color.r, text_color.g, text_color.b, text_color.a);
                // Drag colors: warning (orange-ish) and success (green-ish)
                let drag = glam::vec4(0.9, 0.6, 0.3, 1.0); // Warm warning
                let valid = glam::vec4(0.3, 0.8, 0.5, 1.0); // Cool success
                (bg, border, fill, edge, drag, valid)
            } else {
                // Light theme: use lighter backgrounds with more contrast
                let bg = glam::vec4(0.92, 0.92, 0.93, 1.0);
                let border = glam::vec4(0.70, 0.70, 0.72, 1.0);
                let fill = glam::vec4(0.84, 0.84, 0.86, 1.0);
                let edge = glam::vec4(text_color.r, text_color.g, text_color.b, text_color.a);
                // Drag colors: darker for light theme
                let drag = glam::vec4(0.8, 0.5, 0.2, 1.0); // Warm warning
                let valid = glam::vec4(0.2, 0.7, 0.4, 1.0); // Cool success
                (bg, border, fill, edge, drag, valid)
            };

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
            nodes: self
                .nodes
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .enumerate()
                .map(
                    |(node_index, (((_position, _element), node_tree), node_layout))| {
                        let mut offset = WorldVector::zero();
                        if let (Dragging::Node(drag_node_index, origin), Some(cursor_position)) =
                            (state.dragging.clone(), cursor.position())
                        {
                            if drag_node_index == node_index {
                                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                                let cursor_position: WorldPoint =
                                    camera.screen_to_world().transform_point(cursor_position);
                                offset = cursor_position - origin
                            }
                        }
                        effects::Node {
                            position: node_layout.bounds().position().into_euclid().to_vector()
                                + offset,
                            size: node_layout.bounds().size().into_euclid(),
                            corner_radius: 5.0,
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
                .collect(),
            edges: self.edges.clone(),
            edge_color,
            background_color: bg_color,
            border_color,
            fill_color,
            drag_edge_color,
            drag_edge_valid_color: drag_valid_color,
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
                for (node_index, (((_position, element), tree), layout)) in self
                    .elements_iter()
                    .zip(&tree.children)
                    .zip(layout.children())
                    .enumerate()
                {
                    let node_move_offset =
                        if let Dragging::Node(dragging_node_index, origin) = state.dragging {
                            cursor
                                .position()
                                .filter(|_| dragging_node_index == node_index)
                                .map(|cursor_position| cursor_position - origin.into_iced())
                        } else {
                            None
                        }
                        .unwrap_or(Vector::ZERO);
                    renderer.with_translation(node_move_offset, |renderer| {
                        element
                            .as_widget()
                            .draw(tree, renderer, theme, style, layout, cursor, &viewport);
                    });
                }
            },
        );

        // Draw the foreground primitive
        renderer.draw_primitive(layout.bounds(), primitive_foreground);
        
        // Log zoom level for debugging (visible in browser console for WASM)
        #[cfg(target_arch = "wasm32")]
        {
            let zoom = state.camera.zoom();
            web_sys::console::log_1(&format!("Zoom: {:.2}x", zoom).into());
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

        // Update time for animations
        #[cfg(not(target_arch = "wasm32"))]
        {
            let now = std::time::Instant::now();
            if let Some(last_update) = state.last_update {
                let delta = now.duration_since(last_update).as_secs_f32();
                state.time += delta;
            }
            state.last_update = Some(now);
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;
            
            #[wasm_bindgen]
            unsafe extern "C" {
                #[wasm_bindgen(js_namespace = performance)]
                fn now() -> f64;
            }
            
            let now_ms = now();
            if let Some(last_update) = state.last_update_ms {
                let delta = (now_ms - last_update) / 1000.0; // Convert to seconds
                state.time += delta as f32;
            }
            state.last_update_ms = Some(now_ms);
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) => {
                if let Some(cursor_pos) = screen_cursor.position() {
                    let cursor_pos: ScreenPoint = cursor_pos.into_euclid();

                    let scroll_amount = match delta {
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                        mouse::ScrollDelta::Lines { y, .. } => *y * 10.0,
                    };

                    // Smaller zoom steps for smoother zooming
                    // In WASM, scroll values tend to be larger
                    let zoom_delta = scroll_amount * 0.001 * state.camera.zoom();

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
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    }
                }

                match state.dragging {
                    Dragging::None => {}
                    Dragging::Graph(origin) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
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
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                }

                for (((_, element), tree), layout) in self
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
                                        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
                                            // If we clicked the "from" pin, unplug FROM and drag it
                                            // Keep TO pin connected, drag away from it
                                            if *from_node == node_index && *from_pin == pin_index {
                                                #[cfg(debug_assertions)]
                                                println!(
                                                    "  Unplugging FROM pin - keep TO pin at node {} pin {}, drag FROM end",
                                                    to_node, to_pin
                                                );

                                                // Disconnect the edge
                                                if let Some(handler) = self.on_disconnect_handler() {
                                                    let message = handler(*from_node, *from_pin, *to_node, *to_pin);
                                                    shell.publish(message);
                                                }

                                                // Start dragging FROM the TO pin (the end that stays connected)
                                                // We're now dragging back towards the TO pin
                                                let state = tree.state.downcast_mut::<NodeGraphState>();
                                                state.dragging = Dragging::Edge(
                                                    *to_node,
                                                    *to_pin,
                                                    cursor_position.into_euclid(),
                                                );
                                                shell.capture_event();
                                                return;
                                            }
                                            // If we clicked the "to" pin, unplug TO and drag it
                                            // Keep FROM pin connected, drag away from it
                                            else if *to_node == node_index && *to_pin == pin_index {
                                                #[cfg(debug_assertions)]
                                                println!(
                                                    "  Unplugging TO pin - keep FROM pin at node {} pin {}, drag TO end",
                                                    from_node, from_pin
                                                );

                                                // Disconnect the edge
                                                if let Some(handler) = self.on_disconnect_handler() {
                                                    let message = handler(*from_node, *from_pin, *to_node, *to_pin);
                                                    shell.publish(message);
                                                }

                                                // Start dragging FROM the FROM pin (the end that stays connected)
                                                // We're now dragging away from the FROM pin
                                                let state = tree.state.downcast_mut::<NodeGraphState>();
                                                state.dragging = Dragging::Edge(
                                                    *from_node,
                                                    *from_pin,
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
                                        shell.capture_event();
                                        return;
                                    }
                                }
                            }

                            // check bounds for nodes
                            for (node_index, node_layout) in layout.children().enumerate() {
                                if world_cursor.is_over(node_layout.bounds()) {
                                    println!("dragging node {:?}", node_index);
                                    let state = tree.state.downcast_mut::<NodeGraphState>();
                                    state.dragging =
                                        Dragging::Node(node_index, cursor_position.into_euclid());
                                    shell.capture_event();
                                    return;
                                }
                            }
                        }
                        if let Some(cursor_position) = screen_cursor.position() {
                            // else drag the whole graph
                            let cursor_position: ScreenPoint = cursor_position.into_euclid();
                            let cursor_position: WorldPoint = state.camera.screen_to_world().transform_point(cursor_position);
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
