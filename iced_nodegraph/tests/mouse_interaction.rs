//! `Widget::mouse_interaction` tests: the graph is the only thing between the
//! runtime and its node content, so a cursor a child widget asks for reaches
//! the window only if the graph forwards the query - in the same
//! layout-absolute space the child was laid out and hit-tested in.
//!
//! The four guarantees, one test each:
//!
//! 1. a child under the cursor gets to set the cursor, at any zoom and widget
//!    origin,
//! 2. empty canvas claims nothing, so the host's own cursor stands,
//! 3. a cursor outside the graph claims nothing (a sibling in a `stack` owns
//!    it),
//! 4. an in-flight gesture owns the cursor regardless of what is under it.
//!
//! Like the sibling recording-renderer tests these call the trait methods
//! directly: the guarantee lives in the returned `Interaction`, not in pixels.

use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::{Element, Length, Point, Rectangle, Size, Theme, Vector};
use iced_wgpu::core::clipboard;

use iced_nodegraph::{NodeGraph, node};

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

type Graph = NodeGraph<'static, usize, usize, (), (), Recorder>;

/// A one-node graph placed at `origin` under the given camera, primed by one
/// no-op update so `view()` has synced into the widget camera.
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
        .view(camera_pos, camera_zoom);
    graph.push_node(node(
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

    // One update syncs `view()` into the widget camera (the host value differs
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

/// Feeds one event through `Widget::update`, discarding the messages.
fn drive(
    graph: &mut Graph,
    tree: &mut Tree,
    layout: Layout<'_>,
    event: &iced::Event,
    cursor: mouse::Cursor,
    renderer: &Recorder,
) {
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
        .view(Point::ORIGIN, zoom);
    graph.push_node(node(
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
