//! The `update` event path of [`NodeGraph`]: the `Dragging` state machine
//! and its update-exclusive hit-test helpers.
//!
//! Split out of `widget.rs` mechanically.

use super::*;

// Click detection threshold (in world-space pixels)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary)
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

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
    pub(super) fn update_impl(
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
            state.camera = crate::node_graph::camera::Camera2D::with_zoom_and_position(
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
