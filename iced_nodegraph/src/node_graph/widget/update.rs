//! The `update` event path of [`NodeGraph`]: the `Dragging` state machine
//! and its update-exclusive hit-test helpers.
//!
//! Split out of `widget.rs` mechanically.

use super::*;
use crate::node_graph::camera::Camera2D;
use crate::node_graph::euclid::{WorldRect, WorldSize};
use crate::node_graph::input::KeyAction;
use crate::node_graph::{FocusOptions, FocusTarget};
use iced::touch;

// Click detection threshold (screen px; divide by zoom before comparing
// against world-space distances so the hit target stays constant on screen)
const PIN_CLICK_THRESHOLD: f32 = 8.0;

// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary).
// Screen px, scaled by 1/zoom at the comparison sites like PIN_CLICK_THRESHOLD.
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

// Edge-cut click distance (screen px, scaled by 1/zoom like the above)
const EDGE_CUT_THRESHOLD: f32 = 10.0;

// Touch gesture thresholds: maximum travel (screen px) and duration for a
// press+lift pair to count as a tap.
const TOUCH_TAP_TRAVEL: f32 = 8.0;
const TOUCH_TAP_MAX_SECS: f32 = 0.3;

/// Mutable per-event context threaded through the `update` handlers.
///
/// One instance is built at the top of the `update_with` closure and passed
/// down by `&mut`; handlers destructure it (`let UpdateCtx { tree, shell, .. }
/// = &mut *ctx;`) so disjoint field borrows preserve the inline borrow
/// choreography of the original single-function form (`tree.state` vs
/// `tree.children` vs `shell`).
struct UpdateCtx<'a, 'b, 'm, Message> {
    tree: &'a mut Tree,
    layout: Layout<'b>,
    event: &'a Event,
    world_cursor: mouse::Cursor,
    screen_cursor: mouse::Cursor,
    shell: &'a mut Shell<'m, Message>,
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
            state.camera =
                Camera2D::with_zoom_and_position(zoom, WorldPoint::new(position.x, position.y));
            state.last_synced_view = Some(view);
            // An explicit view() the running tween did not just emit is an
            // app override: it wins and cancels the tween (arbitration rule:
            // explicit view() > user input > running tween > routine sync).
            // A no-op when no tween is running.
            state.camera_tween = None;
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

        // Declarative programmatic focus (`NodeGraph::focus`): resolve the
        // target from live layout and perform the fit exactly once per new
        // `seq` (nonce dedup), mirroring the `view()` / `last_synced_view`
        // pattern above. Unlike the keymap frame actions below this is not
        // gated on `on_pan`: an uncontrolled graph (no `view()`/`on_pan`
        // round trip) can still use `.focus()` to frame content once, since
        // the camera lives in `state` regardless of whether the host
        // observes it (`begin_focus` only *publishes* through `on_pan` when
        // a handler is set).
        if let Some((seq, target, opts)) = &self.focus
            && state.last_focus_seq != Some(*seq)
        {
            state.last_focus_seq = Some(*seq);
            if let Some(world_aabb) = resolve_focus_target(self, layout, state, target) {
                self.begin_focus(state, world_aabb, layout.bounds().size(), opts, shell);
            }
        }

        // Update time for animations
        // Cap delta to prevent large time jumps when app is in background
        let now = Instant::now();

        let mut frame_delta = 0.0_f32;
        if let Some(last_update) = state.last_update {
            let delta = now.duration_since(last_update).as_secs_f32();
            // Cap at 100ms to prevent freeze after background
            let capped_delta = delta.min(0.1);
            state.time += capped_delta;
            frame_delta = capped_delta;
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

            // Advance the focus/frame tween (if any): center-based
            // interpolation with geometric zoom, position recomputed each
            // frame from the frozen viewport/padding via the fit formula so
            // the focused content stays centered throughout. Commits
            // through `on_pan` every frame and keeps `last_synced_view` in
            // step with what it just emitted, so the view()-sync above
            // neither fights it (routine sync suppressed) nor clobbers it
            // once done (arbitration rules above).
            if let Some(tween) = state.camera_tween.as_mut() {
                tween.elapsed += frame_delta;
                let t = if tween.duration > 0.0 {
                    (tween.elapsed / tween.duration).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let e = tween.easing.apply(t);
                let center = WorldPoint::new(
                    tween.start_center.x + (tween.end_center.x - tween.start_center.x) * e,
                    tween.start_center.y + (tween.end_center.y - tween.start_center.y) * e,
                );
                let zoom = tween.start_zoom * (tween.end_zoom / tween.start_zoom).powf(e);
                let position =
                    Camera2D::position_for_center(center, zoom, tween.viewport, tween.padding);
                let viewport_origin = state.camera.viewport_origin();
                state.camera = Camera2D::with_zoom_and_position(zoom, position)
                    .with_viewport_origin(viewport_origin);

                let view = (Point::new(position.x, position.y), zoom);
                if let Some(handler) = self.on_pan_handler() {
                    shell.publish(handler(view.0, view.1));
                }
                state.last_synced_view = Some(view);

                if t < 1.0 {
                    shell.request_redraw();
                } else {
                    state.camera_tween = None;
                }
            }
        }

        // Track keyboard modifiers for Shift/Ctrl selection
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }

        // Handle keyboard shortcuts through the host-configurable keymap
        // (`NodeGraph::keymap`). DeleteSelection is handled AFTER child
        // widgets (further down) so text inputs can consume the key first.
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) = event
        {
            match self.keymap.key_action(key, *physical_key, *modifiers) {
                // Gated on on_clone: without a handler the clone cannot be
                // persisted, so leave the shortcut unhandled and let the key
                // fall through instead of silently swallowing it.
                Some(KeyAction::CloneSelection)
                    if !state.selected_nodes.is_empty() && self.on_clone_handler().is_some() =>
                {
                    let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                    let node_ids = self.translate_node_ids(&indices);
                    if let Some(handler) = self.on_clone_handler() {
                        shell.publish(handler(node_ids));
                    }
                    shell.capture_event();
                }
                Some(KeyAction::SelectAll) => {
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
                Some(KeyAction::ClearSelection) if !state.selected_nodes.is_empty() => {
                    state.selected_nodes.clear();
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(vec![]));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                _ => {}
            }
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

            // User-driven zoom aborts a running focus tween (arbitration:
            // user input beats a tween).
            state.camera_tween = None;
            state.camera = state.camera.zoom_at(cursor_pos, zoom_delta);

            // Commit the new camera (zoom shifts position too).
            if let Some(handler) = self.on_pan_handler() {
                let pos = state.camera.position();
                shell.publish(handler(Point::new(pos.x, pos.y), state.camera.zoom()));
            }

            shell.capture_event();
            shell.request_redraw();
        }

        // Touch: translate the finger stream into the pointer model the rest
        // of this function speaks. Single finger emulates the left button
        // (with a synthesized Available cursor); two fingers pinch-zoom and
        // pan natively and never reach the pointer path. Children see the
        // synthesized mouse events instead of raw touch, so embedded content
        // stays operable by touch without double handling.
        let synthesized = if let Event::Touch(touch_event) = event {
            self.apply_touch(state, touch_event, shell)
        } else {
            None
        };
        let (event, screen_cursor) = match &synthesized {
            Some((event, cursor)) => (event, *cursor),
            None => (event, screen_cursor),
        };

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
                    let mut ctx = UpdateCtx {
                        tree,
                        layout,
                        event,
                        world_cursor,
                        screen_cursor,
                        shell,
                    };
                    let state = ctx.tree.state.downcast_mut::<NodeGraphState>();

                    if state.dragging != Dragging::None
                        && let Event::Mouse(mouse::Event::CursorMoved { .. }) = event
                    {
                        // Emit drag update event with current cursor position
                        if let Some(cursor_position) = world_cursor.position()
                            && let Some(handler) = self.on_drag_update_handler()
                        {
                            ctx.shell.publish(handler(cursor_position));
                        }
                        ctx.shell.capture_event();
                        ctx.shell.request_redraw();
                    }

                    // The `Dragging` state machine, part 1: transitions of an
                    // in-progress drag, one handler per variant. Part 2 - the
                    // `None -> *` entry transitions - is the button-press
                    // dispatch at the bottom of this closure, after child
                    // propagation.
                    match state.dragging.clone() {
                        Dragging::None => {}
                        Dragging::EdgeCutting { .. } => self.handle_edge_cutting(&mut ctx),
                        Dragging::Graph(origin) => self.handle_graph_pan(&mut ctx, origin),
                        Dragging::Node(node_index, origin) => {
                            self.handle_node_drag(&mut ctx, node_index, origin)
                        }
                        Dragging::Edge(from_node, from_pin, _) => {
                            self.handle_edge_drag(&mut ctx, from_node, from_pin)
                        }
                        Dragging::EdgeOver(from_node, from_pin, to_node, to_pin) => {
                            self.handle_edge_over(&mut ctx, from_node, from_pin, to_node, to_pin)
                        }
                        Dragging::BoxSelect(start, _current) => {
                            self.handle_box_select(&mut ctx, start)
                        }
                        Dragging::GroupMove(origin) => self.handle_group_move(&mut ctx, origin),
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
                    let pre_captured = ctx.shell.is_event_captured();
                    for &node_index in z_indices.iter().rev() {
                        let Some((_id, _pos, element, _style, _)) = self.nodes.get_mut(node_index)
                        else {
                            continue;
                        };
                        let Some(child_tree) = ctx.tree.children.get_mut(node_index) else {
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
                            ctx.shell,
                            viewport,
                        );
                        if !pre_captured && ctx.shell.is_event_captured() {
                            break;
                        }
                    }

                    if ctx.shell.is_event_captured() {
                        return;
                    }

                    let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                    // Delete/Backspace: Delete selected nodes.
                    // Handled AFTER child widgets so text inputs can consume the event
                    // first. Gated on on_delete: without a handler the delete cannot be
                    // persisted, so don't consume the key (let it fall through).
                    if let Event::Keyboard(keyboard::Event::KeyPressed {
                        key,
                        physical_key,
                        modifiers,
                        ..
                    }) = event
                        && self.keymap.key_action(key, *physical_key, *modifiers)
                            == Some(KeyAction::DeleteSelection)
                        && !state.selected_nodes.is_empty()
                        && self.on_delete_handler().is_some()
                    {
                        let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
                        let node_ids = self.translate_node_ids(&indices);
                        if let Some(handler) = self.on_delete_handler() {
                            ctx.shell.publish(handler(node_ids));
                        }
                        state.selected_nodes.clear();
                        ctx.shell.capture_event();
                        ctx.shell.request_redraw();
                    }

                    // Frame-all / frame-selection: same after-children
                    // dispatch position as DeleteSelection (a focused text
                    // input consumes Home/f first). Gated on on_pan (like
                    // Clone on on_clone): without a handler the fit cannot
                    // be committed, so the key falls through unconsumed
                    // instead of being silently swallowed. Event capture
                    // only fires on an actual fit -- an unresolvable target
                    // (e.g. frame-selection with nothing selected) is a
                    // no-op that also lets the key fall through, mirroring
                    // Clone's empty-selection guard above.
                    if let Event::Keyboard(keyboard::Event::KeyPressed {
                        key,
                        physical_key,
                        modifiers,
                        ..
                    }) = event
                        && self.on_pan_handler().is_some()
                    {
                        let frame_target =
                            match self.keymap.key_action(key, *physical_key, *modifiers) {
                                Some(KeyAction::FrameAll) => Some(FocusTarget::All),
                                Some(KeyAction::FrameSelection) => Some(FocusTarget::Selection),
                                _ => None,
                            };
                        if let Some(target) = frame_target
                            && let Some(world_aabb) =
                                resolve_focus_target(self, layout, state, &target)
                        {
                            self.begin_focus(
                                state,
                                world_aabb,
                                layout.bounds().size(),
                                &FocusOptions::default(),
                                ctx.shell,
                            );
                            ctx.shell.capture_event();
                        }
                    }

                    // Only process mouse events if cursor is within our bounds
                    if !screen_cursor.is_over(layout.bounds()) {
                        return;
                    }

                    // The `Dragging` state machine, part 2: `None -> *` entry
                    // transitions from button presses.
                    match event {
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                            self.handle_left_press(&mut ctx, &z_indices)
                        }
                        Event::Mouse(mouse::Event::ButtonPressed(button))
                            if *button == self.keymap.pan_button =>
                        {
                            self.handle_pan_press(&mut ctx)
                        }
                        _ => {}
                    }
                },
            );
    }

    /// Folds one touch event into the finger list and returns the pointer
    /// event to process in its place, if any.
    ///
    /// A lone finger emulates the left mouse button (press/move/lift become
    /// `ButtonPressed(Left)`/`CursorMoved`/`ButtonReleased` with an
    /// `Available` cursor at the contact point); a press on empty space pans
    /// instead of box-selecting (see `start_box_select_or_cut`). Two fingers
    /// pinch-zoom and pan the camera directly, committing through `on_pan`
    /// like wheel zoom, and return `None`.
    fn apply_touch(
        &self,
        state: &mut NodeGraphState,
        event: &touch::Event,
        shell: &mut Shell<'_, Message>,
    ) -> Option<(Event, mouse::Cursor)> {
        match *event {
            touch::Event::FingerPressed { id, position } => {
                if let Some(entry) = state.fingers.iter_mut().find(|(f, _)| *f == id) {
                    entry.1 = position;
                    return None;
                }
                state.fingers.push((id, position));
                match state.fingers.len() {
                    1 => {
                        state.touch_tap = Some((id, position, state.time));
                        Some((
                            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                            mouse::Cursor::Available(position),
                        ))
                    }
                    2 => {
                        // Entering the pinch: a second contact cancels any
                        // in-progress one-finger drag.
                        state.touch_tap = None;
                        if state.dragging != Dragging::None {
                            state.dragging = Dragging::None;
                            if let Some(handler) = self.on_drag_end_handler() {
                                shell.publish(handler());
                            }
                            shell.request_redraw();
                        }
                        None
                    }
                    _ => None,
                }
            }
            touch::Event::FingerMoved { id, position } => {
                let index = state.fingers.iter().position(|(f, _)| *f == id)?;
                if state.fingers.len() == 1 {
                    state.fingers[0].1 = position;
                    // A travelling finger is a drag, not a tap.
                    if let Some((_, start, _)) = state.touch_tap
                        && start.distance(position) > TOUCH_TAP_TRAVEL
                    {
                        state.touch_tap = None;
                    }
                    return Some((
                        Event::Mouse(mouse::Event::CursorMoved { position }),
                        mouse::Cursor::Available(position),
                    ));
                }
                if index < 2 {
                    // Pinch: zoom by the contact-distance ratio at the new
                    // midpoint, then pan by the midpoint travel.
                    let prev = (state.fingers[0].1, state.fingers[1].1);
                    state.fingers[index].1 = position;
                    let next = (state.fingers[0].1, state.fingers[1].1);

                    let prev_distance = prev.0.distance(prev.1);
                    let next_distance = next.0.distance(next.1);
                    let prev_mid =
                        Point::new((prev.0.x + prev.1.x) / 2.0, (prev.0.y + prev.1.y) / 2.0);
                    let next_mid =
                        Point::new((next.0.x + next.1.x) / 2.0, (next.0.y + next.1.y) / 2.0);

                    // User-driven pinch aborts a running focus tween
                    // (arbitration: user input beats a tween).
                    state.camera_tween = None;
                    if prev_distance > 1.0 && next_distance > 1.0 {
                        let zoom_delta =
                            (next_distance / prev_distance - 1.0) * state.camera.zoom();
                        let mid: ScreenPoint = next_mid.into_euclid();
                        state.camera = state.camera.zoom_at(mid, zoom_delta);
                    }
                    let zoom = state.camera.zoom();
                    let pan = WorldPoint::new(next_mid.x / zoom, next_mid.y / zoom)
                        - WorldPoint::new(prev_mid.x / zoom, prev_mid.y / zoom);
                    state.camera = state.camera.move_by(pan);

                    // Commit continuously, mirroring wheel zoom.
                    if let Some(handler) = self.on_pan_handler() {
                        let pos = state.camera.position();
                        shell.publish(handler(Point::new(pos.x, pos.y), state.camera.zoom()));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                } else {
                    state.fingers[index].1 = position;
                }
                None
            }
            touch::Event::FingerLifted { id, position }
            | touch::Event::FingerLost { id, position } => {
                state.fingers.retain(|(f, _)| *f != id);
                if !state.fingers.is_empty() {
                    return None;
                }
                let lost = matches!(event, touch::Event::FingerLost { .. });
                // Tap on empty space (quick, motionless, not cancelled): clear
                // the selection, matching a mouse click on empty space (which
                // on touch starts a pan instead of a clearing box-select).
                if let Some((tap_id, _, pressed_at)) = state.touch_tap.take()
                    && tap_id == id
                    && !lost
                    && state.time - pressed_at <= TOUCH_TAP_MAX_SECS
                    && matches!(state.dragging, Dragging::Graph(_))
                    && !state.selected_nodes.is_empty()
                {
                    state.selected_nodes.clear();
                    if let Some(handler) = self.on_select_handler() {
                        shell.publish(handler(vec![]));
                    }
                    shell.request_redraw();
                }
                // Release whichever button the active drag listens for: a
                // touch pan runs as `Dragging::Graph`, which commits on the
                // keymap's pan button.
                let button = if matches!(state.dragging, Dragging::Graph(_)) {
                    self.keymap.pan_button
                } else {
                    mouse::Button::Left
                };
                Some((
                    Event::Mouse(mouse::Event::ButtonReleased(button)),
                    mouse::Cursor::Available(position),
                ))
            }
        }
    }

    /// Handles an in-progress edge-cutting drag: extends the cut trail on cursor
    /// move and commits every pending cut on release.
    fn handle_edge_cutting(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        match event {
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
                        let cut_start = trail.first().copied().unwrap_or(cursor_position);
                        let cut_end = cursor_position;

                        // Clear and recalculate - only edges intersecting cutting line are highlighted
                        pending_cuts.clear();

                        // Check each edge for intersection with the cutting line
                        for (edge_idx, (_id, from_ref, to_ref, _style)) in
                            self.edges.iter().enumerate()
                        {
                            // Resolve user IDs to indices
                            let from_node_idx = match self.node_index(&from_ref.node_id) {
                                Some(idx) => idx,
                                None => continue,
                            };
                            let to_node_idx = match self.node_index(&to_ref.node_id) {
                                Some(idx) => idx,
                                None => continue,
                            };

                            // Get pin positions and sides for bezier calculation
                            let from_pin_data =
                                layout
                                    .children()
                                    .nth(from_node_idx)
                                    .and_then(|node_layout| {
                                        tree.children.get(from_node_idx).and_then(|node_tree| {
                                            let pins = find_pins::<P, UI>(node_tree, node_layout);
                                            pins.iter()
                                                .find(|(_, state, _)| {
                                                    state.pin_id == from_ref.pin_id
                                                })
                                                .map(|(_, state, (pos, _))| (*pos, state.side))
                                        })
                                    });
                            let to_pin_data =
                                layout.children().nth(to_node_idx).and_then(|node_layout| {
                                    tree.children.get(to_node_idx).and_then(|node_tree| {
                                        let pins = find_pins::<P, UI>(node_tree, node_layout);
                                        pins.iter()
                                            .find(|(_, state, _)| state.pin_id == to_ref.pin_id)
                                            .map(|(_, state, (pos, _))| (*pos, state.side))
                                    })
                                });

                            if let (Some((p0, from_side)), Some((p3, to_side))) =
                                (from_pin_data, to_pin_data)
                            {
                                // Calculate bezier control points
                                let dir_from = pin_side_direction(from_side.into());
                                let dir_to = pin_side_direction(to_side.into());
                                let l = adaptive_bezier_length([p0.x, p0.y], [p3.x, p3.y]);
                                let p1 = Point::new(p0.x + dir_from[0] * l, p0.y + dir_from[1] * l);
                                let p2 = Point::new(p3.x + dir_to[0] * l, p3.y + dir_to[1] * l);

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
                    for &edge_idx in pending_cuts.iter() {
                        if let Some((_id, from_ref, to_ref, _)) = self.edges.get(edge_idx) {
                            // Edges already store user IDs (PinRef<N, P>)
                            if let Some(handler) = self.on_disconnect_handler() {
                                shell.publish(handler(from_ref.clone(), to_ref.clone()));
                            }
                        }
                    }
                }
                state.dragging = Dragging::None;
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Handles an in-progress graph pan: commits the camera offset on
    /// right-button release.
    fn handle_graph_pan(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, origin: WorldPoint) {
        let UpdateCtx {
            tree,
            event,
            screen_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        if let Event::Mouse(mouse::Event::ButtonReleased(button)) = event
            && *button == self.keymap.pan_button
        {
            if let Some(cursor_position) = screen_cursor.position() {
                let screen_to_world = state.camera.screen_to_world();
                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                let cursor_position: WorldPoint = screen_to_world.transform_point(cursor_position);
                let offset = cursor_position - origin;
                state.camera = state.camera.move_by(offset);

                // Commit the new camera position on pan release.
                if let Some(handler) = self.on_pan_handler() {
                    let pos = state.camera.position();
                    shell.publish(handler(Point::new(pos.x, pos.y), state.camera.zoom()));
                }
            }
            state.dragging = Dragging::None;
            shell.capture_event();
            shell.request_redraw();
        }
    }

    /// Handles an in-progress single-node drag: reports the final offset on
    /// release (a motionless press+release is a click, not a move).
    fn handle_node_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        origin: WorldPoint,
    ) {
        let UpdateCtx {
            tree,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
            if let Some(cursor_position) = world_cursor.position() {
                let cursor_position = cursor_position.into_euclid();
                let offset = cursor_position - origin;

                // A press+release without motion is a click, not
                // a move: don't emit a spurious move (which would
                // dirty host state / undo history on a plain
                // selection click). Only report an actual drag.
                let moved = offset.x.abs() > f32::EPSILON || offset.y.abs() > f32::EPSILON;

                // Translate internal index to user ID
                if let Some(node_id) = self.index_to_node_id(node_index)
                    && moved
                {
                    // Call on_move handler if set
                    if let Some(handler) = self.on_move_handler() {
                        shell.publish(handler(offset.into_iced(), vec![node_id]));
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

    /// Handles an in-progress edge drag: snap-tests against the valid drop
    /// targets and fires `on_connect` immediately on snap (plug behavior).
    fn handle_edge_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        from_node: usize,
        from_pin: usize,
    ) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Check if cursor is over a valid target pin to transition to EdgeOver
                if let Some(cursor_position) = world_cursor.position() {
                    // Copy valid_drop_targets before iterating over tree.children
                    let valid_targets = state.valid_drop_targets.clone();
                    // Screen-space threshold: constant hit target at any zoom.
                    let snap_threshold = SNAP_THRESHOLD / state.camera.zoom();

                    // Extract from_pin_id while iterating (need access to tree.children)
                    let mut from_pin_id: Option<P> = None;
                    let mut from_dir: Option<PinDirection> = None;
                    let mut target_info: Option<(usize, usize, P, PinDirection)> = None;

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
                            let distance =
                                a.distance(cursor_position).min(b.distance(cursor_position));

                            // Use SNAP_THRESHOLD for entering snap zone
                            if distance < snap_threshold && target_info.is_none() {
                                // Check if this pin is in valid_drop_targets
                                if valid_targets.contains(&(node_index, pin_index)) {
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

                    if let Some((to_node, to_pin, to_pin_id, to_dir)) = target_info {
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

                        state.dragging = Dragging::EdgeOver(from_node, from_pin, to_node, to_pin);
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
        }
    }

    /// Handles the snapped state of an edge drag: unsnap hysteresis
    /// (`UNSNAP_THRESHOLD`) fires `on_disconnect` and falls back to `Edge`.
    fn handle_edge_over(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    ) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Check if still over the target pin, otherwise go back to Edge state
                // Use UNSNAP_THRESHOLD (larger than SNAP_THRESHOLD) to prevent jitter
                if let Some(cursor_position) = world_cursor.position() {
                    let unsnap_threshold = UNSNAP_THRESHOLD / state.camera.zoom();
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
                                let distance =
                                    a.distance(cursor_position).min(b.distance(cursor_position));
                                still_over_pin = distance < unsnap_threshold;
                            }
                        }
                    }

                    if !still_over_pin {
                        // Fire EdgeDisconnected event when leaving snap (plug behavior)
                        let from_node_id = self.index_to_node_id(from_node);
                        let to_node_id = self.index_to_node_id(to_node);

                        if let (Some(from_nid), Some(to_nid), Some(from_pid), Some(to_pid)) =
                            (from_node_id, to_node_id, from_pin_id, to_pin_id)
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
                        state.dragging =
                            Dragging::Edge(from_node, from_pin, cursor_position.into_euclid());
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
        }
    }

    /// Handles an in-progress box selection: tracks the moving corner and
    /// commits the intersecting set on release (Shift adds to the selection).
    fn handle_box_select(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, start: WorldPoint) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        match event {
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

                    // Without the multi-select modifier (keymap, default
                    // Shift): replace selection. With it: add to selection.
                    if !state.modifiers.contains(self.keymap.multi_select_modifiers) {
                        state.selected_nodes.clear();
                    }

                    // Find all nodes that intersect the selection rectangle
                    for (node_index, node_layout) in layout.children().enumerate() {
                        if rects_intersect(&selection_rect, &node_layout.bounds()) {
                            state.selected_nodes.insert(node_index);
                        }
                    }

                    // Notify selection change
                    let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
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
        }
    }

    /// Handles an in-progress group move: reports one shared delta for every
    /// selected node on release.
    fn handle_group_move(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, origin: WorldPoint) {
        let UpdateCtx {
            tree,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Complete group move - notify all selected nodes moved
                let indices: Vec<usize> = state.selected_nodes.iter().copied().collect();
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
        }
    }

    /// Dispatches a left-button press: edge cut, then per-node pin/body
    /// hit-test (top-first by z-order), then the empty-space fallback.
    ///
    /// This holds every `Dragging::None -> *` transition of the left button;
    /// in-progress transitions live in the `handle_*` methods above.
    fn handle_left_press(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, z_indices: &[usize]) {
        // Multi-select-modifier+drag from an occupied pin forks a NEW edge
        // instead of unplugging the existing one.
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        // A press while another drag is in progress (e.g. left press during a
        // pan) must not hijack the state machine mid-drag.
        if state.dragging != Dragging::None {
            return;
        }
        let multi_select_held = state.modifiers.contains(self.keymap.multi_select_modifiers);
        let edge_cut_held = state.modifiers.contains(self.keymap.edge_cut_modifiers);

        // Edge-cut chord (keymap, default Cmd/Ctrl+Click): edge cut tool.
        if edge_cut_held && self.try_cut_edge_at_cursor(ctx) {
            return;
        }

        if let Some(cursor_position) = ctx.world_cursor.position() {
            // Per-node hit-test, top-first by z-order: check this node's pins
            // first, then its body. The first node to own the cursor - pin OR
            // body - wins. This way a body on top blocks click-through to a
            // pin hidden beneath (no accidental edge-drag from a covered pin),
            // while the snap logic during an active edge drag still sees all
            // pins regardless of cover.
            for &node_index in z_indices.iter().rev() {
                if self.try_press_node(ctx, node_index, cursor_position, multi_select_held) {
                    return;
                }
            }
        }

        // Nothing hit - start box selection on empty space, unless COMMAND is
        // held (reserved for edge cutting).
        self.start_box_select_or_cut(ctx);
    }

    /// Cuts the first edge within `EDGE_CUT_THRESHOLD` of the cursor
    /// (Command+Click edge cut). Returns whether a cut consumed the press.
    fn try_cut_edge_at_cursor(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) -> bool {
        let UpdateCtx {
            tree,
            layout,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        let Some(cursor_position) = world_cursor.position() else {
            return false;
        };
        // Screen-space threshold: constant hit target at any zoom.
        let cut_threshold =
            EDGE_CUT_THRESHOLD / tree.state.downcast_ref::<NodeGraphState>().camera.zoom();
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

            // Get pin positions and sides for both ends of the edge
            let from_pin_data = layout
                .children()
                .nth(from_node_idx)
                .and_then(|node_layout| {
                    tree.children.get(from_node_idx).and_then(|node_tree| {
                        let pins = find_pins::<P, UI>(node_tree, node_layout);
                        pins.iter()
                            .find(|(_, state, _)| state.pin_id == from_ref.pin_id)
                            .map(|(_, state, (a, _))| (*a, state.side))
                    })
                });
            let to_pin_data = layout.children().nth(to_node_idx).and_then(|node_layout| {
                tree.children.get(to_node_idx).and_then(|node_tree| {
                    let pins = find_pins::<P, UI>(node_tree, node_layout);
                    pins.iter()
                        .find(|(_, state, _)| state.pin_id == to_ref.pin_id)
                        .map(|(_, state, (a, _))| (*a, state.side))
                })
            });

            if let (Some((from_pos, from_side)), Some((to_pos, to_side))) =
                (from_pin_data, to_pin_data)
            {
                // Measure against the rendered bezier, not the straight
                // chord: same control-point construction as the draw path.
                let dir_from = pin_side_direction(from_side.into());
                let dir_to = pin_side_direction(to_side.into());
                let l = adaptive_bezier_length([from_pos.x, from_pos.y], [to_pos.x, to_pos.y]);
                let p1 = Point::new(from_pos.x + dir_from[0] * l, from_pos.y + dir_from[1] * l);
                let p2 = Point::new(to_pos.x + dir_to[0] * l, to_pos.y + dir_to[1] * l);
                let distance = point_to_bezier_distance(cursor_position, from_pos, p1, p2, to_pos);
                if distance < cut_threshold {
                    // Edges already store user IDs
                    if let Some(handler) = self.on_disconnect_handler() {
                        shell.publish(handler(from_ref.clone(), to_ref.clone()));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                    return true;
                }
            }
        }
        false
    }

    /// Hit-tests one node's pins and body for a left press.
    ///
    /// Pin hits within `PIN_CLICK_THRESHOLD` either unplug an existing
    /// connection (magnetic plug) or start a fresh edge drag; a body hit
    /// selects and starts a node/group drag. Returns whether this node
    /// consumed the press.
    fn try_press_node(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        cursor_position: Point,
        multi_select_held: bool,
    ) -> bool {
        let Some(node_layout) = ctx.layout.children().nth(node_index) else {
            return false;
        };
        let Some(node_tree) = ctx.tree.children.get(node_index) else {
            return false;
        };
        // Owned snapshot: the helpers below re-borrow the tree mutably
        // (state downcast, compute_valid_targets), so borrowed pin states
        // cannot stay alive across those calls.
        let pins: Vec<(usize, P, bool, (Point, Point))> =
            find_pins::<P, UI>(node_tree, node_layout)
                .into_iter()
                .map(|(i, s, pos)| (i, s.pin_id.clone(), s.interactions_disabled, pos))
                .collect();
        let Some(current_node_id) = self.index_to_node_id(node_index) else {
            return false;
        };

        // Screen-space threshold: constant hit target at any zoom.
        let click_threshold = PIN_CLICK_THRESHOLD
            / ctx
                .tree
                .state
                .downcast_ref::<NodeGraphState>()
                .camera
                .zoom();

        for (pin_index, pin_id, disabled, (a, b)) in pins {
            // Pin positions from layout are ALREADY in world space because
            // layout was created with .move_to(world_position).
            let distance = a.distance(cursor_position).min(b.distance(cursor_position));
            if distance < click_threshold && !disabled {
                // Check if this pin has existing connections. Without the
                // multi-select modifier, "unplug" the clicked end (like
                // pulling a cable). With it held, skip the unplug entirely and
                // fall through to start a fresh edge, leaving existing
                // connections intact.
                if !multi_select_held {
                    for (_id, from_ref, to_ref, _style) in &self.edges {
                        // Unplug the clicked end, staying anchored at the
                        // other one: grabbing "from" anchors at TO and vice
                        // versa.
                        let anchor =
                            if from_ref.node_id == current_node_id && from_ref.pin_id == pin_id {
                                to_ref
                            } else if to_ref.node_id == current_node_id && to_ref.pin_id == pin_id {
                                from_ref
                            } else {
                                continue;
                            };
                        if self.try_start_unplug(
                            ctx,
                            anchor,
                            (from_ref, to_ref),
                            (node_index, pin_index),
                        ) {
                            return true;
                        }
                    }
                }

                // No existing connection (or shift held to fork a new edge):
                // start a fresh drag - but only if on_connect is wired.
                // Without it a dropped edge cannot persist, so let the press
                // fall through to node selection instead.
                if self.try_start_edge_drag(
                    ctx,
                    node_index,
                    pin_index,
                    &pin_id,
                    &current_node_id,
                    cursor_position,
                ) {
                    return true;
                }
            }
        }

        // Body check for this same node (still top-first).
        if ctx.world_cursor.is_over(node_layout.bounds()) {
            self.select_or_drag_node(ctx, node_index, cursor_position);
            return true;
        }
        false
    }

    /// Starts the "unplug" drag for one end of an existing edge.
    ///
    /// Magnetic plug: grabbing a connected pin does NOT disconnect yet. The
    /// drag enters the snapped `EdgeOver` state anchored at the OTHER
    /// (`anchor`) end; the hysteresis in `handle_edge_over` fires
    /// `on_disconnect` only once the cursor leaves the grabbed pin by more
    /// than `UNSNAP_THRESHOLD`. Returns `false` when the anchor end cannot
    /// be resolved (caller then tries the next edge).
    fn try_start_unplug(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor: &PinRef<N, P>,
        edge: (&PinRef<N, P>, &PinRef<N, P>),
        grabbed: (usize, usize),
    ) -> bool {
        let Some(anchor_node_idx) = self.node_index(&anchor.node_id) else {
            return false;
        };
        let Some(anchor_pin_idx) =
            resolve_pin_index::<P, UI>(ctx.tree, ctx.layout, anchor_node_idx, &anchor.pin_id)
        else {
            return false;
        };
        // Compute valid targets for the new drag, excluding the grabbed edge
        // so it can be dropped back onto its own input.
        let valid_targets = compute_valid_targets(
            self,
            ctx.tree,
            ctx.layout,
            anchor_node_idx,
            anchor_pin_idx,
            Some(edge),
        );
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
        // Anchor at the kept end, hold the grabbed pin snapped (still
        // connected).
        state.dragging = Dragging::EdgeOver(anchor_node_idx, anchor_pin_idx, grabbed.0, grabbed.1);
        ctx.shell.capture_event();
        true
    }

    /// Starts a fresh edge drag from a pin, gated on `on_connect` being
    /// wired (without it a dropped edge cannot persist).
    fn try_start_edge_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        pin_index: usize,
        pin_id: &P,
        node_id: &N,
        cursor_position: Point,
    ) -> bool {
        if self.on_connect_handler().is_none() {
            return false;
        }
        // Compute valid targets ONCE at drag-start.
        let valid_targets =
            compute_valid_targets(self, ctx.tree, ctx.layout, node_index, pin_index, None);
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
        state.dragging = Dragging::Edge(node_index, pin_index, cursor_position.into_euclid());
        if let Some(handler) = self.on_drag_start_handler() {
            ctx.shell.publish(handler(DragInfo::Edge {
                from_node: node_id.clone(),
                from_pin: pin_id.clone(),
            }));
        }
        ctx.shell.capture_event();
        true
    }

    /// Applies click-selection semantics for a node body press and starts the
    /// matching drag (`Node` or `GroupMove`, gated on `on_move` being wired).
    fn select_or_drag_node(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        cursor_position: Point,
    ) {
        let UpdateCtx { tree, shell, .. } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        let already_selected = state.selected_nodes.contains(&node_index);
        let modifiers = state.modifiers;
        let selection_changed;

        // Handle selection based on the multi-select modifier (keymap,
        // default Shift).
        if modifiers.contains(self.keymap.multi_select_modifiers) {
            // Multi-select click: toggle selection membership
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

        // Decide between single node drag or group move -
        // only when on_move is wired. Node positions come
        // from the host, so without on_move a drag would move
        // the node visually then snap back on the next frame;
        // gate it off (selection below still fires).
        if self.on_move_handler().is_some() {
            if state.selected_nodes.len() > 1 && state.selected_nodes.contains(&node_index) {
                // Multiple nodes selected, start group move
                let selected: Vec<usize> = state.selected_nodes.iter().copied().collect();
                state.dragging = Dragging::GroupMove(cursor_position.into_euclid());
                // Emit drag start event for group
                if let Some(handler) = self.on_drag_start_handler() {
                    shell.publish(handler(DragInfo::Group {
                        node_ids: self.translate_node_ids(&selected),
                    }));
                }
            } else {
                // Single node drag
                state.dragging = Dragging::Node(node_index, cursor_position.into_euclid());
                // Emit drag start event for single node
                if let Some(handler) = self.on_drag_start_handler()
                    && let Some(node_id) = self.index_to_node_id(node_index)
                {
                    shell.publish(handler(DragInfo::Node { node_id }));
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
    }

    /// Starts the empty-space press interaction: edge-cutting with COMMAND
    /// held, box selection otherwise (Shift keeps the current selection).
    fn start_box_select_or_cut(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        let UpdateCtx {
            tree,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        if let Some(cursor_position) = world_cursor.position() {
            let cursor_position: WorldPoint = cursor_position.into_euclid();
            let state = tree.state.downcast_mut::<NodeGraphState>();

            // Edge-cut chord held: start edge cutting mode instead of box selection
            if state.modifiers.contains(self.keymap.edge_cut_modifiers) {
                state.dragging = Dragging::EdgeCutting {
                    trail: vec![cursor_position],
                    pending_cuts: std::collections::HashSet::new(),
                };
                shell.capture_event();
                return;
            }

            // Touch: a press on empty space pans the graph. Box selection
            // needs a keyboard for its additive mode and pan is the dominant
            // touch expectation; a tap (no travel) clears the selection on
            // lift instead (see `apply_touch`).
            if !state.fingers.is_empty() {
                // User-driven pan aborts a running focus tween (arbitration:
                // user input beats a tween).
                state.camera_tween = None;
                state.dragging = Dragging::Graph(cursor_position);
                shell.capture_event();
                return;
            }

            // Clear selection unless the multi-select modifier is held
            if !state.modifiers.contains(self.keymap.multi_select_modifiers) {
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

    /// Starts a graph pan from a press of the keymap's pan button.
    fn handle_pan_press(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        let UpdateCtx {
            tree,
            screen_cursor,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        // Never cancel an in-progress node/edge/box drag: that would drop the
        // drag without emitting on_drag_end or committing the move.
        if state.dragging != Dragging::None {
            return;
        }
        // Right-click: start graph panning
        if let Some(cursor_position) = screen_cursor.position() {
            let cursor_position: ScreenPoint = cursor_position.into_euclid();
            let cursor_position: WorldPoint = state
                .camera
                .screen_to_world()
                .transform_point(cursor_position);
            let state = tree.state.downcast_mut::<NodeGraphState>();
            // User-driven pan aborts a running focus tween (arbitration:
            // user input beats a tween).
            state.camera_tween = None;
            state.dragging = Dragging::Graph(cursor_position.into_euclid());
            shell.capture_event();
        }
    }

    /// Starts a fit toward `world_aabb`: a tween when `opts.animation` is
    /// set with a positive duration, otherwise an immediate jump. Replaces
    /// any running tween (new focus/frame always wins, arbitration rule 1).
    /// The jump commits through `on_pan` immediately, like any other camera
    /// change; the tween commits once per `RedrawRequested` frame (see the
    /// tween-advance block in `update_impl`).
    fn begin_focus(
        &self,
        state: &mut NodeGraphState,
        world_aabb: WorldRect,
        viewport: Size,
        opts: &FocusOptions,
        shell: &mut Shell<'_, Message>,
    ) {
        let (end_position, end_zoom) = Camera2D::fit(world_aabb, viewport, opts);
        let viewport_origin = state.camera.viewport_origin();

        let jump = match opts.animation {
            None => true,
            Some(anim) => anim.duration.as_secs_f32() <= 0.0,
        };

        if jump {
            state.camera_tween = None;
            state.camera = Camera2D::with_zoom_and_position(end_zoom, end_position)
                .with_viewport_origin(viewport_origin);
            if let Some(handler) = self.on_pan_handler() {
                shell.publish(handler(
                    Point::new(end_position.x, end_position.y),
                    end_zoom,
                ));
            }
        } else if let Some(anim) = opts.animation {
            let start_center = Camera2D::center_for_position(
                state.camera.position(),
                state.camera.zoom(),
                viewport,
                opts.padding,
            );
            state.camera_tween = Some(CameraTween {
                start_center,
                start_zoom: state.camera.zoom(),
                end_center: world_aabb.center(),
                end_zoom,
                viewport,
                padding: opts.padding,
                elapsed: 0.0,
                duration: anim.duration.as_secs_f32(),
                easing: anim.easing,
            });
        }
        shell.request_redraw();
    }
}

/// Resolves a [`FocusTarget`] to a world-space AABB using live layout, or
/// `None` for an unknown/empty target -- a no-op per the design (no camera
/// change, no `on_pan`): an unresolvable id is skipped, `All`/`Selection`
/// with nothing to union is empty, `Nodes`/`Edges` union whatever resolves.
fn resolve_focus_target<N, P, E, UI, Message, Renderer>(
    graph: &NodeGraph<'_, N, P, UI, Message, iced::Theme, Renderer, E>,
    layout: Layout<'_>,
    state: &NodeGraphState,
    target: &FocusTarget<N, E>,
) -> Option<WorldRect>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    // Node layout bounds are layout-absolute (`viewport_origin + world`,
    // unzoomed -- layout runs before the camera transform); subtract
    // `viewport_origin` to get true world coordinates.
    let viewport_origin = state.camera.viewport_origin();
    let node_rect = |index: usize| -> Option<WorldRect> {
        let b = layout.children().nth(index)?.bounds();
        Some(WorldRect::new(
            WorldPoint::new(b.x - viewport_origin.x, b.y - viewport_origin.y),
            WorldSize::new(b.width, b.height),
        ))
    };
    let union_of = |rects: &mut dyn Iterator<Item = WorldRect>| rects.reduce(|a, b| a.union(&b));
    // An edge's frame target is the union of its two endpoint nodes' bounds
    // (seeing a connection means seeing both ends it connects); either
    // endpoint failing to resolve skips the whole edge.
    let edge_rect = |id: &E| -> Option<WorldRect> {
        let (_, from, to, _) = graph.edges.iter().find(|(eid, ..)| eid == id)?;
        let a = node_rect(graph.node_index(&from.node_id)?)?;
        let b = node_rect(graph.node_index(&to.node_id)?)?;
        Some(a.union(&b))
    };

    match target {
        FocusTarget::All => union_of(&mut (0..graph.nodes.len()).filter_map(node_rect)),
        FocusTarget::Selection => {
            union_of(&mut state.selected_nodes.iter().copied().filter_map(node_rect))
        }
        FocusTarget::Node(id) => graph.node_index(id).and_then(node_rect),
        FocusTarget::Nodes(ids) => union_of(
            &mut ids
                .iter()
                .filter_map(|id| graph.node_index(id))
                .filter_map(node_rect),
        ),
        FocusTarget::Edge(id) => edge_rect(id),
        FocusTarget::Edges(ids) => union_of(&mut ids.iter().filter_map(edge_rect)),
        FocusTarget::Rect(rect) => Some((*rect).into_euclid()),
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

/// Resolves a pin's positional index within `node_idx` from its user pin id.
///
/// The index is the pin's position in `find_pins` walk order, which is also
/// the `pin_index` the drag states store.
fn resolve_pin_index<P: PinId + 'static, UI: 'static>(
    tree: &Tree,
    layout: Layout<'_>,
    node_idx: usize,
    pin_id: &P,
) -> Option<usize> {
    let node_tree = tree.children.get(node_idx)?;
    let node_layout = layout.children().nth(node_idx)?;
    find_pins::<P, UI>(node_tree, node_layout)
        .iter()
        .position(|(_, s, _)| s.pin_id == *pin_id)
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

/// Minimum distance from a point to a cubic bezier, via uniform flattening.
///
/// 32 segments keep the flattening error far below the 10px cut threshold
/// for edge-scale curves; no allocation.
fn point_to_bezier_distance(point: Point, p0: Point, p1: Point, p2: Point, p3: Point) -> f32 {
    const SEGMENTS: u32 = 32;
    let mut prev = p0;
    let mut min_dist = f32::MAX;
    for i in 1..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let it = 1.0 - t;
        let a = it * it * it;
        let b = 3.0 * it * it * t;
        let c = 3.0 * it * t * t;
        let d = t * t * t;
        let cur = Point::new(
            a * p0.x + b * p1.x + c * p2.x + d * p3.x,
            a * p0.y + b * p1.y + c * p2.y + d * p3.y,
        );
        min_dist = min_dist.min(point_to_line_distance(point, prev, cur));
        prev = cur;
    }
    min_dist
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
