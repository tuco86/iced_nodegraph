//! Overlay-forwarding tests: NodeGraph must hand each node's pop-out overlay
//! (combo box menu, tooltip, vanilla `menu`, ...) up through `Widget::overlay`,
//! wrapped in the camera transform. The wrapper has three jobs, one test each:
//!
//! 1. forward only when a child actually produces an overlay (else `None`),
//! 2. draw the pop-out at the same screen pixel as the node content beneath it,
//!    so it anchors to and scales with that node,
//! 3. hand the wrapped overlay a cursor in the space its anchor was given, so
//!    its own hit-testing agrees with where it is drawn.
//!
//! The anchor space is zoomed-screen space (screen pixels over zoom, origin at
//! the window corner): the one space in which iced's pop-outs judge their room
//! correctly, see `CameraOverlay`.
//!
//! Like the sibling recording-renderer tests, these use a fake renderer: the
//! guarantees live in the overlay element's presence, the absolute rect it
//! draws at, and the cursor it receives -- not in pixel output.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, overlay, renderer};
use iced::{Background, Color, Element, Length, Point, Rectangle, Size, Theme, Vector};
use iced_wgpu::core::clipboard;

use iced_nodegraph::{Indexed, NodeGraph, node};

mod common;

use common::record::{Recorded, Recorder};

// ---------------------------------------------------------------------------
// An overlay that paints a 10x10 quad at a fixed anchor and records what it was
// handed. The anchor is captured (in layout-absolute space) when the host
// widget produces the overlay.
// ---------------------------------------------------------------------------

/// What a probe records about the frame it was drawn in.
#[derive(Default)]
struct ProbeLog {
    /// Anchor the host widget captured: its layout position plus the
    /// translation the graph handed it.
    anchor: Cell<Option<Point>>,
    /// Cursor the overlay's `update` received.
    cursor: Cell<Option<Point>>,
    /// Innermost clip the overlay painted under.
    clip: Cell<Option<Rectangle>>,
    /// `bounds` the overlay's `layout` was given.
    bounds: Cell<Option<Size>>,
}

struct ProbeOverlay {
    anchor: Point,
    log: Rc<ProbeLog>,
}

impl overlay::Overlay<(), Theme, Recorder> for ProbeOverlay {
    fn layout(&mut self, _renderer: &Recorder, bounds: Size) -> layout::Node {
        self.log.bounds.set(Some(bounds));
        layout::Node::new(Size::new(10.0, 10.0)).move_to(self.anchor)
    }
    fn draw(
        &self,
        renderer: &mut Recorder,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        self.log.clip.set(renderer.clip());
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Background::Color(Color::WHITE),
        );
    }
    fn update(
        &mut self,
        _event: &iced::Event,
        _layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Recorder,
        _clipboard: &mut dyn clipboard::Clipboard,
        _shell: &mut iced_wgpu::core::Shell<'_, ()>,
    ) {
        self.log.cursor.set(cursor.position());
    }
}

// ---------------------------------------------------------------------------
// A leaf node-content widget that produces `ProbeOverlay` (mirrors combo_box /
// tooltip exposing a pop-out). The anchor it captures is its own layout
// position plus the incoming translation, exactly as the real widgets do.
// ---------------------------------------------------------------------------
struct OverlayProbe {
    log: Rc<ProbeLog>,
}

impl Widget<(), Theme, Recorder> for OverlayProbe {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(40.0), Length::Fixed(20.0))
    }
    fn layout(&mut self, _: &mut Tree, _: &Recorder, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(40.0), Length::Fixed(20.0), Size::ZERO))
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
    fn overlay<'a>(
        &'a mut self,
        _tree: &'a mut Tree,
        layout: Layout<'a>,
        _renderer: &Recorder,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, (), Theme, Recorder>> {
        let anchor = layout.position() + translation;
        self.log.anchor.set(Some(anchor));
        Some(overlay::Element::new(Box::new(ProbeOverlay {
            anchor,
            log: self.log.clone(),
        })))
    }
}

impl<'a> From<OverlayProbe> for Element<'a, (), Theme, Recorder> {
    fn from(w: OverlayProbe) -> Self {
        Element::new(w)
    }
}

// A leaf with no overlay (the trait default returns `None`).
struct PlainLeaf;
impl Widget<(), Theme, Recorder> for PlainLeaf {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(40.0), Length::Fixed(20.0))
    }
    fn layout(&mut self, _: &mut Tree, _: &Recorder, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(40.0), Length::Fixed(20.0), Size::ZERO))
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
}
impl<'a> From<PlainLeaf> for Element<'a, (), Theme, Recorder> {
    fn from(w: PlainLeaf) -> Self {
        Element::new(w)
    }
}

const VIEWPORT: Size = Size::new(1024.0, 768.0);

/// The graph shape these tests drive: default ids, recording renderer. Named
/// because the ids come first in `NodeGraph`'s parameter list, so reaching
/// `Recorder` means spelling all four of them.
type RecordedGraph = NodeGraph<'static, Indexed, (), Theme, Recorder>;

/// Lays out a single-node graph at `origin` with the given camera, runs one
/// no-op update so `camera()` syncs into the widget camera, and returns the parts
/// needed to drive `overlay()`.
fn graph_with_node(
    origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
    element: Element<'static, (), Theme, Recorder>,
    renderer: &Recorder,
) -> (RecordedGraph, Tree, layout::Node) {
    let mut graph: RecordedGraph = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .camera(camera_pos, camera_zoom);
    graph = graph.push_node(node(0usize, node_world, element));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, VIEWPORT),
    );

    // Sync `camera()` into the widget camera (host value differs from the unset
    // last-synced value); the event itself is a no-op. Mirrors the real
    // pipeline, where update() runs before overlay().
    let layout = Layout::with_offset(origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clip = clipboard::Null;
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        layout,
        mouse::Cursor::Unavailable,
        renderer,
        &mut clip,
        &mut shell,
        &viewport,
    );

    (graph, tree, layout_node)
}

#[test]
fn overlay_forwarded_when_child_has_one() {
    let renderer = Recorder::new(Rc::new(RefCell::new(Recorded::default())));
    let (mut graph, mut tree, layout_node) = graph_with_node(
        Vector::ZERO,
        Point::new(50.0, 50.0),
        Point::ORIGIN,
        1.0,
        Element::from(OverlayProbe { log: Rc::default() }),
        &renderer,
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let ov = graph.overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO);
    assert!(
        ov.is_some(),
        "a node whose child produces an overlay must surface it through NodeGraph::overlay",
    );
}

#[test]
fn no_overlay_when_no_child_has_one() {
    let renderer = Recorder::new(Rc::new(RefCell::new(Recorded::default())));
    let (mut graph, mut tree, layout_node) = graph_with_node(
        Vector::ZERO,
        Point::new(50.0, 50.0),
        Point::ORIGIN,
        1.0,
        Element::from(PlainLeaf),
        &renderer,
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let ov = graph.overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO);
    assert!(
        ov.is_none(),
        "with no child overlay, NodeGraph must yield no overlay (not an empty group)",
    );
}

#[test]
fn overlay_draws_through_camera_transform() {
    // The crux: the pop-out must be drawn at the same screen pixel as the node
    // content beneath it -- origin + (world + camera_pos) * zoom -- so it tracks
    // the node under zoom, pan, and a non-zero widget origin.
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let cam_pos = Point::new(20.0, -10.0);
    let zoom = 2.0;

    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());
    let (mut graph, mut tree, layout_node) = graph_with_node(
        origin,
        world,
        cam_pos,
        zoom,
        Element::from(OverlayProbe { log: Rc::default() }),
        &renderer,
    );
    let layout = Layout::with_offset(origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let mut ov = graph
        .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
        .expect("overlay must be present");
    let onode = ov.as_overlay_mut().layout(&renderer, VIEWPORT);
    let olayout = Layout::new(&onode);
    ov.as_overlay().draw(
        &mut renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        olayout,
        mouse::Cursor::Unavailable,
    );

    let drawn = out
        .borrow()
        .quads
        .first()
        .copied()
        .expect("overlay drew a quad");
    let expected = Point::new(
        origin.x + (world.x + cam_pos.x) * zoom,
        origin.y + (world.y + cam_pos.y) * zoom,
    );
    assert!(
        (drawn.x - expected.x).abs() < 0.5 && (drawn.y - expected.y).abs() < 0.5,
        "overlay drawn at {drawn:?} should sit at {expected:?} (origin {origin:?}, zoom {zoom})",
    );
    // The pop-out scales with the camera too: a 10px anchor box -> 10 * zoom.
    assert!(
        (drawn.width - 10.0 * zoom).abs() < 0.5,
        "overlay should scale with zoom: width {} expected {}",
        drawn.width,
        10.0 * zoom,
    );
}

#[test]
fn overlay_receives_the_cursor_in_its_anchor_space() {
    // Round trip: a screen cursor placed where the overlay anchor draws must
    // reach the wrapped overlay AS that anchor, whatever space the graph put
    // the anchor in -- the pop-out hit-tests the cursor against its own
    // layout, so the two must agree.
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let cam_pos = Point::new(20.0, -10.0);
    let zoom = 2.0;

    let log = Rc::new(ProbeLog::default());
    let renderer = Recorder::new(Rc::new(RefCell::new(Recorded::default())));
    let (mut graph, mut tree, layout_node) = graph_with_node(
        origin,
        world,
        cam_pos,
        zoom,
        Element::from(OverlayProbe { log: log.clone() }),
        &renderer,
    );
    let layout = Layout::with_offset(origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let mut ov = graph
        .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
        .expect("overlay must be present");
    let onode = ov.as_overlay_mut().layout(&renderer, VIEWPORT);
    let olayout = Layout::new(&onode);

    // Screen pixel where the anchor draws (same mapping as the draw test).
    let screen = Point::new(
        origin.x + (world.x + cam_pos.x) * zoom,
        origin.y + (world.y + cam_pos.y) * zoom,
    );
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clip = clipboard::Null;
    ov.as_overlay_mut().update(
        &iced::Event::Mouse(mouse::Event::CursorMoved { position: screen }),
        olayout,
        mouse::Cursor::Available(screen),
        &renderer,
        &mut clip,
        &mut shell,
    );

    let seen = log.cursor.get().expect("overlay must receive a cursor");
    let anchor = log.anchor.get().expect("probe captured its anchor");
    assert!(
        (seen.x - anchor.x).abs() < 0.5 && (seen.y - anchor.y).abs() < 0.5,
        "cursor reached overlay as {seen:?}, expected its anchor {anchor:?}",
    );
}

#[test]
fn overlay_anchored_on_screen_has_room_under_pan() {
    // A node far from the world origin, panned onto the screen: its layout
    // coordinates exceed `window / zoom`, so an anchor left in layout space
    // would tell a menu it has negative room to the right and below. The
    // anchor must land inside the bounds the pop-out is laid out against.
    let world = Point::new(3000.0, 2000.0);
    let cam_pos = Point::new(-2800.0, -1900.0); // node at screen (200, 100)
    let zoom = 1.0;

    let log = Rc::new(ProbeLog::default());
    let renderer = Recorder::detached();
    let (mut graph, mut tree, layout_node) = graph_with_node(
        Vector::ZERO,
        world,
        cam_pos,
        zoom,
        Element::from(OverlayProbe { log: log.clone() }),
        &renderer,
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let mut ov = graph
        .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
        .expect("overlay must be present");
    let _ = ov.as_overlay_mut().layout(&renderer, VIEWPORT);

    let anchor = log.anchor.get().expect("probe captured its anchor");
    let bounds = log.bounds.get().expect("overlay must be laid out");
    assert!(
        (anchor.x - 200.0).abs() < 0.5 && (anchor.y - 100.0).abs() < 0.5,
        "anchor {anchor:?} should be where the node is on screen, (200, 100)",
    );
    assert!(
        anchor.x < bounds.width && anchor.y < bounds.height,
        "anchor {anchor:?} must lie inside the room {bounds:?} the pop-out is given",
    );
}

#[test]
fn overlay_survives_the_runtime_clip_at_zoom() {
    // The runtime clips every overlay by the bounds of the node the overlay
    // returned, computed OUTSIDE the camera transform (iced_core 0.14
    // `overlay/nested.rs`), and `push_clip` bakes in the transformation active
    // at entry and REPLACES the parent clip (iced_graphics 0.14 `layer.rs`). So
    // the node has to keep reporting the untransformed window: a node scaled
    // down to the layout-space region would clip the pop-out to a fraction of
    // the screen. Probe placed so its transformed rect is inside the window but
    // outside `window / zoom` - exactly the band such a node would lose.
    let world = Point::new(300.0, 200.0);
    let zoom = 2.0;

    let log = Rc::new(ProbeLog::default());
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Recorder::new(out.clone());
    let (mut graph, mut tree, layout_node) = graph_with_node(
        Vector::ZERO,
        world,
        Point::ORIGIN,
        zoom,
        Element::from(OverlayProbe { log: log.clone() }),
        &renderer,
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let mut ov = graph
        .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
        .expect("overlay must be present");
    let onode = ov.as_overlay_mut().layout(&renderer, VIEWPORT);
    let olayout = Layout::new(&onode);
    let nested_clip = olayout.bounds();
    renderer.with_layer(nested_clip, |renderer| {
        ov.as_overlay().draw(
            renderer,
            &Theme::Dark,
            &renderer::Style {
                text_color: Color::WHITE,
            },
            olayout,
            mouse::Cursor::Unavailable,
        );
    });

    let drawn = out
        .borrow()
        .quads
        .first()
        .copied()
        .expect("overlay drew a quad");
    let clip = log.clip.get().expect("overlay painted inside a clip");
    assert!(
        clip.intersection(&drawn).is_some_and(|visible| {
            (visible.width - drawn.width).abs() < 0.5 && (visible.height - drawn.height).abs() < 0.5
        }),
        "the pop-out at {drawn:?} is clipped by {clip:?} (untransformed wrapper was \
         {nested_clip:?}): the whole quad must survive",
    );
}

#[test]
fn overlay_lays_out_in_layout_units() {
    // The content lays out in layout-absolute space while `bounds` arrives in
    // screen pixels, so a menu deciding whether it fits below its anchor has to
    // be handed the region divided by zoom - otherwise at zoom 2 it believes it
    // has twice the room it has.
    let zoom = 2.0;
    let log = Rc::new(ProbeLog::default());
    let renderer = Recorder::detached();
    let (mut graph, mut tree, layout_node) = graph_with_node(
        Vector::ZERO,
        Point::new(30.0, 40.0),
        Point::ORIGIN,
        zoom,
        Element::from(OverlayProbe { log: log.clone() }),
        &renderer,
    );
    let layout = Layout::new(&layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, VIEWPORT);

    let mut ov = graph
        .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
        .expect("overlay must be present");
    let _ = ov.as_overlay_mut().layout(&renderer, VIEWPORT);

    let seen = log.bounds.get().expect("overlay must be laid out");
    let expected = Size::new(VIEWPORT.width / zoom, VIEWPORT.height / zoom);
    assert!(
        (seen.width - expected.width).abs() < 0.5 && (seen.height - expected.height).abs() < 0.5,
        "content overlay laid out against {seen:?}, expected {expected:?} at zoom {zoom}",
    );
}
