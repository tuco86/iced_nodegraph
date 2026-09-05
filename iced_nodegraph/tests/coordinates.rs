//! Coordinate-consistency tests for a NodeGraph placed at a non-zero widget
//! origin (e.g. below a toolbar in a `column!`).
//!
//! The widget must render its SDF layers (node fill, pins, drag preview) and
//! its Iced child content at the SAME screen position, and that position must
//! be `widget_origin + (world + camera_position) * zoom`. Historically the SDF
//! path double-counted the widget origin, shifting the fill/pins down by the
//! toolbar height relative to the content.
//!
//! These tests use a recording renderer instead of a real GPU: they reconstruct
//! the absolute screen position of drawn content from the transformation stack
//! (matching iced_graphics' `current * transformation` composition) and capture
//! the bounds handed to `draw_primitive` (the SDF clip rect, in absolute pixels).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::keyboard;
use iced::touch;
use iced::widget::Id;
use iced::{Background, Color, Element, Length, Point, Rectangle, Size, Theme, Vector};
use iced_wgpu::core::clipboard;

use iced_nodegraph::{
    AnchorStatus, Easing, FocusAnimation, FocusOptions, FocusTarget, Indexed, NodeGraph, PinRef,
    anchor, default_anchor_style, edge, focus_operation, node,
};

mod common;

use common::record::{DrawEvent, Recorded, Recorder};

// ---------------------------------------------------------------------------
// A leaf node-content widget that paints one fill_quad covering its bounds, so
// the recorder captures the absolute screen position of the node's content.
// ---------------------------------------------------------------------------
struct ContentProbe;

impl<Message> Widget<Message, Theme, Recorder> for ContentProbe {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(40.0), Length::Fixed(20.0))
    }
    fn layout(&mut self, _: &mut Tree, _: &Recorder, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(40.0), Length::Fixed(20.0), Size::ZERO))
    }
    fn draw(
        &self,
        _: &Tree,
        renderer: &mut Recorder,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Background::Color(Color::BLACK),
        );
    }
}

impl<'a, Message: 'a> From<ContentProbe> for Element<'a, Message, Theme, Recorder> {
    fn from(w: ContentProbe) -> Self {
        Element::new(w)
    }
}

/// The graph shape every case here builds: default ids, no pin payload, the
/// recording renderer.
type Graph<Msg> = NodeGraph<'static, Indexed, Msg, Theme, Recorder>;

/// Lays out a single-node graph, places it at `widget_origin`, applies the
/// given camera (zoom, world position), draws it, and returns the recorded
/// content quad and SDF primitive bounds.
fn draw_at_origin(
    widget_origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
) -> Recorded {
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(camera_pos, camera_zoom);
    graph = graph.push_node(node(0_usize, node_world, Element::from(ContentProbe)));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());

    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    // Simulate the graph sitting at a non-zero origin (e.g. below a toolbar).
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    // One update syncs `camera()` into the widget camera (the host value differs
    // from the unset last-synced value); the event itself is a no-op here.
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut clipboard,
        &mut shell,
        &viewport,
    );

    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport,
    );

    out.borrow().clone()
}

/// Where a node at `world` must land on screen for a graph at `origin` with the
/// given camera: `origin + (world + position) * zoom`.
fn expected_screen(origin: Vector, world: Point, pos: Point, zoom: f32) -> Point {
    Point::new(
        origin.x + (world.x + pos.x) * zoom,
        origin.y + (world.y + pos.y) * zoom,
    )
}

/// The small node-content quad (not the full-area background quad).
fn node_content_quad(rec: &Recorded) -> Rectangle {
    // The background fills the whole 400x400 area (unscaled); the node content
    // is the small 40x20 probe, scaled by zoom. Anything well under the
    // background size is the node.
    rec.quads
        .iter()
        .copied()
        .find(|q| q.width <= 200.0 && q.height <= 200.0)
        .expect("node content quad was not recorded")
}

/// The node-fill SDF primitive (small), not the full-area layers.
fn node_fill_primitive(rec: &Recorded) -> Rectangle {
    rec.primitives
        .iter()
        .copied()
        .find(|p| p.width <= 120.0 && p.height <= 120.0)
        .expect("node fill primitive was not recorded")
}

/// Presses the left mouse button at `screen` over a single-node graph placed at
/// `widget_origin` with the given camera, and returns the selection emitted by
/// `on_select` (if any). Verifies hit-testing maps screen -> the correct node.
fn click_select(
    widget_origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
    screen: Point,
) -> Option<Vec<usize>> {
    let selected: Rc<RefCell<Option<Vec<usize>>>> = Rc::new(RefCell::new(None));
    let sel = selected.clone();

    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(camera_pos, camera_zoom)
        .on_select(move |ids| {
            *sel.borrow_mut() = Some(ids);
        });
    graph = graph.push_node(node(0_usize, node_world, Element::from(ContentProbe)));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let renderer = Recorder::new(out);
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    let cursor = mouse::Cursor::Available(screen);

    // First a CursorMoved so the widget syncs `camera()` and tracks the cursor,
    // then the press that performs the hit-test and selection.
    for event in [
        iced::Event::Mouse(mouse::Event::CursorMoved { position: screen }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    ] {
        graph.update(
            &mut tree,
            &event,
            layout,
            cursor,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
    }

    selected.borrow().clone()
}

#[test]
fn click_hits_node_at_nonzero_origin_zoom1() {
    // Node body spans world (30,40)..(70,60), center (50,50). Graph at (0,100).
    // Screen center at zoom 1 = origin + center = (50, 150).
    let origin = Vector::new(0.0, 100.0);
    let selected = click_select(
        origin,
        Point::new(30.0, 40.0),
        Point::ORIGIN,
        1.0,
        Point::new(50.0, 150.0),
    );
    assert_eq!(
        selected,
        Some(vec![0]),
        "click at the node's screen center must select it (origin {origin:?}, zoom 1)",
    );
}

#[test]
fn click_hits_node_at_nonzero_origin_zoom2() {
    // Same node; at zoom 2 the screen center = origin + center*2 = (100, 200).
    let origin = Vector::new(0.0, 100.0);
    let selected = click_select(
        origin,
        Point::new(30.0, 40.0),
        Point::ORIGIN,
        2.0,
        Point::new(100.0, 200.0),
    );
    assert_eq!(
        selected,
        Some(vec![0]),
        "click at the node's screen center must select it (origin {origin:?}, zoom 2)",
    );
}

// Antialias padding the fill clip adds around the node bbox.
const FILL_PAD: f32 = 6.0;

#[test]
fn content_and_fill_correct_at_origin_zoom1() {
    // Graph at (0, 100); node world (30, 40); default camera. Both content and
    // fill must land at origin + world = (30, 140).
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let rec = draw_at_origin(origin, world, Point::ORIGIN, 1.0);
    let expected = expected_screen(origin, world, Point::ORIGIN, 1.0);

    let content = node_content_quad(&rec);
    let fill = node_fill_primitive(&rec);

    assert!(
        (content.x - expected.x).abs() < 1.0 && (content.y - expected.y).abs() < 1.0,
        "content {content:?} should sit at {expected:?}",
    );
    assert!(
        (fill.x - expected.x).abs() < FILL_PAD && (fill.y - expected.y).abs() < FILL_PAD,
        "fill {fill:?} should sit at {expected:?}",
    );
}

#[test]
fn content_correct_at_origin_zoom2() {
    // The crux: at zoom != 1 with a non-zero widget origin, content must land at
    // origin + (world + pos) * zoom, NOT zoom * (origin + world + pos).
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let zoom = 2.0;
    let rec = draw_at_origin(origin, world, Point::ORIGIN, zoom);
    let expected = expected_screen(origin, world, Point::ORIGIN, zoom);

    let content = node_content_quad(&rec);
    assert!(
        (content.x - expected.x).abs() < 1.0 && (content.y - expected.y).abs() < 1.0,
        "content {content:?} should sit at {expected:?} at zoom {zoom} \
         with widget origin {origin:?}",
    );
}

#[test]
fn fill_correct_at_origin_zoom2() {
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let zoom = 2.0;
    let rec = draw_at_origin(origin, world, Point::ORIGIN, zoom);
    let expected = expected_screen(origin, world, Point::ORIGIN, zoom);

    let fill = node_fill_primitive(&rec);
    assert!(
        (fill.x - expected.x).abs() < FILL_PAD && (fill.y - expected.y).abs() < FILL_PAD,
        "fill {fill:?} should sit at {expected:?} at zoom {zoom} \
         with widget origin {origin:?}",
    );
}

#[test]
fn content_and_fill_coincide_at_origin_zoom2() {
    // Regardless of correctness vs. world, the two layers must not drift apart.
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let rec = draw_at_origin(origin, world, Point::ORIGIN, 2.0);

    let content = node_content_quad(&rec);
    let fill = node_fill_primitive(&rec);
    let dx = (content.x - fill.x).abs();
    let dy = (content.y - fill.y).abs();
    assert!(
        dx < FILL_PAD && dy < FILL_PAD,
        "content {content:?} and fill {fill:?} diverge (dx={dx}, dy={dy})",
    );
}

/// Drags a selection box from screen `p1` to `p2` over empty graph space (the only
/// node is far away) and returns the SDF primitives recorded by a final draw.
fn selection_box_primitives(
    widget_origin: Vector,
    camera_zoom: f32,
    p1: Point,
    p2: Point,
) -> Vec<Rectangle> {
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(Point::ORIGIN, camera_zoom)
        .on_select(|_ids| {});
    // Node far from the drag so the press opens a selection box, not a node click.
    graph = graph.push_node(node(
        0_usize,
        Point::new(900.0, 900.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;

    let send = |graph: &mut Graph<()>,
                tree: &mut Tree,
                shell: &mut iced_wgpu::core::Shell<'_, ()>,
                clipboard: &mut clipboard::Null,
                renderer: &Recorder,
                event: iced::Event,
                at: Point| {
        graph.update(
            tree,
            &event,
            layout,
            mouse::Cursor::Available(at),
            renderer,
            clipboard,
            shell,
            &viewport,
        );
    };

    // Move to p1, press (opens the selection box at p1), drag to p2.
    send(
        &mut graph,
        &mut tree,
        &mut shell,
        &mut clipboard,
        &renderer,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: p1 }),
        p1,
    );
    send(
        &mut graph,
        &mut tree,
        &mut shell,
        &mut clipboard,
        &renderer,
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        p1,
    );
    send(
        &mut graph,
        &mut tree,
        &mut shell,
        &mut clipboard,
        &renderer,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: p2 }),
        p2,
    );

    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Available(p2),
        &viewport,
    );

    out.borrow().primitives.clone()
}

#[test]
fn selection_box_renders_where_dragged_at_nonzero_origin() {
    // The selection box must render at the screen rectangle the user dragged,
    // regardless of widget origin or zoom. Box corners map back to the cursor
    // screen positions, so the select clip should span p1..p2 (plus AA padding).
    let origin = Vector::new(0.0, 100.0);
    let p1 = Point::new(40.0, 160.0);
    let p2 = Point::new(120.0, 240.0);
    let prims = selection_box_primitives(origin, 2.0, p1, p2);

    let expect = Rectangle::new(p1, Size::new(p2.x - p1.x, p2.y - p1.y));
    // The far node's layers sit elsewhere; find the primitive near the drag rect.
    let found = prims.iter().any(|r| {
        (r.x - expect.x).abs() < 8.0
            && (r.y - expect.y).abs() < 8.0
            && (r.width - expect.width).abs() < 12.0
            && (r.height - expect.height).abs() < 12.0
    });
    assert!(
        found,
        "no selection-box primitive near dragged rect {expect:?}; got {prims:?}",
    );
}

// ---------------------------------------------------------------------------
// SDF culling: a node whose screen bounds fall entirely outside the graph must
// not emit its fill/border/pin primitives (clipped_shape_bounds returns None).
// The shadow batch is intentionally NOT per-node culled (it clips to the whole
// graph), so it surfaces as a full-area (~400x400) primitive; assertions target
// the small node-fill-sized primitive specifically.
// ---------------------------------------------------------------------------

/// The small node-fill/border SDF primitive, if one was recorded. Unlike
/// `node_fill_primitive` this does not panic when the node was culled, and the
/// full-area shadow primitive is excluded by the size filter.
fn find_node_fill(rec: &Recorded) -> Option<Rectangle> {
    rec.primitives
        .iter()
        .copied()
        .find(|p| p.width <= 120.0 && p.height <= 120.0)
}

#[test]
fn node_far_offscreen_culls_sdf() {
    // Node at world (900, 900); graph is 400x400 at the origin -> entirely past
    // the right/bottom edge.
    let rec = draw_at_origin(Vector::ZERO, Point::new(900.0, 900.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&rec).is_none(),
        "a node entirely off-screen must not emit a fill primitive: {:?}",
        rec.primitives,
    );
}

#[test]
fn node_offscreen_negative_culls_sdf() {
    // Node spanning (-200,-200)..(-160,-180): off the top-left with no overlap.
    let rec = draw_at_origin(Vector::ZERO, Point::new(-200.0, -200.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&rec).is_none(),
        "a node off the top-left must be culled: {:?}",
        rec.primitives,
    );
}

#[test]
fn node_onscreen_emits_sdf() {
    // Control: a node well inside the graph emits its fill primitive.
    let rec = draw_at_origin(Vector::ZERO, Point::new(100.0, 100.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&rec).is_some(),
        "an on-screen node must emit a fill primitive: {:?}",
        rec.primitives,
    );
}

#[test]
fn node_straddling_right_edge_clips_to_bounds() {
    // Node at world x=380 (graph 400 wide) spans 380..420, 20px past the edge.
    // The fill clip is intersected with the graph bounds, never past x=400.
    let rec = draw_at_origin(Vector::ZERO, Point::new(380.0, 100.0), Point::ORIGIN, 1.0);
    let fill = find_node_fill(&rec).expect("a straddling node still emits a clipped fill");
    assert!(
        fill.x + fill.width <= 400.5,
        "fill clip {fill:?} must not extend past the graph right edge (400)",
    );
}

#[test]
fn node_barely_onscreen_not_culled() {
    // Node at world x=399 (graph 400) overlaps the graph by ~1px -> kept.
    let rec = draw_at_origin(Vector::ZERO, Point::new(399.0, 100.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&rec).is_some(),
        "a 1px overlap must keep the node's fill: {:?}",
        rec.primitives,
    );
}

#[test]
fn pan_culls_then_restores_sdf() {
    // Same node world position; only the camera pan differs. screen_x =
    // (world + pos) * zoom. world=100, zoom=1: pos.x=400 -> screen 500 (off the
    // 400-wide graph, culled); pos.x=0 -> screen 100 (on-screen, emitted).
    let off = draw_at_origin(
        Vector::ZERO,
        Point::new(100.0, 100.0),
        Point::new(400.0, 0.0),
        1.0,
    );
    assert!(
        find_node_fill(&off).is_none(),
        "panning the node off-screen must cull its fill: {:?}",
        off.primitives,
    );
    let on = draw_at_origin(Vector::ZERO, Point::new(100.0, 100.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&on).is_some(),
        "panning the node back on-screen must emit its fill again",
    );
}

#[test]
fn culling_holds_under_zoom() {
    // Under zoom the screen bounds grow: a node at world (250, 250) sits inside
    // the 400px graph at zoom 1, but at zoom 2 its top-left maps to screen
    // (500, 500) -- off-screen -> culled.
    let on = draw_at_origin(Vector::ZERO, Point::new(250.0, 250.0), Point::ORIGIN, 1.0);
    assert!(
        find_node_fill(&on).is_some(),
        "node at (250,250) must be visible at zoom 1: {:?}",
        on.primitives,
    );
    let off = draw_at_origin(Vector::ZERO, Point::new(250.0, 250.0), Point::ORIGIN, 2.0);
    assert!(
        find_node_fill(&off).is_none(),
        "the same node must cull once zoom pushes it off-screen: {:?}",
        off.primitives,
    );
}

// ---------------------------------------------------------------------------
// Recipe-hash stability (R4 / keystone). THE highest-risk unvalidated
// assumption behind the SDF v3 rewrite: that an unchanged node emits a
// byte-identical geometry recipe across frames. Node geometry is built from
// `node_layout.bounds()`; if iced layout jittered sub-ULP, or if any node
// geometry still depended on `time` (the pin-cutout pulse, now removed), the
// recipe would differ frame-to-frame and dedup / arena / instancing would all
// collapse. Driving the real widget's draw path through iced layout for 120
// frames while wall-clock `time` advances, the per-node geometry fingerprint
// (the SDF clip bounds, which are a pure function of the recipe operands) must
// stay identical. This is the gate that must hold before any arena work.
// ---------------------------------------------------------------------------

/// Bit-exact fingerprint of a recorded frame's geometry: every SDF primitive
/// clip rect and content quad, serialized as raw `f32` bits (so -0.0 != 0.0 and
/// NaN payloads are caught, per the native-vs-wasm hash contract).
fn geometry_fingerprint(rec: &Recorded) -> Vec<u32> {
    let mut bits = Vec::new();
    let mut push = |r: &Rectangle| {
        bits.extend_from_slice(&[
            r.x.to_bits(),
            r.y.to_bits(),
            r.width.to_bits(),
            r.height.to_bits(),
        ]);
    };
    for r in &rec.primitives {
        push(r);
    }
    for r in &rec.quads {
        push(r);
    }
    bits
}

#[test]
fn recipe_hash_is_stable_across_120_frames() {
    // A static three-node graph (no edges, so only node geometry is under test).
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(Point::ORIGIN, 1.0);
    for (i, p) in [(30.0, 40.0), (140.0, 90.0), (60.0, 220.0)]
        .into_iter()
        .enumerate()
    {
        graph = graph.push_node(node(i, Point::new(p.0, p.1), Element::from(ContentProbe)));
    }

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        &Recorder::new(Rc::new(RefCell::new(Recorded::default()))),
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut reference: Option<Vec<u32>> = None;
    for frame in 0..120 {
        let out = Rc::new(RefCell::new(Recorded::default()));
        let mut renderer = Recorder::new(out.clone());

        // A no-op cursor move per frame both syncs `camera()` and lets the widget
        // advance its wall-clock animation time, so `time` genuinely varies
        // across the 120 frames while the geometry must not.
        let mut msgs: Vec<()> = Vec::new();
        let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
        let mut clipboard = clipboard::Null;
        graph.update(
            &mut tree,
            &iced::Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(-1.0, -1.0),
            }),
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        graph.draw(
            &tree,
            &mut renderer,
            &Theme::Dark,
            &renderer::Style {
                text_color: Color::WHITE,
            },
            layout,
            mouse::Cursor::Unavailable,
            &viewport,
        );

        let fp = geometry_fingerprint(&out.borrow());
        assert!(
            !fp.is_empty(),
            "frame {frame} recorded no geometry; the harness drew nothing",
        );
        match &reference {
            None => reference = Some(fp),
            Some(r) => assert!(
                *r == fp,
                "node geometry recipe changed on frame {frame}: the recipe is \
                 not hash-stable, so dedup/arena/instancing cannot be trusted",
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Host-integration sandwich order. Hosted iced content interleaves BETWEEN a
// node's SDF layers: per node, in z-order, the stack is [SDF fill,
// element.draw() content, SDF border+pins], and a later node sits entirely
// above an earlier one. This is why the SDF substrate canNOT be flattened into
// one foreground pass under v3 (the per-node `with_layer` fences must stay).
// Driving the full widget draw path, the unified draw-call stream must show,
// for every node's content quad, an SDF layer immediately before AND after it.
// ---------------------------------------------------------------------------

/// Node-content events are the small probe quads (40x20 scaled), distinct from
/// the full-area background quad. Returns their indices in the event stream.
fn content_event_indices(events: &[DrawEvent]) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            DrawEvent::Content(r) if r.width <= 200.0 && r.height <= 200.0 => Some(i),
            _ => None,
        })
        .collect()
}

#[test]
fn hosted_content_sandwiched_between_sdf_layers() {
    // Two nodes, both well on-screen so neither fill nor foreground is culled.
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(Point::ORIGIN, 1.0);
    graph = graph.push_node(node(
        0_usize,
        Point::new(40.0, 40.0),
        Element::from(ContentProbe),
    ));
    graph = graph.push_node(node(
        1_usize,
        Point::new(180.0, 180.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut clipboard,
        &mut shell,
        &viewport,
    );
    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport,
    );

    let rec = out.borrow();
    let content = content_event_indices(&rec.events);
    assert_eq!(
        content.len(),
        2,
        "expected one content quad per node, got {}: {:?}",
        content.len(),
        rec.events,
    );

    for &i in &content {
        assert!(
            i > 0 && matches!(rec.events[i - 1], DrawEvent::Sdf(_)),
            "node content at event {i} is not preceded by its SDF fill: {:?}",
            rec.events,
        );
        assert!(
            i + 1 < rec.events.len() && matches!(rec.events[i + 1], DrawEvent::Sdf(_)),
            "node content at event {i} is not followed by its SDF foreground: {:?}",
            rec.events,
        );
    }

    // The two nodes' stacks do not collapse into each other: node 0's
    // foreground SDF paints before node 1's content (z-order interleave).
    assert!(
        content[0] + 1 < content[1],
        "an SDF layer must separate node 0's content from node 1's: {:?}",
        rec.events,
    );
}

#[test]
fn a_frame_draws_behind_every_plain_node() {
    // The frame is pushed LAST, so its z is the highest one; the render sort
    // has to override that or a frame would cover what it is meant to sit
    // behind (and take every press aimed at those nodes).
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(Point::ORIGIN, 1.0);
    graph = graph.push_node(node(
        0_usize,
        Point::new(40.0, 40.0),
        Element::from(ContentProbe),
    ));
    graph = graph.push_node(
        node(
            1_usize,
            Point::new(180.0, 180.0),
            Element::from(ContentProbe),
        )
        .frame(),
    );
    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport,
    );

    let rec = out.borrow();
    // Each node's content quad sits at its world position, so the draw order of
    // the two probes is readable off the event stream.
    let content_at = |x: f32| {
        content_event_indices(&rec.events)
            .into_iter()
            .find(|&i| matches!(rec.events[i], DrawEvent::Content(r) if r.x == x))
            .unwrap_or_else(|| panic!("no content quad at x={x}: {:?}", rec.events))
    };
    assert!(
        content_at(180.0) < content_at(40.0),
        "the frame drew after the plain node: {:?}",
        rec.events,
    );
}

// ---------------------------------------------------------------------------
// Keymap wiring: the widget resolves keyboard shortcuts and the pan button
// through `NodeGraph::keymap` (host-rebindable). Resolver-only coverage lives
// in `node_graph::input`; these tests prove the widget event path honors a
// rebound or disabled binding end to end through the recording renderer.
// ---------------------------------------------------------------------------

fn key_press(c: char, code: keyboard::key::Code, modifiers: keyboard::Modifiers) -> iced::Event {
    let key = keyboard::Key::Character(c.to_string().into());
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

fn named_key_press(
    named: keyboard::key::Named,
    code: keyboard::key::Code,
    modifiers: keyboard::Modifiers,
) -> iced::Event {
    let key = keyboard::Key::Named(named);
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// Builds a two-node graph, feeds it `events` (each with its cursor), and
/// returns every message the widget published.
fn run_events<Msg: 'static>(
    graph: Graph<Msg>,
    events: &[(iced::Event, mouse::Cursor)],
) -> Vec<Msg> {
    run_events_selected(graph, &[], events)
}

/// Like [`run_events`], but the host marks `selected` node indices, standing in
/// for a frame where the host has already applied a reported selection.
fn run_events_selected<Msg: 'static>(
    graph: Graph<Msg>,
    selected: &[usize],
    events: &[(iced::Event, mouse::Cursor)],
) -> Vec<Msg> {
    run_events_at(graph, Vector::ZERO, selected, events)
}

/// Like [`run_events_selected`], but the graph sits at `widget_origin` - the
/// case every anchor that is captured in one space and consumed in another has
/// to survive.
fn run_events_at<Msg: 'static>(
    mut graph: Graph<Msg>,
    widget_origin: Vector,
    selected: &[usize],
    events: &[(iced::Event, mouse::Cursor)],
) -> Vec<Msg> {
    graph = graph.push_node(
        node(0_usize, Point::new(10.0, 10.0), Element::from(ContentProbe))
            .selected(selected.contains(&0)),
    );
    graph = graph.push_node(
        node(
            1_usize,
            Point::new(120.0, 10.0),
            Element::from(ContentProbe),
        )
        .selected(selected.contains(&1)),
    );

    let mut tree = Tree::new(&graph as &dyn Widget<Msg, Theme, Recorder>);
    let renderer = Recorder::new(Rc::new(RefCell::new(Recorded::default())));
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut msgs: Vec<Msg> = Vec::new();
    let mut clipboard = clipboard::Null;
    for (event, cursor) in events {
        let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
        graph.update(
            &mut tree,
            event,
            layout,
            *cursor,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
    }
    msgs
}

#[test]
fn default_keymap_select_all_publishes_selection() {
    let graph: Graph<Vec<usize>> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_select(|ids| ids);

    let msgs = run_events(
        graph,
        &[(
            key_press('a', keyboard::key::Code::KeyA, keyboard::Modifiers::COMMAND),
            mouse::Cursor::Unavailable,
        )],
    );
    let mut selected = msgs
        .into_iter()
        .next()
        .expect("select-all published no selection");
    selected.sort_unstable();
    assert_eq!(selected, vec![0, 1]);
}

#[test]
fn rebound_select_all_moves_to_the_new_combo() {
    let keymap = iced_nodegraph::Keymap {
        select_all: Some(iced_nodegraph::KeyCombo::command('l')),
        ..iced_nodegraph::Keymap::default()
    };
    let graph: Graph<Vec<usize>> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .keymap(keymap)
        .on_select(|ids| ids);

    let msgs = run_events(
        graph,
        &[
            // The default combo must be inert once rebound.
            (
                key_press('a', keyboard::key::Code::KeyA, keyboard::Modifiers::COMMAND),
                mouse::Cursor::Unavailable,
            ),
            (
                key_press('l', keyboard::key::Code::KeyL, keyboard::Modifiers::COMMAND),
                mouse::Cursor::Unavailable,
            ),
        ],
    );
    assert_eq!(msgs.len(), 1, "only the rebound combo may select: {msgs:?}");
    let mut selected = msgs.into_iter().next().unwrap();
    selected.sort_unstable();
    assert_eq!(selected, vec![0, 1]);
}

#[test]
fn keymap_none_disables_all_shortcuts() {
    let graph: Graph<Vec<usize>> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .keymap(iced_nodegraph::Keymap::none())
        .on_select(|ids| ids);

    let msgs = run_events(
        graph,
        &[(
            key_press('a', keyboard::key::Code::KeyA, keyboard::Modifiers::COMMAND),
            mouse::Cursor::Unavailable,
        )],
    );
    assert!(msgs.is_empty(), "disabled keymap still published: {msgs:?}");
}

#[test]
fn rebound_pan_button_commits_a_pan() {
    let over = mouse::Cursor::Available(Point::new(200.0, 200.0));
    let events = |button: mouse::Button| {
        vec![
            (
                iced::Event::Mouse(mouse::Event::ButtonPressed(button)),
                over,
            ),
            (
                iced::Event::Mouse(mouse::Event::ButtonReleased(button)),
                over,
            ),
        ]
    };

    // Default keymap: middle button is unbound, no pan is committed.
    let default_graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));
    let msgs = run_events(default_graph, &events(mouse::Button::Middle));
    assert!(
        msgs.is_empty(),
        "unbound middle button committed a pan: {msgs:?}"
    );

    // Rebound to middle: the same press/release pair commits a pan.
    let keymap = iced_nodegraph::Keymap {
        pan_button: mouse::Button::Middle,
        ..iced_nodegraph::Keymap::default()
    };
    let rebound_graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .keymap(keymap)
        .on_camera(|position, zoom| (position, zoom));
    let msgs = run_events(rebound_graph, &events(mouse::Button::Middle));
    assert_eq!(
        msgs.len(),
        1,
        "rebound pan button must commit exactly one pan: {msgs:?}"
    );
}

// ---------------------------------------------------------------------------
// Touch gestures: the widget translates the finger stream into its pointer
// model (single finger = left button, empty-space drag = pan, two fingers =
// pinch). These drive the full `update` path with synthetic touch events.
// ---------------------------------------------------------------------------

fn finger_press(id: u64, position: Point) -> (iced::Event, mouse::Cursor) {
    (
        iced::Event::Touch(touch::Event::FingerPressed {
            id: touch::Finger(id),
            position,
        }),
        mouse::Cursor::Unavailable,
    )
}

fn finger_move(id: u64, position: Point) -> (iced::Event, mouse::Cursor) {
    (
        iced::Event::Touch(touch::Event::FingerMoved {
            id: touch::Finger(id),
            position,
        }),
        mouse::Cursor::Unavailable,
    )
}

fn finger_lift(id: u64, position: Point) -> (iced::Event, mouse::Cursor) {
    (
        iced::Event::Touch(touch::Event::FingerLifted {
            id: touch::Finger(id),
            position,
        }),
        mouse::Cursor::Unavailable,
    )
}

#[test]
fn touch_drag_on_empty_space_pans_the_graph() {
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    let msgs = run_events(
        graph,
        &[
            finger_press(1, Point::new(300.0, 300.0)),
            finger_move(1, Point::new(250.0, 300.0)),
            finger_lift(1, Point::new(250.0, 300.0)),
        ],
    );
    assert_eq!(
        msgs.len(),
        1,
        "touch pan must commit exactly once: {msgs:?}"
    );
    let (position, zoom) = msgs[0];
    assert!(
        (position.x + 50.0).abs() < 1e-3 && position.y.abs() < 1e-3,
        "touch pan committed the wrong offset: {position:?}",
    );
    assert!((zoom - 1.0).abs() < 1e-6);
}

/// A touch pan captures its anchor from the layout-absolute press position,
/// while the release compares against the raw screen cursor - so the widget
/// origin has to be folded back out in between. At a non-zero origin the
/// committed pan is the finger's travel and nothing more; counting the origin
/// twice would report it plus the toolbar offset.
#[test]
fn touch_pan_at_nonzero_origin_commits_only_the_finger_travel() {
    let origin = Vector::new(40.0, 100.0);
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    let msgs = run_events_at(
        graph,
        origin,
        &[],
        &[
            finger_press(1, Point::new(origin.x + 300.0, origin.y + 300.0)),
            finger_move(1, Point::new(origin.x + 250.0, origin.y + 300.0)),
            finger_lift(1, Point::new(origin.x + 250.0, origin.y + 300.0)),
        ],
    );
    assert_eq!(
        msgs.len(),
        1,
        "touch pan must commit exactly once: {msgs:?}"
    );
    let (position, zoom) = msgs[0];
    assert!(
        (position.x + 50.0).abs() < 1e-3 && position.y.abs() < 1e-3,
        "touch pan at origin {origin:?} committed {position:?}, expected (-50, 0)",
    );
    assert!((zoom - 1.0).abs() < 1e-6);
}

#[test]
fn touch_tap_selects_a_node() {
    let graph: Graph<Vec<usize>> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_select(|ids| ids);

    let msgs = run_events(
        graph,
        &[
            // Tap on node 0 (world 10,10 + 40x20 probe; camera identity).
            finger_press(1, Point::new(30.0, 20.0)),
            finger_lift(1, Point::new(30.0, 20.0)),
        ],
    );
    assert_eq!(msgs, vec![vec![0]], "a tap must select the node under it");
}

/// The clear only fires when something IS selected, and selection now lives on
/// the host's nodes - so this needs a host that has applied one.
#[test]
fn touch_tap_on_empty_space_clears_the_selection() {
    let graph: Graph<Vec<usize>> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_select(|ids| ids);

    let msgs = run_events_selected(
        graph,
        &[0],
        &[
            finger_press(1, Point::new(300.0, 300.0)),
            finger_lift(1, Point::new(300.0, 300.0)),
        ],
    );
    assert_eq!(msgs, vec![Vec::<usize>::new()], "an empty tap must clear");
}

#[test]
fn two_finger_pinch_zooms_the_camera() {
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    let msgs = run_events(
        graph,
        &[
            finger_press(1, Point::new(100.0, 200.0)),
            finger_press(2, Point::new(300.0, 200.0)),
            // Contact distance 200 -> 300: zoom 1.0 -> 1.5.
            finger_move(2, Point::new(400.0, 200.0)),
        ],
    );
    let (_, zoom) = *msgs.last().expect("pinch published no camera commit");
    assert!(
        (zoom - 1.5).abs() < 1e-4,
        "pinch must scale zoom by the distance ratio, got {zoom}",
    );
}

#[test]
fn second_finger_cancels_a_touch_node_drag() {
    let graph: Graph<&'static str> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_move(|_, _| "move")
        .on_drag_end(|| "end");

    let msgs = run_events(
        graph,
        &[
            // Press on node 0 starts a node drag (on_move is wired).
            finger_press(1, Point::new(30.0, 20.0)),
            // A second contact enters pinch mode and cancels the drag.
            finger_press(2, Point::new(300.0, 200.0)),
        ],
    );
    assert_eq!(
        msgs,
        vec!["end"],
        "second finger must cancel the drag via on_drag_end",
    );
}

// ---------------------------------------------------------------------------
// Resize-grip cursor: `mouse_interaction` hit-tests the grip in the same
// layout-absolute space the press path uses, so the reported cursor has to
// survive a non-zero widget origin AND a panned, zoomed camera - the exact
// combination the rest of this file exists for.
// ---------------------------------------------------------------------------

/// Screen pixel of a layout-absolute point (`widget_origin + world`) under the
/// given camera: `screen = origin + (p - origin + camera_position) * zoom`.
fn layout_to_screen(origin: Vector, p: Point, camera_pos: Point, zoom: f32) -> Point {
    Point::new(
        origin.x + (p.x - origin.x + camera_pos.x) * zoom,
        origin.y + (p.y - origin.y + camera_pos.y) * zoom,
    )
}

/// The interaction a graph at `widget_origin` reports for a screen `cursor`,
/// holding one 40x20 node at the world origin.
fn interaction_at(
    widget_origin: Vector,
    camera_pos: Point,
    camera_zoom: f32,
    cursor: Point,
    resizable: bool,
) -> mouse::Interaction {
    let mut graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_resize(|_, _| ())
        .camera(camera_pos, camera_zoom);
    graph = graph
        .push_node(node(0_usize, Point::ORIGIN, Element::from(ContentProbe)).resizable(resizable));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let renderer = Recorder::detached();
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    // One update syncs `camera()` into the widget camera and its viewport origin.
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved { position: cursor }),
        layout,
        mouse::Cursor::Available(cursor),
        &renderer,
        &mut clipboard,
        &mut shell,
        &viewport,
    );

    graph.mouse_interaction(
        &tree,
        layout,
        mouse::Cursor::Available(cursor),
        &viewport,
        &renderer,
    )
}

// Widget at (60, 40), camera panned (25, 15) and zoomed 2x. The 40x20 node sits
// at the world origin, so its layout-absolute body is (60, 40)..(100, 60) and
// its grip - RESIZE_GRIP_SIDE (12) / zoom (2) = 6 world px, under the half-node
// cap - is (94, 54)..(100, 60).
const GRIP_ORIGIN: Vector = Vector::new(60.0, 40.0);
const GRIP_CAMERA: Point = Point::new(25.0, 15.0);
const GRIP_ZOOM: f32 = 2.0;

#[test]
fn hovering_the_grip_of_a_resizable_node_reports_the_resize_cursor() {
    let cursor = layout_to_screen(GRIP_ORIGIN, Point::new(97.0, 57.0), GRIP_CAMERA, GRIP_ZOOM);
    assert_eq!(
        interaction_at(GRIP_ORIGIN, GRIP_CAMERA, GRIP_ZOOM, cursor, true),
        mouse::Interaction::ResizingDiagonallyDown,
    );
}

#[test]
fn hovering_a_node_body_outside_the_grip_reports_no_cursor() {
    let cursor = layout_to_screen(GRIP_ORIGIN, Point::new(70.0, 45.0), GRIP_CAMERA, GRIP_ZOOM);
    assert_eq!(
        interaction_at(GRIP_ORIGIN, GRIP_CAMERA, GRIP_ZOOM, cursor, true),
        mouse::Interaction::default(),
    );
}

#[test]
fn the_corner_of_a_non_resizable_node_reports_no_cursor() {
    let cursor = layout_to_screen(GRIP_ORIGIN, Point::new(97.0, 57.0), GRIP_CAMERA, GRIP_ZOOM);
    assert_eq!(
        interaction_at(GRIP_ORIGIN, GRIP_CAMERA, GRIP_ZOOM, cursor, false),
        mouse::Interaction::default(),
    );
}

// ---------------------------------------------------------------------------
// Fit-to-view: the `Home`/`f` keymap actions resolve their targets against live
// layout and drive the camera through the same fit math; the pure math is covered
// next to it in `node_graph::camera`. What these pin is the widget path: what
// reaches `on_camera`, and when nothing may.
// ---------------------------------------------------------------------------

#[test]
fn frame_all_keypress_emits_on_camera() {
    // `Home` carries the default `FocusOptions` (a 300ms tween), so the press
    // only starts the tween; the first `RedrawRequested` after it advances the
    // tween and commits through `on_camera`.
    // count is deterministic even though the tick's camera value is
    // wall-clock timed.
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    let msgs = run_events(
        graph,
        &[
            (
                named_key_press(
                    keyboard::key::Named::Home,
                    keyboard::key::Code::Home,
                    keyboard::Modifiers::empty(),
                ),
                mouse::Cursor::Unavailable,
            ),
            (
                iced::Event::Window(iced::window::Event::RedrawRequested(
                    iced::time::Instant::now(),
                )),
                mouse::Cursor::Unavailable,
            ),
        ],
    );
    assert_eq!(
        msgs.len(),
        1,
        "Home must start a tween that commits exactly one camera change on the next redraw: {msgs:?}"
    );
}

#[test]
fn frame_selection_with_nothing_selected_is_a_noop() {
    // Bare `f` with an empty selection resolves no AABB: no camera change and
    // no `on_camera`, mirroring Blender's "View Selected" on an empty pick.
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    let msgs = run_events(
        graph,
        &[(
            key_press('f', keyboard::key::Code::KeyF, keyboard::Modifiers::empty()),
            mouse::Cursor::Unavailable,
        )],
    );
    assert!(
        msgs.is_empty(),
        "frame-selection with nothing selected must be a no-op: {msgs:?}"
    );
}

#[test]
fn frame_all_without_on_camera_falls_through_unconsumed() {
    // No `on_camera`: the widget cannot commit a fit, so `Home` must not be
    // swallowed - the same gating `CloneSelection` has without `on_clone`.
    let graph: Graph<()> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0));

    let msgs = run_events(
        graph,
        &[(
            named_key_press(
                keyboard::key::Named::Home,
                keyboard::key::Code::Home,
                keyboard::Modifiers::empty(),
            ),
            mouse::Cursor::Unavailable,
        )],
    );
    assert!(
        msgs.is_empty(),
        "Home without a camera callback must publish nothing: {msgs:?}"
    );
}

#[test]
fn focus_operation_commits_once_per_request() {
    let options = FocusOptions {
        animation: None,
        ..FocusOptions::default()
    };
    let mut graph: Graph<(Point, f32)> = NodeGraph::default()
        .id(Id::new("g"))
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom))
        .push_node(node(
            0_usize,
            Point::new(10.0, 10.0),
            Element::from(ContentProbe),
        ))
        .push_node(node(
            1_usize,
            Point::new(120.0, 10.0),
            Element::from(ContentProbe),
        ));
    let mut tree = Tree::new(&graph as &dyn Widget<(Point, f32), Theme, Recorder>);
    let renderer = Recorder::detached();
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let no_op_event = iced::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::ORIGIN,
    });
    let mut msgs: Vec<(Point, f32)> = Vec::new();

    let mut request = focus_operation(Id::new("g"), FocusTarget::<Indexed>::All, options.clone());
    graph.operate(&mut tree, layout, &renderer, &mut request);

    let send = |graph: &mut Graph<(Point, f32)>, tree: &mut Tree, msgs: &mut Vec<(Point, f32)>| {
        let mut shell = iced_wgpu::core::Shell::new(msgs);
        let mut clipboard = clipboard::Null;
        graph.update(
            tree,
            &no_op_event,
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
    };

    send(&mut graph, &mut tree, &mut msgs);
    assert_eq!(msgs.len(), 1, "a focus request must commit once: {msgs:?}");

    send(&mut graph, &mut tree, &mut msgs);
    assert_eq!(
        msgs.len(),
        1,
        "an update without a request must not refit: {msgs:?}"
    );

    let mut request = focus_operation(Id::new("g"), FocusTarget::<Indexed>::All, options);
    graph.operate(&mut tree, layout, &renderer, &mut request);
    send(&mut graph, &mut tree, &mut msgs);
    assert_eq!(
        msgs.len(),
        2,
        "a fresh focus request must re-trigger the fit: {msgs:?}"
    );
}

/// Like [`run_events`], but also returns each event's resulting
/// [`iced::window::RedrawRequest`].
///
/// The plain harness drops the `Shell` after every event, which hides redraw
/// scheduling entirely - and scheduling is load-bearing for the focus tween:
/// `UserInterface::update` rebuilds `redraw_request` from `Wait` on every pass
/// and iced_winit's redraw loop keeps only the LAST pass's state, so a tween
/// that fails to re-assert the request on a re-entrant pass never gets another
/// frame.
fn run_events_collecting_redraw<Msg: 'static>(
    mut graph: Graph<Msg>,
    target: FocusTarget<Indexed>,
    options: FocusOptions,
    events: &[(iced::Event, mouse::Cursor)],
) -> (Vec<Msg>, Vec<iced::window::RedrawRequest>) {
    graph = graph.push_node(node(
        0_usize,
        Point::new(10.0, 10.0),
        Element::from(ContentProbe),
    ));
    graph = graph.push_node(node(
        1_usize,
        Point::new(120.0, 10.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<Msg, Theme, Recorder>);
    let renderer = Recorder::detached();
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let mut request = focus_operation(Id::new("g"), target, options);
    graph.operate(&mut tree, layout, &renderer, &mut request);

    let mut msgs: Vec<Msg> = Vec::new();
    let mut requests = Vec::new();
    let mut clipboard = clipboard::Null;
    for (event, cursor) in events {
        let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
        graph.update(
            &mut tree,
            event,
            layout,
            *cursor,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        requests.push(shell.redraw_request());
    }
    (msgs, requests)
}

#[test]
fn reentrant_redraw_still_requests_the_next_frame() {
    // A re-entrant pass must stay SILENT but must still keep the animation
    // scheduled. `UserInterface::update` rebuilds `redraw_request` from `Wait`
    // on every pass and iced_winit's redraw loop breaks with the LAST pass's
    // state, so a guard that skips `request_redraw()` too throws away the
    // request pass 1 made and no further frame is ever scheduled. Observable
    // symptom: the camera advances exactly one tween step per triggering event
    // and then stops, creeping toward the target one keypress at a time.
    let graph: Graph<(Point, f32)> = NodeGraph::default()
        .id(Id::new("g"))
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));

    // Two passes of ONE frame: the second is re-entrant (same `Instant`). The
    // default 300ms animation is nowhere near done after a single frame, so
    // both passes must leave a redraw scheduled.
    let same_instant = iced::time::Instant::now();
    let redraw = || {
        (
            iced::Event::Window(iced::window::Event::RedrawRequested(same_instant)),
            mouse::Cursor::Unavailable,
        )
    };
    let (msgs, requests) = run_events_collecting_redraw(
        graph,
        FocusTarget::<Indexed>::All,
        FocusOptions::default(),
        &[redraw(), redraw()],
    );

    assert_eq!(msgs.len(), 1, "only the first pass may publish: {msgs:?}");
    assert_eq!(
        requests,
        vec![
            iced::window::RedrawRequest::NextFrame,
            iced::window::RedrawRequest::NextFrame
        ],
        "a live tween must keep the next frame scheduled on EVERY pass, \
         including the silent re-entrant one - iced keeps only the last",
    );
}

#[test]
fn tween_converges_under_simulated_iced_redraw_loop() {
    // End-to-end model of the loop the widget lives in, because the per-event
    // tests cannot see a stall: iced_winit re-runs `UserInterface::update` for
    // one `RedrawRequested` while a pass keeps producing messages (max 3
    // passes), then schedules the next frame ONLY if the LAST pass asked for
    // one. Reproducing that termination rule is the difference between "the
    // tween publishes a value" and "the tween actually animates".
    let options = FocusOptions {
        animation: Some(FocusAnimation {
            duration: Duration::from_millis(300),
            easing: Easing::EaseInOutCubic,
        }),
        ..FocusOptions::default()
    };
    let mut graph: Graph<(Point, f32)> = NodeGraph::default()
        .id(Id::new("g"))
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));
    graph = graph.push_node(node(
        0_usize,
        Point::new(10.0, 10.0),
        Element::from(ContentProbe),
    ));
    graph = graph.push_node(node(
        1_usize,
        Point::new(120.0, 10.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(Point, f32), Theme, Recorder>);
    let renderer = Recorder::detached();
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let mut request = focus_operation(Id::new("g"), FocusTarget::<Indexed>::All, options);
    graph.operate(&mut tree, layout, &renderer, &mut request);
    let mut clipboard = clipboard::Null;

    let mut msgs: Vec<(Point, f32)> = Vec::new();
    let mut frame_at = iced::time::Instant::now();
    let mut frames = 0;

    // 60fps for well past the 300ms duration; the loop is expected to stop
    // itself once the tween finishes and stops asking for frames.
    for _ in 0..60 {
        let event = iced::Event::Window(iced::window::Event::RedrawRequested(frame_at));
        let mut scheduled_next_frame = false;

        // iced's inner pass loop: repeat while a pass produced a message,
        // capped at 3 passes. The LAST pass's request is the one that counts.
        for _ in 0..3 {
            let before = msgs.len();
            let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
            graph.update(
                &mut tree,
                &event,
                layout,
                mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard,
                &mut shell,
                &viewport,
            );
            scheduled_next_frame = shell.redraw_request() == iced::window::RedrawRequest::NextFrame;
            if msgs.len() == before {
                break;
            }
        }

        frames += 1;
        if !scheduled_next_frame {
            break;
        }
        frame_at += Duration::from_millis(16);
    }

    // A 300ms tween at 60fps needs ~19 frames. One frame means the stall is
    // back: the camera would jump a single easing step per triggering event.
    assert!(
        frames > 15,
        "tween must keep scheduling frames for its whole duration, ran {frames}",
    );
    assert!(
        frames < 60,
        "tween must stop scheduling frames once finished, ran {frames}",
    );

    // And it must land exactly on the fit target, not merely near it. The jump
    // path (`animation: None`) commits that target through the same public
    // route in one message - ground truth without restating node geometry.
    let jump_graph: Graph<(Point, f32)> = NodeGraph::default()
        .id(Id::new("g"))
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom));
    let target = *run_events_collecting_redraw(
        jump_graph,
        FocusTarget::<Indexed>::All,
        FocusOptions {
            animation: None,
            ..FocusOptions::default()
        },
        &[(
            iced::Event::Mouse(mouse::Event::CursorMoved {
                position: Point::ORIGIN,
            }),
            mouse::Cursor::Unavailable,
        )],
    )
    .0
    .first()
    .expect("jump path must commit the fit target");

    let (final_position, final_zoom) = *msgs.last().expect("tween must publish");
    assert!(
        (final_position.x - target.0.x).abs() < 1e-2
            && (final_position.y - target.0.y).abs() < 1e-2,
        "final {final_position:?} must equal fit target {:?}",
        target.0,
    );
    assert!((final_zoom - target.1).abs() < 1e-4);
}

// ---------------------------------------------------------------------------
// Anchors in a fit. An anchor is no layout child, so its extent is the circle
// an orbit describes - and only the widget knows those radii, since they come
// out of a style closure. `FocusTarget::Rect` is the oracle throughout: it
// runs the same fit math through the public escape hatch, so a test can pin
// WHICH rect was framed without restating how a rect becomes a camera.
// ---------------------------------------------------------------------------

const ANCHOR_ID: usize = 9;
const ANCHOR_AT: Point = Point::new(900.0, 600.0);
/// The two nodes a routed cable runs between, and the size `ContentProbe` lays
/// out to.
const FIT_NODE_A: Point = Point::new(10.0, 10.0);
const FIT_NODE_B: Point = Point::new(120.0, 10.0);
const PROBE_SIZE: Size = Size::new(40.0, 20.0);

fn jump() -> FocusOptions {
    FocusOptions {
        animation: None,
        ..FocusOptions::default()
    }
}

/// The world bounds of an anchor's `orbit` ring, from the default style the
/// widget falls back to before a first draw has published radii.
fn ring_bounds(center: Point, orbit: u8) -> Rectangle {
    let radius = default_anchor_style(&Theme::Dark, AnchorStatus::Idle).orbit_radius(orbit);
    Rectangle::new(
        Point::new(center.x - radius, center.y - radius),
        Size::new(2.0 * radius, 2.0 * radius),
    )
}

/// The world bounds of a `ContentProbe` node at `at`.
fn node_bounds(at: Point) -> Rectangle {
    Rectangle::new(at, PROBE_SIZE)
}

fn union(a: Rectangle, b: Rectangle) -> Rectangle {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rectangle::new(
        Point::new(x, y),
        Size::new(
            (a.x + a.width).max(b.x + b.width) - x,
            (a.y + a.height).max(b.y + b.height) - y,
        ),
    )
}

/// Frames `target` in a graph the caller filled itself and returns the fit it
/// committed: lay out, hand the graph the focus operation, feed one event so
/// the pending fit starts. [`run_events`] always pushes two nodes, and a graph
/// made of anchors alone is exactly the case a node-bounds-only fit gets
/// wrong.
fn committed_fit(
    mut graph: Graph<(Point, f32)>,
    target: FocusTarget<Indexed>,
) -> Option<(Point, f32)> {
    let mut tree = Tree::new(&graph as &dyn Widget<(Point, f32), Theme, Recorder>);
    let renderer = Recorder::detached();
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut request = focus_operation(Id::new("g"), target, jump());
    graph.operate(&mut tree, layout, &renderer, &mut request);

    let mut msgs: Vec<(Point, f32)> = Vec::new();
    let mut clipboard = clipboard::Null;
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::ORIGIN,
        }),
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut clipboard,
        &mut shell,
        &viewport,
    );
    drop(shell);
    msgs.into_iter().next()
}

/// An empty graph carrying the id the focus operations address, reporting
/// every camera commit as its message.
fn focusable() -> Graph<(Point, f32)> {
    NodeGraph::default()
        .id(Id::new("g"))
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .on_camera(|position, zoom| (position, zoom))
}

/// [`focusable`] with the two nodes [`run_events`] pushes, so a fit with and
/// without extra content can be compared.
fn focusable_with_nodes() -> Graph<(Point, f32)> {
    focusable()
        .push_node(node(0, Point::new(10.0, 10.0), Element::from(ContentProbe)))
        .push_node(node(
            1,
            Point::new(120.0, 10.0),
            Element::from(ContentProbe),
        ))
}

fn assert_same_fit(fit: (Point, f32), expected: (Point, f32), what: &str) {
    assert!(
        (fit.0.x - expected.0.x).abs() < 1e-2
            && (fit.0.y - expected.0.y).abs() < 1e-2
            && (fit.1 - expected.1).abs() < 1e-4,
        "{what}: fitted {fit:?}, expected {expected:?}",
    );
}

#[test]
fn frame_all_resolves_a_graph_made_of_anchors() {
    // Nothing but an anchor is still content. Framing only layout children
    // makes `Home` a silent no-op here - no camera change, no `on_camera` - on a
    // graph that plainly has something to show.
    let graph = focusable().push_anchor(anchor(ANCHOR_ID, ANCHOR_AT));
    let fit =
        committed_fit(graph, FocusTarget::All).expect("an anchor is content: All must resolve it");

    let expected = committed_fit(focusable(), FocusTarget::Rect(ring_bounds(ANCHOR_AT, 0)))
        .expect("the Rect escape hatch must commit a fit");
    assert_same_fit(fit, expected, "an unattached anchor frames its one ring");
}

#[test]
fn frame_all_widens_to_reach_an_anchor_outside_the_nodes() {
    // The anchor sits well past both nodes, so including it must force the
    // camera to pull back. Equal zoom would mean the anchor was cropped out.
    let anchored = focusable_with_nodes().push_anchor(anchor(ANCHOR_ID, ANCHOR_AT));
    let with = committed_fit(anchored, FocusTarget::All).expect("All must commit a fit");
    let without =
        committed_fit(focusable_with_nodes(), FocusTarget::All).expect("All must commit a fit");

    assert!(
        with.1 < without.1,
        "an anchor beyond the nodes must widen the fit: {with:?} vs {without:?}",
    );
}

#[test]
fn focusing_an_anchor_frames_its_ring() {
    // Anchors have their own id space and their own focus target; `Anchor` is
    // how a host names one, and it must resolve rather than silently no-op.
    let graph = focusable().push_anchor(anchor(ANCHOR_ID, ANCHOR_AT));
    let fit =
        committed_fit(graph, FocusTarget::Anchor(ANCHOR_ID)).expect("an anchor id must resolve");

    let expected = committed_fit(focusable(), FocusTarget::Rect(ring_bounds(ANCHOR_AT, 0)))
        .expect("the Rect escape hatch must commit a fit");
    assert_same_fit(fit, expected, "focusing an anchor by id frames its ring");
}

#[test]
fn focusing_a_node_never_reaches_an_anchor() {
    // The two id spaces are separate, so the same integer means different
    // things to `Node` and `Anchor`. An id only an anchor carries is unknown to
    // `Node`, which makes it a no-op - the camera must not commit a fit, and it
    // must NOT quietly frame the anchor instead.
    let graph = focusable().push_anchor(anchor(ANCHOR_ID, ANCHOR_AT));

    assert!(
        committed_fit(graph, FocusTarget::Node(ANCHOR_ID)).is_none(),
        "a node target must not resolve through the anchor map",
    );
}

#[test]
fn focusing_a_routed_cable_frames_its_anchor() {
    // Seeing a connection means seeing where it RUNS, and a routed cable does
    // not run between its pins: the anchor here sits far outside both endpoint
    // nodes, so framing the endpoints alone leaves the cable's whole detour off
    // screen.
    let graph = focusable()
        .push_node(node(0, FIT_NODE_A, Element::from(ContentProbe)))
        .push_node(node(1, FIT_NODE_B, Element::from(ContentProbe)))
        .push_anchor(anchor(ANCHOR_ID, ANCHOR_AT))
        .push_edge(edge((), PinRef::new(0, 0), PinRef::new(1, 0)).route([ANCHOR_ID]));
    let fit = committed_fit(graph, FocusTarget::Edge(())).expect("a routed edge must resolve");

    let expected = committed_fit(
        focusable(),
        FocusTarget::Rect(union(
            union(node_bounds(FIT_NODE_A), node_bounds(FIT_NODE_B)),
            ring_bounds(ANCHOR_AT, 0),
        )),
    )
    .expect("the Rect escape hatch must commit a fit");
    assert_same_fit(
        fit,
        expected,
        "a routed cable frames both pins and every ring it wraps",
    );
}
