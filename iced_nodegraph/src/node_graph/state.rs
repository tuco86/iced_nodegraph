//! Internal state management for the node graph widget.
//!
//! This module contains the persistent state that lives between frames:
//! - Camera position and zoom
//! - Current drag operation (node, edge, selection box, etc.)
//! - Animation timing
//! - Selection state
//! - Keyboard modifier tracking

use super::Easing;
use super::GraphInfo;
use super::camera::Camera2D;
use super::euclid::WorldPoint;
use iced::{Padding, Point, Size, keyboard, touch};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use web_time::Instant;

/// In-flight camera animation started by
/// [`NodeGraph::focus`](super::NodeGraph::focus) or a keymap frame action
/// (`Home`/`f`), advanced once per `RedrawRequested` frame in `update()`.
/// Center-based interpolation with geometric zoom; `position` is
/// recomputed each frame from `center`/`zoom` via
/// [`Camera2D::position_for_center`], using the `viewport`/`padding` frozen
/// at tween start, so the focused content stays centered throughout.
///
/// Arbitration: user input aborts a running tween; the tween in turn
/// suppresses the routine `view()` sync while it runs, except for an
/// explicit app override that pushes a `view()` differing from the
/// tween's own last emission (see the `view()`-sync block in `update.rs`).
#[derive(Debug, Clone, Copy)]
pub(super) struct CameraTween {
    pub(super) start_center: WorldPoint,
    pub(super) start_zoom: f32,
    pub(super) end_center: WorldPoint,
    pub(super) end_zoom: f32,
    /// Viewport size frozen at tween start.
    pub(super) viewport: Size,
    /// Padding frozen at tween start.
    pub(super) padding: Padding,
    pub(super) elapsed: f32,
    pub(super) duration: f32,
    pub(super) easing: Easing,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Dragging {
    #[default]
    None,
    Graph(WorldPoint),                    // cursor origin (right mouse button pan)
    Node(usize, WorldPoint),              // node id and cursor origin
    Edge(usize, usize, WorldPoint),       // from_node and from_pin and cursor origin
    EdgeOver(usize, usize, usize, usize), // from_node, from_pin, to_node and to_pin
    BoxSelect(WorldPoint, WorldPoint),    // start point, current point (left mouse on empty space)
    GroupMove(WorldPoint),                // origin point (when dragging a selected node, all move)
    /// Fruit Ninja edge cutting: trail of cursor positions and pending edges to cut
    EdgeCutting {
        trail: Vec<WorldPoint>,
        pending_cuts: HashSet<usize>,
    },
}

#[derive(Debug)]
pub(super) struct NodeGraphState {
    pub(super) camera: Camera2D,
    pub(super) dragging: Dragging,
    pub(super) time: f32,
    pub(super) last_update: Option<Instant>,
    pub(super) selected_nodes: HashSet<usize>,
    /// Last externally-provided selection (via `NodeGraph::selection()`) that
    /// we synced into `selected_nodes`. Lets us tell apart "host pushed a new
    /// selection" (sync needed) from "internal box-select just changed state
    /// but the host has not yet seen the on_select message" (sync would clobber
    /// the new state with the still-stale external value).
    pub(super) last_synced_external: Option<HashSet<usize>>,
    pub(super) modifiers: keyboard::Modifiers,
    /// Valid drop targets computed at edge drag start.
    /// Contains (node_index, pin_index) pairs that are valid connection targets.
    /// Only populated during Edge/EdgeOver dragging states.
    pub(super) valid_drop_targets: HashSet<(usize, usize)>,
    /// Last host-provided view (`view()`) that we synced into `camera`. Lets us
    /// tell apart "host pushed a new camera" (sync needed) from "internal pan/zoom
    /// changed the camera but the matching `on_pan` has not yet round-tripped
    /// back into `view`" (syncing would clobber it). Mirrors
    /// `last_synced_external` for selection.
    pub(super) last_synced_view: Option<(Point, f32)>,
    /// In-flight camera tween started by `NodeGraph::focus` or a keymap
    /// frame action. `None` when the camera is not currently animating.
    pub(super) camera_tween: Option<CameraTween>,
    /// Last `seq` from `NodeGraph::focus` that was processed (fit performed
    /// or resolved to a no-op). Nonce dedup, mirroring `last_synced_view`.
    pub(super) last_focus_seq: Option<u64>,
    /// Set during draw() when any SDF primitive has active animations.
    /// Read during update() to drive continuous redraws via shell.request_redraw().
    pub(super) sdf_animated: Cell<bool>,
    /// Latest per-frame diagnostics, written during draw() and taken during
    /// update() to publish via the `on_info` callback (one frame behind).
    pub(super) last_info: RefCell<Option<GraphInfo>>,
    /// Per-node z-order timestamp. Higher = more recently moved (or newly added).
    /// Indexed by internal node index. Newly seen indices are auto-assigned the
    /// next counter value so freshly pushed nodes spawn on top of older ones.
    pub(super) node_z: HashMap<usize, u64>,
    /// Monotonic counter that feeds into `node_z`. Bumped on move release and
    /// on first sight of a new node index.
    pub(super) z_counter: u64,
    /// Currently pressed touch contacts in press order (screen positions).
    /// The first entry is the "primary" finger that emulates the left mouse
    /// button; the first two entries drive the pinch gesture.
    pub(super) fingers: Vec<(touch::Finger, Point)>,
    /// Tap candidate: (finger, press position, press time from `time`).
    /// Cleared when the finger travels or a second finger joins.
    pub(super) touch_tap: Option<(touch::Finger, Point, f32)>,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self {
            camera: Camera2D::new(),
            dragging: Default::default(),
            time: 0.0,
            last_update: None,
            selected_nodes: HashSet::new(),
            last_synced_external: None,
            modifiers: keyboard::Modifiers::default(),
            valid_drop_targets: HashSet::new(),
            last_synced_view: None,
            camera_tween: None,
            last_focus_seq: None,
            sdf_animated: Cell::new(false),
            last_info: RefCell::new(None),
            node_z: HashMap::new(),
            z_counter: 0,
            fingers: Vec::new(),
            touch_tap: None,
        }
    }
}

impl NodeGraphState {
    /// Ensure every index in `0..node_count` has a z entry. Newly seen indices
    /// receive the next counter value, so freshly pushed nodes render on top.
    pub(super) fn ensure_z_entries(&mut self, node_count: usize) {
        for idx in 0..node_count {
            if let std::collections::hash_map::Entry::Vacant(e) = self.node_z.entry(idx) {
                e.insert(self.z_counter);
                self.z_counter = self.z_counter.wrapping_add(1);
            }
        }
    }

    /// Promote a single node to the top of the z-order.
    pub(super) fn promote_z(&mut self, idx: usize) {
        self.node_z.insert(idx, self.z_counter);
        self.z_counter = self.z_counter.wrapping_add(1);
    }

    /// Promote a group of nodes to the top, preserving their relative order.
    pub(super) fn promote_z_many(&mut self, indices: &[usize]) {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_by_key(|i| self.node_z.get(i).copied().unwrap_or(0));
        for idx in sorted {
            self.promote_z(idx);
        }
    }
}

/// Returns node indices in render order (back to front).
/// Unselected nodes by z ascending, then selected nodes by z ascending.
/// Reverse this iterator for top-first hit-test / event propagation.
pub(super) fn z_render_indices(state: &NodeGraphState, node_count: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..node_count).collect();
    indices.sort_by_key(|&i| {
        let selected = state.selected_nodes.contains(&i);
        let z = state.node_z.get(&i).copied().unwrap_or(0);
        (selected, z)
    });
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::Point2D;

    #[test]
    fn test_dragging_default_is_none() {
        let dragging: Dragging = Default::default();
        assert_eq!(dragging, Dragging::None);
    }

    #[test]
    fn test_dragging_states_not_equal() {
        let origin = Point2D::new(10.0, 20.0);

        assert_ne!(Dragging::None, Dragging::Graph(origin));
        assert_ne!(Dragging::Graph(origin), Dragging::Node(0, origin));
        assert_ne!(Dragging::Node(0, origin), Dragging::Edge(0, 0, origin));
    }

    #[test]
    fn test_dragging_graph_stores_origin() {
        let origin = Point2D::new(100.0, 200.0);
        let dragging = Dragging::Graph(origin);

        if let Dragging::Graph(stored) = dragging {
            assert_eq!(stored.x, 100.0);
            assert_eq!(stored.y, 200.0);
        } else {
            panic!("Expected Dragging::Graph");
        }
    }

    #[test]
    fn test_dragging_node_stores_index_and_origin() {
        let origin = Point2D::new(50.0, 75.0);
        let dragging = Dragging::Node(5, origin);

        if let Dragging::Node(idx, stored) = dragging {
            assert_eq!(idx, 5);
            assert_eq!(stored.x, 50.0);
            assert_eq!(stored.y, 75.0);
        } else {
            panic!("Expected Dragging::Node");
        }
    }

    #[test]
    fn test_dragging_edge_stores_node_pin_and_cursor() {
        let cursor = Point2D::new(300.0, 400.0);
        let dragging = Dragging::Edge(2, 1, cursor);

        if let Dragging::Edge(node, pin, stored) = dragging {
            assert_eq!(node, 2);
            assert_eq!(pin, 1);
            assert_eq!(stored.x, 300.0);
            assert_eq!(stored.y, 400.0);
        } else {
            panic!("Expected Dragging::Edge");
        }
    }

    #[test]
    fn test_box_select_stores_two_points() {
        let start = Point2D::new(0.0, 0.0);
        let current = Point2D::new(100.0, 100.0);
        let dragging = Dragging::BoxSelect(start, current);

        if let Dragging::BoxSelect(s, c) = dragging {
            assert_eq!(s.x, 0.0);
            assert_eq!(s.y, 0.0);
            assert_eq!(c.x, 100.0);
            assert_eq!(c.y, 100.0);
        } else {
            panic!("Expected Dragging::BoxSelect");
        }
    }

    #[test]
    fn test_group_move_stores_origin() {
        let origin = Point2D::new(250.0, 350.0);
        let dragging = Dragging::GroupMove(origin);

        if let Dragging::GroupMove(stored) = dragging {
            assert_eq!(stored.x, 250.0);
            assert_eq!(stored.y, 350.0);
        } else {
            panic!("Expected Dragging::GroupMove");
        }
    }

    #[test]
    fn test_edge_cutting_trail() {
        let trail = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(20.0, 20.0),
        ];
        let mut pending_cuts = HashSet::new();
        pending_cuts.insert(1);
        pending_cuts.insert(3);
        let dragging = Dragging::EdgeCutting {
            trail: trail.clone(),
            pending_cuts: pending_cuts.clone(),
        };

        if let Dragging::EdgeCutting {
            trail: stored,
            pending_cuts: cuts,
        } = dragging
        {
            assert_eq!(stored.len(), 3);
            assert_eq!(stored[0].x, 0.0);
            assert_eq!(stored[2].x, 20.0);
            assert!(cuts.contains(&1));
            assert!(cuts.contains(&3));
            assert!(!cuts.contains(&2));
        } else {
            panic!("Expected Dragging::EdgeCutting");
        }
    }

    #[test]
    fn test_selection_set_operations() {
        let mut state = NodeGraphState::default();

        // Start empty
        assert!(state.selected_nodes.is_empty());

        // Add nodes
        state.selected_nodes.insert(0);
        state.selected_nodes.insert(2);
        state.selected_nodes.insert(5);

        assert_eq!(state.selected_nodes.len(), 3);
        assert!(state.selected_nodes.contains(&0));
        assert!(state.selected_nodes.contains(&2));
        assert!(state.selected_nodes.contains(&5));
        assert!(!state.selected_nodes.contains(&1));

        // Remove node
        state.selected_nodes.remove(&2);
        assert_eq!(state.selected_nodes.len(), 2);
        assert!(!state.selected_nodes.contains(&2));

        // Clear all
        state.selected_nodes.clear();
        assert!(state.selected_nodes.is_empty());
    }

    #[test]
    fn test_node_graph_state_default() {
        let state = NodeGraphState::default();

        assert_eq!(state.dragging, Dragging::None);
        assert_eq!(state.time, 0.0);
        assert!(state.last_update.is_none());
        assert!(state.selected_nodes.is_empty());
        assert!(state.valid_drop_targets.is_empty());
        assert!(state.node_z.is_empty());
        assert_eq!(state.z_counter, 0);
    }

    #[test]
    fn test_ensure_z_entries_assigns_new_indices() {
        let mut state = NodeGraphState::default();
        state.ensure_z_entries(3);

        assert_eq!(state.node_z.get(&0), Some(&0));
        assert_eq!(state.node_z.get(&1), Some(&1));
        assert_eq!(state.node_z.get(&2), Some(&2));
        assert_eq!(state.z_counter, 3);

        // Re-running with same count does not bump existing entries.
        state.ensure_z_entries(3);
        assert_eq!(state.z_counter, 3);

        // Growing assigns higher z to new indices (so freshly pushed nodes go on top).
        state.ensure_z_entries(5);
        assert_eq!(state.node_z.get(&3), Some(&3));
        assert_eq!(state.node_z.get(&4), Some(&4));
    }

    #[test]
    fn test_promote_z_puts_node_on_top() {
        let mut state = NodeGraphState::default();
        state.ensure_z_entries(3);

        state.promote_z(0);
        // 0 should now have the highest z.
        let z0 = state.node_z[&0];
        let z1 = state.node_z[&1];
        let z2 = state.node_z[&2];
        assert!(z0 > z1);
        assert!(z0 > z2);
    }

    #[test]
    fn test_promote_z_many_preserves_relative_order() {
        let mut state = NodeGraphState::default();
        state.ensure_z_entries(4);
        // Initial z: 0=0, 1=1, 2=2, 3=3

        // Promote {0, 2}: 2 was higher than 0 before, so after promotion 2 must still be higher.
        state.promote_z_many(&[0, 2]);
        assert!(state.node_z[&0] > state.node_z[&1]);
        assert!(state.node_z[&0] > state.node_z[&3]);
        assert!(state.node_z[&2] > state.node_z[&0]);
    }

    #[test]
    fn test_z_render_indices_unselected_then_selected() {
        let mut state = NodeGraphState::default();
        state.ensure_z_entries(4);

        // Make 1 most recently moved among unselected.
        state.promote_z(1);
        // Select 3.
        state.selected_nodes.insert(3);

        let order = z_render_indices(&state, 4);

        // Selected goes last (on top). 3 must be at the end.
        assert_eq!(order.last(), Some(&3));
        // Among unselected (0, 2, 1), 1 has highest z, so it must come just
        // before the selected block.
        let one_pos = order.iter().position(|&i| i == 1).unwrap();
        assert_eq!(one_pos, 2);
    }

    #[test]
    fn test_z_render_indices_selected_sorted_by_z() {
        let mut state = NodeGraphState::default();
        state.ensure_z_entries(3);
        state.selected_nodes.insert(0);
        state.selected_nodes.insert(2);
        // 2 is more recently assigned z, so it should render on top of 0.

        let order = z_render_indices(&state, 3);

        // 1 (unselected) first, then 0 and 2 (selected, with 2 on top).
        assert_eq!(order, vec![1, 0, 2]);
    }
}
