//! High-level interaction tests driving NodeGraph through `iced_test::Simulator`.
//!
//! Unlike the recording-renderer tests in `tests/coordinates.rs` and
//! `tests/clipping.rs` (which assert on render geometry via a fake
//! renderer), these tests exercise the widget end-to-end through the real iced
//! event pipeline: layout -> update -> message emission. These tests validate
//! interaction logic and the Messages the event callbacks publish; the one
//! snapshot test additionally rasterizes (see its backend note).
//!
//! Coordinate model: the graph fills the 1024x768 root with the default camera
//! (zoom 1, no pan, origin (0,0)), so world coordinates equal screen pixels.
//! A node pushed at world `p` with content size `w x h` has a body spanning
//! `p .. p + (w, h)`.

use iced::widget::{container, text};
use iced::{Element, Length, Point, Size, Theme, Vector};
use iced::{keyboard, mouse};
use iced_nodegraph::{
    AnchorStatus, DragInfo, Ids, Minimap, NodeGraph, PinRef, anchor, default_anchor_style, edge,
    node, pin,
};
use iced_test::Simulator;

/// The vocabulary every scene here shares: `usize` ids throughout, so an
/// edge can be named by `on_edge_delete` and a message enum serves both the
/// plain and the routed scenes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SimIds;

impl Ids for SimIds {
    type NodeId = usize;
    type PinId = usize;
    type EdgeId = usize;
    type AnchorId = usize;
    type Payload = ();
}

type Renderer = iced::Renderer;
type Graph = NodeGraph<'static, SimIds, Msg, Theme, Renderer>;
type Pin = PinRef<SimIds>;

/// Captures every interaction callback the graph can emit.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(Vec<usize>),
    Move(Vector, Vec<usize>),
    Resize(usize, Size),
    Clone(Vec<usize>),
    Delete(Vec<usize>),
    Connect(Pin, Pin),
    Disconnect(Pin, Pin),
    Camera(Point, f32),
    DragStart(DragInfo<SimIds>),
    DragUpdate(Point),
    DragEnd,
    Button,
    Input(String),
    EdgeDelete(Vec<usize>),
    AnchorMove(usize, Point),
    AnchorCreated(usize, Point),
    RouteAttached(usize, usize),
    RouteDetached(usize, usize),
    AnchorDeleted(usize),
}

const NODE_W: f32 = 60.0;
const NODE_H: f32 = 30.0;

/// Builds a graph with one fixed-size node body per `(id, world-position)`,
/// every interaction callback wired into `Msg`.
fn graph_with(nodes: &[(usize, Point)]) -> Element<'static, Msg, Theme, Renderer> {
    graph_with_selected(nodes, &[])
}

/// Like [`graph_with`], but the host marks `selected` ids - standing in for a
/// frame where it has already applied a reported selection.
fn graph_with_selected(
    nodes: &[(usize, Point)],
    selected: &[usize],
) -> Element<'static, Msg, Theme, Renderer> {
    graph_of(nodes, selected, false).into()
}

/// Like [`graph_with`], but every node carries a resize grip.
fn resizable_graph(nodes: &[(usize, Point)]) -> Element<'static, Msg, Theme, Renderer> {
    graph_of(nodes, &[], true).into()
}

/// Like [`graph_with_selected`], but node drags snap to a [`GRID`]-wide world
/// grid.
fn snap_graph(
    nodes: &[(usize, Point)],
    selected: &[usize],
) -> Element<'static, Msg, Theme, Renderer> {
    graph_of(nodes, selected, false).snap_grid(GRID).into()
}

fn graph_of(nodes: &[(usize, Point)], selected: &[usize], resizable: bool) -> Graph {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select)
        .on_move(Msg::Move)
        .on_resize(Msg::Resize)
        .on_clone(Msg::Clone)
        .on_delete(Msg::Delete)
        .on_drag_start(Msg::DragStart)
        .on_drag_update(Msg::DragUpdate)
        .on_drag_end(|| Msg::DragEnd);
    for &(id, pos) in nodes {
        let body = container(iced::widget::text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H));
        ng = ng.push_node(
            node(id, pos, body)
                .selected(selected.contains(&id))
                .resizable(resizable),
        );
    }
    ng
}

/// Screen center of a node body whose top-left world position is `p`.
fn center(p: Point) -> Point {
    Point::new(p.x + NODE_W / 2.0, p.y + NODE_H / 2.0)
}

fn moved(p: Point) -> iced::Event {
    iced::Event::Mouse(mouse::Event::CursorMoved { position: p })
}
fn press() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}
fn release() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

/// A full left-button drag from `from` to `to` (press, move, release).
fn drag(ui: &mut Simulator<'_, Msg, Theme, Renderer>, from: Point, to: Point) {
    ui.point_at(from);
    ui.simulate([moved(from), press()]);
    ui.point_at(to);
    ui.simulate([moved(to), release()]);
}

/// A left click at `at` (press and release in place).
fn click(ui: &mut Simulator<'_, Msg, Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate([moved(at), press(), release()]);
}

/// A key press carrying `modifiers` (Simulator's `tap_key` cannot set them).
fn key_pressed(key: keyboard::Key, modifiers: keyboard::Modifiers) -> iced::Event {
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// Mirrors iced's `Modifiers::command()`: Cmd on macOS, Ctrl elsewhere. The
/// graph's shortcuts gate on `command()`, so tests must send the platform's
/// command modifier or they pass on one OS and fail on the other.
fn cmd() -> keyboard::Modifiers {
    #[cfg(target_os = "macos")]
    {
        keyboard::Modifiers::LOGO
    }
    #[cfg(not(target_os = "macos"))]
    {
        keyboard::Modifiers::CTRL
    }
}

fn messages(ui: Simulator<'_, Msg, Theme, Renderer>) -> Vec<Msg> {
    ui.into_messages().collect()
}

/// Selection order comes from a HashSet, so normalize before comparing.
fn sorted(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v
}

/// Last selection the graph reported, sorted.
fn last_selection(msgs: &[Msg]) -> Option<Vec<usize>> {
    msgs.iter().rev().find_map(|m| match m {
        Msg::Select(ids) => Some(sorted(ids.clone())),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

#[test]
fn click_selects_node() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    assert_eq!(last_selection(&messages(ui)), Some(vec![0]));
}

#[test]
fn click_unselected_node_replaces_selection() {
    let mut ui = Simulator::new(graph_with(&[
        (0, Point::new(100.0, 100.0)),
        (1, Point::new(400.0, 100.0)),
    ]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    click(&mut ui, center(Point::new(400.0, 100.0)));
    // Plain click on a different node clears the old selection.
    assert_eq!(last_selection(&messages(ui)), Some(vec![1]));
}

#[test]
fn shift_click_adds_to_selection() {
    // Node 0 already selected by the host; shift-click must extend, not replace.
    let mut ui = Simulator::new(graph_with_selected(
        &[(0, Point::new(100.0, 100.0)), (1, Point::new(400.0, 100.0))],
        &[0],
    ));

    let shift = keyboard::Modifiers::SHIFT;
    let a = center(Point::new(400.0, 100.0));
    ui.point_at(a);
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        shift,
    ))]);
    ui.simulate([moved(a), press(), release()]);

    assert_eq!(last_selection(&messages(ui)), Some(vec![0, 1]));
}

#[test]
fn click_empty_space_clears_selection() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    // Press+release far from any node is an empty selection box -> clears.
    click(&mut ui, Point::new(700.0, 600.0));
    assert_eq!(last_selection(&messages(ui)), Some(vec![]));
}

#[test]
fn ctrl_a_selects_all() {
    let mut ui = Simulator::new(graph_with(&[
        (0, Point::new(100.0, 100.0)),
        (1, Point::new(400.0, 100.0)),
        (2, Point::new(700.0, 100.0)),
    ]));
    ui.point_at(Point::new(500.0, 400.0));
    ui.simulate([key_pressed(keyboard::Key::Character("a".into()), cmd())]);
    assert_eq!(last_selection(&messages(ui)), Some(vec![0, 1, 2]));
}

#[test]
fn escape_clears_selection() {
    let mut ui = Simulator::new(graph_with_selected(&[(0, Point::new(100.0, 100.0))], &[0]));
    ui.point_at(center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::default(),
    )]);
    assert_eq!(last_selection(&messages(ui)), Some(vec![]));
}

#[test]
fn selection_box_grabs_enclosed_nodes() {
    let mut ui = Simulator::new(graph_with(&[
        (0, Point::new(100.0, 100.0)),
        (1, Point::new(300.0, 100.0)),
        (2, Point::new(700.0, 500.0)), // outside the box
    ]));
    // Drag a box over nodes 0 and 1 only, starting on empty space.
    drag(&mut ui, Point::new(50.0, 50.0), Point::new(400.0, 200.0));
    assert_eq!(last_selection(&messages(ui)), Some(vec![0, 1]));
}

// ---------------------------------------------------------------------------
// Movement
// ---------------------------------------------------------------------------

#[test]
fn drag_node_emits_move_with_delta() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(graph_with(&[(0, start)]));
    // Drag the body center by (+50, +20).
    drag(
        &mut ui,
        center(start),
        center(start) + Vector::new(50.0, 20.0),
    );

    let msgs = messages(ui);
    let moved = msgs.iter().find_map(|m| match m {
        Msg::Move(delta, ids) => Some((*delta, sorted(ids.clone()))),
        _ => None,
    });
    let (delta, ids) = moved.expect("dragging a node must emit Move");
    assert_eq!(ids, vec![0]);
    assert!(
        (delta.x - 50.0).abs() < 0.5 && (delta.y - 20.0).abs() < 0.5,
        "node should move by (50, 20), got {delta:?}",
    );
}

#[test]
fn group_move_emits_move_with_delta_and_all_ids() {
    // Both selected by the host; dragging one reports the whole group.
    let mut ui = Simulator::new(graph_with_selected(
        &[(0, Point::new(100.0, 100.0)), (1, Point::new(400.0, 100.0))],
        &[0, 1],
    ));
    let from = center(Point::new(100.0, 100.0));
    drag(&mut ui, from, from + Vector::new(30.0, -10.0));

    let msgs = messages(ui);
    let group = msgs.iter().find_map(|m| match m {
        Msg::Move(delta, ids) => Some((*delta, sorted(ids.clone()))),
        _ => None,
    });
    let (delta, ids) = group.expect("dragging a multi-selection must emit Move");
    assert_eq!(ids, vec![0, 1]);
    assert!(
        (delta.x - 30.0).abs() < 0.5 && (delta.y + 10.0).abs() < 0.5,
        "group delta should be (30, -10), got {delta:?}",
    );
}

// ---------------------------------------------------------------------------
// Grid snap
// ---------------------------------------------------------------------------

/// Grid spacing [`snap_graph`] uses. The scenes below place nodes off the grid
/// on purpose: a node already on it would pass every snap assertion by
/// accident.
const GRID: f32 = 40.0;

/// The delta and the ids of the first `Move` in `msgs`.
fn first_move(msgs: &[Msg]) -> (Vector, Vec<usize>) {
    msgs.iter()
        .find_map(|m| match m {
            Msg::Move(delta, ids) => Some((*delta, sorted(ids.clone()))),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a Move, got {msgs:?}"))
}

#[test]
fn snapped_drag_reports_the_delta_that_lands_on_the_grid() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(snap_graph(&[(0, start)], &[]));
    // (100,100) + (50,20) = (150,120): the nearest grid origin is (160,120).
    drag(
        &mut ui,
        center(start),
        center(start) + Vector::new(50.0, 20.0),
    );

    let (delta, ids) = first_move(&messages(ui));
    assert_eq!(ids, vec![0]);
    assert_eq!((delta.x, delta.y), (60.0, 20.0), "got {delta:?}");
}

#[test]
fn snap_override_modifier_reports_the_raw_delta() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(snap_graph(&[(0, start)], &[]));
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::ALT,
    ))]);
    drag(
        &mut ui,
        center(start),
        center(start) + Vector::new(50.0, 20.0),
    );

    let (delta, ids) = first_move(&messages(ui));
    assert_eq!(ids, vec![0]);
    assert_eq!((delta.x, delta.y), (50.0, 20.0), "got {delta:?}");
}

#[test]
fn snapped_group_move_shares_the_grabbed_node_delta() {
    // Only the grabbed node lands on the grid; its partner keeps the same
    // relative offset, which is what one shared delta means.
    let (a, b) = (Point::new(100.0, 100.0), Point::new(413.0, 100.0));
    let mut ui = Simulator::new(snap_graph(&[(0, a), (1, b)], &[0, 1]));
    drag(&mut ui, center(a), center(a) + Vector::new(50.0, 20.0));

    let (delta, ids) = first_move(&messages(ui));
    assert_eq!(ids, vec![0, 1]);
    assert_eq!((delta.x, delta.y), (60.0, 20.0), "got {delta:?}");
}

/// The report is the anchor's snapped position, not the cursor's: the
/// anchor starts off the grid and the raw drop point (142, 117) is off it too.
#[test]
fn anchor_drag_lands_on_the_snap_grid() {
    let at = Point::new(105.0, 105.0);
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .snap_grid(GRID)
        .on_anchor_move(Msg::AnchorMove);
    ng = ng.push_anchor(anchor(ANCHOR_A, at));
    let mut ui = Simulator::new(Element::from(ng));
    drag(&mut ui, at, at + Vector::new(37.0, 12.0));

    assert_eq!(
        messages(ui),
        vec![Msg::AnchorMove(ANCHOR_A, Point::new(160.0, 120.0))]
    );
}

/// The far corner lands on the grid, not the size: a node whose origin is off
/// the grid still gets on-grid right and bottom edges.
#[test]
fn resize_snaps_the_far_corner() {
    let start = Point::new(10.0, 10.0);
    let mut ui = Simulator::new(Element::from(
        graph_of(&[(0, start)], &[], true).snap_grid(GRID),
    ));
    // Raw size (167, 93) puts the far corner at (177, 103); the nearest grid
    // point is (160, 120).
    drag(&mut ui, grip(start), grip(start) + Vector::new(107.0, 63.0));

    let (id, size) = last_resize(&messages(ui)).expect("dragging the grip must emit Resize");
    assert_eq!(id, 0);
    assert_eq!(size, Size::new(150.0, 110.0));
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

const FRAME_ID: usize = 3;
const INSIDE_ID: usize = 1;
const OUTSIDE_ID: usize = 2;
const FRAME_SIZE: Size = Size::new(400.0, 300.0);

/// A 400x300 frame at the origin with one node inside it and one outside,
/// the frame pushed LAST so it also carries the highest z: a press only reaches
/// it where no node covers the point.
fn frame_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng = graph_of(
        &[
            (INSIDE_ID, Point::new(50.0, 50.0)),
            (OUTSIDE_ID, Point::new(600.0, 600.0)),
        ],
        &[],
        false,
    );
    let body = container(text("frame"))
        .width(Length::Fixed(FRAME_SIZE.width))
        .height(Length::Fixed(FRAME_SIZE.height));
    ng = ng.push_node(node(FRAME_ID, Point::ORIGIN, body).frame());
    ng.into()
}

#[test]
fn dragging_a_frame_carries_the_nodes_inside_it() {
    let mut ui = Simulator::new(frame_graph());
    // (300,250) is inside the frame and clear of the node at (50,50).
    let empty_area = Point::new(300.0, 250.0);
    drag(&mut ui, empty_area, empty_area + Vector::new(30.0, 10.0));

    let (delta, ids) = first_move(&messages(ui));
    assert_eq!(
        ids,
        vec![INSIDE_ID, FRAME_ID],
        "the frame must carry the node inside it and leave the one outside",
    );
    assert_eq!((delta.x, delta.y), (30.0, 10.0), "got {delta:?}");
}

#[test]
fn pressing_a_node_inside_a_frame_drags_only_that_node() {
    let mut ui = Simulator::new(frame_graph());
    let from = center(Point::new(50.0, 50.0));
    drag(&mut ui, from, from + Vector::new(30.0, 10.0));

    let (delta, ids) = first_move(&messages(ui));
    assert_eq!(
        ids,
        vec![INSIDE_ID],
        "the node over the frame must take the press for itself",
    );
    assert_eq!((delta.x, delta.y), (30.0, 10.0), "got {delta:?}");
}

// ---------------------------------------------------------------------------
// Keyboard commands
// ---------------------------------------------------------------------------

#[test]
fn delete_key_requests_delete_of_selection() {
    let mut ui = Simulator::new(graph_with_selected(&[(0, Point::new(100.0, 100.0))], &[0]));
    ui.point_at(center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Delete),
        keyboard::Modifiers::default(),
    )]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Delete(vec![0])),
        "Delete key must request deletion of the selection: {msgs:?}",
    );
}

/// Selection travels on the node, so it cannot be lost to builder order the way
/// a graph-level setter resolved against a not-yet-filled node list could be.
/// Marking the node is enough - no call ordering, no id-to-index step.
#[test]
fn a_node_marked_selected_is_acted_on_without_any_prior_interaction() {
    let mut ui = Simulator::new(graph_with_selected(&[(7, Point::new(100.0, 100.0))], &[7]));
    ui.point_at(center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Delete),
        keyboard::Modifiers::default(),
    )]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Delete(vec![7])),
        "a host-marked selection must be live on the first frame: {msgs:?}",
    );
}

/// A burst of clicks composes even though the host has not applied any of them
/// yet: the widget holds what it reported until the host's own value moves on.
/// Without that, the second shift-click would start from the stale (empty) host
/// selection and replace the first instead of extending it.
#[test]
fn a_second_click_composes_with_one_the_host_has_not_applied_yet() {
    let (a, b) = (Point::new(100.0, 100.0), Point::new(400.0, 100.0));
    let mut ui = Simulator::new(graph_with(&[(0, a), (1, b)]));
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::SHIFT,
    ))]);
    ui.point_at(center(a));
    ui.simulate([moved(center(a)), press(), release()]);
    ui.point_at(center(b));
    ui.simulate([moved(center(b)), press(), release()]);

    let msgs = messages(ui);
    assert_eq!(
        last_selection(&msgs),
        Some(vec![0, 1]),
        "the second shift-click must extend the first, not replace it: {msgs:?}",
    );
}

/// The host's value is final: when it reports something else, the held value is
/// dropped rather than fighting it.
#[test]
fn a_host_selection_overrides_what_the_widget_reported() {
    let (a, b) = (Point::new(100.0, 100.0), Point::new(400.0, 100.0));
    // The host says node 1 is selected, whatever the widget last reported.
    let mut ui = Simulator::new(graph_with_selected(&[(0, a), (1, b)], &[1]));
    // Shift-click node 0: extends the HOST's selection, so both.
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::SHIFT,
    ))]);
    ui.point_at(center(a));
    ui.simulate([moved(center(a)), press(), release()]);

    let msgs = messages(ui);
    assert_eq!(
        last_selection(&msgs),
        Some(vec![0, 1]),
        "an interaction must build on the host's selection: {msgs:?}",
    );
}

#[test]
fn ctrl_d_requests_clone_of_selection() {
    let mut ui = Simulator::new(graph_with_selected(&[(0, Point::new(100.0, 100.0))], &[0]));
    ui.point_at(center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(keyboard::Key::Character("d".into()), cmd())]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Clone(vec![0])),
        "Ctrl+D must request cloning of the selection: {msgs:?}",
    );
}

#[test]
fn ctrl_d_without_selection_does_nothing() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    ui.point_at(Point::new(500.0, 400.0));
    ui.simulate([key_pressed(keyboard::Key::Character("d".into()), cmd())]);

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Clone(_))),
        "Ctrl+D with no selection must not request a clone: {msgs:?}",
    );
}

#[test]
fn click_without_motion_does_not_emit_move() {
    // Regression: a press+release in place is a selection click, not a drag.
    // It must not emit a NodeMoved (which would dirty host undo history /
    // sync state on every click).
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Move(..))),
        "a click without motion must not emit Move: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Edge connect / disconnect (pin drag)
//
// Each node here holds exactly one fixed-size pin, so the connection anchor is
// predictable: a Right pin anchors at the node's right edge, a Left pin at its
// left edge, both at the node's vertical center. With NODE_W x NODE_H content
// at world top-left `p`, the anchors are:
//   output (Right): (p.x + NODE_W, p.y + NODE_H/2)
//   input  (Left) : (p.x,          p.y + NODE_H/2)
// ---------------------------------------------------------------------------

const OUT_POS: Point = Point::new(100.0, 100.0);
const IN_POS: Point = Point::new(300.0, 100.0);

fn out_anchor() -> Point {
    Point::new(OUT_POS.x + NODE_W, OUT_POS.y + NODE_H / 2.0)
}
fn in_anchor() -> Point {
    Point::new(IN_POS.x, IN_POS.y + NODE_H / 2.0)
}

fn pin_body<M: 'static>() -> iced::widget::Container<'static, M, Theme, Renderer> {
    container(text("p"))
        .width(Length::Fixed(NODE_W))
        .height(Length::Fixed(NODE_H))
}

/// Two single-pin nodes: node 0 has a Right/Output pin, node 1 a Left/Input pin.
/// `connect_ok` drives `can_connect`; `seed_edge` pre-pushes edge 0:0 -> 1:0.
fn pin_graph(connect_ok: bool, seed_edge: bool) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect)
        .on_disconnect(Msg::Disconnect)
        .can_connect(move |_, _| connect_ok);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    if seed_edge {
        ng = ng.push_edge(edge(0, PinRef::new(0, 0), PinRef::new(1, 0)));
    }
    ng.into()
}

#[test]
fn drag_output_to_input_connects() {
    let mut ui = Simulator::new(pin_graph(true, false));
    drag(&mut ui, out_anchor(), in_anchor());

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "dragging output -> input must connect them: {msgs:?}",
    );
}

#[test]
fn drag_input_to_output_reports_output_first() {
    // Drag starts on the INPUT pin; the reported pair must still be
    // output-first (orient_connection), matching the rendered data-flow.
    let mut ui = Simulator::new(pin_graph(true, false));
    drag(&mut ui, in_anchor(), out_anchor());

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "connection must be normalized output-first regardless of drag direction: {msgs:?}",
    );
}

#[test]
fn drag_to_empty_space_does_not_connect() {
    let mut ui = Simulator::new(pin_graph(true, false));
    drag(&mut ui, out_anchor(), Point::new(600.0, 500.0));

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "releasing over empty space must not connect: {msgs:?}",
    );
}

#[test]
fn can_connect_false_blocks_connection() {
    let mut ui = Simulator::new(pin_graph(false, false));
    drag(&mut ui, out_anchor(), in_anchor());

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "can_connect returning false must block the snap/connect: {msgs:?}",
    );
}

/// Ctrl+click on the edge line cuts it.
#[test]
fn ctrl_click_on_edge_disconnects() {
    let mut ui = Simulator::new(pin_graph(true, true));
    let mid = Point::new((out_anchor().x + in_anchor().x) / 2.0, out_anchor().y);
    ui.point_at(mid);
    // ModifiersChanged + a CursorMoved so pins compute their anchors, then a
    // ctrl-held press on the edge.
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(cmd())),
        moved(mid),
    ]);
    ui.simulate([press(), release()]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Disconnect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "ctrl+click on an edge must disconnect it: {msgs:?}",
    );
}

/// A cut edge is reported by the host's own edge id, so a host keyed by id needs
/// no endpoint search. `on_disconnect` cannot serve this: it also fires while a
/// drag leaves a snapped pin, where no host edge exists.
#[test]
fn cutting_an_edge_reports_its_host_id() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct WireIds;

    impl Ids for WireIds {
        type NodeId = usize;
        type PinId = usize;
        type EdgeId = &'static str;
        type AnchorId = usize;
        type Payload = ();
    }

    #[derive(Debug, Clone, PartialEq)]
    enum M {
        Disconnect(PinRef<WireIds>, PinRef<WireIds>),
        Cut(Vec<&'static str>),
    }

    let mut ng: NodeGraph<'_, WireIds, M, Theme, Renderer> = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_disconnect(M::Disconnect)
        .on_edge_delete(M::Cut);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    ng = ng.push_edge(edge("wire-7", PinRef::new(0, 0), PinRef::new(1, 0)));

    let mut ui = Simulator::new(Element::from(ng));
    let mid = Point::new((out_anchor().x + in_anchor().x) / 2.0, out_anchor().y);
    ui.point_at(mid);
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(cmd())),
        moved(mid),
    ]);
    ui.simulate([press(), release()]);

    let msgs: Vec<M> = ui.into_messages().collect();
    assert!(
        msgs.contains(&M::Cut(vec!["wire-7"])),
        "a cut must name the edge id the host supplied: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Camera: right-drag pan and wheel zoom
// ---------------------------------------------------------------------------

fn camera_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_camera(Msg::Camera);
    ng = ng.push_node(node(
        0usize,
        Point::new(100.0, 100.0),
        container(text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H)),
    ));
    ng.into()
}

fn last_camera(msgs: &[Msg]) -> Option<(Point, f32)> {
    msgs.iter().rev().find_map(|m| match m {
        Msg::Camera(pos, zoom) => Some((*pos, *zoom)),
        _ => None,
    })
}

fn right_press() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
}
fn right_release() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right))
}

#[test]
fn right_drag_pans_camera() {
    let mut ui = Simulator::new(camera_graph());
    let from = Point::new(400.0, 400.0);
    let to = Point::new(460.0, 430.0); // +60, +30 screen
    ui.point_at(from);
    ui.simulate([moved(from), right_press()]);
    ui.point_at(to);
    ui.simulate([moved(to), right_release()]);

    let msgs = messages(ui);
    let (pos, zoom) = last_camera(&msgs).expect("right-drag must change the camera");
    // At zoom 1, panning by (+60,+30) screen shifts the camera position by the
    // same world amount.
    assert!(
        (zoom - 1.0).abs() < 1e-3,
        "pan must not change zoom: {zoom}"
    );
    assert!(
        (pos.x - 60.0).abs() < 1.0 && (pos.y - 30.0).abs() < 1.0,
        "camera should pan by (60, 30), got {pos:?}",
    );
}

#[test]
fn wheel_scroll_zooms_camera() {
    let mut ui = Simulator::new(camera_graph());
    let at = Point::new(400.0, 400.0);
    ui.point_at(at);
    ui.simulate([
        moved(at),
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 },
        }),
    ]);

    let msgs = messages(ui);
    let (_pos, zoom) = last_camera(&msgs).expect("wheel scroll must change the camera");
    assert!(
        zoom > 1.0,
        "scrolling up must zoom in (zoom > 1), got {zoom}",
    );
}

// ---------------------------------------------------------------------------
// Magnetic-plug grab: hysteresis + re-wiring
//
// Grabbing a CONNECTED pin does not disconnect on contact. The edge stays
// snapped (EdgeOver) until the cursor leaves the grabbed pin by more than
// UNSNAP_THRESHOLD (15px); only then does on_disconnect fire. The grabbed end
// can then be dropped on another compatible pin to re-wire.
// ---------------------------------------------------------------------------

fn last_msgs_after_grab(to: Point) -> Vec<Msg> {
    // Seeded edge 0:0 (output) -> 1:0 (input). Grab the input pin and drag to
    // `to`, then release.
    let mut ui = Simulator::new(pin_graph(true, true));
    let from = in_anchor();
    ui.point_at(from);
    ui.simulate([moved(from), press()]);
    ui.point_at(to);
    ui.simulate([moved(to), release()]);
    messages(ui)
}

#[test]
fn grabbing_connected_pin_in_place_keeps_connection() {
    // Press + release on a connected pin without moving: still connected.
    let msgs = last_msgs_after_grab(in_anchor());
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Disconnect(_, _))),
        "grabbing a connected pin in place must not disconnect: {msgs:?}",
    );
}

#[test]
fn dragging_connected_pin_within_hysteresis_keeps_connection() {
    // Move 10px (< UNSNAP_THRESHOLD 15): magnetically stays connected.
    let near = Point::new(in_anchor().x + 10.0, in_anchor().y);
    let msgs = last_msgs_after_grab(near);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Disconnect(_, _))),
        "a sub-threshold drag must not unplug the connection: {msgs:?}",
    );
}

#[test]
fn dragging_connected_pin_past_hysteresis_disconnects() {
    // Move 30px (> UNSNAP_THRESHOLD 15): the plug pops out.
    let far = Point::new(in_anchor().x + 30.0, in_anchor().y);
    let msgs = last_msgs_after_grab(far);
    assert!(
        msgs.contains(&Msg::Disconnect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "dragging past the hysteresis threshold must disconnect: {msgs:?}",
    );
}

// Three nodes: output 0:0 -> input 1:0 (seeded), plus a spare input 2:0.
fn rewire_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect)
        .on_disconnect(Msg::Disconnect);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    ng = ng.push_node(node(
        2usize,
        Point::new(IN_POS.x, 300.0),
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    ng = ng.push_edge(edge(0, PinRef::new(0, 0), PinRef::new(1, 0)));
    ng.into()
}

#[test]
fn rewire_grabbed_pin_to_another_pin() {
    // Grab the input end of 0:0 -> 1:0, pull it past the threshold (pop), then
    // drop it on input 2:0. Expect the old edge to disconnect and a new edge to
    // 2:0 to connect. The pop and the re-snap need separate cursor moves.
    let mut ui = Simulator::new(rewire_graph());
    let grab = in_anchor(); // node 1 input
    let target = Point::new(IN_POS.x, 315.0); // node 2 input anchor

    ui.point_at(grab);
    ui.simulate([moved(grab), press()]);
    // Pull straight down, clearing node 1 pin by more than UNSNAP_THRESHOLD.
    let midway = Point::new(grab.x, 220.0);
    ui.point_at(midway);
    ui.simulate([moved(midway)]);
    // Now snap onto node 2 input.
    ui.point_at(target);
    ui.simulate([moved(target), release()]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Disconnect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "re-wiring must disconnect the original edge: {msgs:?}",
    );
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(2, 0))),
        "re-wiring must connect the grabbed end to the new pin: {msgs:?}",
    );
}

#[test]
fn rewire_back_to_own_input_reconnects() {
    // Grab the input end of 0:0 -> 1:0, pull it past the threshold (disconnect),
    // then drop it back on the SAME input 1:0. Under the default
    // (input_not_occupied), this only works because the edge being dragged is
    // excluded from the occupancy check, so its own input stays a valid target.
    let mut ui = Simulator::new(rewire_graph());
    let grab = in_anchor();

    ui.point_at(grab);
    ui.simulate([moved(grab), press()]);
    let midway = Point::new(grab.x, 220.0); // clear node 1 pin past UNSNAP_THRESHOLD
    ui.point_at(midway);
    ui.simulate([moved(midway)]);
    ui.point_at(grab); // back onto the original input
    ui.simulate([moved(grab), release()]);

    let msgs = messages(ui);
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Disconnect(_, _))),
        "popping the edge off its input must disconnect first: {msgs:?}",
    );
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "dropping back on the original input must reconnect it: {msgs:?}",
    );
}

#[test]
fn default_rejects_second_edge_to_occupied_input() {
    // No can_connect: the built-in default enforces one-edge-per-input. Input 1:0
    // is already wired from 0:0, so dragging a second output (2:0) onto it must not
    // connect.
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    ng = ng.push_node(node(
        2usize,
        Point::new(OUT_POS.x, 300.0),
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_edge(edge(0, PinRef::new(0, 0), PinRef::new(1, 0)));
    let mut ui = Simulator::new(Element::from(ng));

    let from = Point::new(OUT_POS.x + NODE_W, 300.0 + NODE_H / 2.0); // node 2 right pin
    drag(&mut ui, from, in_anchor());

    let msgs = messages(ui);
    assert!(
        !msgs.contains(&Msg::Connect(PinRef::new(2, 0), PinRef::new(1, 0))),
        "default one-edge-per-input must reject a second edge to an occupied input: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Occluded interactions: a node body on top covering another node pin.
//
// Node 2 is a plain body (no pin) placed on top, covering node 1 input anchor.
// Expected: you can DROP a connection onto the covered pin (snap sees all pins
// regardless of cover), but you cannot START an edge drag from it (the covering
// body intercepts the press).
// ---------------------------------------------------------------------------

fn occlusion_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect)
        .on_disconnect(Msg::Disconnect)
        .on_select(Msg::Select)
        .on_move(Msg::Move);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    // Cover node 1 input anchor (IN_POS.x, IN_POS.y + H/2) with a plain body.
    ng = ng.push_node(node(
        2usize,
        Point::new(IN_POS.x - NODE_W / 2.0, IN_POS.y),
        container(text("cover"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H)),
    ));
    ng.into()
}

#[test]
fn drop_connect_through_covering_node_is_possible() {
    // Drag from the visible output and drop on the covered input: snap reaches
    // the pin under the cover, so the connection forms.
    let mut ui = Simulator::new(occlusion_graph());
    drag(&mut ui, out_anchor(), in_anchor());

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "dropping onto a covered pin must still connect: {msgs:?}",
    );
}

#[test]
fn drag_start_on_covered_pin_is_blocked() {
    // Press on the covered input pin: the covering body (node 2) takes the
    // press, so no edge drag starts. Dragging to the output therefore connects
    // nothing.
    let mut ui = Simulator::new(occlusion_graph());
    drag(&mut ui, in_anchor(), out_anchor());

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "a covered pin must not start an edge drag: {msgs:?}",
    );
    // The covering node is what actually got grabbed.
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Msg::Select(ids) if ids.contains(&2))),
        "the covering node should receive the press: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Occluded zoom: an opaque overlay on top of the graph swallows the wheel, so
// the covered graph must not zoom.
// ---------------------------------------------------------------------------

fn overlaid_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_camera(Msg::Camera);
    ng = ng.push_node(node(
        0usize,
        Point::new(100.0, 100.0),
        container(text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H)),
    ));
    let graph: Element<'static, Msg, Theme, Renderer> = ng.into();
    let overlay =
        iced::widget::opaque(container(text("")).width(Length::Fill).height(Length::Fill));
    iced::widget::stack![graph, overlay].into()
}

#[test]
fn wheel_over_opaque_overlay_does_not_zoom_graph() {
    let mut ui = Simulator::new(overlaid_graph());
    let at = Point::new(400.0, 400.0);
    ui.point_at(at);
    ui.simulate([
        moved(at),
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 },
        }),
    ]);

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Camera(_, _))),
        "a covered graph must not zoom under the overlay: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Child-widget interaction: events route to widgets INSIDE a node first.
// ---------------------------------------------------------------------------

#[test]
fn click_on_button_in_node_routes_to_button_not_node() {
    // A button inside a node must consume the click; the node must NOT select.
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select);
    ng = ng.push_node(node(
        0usize,
        Point::new(100.0, 100.0),
        iced::widget::button(text("go"))
            .width(Length::Fixed(80.0))
            .height(Length::Fixed(30.0))
            .on_press(Msg::Button),
    ));
    let mut ui = Simulator::new(Element::from(ng));

    click(&mut ui, Point::new(140.0, 115.0)); // inside the button

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Button),
        "the button inside the node must receive the click: {msgs:?}",
    );
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Select(_))),
        "clicking a child button must not select the node: {msgs:?}",
    );
}

#[test]
fn backspace_in_focused_text_input_does_not_delete_node() {
    // With a text_input inside a node focused, Backspace edits the text; the
    // node (even when selected) must survive, because the input consumes the
    // key before the graph's delete handler runs.
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select)
        .on_delete(Msg::Delete);
    ng = ng.push_node(node(
        0usize,
        Point::new(100.0, 100.0),
        iced::widget::text_input("", "abc")
            .width(Length::Fixed(120.0))
            .on_input(Msg::Input),
    ));
    let mut ui = Simulator::new(Element::from(ng));

    // Focus the input, then select the node via Ctrl+A (handled by the graph
    // before children), then Backspace.
    click(&mut ui, Point::new(150.0, 115.0));
    ui.simulate([key_pressed(keyboard::Key::Character("a".into()), cmd())]);
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Backspace),
        keyboard::Modifiers::default(),
    )]);

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Delete(_))),
        "Backspace in a focused text_input must not delete the node: {msgs:?}",
    );
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Input(s) if s == "ab")),
        "the focused text_input should have consumed Backspace (abc -> ab): {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Shared node/anchor id space: debug-build guard. `node_index`/`anchor_index`
// resolve to the first match, so a duplicate renders one element twice and
// behaves undefined. Edges need no such guard: they carry no id and are
// addressed by index.
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "duplicate node id")]
fn push_node_rejects_duplicate_id_in_debug() {
    let body = || {
        container(text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H))
    };
    let _: Graph = NodeGraph::default()
        .push_node(node(7, Point::new(0.0, 0.0), body()))
        .push_node(node(7, Point::new(50.0, 50.0), body()));
}

/// A node and an anchor may carry the same id: the two are separate id spaces,
/// so a host numbering anchors from zero never has to know what its nodes use.
#[test]
fn a_node_and_an_anchor_may_share_a_number() {
    let mut ng: Graph = NodeGraph::default();
    ng = ng.push_node(node(
        7usize,
        Point::new(0.0, 0.0),
        container(text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H)),
    ));
    ng = ng.push_anchor(anchor(7usize, Point::new(120.0, 120.0)));

    let ui = Simulator::new(Element::from(ng));
    let _ = ui.into_messages();
}

// ---------------------------------------------------------------------------
// Connection validation (no can_connect): direction + self-pin rules.
//
// Duplicate-edge rejection is intentionally NOT a widget guarantee - it is the
// host's job via can_connect - so it is not asserted here.
// ---------------------------------------------------------------------------

#[test]
fn output_to_output_does_not_connect() {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    // Second output pin, anchored at IN_POS left edge for an easy drag target.
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Output),
    ));
    let mut ui = Simulator::new(Element::from(ng));

    drag(&mut ui, out_anchor(), in_anchor());

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "two output pins must not connect (direction rule): {msgs:?}",
    );
}

#[test]
fn cannot_connect_pin_to_itself() {
    // Dragging a pin and releasing back on itself must not self-connect (the
    // source pin is excluded from valid targets).
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_connect(Msg::Connect);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    let mut ui = Simulator::new(Element::from(ng));

    let a = out_anchor();
    ui.point_at(a);
    ui.simulate([moved(a), press()]);
    let nudge = Point::new(a.x + 3.0, a.y); // small move, still on the pin
    ui.point_at(nudge);
    ui.simulate([moved(nudge), release()]);

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "a pin must not connect to itself: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Zoom-at-cursor stability: the world point under the cursor stays fixed.
// ---------------------------------------------------------------------------

#[test]
fn wheel_zoom_keeps_world_point_under_cursor() {
    let mut ui = Simulator::new(camera_graph());
    let at = Point::new(400.0, 300.0);
    // Default camera (zoom 1, pos 0): world under the cursor == screen point.
    ui.point_at(at);
    ui.simulate([
        moved(at),
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 4.0 },
        }),
    ]);

    let msgs = messages(ui);
    let (pos, zoom) = last_camera(&msgs).expect("wheel must change the camera");
    assert!(zoom > 1.0, "scroll up should zoom in: {zoom}");
    // screen_to_world: world = screen/zoom - position. The world point under the
    // cursor must be unchanged (== `at`).
    let wx = at.x / zoom - pos.x;
    let wy = at.y / zoom - pos.y;
    assert!(
        (wx - at.x).abs() < 0.5 && (wy - at.y).abs() < 0.5,
        "world point under cursor drifted after zoom: was {at:?}, now ({wx}, {wy})",
    );
}

// ---------------------------------------------------------------------------
// Hit detection under zoom + pan: the real widget pipeline must locate pins and
// edges when the camera is NOT at the default (zoom 1, no pan), so world pixels
// differ from screen pixels. The other tests run at zoom 1 (world == screen),
// which never exercises the screen<->world transform in hit detection.
//
// World->screen with camera (position, zoom): screen = (world + position) * zoom.
// ---------------------------------------------------------------------------

const CAM_POS: Point = Point::new(50.0, 50.0);
const CAM_ZOOM: f32 = 2.0;

/// Maps a world point to its screen pixel under the (CAM_POS, CAM_ZOOM) camera.
fn world_to_screen(world: Point) -> Point {
    Point::new(
        (world.x + CAM_POS.x) * CAM_ZOOM,
        (world.y + CAM_POS.y) * CAM_ZOOM,
    )
}

/// The same two single-pin nodes as `pin_graph`, but viewed through a zoomed and
/// panned camera, so pin anchors land at non-trivial screen pixels.
fn zoomed_pin_graph(seed_edge: bool) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .camera(CAM_POS, CAM_ZOOM)
        .on_connect(Msg::Connect)
        .on_disconnect(Msg::Disconnect);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    if seed_edge {
        ng = ng.push_edge(edge(0, PinRef::new(0, 0), PinRef::new(1, 0)));
    }
    ng.into()
}

#[test]
fn drag_connects_under_zoom_and_pan() {
    // The output and input anchors are world points; their screen pixels depend
    // on the camera. Correct screen->world hit detection means dragging between
    // the two screen pixels connects them just as it does at zoom 1.
    let mut ui = Simulator::new(zoomed_pin_graph(false));
    drag(
        &mut ui,
        world_to_screen(out_anchor()),
        world_to_screen(in_anchor()),
    );

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Connect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "dragging output -> input under zoom+pan must connect them: {msgs:?}",
    );
}

#[test]
fn ctrl_click_on_edge_disconnects_under_zoom_and_pan() {
    // Ctrl+click on the edge midpoint (in screen space) must hit the edge line
    // even though world != screen, exercising edge hit detection under zoom.
    let mut ui = Simulator::new(zoomed_pin_graph(true));
    // Both anchors share a world y, so the bezier midpoint sits on that y.
    let mid_world = Point::new((out_anchor().x + in_anchor().x) / 2.0, out_anchor().y);
    let mid = world_to_screen(mid_world);
    ui.point_at(mid);
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(cmd())),
        moved(mid),
    ]);
    ui.simulate([press(), release()]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Disconnect(PinRef::new(0, 0), PinRef::new(1, 0))),
        "ctrl+click on the edge under zoom+pan must disconnect it: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Shift-click toggles selection off.
// ---------------------------------------------------------------------------

#[test]
fn shift_click_deselects_already_selected_node() {
    let mut ui = Simulator::new(graph_with_selected(&[(0, Point::new(100.0, 100.0))], &[0]));
    let c = center(Point::new(100.0, 100.0));

    ui.point_at(c);
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::SHIFT,
    ))]);
    ui.simulate([moved(c), press(), release()]); // shift-click again -> toggle off

    assert_eq!(last_selection(&messages(ui)), Some(vec![]));
}

// ---------------------------------------------------------------------------
// Live drag hooks
//
// `on_drag_start` / `on_drag_update` / `on_drag_end` report a drag while it
// happens, for hosts that mirror it elsewhere (a collaborative session, an
// inspector). They are observers: unlike `on_move`, nothing is gated on them,
// and they must bracket every drag exactly once.
// ---------------------------------------------------------------------------

fn drag_infos(msgs: &[Msg]) -> Vec<DragInfo<SimIds>> {
    msgs.iter()
        .filter_map(|m| match m {
            Msg::DragStart(info) => Some(info.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn node_drag_is_bracketed_by_start_and_end() {
    let pos = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(graph_with(&[(0, pos)]));
    drag(&mut ui, center(pos), Point::new(300.0, 260.0));

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::Node { node_id: 0 }],
        "one node drag must report exactly one start, naming the node's user id: {msgs:?}",
    );
    assert_eq!(
        msgs.iter().filter(|m| **m == Msg::DragEnd).count(),
        1,
        "the drag must end exactly once: {msgs:?}",
    );
}

#[test]
fn drag_update_reports_the_world_cursor() {
    let pos = Point::new(100.0, 100.0);
    let target = Point::new(400.0, 300.0);
    let mut ui = Simulator::new(graph_with(&[(0, pos)]));
    drag(&mut ui, center(pos), target);

    // Default camera: zoom 1, no pan, so world coordinates equal screen pixels.
    let updates: Vec<Point> = messages(ui)
        .into_iter()
        .filter_map(|m| match m {
            Msg::DragUpdate(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(updates.last(), Some(&target));
}

#[test]
fn dragging_a_multi_selection_reports_the_whole_group() {
    let (a, b) = (Point::new(100.0, 100.0), Point::new(400.0, 100.0));
    // Both selected by the host, so grabbing one starts a group drag.
    let mut ui = Simulator::new(graph_with_selected(&[(0, a), (1, b)], &[0, 1]));
    drag(&mut ui, center(b), Point::new(600.0, 300.0));

    let msgs = messages(ui);
    let group = drag_infos(&msgs)
        .into_iter()
        .find_map(|info| match info {
            DragInfo::Group { node_ids } => Some(sorted(node_ids)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("dragging a selected node must report a group: {msgs:?}"));
    assert_eq!(group, vec![0, 1]);
}

#[test]
fn selection_box_drag_reports_its_anchor() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(400.0, 400.0))]));
    let anchor = Point::new(50.0, 60.0);
    drag(&mut ui, anchor, Point::new(500.0, 500.0));

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::SelectionBox {
            start_x: anchor.x,
            start_y: anchor.y,
        }],
        "a drag on empty canvas must report the selection box anchor: {msgs:?}",
    );
}

/// The number of `DragEnd`s in `msgs`.
fn drag_ends(msgs: &[Msg]) -> usize {
    msgs.iter().filter(|m| **m == Msg::DragEnd).count()
}

#[test]
fn anchor_drag_reports_the_anchor() {
    let at = Point::new(200.0, 150.0);
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_anchor_move(Msg::AnchorMove)
        .on_drag_start(Msg::DragStart)
        .on_drag_end(|| Msg::DragEnd);
    ng = ng.push_anchor(anchor(ANCHOR_A, at));
    let mut ui = Simulator::new(Element::from(ng));
    drag(
        &mut ui,
        at + CORE_NUDGE,
        at + CORE_NUDGE + Vector::new(60.0, 40.0),
    );

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::Anchor {
            anchor_id: ANCHOR_A
        }],
        "an anchor drag must report exactly one start naming the anchor: {msgs:?}",
    );
    assert_eq!(
        drag_ends(&msgs),
        1,
        "the drag must end exactly once: {msgs:?}"
    );
}

#[test]
fn route_drag_reports_the_edge() {
    let ng = routing_graph()
        .on_drag_start(Msg::DragStart)
        .on_drag_end(|| Msg::DragEnd);
    let mut ui = Simulator::new(route_scene(ng, View::IDENTITY, &[], &[]));
    drag(&mut ui, bare_mid(), Point::new(330.0, 400.0));

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::Route {
            edge_id: ROUTE_EDGE
        }],
        "a grab of the cable's run must report exactly one start naming the edge: {msgs:?}",
    );
    assert_eq!(
        drag_ends(&msgs),
        1,
        "the drag must end exactly once: {msgs:?}"
    );
}

#[test]
fn resize_drag_reports_the_node() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(resizable_graph(&[(0, start)]));
    drag(&mut ui, grip(start), grip(start) + Vector::new(40.0, 10.0));

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::Resize { node_id: 0 }],
        "a grip drag must report exactly one start naming the node: {msgs:?}",
    );
    assert_eq!(
        drag_ends(&msgs),
        1,
        "the drag must end exactly once: {msgs:?}"
    );
}

#[test]
fn edge_cut_drag_reports_the_cutting_tool() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(400.0, 400.0))]));
    let from = Point::new(50.0, 60.0);
    ui.point_at(from);
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(cmd())),
        moved(from),
    ]);
    drag(&mut ui, from, Point::new(300.0, 300.0));

    let msgs = messages(ui);
    assert_eq!(
        drag_infos(&msgs),
        vec![DragInfo::EdgeCut],
        "a cut across the canvas must report exactly one start: {msgs:?}",
    );
    assert_eq!(
        drag_ends(&msgs),
        1,
        "the drag must end exactly once: {msgs:?}"
    );
}

// ---------------------------------------------------------------------------
// Snapshot regression: a node dragged to the graph edge (partially clipped)
// and back to its origin (still held) must render identically to before the
// drag. If clip/culling is stale (computed before the move), the previously
// clipped side stays clipped.
//
// Backend NOTE: iced_test renders with WGPU when a GPU/adapter is available
// (golden files are suffixed `-wgpu`), else it falls back to tiny_skia where
// SDF `draw_primitive` is a no-op. So snapshots see the SDF node fill/border/
// pins only under WGPU; the iced child content (the colored body + text here)
// renders under both. This test asserts on the child content, which exercises
// the clip path regardless of backend.
// ---------------------------------------------------------------------------

fn snapshot_node_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select)
        // on_move is required for node dragging (the widget gates the drag on it).
        .on_move(Msg::Move);
    // Left-aligned text inside the body, so it lands on the side that gets
    // clipped when the node is dragged off the left edge.
    let body = container(text("HELLO WORLD").size(24))
        .width(Length::Fixed(160.0))
        .height(Length::Fixed(80.0))
        .style(|_theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(
                0.9, 0.2, 0.2,
            ))),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        });
    // Centered: 1024x768 -> node spans (432,344)..(592,424).
    ng = ng.push_node(node(0usize, Point::new(432.0, 344.0), body));
    ng.into()
}

/// Removes a golden and the backend-suffixed variants iced_test may have
/// written (`-wgpu`, `-tiny-skia`), so each run starts from a clean reference.
fn clear_golden(stem: &str) {
    let dir = std::env::temp_dir();
    for suffix in ["", "-wgpu", "-tiny-skia"] {
        let _ = std::fs::remove_file(dir.join(format!("{stem}{suffix}.png")));
    }
}

#[test]
fn node_dragged_to_edge_and_back_renders_identically() {
    // Regression: dragging a node so its child content is clipped at the graph
    // edge and back to the origin (still held) must restore the render exactly.
    // The clip is recomputed per frame from the live drag offset, which is
    // exactly 0.0 on return, so the round trip is pixel-identical.
    let mut ui = Simulator::new(snapshot_node_graph());
    let origin = Point::new(512.0, 384.0); // node body center

    click(&mut ui, origin); // select
    let at_origin = ui.snapshot(&Theme::Dark).expect("origin snapshot");

    ui.point_at(origin);
    ui.simulate([moved(origin), press()]);
    let edge = Point::new(30.0, 384.0); // offset ~ -482 -> node left edge ~ -50
    ui.point_at(edge);
    ui.simulate([moved(edge)]);
    let at_edge = ui.snapshot(&Theme::Dark).expect("edge snapshot");
    ui.point_at(origin);
    ui.simulate([moved(origin)]); // back to start, still dragging
    let back = ui.snapshot(&Theme::Dark).expect("round-trip snapshot");

    // Golden holds the origin frame; compare the other two against it.
    // (matches_image appends a `-<backend>` suffix and creates the file when
    // absent; temp dir keeps any leftover out of the repo.)
    let stem = "iced_ng_dragback_origin";
    clear_golden(stem);
    let golden = std::env::temp_dir().join(format!("{stem}.png"));
    let _ = at_origin.matches_image(&golden).expect("write golden");
    let edge_differs = !at_edge.matches_image(&golden).expect("edge vs origin");
    let back_matches = back.matches_image(&golden).expect("round-trip vs origin");
    clear_golden(stem);

    // Guard against a vacuous test: the edge frame must actually differ.
    assert!(
        edge_differs,
        "edge frame should differ from origin (drag/clip not exercised)",
    );
    assert!(
        back_matches,
        "node dragged to the edge and back must render identically to origin",
    );
}

#[test]
fn pin_press_without_on_connect_falls_through_to_selection() {
    // Gating: with no on_connect wired, pressing a pin must not start an edge drag;
    // the press falls through to selecting the pin's node instead.
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select); // deliberately no on_connect
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    let mut ui = Simulator::new(Element::from(ng));

    // Just inside node 0's right edge: within PIN_CLICK_THRESHOLD of the pin, yet
    // still over the body, so a blocked edge drag falls through to body selection.
    let near_pin = Point::new(OUT_POS.x + NODE_W - 2.0, OUT_POS.y + NODE_H / 2.0);
    drag(&mut ui, near_pin, in_anchor());

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Connect(_, _))),
        "without on_connect, pressing a pin must not start an edge: {msgs:?}",
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Msg::Select(ids) if ids.contains(&0))),
        "the pin press should fall through to selecting its node: {msgs:?}",
    );
}

/// A host that only reads `on_select` and never marks a node: selection has to
/// keep working on its own, so a click followed by Delete acts on the clicked
/// node. Marking nodes is an override, not a requirement.
#[test]
fn selection_works_without_the_host_marking_anything() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Delete),
        keyboard::Modifiers::default(),
    )]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::Delete(vec![0])),
        "an unmarked host must still get a working selection: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Corner-grip resize
//
// The grip is the bottom-right RESIZE_GRIP_SIDE (12 world px at zoom 1) square
// of a resizable node's body, so for a NODE_W x NODE_H body at world `p` it
// spans `p + (48, 18) .. p + (60, 30)`. A resize is only ever REPORTED: the
// host owns the content, so the node keeps its size for the whole drag and
// every report is absolute (`size at press + cursor delta`).
// ---------------------------------------------------------------------------

/// A point inside the bottom-right grip of a node whose top-left is `p`.
fn grip(p: Point) -> Point {
    Point::new(p.x + NODE_W - 6.0, p.y + NODE_H - 6.0)
}

fn last_resize(msgs: &[Msg]) -> Option<(usize, Size)> {
    msgs.iter().rev().find_map(|m| match m {
        Msg::Resize(id, size) => Some((*id, *size)),
        _ => None,
    })
}

fn any_move(msgs: &[Msg]) -> bool {
    msgs.iter().any(|m| matches!(m, Msg::Move(..)))
}

#[test]
fn grip_drag_on_resizable_node_reports_the_new_size() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(resizable_graph(&[(0, start)]));
    drag(&mut ui, grip(start), grip(start) + Vector::new(40.0, 10.0));

    let msgs = messages(ui);
    let (id, size) = last_resize(&msgs).expect("dragging the grip must emit Resize");
    assert_eq!(id, 0);
    assert!(
        (size.width - (NODE_W + 40.0)).abs() < 0.5 && (size.height - (NODE_H + 10.0)).abs() < 0.5,
        "grip drag should report (100, 40), got {size:?}",
    );
    assert!(
        !any_move(&msgs),
        "a resize must not move the node: {msgs:?}",
    );
}

#[test]
fn grip_drag_on_non_resizable_node_moves_it_instead() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(graph_with(&[(0, start)]));
    drag(&mut ui, grip(start), grip(start) + Vector::new(40.0, 10.0));

    let msgs = messages(ui);
    assert_eq!(last_resize(&msgs), None, "unmarked node must not resize");
    let moved = msgs.iter().find_map(|m| match m {
        Msg::Move(delta, ids) => Some((*delta, sorted(ids.clone()))),
        _ => None,
    });
    let (delta, ids) = moved.expect("the corner of an unmarked node still drags it");
    assert_eq!(ids, vec![0]);
    assert!(
        (delta.x - 40.0).abs() < 0.5 && (delta.y - 10.0).abs() < 0.5,
        "node should move by (40, 10), got {delta:?}",
    );
}

/// Dragging the grip up and left past zero clamps at MIN_NODE_SIZE, so the node
/// keeps a body to grab and a grip to grow it back by.
#[test]
fn grip_drag_clamps_to_the_minimum_size() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(resizable_graph(&[(0, start)]));
    // Delta (-44, -14) would ask for 16x16; both axes hit the floor.
    drag(&mut ui, grip(start), start + Vector::new(10.0, 10.0));

    let msgs = messages(ui);
    let (_, size) = last_resize(&msgs).expect("dragging the grip must emit Resize");
    assert_eq!(size, Size::new(32.0, 24.0));
}

#[test]
fn body_drag_on_a_resizable_node_still_moves_it() {
    let start = Point::new(100.0, 100.0);
    let mut ui = Simulator::new(resizable_graph(&[(0, start)]));
    drag(
        &mut ui,
        center(start),
        center(start) + Vector::new(50.0, 20.0),
    );

    let msgs = messages(ui);
    assert_eq!(
        last_resize(&msgs),
        None,
        "a body drag is a move, not a resize: {msgs:?}",
    );
    let moved = msgs.iter().find_map(|m| match m {
        Msg::Move(delta, ids) => Some((*delta, sorted(ids.clone()))),
        _ => None,
    });
    let (delta, ids) = moved.expect("dragging a resizable node's body must emit Move");
    assert_eq!(ids, vec![0]);
    assert!(
        (delta.x - 50.0).abs() < 0.5 && (delta.y - 20.0).abs() < 0.5,
        "node should move by (50, 20), got {delta:?}",
    );
}

// ---------------------------------------------------------------------------
// Anchor dragging. An anchor is a grabbable object of the graph itself, and
// like every other gesture the widget only REPORTS the result - it never moves
// the anchor.
// ---------------------------------------------------------------------------

/// The camera a scene is viewed through together with the widget origin it is
/// drawn at: the two terms of the mapping a press has to aim through,
/// `screen = origin + (world + position) * zoom`.
#[derive(Debug, Clone, Copy)]
struct View {
    position: Point,
    zoom: f32,
    origin: Vector,
}

impl View {
    /// The default camera at the window origin, where a world point IS its
    /// screen pixel.
    const IDENTITY: Self = Self {
        position: Point::ORIGIN,
        zoom: 1.0,
        origin: Vector::ZERO,
    };

    /// The same camera read through a graph drawn `TOOLBAR` px down the window.
    const fn below_toolbar(self) -> Self {
        Self {
            position: self.position,
            zoom: self.zoom,
            origin: Vector::new(0.0, TOOLBAR),
        }
    }

    /// The screen pixel this view draws `world` at.
    fn screen(self, world: Point) -> Point {
        Point::new(
            self.origin.x + (world.x + self.position.x) * self.zoom,
            self.origin.y + (world.y + self.position.y) * self.zoom,
        )
    }
}

/// Half scale: a screen-pixel threshold divided by zoom reaches twice as far
/// into the world as it does at zoom 1, while a ring radius and a cable's arc
/// length stay where they are.
const HALF_SCALE: View = View {
    position: Point::ORIGIN,
    zoom: 0.5,
    origin: Vector::ZERO,
};

/// Double scale, panned, so neither term of the mapping is the identity.
const DOUBLE_SCALE: View = View {
    position: Point::new(-100.0, -100.0),
    zoom: 2.0,
    origin: Vector::ZERO,
};

/// The cameras every anchor and route gesture is replayed under. Zoom 1 is the
/// one point where a threshold divided by zoom and a world-fixed radius agree,
/// so a gesture pinned only there is pinned nowhere.
const OFF_UNIT_VIEWS: [View; 2] = [HALF_SCALE, DOUBLE_SCALE];

/// A press this far off an anchor's core, in world units: inside the core's
/// grab box at every zoom these tests use, and off centre enough that a report
/// carrying the anchor's own position can be told from one carrying the
/// cursor's.
const CORE_NUDGE: Vector = Vector::new(2.0, 1.0);

/// Radius of an anchor's innermost orbit, read from the style the widget
/// resolves it from. Every anchor in these scenes carries at most one edge, so
/// orbit 0 is the only ring in play.
fn orbit_0() -> f32 {
    default_anchor_style(&Theme::Dark, AnchorStatus::Idle).orbit_offset
}

/// The anchor a routed cable in these scenes wraps.
const ANCHOR_A: usize = 7;
/// A second anchor, off the cable: nothing reaches it until a route drag
/// carries the cursor there.
const ANCHOR_B: usize = 8;

/// A graph whose only content is one anchor, its move gesture wired.
fn graph_with_anchor(view: View, at: Point) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .camera(view.position, view.zoom)
        .on_anchor_move(Msg::AnchorMove);
    ng = ng.push_anchor(anchor(ANCHOR_A, at));
    ng.into()
}

/// The drag reports the world position the anchor ended up at, outright rather
/// than as a delta, so a host that lost a frame still lands where the cursor is.
///
/// The press sits OFF the core's centre: the report is the anchor's own
/// position plus the cursor's travel, and a press on the centre could not tell
/// that apart from a report of the drop point.
#[test]
fn dragging_an_anchor_reports_its_new_position() {
    let at = Point::new(200.0, 150.0);
    let travel = Vector::new(60.0, 40.0);
    let mut ui = Simulator::new(graph_with_anchor(View::IDENTITY, at));
    drag(&mut ui, at + CORE_NUDGE, at + CORE_NUDGE + travel);

    assert_eq!(messages(ui), vec![Msg::AnchorMove(ANCHOR_A, at + travel)]);
}

/// A press and release without motion is a click, not a move - the same rule
/// node dragging follows, so a plain click never dirties host state.
#[test]
fn clicking_an_anchor_reports_no_move() {
    let at = Point::new(200.0, 150.0);
    let mut ui = Simulator::new(graph_with_anchor(View::IDENTITY, at));
    click(&mut ui, at);

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::AnchorMove(..))),
        "a motionless click reported a move: {msgs:?}",
    );
}

/// The grab is gated on `on_anchor_move` being wired: without it the press
/// falls through to the selection box rather than starting a drag the host
/// could never apply.
#[test]
fn an_anchor_is_not_grabbable_without_the_callback() {
    let at = Point::new(200.0, 150.0);
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select);
    ng = ng.push_anchor(anchor(ANCHOR_A, at));
    let mut ui = Simulator::new(Element::from(ng));
    drag(&mut ui, at, Point::new(260.0, 190.0));

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::AnchorMove(..))),
        "an anchor moved without a handler to report it to: {msgs:?}",
    );
}

// ---------------------------------------------------------------------------
// Route gestures. An edge connects two pins and carries the anchors it wraps;
// every gesture below edits that route, and every one of them only REPORTS.
//
// The scene: node 0's Right/Output pin at `out_anchor()` = (160, 115), node 1's
// Left/Input pin at `route_in_anchor()` = (500, 115). Both bezier control
// points of the unrouted cable lie on y = 115, so the bare cable IS the
// straight 340 px segment between the pins; `bare_mid()` sits 170 along it,
// clear of the 24 px end zone at either end.
//
// Routed through the anchor at `A_AT`, the cable leaves that line and runs
// tangent to orbit 0 instead, wrapping the side of the ring AWAY from the
// pin-to-pin run. `wrap_point()` - the lowest point of that circle - therefore
// lies on the arc the cable draws, `orbit_0()` px from the core it belongs to.
// ---------------------------------------------------------------------------

/// The graph shape the route scenes need: edge ids are `usize`, so an edge is
/// named by the index it was pushed at.
type RouteGraph = Graph;

/// The id of the one edge a route scene carries.
const ROUTE_EDGE: usize = 0;

/// The input node of the route scene, far enough from the output that the cable
/// has a mid-run between the two end zones.
const ROUTE_IN_POS: Point = Point::new(500.0, 100.0);

/// Anchor A, 85 px below the bare cable: far enough that the wrap and the
/// straight run are never the same press.
const A_AT: Point = Point::new(330.0, 200.0);
/// Anchor B, well clear of the cable.
const B_AT: Point = Point::new(330.0, 400.0);

fn route_in_anchor() -> Point {
    Point::new(ROUTE_IN_POS.x, ROUTE_IN_POS.y + NODE_H / 2.0)
}

/// The midpoint of the bare cable's straight run.
fn bare_mid() -> Point {
    Point::new((out_anchor().x + route_in_anchor().x) / 2.0, out_anchor().y)
}

/// Where a cable routed through the anchor at `at` wraps it.
fn wrap_point(at: Point) -> Point {
    Point::new(at.x, at.y + orbit_0())
}

/// Fills `ng` with the route scene, read through `view`: node 0's output pin,
/// node 1's input pin, one anchor per entry of `anchors`, and edge
/// [`ROUTE_EDGE`] between the two pins routed through `route`.
fn route_scene(
    ng: RouteGraph,
    view: View,
    anchors: &[(usize, Point)],
    route: &[usize],
) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng = ng.camera(view.position, view.zoom);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        ROUTE_IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    for &(id, at) in anchors {
        ng = ng.push_anchor(anchor(id, at));
    }
    ng = ng.push_edge(
        edge(ROUTE_EDGE, PinRef::new(0, 0), PinRef::new(1, 0)).route(route.iter().copied()),
    );
    ng.into()
}

/// All three route-drag callbacks wired - the gating the mid-run and wrap zones
/// ask for before they are live at all.
fn routing_graph() -> RouteGraph {
    NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_anchor_create(Msg::AnchorCreated)
        .on_route_attach(Msg::RouteAttached)
        .on_route_detach(Msg::RouteDetached)
}

fn pan_press() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
}
fn pan_release() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right))
}

/// A grab of the cable's run puts a new anchor where the drag ends: the widget
/// mints nothing itself, it names the edge and the world point and lets the
/// host decide what the anchor is called.
#[test]
fn grabbing_a_cable_mid_run_creates_an_anchor_on_release() {
    let mut ui = Simulator::new(route_scene(routing_graph(), View::IDENTITY, &[], &[]));
    let dropped_at = Point::new(330.0, 400.0);
    drag(&mut ui, bare_mid(), dropped_at);

    assert_eq!(
        messages(ui),
        vec![Msg::AnchorCreated(ROUTE_EDGE, dropped_at)],
    );
}

/// Attaching is plug behaviour: the moment the drag reaches an anchor's snap
/// distance the attachment is published, so the host's own route is what the
/// next frame draws. Release commits nothing further.
#[test]
fn a_route_drag_snaps_onto_an_anchor_immediately() {
    let snapped = || {
        let mut ui = Simulator::new(route_scene(
            routing_graph(),
            View::IDENTITY,
            &[(ANCHOR_B, B_AT)],
            &[],
        ));
        ui.point_at(bare_mid());
        ui.simulate([moved(bare_mid()), press()]);
        ui.point_at(B_AT);
        ui.simulate([moved(B_AT)]);
        ui
    };

    assert_eq!(
        messages(snapped()),
        vec![Msg::RouteAttached(ROUTE_EDGE, ANCHOR_B)],
        "the attachment must be published on snap, not held back for the release",
    );

    let mut released = snapped();
    released.simulate([release()]);
    assert_eq!(
        messages(released),
        vec![Msg::RouteAttached(ROUTE_EDGE, ANCHOR_B)],
        "releasing on the anchor must add nothing: it is already attached",
    );
}

/// Leaving the ring again reports the detachment while the drag is still in
/// flight, the mirror of the snap - a drag that only committed on release could
/// not be talked out of an anchor it brushed past.
#[test]
fn leaving_the_anchor_detaches_again() {
    let mut ui = Simulator::new(route_scene(
        routing_graph(),
        View::IDENTITY,
        &[(ANCHOR_B, B_AT)],
        &[],
    ));
    ui.point_at(bare_mid());
    ui.simulate([moved(bare_mid()), press()]);
    ui.point_at(B_AT);
    ui.simulate([moved(B_AT)]);
    let away = Point::new(B_AT.x, B_AT.y - 100.0);
    ui.point_at(away);
    ui.simulate([moved(away)]);

    assert_eq!(
        messages(ui),
        vec![
            Msg::RouteAttached(ROUTE_EDGE, ANCHOR_B),
            Msg::RouteDetached(ROUTE_EDGE, ANCHOR_B),
        ],
    );
}

/// A grab at the wrap holds the anchor rather than dropping it: the cable comes
/// off only once the cursor is past the unsnap distance, so a hand that shakes
/// while pressing leaves the route exactly as it was.
#[test]
fn a_wrap_grab_detaches_only_past_the_unsnap_threshold() {
    let wrap = wrap_point(A_AT);
    let scene = || {
        route_scene(
            routing_graph(),
            View::IDENTITY,
            &[(ANCHOR_A, A_AT)],
            &[ANCHOR_A],
        )
    };

    let mut wiggled = Simulator::new(scene());
    // 13 units outside orbit 0, and so 2 short of the UNSNAP_THRESHOLD of 15
    // the ring has to be left by: a shrunken threshold lets go right here.
    let nudge = Point::new(A_AT.x, A_AT.y + orbit_0() + 13.0);
    wiggled.point_at(wrap);
    wiggled.simulate([moved(wrap), press()]);
    wiggled.point_at(nudge);
    wiggled.simulate([moved(nudge), release()]);
    assert_eq!(
        messages(wiggled),
        Vec::<Msg>::new(),
        "a wrap grab that never left the ring must keep the anchor",
    );

    let mut pulled = Simulator::new(scene());
    let away = Point::new(A_AT.x, A_AT.y + 300.0);
    pulled.point_at(wrap);
    pulled.simulate([moved(wrap), press()]);
    pulled.point_at(away);
    pulled.simulate([moved(away), release()]);
    assert_eq!(
        messages(pulled),
        vec![
            Msg::RouteDetached(ROUTE_EDGE, ANCHOR_A),
            Msg::AnchorCreated(ROUTE_EDGE, away),
        ],
        "pulling the cable off must report the detachment, then the anchor the \
         release leaves behind",
    );
}

/// The pan button both pans and clicks, and travel is what tells them apart: a
/// click on a core deletes the anchor and pans nothing, a drag from the same
/// press pans from that very point and deletes nothing.
#[test]
fn a_click_of_the_pan_button_on_a_core_deletes() {
    let scene = || {
        route_scene(
            routing_graph()
                .on_anchor_delete(Msg::AnchorDeleted)
                .on_camera(Msg::Camera),
            View::IDENTITY,
            &[(ANCHOR_A, A_AT)],
            &[ANCHOR_A],
        )
    };

    let mut clicked = Simulator::new(scene());
    clicked.point_at(A_AT);
    clicked.simulate([moved(A_AT), pan_press(), pan_release()]);
    assert_eq!(
        messages(clicked),
        vec![Msg::AnchorDeleted(ANCHOR_A)],
        "a travel-free pan-button click on a core deletes it and pans nothing",
    );

    let mut dragged = Simulator::new(scene());
    let to = Point::new(A_AT.x + 60.0, A_AT.y);
    dragged.point_at(A_AT);
    dragged.simulate([moved(A_AT), pan_press()]);
    dragged.point_at(to);
    dragged.simulate([moved(to), pan_release()]);
    assert_eq!(
        messages(dragged),
        vec![Msg::Camera(Point::new(60.0, 0.0), 1.0)],
        "a pan-button press that travelled is a pan seeded at the press point, \
         and never a delete",
    );
}

/// The same click on a wrap takes that one cable off the anchor, leaving the
/// anchor and every other cable through it alone.
#[test]
fn a_click_of_the_pan_button_on_a_wrap_detaches() {
    let mut ui = Simulator::new(route_scene(
        routing_graph().on_camera(Msg::Camera),
        View::IDENTITY,
        &[(ANCHOR_A, A_AT)],
        &[ANCHOR_A],
    ));
    let wrap = wrap_point(A_AT);
    ui.point_at(wrap);
    ui.simulate([moved(wrap), pan_press(), pan_release()]);

    assert_eq!(
        messages(ui),
        vec![Msg::RouteDetached(ROUTE_EDGE, ANCHOR_A)],
        "a pan-button click on a wrap detaches that cable and pans nothing",
    );
}

/// An unwired zone is INERT, not merely silent: the press has to reach whatever
/// is behind the cable, which on empty canvas is the selection box.
#[test]
fn route_gestures_without_callbacks_fall_through() {
    let ng: RouteGraph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_drag_start(Msg::DragStart);
    let mut ui = Simulator::new(route_scene(ng, View::IDENTITY, &[], &[]));
    let at = bare_mid();
    ui.point_at(at);
    ui.simulate([moved(at), press()]);

    assert_eq!(
        messages(ui),
        vec![Msg::DragStart(DragInfo::SelectionBox {
            start_x: at.x,
            start_y: at.y,
        })],
        "the mid-run zone must not swallow a press it has no callback for",
    );
}

/// One edge is one cable: two cables sharing an anchor are two connections, and
/// a cut through one reports exactly that one - by its host id, with the other
/// left wired.
#[test]
fn cutting_a_routed_cable_kills_one_edge() {
    let mut ng: RouteGraph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_disconnect(Msg::Disconnect)
        .on_edge_delete(Msg::EdgeDelete);
    let shared = Point::new(330.0, 300.0);
    for (id, at) in [(0usize, OUT_POS), (2usize, Point::new(OUT_POS.x, 500.0))] {
        ng = ng.push_node(node(id, at, pin!(Right, 0usize, pin_body::<_>(), Output)));
    }
    for (id, at) in [
        (1usize, ROUTE_IN_POS),
        (3usize, Point::new(ROUTE_IN_POS.x, 500.0)),
    ] {
        ng = ng.push_node(node(id, at, pin!(Left, 0usize, pin_body::<_>(), Input)));
    }
    ng = ng.push_anchor(anchor(ANCHOR_A, shared));
    ng = ng.push_edge(edge(0usize, PinRef::new(0, 0), PinRef::new(1, 0)).route([ANCHOR_A]));
    ng = ng.push_edge(edge(1usize, PinRef::new(2, 0), PinRef::new(3, 0)).route([ANCHOR_A]));

    let mut ui = Simulator::new(Element::from(ng));
    // 100 px along the upper cable, where it has already left the straight run
    // toward the shared anchor but is still 147 px from the lower one.
    let on_upper_cable = Point::new(228.0, 160.0);
    ui.point_at(on_upper_cable);
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(cmd())),
        moved(on_upper_cable),
    ]);
    ui.simulate([press(), release()]);

    assert_eq!(
        messages(ui),
        vec![
            Msg::Disconnect(PinRef::new(0, 0), PinRef::new(1, 0)),
            Msg::EdgeDelete(vec![0]),
        ],
        "a cut through one routed cable must take that edge and only that edge",
    );
}

/// A busy anchor takes a second cable on its next free orbit, and takes it
/// DURING the drag like every other snap.
///
/// The occupied case is its own test because the ring the drag is measured
/// against is not the anchor's innermost one: orbit 0 already carries a cable,
/// so the second cable is offered orbit 1 and snaps at that larger radius.
#[test]
fn a_route_drag_snaps_onto_an_occupied_anchor() {
    let scene = || {
        let mut ng = routing_graph();
        ng = ng.push_node(node(
            0usize,
            OUT_POS,
            pin!(Right, 0usize, pin_body::<_>(), Output),
        ));
        ng = ng.push_node(node(
            1usize,
            ROUTE_IN_POS,
            pin!(Left, 0usize, pin_body::<_>(), Input),
        ));
        ng = ng.push_node(node(
            2usize,
            Point::new(OUT_POS.x, 500.0),
            pin!(Right, 0usize, pin_body::<_>(), Output),
        ));
        ng = ng.push_node(node(
            3usize,
            Point::new(ROUTE_IN_POS.x, 500.0),
            pin!(Left, 0usize, pin_body::<_>(), Input),
        ));
        ng = ng.push_anchor(anchor(ANCHOR_A, A_AT));
        // Edge 0 already wraps A, so A is singly occupied.
        ng = ng.push_edge(edge(0usize, PinRef::new(0, 0), PinRef::new(1, 0)).route([ANCHOR_A]));
        // Edge 1 runs well below and is unrouted.
        ng = ng.push_edge(edge(1usize, PinRef::new(2, 0), PinRef::new(3, 0)));
        Element::from(ng)
    };
    // The lower cable's mid-run, and the core of the busy anchor.
    let lower_mid = Point::new(bare_mid().x, 515.0);

    let mut ui = Simulator::new(scene());
    ui.point_at(lower_mid);
    ui.simulate([moved(lower_mid), press()]);
    ui.point_at(A_AT);
    ui.simulate([moved(A_AT)]);

    assert_eq!(
        messages(ui),
        vec![Msg::RouteAttached(1, ANCHOR_A)],
        "a second cable must attach on snap, not wait for the release",
    );
}

/// A cable pulled off one anchor can be put straight onto another.
///
/// The detached anchor stays snap-eligible so the drag can also put it back,
/// and that must not stop a DIFFERENT anchor from taking it.
#[test]
fn a_detached_cable_can_be_attached_to_another_anchor() {
    let mut ui = Simulator::new(route_scene(
        routing_graph(),
        View::IDENTITY,
        &[(ANCHOR_A, A_AT), (ANCHOR_B, B_AT)],
        &[ANCHOR_A],
    ));
    let wrap = wrap_point(A_AT);
    ui.point_at(wrap);
    ui.simulate([moved(wrap), press()]);
    // Off A by more than the unsnap distance, clear of both anchors.
    let between = Point::new(A_AT.x, (A_AT.y + B_AT.y) / 2.0);
    ui.point_at(between);
    ui.simulate([moved(between)]);
    // Onto B.
    ui.point_at(B_AT);
    ui.simulate([moved(B_AT)]);

    assert_eq!(
        messages(ui),
        vec![
            Msg::RouteDetached(ROUTE_EDGE, ANCHOR_A),
            Msg::RouteAttached(ROUTE_EDGE, ANCHOR_B),
        ],
        "a cable pulled off one anchor must be able to land on another",
    );
}

// ---------------------------------------------------------------------------
// The same gestures at other zooms. Every threshold a press is resolved
// against is a screen-pixel distance divided by zoom, so it lands in world
// units and GROWS without bound as the camera pulls back, while the ring radius
// and the arc length it competes with stay where they are. Zoom 1 is the one
// point where the two agree, so a gesture pinned only there is pinned nowhere.
//
// The gesture itself is unchanged: the press aims at the SCREEN pixel the view
// draws the target at, and the messages are the ones zoom 1 publishes.
// ---------------------------------------------------------------------------

/// A world point clear of every node, cable and anchor of the route scene, and
/// on screen under each view these tests use.
const CLEAR_OF_EVERYTHING: Point = Point::new(330.0, 300.0);

/// Pulled back until the raw end-zone budget (`EDGE_END_GRAB_LENGTH`, 24
/// screen px) covers 192 world units at either end of the 340 unit cable, so
/// the two ends would meet in the middle and leave no run to grab.
const EIGHTH_SCALE: View = View {
    position: Point::ORIGIN,
    zoom: 0.125,
    origin: Vector::ZERO,
};

/// The wrap is grabbed by its RING, which is world-fixed: a press on it has to
/// reach the cable at every zoom rather than being swallowed by whatever zone
/// grew over it.
///
/// The anchor's own move gesture is wired, so the core is a live competitor for
/// the press - and a core that took it would drag the anchor away instead of
/// taking the cable off it.
#[test]
fn a_wrap_grab_detaches_at_any_zoom() {
    for view in OFF_UNIT_VIEWS {
        let zoom = view.zoom;
        let mut ui = Simulator::new(route_scene(
            routing_graph().on_anchor_move(Msg::AnchorMove),
            view,
            &[(ANCHOR_A, A_AT)],
            &[ANCHOR_A],
        ));
        drag(
            &mut ui,
            view.screen(wrap_point(A_AT)),
            view.screen(CLEAR_OF_EVERYTHING),
        );

        assert_eq!(
            messages(ui),
            vec![
                Msg::RouteDetached(ROUTE_EDGE, ANCHOR_A),
                Msg::AnchorCreated(ROUTE_EDGE, CLEAR_OF_EVERYTHING),
            ],
            "the wrap must take the press and report the detach at zoom {zoom}",
        );
    }
}

/// The pan-button click on a wrap, with the anchor's own delete wired: a press
/// the core swallowed does not merely go quiet, it deletes the anchor the click
/// was meant to take one cable off.
#[test]
fn a_pan_button_click_on_a_wrap_detaches_at_any_zoom() {
    for view in OFF_UNIT_VIEWS {
        let zoom = view.zoom;
        let mut ui = Simulator::new(route_scene(
            routing_graph()
                .on_anchor_delete(Msg::AnchorDeleted)
                .on_camera(Msg::Camera),
            view,
            &[(ANCHOR_A, A_AT)],
            &[ANCHOR_A],
        ));
        let wrap = view.screen(wrap_point(A_AT));
        ui.point_at(wrap);
        ui.simulate([moved(wrap), pan_press(), pan_release()]);

        assert_eq!(
            messages(ui),
            vec![Msg::RouteDetached(ROUTE_EDGE, ANCHOR_A)],
            "the click must detach that cable and leave the anchor at zoom {zoom}",
        );
    }
}

/// The core stays grabbable at every zoom, and the press is off centre, so the
/// report has to carry the anchor's own position plus the cursor's travel.
#[test]
fn an_anchor_core_is_grabbable_at_any_zoom() {
    let at = Point::new(200.0, 150.0);
    let travel = Vector::new(60.0, 40.0);
    for view in OFF_UNIT_VIEWS {
        let zoom = view.zoom;
        let mut ui = Simulator::new(graph_with_anchor(view, at));
        drag(
            &mut ui,
            view.screen(at + CORE_NUDGE),
            view.screen(at + CORE_NUDGE + travel),
        );

        assert_eq!(
            messages(ui),
            vec![Msg::AnchorMove(ANCHOR_A, at + travel)],
            "the core must grab and report the same move at zoom {zoom}",
        );
    }
}

/// The run between the two end zones is grabbable at every zoom, [`EIGHTH_SCALE`]
/// included: an end zone that outgrew the cable would take the press for an
/// unplug and create nothing.
#[test]
fn a_cable_mid_run_creates_an_anchor_at_any_zoom() {
    for view in [HALF_SCALE, DOUBLE_SCALE, EIGHTH_SCALE] {
        let zoom = view.zoom;
        let mut ui = Simulator::new(route_scene(routing_graph(), view, &[], &[]));
        drag(
            &mut ui,
            view.screen(bare_mid()),
            view.screen(CLEAR_OF_EVERYTHING),
        );

        assert_eq!(
            messages(ui),
            vec![Msg::AnchorCreated(ROUTE_EDGE, CLEAR_OF_EVERYTHING)],
            "the cable's run must take the press and name the anchor at zoom {zoom}",
        );
    }
}

// ---------------------------------------------------------------------------
// Anchors at a non-zero widget origin. An anchor is drawn by the graph itself
// rather than laid out as a child, so nothing carries the widget's origin for
// it: the hit test has to fold it in the same way `draw` does, or the dot the
// user aims at and the target that answers are `TOOLBAR` px apart. Nodes are
// immune - their layout bounds already carry the origin - which is why every
// other test here, sitting at the window origin, cannot see the difference.
//
// The scene puts a fixed-height element above the graph, so anchor 9 at world
// (230, 300) is DRAWN at (230, 400).
// ---------------------------------------------------------------------------

/// Height of the element above the graph.
const TOOLBAR: f32 = 100.0;
/// The edge and anchor of the off-origin scene.
const OFFSET_EDGE: usize = 7;
const OFFSET_ANCHOR: usize = 9;
/// World position of the off-origin scene's anchor.
const OFFSET_ANCHOR_AT: Point = Point::new(230.0, 300.0);

/// The view the off-origin scene is read through at zoom 1: the toolbar is part
/// of the mapping, so a world point lands `TOOLBAR` px down the window.
const OFFSET_VIEW: View = View::IDENTITY.below_toolbar();

/// Where a world point of the off-origin scene lands on screen at zoom 1.
fn drawn_at(world: Point) -> Point {
    OFFSET_VIEW.screen(world)
}

/// Output 0:0 -> input 1:0 as edge [`OFFSET_EDGE`] routed through `route`, plus
/// anchor [`OFFSET_ANCHOR`], read through `view`, with the whole graph pushed
/// down by a toolbar.
fn offset_scene(view: View, route: &[usize]) -> Element<'static, Msg, Theme, Renderer> {
    use iced::widget::column;

    let mut ng: RouteGraph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .camera(view.position, view.zoom)
        .on_select(Msg::Select)
        .on_camera(Msg::Camera)
        .on_anchor_move(Msg::AnchorMove)
        .on_anchor_create(Msg::AnchorCreated)
        .on_anchor_delete(Msg::AnchorDeleted)
        .on_route_attach(Msg::RouteAttached)
        .on_route_detach(Msg::RouteDetached);
    ng = ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body::<_>(), Output),
    ));
    ng = ng.push_node(node(
        1usize,
        IN_POS,
        pin!(Left, 0usize, pin_body::<_>(), Input),
    ));
    ng = ng.push_anchor(anchor(OFFSET_ANCHOR, OFFSET_ANCHOR_AT));
    ng =
        ng.push_edge(edge(OFFSET_EDGE, PinRef::new(0, 0), PinRef::new(1, 0)).route(route.to_vec()));
    column![
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fixed(TOOLBAR)),
        ng
    ]
    .into()
}

/// The core is drawn at world plus widget origin, so that is where it has to be
/// grabbable - and what it reports is still a world position, because that is
/// what the host stores: the anchor's own coordinates plus the cursor's travel,
/// which a press on the core's centre could not tell from the drop point.
#[test]
fn an_anchor_core_is_grabbable_where_it_is_drawn() {
    let travel = Vector::new(50.0, 40.0);
    let press_at = OFFSET_ANCHOR_AT + CORE_NUDGE;
    let mut ui = Simulator::new(offset_scene(OFFSET_VIEW, &[]));
    drag(&mut ui, drawn_at(press_at), drawn_at(press_at + travel));

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::AnchorMove(OFFSET_ANCHOR, OFFSET_ANCHOR_AT + travel)),
        "the drawn core must grab and report a world position: {msgs:?}",
    );
}

/// And nowhere else: an anchor that answers a press `TOOLBAR` px above its dot
/// is a target the user cannot see.
#[test]
fn nothing_is_grabbable_at_the_unshifted_world_position() {
    let mut ui = Simulator::new(offset_scene(OFFSET_VIEW, &[]));
    drag(&mut ui, OFFSET_ANCHOR_AT, Point::new(280.0, 340.0));

    let msgs = messages(ui);
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::AnchorMove(..))),
        "the anchor must not answer where it is not drawn: {msgs:?}",
    );
}

/// The pan-button click runs through the same core hit test the drag does.
#[test]
fn a_pan_button_click_on_the_drawn_core_deletes_it() {
    let mut ui = Simulator::new(offset_scene(OFFSET_VIEW, &[]));
    let drawn = drawn_at(OFFSET_ANCHOR_AT);
    ui.point_at(drawn);
    ui.simulate([moved(drawn), pan_press(), pan_release()]);

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::AnchorDeleted(OFFSET_ANCHOR)),
        "the drawn core must take the delete click: {msgs:?}",
    );
}

/// `on_anchor_create` reports a WORLD position - the host stores it as an
/// anchor position - so the widget origin has to be folded back out of it.
#[test]
fn a_created_anchor_is_reported_in_world_space() {
    let mut ui = Simulator::new(offset_scene(OFFSET_VIEW, &[]));
    let mid_run = drawn_at(Point::new(230.0, out_anchor().y));
    drag(&mut ui, mid_run, drawn_at(Point::new(500.0, 400.0)));

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::AnchorCreated(OFFSET_EDGE, Point::new(500.0, 400.0))),
        "a created anchor is reported where the host would store it: {msgs:?}",
    );
}

/// The wrap arc is drawn around the anchor's drawn position, so that is where
/// the wrap is grabbed - and pulling it that far off has to report the detach.
#[test]
fn a_wrap_is_grabbable_where_it_is_drawn() {
    let mut ui = Simulator::new(offset_scene(OFFSET_VIEW, &[OFFSET_ANCHOR]));
    let ring_bottom = drawn_at(wrap_point(OFFSET_ANCHOR_AT));
    drag(&mut ui, ring_bottom, drawn_at(Point::new(600.0, 500.0)));

    let msgs = messages(ui);
    assert!(
        msgs.contains(&Msg::RouteDetached(OFFSET_EDGE, OFFSET_ANCHOR)),
        "the drawn wrap must take the press and report the anchor it came off: {msgs:?}",
    );
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::Select(_))),
        "the wrap took the press, so no selection box opened: {msgs:?}",
    );
    assert!(
        !msgs.iter().any(|m| matches!(m, Msg::AnchorMove(..))),
        "the wrap took the press, so the core never moved: {msgs:?}",
    );
}

/// The origin and the zoom compose: the wrap of an off-origin graph is grabbed
/// where the two together draw it, and the gesture reports the same detach it
/// does at zoom 1.
#[test]
fn a_wrap_is_grabbable_where_it_is_drawn_at_any_zoom() {
    for camera in OFF_UNIT_VIEWS {
        let view = camera.below_toolbar();
        let zoom = view.zoom;
        let dropped_at = Point::new(400.0, 400.0);
        let mut ui = Simulator::new(offset_scene(view, &[OFFSET_ANCHOR]));
        drag(
            &mut ui,
            view.screen(wrap_point(OFFSET_ANCHOR_AT)),
            view.screen(dropped_at),
        );

        let msgs = messages(ui);
        assert!(
            msgs.contains(&Msg::RouteDetached(OFFSET_EDGE, OFFSET_ANCHOR)),
            "the drawn wrap must report the detach at zoom {zoom}: {msgs:?}",
        );
        assert!(
            msgs.contains(&Msg::AnchorCreated(OFFSET_EDGE, dropped_at)),
            "the pulled-off cable must leave an anchor at zoom {zoom}: {msgs:?}",
        );
        assert!(
            !msgs.iter().any(|m| matches!(m, Msg::AnchorMove(..))),
            "the wrap took the press, so the core never moved at zoom {zoom}: {msgs:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// Minimap
// ---------------------------------------------------------------------------

/// Where the default minimap's center lands in the 1024x768 root: 200x150 in
/// the bottom-right corner, 12 px off both edges.
const MAP_CENTER: Point = Point::new(1024.0 - 12.0 - 100.0, 768.0 - 12.0 - 75.0);

/// A graph with the default minimap enabled and the callbacks a map press must
/// not fire wired alongside the one it must.
fn minimap_graph(
    nodes: &[(usize, Point)],
    anchors: &[(usize, Point)],
) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .minimap(Minimap::default())
        .on_camera(Msg::Camera)
        .on_select(Msg::Select)
        .on_move(Msg::Move);
    for &(id, pos) in nodes {
        let body = container(text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H));
        ng = ng.push_node(node(id, pos, body));
    }
    for &(id, at) in anchors {
        ng = ng.push_anchor(anchor(id, at));
    }
    ng.into()
}

/// Whether the world point `p` lands inside the 1024x768 viewport under the
/// camera `(position, zoom)`: screen = (world + position) * zoom.
fn in_viewport(p: Point, position: Point, zoom: f32) -> bool {
    let on_screen = Point::new((p.x + position.x) * zoom, (p.y + position.y) * zoom);
    (0.0..=1024.0).contains(&on_screen.x) && (0.0..=768.0).contains(&on_screen.y)
}

/// The map's center is the center of what it shows, so a press there has to
/// bring the graph into view - here from a node 1000 px past the viewport's
/// far corner, which the default camera does not show at all.
#[test]
fn a_press_at_the_minimap_center_brings_the_graph_into_view() {
    const NEAR: Point = Point::new(100.0, 100.0);
    const FAR: Point = Point::new(2000.0, 1500.0);
    let mut ui = Simulator::new(minimap_graph(&[(0, NEAR), (1, FAR)], &[]));
    click(&mut ui, MAP_CENTER);

    let msgs = messages(ui);
    let (position, zoom) = last_camera(&msgs).expect("a map press must publish a camera");
    // The center of the graph's world bounds, mapped through the published
    // camera.
    let center = Point::new(
        (NEAR.x + FAR.x + NODE_W) / 2.0,
        (NEAR.y + FAR.y + NODE_H) / 2.0,
    );
    assert!(
        in_viewport(center, position, zoom),
        "the graph's center must land in the viewport, got camera {position:?} at zoom {zoom}",
    );
}

/// The map bounds what a frame-all would fit, anchors included: an anchor past
/// every node is on the map, so the map's center is the center of node and
/// anchor together. Bounded by nodes alone, the press would center on the
/// lone node and leave that point off screen.
#[test]
fn the_minimap_bounds_anchors_beyond_every_node() {
    const NEAR: Point = Point::new(100.0, 100.0);
    const FAR_ANCHOR: Point = Point::new(2000.0, 1500.0);
    let mut ui = Simulator::new(minimap_graph(&[(0, NEAR)], &[(ANCHOR_A, FAR_ANCHOR)]));
    click(&mut ui, MAP_CENTER);

    let msgs = messages(ui);
    let (position, zoom) = last_camera(&msgs).expect("a map press must publish a camera");
    let center = Point::new((NEAR.x + FAR_ANCHOR.x) / 2.0, (NEAR.y + FAR_ANCHOR.y) / 2.0);
    assert!(
        in_viewport(center, position, zoom),
        "the node-and-anchor center must land in the viewport, got camera {position:?} \
         at zoom {zoom}",
    );
}

/// The map draws over the canvas, so it must also take the press over it: a
/// node the map happens to cover is chrome's business, not the node's.
#[test]
fn a_press_on_the_minimap_never_reaches_the_node_beneath_it() {
    let covered = Point::new(MAP_CENTER.x - NODE_W / 2.0, MAP_CENTER.y - NODE_H / 2.0);
    let mut ui = Simulator::new(minimap_graph(&[(0, covered)], &[]));
    drag(
        &mut ui,
        MAP_CENTER,
        Point::new(MAP_CENTER.x + 20.0, MAP_CENTER.y + 10.0),
    );

    let msgs = messages(ui);
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, Msg::Move(..) | Msg::Select(_))),
        "the covered node must neither move nor be selected: {msgs:?}",
    );
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Camera(..))),
        "the press must still steer the camera: {msgs:?}",
    );
}
