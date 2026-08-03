//! Overlay-forwarding tests: NodeGraph must hand each node's pop-out overlay
//! (combo box menu, tooltip, vanilla `menu`, ...) up through `Widget::overlay`,
//! wrapped in the camera transform. The wrapper has three jobs, one test each:
//!
//! 1. forward only when a child actually produces an overlay (else `None`),
//! 2. draw the pop-out through the same world->screen transform as node content,
//!    so it anchors to and scales with the node beneath it,
//! 3. map the screen cursor back into layout space (the inverse transform) for
//!    the wrapped overlay's hit-testing.
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

use iced_nodegraph::{NodeGraph, node};

mod common;

use common::record::{Recorded, Recorder};

// ---------------------------------------------------------------------------
// An overlay that paints a 10x10 quad at a fixed anchor and records the cursor
// it last received. The anchor is captured (in layout-absolute space) when the
// host widget produces the overlay.
// ---------------------------------------------------------------------------
struct ProbeOverlay {
    anchor: Point,
    cursor_seen: Rc<Cell<Option<Point>>>,
}

impl overlay::Overlay<(), Theme, Recorder> for ProbeOverlay {
    fn layout(&mut self, _renderer: &Recorder, _bounds: Size) -> layout::Node {
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
        self.cursor_seen.set(cursor.position());
    }
}

// ---------------------------------------------------------------------------
// A leaf node-content widget that produces `ProbeOverlay` (mirrors combo_box /
// tooltip exposing a pop-out). The anchor it captures is its own layout
// position plus the incoming translation, exactly as the real widgets do.
// ---------------------------------------------------------------------------
struct OverlayProbe {
    cursor_seen: Rc<Cell<Option<Point>>>,
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
        Some(overlay::Element::new(Box::new(ProbeOverlay {
            anchor,
            cursor_seen: self.cursor_seen.clone(),
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

/// Lays out a single-node graph at `origin` with the given camera, runs one
/// no-op update so `view()` syncs into the widget camera, and returns the parts
/// needed to drive `overlay()`.
fn graph_with_node(
    origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
    element: Element<'static, (), Theme, Recorder>,
    renderer: &Recorder,
) -> (
    NodeGraph<'static, usize, usize, (), (), Theme, Recorder>,
    Tree,
    layout::Node,
) {
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Recorder> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .view(camera_pos, camera_zoom);
    graph.push_node(node(0usize, node_world, element));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Recorder>);
    let layout_node = graph.layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, VIEWPORT),
    );

    // Sync `view()` into the widget camera (host value differs from the unset
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
        Element::from(OverlayProbe {
            cursor_seen: Rc::new(Cell::new(None)),
        }),
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
        Element::from(OverlayProbe {
            cursor_seen: Rc::new(Cell::new(None)),
        }),
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
fn overlay_maps_cursor_into_layout_space() {
    // Round trip: a screen cursor placed where the overlay anchor draws must
    // reach the wrapped overlay as the anchor's layout-absolute coordinate
    // (origin + world) -- the inverse of the draw transform.
    let origin = Vector::new(0.0, 100.0);
    let world = Point::new(30.0, 40.0);
    let cam_pos = Point::new(20.0, -10.0);
    let zoom = 2.0;

    let cursor_seen = Rc::new(Cell::new(None));
    let renderer = Recorder::new(Rc::new(RefCell::new(Recorded::default())));
    let (mut graph, mut tree, layout_node) = graph_with_node(
        origin,
        world,
        cam_pos,
        zoom,
        Element::from(OverlayProbe {
            cursor_seen: cursor_seen.clone(),
        }),
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

    let seen = cursor_seen.get().expect("overlay must receive a cursor");
    let expected = Point::new(origin.x + world.x, origin.y + world.y);
    assert!(
        (seen.x - expected.x).abs() < 0.5 && (seen.y - expected.y).abs() < 0.5,
        "cursor reached overlay as {seen:?}, expected layout-absolute {expected:?}",
    );
}
