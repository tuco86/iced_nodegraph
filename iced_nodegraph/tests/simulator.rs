//! High-level interaction tests driving NodeGraph through `iced_test::Simulator`.
//!
//! Unlike the recording-renderer tests in `src/coordinate_tests.rs` and
//! `src/clipping_tests.rs` (which assert on render geometry via a fake
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
use iced::{Element, Length, Point, Theme, Vector};
use iced::{keyboard, mouse};
use iced_nodegraph::{NodeGraph, PinRef, edge, node, pin};
use iced_test::Simulator;

type Renderer = iced::Renderer;
type Graph = NodeGraph<'static, usize, usize, (), Msg, Theme, Renderer>;
type Pin = PinRef<usize, usize>;

/// Captures every interaction callback the graph can emit.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(Vec<usize>),
    Move(Vector, Vec<usize>),
    Clone(Vec<usize>),
    Delete(Vec<usize>),
    Connect(Pin, Pin),
    Disconnect(Pin, Pin),
    Camera(Point, f32),
    Button,
    Input(String),
}

const NODE_W: f32 = 60.0;
const NODE_H: f32 = 30.0;

/// Builds a graph with one fixed-size node body per `(id, world-position)`,
/// every interaction callback wired into `Msg`.
fn graph_with(nodes: &[(usize, Point)]) -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_select(Msg::Select)
        .on_move(Msg::Move)
        .on_clone(Msg::Clone)
        .on_delete(Msg::Delete);
    for &(id, pos) in nodes {
        let body = container(iced::widget::text("n"))
            .width(Length::Fixed(NODE_W))
            .height(Length::Fixed(NODE_H));
        ng.push_node(node(id, pos, body));
    }
    ng.into()
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
    let mut ui = Simulator::new(graph_with(&[
        (0, Point::new(100.0, 100.0)),
        (1, Point::new(400.0, 100.0)),
    ]));
    click(&mut ui, center(Point::new(100.0, 100.0)));

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
    // Press+release far from any node performs an empty box select -> clears.
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
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
    ui.simulate([key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::default(),
    )]);
    assert_eq!(last_selection(&messages(ui)), Some(vec![]));
}

#[test]
fn box_select_grabs_enclosed_nodes() {
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
    let mut ui = Simulator::new(graph_with(&[
        (0, Point::new(100.0, 100.0)),
        (1, Point::new(400.0, 100.0)),
    ]));
    // Select both, then drag one of them: the move reports the whole group.
    ui.point_at(Point::new(500.0, 400.0));
    ui.simulate([key_pressed(keyboard::Key::Character("a".into()), cmd())]);
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
// Keyboard commands
// ---------------------------------------------------------------------------

#[test]
fn delete_key_requests_delete_of_selection() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
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

#[test]
fn ctrl_d_requests_clone_of_selection() {
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    click(&mut ui, center(Point::new(100.0, 100.0)));
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

fn pin_body() -> iced::widget::Container<'static, Msg, Theme, Renderer> {
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
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    ng.push_node(node(1usize, IN_POS, pin!(Left, 0usize, pin_body(), Input)));
    if seed_edge {
        ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));
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

#[test]
fn ctrl_click_on_edge_disconnects() {
    // Ctrl+click on the edge line (Fruit Ninja cut) disconnects it.
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

// ---------------------------------------------------------------------------
// Camera: right-drag pan and wheel zoom
// ---------------------------------------------------------------------------

fn camera_graph() -> Element<'static, Msg, Theme, Renderer> {
    let mut ng: Graph = NodeGraph::default()
        .width(Length::Fill)
        .height(Length::Fill)
        .on_pan(Msg::Camera);
    ng.push_node(node(
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
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    ng.push_node(node(1usize, IN_POS, pin!(Left, 0usize, pin_body(), Input)));
    ng.push_node(node(
        2usize,
        Point::new(IN_POS.x, 300.0),
        pin!(Left, 0usize, pin_body(), Input),
    ));
    ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));
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
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    ng.push_node(node(1usize, IN_POS, pin!(Left, 0usize, pin_body(), Input)));
    // Cover node 1 input anchor (IN_POS.x, IN_POS.y + H/2) with a plain body.
    ng.push_node(node(
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
        .on_pan(Msg::Camera);
    ng.push_node(node(
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
    ng.push_node(node(
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
    ng.push_node(node(
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
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    // Second output pin, anchored at IN_POS left edge for an easy drag target.
    ng.push_node(node(1usize, IN_POS, pin!(Left, 0usize, pin_body(), Output)));
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
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
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
        .view(CAM_POS, CAM_ZOOM)
        .on_connect(Msg::Connect)
        .on_disconnect(Msg::Disconnect);
    ng.push_node(node(
        0usize,
        OUT_POS,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    ng.push_node(node(1usize, IN_POS, pin!(Left, 0usize, pin_body(), Input)));
    if seed_edge {
        ng.push_edge(edge!(PinRef::new(0, 0), PinRef::new(1, 0)));
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
    let mut ui = Simulator::new(graph_with(&[(0, Point::new(100.0, 100.0))]));
    let c = center(Point::new(100.0, 100.0));
    click(&mut ui, c); // select node 0

    ui.point_at(c);
    ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::SHIFT,
    ))]);
    ui.simulate([moved(c), press(), release()]); // shift-click again -> toggle off

    assert_eq!(last_selection(&messages(ui)), Some(vec![]));
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
        .on_select(Msg::Select);
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
    ng.push_node(node(0usize, Point::new(432.0, 344.0), body));
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
