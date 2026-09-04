//! `Widget::mouse_interaction` tests: the graph is the only thing between the
//! runtime and its node content, so a cursor a child widget asks for reaches
//! the window only if the graph forwards the query - in the same
//! layout-absolute space the child was laid out and hit-tested in.
//!
//! The six guarantees, one test each:
//!
//! 1. a child under the cursor gets to set the cursor, at any zoom and widget
//!    origin,
//! 2. empty canvas claims nothing, so the host's own cursor stands,
//! 3. a cursor outside the graph claims nothing (a sibling in a `stack` owns
//!    it),
//! 4. an in-flight gesture owns the cursor regardless of what is under it,
//! 5. an anchor's and a cable's grabbable parts claim the grab cursor, and only
//!    while the gesture behind them is wired,
//! 6. hovering one asks for the frame its feedback is drawn in.
//!
//! Like the sibling recording-renderer tests these call the trait methods
//! directly: the guarantee lives in the returned value, not in pixels.

use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::widget::{container, text};
use iced::{Element, Length, Point, Rectangle, Size, Theme, Vector, window};
use iced_wgpu::core::clipboard;

use iced_nodegraph::{
    AnchorStatus, Indexed, NodeGraph, PinRef, anchor, default_anchor_style, edge, node, pin,
};

mod common;

use common::record::Recorder;

// ---------------------------------------------------------------------------
// A leaf node-content widget that claims `Pointer` while the cursor is over it
// (what `button`, `text_input` and friends do), and records the cursor it was
// handed so a failure distinguishes "not forwarded" from "forwarded in the
// wrong space".
// ---------------------------------------------------------------------------
struct InteractionProbe {
    cursor_seen: Rc<Cell<Option<Point>>>,
}

impl<Message> Widget<Message, Theme, Recorder> for InteractionProbe {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(PROBE_SIZE.width),
            Length::Fixed(PROBE_SIZE.height),
        )
    }
    fn layout(&mut self, _: &mut Tree, _: &Recorder, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(PROBE_SIZE.width),
            Length::Fixed(PROBE_SIZE.height),
            Size::ZERO,
        ))
    }
    fn draw(
        &self,
        _: &Tree,
        _: &mut Recorder,
        _: &Theme,
        _: &renderer::Style,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Recorder,
    ) -> mouse::Interaction {
        self.cursor_seen.set(cursor.position());
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message: 'a> From<InteractionProbe> for Element<'a, Message, Theme, Recorder> {
    fn from(w: InteractionProbe) -> Self {
        Element::new(w)
    }
}

const PROBE_SIZE: Size = Size::new(40.0, 20.0);
const VIEWPORT: Size = Size::new(1024.0, 768.0);
const GRAPH_SIZE: Size = Size::new(400.0, 400.0);

type Graph = NodeGraph<'static, Indexed, (), Recorder>;

/// A one-node graph placed at `origin` under the given camera, primed by one
/// no-op update so `camera()` has synced into the widget camera.
fn graph_with_probe(
    origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
    cursor_seen: Rc<Cell<Option<Point>>>,
    renderer: &Recorder,
) -> (Graph, Tree, layout::Node) {
    let mut graph: Graph = NodeGraph::default()
        .width(Length::Fixed(GRAPH_SIZE.width))
        .height(Length::Fixed(GRAPH_SIZE.height))
        .camera(camera_pos, camera_zoom);
    graph = graph.push_node(node(
        0usize,
        node_world,
        Element::from(InteractionProbe { cursor_seen }),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, VIEWPORT),
    );

    // One update syncs `camera()` into the widget camera (the host value differs
    // from the unset last-synced value); the event itself is a no-op, and the
    // cursor is parked off-widget so no gesture starts.
    let layout = Layout::with_offset(origin, &layout_node);
    drive(
        &mut graph,
        &mut tree,
        layout,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        mouse::Cursor::Unavailable,
        renderer,
    );

    (graph, tree, layout_node)
}

/// Feeds one event through `Widget::update`, discarding the messages, and
/// reports the redraw the graph asked for.
fn drive(
    graph: &mut Graph,
    tree: &mut Tree,
    layout: Layout<'_>,
    event: &iced::Event,
    cursor: mouse::Cursor,
    renderer: &Recorder,
) -> window::RedrawRequest {
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clip = clipboard::Null;
    graph.update(
        tree,
        event,
        layout,
        cursor,
        renderer,
        &mut clip,
        &mut shell,
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
    );
    shell.redraw_request()
}

/// The screen pixel a layout-absolute point is drawn at:
/// `origin + (world + camera_position) * zoom`.
fn to_screen(origin: Vector, world: Point, camera_pos: Point, zoom: f32) -> Point {
    Point::new(
        origin.x + (world.x + camera_pos.x) * zoom,
        origin.y + (world.y + camera_pos.y) * zoom,
    )
}

#[test]
fn child_under_cursor_sets_the_cursor() {
    // The crux: at zoom 2 and a non-zero widget origin, a screen cursor inside
    // the probe's rendered rect must reach the probe as the matching
    // layout-absolute point, and its `Pointer` must survive the trip back.
    let origin = Vector::new(40.0, 100.0);
    let world = Point::new(10.0, 20.0);
    let cam_pos = Point::new(15.0, -5.0);
    let zoom = 2.0;

    let cursor_seen = Rc::new(Cell::new(None));
    let renderer = Recorder::detached();
    let (graph, tree, layout_node) =
        graph_with_probe(origin, world, cam_pos, zoom, cursor_seen.clone(), &renderer);
    let layout = Layout::with_offset(origin, &layout_node);

    // Centre of the probe, in world coordinates, mapped to its screen pixel.
    let inside_world = Point::new(
        world.x + PROBE_SIZE.width / 2.0,
        world.y + PROBE_SIZE.height / 2.0,
    );
    let screen = to_screen(origin, inside_world, cam_pos, zoom);

    let interaction = graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(screen),
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
        &renderer,
    );

    let seen = cursor_seen
        .get()
        .expect("the graph must forward the query to the node under the cursor");
    let expected = Point::new(origin.x + inside_world.x, origin.y + inside_world.y);
    assert!(
        (seen.x - expected.x).abs() < 0.5 && (seen.y - expected.y).abs() < 0.5,
        "probe was handed {seen:?}, expected layout-absolute {expected:?}",
    );
    assert_eq!(
        interaction,
        mouse::Interaction::Pointer,
        "the child's cursor must be what the graph reports",
    );
}

#[test]
fn empty_canvas_claims_nothing() {
    let origin = Vector::new(40.0, 100.0);
    let world = Point::new(10.0, 20.0);
    let cam_pos = Point::ORIGIN;
    let zoom = 1.0;

    let cursor_seen = Rc::new(Cell::new(None));
    let renderer = Recorder::detached();
    let (graph, tree, layout_node) =
        graph_with_probe(origin, world, cam_pos, zoom, cursor_seen.clone(), &renderer);
    let layout = Layout::with_offset(origin, &layout_node);

    // Inside the graph, far from the only node.
    let screen = to_screen(origin, Point::new(300.0, 300.0), cam_pos, zoom);
    let interaction = graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(screen),
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
        &renderer,
    );

    assert_eq!(
        interaction,
        mouse::Interaction::None,
        "bare canvas must leave the cursor to the host",
    );
    assert!(
        cursor_seen.get().is_none(),
        "a node the cursor is not over must not be queried",
    );
}

#[test]
fn cursor_outside_the_graph_claims_nothing() {
    let origin = Vector::new(40.0, 100.0);
    let renderer = Recorder::detached();
    let (graph, tree, layout_node) = graph_with_probe(
        origin,
        Point::new(10.0, 20.0),
        Point::ORIGIN,
        1.0,
        Rc::new(Cell::new(None)),
        &renderer,
    );
    let layout = Layout::with_offset(origin, &layout_node);

    // Just left of the widget origin: inside the window, outside the graph.
    let screen = Point::new(origin.x - 5.0, origin.y + 10.0);
    let interaction = graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(screen),
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
        &renderer,
    );

    assert_eq!(
        interaction,
        mouse::Interaction::None,
        "outside its own bounds the graph must not claim the cursor",
    );
}

#[test]
fn pan_drag_claims_the_grabbing_cursor() {
    let origin = Vector::new(40.0, 100.0);
    let renderer = Recorder::detached();
    let (mut graph, mut tree, layout_node) = graph_with_probe(
        origin,
        Point::new(10.0, 20.0),
        Point::ORIGIN,
        1.0,
        Rc::new(Cell::new(None)),
        &renderer,
    );
    let layout = Layout::with_offset(origin, &layout_node);

    // Press the default pan button (right) over empty canvas.
    let press = Point::new(origin.x + 300.0, origin.y + 300.0);
    drive(
        &mut graph,
        &mut tree,
        layout,
        &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        mouse::Cursor::Available(press),
        &renderer,
    );

    // Dragged well past the graph: the gesture keeps the cursor anyway.
    let dragged = Point::new(origin.x - 200.0, origin.y - 90.0);
    let interaction = graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(dragged),
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
        &renderer,
    );

    assert_eq!(
        interaction,
        mouse::Interaction::Grabbing,
        "a pan in flight must show the grabbing cursor",
    );
}

/// The probe cases pin the wiring; this one pins the outcome a user sees, with
/// a real iced widget whose `mouse_interaction` the graph does not control:
/// hovering a `text_input` inside a node must reach the window as an I-beam,
/// at zoom, from a graph that is not at the window origin.
#[test]
fn a_text_input_in_a_node_asks_for_the_text_cursor() {
    let origin = Vector::new(40.0, 100.0);
    let world = Point::new(10.0, 20.0);
    let zoom = 2.0;

    let renderer = Recorder::detached();
    let mut graph: Graph = NodeGraph::default()
        .width(Length::Fixed(GRAPH_SIZE.width))
        .height(Length::Fixed(GRAPH_SIZE.height))
        .camera(Point::ORIGIN, zoom);
    graph = graph.push_node(node(
        0usize,
        world,
        Element::from(iced_widget::text_input::<(), Theme, Recorder>("", "hello").on_input(|_| ())),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, VIEWPORT),
    );
    let layout = Layout::with_offset(origin, &layout_node);
    drive(
        &mut graph,
        &mut tree,
        layout,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        mouse::Cursor::Unavailable,
        &renderer,
    );

    // A point a few layout px inside the input, mapped to its screen pixel.
    let inside = Point::new(world.x + 5.0, world.y + 5.0);
    let screen = to_screen(origin, inside, Point::ORIGIN, zoom);
    let interaction = graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(screen),
        &Rectangle::new(Point::ORIGIN, VIEWPORT),
        &renderer,
    );

    assert_eq!(
        interaction,
        mouse::Interaction::Text,
        "hovering a text input inside a node must report the text cursor",
    );
}

// ---------------------------------------------------------------------------
// The graph's own grabbable parts: an anchor core, and the stretches of a cable
// a press takes hold of. Node A's Right/Output pin lands at (60, 50) and node
// B's Left/Input pin at (300, 50); the anchor sits 110 px below that run, so
// the cable routed through it leaves the straight line and wraps orbit 0.
// ---------------------------------------------------------------------------

const CABLE_NODE_A: Point = Point::new(20.0, 40.0);
const CABLE_NODE_B: Point = Point::new(300.0, 40.0);
const CABLE_ANCHOR: usize = 9;
const CABLE_ANCHOR_AT: Point = Point::new(160.0, 160.0);
/// Radius of orbit 0, read from the style the widget resolves it from.
fn orbit_0() -> f32 {
    default_anchor_style(&Theme::Dark, AnchorStatus::Idle).orbit_offset
}

/// The lowest point of the anchor's orbit 0. The cable wraps the side of the
/// ring away from the pin-to-pin run, so this point lies on the arc it draws.
fn wrap_probe() -> Point {
    Point::new(CABLE_ANCHOR_AT.x, CABLE_ANCHOR_AT.y + orbit_0())
}

/// A point on the cable 100 px along it: past the 24 px end zone at the output
/// pin, and 55 px short of the wrap.
fn run_probe() -> Point {
    Point::new(126.0, 100.0)
}

/// Bare canvas, clear of every node, cable and ring in the scene.
fn empty_probe() -> Point {
    Point::new(350.0, 350.0)
}

fn pin_body() -> iced::widget::Container<'static, (), Theme, Recorder> {
    container(text("p"))
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(20.0))
}

/// One cable routed through one anchor under the given camera, primed by a
/// no-op update so `camera()` has synced into the widget camera. `wired` decides
/// whether the anchor and route gestures exist at all.
fn graph_with_cable(
    renderer: &Recorder,
    wired: bool,
    camera_pos: Point,
    camera_zoom: f32,
) -> (Graph, Tree, layout::Node) {
    let mut graph: Graph = NodeGraph::default()
        .width(Length::Fixed(GRAPH_SIZE.width))
        .height(Length::Fixed(GRAPH_SIZE.height))
        .camera(camera_pos, camera_zoom);
    if wired {
        graph = graph
            .on_anchor_move(|_, _| ())
            .on_anchor_create(|_, _| ())
            .on_route_attach(|_, _| ())
            .on_route_detach(|_, _| ());
    }
    graph = graph.push_node(node(
        0usize,
        CABLE_NODE_A,
        pin!(Right, 0usize, pin_body(), Output),
    ));
    graph = graph.push_node(node(
        1usize,
        CABLE_NODE_B,
        pin!(Left, 0usize, pin_body(), Input),
    ));
    graph = graph.push_anchor(anchor(CABLE_ANCHOR, CABLE_ANCHOR_AT));
    graph = graph.push_edge(edge((), PinRef::new(0, 0), PinRef::new(1, 0)).route([CABLE_ANCHOR]));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, VIEWPORT),
    );
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);
    drive(
        &mut graph,
        &mut tree,
        layout,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        mouse::Cursor::Unavailable,
        renderer,
    );
    (graph, tree, layout_node)
}

/// The graph's own grabbable parts announce themselves before the click: the
/// core a press would move, the wrap it would pull off the anchor, and the run
/// it would put a new anchor on.
///
/// And only those - an unwired gesture leaves its zone inert, so the cursor
/// must not promise a grab the host could never apply.
#[test]
fn an_anchors_grabbable_parts_offer_a_grab() {
    let renderer = Recorder::detached();

    let ask = |wired: bool, screen: Point| {
        let (graph, tree, layout_node) = graph_with_cable(&renderer, wired, Point::ORIGIN, 1.0);
        graph.mouse_interaction(
            &tree,
            Layout::with_offset(Vector::ZERO, &layout_node),
            mouse::Cursor::Available(screen),
            &Rectangle::new(Point::ORIGIN, VIEWPORT),
            &renderer,
        )
    };

    for (what, at) in [
        ("the anchor core", CABLE_ANCHOR_AT),
        ("the wrap", wrap_probe()),
        ("the cable's run", run_probe()),
    ] {
        assert_eq!(
            ask(true, at),
            mouse::Interaction::Grab,
            "{what} must offer a grab",
        );
        assert_eq!(
            ask(false, at),
            mouse::Interaction::None,
            "{what} must claim nothing while its gesture is unwired",
        );
    }

    assert_eq!(
        ask(true, empty_probe()),
        mouse::Interaction::None,
        "bare canvas claims nothing",
    );
}

/// The same three parts through a camera that is not the identity: every grab
/// target here is a screen-space distance, so the part drawn under the cursor
/// is the one that answers, and the zoom only decides which pixel that is.
#[test]
fn the_grabbable_parts_offer_a_grab_at_any_zoom() {
    let renderer = Recorder::detached();

    for zoom in [0.5, 2.0] {
        let ask = |world: Point| {
            let (graph, tree, layout_node) = graph_with_cable(&renderer, true, Point::ORIGIN, zoom);
            graph.mouse_interaction(
                &tree,
                Layout::with_offset(Vector::ZERO, &layout_node),
                mouse::Cursor::Available(to_screen(Vector::ZERO, world, Point::ORIGIN, zoom)),
                &Rectangle::new(Point::ORIGIN, VIEWPORT),
                &renderer,
            )
        };

        for (what, at) in [
            ("the anchor core", CABLE_ANCHOR_AT),
            ("the wrap", wrap_probe()),
            ("the cable's run", run_probe()),
        ] {
            assert_eq!(
                ask(at),
                mouse::Interaction::Grab,
                "{what} must offer a grab at zoom {zoom}",
            );
        }
    }
}

/// Hover feedback keeps no state - `draw` resolves it from the geometry it
/// strokes - so all it needs from `update` is a FRAME to be drawn in. An idle
/// graph asks for none, which is why moving onto a grabbable part has to ask,
/// and moving off it has to ask for the frame that clears what was drawn.
#[test]
fn hovering_an_anchor_asks_for_a_frame() {
    let renderer = Recorder::detached();
    let (mut graph, mut tree, layout_node) = graph_with_cable(&renderer, true, Point::ORIGIN, 1.0);
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);

    let move_to = |graph: &mut Graph, tree: &mut Tree, to: Point| {
        drive(
            graph,
            tree,
            layout,
            &iced::Event::Mouse(mouse::Event::CursorMoved { position: to }),
            mouse::Cursor::Available(to),
            &renderer,
        )
    };

    let onto = move_to(&mut graph, &mut tree, CABLE_ANCHOR_AT);
    let leaving = move_to(&mut graph, &mut tree, empty_probe());
    let clear = move_to(&mut graph, &mut tree, Point::new(360.0, 360.0));

    assert_ne!(
        onto,
        window::RedrawRequest::Wait,
        "moving onto the core must ask for the frame its feedback draws in",
    );
    assert_ne!(
        leaving,
        window::RedrawRequest::Wait,
        "moving off the core must ask for the frame that clears it",
    );
    assert_eq!(
        clear,
        window::RedrawRequest::Wait,
        "moving over bare canvas must leave the graph idle",
    );
}
