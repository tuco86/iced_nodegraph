//! The `update` event path of [`NodeGraph`]: the [`Dragging`] state machine and
//! the hit tests that drive its transitions.
//!
//! Every interaction the widget supports is a transition of that one enum, so a
//! new gesture is a new variant plus its entry and exit edges - never a flag
//! alongside it. Thresholds are declared in screen pixels and divided by zoom at
//! each comparison, so the on-screen hit target is constant at any zoom.

use super::*;
use crate::node_graph::camera::Camera2D;
use crate::node_graph::euclid::{WorldRect, WorldSize};
use crate::node_graph::input::KeyAction;
use crate::node_graph::{
    ANCHOR_GRAB_THRESHOLD, DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING, EDGE_CUT_THRESHOLD,
    FocusOptions, FocusTarget, PIN_CLICK_THRESHOLD,
};
use iced_widget::core::{touch, window};
use std::collections::HashSet;

/// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary).
/// Screen px, scaled by 1/zoom at the comparison sites like
/// [`PIN_CLICK_THRESHOLD`].
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

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

impl<N, P, UI, Message, Renderer, E> NodeGraph<'_, N, P, UI, Message, Renderer, E>
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
        if let Some(view) = self.view
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
        state.camera = state.camera_for(layout);

        // Drop the pending selection once the host has moved on - it either applied
        // what we reported or set its own value; either way its word is final.
        // Comparing against the host (not against the pending value) is what stops
        // a not-yet-refreshed host frame from undoing an interaction that has not
        // round-tripped yet. This belongs here, not in `push_node`: that never
        // knows whether another node follows, so it never sees a complete
        // selection to compare.
        let host_selection = self.host_selection();
        if state.selection_baseline.as_ref() != Some(&host_selection) {
            state.pending_selection = None;
            state.selection_baseline = Some(host_selection);
        }

        // Assign z-order entries to any newly-seen node indices so freshly
        // pushed nodes spawn on top of older ones.
        state.ensure_z_entries(self.nodes.len());
        // One selection read per event, shared by every gate and payload below.
        let selection = self.resolved_selection(state);
        let z_indices = z_render_indices(state, self.nodes.len(), |i| selection.contains(&i));

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

        if let Some(last_update) = state.last_update {
            let delta = now.duration_since(last_update).as_secs_f32();
            // Cap at 100ms to prevent freeze after background
            state.time += delta.min(0.1);
        }
        state.last_update = Some(now);

        // On each frame, drive continuous redraws for SDF animations and deliver
        // the diagnostics measured during the previous draw().
        if let Event::Window(window::Event::RedrawRequested(redraw_at)) = event {
            if state.sdf_animated.get() {
                shell.request_redraw();
            }
            // Publish the stashed GraphInfo (set during draw) one frame behind,
            // mirroring the controlled on_pan pattern. A host showing live
            // diagnostics needs a steady frame stream, so keep redraws flowing.
            if let Some(handler) = self.on_info.as_ref() {
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
            //
            // Driven by the REDRAW EVENT'S OWN timestamp (`redraw_at`), not
            // `frame_delta`: iced_winit dispatches non-redraw events (e.g.
            // CursorMoved) in a separate update pass that runs immediately
            // before the redraw pass each frame, so `frame_delta` is
            // whichever event reached `update()` first -- often near-zero
            // for the redraw pass once anything else is in flight that
            // frame, which would stall the tween. `last_redraw` also flags
            // a RE-ENTRANT pass of the SAME `RedrawRequested` cycle:
            // iced_winit re-runs the redraw update (reusing one `Instant`
            // for every pass) up to three times while a pass keeps
            // invalidating layout, and advancing/publishing again there
            // would both warp the tween's clock (near-zero elapsed) and
            // publish a second `on_pan` whose value disagrees with
            // `last_synced_view` in the low f32 bits -- which the
            // view()-sync block above would then mistake for an app
            // override and snap the camera back, aborting the tween.
            let is_reentrant_redraw = state.last_redraw == Some(*redraw_at);
            let redraw_delta = state
                .last_redraw
                .map(|last| redraw_at.duration_since(last).as_secs_f32().min(0.1))
                .unwrap_or(0.0);
            state.last_redraw = Some(*redraw_at);

            if is_reentrant_redraw {
                // A re-entrant pass must not advance or publish, but it MUST
                // still re-assert the redraw request. `UserInterface::update`
                // rebuilds `redraw_request` from `Wait` on every pass
                // (iced_runtime/src/user_interface.rs:193) and the winit
                // redraw loop keeps only the LAST pass's state, so a request
                // made by the first pass is discarded the moment a second
                // pass stays silent -- the tween would then advance exactly
                // one frame per triggering event and stall, creeping toward
                // the target one keypress at a time instead of animating.
                if state.camera_tween.is_some() {
                    shell.request_redraw();
                }
            } else if let Some(tween) = state.camera_tween.as_mut() {
                tween.elapsed += redraw_delta;
                let t = if tween.duration > 0.0 {
                    (tween.elapsed / tween.duration).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let e = tween.easing.apply(t);
                // One perceptual path for both halves: geometric zoom with a
                // 1/zoom-weighted center, so the image moves as one body
                // instead of pan and zoom each running their own ramp.
                let (center, zoom) = Camera2D::tween_step(
                    tween.start_center,
                    tween.start_zoom,
                    tween.end_center,
                    tween.end_zoom,
                    e,
                );
                let position =
                    Camera2D::position_for_center(center, zoom, tween.viewport, tween.padding);
                let viewport_origin = state.camera.viewport_origin();
                state.camera = Camera2D::with_zoom_and_position(zoom, position)
                    .with_viewport_origin(viewport_origin);

                let view = (Point::new(position.x, position.y), zoom);
                if let Some(handler) = self.on_pan.as_ref() {
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
                    if !selection.is_empty() && self.on_clone.as_ref().is_some() =>
                {
                    let node_ids = self.selection_ids(&selection);
                    if let Some(handler) = self.on_clone.as_ref() {
                        shell.publish(handler(node_ids));
                    }
                    shell.capture_event();
                }
                Some(KeyAction::SelectAll) => {
                    state.pending_selection = Some((0..self.nodes.len()).collect());
                    let selected: Vec<N> = self.nodes.iter().map(|node| node.id.clone()).collect();
                    if let Some(handler) = self.on_select.as_ref() {
                        shell.publish(handler(selected));
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                Some(KeyAction::ClearSelection) if !selection.is_empty() => {
                    state.pending_selection = Some(HashSet::new());
                    if let Some(handler) = self.on_select.as_ref() {
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
            if let Some(handler) = self.on_pan.as_ref() {
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
                            && let Some(handler) = self.on_drag_update.as_ref()
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
                        Dragging::Node {
                            node: node_index,
                            origin,
                        } => self.handle_node_drag(&mut ctx, node_index, origin),
                        Dragging::Resize {
                            node: node_index,
                            origin,
                            start,
                        } => self.handle_resize(&mut ctx, node_index, origin, start),
                        Dragging::Edge {
                            from_node,
                            from_pin,
                            origin: _,
                        } => self.handle_edge_drag(&mut ctx, from_node, from_pin),
                        Dragging::EdgeOver {
                            from_node,
                            from_pin,
                            to_node,
                            to_pin,
                        } => self.handle_edge_over(&mut ctx, from_node, from_pin, to_node, to_pin),
                        Dragging::SelectionBox(start, _current) => {
                            self.handle_selection_box(&mut ctx, start)
                        }
                        Dragging::GroupMove(origin) => self.handle_group_move(&mut ctx, origin),
                        Dragging::EdgeOverOrbit {
                            from_node,
                            from_pin,
                            anchor,
                            orbit,
                            hand,
                        } => self.handle_edge_over_orbit(
                            &mut ctx, from_node, from_pin, anchor, orbit, hand,
                        ),
                        Dragging::EdgeFromOrbit {
                            anchor,
                            orbit,
                            hand,
                            origin: _,
                        } => self.handle_edge_from_orbit(&mut ctx, anchor, orbit, hand),
                        Dragging::OrbitEdgeOver {
                            anchor,
                            orbit,
                            hand,
                            to_node,
                            to_pin,
                        } => self
                            .handle_orbit_edge_over(&mut ctx, anchor, orbit, hand, to_node, to_pin),
                        Dragging::Anchor { anchor, origin } => {
                            self.handle_anchor_drag(&mut ctx, anchor, origin)
                        }
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
                        let Some(node) = self.nodes.get_mut(node_index) else {
                            continue;
                        };
                        let element = &mut node.element;
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
                        && !selection.is_empty()
                        && self.on_delete.as_ref().is_some()
                    {
                        if let Some(handler) = self.on_delete.as_ref() {
                            ctx.shell.publish(handler(self.selection_ids(&selection)));
                        }
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
                        && self.on_pan.as_ref().is_some()
                    {
                        let frame_target =
                            match self.keymap.key_action(key, *physical_key, *modifiers) {
                                Some(KeyAction::FrameAll) => Some(FocusTarget::All),
                                Some(KeyAction::FrameSelection) => Some(FocusTarget::Selection),
                                _ => None,
                            };
                        // `state` is re-derived here rather than held from the
                        // top of the closure: the drag dispatch above needs
                        // `&mut ctx`, and one long-lived borrow of `ctx.tree`
                        // across it would conflict with every handler call.
                        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
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
    /// instead of opening a selection box (see `start_selection_box_or_cut`). Two fingers
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
                            if let Some(handler) = self.on_drag_end.as_ref() {
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
                    if let Some(handler) = self.on_pan.as_ref() {
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
                // on touch starts a pan instead of a clearing selection box).
                if let Some((tap_id, _, pressed_at)) = state.touch_tap.take()
                    && tap_id == id
                    && !lost
                    && state.time - pressed_at <= TOUCH_TAP_MAX_SECS
                    && matches!(state.dragging, Dragging::Graph(_))
                    && !self.resolved_selection(state).is_empty()
                {
                    state.pending_selection = Some(HashSet::new());
                    if let Some(handler) = self.on_select.as_ref() {
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
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = world_cursor.position() else {
                    return;
                };
                let cursor_position: LayoutPoint = cursor_position.into_euclid();
                // Cable geometry reads the tree immutably, so it is taken before
                // the mutable state borrow the trail needs.
                let paths = {
                    let state = tree.state.downcast_ref::<NodeGraphState>();
                    cable_paths(self, tree, *layout, state)
                };
                let state = tree.state.downcast_mut::<NodeGraphState>();
                if let Dragging::EdgeCutting {
                    ref mut trail,
                    ref mut pending_cuts,
                } = state.dragging
                {
                    trail.push(cursor_position);
                    let cut_start = trail.first().copied().unwrap_or(cursor_position);

                    // Recomputed from scratch each move: the trail is a single
                    // chord from its start, so a cut can be taken back by
                    // dragging away again.
                    pending_cuts.clear();
                    for (chain, path) in &paths {
                        if path.intersects(
                            [cut_start.x, cut_start.y],
                            [cursor_position.x, cursor_position.y],
                        ) {
                            // A cable is cut whole, so its whole chain is
                            // marked - that is also what paints them all in the
                            // pending-cut color.
                            pending_cuts.extend(chain.iter().copied());
                        }
                    }
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<NodeGraphState>();
                let cuts: Vec<usize> = match &state.dragging {
                    Dragging::EdgeCutting { pending_cuts, .. } => {
                        pending_cuts.iter().copied().collect()
                    }
                    _ => Vec::new(),
                };
                state.dragging = Dragging::None;
                self.report_cut(shell, &cuts);
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
                if let Some(handler) = self.on_pan.as_ref() {
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
        origin: LayoutPoint,
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
                if let Some(node_id) = self.node_id_at(node_index).cloned()
                    && moved
                {
                    // Call on_move handler if set
                    if let Some(handler) = self.on_move.as_ref() {
                        shell.publish(handler(offset.into_iced(), vec![node_id]));
                    }
                }
            }
            // Promote this node to the top of the z-order on drop.
            state.promote_z(node_index);
            state.dragging = Dragging::None;
            // Emit drag end event
            if let Some(handler) = self.on_drag_end.as_ref() {
                shell.publish(handler());
            }
            shell.capture_event();
            shell.invalidate_layout();
            shell.request_redraw();
        }
    }

    /// Handles an in-progress anchor drag: reports the world position the
    /// anchor was dragged to, on release.
    ///
    /// A report, not a change: the widget previews the offset in `draw` and the
    /// host applies it. A motionless press+release is a click, not a move, and
    /// emits nothing.
    fn handle_anchor_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor_index: usize,
        origin: WorldPoint,
    ) {
        let UpdateCtx {
            tree,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        if !matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) {
            return;
        }
        if let Some(cursor_position) = world_cursor.position() {
            let offset = cursor_position.into_euclid() - origin;
            let moved = offset.x.abs() > f32::EPSILON || offset.y.abs() > f32::EPSILON;
            if let Some(anchor) = self.anchors.get(anchor_index)
                && let Some(handler) = self.on_anchor_move.as_ref()
                && moved
            {
                shell.publish(handler(
                    anchor.id.clone(),
                    Point::new(anchor.position.x + offset.x, anchor.position.y + offset.y),
                ));
            }
        }
        tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
        if let Some(handler) = self.on_drag_end.as_ref() {
            shell.publish(handler());
        }
        shell.capture_event();
        shell.request_redraw();
    }

    /// Handles an in-progress grip resize: reports the size the node's content
    /// should have on every cursor move, and ends the drag on release.
    ///
    /// A report, not a change. The widget has no node size to set - the content
    /// element's layout is the size - so the node keeps its current bounds for
    /// the whole drag and only grows once the host lays its content out at the
    /// reported size. Measuring against `start` (the size at press) rather than
    /// against the live bounds is what makes that lag harmless: every report is
    /// absolute, so a host that applies none, some or all of them still lands
    /// exactly under the cursor.
    ///
    /// Release commits nothing further: the last report already carried the
    /// final size, and neither a move nor a selection change belongs to a
    /// gesture that never moved the node.
    fn handle_resize(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        origin: LayoutPoint,
        start: Size,
    ) {
        let UpdateCtx {
            tree,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = world_cursor.position() else {
                    return;
                };
                // Same conversion as `Dragging::Node`: both points are in
                // layout-absolute space, so that origin term cancels and the
                // delta is pure world units.
                let offset = cursor_position.into_euclid() - origin;
                let Some(node_id) = self.node_id_at(node_index).cloned() else {
                    return;
                };
                if let Some(handler) = self.on_resize.as_ref() {
                    let size = Size::new(
                        (start.width + offset.x).max(MIN_NODE_SIZE.width),
                        (start.height + offset.y).max(MIN_NODE_SIZE.height),
                    );
                    shell.publish(handler(node_id, size));
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<NodeGraphState>();
                state.dragging = Dragging::None;
                shell.capture_event();
                shell.invalidate_layout();
                shell.request_redraw();
            }
            _ => {}
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
                        let from_node_id = self.node_id_at(from_node).cloned();
                        let to_node_id = self.node_id_at(to_node).cloned();

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

                            if let Some(handler) = self.on_connect.as_ref() {
                                shell
                                    .publish(handler(EdgeEnd::Pin(from_ref), EdgeEnd::Pin(to_ref)));
                            }
                        }

                        state.dragging = Dragging::EdgeOver {
                            from_node,
                            from_pin,
                            to_node,
                            to_pin,
                        };
                    } else if let Some(origin) =
                        pin_world_position::<P, UI>(&tree.children, *layout, from_node, from_pin)
                    {
                        // No pin took the drag, so try the anchor orbits. A pin
                        // always wins: it is a real endpoint, an orbit only a
                        // waypoint.
                        let snap = nearest_droppable_orbit(
                            self,
                            state,
                            origin,
                            cursor_position.into_euclid(),
                            snap_threshold,
                        );
                        if let Some((anchor_index, orbit, hand)) = snap
                            && let (Some(from_nid), Some(from_pid), Some(anchor_id)) = (
                                self.node_id_at(from_node).cloned(),
                                from_pin_id,
                                self.anchors.get(anchor_index).map(|a| a.id.clone()),
                            )
                        {
                            // An orbit has no direction, so there is nothing to
                            // normalize: the dragged pin stays first.
                            if let Some(handler) = self.on_connect.as_ref() {
                                shell.publish(handler(
                                    EdgeEnd::Pin(PinRef::new(from_nid, from_pid)),
                                    EdgeEnd::Orbit {
                                        anchor: anchor_id,
                                        orbit,
                                        hand,
                                    },
                                ));
                            }
                            state.dragging = Dragging::EdgeOverOrbit {
                                from_node,
                                from_pin,
                                anchor: anchor_index,
                                orbit,
                                hand,
                            };
                        }
                    }
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = Dragging::None;
                // Emit drag end event
                if let Some(handler) = self.on_drag_end.as_ref() {
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
                        let from_node_id = self.node_id_at(from_node).cloned();
                        let to_node_id = self.node_id_at(to_node).cloned();

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

                            if let Some(handler) = self.on_disconnect.as_ref() {
                                shell
                                    .publish(handler(EdgeEnd::Pin(from_ref), EdgeEnd::Pin(to_ref)));
                            }
                        }

                        // Moved away from pin, go back to dragging
                        state.dragging = Dragging::Edge {
                            from_node,
                            from_pin,
                            origin: cursor_position.into_euclid(),
                        };
                    }
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Edge already connected via snap event - just end the drag
                state.dragging = Dragging::None;
                // Emit drag end event
                if let Some(handler) = self.on_drag_end.as_ref() {
                    shell.publish(handler());
                }
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// The snapped state of an edge drag that landed on an anchor orbit: the
    /// same hysteresis as [`handle_edge_over`], measured against the ring
    /// instead of a pin.
    fn handle_edge_over_orbit(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        from_node: usize,
        from_pin: usize,
        anchor_index: usize,
        orbit: u8,
        hand: Hand,
    ) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = world_cursor.position() else {
                    return;
                };
                let from_pin_id =
                    find_pin_id::<P, UI>(&tree.children, *layout, from_node, from_pin);
                let state = tree.state.downcast_mut::<NodeGraphState>();
                let unsnap_threshold = UNSNAP_THRESHOLD / state.camera.zoom();
                let (offset, spacing) = state
                    .orbit_geometry
                    .borrow()
                    .get(anchor_index)
                    .copied()
                    .unwrap_or((DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING));
                let ring = self
                    .anchors
                    .get(anchor_index)
                    .map(|anchor| edge_path::Orbit {
                        center: [anchor.position.x, anchor.position.y],
                        radius: offset + orbit as f32 * spacing,
                    });
                let still_over = ring.is_some_and(|ring| {
                    ring.ring_distance([cursor_position.x, cursor_position.y]) < unsnap_threshold
                });

                if !still_over {
                    if let (Some(from_nid), Some(from_pid), Some(anchor_id)) = (
                        self.node_id_at(from_node).cloned(),
                        from_pin_id,
                        self.anchors.get(anchor_index).map(|a| a.id.clone()),
                    ) && let Some(handler) = self.on_disconnect.as_ref()
                    {
                        shell.publish(handler(
                            EdgeEnd::Pin(PinRef::new(from_nid, from_pid)),
                            EdgeEnd::Orbit {
                                anchor: anchor_id,
                                orbit,
                                hand,
                            },
                        ));
                    }
                    tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::Edge {
                        from_node,
                        from_pin,
                        origin: cursor_position.into_euclid(),
                    };
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    shell.publish(handler());
                }
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Handles a loose cable kept at an orbit: snap-tests against the valid
    /// target pins and reports the connection on snap, the mirror of
    /// [`handle_edge_drag`] with a ring at the fixed end.
    fn handle_edge_from_orbit(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor_index: usize,
        orbit: u8,
        hand: Hand,
    ) {
        let UpdateCtx {
            tree,
            layout,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = world_cursor.position() else {
                    return;
                };
                let state = tree.state.downcast_ref::<NodeGraphState>();
                let snap_threshold = SNAP_THRESHOLD / state.camera.zoom();
                let valid_targets = state.valid_drop_targets.clone();
                let target = nearest_valid_pin::<P, UI>(
                    &tree.children,
                    *layout,
                    &valid_targets,
                    cursor_position,
                    snap_threshold,
                );

                if let Some((to_node, to_pin, to_pin_id)) = target {
                    if let (Some(to_nid), Some(anchor_id)) = (
                        self.node_id_at(to_node).cloned(),
                        self.anchors.get(anchor_index).map(|a| a.id.clone()),
                    ) && let Some(handler) = self.on_connect.as_ref()
                    {
                        shell.publish(handler(
                            EdgeEnd::Pin(PinRef::new(to_nid, to_pin_id)),
                            EdgeEnd::Orbit {
                                anchor: anchor_id,
                                orbit,
                                hand,
                            },
                        ));
                    }
                    tree.state.downcast_mut::<NodeGraphState>().dragging =
                        Dragging::OrbitEdgeOver {
                            anchor: anchor_index,
                            orbit,
                            hand,
                            to_node,
                            to_pin,
                        };
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    shell.publish(handler());
                }
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Handles a cable kept at an orbit whose grabbed pin is still snapped:
    /// leaving that pin by more than `UNSNAP_THRESHOLD` reports the
    /// disconnection and drops back to a loose drag.
    fn handle_orbit_edge_over(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor_index: usize,
        orbit: u8,
        hand: Hand,
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
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = world_cursor.position() else {
                    return;
                };
                let state = tree.state.downcast_ref::<NodeGraphState>();
                let unsnap_threshold = UNSNAP_THRESHOLD / state.camera.zoom();
                let held = pin_anchors::<P, UI>(&tree.children, *layout, to_node, to_pin);
                let still_over = held.is_some_and(|(a, b)| {
                    a.distance(cursor_position).min(b.distance(cursor_position)) < unsnap_threshold
                });

                if !still_over {
                    let to_pin_id = find_pin_id::<P, UI>(&tree.children, *layout, to_node, to_pin);
                    if let (Some(to_nid), Some(to_pid), Some(anchor_id)) = (
                        self.node_id_at(to_node).cloned(),
                        to_pin_id,
                        self.anchors.get(anchor_index).map(|a| a.id.clone()),
                    ) && let Some(handler) = self.on_disconnect.as_ref()
                    {
                        shell.publish(handler(
                            EdgeEnd::Pin(PinRef::new(to_nid, to_pid)),
                            EdgeEnd::Orbit {
                                anchor: anchor_id,
                                orbit,
                                hand,
                            },
                        ));
                    }
                    tree.state.downcast_mut::<NodeGraphState>().dragging =
                        Dragging::EdgeFromOrbit {
                            anchor: anchor_index,
                            orbit,
                            hand,
                            origin: cursor_position.into_euclid(),
                        };
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    shell.publish(handler());
                }
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Handles an in-progress selection box: tracks the moving corner and
    /// commits the intersecting set on release (Shift adds to the selection).
    fn handle_selection_box(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, start: LayoutPoint) {
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
                // Update the selection box's moving corner
                if let Some(cursor_position) = world_cursor.position() {
                    state.dragging = Dragging::SelectionBox(start, cursor_position.into_euclid());
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Close the selection box - collect the nodes it intersects
                if let Some(cursor_position) = world_cursor.position() {
                    let end: LayoutPoint = cursor_position.into_euclid();
                    let selection_rect = selection_rect_from_points(start, end);

                    // Without the multi-select modifier (keymap, default
                    // Shift): replace the selection. With it: add to it.
                    let additive = state.modifiers.contains(self.keymap.multi_select_modifiers);
                    let mut selected: HashSet<usize> = if additive {
                        self.resolved_selection(state)
                    } else {
                        HashSet::new()
                    };
                    for (node_index, node_layout) in layout.children().enumerate() {
                        if rects_intersect(&selection_rect, &node_layout.bounds()) {
                            selected.insert(node_index);
                        }
                    }

                    if let Some(handler) = self.on_select.as_ref() {
                        shell.publish(handler(self.selection_ids(&selected)));
                    }
                    state.pending_selection = Some(selected);
                }
                state.dragging = Dragging::None;
                // Emit drag end event
                if let Some(handler) = self.on_drag_end.as_ref() {
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
    fn handle_group_move(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, origin: LayoutPoint) {
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
                let indices: Vec<usize> = Self::selection_indices(&self.resolved_selection(state));
                if let Some(cursor_position) = world_cursor.position() {
                    let cursor_position: LayoutPoint = cursor_position.into_euclid();
                    let offset = cursor_position - origin;

                    // Translate internal indices to user IDs
                    let node_ids = self.node_ids_at(&indices);
                    let delta = offset.into_iced();
                    if let Some(handler) = self.on_move.as_ref() {
                        shell.publish(handler(delta, node_ids));
                    }
                }
                // Promote moved nodes to the top of the z-order.
                state.promote_z_many(&indices);
                state.dragging = Dragging::None;
                // Emit drag end event
                if let Some(handler) = self.on_drag_end.as_ref() {
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

            // Anchors sit above the cables but below the nodes, exactly as they
            // are drawn, so a node covering one wins the press.
            if self.try_start_anchor_drag(ctx, cursor_position.into_euclid()) {
                return;
            }
        }

        // Nothing hit - open a selection box on empty space, unless COMMAND is
        // held (reserved for edge cutting).
        self.start_selection_box_or_cut(ctx);
    }

    /// Grabs the anchor whose core is under the cursor. Returns whether the
    /// press was consumed.
    ///
    /// Only while [`on_anchor_move`](NodeGraph::on_anchor_move) is wired: a
    /// drag the host cannot apply would snap back on release, which reads as
    /// the widget being broken rather than as the feature being off.
    fn try_start_anchor_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        cursor_position: WorldPoint,
    ) -> bool {
        if self.on_anchor_move.is_none() {
            return false;
        }
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        // Screen-space threshold like every other hit target, so the core stays
        // grabbable at any zoom even once it is only a few pixels across.
        let half = ANCHOR_GRAB_THRESHOLD / state.camera.zoom();
        let hit = self.anchors.iter().position(|anchor| {
            (cursor_position.x - anchor.position.x).abs() <= half
                && (cursor_position.y - anchor.position.y).abs() <= half
        });
        let Some(anchor_index) = hit else {
            return false;
        };
        state.dragging = Dragging::Anchor {
            anchor: anchor_index,
            origin: cursor_position,
        };
        ctx.shell.capture_event();
        ctx.shell.request_redraw();
        true
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
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let cut_threshold = EDGE_CUT_THRESHOLD / state.camera.zoom();
        let cursor = [cursor_position.x, cursor_position.y];

        // A cut destroys a CABLE, not an edge: slicing a run that threads an
        // anchor takes every edge in it, so the gesture cannot leave a stump
        // dangling on a ring.
        for (chain, path) in cable_paths(self, tree, *layout, state) {
            if path.distance(cursor) >= cut_threshold {
                continue;
            }
            self.report_cut(shell, &chain);
            shell.capture_event();
            shell.request_redraw();
            return true;
        }
        false
    }

    /// Reports every edge of a cut cable: `on_disconnect` per edge (live
    /// feedback, endpoint-shaped) and one batched `on_edge_delete` naming their
    /// host ids.
    fn report_cut(&self, shell: &mut Shell<'_, Message>, chain: &[usize]) {
        let mut ids = Vec::with_capacity(chain.len());
        for &edge_index in chain {
            let Some(edge) = self.edges.get(edge_index) else {
                continue;
            };
            if let Some(handler) = self.on_disconnect.as_ref() {
                shell.publish(handler(edge.from.clone(), edge.to.clone()));
            }
            ids.push(edge.id.clone());
        }
        if let Some(handler) = self.on_edge_delete.as_ref()
            && !ids.is_empty()
        {
            shell.publish(handler(ids));
        }
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
        let Some(current_node_id) = self.node_id_at(node_index).cloned() else {
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
                    for (edge_index, Edge { from, to, .. }) in self.edges.iter().enumerate() {
                        let (Some(from_ref), Some(to_ref)) = (from.pin(), to.pin()) else {
                            continue;
                        };
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
                        if self.try_start_unplug(ctx, anchor, edge_index, (node_index, pin_index)) {
                            return true;
                        }
                    }

                    // A cable that runs through an anchor keeps its ring and
                    // hands the pin to the cursor, the mirror of the case
                    // above.
                    for (edge_index, edge) in self.edges.iter().enumerate() {
                        let grabbed = |end: &EdgeEnd<N, P>| {
                            end.pin().is_some_and(|pin| {
                                pin.node_id == current_node_id && pin.pin_id == pin_id
                            })
                        };
                        let kept = if grabbed(&edge.from) {
                            &edge.to
                        } else if grabbed(&edge.to) {
                            &edge.from
                        } else {
                            continue;
                        };
                        let EdgeEnd::Orbit {
                            anchor,
                            orbit,
                            hand,
                        } = kept
                        else {
                            continue;
                        };
                        if self.try_start_orbit_unplug(
                            ctx,
                            anchor,
                            *orbit,
                            *hand,
                            edge_index,
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

        // Body check for this same node (still top-first). The grip is a corner
        // of that body, so it is tested here and takes the press before the
        // move drag would - pins keep their precedence either way.
        if ctx.world_cursor.is_over(node_layout.bounds()) {
            if self.try_start_resize(ctx, node_index, cursor_position, node_layout.bounds()) {
                return true;
            }
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
        edge_index: usize,
        grabbed: (usize, usize),
    ) -> bool {
        let Some(anchor_node_idx) = self.node_index(&anchor.node_id) else {
            return false;
        };
        let Some(anchor_pin_idx) = pin_by_id::<P, UI>(
            &ctx.tree.children,
            ctx.layout,
            anchor_node_idx,
            &anchor.pin_id,
        )
        .map(|(index, _, _)| index) else {
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
            Some(edge_index),
        );
        let valid_orbits =
            compute_valid_orbits(self, ctx.tree, ctx.layout, anchor_node_idx, anchor_pin_idx);
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
        state.valid_drop_orbits = valid_orbits;
        // Anchor at the kept end, hold the grabbed pin snapped (still
        // connected).
        state.dragging = Dragging::EdgeOver {
            from_node: anchor_node_idx,
            from_pin: anchor_pin_idx,
            to_node: grabbed.0,
            to_pin: grabbed.1,
        };
        ctx.shell.capture_event();
        true
    }

    /// Starts the unplug drag for the pin end of a cable whose other end is an
    /// anchor orbit: the ring is kept, the pin comes loose.
    ///
    /// Valid targets are computed as if the drag came from the orbit's OTHER
    /// edge, because that is what a re-plugged pin would end up connected
    /// through. An orbit holding nothing else imposes no rule at all - it
    /// passes cables through and has no type of its own.
    fn try_start_orbit_unplug(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor_id: &N,
        orbit: u8,
        hand: Hand,
        edge_index: usize,
        grabbed: (usize, usize),
    ) -> bool {
        if self.on_connect.is_none() {
            return false;
        }
        let Some(anchor_index) = self.anchor_index(anchor_id) else {
            return false;
        };
        let key = (anchor_index, orbit);
        let partner_pin = self
            .orbit_attachments()
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|&edge_index| self.orbit_far_end(edge_index, key))
            .filter_map(|end| end.pin())
            .find(|pin| !(self.node_index(&pin.node_id) == Some(grabbed.0)))
            .cloned();

        let valid_targets = match partner_pin {
            Some(pin) => {
                let Some(node_index) = self.node_index(&pin.node_id) else {
                    return false;
                };
                let Some((pin_index, _, _)) =
                    pin_by_id::<P, UI>(&ctx.tree.children, ctx.layout, node_index, &pin.pin_id)
                else {
                    return false;
                };
                compute_valid_targets(
                    self,
                    ctx.tree,
                    ctx.layout,
                    node_index,
                    pin_index,
                    Some(edge_index),
                )
            }
            None => every_enabled_pin::<P, UI>(&ctx.tree.children, ctx.layout),
        };

        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
        state.valid_drop_orbits = std::collections::HashSet::new();
        state.dragging = Dragging::OrbitEdgeOver {
            anchor: anchor_index,
            orbit,
            hand,
            to_node: grabbed.0,
            to_pin: grabbed.1,
        };
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
        if self.on_connect.as_ref().is_none() {
            return false;
        }
        // Compute valid targets ONCE at drag-start.
        let valid_targets =
            compute_valid_targets(self, ctx.tree, ctx.layout, node_index, pin_index, None);
        let valid_orbits = compute_valid_orbits(self, ctx.tree, ctx.layout, node_index, pin_index);
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
        state.valid_drop_orbits = valid_orbits;
        state.dragging = Dragging::Edge {
            from_node: node_index,
            from_pin: pin_index,
            origin: cursor_position.into_euclid(),
        };
        if let Some(handler) = self.on_drag_start.as_ref() {
            ctx.shell.publish(handler(DragInfo::Edge {
                from_node: node_id.clone(),
                from_pin: pin_id.clone(),
            }));
        }
        ctx.shell.capture_event();
        true
    }

    /// Starts a resize drag when the press lands in a resizable node's grip.
    /// Returns whether the grip took the press.
    ///
    /// Gated on `on_resize` for the same reason the move drag is gated on
    /// `on_move`: node size is the host's content layout, so without a handler
    /// the drag would have nowhere to land - and no grip is drawn either, so
    /// the corner is plain node body.
    fn try_start_resize(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        cursor_position: Point,
        bounds: Rectangle,
    ) -> bool {
        if self.on_resize.is_none() || !self.nodes[node_index].resizable {
            return false;
        }
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        if !resize_grip_zone(bounds, state.camera.zoom()).contains(cursor_position) {
            return false;
        }
        state.dragging = Dragging::Resize {
            node: node_index,
            origin: cursor_position.into_euclid(),
            start: bounds.size(),
        };
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
        // The flags are last frame's selection, which is exactly the question
        // here: was this node already part of a selection when it was grabbed?
        let resolved = self.resolved_selection(state);
        let already_selected = resolved.contains(&node_index);
        let current: Vec<usize> = Self::selection_indices(&resolved);
        let modifiers = state.modifiers;
        let selection_changed;

        // Handle selection based on the multi-select modifier (keymap,
        // default Shift).
        let new_selection: Vec<usize> = if modifiers.contains(self.keymap.multi_select_modifiers) {
            selection_changed = true;
            if already_selected {
                // Multi-select click on a selected node: drop it.
                current
                    .iter()
                    .copied()
                    .filter(|&i| i != node_index)
                    .collect()
            } else {
                let mut next = current.clone();
                next.push(node_index);
                next
            }
        } else if !already_selected {
            // Regular click on an unselected node: this node alone.
            selection_changed = true;
            vec![node_index]
        } else {
            // Already selected and no modifier: keep it, so a group drag works.
            selection_changed = false;
            current.clone()
        };

        // Decide between single node drag or group move -
        // only when on_move is wired. Node positions come
        // from the host, so without on_move a drag would move
        // the node visually then snap back on the next frame;
        // gate it off (selection below still fires).
        if self.on_move.as_ref().is_some() {
            if current.len() > 1 && already_selected {
                // Multiple nodes selected, start group move
                let selected = current.clone();
                state.dragging = Dragging::GroupMove(cursor_position.into_euclid());
                // Emit drag start event for group
                if let Some(handler) = self.on_drag_start.as_ref() {
                    shell.publish(handler(DragInfo::Group {
                        node_ids: self.node_ids_at(&selected),
                    }));
                }
            } else {
                // Single node drag
                state.dragging = Dragging::Node {
                    node: node_index,
                    origin: cursor_position.into_euclid(),
                };
                // Emit drag start event for single node
                if let Some(handler) = self.on_drag_start.as_ref()
                    && let Some(node_id) = self.node_id_at(node_index).cloned()
                {
                    shell.publish(handler(DragInfo::Node { node_id }));
                }
            }
        }

        // Notify selection change, and hold the new value so a second click
        // arriving before the host applies this one composes with it.
        if selection_changed {
            let selected = self.node_ids_at(&new_selection);
            if let Some(handler) = self.on_select.as_ref() {
                shell.publish(handler(selected));
            }
            let state = tree.state.downcast_mut::<NodeGraphState>();
            state.pending_selection = Some(new_selection.into_iter().collect());
        }

        shell.capture_event();
    }

    /// Starts the empty-space press interaction: edge-cutting with COMMAND
    /// held, a selection box otherwise (Shift keeps the current selection).
    fn start_selection_box_or_cut(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        let UpdateCtx {
            tree,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        if let Some(cursor_position) = world_cursor.position() {
            let cursor_position: LayoutPoint = cursor_position.into_euclid();
            let state = tree.state.downcast_mut::<NodeGraphState>();

            // Edge-cut chord held: start edge cutting instead of a selection box
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
                // A pan anchor is compared against the raw screen cursor, so it
                // is a world point: fold the viewport origin back out of the
                // layout-absolute press position first.
                state.dragging = Dragging::Graph(state.camera.layout_to_world(cursor_position));
                shell.capture_event();
                return;
            }

            // The selection is replaced (or extended) when the box closes, so the
            // press leaves the current highlight visible while rubber-banding.
            state.dragging = Dragging::SelectionBox(cursor_position, cursor_position);
            // Emit drag start for the selection box
            if let Some(handler) = self.on_drag_start.as_ref() {
                shell.publish(handler(DragInfo::SelectionBox {
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
            if let Some(handler) = self.on_pan.as_ref() {
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
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
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
    // unzoomed - layout runs before the camera transform), and the fit math
    // works in world coordinates, so every rect folds the origin back out
    // through the camera's single conversion.
    let node_rect = |index: usize| -> Option<WorldRect> {
        let b = layout.children().nth(index)?.bounds();
        let origin = state.camera.layout_to_world(LayoutPoint::new(b.x, b.y));
        Some(WorldRect::new(origin, WorldSize::new(b.width, b.height)))
    };
    let union_of = |rects: &mut dyn Iterator<Item = WorldRect>| rects.reduce(|a, b| a.union(&b));
    // An edge's frame target is the union of the bounds of whatever nodes it
    // touches (seeing a connection means seeing the ends it connects). An orbit
    // end contributes nothing: an anchor has no layout child to measure.
    let edge_rect = |id: &E| -> Option<WorldRect> {
        let edge = graph.edges.iter().find(|edge| edge.id == *id)?;
        union_of(
            &mut [&edge.from, &edge.to]
                .into_iter()
                .filter_map(|end| end.pin())
                .filter_map(|pin| graph.node_index(&pin.node_id))
                .filter_map(node_rect),
        )
    };

    match target {
        FocusTarget::All => union_of(&mut (0..graph.nodes.len()).filter_map(node_rect)),
        FocusTarget::Selection => union_of(
            &mut graph
                .resolved_selection(state)
                .into_iter()
                .filter_map(node_rect),
        ),
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
/// `excluded_edge` is the index of the edge currently being re-routed, left out
/// of the occupancy check so it can be dropped back onto its own input. An index
/// rather than an endpoint pair, because a re-routed cable may have an orbit at
/// one end and no pin pair to name it by. Pass `None` when starting a fresh
/// edge.
fn compute_valid_targets<N, P, UI, Message, Renderer, E>(
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
    tree: &Tree,
    layout: Layout<'_>,
    from_node: usize,
    from_pin: usize,
    excluded_edge: Option<usize>,
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
        .enumerate()
        .filter(|(index, _)| Some(*index) != excluded_edge)
        .flat_map(|(_, edge)| [&edge.from, &edge.to])
        .filter_map(|end| end.pin())
        .map(|pin| (&pin.node_id, &pin.pin_id))
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

/// Every pin in the graph that accepts interaction, as `(node, pin)` indices.
///
/// The target set when nothing constrains the drop: an orbit with no other
/// cable on it has no type to be compatible with, so every reachable pin is
/// fair game.
fn every_enabled_pin<P, UI>(
    children: &[Tree],
    layout: Layout<'_>,
) -> std::collections::HashSet<(usize, usize)>
where
    P: PinId + 'static,
    UI: Clone + 'static,
{
    let mut pins = std::collections::HashSet::new();
    for (node_index, node_tree) in children.iter().enumerate() {
        let Some(node_layout) = layout.children().nth(node_index) else {
            continue;
        };
        for (pin_index, pin_state, _) in find_pins::<P, UI>(node_tree, node_layout) {
            if !pin_state.interactions_disabled {
                pins.insert((node_index, pin_index));
            }
        }
    }
    pins
}

/// The two hit anchors of pin `pin_index` on node `node_index`.
fn pin_anchors<P, UI>(
    children: &[Tree],
    layout: Layout<'_>,
    node_index: usize,
    pin_index: usize,
) -> Option<(Point, Point)>
where
    P: PinId + 'static,
    UI: Clone + 'static,
{
    let node_tree = children.get(node_index)?;
    let node_layout = layout.children().nth(node_index)?;
    find_pins::<P, UI>(node_tree, node_layout)
        .into_iter()
        .nth(pin_index)
        .map(|(_, _, anchors)| anchors)
}

/// The valid target pin nearest `cursor` within `threshold`, as
/// `(node index, pin index, pin id)`.
fn nearest_valid_pin<P, UI>(
    children: &[Tree],
    layout: Layout<'_>,
    valid: &std::collections::HashSet<(usize, usize)>,
    cursor: Point,
    threshold: f32,
) -> Option<(usize, usize, P)>
where
    P: PinId + 'static,
    UI: Clone + 'static,
{
    let mut best: Option<(f32, usize, usize, P)> = None;
    for (node_index, node_tree) in children.iter().enumerate() {
        let Some(node_layout) = layout.children().nth(node_index) else {
            continue;
        };
        for (pin_index, pin_state, (a, b)) in find_pins::<P, UI>(node_tree, node_layout) {
            if !valid.contains(&(node_index, pin_index)) {
                continue;
            }
            let distance = a.distance(cursor).min(b.distance(cursor));
            if distance >= threshold {
                continue;
            }
            if best.as_ref().is_none_or(|(d, ..)| distance < *d) {
                best = Some((distance, node_index, pin_index, pin_state.pin_id.clone()));
            }
        }
    }
    best.map(|(_, node, pin, id)| (node, pin, id))
}

/// The user id of pin `pin_index` on node `node_index`, read out of the
/// laid-out tree.
fn find_pin_id<P, UI>(
    children: &[Tree],
    layout: Layout<'_>,
    node_index: usize,
    pin_index: usize,
) -> Option<P>
where
    P: PinId + 'static,
    UI: Clone + 'static,
{
    let node_tree = children.get(node_index)?;
    let node_layout = layout.children().nth(node_index)?;
    find_pins::<P, UI>(node_tree, node_layout)
        .into_iter()
        .nth(pin_index)
        .map(|(_, state, _)| state.pin_id.clone())
}

/// World position of pin `pin_index` on node `node_index`, read out of the
/// laid-out tree. The two pin anchors coincide for a point pin, so the first
/// is the position.
fn pin_world_position<P, UI>(
    children: &[Tree],
    layout: Layout<'_>,
    node_index: usize,
    pin_index: usize,
) -> Option<WorldPoint>
where
    P: PinId + 'static,
    UI: Clone + 'static,
{
    let node_tree = children.get(node_index)?;
    let node_layout = layout.children().nth(node_index)?;
    find_pins::<P, UI>(node_tree, node_layout)
        .into_iter()
        .nth(pin_index)
        .map(|(_, _, (a, _))| a.into_euclid())
}

/// The orbit a drag should snap to: among the ones it may attach to, the one
/// whose RING lies nearest the cursor, within `threshold`.
///
/// Nearest to the ring, not to the anchor centre - the cursor is aimed at a
/// circle, and an outer ring you are sitting on top of must win over an inner
/// one you merely happen to be closer to the middle of.
///
/// Returns the orbit and which way round the cable wraps, taken from the side
/// of the ring the cursor is on.
fn nearest_droppable_orbit<N, P, UI, Message, Renderer, E>(
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
    state: &NodeGraphState,
    from: WorldPoint,
    cursor: WorldPoint,
    threshold: f32,
) -> Option<(usize, u8, Hand)>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
{
    let geometry = state.orbit_geometry.borrow();
    let from = [from.x, from.y];
    let at = [cursor.x, cursor.y];
    state
        .valid_drop_orbits
        .iter()
        .filter_map(|&(anchor_index, orbit)| {
            let anchor = graph.anchors.get(anchor_index)?;
            // Before the first draw there are no resolved radii yet; the
            // built-in geometry stands in, and `default_anchor_style` is built
            // from the same constants so the two agree.
            let (offset, spacing) = geometry
                .get(anchor_index)
                .copied()
                .unwrap_or((DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING));
            let ring = edge_path::Orbit {
                center: [anchor.position.x, anchor.position.y],
                radius: offset + orbit as f32 * spacing,
            };
            let distance = ring.ring_distance(at);
            if distance > threshold {
                return None;
            }
            let hand = ring.drop_hand(from, at)?;
            Some((distance, anchor_index, orbit, hand))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, anchor_index, orbit, hand)| (anchor_index, orbit, hand))
}

/// Computes the anchor orbits an edge drag may attach to, as
/// `(anchor index, orbit)`.
///
/// An anchor passes cables through and has no type of its own, so the
/// connection rules only bite once something is already on the orbit:
///
/// - Full (both hands taken): not a target, there is no hand left.
/// - Empty: accepts anything. There is nothing to be compatible with yet.
/// - Half: validated against the FAR end of the edge already attached, since
///   dropping here would join the two into one connection. A far end that is
///   itself an orbit is accepted - the chain is still open at its other end.
///
/// Only orbits [`visible_orbits`] offers are considered, so shells fill from
/// the inside out instead of a drag being able to jump to an outer ring.
fn compute_valid_orbits<N, P, UI, Message, Renderer, E>(
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
    tree: &Tree,
    layout: Layout<'_>,
    from_node: usize,
    from_pin: usize,
) -> std::collections::HashSet<(usize, u8)>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let mut valid = std::collections::HashSet::new();
    if graph.anchors.is_empty() {
        return valid;
    }

    let from_state = tree.children.get(from_node).and_then(|node_tree| {
        layout.children().nth(from_node).and_then(|node_layout| {
            find_pins::<P, UI>(node_tree, node_layout)
                .into_iter()
                .nth(from_pin)
                .map(|(_, state, _)| state.clone())
        })
    });
    let (Some(from_state), Some(from_node_id)) = (from_state, graph.node_id_at(from_node)) else {
        return valid;
    };

    let junctions = graph.orbit_attachments();
    for anchor_index in 0..graph.anchors.len() {
        let occupied: std::collections::HashSet<u8> = junctions
            .keys()
            .filter(|(a, _)| *a == anchor_index)
            .map(|(_, orbit)| *orbit)
            .collect();

        for orbit in crate::node_graph::visible_orbits(&occupied) {
            let key = (anchor_index, orbit);
            let attached = junctions.get(&key).map_or(0, |edges| edges.len());
            if attached >= 2 {
                continue;
            }
            let accepted = match junctions.get(&key).and_then(|edges| edges.first()) {
                None => true,
                Some(&partner) => match graph.orbit_far_end(partner, key) {
                    Some(EdgeEnd::Orbit { .. }) | None => true,
                    Some(EdgeEnd::Pin(far)) => {
                        pin_pair_accepted(graph, tree, layout, &from_state, from_node_id, far)
                    }
                },
            };
            if accepted {
                valid.insert(key);
            }
        }
    }
    valid
}

/// Runs the graph's connection rule for the dragged pin against a concrete far
/// pin, resolving that pin's live state out of the laid-out tree.
fn pin_pair_accepted<N, P, UI, Message, Renderer, E>(
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
    tree: &Tree,
    layout: Layout<'_>,
    from_state: &NodePinState<P, UI>,
    from_node_id: &N,
    far: &PinRef<N, P>,
) -> bool
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let Some(far_index) = graph.node_index(&far.node_id) else {
        return true;
    };
    let Some(far_state) = tree.children.get(far_index).and_then(|node_tree| {
        layout.children().nth(far_index).and_then(|node_layout| {
            find_pins::<P, UI>(node_tree, node_layout)
                .into_iter()
                .find(|(_, state, _)| state.pin_id == far.pin_id)
                .map(|(_, state, _)| state.clone())
        })
    }) else {
        return true;
    };
    // Neither end counts as occupied: the cable would terminate on the orbit,
    // not on the far pin, so the one-edge-per-input rule has nothing to bite.
    let from_end = PinEnd::new(
        from_node_id,
        &from_state.pin_id,
        from_state.direction,
        &from_state.user_info,
        false,
    );
    let to_end = PinEnd::new(
        &far.node_id,
        &far_state.pin_id,
        far_state.direction,
        &far_state.user_info,
        false,
    );
    match &graph.can_connect {
        Some(can_connect) => can_connect(from_end, to_end),
        None => crate::connection::default_can_connect(from_end, to_end),
    }
}

/// The positional index, anchors and side of pin `pin_id` on node `node_index`,
/// read out of the laid-out tree.
///
/// The index is the pin's position in `find_pins` walk order, which is also the
/// `pin_index` the drag states store. `None` when the node index is out of
/// range or the node has no such pin - a host may push an edge naming a pin
/// this frame's content does not contain.
///
/// Takes `tree.children` rather than the `Tree`, so a caller mid-interaction
/// can hold its `tree.state` borrow across the lookup.
fn pin_by_id<P: PinId + 'static, UI: 'static>(
    node_trees: &[Tree],
    layout: Layout<'_>,
    node_index: usize,
    pin_id: &P,
) -> Option<(usize, (Point, Point), PinSide)> {
    let node_tree = node_trees.get(node_index)?;
    let node_layout = layout.children().nth(node_index)?;
    find_pins::<P, UI>(node_tree, node_layout)
        .iter()
        .find(|(_, state, _)| state.pin_id == *pin_id)
        .map(|(index, state, anchors)| (*index, *anchors, state.side))
}

/// Creates a selection rectangle from two layout-absolute corner points
/// (handles any corner order), ready to compare against child layout bounds.
fn selection_rect_from_points(a: LayoutPoint, b: LayoutPoint) -> Rectangle {
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

/// Every connection's cable geometry in world space, paired with the edge
/// indices it was built from.
///
/// The cut paths measure against THIS - the cable as it is drawn, wraps and
/// all, rather than a direct pin-to-pin chord - so a cable that runs through an
/// anchor is hit where it actually is.
///
/// Curvature is taken as [`EdgeCurve::BezierCubic`]: resolving an edge's own
/// style needs the theme, which the interaction path does not have. Only a host
/// that switches an edge to `Line` sees a difference, and only in where the cut
/// threshold bites.
fn cable_paths<N, P, UI, Message, Renderer, E>(
    graph: &NodeGraph<'_, N, P, UI, Message, Renderer, E>,
    tree: &Tree,
    layout: Layout<'_>,
    state: &NodeGraphState,
) -> Vec<(Vec<usize>, edge_path::EdgePath)>
where
    N: NodeId + 'static,
    P: PinId + 'static,
    E: EdgeId + 'static,
    UI: Clone + 'static,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let geometry = state.orbit_geometry.borrow();
    let station = |end: &EdgeEnd<N, P>| -> Option<edge_path::Hop> {
        match end {
            EdgeEnd::Pin(pin) => {
                let node_index = graph.node_index(&pin.node_id)?;
                let (_, (position, _), side) =
                    pin_by_id::<P, UI>(&tree.children, layout, node_index, &pin.pin_id)?;
                Some(edge_path::Hop::Pin {
                    point: [position.x, position.y],
                    side: side.into(),
                })
            }
            EdgeEnd::Orbit {
                anchor,
                orbit,
                hand,
            } => {
                let anchor_index = graph.anchor_index(anchor)?;
                let center = graph.anchors.get(anchor_index)?.position;
                let (offset, spacing) = geometry
                    .get(anchor_index)
                    .copied()
                    .unwrap_or((DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING));
                Some(edge_path::Hop::Wrap {
                    orbit: edge_path::Orbit {
                        center: [center.x, center.y],
                        radius: offset + *orbit as f32 * spacing,
                    },
                    hand: *hand,
                })
            }
        }
    };
    let orbit_key = |end: &EdgeEnd<N, P>| match end {
        EdgeEnd::Orbit { anchor, orbit, .. } => {
            graph.anchor_index(anchor).map(|index| (index, *orbit))
        }
        EdgeEnd::Pin(_) => None,
    };

    let mut paths = Vec::new();
    for chain in graph.connection_chains() {
        let first = &graph.edges[chain[0]];
        // The chain starts at a pin; which one does not matter for a distance
        // query, so take `from` unless it is the orbit continuing the chain.
        let head_is_from = matches!(first.from, EdgeEnd::Pin(_));
        let (head, tail) = if head_is_from {
            (&first.from, &first.to)
        } else {
            (&first.to, &first.from)
        };
        let Some(head_hop) = station(head) else {
            continue;
        };
        let mut hops = vec![head_hop];

        let mut entry: Option<(usize, u8)> = None;
        let mut next = Some(tail);
        for &edge_index in &chain {
            let end = match next.take() {
                Some(end) => end,
                None => {
                    let edge = &graph.edges[edge_index];
                    if orbit_key(&edge.from) == entry {
                        &edge.to
                    } else {
                        &edge.from
                    }
                }
            };
            let Some(hop) = station(end) else { break };
            let is_wrap = matches!(hop, edge_path::Hop::Wrap { .. });
            hops.push(hop);
            if !is_wrap {
                break;
            }
            entry = orbit_key(end);
        }

        paths.push((
            chain,
            edge_path::build(&hops, &crate::style::EdgeCurve::BezierCubic),
        ));
    }
    paths
}
