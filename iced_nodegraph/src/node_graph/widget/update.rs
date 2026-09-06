//! The `update` event path of [`NodeGraph`]: the [`Dragging`] state machine and
//! the hit tests that drive its transitions.
//!
//! Every interaction the widget supports is a transition of that one enum, so a
//! new gesture is a new variant plus its entry and exit edges - never a flag
//! alongside it. Thresholds are declared in screen pixels and divided by zoom at
//! each comparison, so the on-screen hit target is constant at any zoom.

use super::*;
use crate::node_graph::camera::Camera2D;
use crate::node_graph::euclid::{WorldRect, WorldSize, WorldVector};
use crate::node_graph::input::KeyAction;
use crate::node_graph::state::AnchorGeometry;
use crate::node_graph::{EDGE_CUT_THRESHOLD, FocusOptions, FocusTarget, PIN_CLICK_THRESHOLD};
use euclid::Vector2D;
use iced_widget::core::{Padding, touch, window};
use std::collections::HashSet;

/// Hysteresis thresholds for edge snap/unsnap (prevents jitter at boundary).
/// Screen px, scaled by 1/zoom at the comparison sites like
/// [`PIN_CLICK_THRESHOLD`].
const SNAP_THRESHOLD: f32 = 10.0; // Distance to enter snap zone
const UNSNAP_THRESHOLD: f32 = 15.0; // Distance to leave snap zone (larger = more stable)

/// Travel (screen px) a press may drift and still count as a click rather than
/// a drag: a touch tap, and a pan-button press that commits an anchor delete or
/// a route detach instead of panning.
const TOUCH_TAP_TRAVEL: f32 = 8.0;
/// Longest a touch press and lift may take to count as a tap.
const TOUCH_TAP_MAX_SECS: f32 = 0.3;

/// Which part of a cable is under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CableZone {
    /// The stretch at one end: a press unplugs that end. `at_start` names the
    /// OUTPUT-side end, the one the cable's hop chain begins at.
    End { edge: usize, at_start: bool },
    /// Where the cable wraps an anchor.
    Wrap { edge: usize, anchor: usize },
    /// The run between the end zones and the wraps.
    Run { edge: usize },
}

/// A resolved zone plus the arc-length window of the cable a press takes hold
/// of, which is also the stretch the hover feedback marks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CableHit {
    pub zone: CableZone,
    pub window: (f32, f32),
}

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

impl<I, Message, Theme, Renderer> NodeGraph<'_, I, Message, Theme, Renderer>
where
    I: Ids,
    Theme: Catalog,
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

        // Sync the host-controlled camera (`camera()`) into the state, but only
        // when the host changed it since we last synced. Comparing against the
        // live camera would also fire while the user is mid pan/zoom (before
        // the matching `on_camera` round-trips back into `camera`), clobbering
        // the interaction with a stale value. Same race-avoidance as selection.
        if let Some(camera) = self.camera
            && state.last_synced_camera != Some(camera)
        {
            let (position, zoom) = camera;
            state.camera =
                Camera2D::with_zoom_and_position(zoom, WorldPoint::new(position.x, position.y));
            state.last_synced_camera = Some(camera);
            // An explicit camera() the running tween did not just emit is an
            // app override: it wins and cancels the tween (arbitration rule:
            // explicit camera() > user input > running tween > routine sync).
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
        let z_indices = z_render_indices(
            state,
            self.nodes.len(),
            |i| selection.contains(&i),
            |i| self.nodes[i].frame,
        );

        // A `focus` task the operate pass resolved against the layout (see
        // `Widget::operate`): start the fit here, where a shell exists to
        // commit through. Unlike the keymap frame actions below this is not
        // gated on `on_camera`: an uncontrolled graph (no `camera()` /
        // `on_camera` round trip) can still be framed, since the camera lives
        // in `state` regardless of whether the host observes it
        // (`begin_focus` only *publishes* through `on_camera` when a handler
        // is set).
        if let Some((world_aabb, opts)) = state.pending_focus.take() {
            self.begin_focus(state, world_aabb, layout.bounds().size(), &opts, shell);
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
            // mirroring the controlled on_camera pattern. A host showing live
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
            // through `on_camera` every frame and keeps `last_synced_camera`
            // in step with what it just emitted, so the camera()-sync above
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
            // publish a second `on_camera` whose value disagrees with
            // `last_synced_camera` in the low f32 bits -- which the
            // camera()-sync block above would then mistake for an app
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

                let camera = (Point::new(position.x, position.y), zoom);
                if let Some(handler) = self.on_camera.as_ref() {
                    shell.publish(handler(camera.0, camera.1));
                }
                state.last_synced_camera = Some(camera);

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
                    let selected: Vec<I::NodeId> =
                        self.nodes.iter().map(|node| node.id.clone()).collect();
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
            if let Some(handler) = self.on_camera.as_ref() {
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
                        Dragging::None => self.refresh_hover(&mut ctx),
                        Dragging::EdgeCutting { .. } => self.handle_edge_cutting(&mut ctx),
                        Dragging::Graph(origin) => self.handle_graph_pan(&mut ctx, origin),
                        Dragging::Node {
                            node: node_index,
                            followers,
                            ..
                        } => self.handle_node_drag(&mut ctx, node_index, &followers),
                        Dragging::Anchor { anchor, origin } => {
                            self.handle_anchor_drag(&mut ctx, anchor, origin)
                        }
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
                        Dragging::Route { edge, detached } => {
                            self.handle_route_drag(&mut ctx, edge, detached)
                        }
                        // The anchor it left is re-derived on the way out, so
                        // the variant's own `detached` is only the preview's.
                        Dragging::RouteOver { edge, anchor, .. } => {
                            self.handle_route_over(&mut ctx, edge, anchor)
                        }
                        Dragging::PressPending {
                            origin_world,
                            origin_screen,
                            target,
                        } => {
                            self.handle_press_pending(&mut ctx, origin_world, origin_screen, target)
                        }
                        Dragging::SelectionBox(start, _current) => {
                            self.handle_selection_box(&mut ctx, start)
                        }
                        Dragging::GroupMove {
                            anchor, followers, ..
                        } => self.handle_group_move(&mut ctx, anchor, &followers),
                        Dragging::Minimap => self.handle_minimap_drag(&mut ctx),
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
                    // input consumes Home/f first). Gated on on_camera (like
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
                        && self.on_camera.as_ref().is_some()
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
    /// pinch-zoom and pan the camera directly, committing through `on_camera`
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
                    if let Some(handler) = self.on_camera.as_ref() {
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
        match ctx.event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(cursor_position) = ctx.world_cursor.position() {
                    let cursor_position: LayoutPoint = cursor_position.into_euclid();
                    // The cables as they are drawn, wraps and all, so a routed
                    // cable is cut where it actually runs.
                    let cables = self.cable_geometry(ctx.tree, ctx.layout);
                    let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                    if let Dragging::EdgeCutting {
                        ref mut trail,
                        ref mut pending_cuts,
                    } = state.dragging
                    {
                        trail.push(cursor_position);

                        // The cut runs from where the drag started to the cursor.
                        let cut_start = trail.first().copied().unwrap_or(cursor_position);
                        let from = [cut_start.x, cut_start.y];
                        let to = [cursor_position.x, cursor_position.y];

                        // Only what the line crosses right now is pending.
                        pending_cuts.clear();
                        for (geometry, built) in &cables {
                            if built.path.intersects(from, to) {
                                pending_cuts.insert(geometry.edge);
                            }
                        }
                    }
                }
                ctx.shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Delete all pending edges on release
                let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                if let Dragging::EdgeCutting { pending_cuts, .. } = &state.dragging {
                    let mut cut_ids = Vec::new();
                    for &edge_idx in pending_cuts.iter() {
                        if let Some(Edge { id, from, to, .. }) = self.edges.get(edge_idx) {
                            if let Some(handler) = self.on_disconnect.as_ref() {
                                ctx.shell.publish(handler(from.clone(), to.clone()));
                            }
                            cut_ids.push(id.clone());
                        }
                    }
                    if let Some(handler) = self.on_edge_delete.as_ref()
                        && !cut_ids.is_empty()
                    {
                        ctx.shell.publish(handler(cut_ids));
                    }
                }
                ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    ctx.shell.publish(handler());
                }
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
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
                if let Some(handler) = self.on_camera.as_ref() {
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
    ///
    /// `followers` ride along without being selected - the contents a frame
    /// press collected - and share the grabbed node's delta, so the frame and
    /// what it holds stay put relative to each other.
    fn handle_node_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        followers: &[usize],
    ) {
        let UpdateCtx {
            tree,
            event,
            world_cursor,
            shell,
            ..
        } = &mut *ctx;
        if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
            let moved_indices = merged_indices(&[node_index], followers);
            if let Some(cursor_position) = world_cursor.position() {
                let state = tree.state.downcast_ref::<NodeGraphState>();
                let offset = drag_offset(state, self, node_index, cursor_position.into_euclid());

                // A press+release without motion is a click, not
                // a move: don't emit a spurious move (which would
                // dirty host state / undo history on a plain
                // selection click). Only report an actual drag.
                let moved = offset.x.abs() > f32::EPSILON || offset.y.abs() > f32::EPSILON;

                let node_ids = self.node_ids_at(&moved_indices);
                if moved
                    && !node_ids.is_empty()
                    && let Some(handler) = self.on_move.as_ref()
                {
                    shell.publish(handler(offset.into_iced(), node_ids));
                }
            }
            let state = tree.state.downcast_mut::<NodeGraphState>();
            // Promote what was dragged to the top of the z-order on drop; the
            // render sort keeps a frame behind its contents regardless.
            state.promote_z_many(&moved_indices);
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
            // Both ends of the delta in one space: `origin` is a world point,
            // and the cursor arrives layout-absolute.
            let state = tree.state.downcast_ref::<NodeGraphState>();
            let cursor = state.camera.layout_to_world(cursor_position.into_euclid());
            let offset = anchor_drag_offset(state, self, anchor_index, cursor - origin);
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
                    // The far corner is what lands on the grid, not the size:
                    // a node whose origin is off-grid still gets an on-grid
                    // right and bottom edge.
                    let state = tree.state.downcast_ref::<NodeGraphState>();
                    let extent = snapped_delta(
                        state,
                        self,
                        self.nodes[node_index].position,
                        LayoutVector::new(start.width + offset.x, start.height + offset.y),
                    );
                    let size = Size::new(
                        extent.x.max(MIN_NODE_SIZE.width),
                        extent.y.max(MIN_NODE_SIZE.height),
                    );
                    shell.publish(handler(node_id, size));
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<NodeGraphState>();
                state.dragging = Dragging::None;
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
                    let mut from_pin_id: Option<I::PinId> = None;
                    let mut from_dir: Option<PinDirection> = None;
                    let mut target_info: Option<(usize, usize, I::PinId, PinDirection)> = None;

                    // Check all pins for proximity and validity (use SNAP_THRESHOLD to enter)
                    for (node_index, (node_layout, node_tree)) in
                        layout.children().zip(&tree.children).enumerate()
                    {
                        for (pin_index, pin_state, (a, b)) in find_pins::<I>(node_tree, node_layout)
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
                                shell.publish(handler(from_ref, to_ref));
                            }
                        }

                        state.dragging = Dragging::EdgeOver {
                            from_node,
                            from_pin,
                            to_node,
                            to_pin,
                        };
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
                    let mut from_pin_id: Option<I::PinId> = None;
                    let mut to_pin_id: Option<I::PinId> = None;
                    let mut from_dir: Option<PinDirection> = None;
                    let mut to_dir: Option<PinDirection> = None;

                    for (node_index, (node_layout, node_tree)) in
                        layout.children().zip(&tree.children).enumerate()
                    {
                        for (pin_index, pin_state, (a, b)) in find_pins::<I>(node_tree, node_layout)
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
                                shell.publish(handler(from_ref, to_ref));
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

    /// Handles an in-progress route drag: snap-tests the phantom anchor it
    /// holds against every eligible anchor and reports the attachment on snap
    /// (plug behaviour), or a new anchor at the cursor on release.
    ///
    /// A plain click on a cable is a grab that never moved, so it creates an
    /// anchor where it was pressed; the undo is a pan-button click on the core.
    fn handle_route_drag(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        edge: usize,
        detached: Option<usize>,
    ) {
        match ctx.event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(cursor_position) = ctx.world_cursor.position()
                    && let Some(anchor) =
                        self.route_snap_target(ctx.tree, edge, detached, cursor_position)
                {
                    if let Some(handler) = self.on_route_attach.as_ref()
                        && let Some(edge_id) = self.edges.get(edge).map(|edge| edge.id.clone())
                        && let Some(anchor_id) =
                            self.anchors.get(anchor).map(|anchor| anchor.id.clone())
                    {
                        ctx.shell.publish(handler(edge_id, anchor_id));
                    }
                    ctx.tree.state.downcast_mut::<NodeGraphState>().dragging =
                        Dragging::RouteOver {
                            edge,
                            anchor,
                            // Snapping back onto the anchor the drag pulled off
                            // supersedes that detachment: the anchor is wanted
                            // again, so it must stop being held out of the
                            // preview. Leaving it excluded here contradicts
                            // being snapped to it, and the cable would draw
                            // with no wrap at all - the real one suppressed by
                            // the exclusion, the offered one skipped because
                            // the host's route still names the anchor.
                            detached: detached.filter(|held| *held != anchor),
                        };
                }
                ctx.shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(cursor_position) = ctx.world_cursor.position()
                    && let Some(handler) = self.on_anchor_create.as_ref()
                    && let Some(edge_id) = self.edges.get(edge).map(|edge| edge.id.clone())
                {
                    // An anchor's position is a world point the host stores, so
                    // the layout-absolute cursor folds the viewport origin back
                    // out. `on_anchor_move` needs no such step: it reports
                    // position plus a delta, and a delta carries no origin.
                    let at = ctx
                        .tree
                        .state
                        .downcast_ref::<NodeGraphState>()
                        .camera
                        .layout_to_world(cursor_position.into_euclid());
                    ctx.shell.publish(handler(edge_id, Point::new(at.x, at.y)));
                }
                ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    ctx.shell.publish(handler());
                }
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Handles the snapped state of a route drag: leaving the held ring by more
    /// than `UNSNAP_THRESHOLD` reports the detachment and falls back to
    /// [`Dragging::Route`].
    ///
    /// Releasing here commits nothing: the attachment was published on snap.
    fn handle_route_over(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        edge: usize,
        anchor: usize,
    ) {
        match ctx.event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = ctx.world_cursor.position() else {
                    return;
                };
                let unsnap = UNSNAP_THRESHOLD
                    / ctx
                        .tree
                        .state
                        .downcast_ref::<NodeGraphState>()
                        .camera
                        .zoom();
                // Measured against the anchor's OUTERMOST ring, the same circle
                // the snap used, so the gesture does not depend on which orbit
                // the cable ends up on: that is decided from the geometry of
                // every cable through the anchor and can change while the drag
                // is still in flight.
                let ring = self.anchor_reach(
                    ctx.tree,
                    anchor,
                    Some(PendingRoute {
                        edge,
                        attach: Some(anchor),
                        detach: None,
                    }),
                );
                let left = match ring {
                    Some(ring) => {
                        Point::new(ring.center[0], ring.center[1]).distance(cursor_position)
                            > ring.radius + unsnap
                    }
                    // An anchor the host has dropped mid-drag is past every
                    // threshold there is.
                    None => true,
                };
                if left {
                    if let Some(handler) = self.on_route_detach.as_ref()
                        && let Some(edge_id) = self.edges.get(edge).map(|edge| edge.id.clone())
                        && let Some(anchor_id) =
                            self.anchors.get(anchor).map(|anchor| anchor.id.clone())
                    {
                        ctx.shell.publish(handler(edge_id, anchor_id));
                    }
                    ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::Route {
                        edge,
                        // Held out of the preview until the host applies the
                        // detach, and eligible again so the drag can put it back.
                        detached: Some(anchor),
                    };
                }
                ctx.shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    ctx.shell.publish(handler());
                }
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
            }
            _ => {}
        }
    }

    /// The anchor a route drag on `edge` snaps to at `cursor`: the nearest
    /// eligible one whose offered ring the cursor has reached.
    ///
    /// Measured against the ring the cable would RUN on rather than the
    /// anchor's innermost one, so a busy anchor takes the cable where it will
    /// actually sit.
    fn route_snap_target(
        &self,
        tree: &Tree,
        edge: usize,
        detached: Option<usize>,
        cursor: Point,
    ) -> Option<usize> {
        let snap = SNAP_THRESHOLD / tree.state.downcast_ref::<NodeGraphState>().camera.zoom();
        // The detach is folded in so an anchor the drag just left offers the
        // reach it will have once the host applies it; the attach is not,
        // because that is what each candidate is being asked about.
        let pending = PendingRoute {
            edge,
            attach: None,
            detach: detached,
        };
        let mut best: Option<(usize, f32)> = None;
        for anchor in self.route_snap_eligible(edge, detached) {
            let Some(ring) = self.anchor_reach(
                tree,
                anchor,
                Some(PendingRoute {
                    attach: Some(anchor),
                    ..pending
                }),
            ) else {
                continue;
            };
            // The ring centre IS the anchor's core in the cursor's own space,
            // so nothing here has to reach for the raw world position.
            let distance = Point::new(ring.center[0], ring.center[1]).distance(cursor);
            if distance > ring.radius + snap {
                continue;
            }
            match best {
                Some((_, held)) if held <= distance => {}
                _ => best = Some((anchor, distance)),
            }
        }
        best.map(|(anchor, _)| anchor)
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
    /// selected node, plus the frame contents following them, on release.
    ///
    /// `anchor` is the node the press landed on, whose origin carries the grid
    /// snap for the whole group.
    fn handle_group_move(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        anchor: usize,
        followers: &[usize],
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
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_ref::<NodeGraphState>();
                // Complete group move - notify all selected nodes moved
                let selected = Self::selection_indices(&self.resolved_selection(state));
                let indices = merged_indices(&selected, followers);
                if let Some(cursor_position) = world_cursor.position() {
                    let offset = drag_offset(state, self, anchor, cursor_position.into_euclid());

                    // Translate internal indices to user IDs
                    let node_ids = self.node_ids_at(&indices);
                    if let Some(handler) = self.on_move.as_ref() {
                        shell.publish(handler(offset.into_iced(), node_ids));
                    }
                }
                let state = tree.state.downcast_mut::<NodeGraphState>();
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

    /// Handles an in-progress minimap drag: every cursor position re-centers
    /// the camera on the world point the map shows there, and the release ends
    /// the gesture.
    ///
    /// Absolute rather than incremental, so the viewport marker stays under the
    /// cursor no matter how far the drag has travelled or how the map's extent
    /// changed under it.
    fn handle_minimap_drag(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        match ctx.event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(cursor) = ctx.screen_cursor.position() {
                    self.center_camera_from_minimap(ctx, cursor);
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
            }
            _ => {}
        }
    }

    /// The world rect each anchor occupies, in index order: the outermost ring
    /// it carries, or orbit 0 for one carrying no edge, the ring a bare core
    /// sits inside. Read off the geometry the last draw published.
    ///
    /// An anchor has no child layout of its own, so its position is already
    /// the world point. Framing and the minimap both bound anchors through
    /// this, so the map shows everything a frame-all would fit.
    pub(super) fn anchor_ring_rects(&self, state: &NodeGraphState) -> Vec<WorldRect> {
        let anchor_geometry = state.anchor_geometry.borrow();
        let rings = self.anchor_rings(None);
        self.anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| {
                let orbit = u8::try_from(rings.get(index).copied().unwrap_or(0).saturating_sub(1))
                    .unwrap_or(u8::MAX);
                let radius = anchor_geometry
                    .get(index)
                    .copied()
                    .unwrap_or_default()
                    .orbit_radius(orbit);
                let center = anchor.position;
                WorldRect::new(
                    WorldPoint::new(center.x - radius, center.y - radius),
                    WorldSize::new(radius * 2.0, radius * 2.0),
                )
            })
            .collect()
    }

    /// Centers the viewport on the world point the minimap shows at `cursor`,
    /// clamped to the map, and commits it - the continuous commit wheel zoom
    /// and pinch use, since a map gesture has no single release to report.
    ///
    /// The map's world extent is re-derived from this frame's layout and
    /// camera, so the mapping a press reads is the one the map was drawn with.
    fn center_camera_from_minimap(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, cursor: Point) {
        let Some(minimap) = self.minimap.as_ref() else {
            return;
        };
        let UpdateCtx {
            tree,
            layout,
            shell,
            ..
        } = &mut *ctx;
        let state = tree.state.downcast_mut::<NodeGraphState>();
        let bounds = layout.bounds();
        let map = minimap::rect(minimap, bounds);
        let visible = state.camera.visible_world_rect(bounds);
        let world = minimap::world_bounds(
            layout
                .children()
                .map(|child| {
                    let b = child.bounds();
                    WorldRect::new(
                        state.camera.layout_to_world(LayoutPoint::new(b.x, b.y)),
                        WorldSize::new(b.width, b.height),
                    )
                })
                .chain(self.anchor_ring_rects(state)),
            visible,
        );
        // Outside the pane the mapping would keep scaling, so a drag that left
        // it would fly the camera off the graph; the edge of the map is the
        // furthest it can steer.
        let at = Point::new(
            cursor.x.clamp(map.x, map.x + map.width),
            cursor.y.clamp(map.y, map.y + map.height),
        );
        let target = Camera2D::position_for_center(
            minimap::Projection::new(map, world).map_to_world(at),
            state.camera.zoom(),
            bounds.size(),
            Padding::ZERO,
        );
        // User input beats a running tween (arbitration rule 2).
        state.camera_tween = None;
        state.camera = state.camera.move_by(target - state.camera.position());
        if let Some(handler) = self.on_camera.as_ref() {
            shell.publish(handler(Point::new(target.x, target.y), state.camera.zoom()));
        }
        shell.request_redraw();
    }

    /// Dispatches a left-button press: the minimap, then an edge cut, then the
    /// per-node pin/body hit-test (top-first by z-order), then the empty-space
    /// fallback.
    ///
    /// This holds every `Dragging::None -> *` transition of the left button;
    /// in-progress transitions live in the `handle_*` methods above.
    fn handle_left_press(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, z_indices: &[usize]) {
        // A press while another drag is in progress (e.g. left press during a
        // pan) must not hijack the state machine mid-drag.
        if ctx.tree.state.downcast_ref::<NodeGraphState>().dragging != Dragging::None {
            return;
        }

        // The map is drawn over every layer, so it takes the press before any
        // node, pin, cable or anchor is asked about it.
        if self.try_press_minimap(ctx) {
            return;
        }

        let state = ctx.tree.state.downcast_ref::<NodeGraphState>();
        // Multi-select-modifier+drag from an occupied pin forks a NEW edge
        // instead of unplugging the existing one.
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
            // Anchors and cables are drawn under every node, so they answer
            // only once no node has claimed the press.
            if self.try_start_anchor_drag(ctx, cursor_position.into_euclid())
                || self.try_press_cable(ctx, cursor_position)
            {
                return;
            }
        }

        // Nothing hit - open a selection box on empty space, unless COMMAND is
        // held (reserved for edge cutting).
        self.start_selection_box_or_cut(ctx);
    }

    /// Takes a left press that landed on the minimap: jumps the camera to the
    /// world point pressed and holds the map for the rest of the gesture.
    /// Returns whether the map consumed the press.
    ///
    /// Screen space, because that is where the map was placed: the press is
    /// tested against the raw cursor, not the camera-inverted one the node hit
    /// tests use.
    fn try_press_minimap(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) -> bool {
        let Some(minimap) = self.minimap.as_ref() else {
            return false;
        };
        let Some(cursor) = ctx.screen_cursor.position() else {
            return false;
        };
        if !minimap::rect(minimap, ctx.layout.bounds()).contains(cursor) {
            return false;
        }
        self.center_camera_from_minimap(ctx, cursor);
        ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::Minimap;
        ctx.shell.capture_event();
        true
    }

    /// Cuts the first edge within `EDGE_CUT_THRESHOLD` of the cursor
    /// (Command+Click edge cut). Returns whether a cut consumed the press.
    ///
    /// Measured against the routed cable, so an edge that wraps an anchor is
    /// cut where it runs.
    fn try_cut_edge_at_cursor(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) -> bool {
        let Some(cursor_position) = ctx.world_cursor.position() else {
            return false;
        };
        // Screen-space threshold: constant hit target at any zoom.
        let cut_threshold = EDGE_CUT_THRESHOLD
            / ctx
                .tree
                .state
                .downcast_ref::<NodeGraphState>()
                .camera
                .zoom();
        let at = [cursor_position.x, cursor_position.y];
        let cut = self
            .cable_geometry(ctx.tree, ctx.layout)
            .into_iter()
            .find(|(_, built)| built.path.distance(at) < cut_threshold)
            .map(|(geometry, _)| geometry.edge);
        let Some(Edge { id, from, to, .. }) = cut.and_then(|edge| self.edges.get(edge)) else {
            return false;
        };
        if let Some(handler) = self.on_disconnect.as_ref() {
            ctx.shell.publish(handler(from.clone(), to.clone()));
        }
        if let Some(handler) = self.on_edge_delete.as_ref() {
            ctx.shell.publish(handler(vec![id.clone()]));
        }
        ctx.shell.capture_event();
        ctx.shell.request_redraw();
        true
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
        let pins: Vec<(usize, I::PinId, bool, (Point, Point))> =
            find_pins::<I>(node_tree, node_layout)
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
                    for Edge {
                        from: from_ref,
                        to: to_ref,
                        ..
                    } in &self.edges
                    {
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
        anchor: &PinRef<I>,
        edge: (&PinRef<I>, &PinRef<I>),
        grabbed: (usize, usize),
    ) -> bool {
        let Some(anchor_node_idx) = self.node_index(&anchor.node_id) else {
            return false;
        };
        let Some(anchor_pin_idx) = pin_by_id::<I>(
            &ctx.tree.children,
            ctx.layout,
            anchor_node_idx,
            &anchor.pin_id,
        )
        .map(|(index, ..)| index) else {
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
        state.dragging = Dragging::EdgeOver {
            from_node: anchor_node_idx,
            from_pin: anchor_pin_idx,
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
        pin_id: &I::PinId,
        node_id: &I::NodeId,
        cursor_position: Point,
    ) -> bool {
        if self.on_connect.as_ref().is_none() {
            return false;
        }
        // Compute valid targets ONCE at drag-start.
        let valid_targets =
            compute_valid_targets(self, ctx.tree, ctx.layout, node_index, pin_index, None);
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        state.valid_drop_targets = valid_targets;
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
        if let Some(handler) = self.on_drag_start.as_ref()
            && let Some(node_id) = self.node_id_at(node_index).cloned()
        {
            ctx.shell.publish(handler(DragInfo::Resize { node_id }));
        }
        ctx.shell.capture_event();
        true
    }

    /// The nodes the frames among `pressed` carry: every node whose layout rect
    /// lies fully inside a frame's, minus the frame itself and anything
    /// `already_moving` reports (the selection a group move already covers).
    ///
    /// Containment is resolved here, at press time, and never stored - the
    /// widget keeps no graph state between frames, so a node dropped into a
    /// frame is carried by the next drag with nothing to register. Non-frame
    /// entries in `pressed` contribute nothing, which is what makes an ordinary
    /// node drag pay a single flag test.
    fn frame_followers(
        &self,
        layout: Layout<'_>,
        pressed: &[usize],
        already_moving: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        let mut followers: Vec<usize> = Vec::new();
        for &frame_index in pressed {
            if !self.nodes.get(frame_index).is_some_and(|node| node.frame) {
                continue;
            }
            let Some(frame_bounds) = layout.children().nth(frame_index).map(|l| l.bounds()) else {
                continue;
            };
            for (index, node_layout) in layout.children().enumerate() {
                if index == frame_index
                    || already_moving(index)
                    || followers.contains(&index)
                    || !contains_rect(frame_bounds, node_layout.bounds())
                {
                    continue;
                }
                followers.push(index);
            }
        }
        followers.sort_unstable();
        followers
    }

    /// Applies click-selection semantics for a node body press and starts the
    /// matching drag (`Node` or `GroupMove`, gated on `on_move` being wired).
    fn select_or_drag_node(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        node_index: usize,
        cursor_position: Point,
    ) {
        let UpdateCtx {
            tree,
            layout,
            shell,
            ..
        } = &mut *ctx;
        let layout = *layout;
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
                let followers = self.frame_followers(layout, &selected, |i| resolved.contains(&i));
                state.dragging = Dragging::GroupMove {
                    origin: cursor_position.into_euclid(),
                    anchor: node_index,
                    followers,
                };
                // Emit drag start event for group
                if let Some(handler) = self.on_drag_start.as_ref() {
                    shell.publish(handler(DragInfo::Group {
                        node_ids: self.node_ids_at(&selected),
                    }));
                }
            } else {
                // Single node drag, carrying this frame's contents if it is one
                let followers = self.frame_followers(layout, &[node_index], |_| false);
                state.dragging = Dragging::Node {
                    node: node_index,
                    origin: cursor_position.into_euclid(),
                    followers,
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
                if let Some(handler) = self.on_drag_start.as_ref() {
                    shell.publish(handler(DragInfo::EdgeCut));
                }
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

    /// Starts a graph pan from a press of the keymap's pan button, unless the
    /// press lands on something that button clicks instead: an anchor core to
    /// delete, or a wrap to detach. Either one is provisional - see
    /// [`handle_press_pending`](Self::handle_press_pending).
    fn handle_pan_press(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        let state = ctx.tree.state.downcast_ref::<NodeGraphState>();
        // Never cancel an in-progress node/edge/box drag: that would drop the
        // drag without emitting on_drag_end or committing the move.
        if state.dragging != Dragging::None {
            return;
        }
        let Some(cursor_position) = ctx.screen_cursor.position() else {
            return;
        };
        let origin_world: WorldPoint = state
            .camera
            .screen_to_world()
            .transform_point(cursor_position.into_euclid());

        if let Some(target) = self.press_target_at(ctx) {
            let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
            state.dragging = Dragging::PressPending {
                origin_world,
                origin_screen: cursor_position,
                target,
            };
            ctx.shell.capture_event();
            return;
        }

        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        // User-driven pan aborts a running focus tween (arbitration: user
        // input beats a tween).
        state.camera_tween = None;
        state.dragging = Dragging::Graph(origin_world);
        ctx.shell.capture_event();
    }

    /// What the pan button would click at the cursor, or `None` when the press
    /// is an ordinary pan.
    ///
    /// Each target is gated on the callback that would carry its click, not on
    /// the ones the route DRAG needs: a host that only wires
    /// [`on_route_detach`](NodeGraph::on_route_detach) still gets the detach
    /// click.
    fn press_target_at(&self, ctx: &UpdateCtx<'_, '_, '_, Message>) -> Option<PressTarget> {
        let cursor_position = ctx.world_cursor.position()?;
        // Anchors and cables are drawn UNDER every node, so they answer only
        // once no node covers the cursor - the same precedence the left press
        // and the cursor icon already follow. Without it, dragging a node over
        // an anchor turns a pan-button click on that node's body into a delete
        // of an anchor the user cannot see.
        if ctx
            .layout
            .children()
            .any(|node| node.bounds().contains(cursor_position))
        {
            return None;
        }
        if self.on_anchor_delete.is_some()
            && let Some(anchor) = self.core_at(ctx.tree, cursor_position.into_euclid())
        {
            return Some(PressTarget::AnchorCore { anchor });
        }
        if self.on_route_detach.is_some()
            && let Some(CableHit {
                zone: CableZone::Wrap { edge, anchor },
                ..
            }) = self.cable_zone_at(ctx.tree, ctx.layout, cursor_position.into_euclid())
        {
            return Some(PressTarget::Wrap { edge, anchor });
        }
        None
    }

    /// Handles a pan-button press held over a clickable target: travel makes it
    /// the pan it would have been, a release without travel commits the click.
    ///
    /// The pan continues from the ORIGINAL press point, so the image does not
    /// jump by however far the cursor got before the press was one.
    fn handle_press_pending(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        origin_world: WorldPoint,
        origin_screen: Point,
        target: PressTarget,
    ) {
        match ctx.event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(cursor_position) = ctx.screen_cursor.position() else {
                    return;
                };
                if origin_screen.distance(cursor_position) <= TOUCH_TAP_TRAVEL {
                    return;
                }
                let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                // User-driven pan aborts a running focus tween (arbitration:
                // user input beats a tween).
                state.camera_tween = None;
                state.dragging = Dragging::Graph(origin_world);
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(button))
                if *button == self.keymap.pan_button =>
            {
                match target {
                    PressTarget::AnchorCore { anchor } => {
                        if let Some(handler) = self.on_anchor_delete.as_ref()
                            && let Some(id) = self.anchors.get(anchor).map(|a| a.id.clone())
                        {
                            ctx.shell.publish(handler(id));
                        }
                    }
                    PressTarget::Wrap { edge, anchor } => {
                        if let Some(handler) = self.on_route_detach.as_ref()
                            && let Some(edge_id) = self.edges.get(edge).map(|e| e.id.clone())
                            && let Some(anchor_id) = self.anchors.get(anchor).map(|a| a.id.clone())
                        {
                            ctx.shell.publish(handler(edge_id, anchor_id));
                        }
                    }
                }
                // Nothing panned, so no `on_camera`. `on_drag_end` still fires:
                // this is a `Dragging::* -> None` transition like any other, and
                // a host that collects orphaned anchors when a gesture finishes
                // would otherwise never hear that this one did.
                ctx.tree.state.downcast_mut::<NodeGraphState>().dragging = Dragging::None;
                if let Some(handler) = self.on_drag_end.as_ref() {
                    ctx.shell.publish(handler());
                }
                ctx.shell.capture_event();
                ctx.shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Every edge's routed cable, in the layout-absolute space the press path
    /// hit-tests in.
    ///
    /// The one walk from topology to geometry on the interaction side: pressing,
    /// cutting and hovering all measure against THIS, so a cable that wraps an
    /// anchor is hit where it runs rather than along the chord between its pins.
    ///
    /// No drag preview is folded in. Nothing here aims at the phantom leg
    /// following the cursor: the gesture paths run either before a route drag
    /// starts or against the cable the host wrote.
    ///
    /// Curvature comes from the curves the last `draw` published, so a cable is
    /// hit-tested against the shape on screen: resolving an edge's own style
    /// needs a theme, which the interaction path does not have. An interaction
    /// cannot change a curve, so a frame-old value is the same value. Before the
    /// first frame there is nothing published and nothing drawn to hit either,
    /// and [`EdgeCurve::default`] stands in.
    fn cable_geometry(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
    ) -> Vec<(CableGeometry<'_, I>, edge_path::Built)> {
        let pin = |pin: &PinRef<I>| -> Option<Station> {
            let node_index = self.node_index(&pin.node_id)?;
            let (_, (point, _), side, direction) =
                pin_by_id::<I>(&tree.children, layout, node_index, &pin.pin_id)?;
            Some(Station {
                point: [point.x, point.y],
                side: side.into(),
                direction: Some(direction),
            })
        };
        let ring = |anchor: usize, orbit: u8| self.orbit_ring(tree, anchor, orbit);
        let curves = tree
            .state
            .downcast_ref::<NodeGraphState>()
            .edge_curves
            .borrow();
        let curve = |edge: usize| curves.get(edge).copied().unwrap_or_default();
        self.edge_hops(&pin, &ring, &curve, None)
            .into_iter()
            .map(|geometry| {
                let built = edge_path::build(&geometry.hops, &curve(geometry.edge));
                (geometry, built)
            })
            .collect()
    }

    /// One edge's endpoints in the order its cable runs them, output pin first.
    ///
    /// Read back off the same walk the zones were measured along, so the end a
    /// press grabs is the end the cursor was near.
    fn oriented_ends(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        edge: usize,
    ) -> Option<(&PinRef<I>, &PinRef<I>)> {
        self.cable_geometry(tree, layout)
            .into_iter()
            .find(|(geometry, _)| geometry.edge == edge)
            .map(|(geometry, _)| geometry.ends)
    }

    /// The cable part under `cursor`, or `None` when nothing is close enough or
    /// the gesture's callbacks are unwired.
    ///
    /// An unwired zone reports nothing rather than reporting a zone that cannot
    /// act, so the press falls through to what is behind the cable - the same
    /// shape every other gate in this file has.
    pub(super) fn cable_hit_at(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: LayoutPoint,
    ) -> Option<CableHit> {
        let hit = self.cable_zone_at(tree, layout, cursor)?;
        let wired = match hit.zone {
            // Unplugging an end re-routes it, which is a connection the host
            // has to be able to persist.
            CableZone::End { .. } => self.on_connect.is_some(),
            // A route drag can create, attach and detach before it is over, so
            // all three have to be there before it starts.
            CableZone::Wrap { .. } | CableZone::Run { .. } => {
                self.on_anchor_create.is_some()
                    && self.on_route_attach.is_some()
                    && self.on_route_detach.is_some()
            }
        };
        wired.then_some(hit)
    }

    /// The cable part under `cursor`, whatever is wired.
    ///
    /// Every cable is classified and the CLOSEST zone wins, measured by the
    /// distance each kind of zone is defined by: a wrap by how far the cursor
    /// is from the ring the frame draws, an end or a run by how far it is from
    /// the cable itself. Picking one cable up front by lateral distance instead
    /// would hand a press on a visible outer ring to whichever cable happens to
    /// run nearer that spot - a wrap belongs to the edge whose ring it is.
    fn cable_zone_at(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: LayoutPoint,
    ) -> Option<CableHit> {
        let cables = self.cable_geometry(tree, layout);
        let mut best: Option<(CableHit, f32)> = None;
        for (geometry, built) in &cables {
            let Some(candidate) = self.cable_zone_of(tree, geometry, built, cursor) else {
                continue;
            };
            match best {
                Some((_, held)) if held <= candidate.1 => {}
                _ => best = Some(candidate),
            }
        }
        best.map(|(hit, _)| hit)
    }

    /// The zone one cable offers at `cursor`, paired with the distance that
    /// zone is measured by so [`cable_zone_at`](Self::cable_zone_at) can rank
    /// cables against each other.
    fn cable_zone_of(
        &self,
        tree: &Tree,
        geometry: &CableGeometry<'_, I>,
        built: &edge_path::Built,
        cursor: LayoutPoint,
    ) -> Option<(CableHit, f32)> {
        // Screen-space thresholds: constant hit targets at any zoom. Each one
        // that competes with a WORLD-fixed quantity is capped against it below,
        // because dividing by zoom grows a threshold without bound as the
        // camera pulls back while a ring radius or a cable's length stays put.
        let zoom = tree.state.downcast_ref::<NodeGraphState>().camera.zoom();
        let grab = EDGE_GRAB_THRESHOLD / zoom;
        let ring_grab = ANCHOR_GRAB_THRESHOLD / zoom;
        let at = [cursor.x, cursor.y];

        let near = built.path.nearest(at);
        let edge = geometry.edge;
        let total = built.path.total_len();

        // The end zones first: a wrap or a run reaching into one would take a
        // press that is much more likely meant for the plug. Capped at a third
        // of the cable so the two ends can never meet in the middle: a cable
        // shorter than twice the raw budget would otherwise be end zone from
        // tip to tip, leaving no run to grab and no wrap to detach.
        let end_zone = (EDGE_END_GRAB_LENGTH / zoom).min(total / 3.0);
        let from_start = near.arc_len;
        let from_end = total - near.arc_len;
        if near.distance <= grab && from_start.min(from_end) <= end_zone {
            let at_start = from_start <= from_end;
            let window = if at_start {
                (0.0, end_zone.min(total))
            } else {
                ((total - end_zone).max(0.0), total)
            };
            return Some((
                CableHit {
                    zone: CableZone::End { edge, at_start },
                    window,
                },
                near.distance,
            ));
        }

        // A wrap is grabbed by its RING, not by the cable's lateral distance:
        // the ring is what the frame draws to say this edge passes here.
        for touch in &built.touches {
            let Some(&(_, (anchor, orbit))) =
                geometry.rings.iter().find(|(hop, _)| *hop == touch.hop)
            else {
                // A phantom wrap names no anchor of the host's.
                continue;
            };
            let Some(ring) = self.orbit_ring(tree, anchor, orbit) else {
                continue;
            };
            // Capped at half the radius so the band can never reach the
            // anchor's centre and swallow the whole disc.
            let ring_grab = ring_grab.min(ring.radius * 0.5);
            if near.arc_len < touch.span.0 - ring_grab || near.arc_len > touch.span.1 + ring_grab {
                continue;
            }
            let off_ring = ring.ring_distance(at);
            if off_ring > ring_grab {
                continue;
            }
            return Some((
                CableHit {
                    zone: CableZone::Wrap { edge, anchor },
                    window: touch.span,
                },
                off_ring,
            ));
        }

        if near.distance <= grab {
            // The stretch a press takes hold of is centred on the cursor, so
            // the glow marks the grab rather than the whole cable.
            let half = end_zone / 2.0;
            return Some((
                CableHit {
                    zone: CableZone::Run { edge },
                    window: (
                        (near.arc_len - half).max(0.0),
                        (near.arc_len + half).min(total),
                    ),
                },
                near.distance,
            ));
        }
        None
    }

    /// Where an anchor sits in the layout-absolute space every gesture
    /// hit-tests in.
    ///
    /// A node arrives layout-absolute already - its child layout carries the
    /// viewport origin - but an anchor has no layout of its own, so the origin
    /// has to be folded in here. It is the same conversion `draw` applies
    /// before it strokes a core and its rings, and without it a graph placed
    /// anywhere but the window origin is grabbed where it would have been at
    /// the origin rather than where it is.
    fn anchor_layout_point(&self, tree: &Tree, anchor: usize) -> Option<LayoutPoint> {
        let position = self.anchors.get(anchor)?.position;
        Some(
            tree.state
                .downcast_ref::<NodeGraphState>()
                .camera
                .world_to_layout(position.into_euclid()),
        )
    }

    /// The circle an orbit describes, in the layout-absolute space the cable is
    /// built in, at the radii the last frame published.
    pub(super) fn orbit_ring(
        &self,
        tree: &Tree,
        anchor: usize,
        orbit: u8,
    ) -> Option<edge_path::Orbit> {
        let center = self.anchor_layout_point(tree, anchor)?;
        let radius = tree
            .state
            .downcast_ref::<NodeGraphState>()
            .anchor_geometry
            .borrow()
            .get(anchor)
            .copied()
            .unwrap_or_default()
            .orbit_radius(orbit);
        Some(edge_path::Orbit {
            center: [center.x, center.y],
            radius,
        })
    }

    /// The outermost circle an anchor shows, which is how far a route drag can
    /// reach it.
    ///
    /// Snap and unsnap are both measured against this rather than against the
    /// orbit the cable will occupy. The orbit is decided from the geometry of
    /// every cable through the anchor, so it can change while the drag is still
    /// in flight; the anchor's reach cannot, which is what keeps a drag from
    /// detaching itself the frame after it attached.
    ///
    /// `pending` folds in the drag's own edit, so an anchor about to gain a
    /// cable already offers the ring that cable will add.
    fn anchor_reach(
        &self,
        tree: &Tree,
        anchor: usize,
        pending: Option<PendingRoute>,
    ) -> Option<edge_path::Orbit> {
        let rings = self.anchor_rings(pending).get(anchor).copied()?;
        let outermost = u8::try_from(rings.saturating_sub(1)).unwrap_or(u8::MAX);
        self.orbit_ring(tree, anchor, outermost)
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
        cursor_position: LayoutPoint,
    ) -> bool {
        let Some(anchor_index) = self.anchor_core_at(ctx.tree, cursor_position) else {
            return false;
        };
        let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
        // A world point, as the variant declares: the preview measures the
        // offset against the screen cursor mapped straight into world space,
        // so an origin term left in here would show up as a jump.
        let origin = state.camera.layout_to_world(cursor_position);
        state.dragging = Dragging::Anchor {
            anchor: anchor_index,
            origin,
        };
        if let Some(handler) = self.on_drag_start.as_ref() {
            ctx.shell.publish(handler(DragInfo::Anchor {
                anchor_id: self.anchors[anchor_index].id.clone(),
            }));
        }
        ctx.shell.capture_event();
        ctx.shell.request_redraw();
        true
    }

    /// The anchor whose core is under `cursor`.
    ///
    /// `None` while [`on_anchor_move`](NodeGraph::on_anchor_move) is unwired:
    /// there is no gesture to offer, so nothing to point at either.
    pub(super) fn anchor_core_at(&self, tree: &Tree, cursor: LayoutPoint) -> Option<usize> {
        self.on_anchor_move.as_ref()?;
        self.core_at(tree, cursor)
    }

    /// The anchor whose core is under `cursor`, whatever is wired.
    ///
    /// `cursor` is the layout-absolute cursor the child walk hands out, the
    /// same one the pin and cable tests take, which is why the cores it is
    /// compared against go through
    /// [`anchor_layout_point`](Self::anchor_layout_point).
    fn core_at(&self, tree: &Tree, cursor: LayoutPoint) -> Option<usize> {
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let zoom = state.camera.zoom();
        let geometry = state.anchor_geometry.borrow();
        (0..self.anchors.len()).find(|&anchor| {
            let half = core_grab_half(geometry.get(anchor).copied().unwrap_or_default(), zoom);
            self.anchor_layout_point(tree, anchor).is_some_and(|core| {
                (cursor.x - core.x).abs() <= half && (cursor.y - core.y).abs() <= half
            })
        })
    }

    /// Starts the gesture the cable under the cursor offers: unplugging an end,
    /// or a route drag from a wrap or the run between them. Returns whether the
    /// cable took the press.
    fn try_press_cable(
        &self,
        ctx: &mut UpdateCtx<'_, '_, '_, Message>,
        cursor_position: Point,
    ) -> bool {
        let Some(hit) = self.cable_hit_at(ctx.tree, ctx.layout, cursor_position.into_euclid())
        else {
            return false;
        };
        match hit.zone {
            CableZone::End { edge, at_start } => {
                let Some(cable) = self.edges.get(edge) else {
                    return false;
                };
                let Some((output, input)) = self.oriented_ends(ctx.tree, ctx.layout, edge) else {
                    return false;
                };
                // The hop chain starts at the output pin, so that is the end
                // the arc length was measured from.
                let (near, far) = if at_start {
                    (output, input)
                } else {
                    (input, output)
                };
                let Some(near_node) = self.node_index(&near.node_id) else {
                    return false;
                };
                let Some((near_pin, ..)) =
                    pin_by_id::<I>(&ctx.tree.children, ctx.layout, near_node, &near.pin_id)
                else {
                    return false;
                };
                self.try_start_unplug(ctx, far, (&cable.from, &cable.to), (near_node, near_pin))
            }
            // Already wrapping this anchor: the grab publishes no route edit
            // until it leaves (`handle_route_over`).
            CableZone::Wrap { edge, anchor } => {
                let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                state.dragging = Dragging::RouteOver {
                    edge,
                    anchor,
                    detached: None,
                };
                self.publish_route_start(ctx, edge);
                true
            }
            CableZone::Run { edge } => {
                let state = ctx.tree.state.downcast_mut::<NodeGraphState>();
                state.dragging = Dragging::Route {
                    edge,
                    detached: None,
                };
                self.publish_route_start(ctx, edge);
                true
            }
        }
    }

    /// Reports the route drag just started on `edge` and claims the press.
    fn publish_route_start(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>, edge: usize) {
        if let Some(handler) = self.on_drag_start.as_ref() {
            ctx.shell.publish(handler(DragInfo::Route {
                edge_id: self.edges[edge].id.clone(),
            }));
        }
        ctx.shell.capture_event();
        ctx.shell.request_redraw();
    }

    /// Asks for the frame the hover feedback needs.
    ///
    /// A hover keeps no geometry - `draw` resolves what is under the cursor from
    /// the same walk it strokes - but drawing it needs a frame, and a cursor
    /// move over an idle graph asks for none. So this decides only WHETHER a
    /// frame is owed, and it errs wide on purpose: anywhere feedback could
    /// appear counts, plus the move that leaves such a place, which is the frame
    /// that clears it.
    fn refresh_hover(&self, ctx: &mut UpdateCtx<'_, '_, '_, Message>) {
        if !matches!(ctx.event, Event::Mouse(mouse::Event::CursorMoved { .. })) {
            return;
        }
        let inside = ctx
            .world_cursor
            .position()
            .is_some_and(|at| self.hover_zone(ctx.tree, ctx.layout, at));
        let state = ctx.tree.state.downcast_ref::<NodeGraphState>();
        if inside || state.hover_zone.get() {
            ctx.shell.request_redraw();
        }
        state.hover_zone.set(inside);
    }

    /// Whether `cursor` is anywhere the hover feedback can draw something: an
    /// anchor's core, or a stretch of cable a press would take.
    ///
    /// Ordered cheapest first, and a superset of what `draw` lights up.
    fn hover_zone(&self, tree: &Tree, layout: Layout<'_>, cursor: Point) -> bool {
        self.anchor_core_at(tree, cursor.into_euclid()).is_some()
            || self
                .cable_hit_at(tree, layout, cursor.into_euclid())
                .is_some()
    }

    /// Starts a fit toward `world_aabb`: a tween when `opts.animation` is
    /// set with a positive duration, otherwise an immediate jump. Cancels
    /// any running tween (new focus/frame always wins, arbitration rule 1).
    /// The jump commits through `on_camera` immediately, like any other camera
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
            if let Some(handler) = self.on_camera.as_ref() {
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

/// Half-extent of the square a press may land in and still grab an anchor's
/// core, in world units, for an anchor whose frame published `geometry`.
///
/// [`ANCHOR_GRAB_THRESHOLD`] is screen pixels, so it is divided by zoom like
/// every other hit target. Both clamps around it are WORLD-fixed, so each has
/// to be stated in its own right:
///
/// - the floor is the core the frame paints. Without it, zooming past
///   `ANCHOR_GRAB_THRESHOLD / core_half` leaves the box narrower than the dot
///   on screen, and a press well inside the dot falls through to the canvas
///   gesture behind it.
/// - the cap keeps the square's CORNER - hence `FRAC_1_SQRT_2` - off orbit 0.
///   The core is offered a press before any cable zone, so a square reaching
///   the innermost ring would take every press meant for the innermost wrap.
///   Same shape as `resize_grip_zone`, which caps a screen-sized grip by the
///   node it sits on.
///
/// The cap is applied last, so it wins where the two disagree, which is only
/// when a host styles `core_size` above `sqrt(2) * orbit_offset` - a core
/// already painted over its own innermost ring. Losing the floor costs the core
/// its own reach; losing the cap costs the wrap its gesture entirely, since
/// route detach has no other entry.
fn core_grab_half(geometry: AnchorGeometry, zoom: f32) -> f32 {
    (ANCHOR_GRAB_THRESHOLD / zoom)
        .max(geometry.core_half())
        .min(geometry.orbit_radius(0) * std::f32::consts::FRAC_1_SQRT_2)
}

/// Resolves a [`FocusTarget`] to a world-space AABB using live layout, or
/// `None` for an unknown/empty target -- a no-op per the design (no camera
/// change, no `on_camera`): an unresolvable id is skipped, `All`/`Selection`
/// with nothing to union is empty, `Nodes`/`Edges` union whatever resolves.
pub(super) fn resolve_focus_target<I, Message, Theme, Renderer>(
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    layout: Layout<'_>,
    state: &NodeGraphState,
    target: &FocusTarget<I>,
) -> Option<WorldRect>
where
    I: Ids,
    Theme: Catalog,
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
    let anchor_rects = graph.anchor_ring_rects(state);
    let anchor_rect = |index: usize| -> Option<WorldRect> { anchor_rects.get(index).copied() };
    let by_node =
        |id: &I::NodeId| -> Option<WorldRect> { graph.node_index(id).and_then(node_rect) };
    let by_anchor =
        |id: &I::AnchorId| -> Option<WorldRect> { graph.anchor_index(id).and_then(&anchor_rect) };
    // An edge's frame target is the union of its two endpoint nodes' bounds and
    // every anchor it wraps (seeing a connection means seeing where it runs);
    // either endpoint failing to resolve skips the whole edge.
    let edge_rect = |id: &I::EdgeId| -> Option<WorldRect> {
        let index = graph.edges.iter().position(|edge| edge.id == *id)?;
        let edge = &graph.edges[index];
        let a = node_rect(graph.node_index(&edge.from.node_id)?)?;
        let b = node_rect(graph.node_index(&edge.to.node_id)?)?;
        Some(
            graph
                .resolved_route(index)
                .into_iter()
                .filter_map(&anchor_rect)
                .fold(a.union(&b), |whole, ring| whole.union(&ring)),
        )
    };

    match target {
        FocusTarget::All => union_of(
            &mut (0..graph.nodes.len())
                .filter_map(node_rect)
                .chain((0..graph.anchors.len()).filter_map(&anchor_rect)),
        ),
        FocusTarget::Selection => union_of(
            &mut graph
                .resolved_selection(state)
                .into_iter()
                .filter_map(node_rect),
        ),
        FocusTarget::Node(id) => by_node(id),
        FocusTarget::Nodes(ids) => union_of(&mut ids.iter().filter_map(&by_node)),
        FocusTarget::Anchor(id) => by_anchor(id),
        FocusTarget::Anchors(ids) => union_of(&mut ids.iter().filter_map(&by_anchor)),
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
fn compute_valid_targets<I, Message, Theme, Renderer>(
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    tree: &Tree,
    layout: Layout<'_>,
    from_node: usize,
    from_pin: usize,
    excluded_edge: Option<(&PinRef<I>, &PinRef<I>)>,
) -> std::collections::HashSet<(usize, usize)>
where
    I: Ids,
    Theme: Catalog,
    Renderer: iced_wgpu::core::renderer::Renderer + iced_wgpu::primitive::Renderer,
{
    let mut valid_targets = std::collections::HashSet::new();

    // Get the source pin state for validation.
    let from_pin_state = tree.children.get(from_node).and_then(|node_tree| {
        layout.children().nth(from_node).and_then(|node_layout| {
            find_pins::<I>(node_tree, node_layout)
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
    let occupied: std::collections::HashSet<(&I::NodeId, &I::PinId)> = graph
        .edges
        .iter()
        .filter(|edge| excluded_edge != Some((&edge.from, &edge.to)))
        .flat_map(|edge| {
            [
                (&edge.from.node_id, &edge.from.pin_id),
                (&edge.to.node_id, &edge.to.pin_id),
            ]
        })
        .collect();
    let is_occupied =
        |node_id: &I::NodeId, pin_id: &I::PinId| occupied.contains(&(node_id, pin_id));

    // Iterate all pins in all nodes
    for (node_index, (node_layout, node_tree)) in layout.children().zip(&tree.children).enumerate()
    {
        for (pin_index, pin_state, _) in find_pins::<I>(node_tree, node_layout) {
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

/// The positional index, anchors, side and direction of pin `pin_id` on node
/// `node_index`, read out of the laid-out tree.
///
/// The index is the pin's position in `find_pins` walk order, which is also the
/// `pin_index` the drag states store. `None` when the node index is out of
/// range or the node has no such pin - a host may push an edge naming a pin
/// this frame's content does not contain.
///
/// Takes `tree.children` rather than the `Tree`, so a caller mid-interaction
/// can hold its `tree.state` borrow across the lookup.
fn pin_by_id<I: Ids>(
    node_trees: &[Tree],
    layout: Layout<'_>,
    node_index: usize,
    pin_id: &I::PinId,
) -> Option<(usize, (Point, Point), PinSide, PinDirection)> {
    let node_tree = node_trees.get(node_index)?;
    let node_layout = layout.children().nth(node_index)?;
    find_pins::<I>(node_tree, node_layout)
        .iter()
        .find(|(_, state, _)| state.pin_id == *pin_id)
        .map(|(index, state, anchors)| (*index, *anchors, state.side, state.direction))
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

/// Whether `outer` fully encloses `inner`, edges counting as inside.
fn contains_rect(outer: Rectangle, inner: Rectangle) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// The union of `a` and `b` as node indices in push order, deduped: the set
/// `on_move` reports and the z-order promotes.
fn merged_indices(a: &[usize], b: &[usize]) -> Vec<usize> {
    let mut merged = Vec::with_capacity(a.len() + b.len());
    merged.extend_from_slice(a);
    merged.extend_from_slice(b);
    merged.sort_unstable();
    merged.dedup();
    merged
}

/// Whether the in-flight drag carries `node_index`.
///
/// `is_selected` answers against the frame's resolved selection, which only a
/// group move consults; callers pass the selection they already hold rather
/// than resolving one per node.
pub(super) fn drag_carries(
    state: &NodeGraphState,
    node_index: usize,
    is_selected: impl Fn(usize) -> bool,
) -> bool {
    match &state.dragging {
        Dragging::Node {
            node, followers, ..
        } => *node == node_index || followers.contains(&node_index),
        Dragging::GroupMove { followers, .. } => {
            is_selected(node_index) || followers.contains(&node_index)
        }
        _ => false,
    }
}

/// The one shared delta of an in-flight node or group drag, in layout units,
/// or `None` when the pointer is dragging something else.
///
/// Every node the drag carries moves by this exact vector, so a group keeps its
/// internal layout and only the grabbed node lands on the snap grid.
pub(super) fn drag_delta<I, Message, Theme, Renderer>(
    state: &NodeGraphState,
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    cursor_layout: LayoutPoint,
) -> Option<LayoutVector>
where
    I: Ids,
    Theme: Catalog,
{
    let (origin, anchor) = match &state.dragging {
        Dragging::Node { node, origin, .. } => (*origin, *node),
        Dragging::GroupMove { origin, anchor, .. } => (*origin, *anchor),
        _ => return None,
    };
    let origin_world = graph.nodes.get(anchor)?.position;
    Some(snapped_delta(
        state,
        graph,
        origin_world,
        cursor_layout - origin,
    ))
}

/// The layout-space offset the in-flight drag applies to `node_index` this
/// frame: the shared drag delta if the drag carries it, zero otherwise.
///
/// The single answer to "where does this node sit right now", read by the draw
/// preview and by the release handlers that publish `on_move`, so what the user
/// sees and what the host is told cannot drift apart.
pub(super) fn drag_offset<I, Message, Theme, Renderer>(
    state: &NodeGraphState,
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    node_index: usize,
    cursor_layout: LayoutPoint,
) -> LayoutVector
where
    I: Ids,
    Theme: Catalog,
{
    let Some(delta) = drag_delta(state, graph, cursor_layout) else {
        return LayoutVector::zero();
    };
    // Only a group move needs the selection, so nothing else pays for it.
    let selection = match state.dragging {
        Dragging::GroupMove { .. } => graph.resolved_selection(state),
        _ => HashSet::new(),
    };
    if drag_carries(state, node_index, |i| selection.contains(&i)) {
        delta
    } else {
        LayoutVector::zero()
    }
}

/// The world-space offset an in-flight anchor drag applies to `anchor_index`:
/// `raw` snapped so the anchor lands on the graph's grid.
///
/// Read by the draw preview and by the release handler that publishes
/// `on_anchor_move`, so the anchor lands where it was shown.
pub(super) fn anchor_drag_offset<I, Message, Theme, Renderer>(
    state: &NodeGraphState,
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    anchor_index: usize,
    raw: WorldVector,
) -> WorldVector
where
    I: Ids,
    Theme: Catalog,
{
    let Some(origin) = graph
        .anchors
        .get(anchor_index)
        .map(|anchor| anchor.position)
    else {
        return raw;
    };
    snapped_delta(state, graph, origin, raw)
}

/// `raw` adjusted so the world point `origin` lands on the graph's snap grid
/// once moved by it; `raw` unchanged when no grid is set or the override
/// modifier is held.
///
/// The modifier is read off the live state, so pressing or releasing it
/// mid-drag takes effect on the next frame. Layout and world space differ only
/// in origin, so a delta is the same vector in both; the unit is generic for
/// that reason.
pub(super) fn snapped_delta<I, Message, Theme, Renderer, U>(
    state: &NodeGraphState,
    graph: &NodeGraph<'_, I, Message, Theme, Renderer>,
    origin: Point,
    raw: Vector2D<f32, U>,
) -> Vector2D<f32, U>
where
    I: Ids,
    Theme: Catalog,
{
    let Some(spacing) = graph.snap_grid.filter(|s| *s > 0.0 && s.is_finite()) else {
        return raw;
    };
    if state.modifiers.contains(graph.keymap.snap_override) {
        return raw;
    }
    let snap = |from: f32, delta: f32| ((from + delta) / spacing).round() * spacing - from;
    Vector2D::new(snap(origin.x, raw.x), snap(origin.y, raw.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::DEFAULT_ORBIT_OFFSET;
    use std::f32::consts::{FRAC_1_SQRT_2, SQRT_2};

    /// Zooms spanning the range the camera clamps to, plus the crossover the
    /// core floor exists for.
    const ZOOMS: [f32; 9] = [0.1, 0.5, 0.9, 1.0, 2.0, 7.0 / 3.0, 3.0, 5.0, 10.0];

    /// A press anywhere inside the core the frame paints grabs it, at every
    /// zoom.
    ///
    /// The screen-pixel threshold and the world-fixed core cross at
    /// `ANCHOR_GRAB_THRESHOLD / core_half`, zoom 7/3 for the default 6 unit
    /// core. Above that an unfloored box is narrower than the dot on screen: at
    /// zoom 10 it would span 1.4 world units against the core's 6, so a press
    /// two thirds of the way from the centre to the core's edge would fall
    /// through to the canvas gesture behind it.
    #[test]
    fn the_core_grab_box_covers_the_painted_core() {
        let geometry = AnchorGeometry::default();
        for zoom in ZOOMS {
            let half = core_grab_half(geometry, zoom);
            assert!(
                half >= geometry.core_half(),
                "at zoom {zoom} the core grab box reaches {half} against a core \
                 reaching {}: a press inside the dot would open a selection box",
                geometry.core_half(),
            );
        }
    }

    /// The grab box stays clear of orbit 0 at every zoom and radius a host can
    /// produce, corner included.
    ///
    /// The two quantities scale OPPOSITELY: the box is a screen-pixel threshold
    /// divided by zoom, so it grows as the camera pulls back, while the ring is
    /// a world radius that does not. A box touching the ring would take every
    /// press meant for the innermost wrap, because the core is offered a press
    /// first.
    #[test]
    fn the_core_grab_box_never_reaches_orbit_zero() {
        for zoom in ZOOMS {
            for orbit_offset in [4.0, DEFAULT_ORBIT_OFFSET, 40.0] {
                let geometry = AnchorGeometry {
                    orbit_offset,
                    ..AnchorGeometry::default()
                };
                let corner = core_grab_half(geometry, zoom) * SQRT_2;
                assert!(
                    corner <= orbit_offset + 1e-4,
                    "at zoom {zoom} with orbit 0 at {orbit_offset} the core grab \
                     box reaches {corner}: a press on the innermost wrap would \
                     grab the core instead",
                );
            }
        }
    }

    /// Where the two clamps disagree the cap wins, so the innermost wrap keeps
    /// its press and the core loses some of its own reach.
    ///
    /// They can only disagree on a core styled wider than `sqrt(2)` times its
    /// orbit 0, which is a core already painted over its own innermost ring.
    #[test]
    fn the_orbit_cap_outranks_the_core_floor() {
        let geometry = AnchorGeometry {
            core_size: 40.0,
            ..AnchorGeometry::default()
        };
        assert!(geometry.core_half() > geometry.orbit_radius(0) * FRAC_1_SQRT_2);
        for zoom in ZOOMS {
            let half = core_grab_half(geometry, zoom);
            assert!(
                half * SQRT_2 <= geometry.orbit_radius(0) + 1e-4,
                "at zoom {zoom} a core overlapping its own orbit 0 pushed the \
                 grab box corner to {}, past the ring at {}",
                half * SQRT_2,
                geometry.orbit_radius(0),
            );
            assert!(
                half < geometry.core_half(),
                "at zoom {zoom} the floor at {} beat the cap",
                geometry.core_half(),
            );
        }
    }
}
