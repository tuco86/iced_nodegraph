//! State that survives between frames, owned by the widget's `tree::State`:
//! camera, drag, z-order and touch contacts.
//!
//! Everything here is keyed by *node index* - the node's position in the
//! `NodeGraph::nodes` vector for the current frame - not by the host's node id.
//! An index is a per-frame identity: the host owns node order, so a reorder
//! re-maps every index. Selection is not state this module owns; it is a
//! property of each [`Node`] and is read off the host's nodes every frame. The
//! only selection value kept here is `pending_selection`, the reported-but-not-
//! yet-applied working copy, guarded by `selection_baseline`.

use super::Easing;
use super::GraphInfo;
use super::camera::Camera2D;
use super::euclid::{IntoEuclid, LayoutPoint, WorldPoint};
use super::{DEFAULT_CORE_SIZE, DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING};
use crate::style::EdgeCurve;
use iced_widget::core::{Layout, Padding, Point, Size, keyboard, touch};
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

/// What the pointer is currently dragging, with the point captured at press.
///
/// The captured points live in two spaces, and which one a variant uses follows
/// the cursor its delta is taken against. Panning and anchor moves measure
/// against the raw screen cursor mapped through [`Camera2D::screen_to_world`],
/// so their captured points are [`WorldPoint`]s. Every other gesture measures
/// against the layout-absolute cursor the camera hands the child walk, so its
/// captured point is a [`LayoutPoint`]. The two spaces differ by the viewport
/// origin, so a variant's captured point has to share the space of the cursor
/// it is subtracted from or that origin survives into the delta.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Dragging {
    #[default]
    None,
    /// Panning the canvas (right mouse button).
    Graph(WorldPoint),
    /// Moving one unselected node.
    Node { node: usize, origin: LayoutPoint },
    /// Moving every selected node together.
    GroupMove(LayoutPoint),
    /// Resizing one node by its bottom-right grip. `start` is the node's
    /// content size at press: the reported size is `start + cursor delta`, so
    /// the drag stays exact even though the node itself never changes size
    /// until the host applies the report.
    Resize {
        node: usize,
        origin: LayoutPoint,
        start: Size,
    },
    /// A loose edge held at the cursor, anchored at its source pin.
    Edge {
        from_node: usize,
        from_pin: usize,
        origin: LayoutPoint,
    },
    /// A dragged edge snapped onto a compatible target pin. Releasing here keeps
    /// the connection.
    EdgeOver {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    /// Rubber-band selection: the press corner and the live corner.
    SelectionBox(LayoutPoint, LayoutPoint),
    /// Slicing across edges: the cursor trail and the edge indices it has
    /// crossed so far, cut on release.
    EdgeCutting {
        trail: Vec<LayoutPoint>,
        pending_cuts: HashSet<usize>,
    },
    /// Moving one anchor. `origin` is a [`WorldPoint`] because the preview
    /// subtracts it from the raw screen cursor mapped through
    /// [`Camera2D::screen_to_world`]; a layout-space origin would leave the
    /// viewport origin in the offset, and the preview would sit that far off
    /// the cursor.
    Anchor { anchor: usize, origin: WorldPoint },
    /// Re-routing one edge with a phantom anchor held at the cursor. `detached`
    /// is the anchor a wrap grab pulled off: hidden from the preview until the
    /// host applies the detach, and always snap-eligible so the drag can put it
    /// straight back.
    Route {
        edge: usize,
        detached: Option<usize>,
    },
    /// A route drag snapped onto an anchor. The attachment is already
    /// published, so releasing here commits nothing further.
    RouteOver {
        edge: usize,
        anchor: usize,
        detached: Option<usize>,
    },
    /// A pan-button press over something clickable, before it is known which
    /// it is: travel turns it into a pan seeded at the press point, a release
    /// without travel commits the click.
    PressPending {
        origin_world: WorldPoint,
        origin_screen: Point,
        target: PressTarget,
    },
}

/// What a [`Dragging::PressPending`] would click if it never travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PressTarget {
    AnchorCore { anchor: usize },
    Wrap { edge: usize, anchor: usize },
}

/// The geometry `draw` last resolved for one anchor: the core it fills and the
/// radii it lays cables tangent to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AnchorGeometry {
    /// Side length of the core, in world units. The core is centred on the
    /// anchor position, so it reaches `core_size / 2` in every direction.
    pub(super) core_size: f32,
    /// Radius of orbit 0.
    pub(super) orbit_offset: f32,
    /// Additional radius per orbit.
    pub(super) orbit_spacing: f32,
}

impl Default for AnchorGeometry {
    /// The same values `style::defaults` builds its own anchor style from, for
    /// anchors no frame has resolved a style for yet.
    fn default() -> Self {
        Self {
            core_size: DEFAULT_CORE_SIZE,
            orbit_offset: DEFAULT_ORBIT_OFFSET,
            orbit_spacing: DEFAULT_ORBIT_SPACING,
        }
    }
}

impl AnchorGeometry {
    /// How far the core reaches from the anchor position along either axis.
    pub(super) fn core_half(&self) -> f32 {
        self.core_size * 0.5
    }

    /// Radius of orbit `orbit`.
    pub(super) fn orbit_radius(&self, orbit: u8) -> f32 {
        self.orbit_offset + orbit as f32 * self.orbit_spacing
    }
}

#[derive(Debug)]
pub(super) struct NodeGraphState {
    pub(super) camera: Camera2D,
    pub(super) dragging: Dragging,
    pub(super) time: f32,
    pub(super) last_update: Option<Instant>,
    /// The selection the widget has reported but not yet seen applied, so a burst
    /// of clicks composes instead of each one starting from the host's stale
    /// value. Interaction and rendering both read this when it is set.
    pub(super) pending_selection: Option<HashSet<usize>>,
    /// The host selection `pending_selection` was derived from. When the host
    /// pushes anything else it has moved on - it may have applied our value, or
    /// set its own - and the pending value is dropped. Comparing against the host
    /// rather than against `pending_selection` is what keeps a stale host value
    /// from clobbering an interaction that has not round-tripped yet.
    pub(super) selection_baseline: Option<HashSet<usize>>,
    pub(super) modifiers: keyboard::Modifiers,
    /// Valid drop targets computed at edge drag start.
    /// Contains (node_index, pin_index) pairs that are valid connection targets.
    /// Only populated during Edge/EdgeOver dragging states.
    pub(super) valid_drop_targets: HashSet<(usize, usize)>,
    /// Per-anchor core size and orbit radii as last resolved in `draw`, indexed
    /// by anchor index. Anchors no frame has reached are absent, and readers
    /// fall back to [`AnchorGeometry::default`].
    ///
    /// These come off `AnchorStyle`, which needs the theme - and `update` has
    /// none - so the geometry travels one frame behind, like `last_info`. One
    /// frame behind is one frame STALE here, not merely late: `draw` resolves
    /// each anchor's style at that anchor's live `AnchorStatus`, and
    /// `core_size`/`orbit_offset`/`orbit_spacing` are public fields a host may
    /// set per status, so a host that widens a valid target's orbits moves
    /// these values for as long as the drag lasts, and the interaction path
    /// reads the frame before each change.
    pub(super) anchor_geometry: RefCell<Vec<AnchorGeometry>>,
    /// The [`EdgeCurve`] each edge was last drawn with, indexed by edge index.
    ///
    /// As long as the edge list of the frame that WROTE it, which need not be
    /// the frame reading it: the host owns the edge list, so a frame that
    /// pushes an edge reads a vector indexed for the previous one. That is what
    /// both readers' `.get(edge).copied().unwrap_or_default()` covers.
    ///
    /// Resolving an edge style needs the theme, and only `draw` has one, so the
    /// curve arrives here a frame late. Edges `draw` never reached - an
    /// endpoint pin that did not resolve - hold [`EdgeCurve::default`].
    pub(super) edge_curves: RefCell<Vec<EdgeCurve>>,
    /// Last host-provided view (`view()`) that we synced into `camera`. Lets us
    /// tell apart "host pushed a new camera" (sync needed) from "internal pan/zoom
    /// changed the camera but the matching `on_pan` has not yet round-tripped
    /// back into `view`" (syncing would clobber it). Selection needs no such
    /// guard: it is not state here at all, it travels on each [`Node`].
    pub(super) last_synced_view: Option<(Point, f32)>,
    /// In-flight camera tween started by `NodeGraph::focus` or a keymap
    /// frame action. `None` when the camera is not currently animating.
    pub(super) camera_tween: Option<CameraTween>,
    /// Last `seq` from `NodeGraph::focus` that was processed (fit performed
    /// or resolved to a no-op). Nonce dedup, mirroring `last_synced_view`.
    pub(super) last_focus_seq: Option<u64>,
    /// Timestamp of the last `RedrawRequested` event the focus tween advanced
    /// against, distinct from `last_update` (which times every animation).
    /// Two jobs: it derives the tween's delta from the redraw event's OWN
    /// timestamp, because iced dispatches non-redraw events in a separate
    /// pass whose delta would otherwise starve the tween; and an equal
    /// timestamp identifies a RE-ENTRANT pass of the same redraw cycle, which
    /// must neither advance the clock nor publish again.
    pub(super) last_redraw: Option<Instant>,
    /// Set during draw() when any SDF primitive has active animations.
    /// Read during update() to drive continuous redraws via shell.request_redraw().
    pub(super) sdf_animated: Cell<bool>,
    /// Whether the last cursor position was somewhere the hover feedback draws
    /// something.
    ///
    /// A hover has no state of its own - `draw` resolves it from the same
    /// geometry it strokes - but it does need a FRAME to be drawn in, and a
    /// cursor move over an idle graph asks for none. This is what says one is
    /// owed, including on the move that LEAVES the zone and has to clear it.
    pub(super) hover_zone: Cell<bool>,
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
            pending_selection: None,
            selection_baseline: None,
            modifiers: keyboard::Modifiers::default(),
            valid_drop_targets: HashSet::new(),
            anchor_geometry: RefCell::new(Vec::new()),
            edge_curves: RefCell::new(Vec::new()),
            last_synced_view: None,
            camera_tween: None,
            last_focus_seq: None,
            last_redraw: None,
            sdf_animated: Cell::new(false),
            hover_zone: Cell::new(false),
            last_info: RefCell::new(None),
            node_z: HashMap::new(),
            z_counter: 0,
            fingers: Vec::new(),
            touch_tap: None,
        }
    }
}

impl NodeGraphState {
    /// The camera anchored to this frame's widget origin, which is where all
    /// screen/world conversion for the graph starts. The stored camera holds
    /// only pan and zoom; the widget's screen position is a layout fact that
    /// changes without any interaction, so it is folded in per frame rather
    /// than kept in state.
    pub(super) fn camera_for(&self, layout: Layout<'_>) -> Camera2D {
        self.camera
            .with_viewport_origin(layout.bounds().position().into_euclid().to_vector())
    }

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
///
/// `is_selected` reads the flag off the host's [`Node`], since selection is not
/// state this module owns.
pub(super) fn z_render_indices(
    state: &NodeGraphState,
    node_count: usize,
    is_selected: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..node_count).collect();
    indices.sort_by_key(|&i| {
        let z = state.node_z.get(&i).copied().unwrap_or(0);
        (is_selected(i), z)
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
        let world = WorldPoint::new(10.0, 20.0);
        let origin = LayoutPoint::new(10.0, 20.0);

        assert_ne!(Dragging::None, Dragging::Graph(world));
        assert_ne!(Dragging::Graph(world), Dragging::Node { node: 0, origin });
        assert_ne!(
            Dragging::Node { node: 0, origin },
            Dragging::Edge {
                from_node: 0,
                from_pin: 0,
                origin
            }
        );
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
        let dragging = Dragging::Node { node: 5, origin };

        if let Dragging::Node {
            node: idx,
            origin: stored,
        } = dragging
        {
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
        let dragging = Dragging::Edge {
            from_node: 2,
            from_pin: 1,
            origin: cursor,
        };

        if let Dragging::Edge {
            from_node: node,
            from_pin: pin,
            origin: stored,
        } = dragging
        {
            assert_eq!(node, 2);
            assert_eq!(pin, 1);
            assert_eq!(stored.x, 300.0);
            assert_eq!(stored.y, 400.0);
        } else {
            panic!("Expected Dragging::Edge");
        }
    }

    #[test]
    fn selection_box_stores_two_points() {
        let start = Point2D::new(0.0, 0.0);
        let current = Point2D::new(100.0, 100.0);
        let dragging = Dragging::SelectionBox(start, current);

        if let Dragging::SelectionBox(s, c) = dragging {
            assert_eq!(s.x, 0.0);
            assert_eq!(s.y, 0.0);
            assert_eq!(c.x, 100.0);
            assert_eq!(c.y, 100.0);
        } else {
            panic!("Expected Dragging::SelectionBox");
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
    fn test_node_graph_state_default() {
        let state = NodeGraphState::default();

        assert_eq!(state.dragging, Dragging::None);
        assert_eq!(state.time, 0.0);
        assert!(state.last_update.is_none());
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
        let order = z_render_indices(&state, 4, |i| i == 3);

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
        // 2 is more recently assigned z, so it should render on top of 0.
        let order = z_render_indices(&state, 3, |i| i == 0 || i == 2);

        // 1 (unselected) first, then 0 and 2 (selected, with 2 on top).
        assert_eq!(order, vec![1, 0, 2]);
    }
}
