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

use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::{
    Background, Color, Element, Length, Pixels, Point, Rectangle, Size, Theme, Transformation,
    Vector,
};
use iced_widget::core::clipboard;
use iced_widget::core::image;
use iced_widget::core::text;

use crate::{NodeGraph, node};

// ---------------------------------------------------------------------------
// Recording renderer: tracks the transformation stack (composed like
// iced_graphics: child = current * transformation) so we can map drawn
// positions back to absolute screen pixels, and records primitive clip bounds.
// ---------------------------------------------------------------------------
#[derive(Debug, Default, Clone)]
struct Recorded {
    /// Absolute screen rects of `fill_quad` calls (transformation applied).
    quads: Vec<Rectangle>,
    /// Bounds handed to `draw_primitive` (SDF layers), in order.
    primitives: Vec<Rectangle>,
}

struct Rec {
    stack: Vec<Transformation>,
    out: Rc<RefCell<Recorded>>,
}

impl Rec {
    fn new(out: Rc<RefCell<Recorded>>) -> Self {
        Self {
            stack: vec![Transformation::IDENTITY],
            out,
        }
    }
    fn cur(&self) -> Transformation {
        *self.stack.last().unwrap()
    }
}

impl renderer::Renderer for Rec {
    fn start_layer(&mut self, _bounds: Rectangle) {}
    fn end_layer(&mut self) {}
    fn start_transformation(&mut self, t: Transformation) {
        // iced_graphics composes the new transformation onto the current one.
        self.stack.push(self.cur() * t);
    }
    fn end_transformation(&mut self) {
        self.stack.pop();
        if self.stack.is_empty() {
            self.stack.push(Transformation::IDENTITY);
        }
    }
    fn reset(&mut self, _new_bounds: Rectangle) {}
    fn fill_quad(&mut self, quad: renderer::Quad, _background: impl Into<Background>) {
        let abs = quad.bounds * self.cur();
        self.out.borrow_mut().quads.push(abs);
    }
    fn allocate_image(
        &mut self,
        _handle: &image::Handle,
        _callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
    ) {
    }
}

impl text::Renderer for Rec {
    type Font = iced::Font;
    type Paragraph = ();
    type Editor = ();

    const ICON_FONT: iced::Font = iced::Font::DEFAULT;
    const CHECKMARK_ICON: char = '0';
    const ARROW_DOWN_ICON: char = '0';
    const SCROLL_UP_ICON: char = '0';
    const SCROLL_DOWN_ICON: char = '0';
    const SCROLL_LEFT_ICON: char = '0';
    const SCROLL_RIGHT_ICON: char = '0';
    const ICED_LOGO: char = '0';

    fn default_font(&self) -> Self::Font {
        iced::Font::default()
    }
    fn default_size(&self) -> Pixels {
        Pixels(16.0)
    }
    fn fill_paragraph(&mut self, _: &Self::Paragraph, _: Point, _: Color, _: Rectangle) {}
    fn fill_editor(&mut self, _: &Self::Editor, _: Point, _: Color, _: Rectangle) {}
    fn fill_text(&mut self, _: text::Text, _: Point, _: Color, _: Rectangle) {}
}

impl iced_wgpu::primitive::Renderer for Rec {
    fn draw_primitive(&mut self, bounds: Rectangle, _primitive: impl iced_wgpu::Primitive) {
        let abs = bounds * self.cur();
        self.out.borrow_mut().primitives.push(abs);
    }
}

// ---------------------------------------------------------------------------
// A leaf node-content widget that paints one fill_quad covering its bounds, so
// the recorder captures the absolute screen position of the node's content.
// ---------------------------------------------------------------------------
struct ContentProbe;

impl<Message> Widget<Message, Theme, Rec> for ContentProbe {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(40.0), Length::Fixed(20.0))
    }
    fn layout(&mut self, _: &mut Tree, _: &Rec, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(40.0), Length::Fixed(20.0), Size::ZERO))
    }
    fn draw(
        &self,
        _: &Tree,
        renderer: &mut Rec,
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

impl<'a, Message: 'a> From<ContentProbe> for Element<'a, Message, Theme, Rec> {
    fn from(w: ContentProbe) -> Self {
        Element::new(w)
    }
}

/// Lays out a single-node graph, places it at `widget_origin`, applies the
/// given camera (zoom, world position), draws it, and returns the recorded
/// content quad and SDF primitive bounds.
fn draw_at_origin(
    widget_origin: Vector,
    node_world: Point,
    camera_pos: Point,
    camera_zoom: f32,
) -> Recorded {
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .initial_camera(camera_pos, camera_zoom);
    graph.push_node(node(0_usize, node_world, Element::from(ContentProbe)));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Rec>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Rec::new(out.clone());

    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    // Simulate the graph sitting at a non-zero origin (e.g. below a toolbar).
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    // One update applies `initial_camera` into the widget state (it is gated to
    // run once); the event itself is a no-op for our measurement.
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut msgs);
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

    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .initial_camera(camera_pos, camera_zoom)
        .on_select(move |ids| {
            *sel.borrow_mut() = Some(ids);
        });
    graph.push_node(node(0_usize, node_world, Element::from(ContentProbe)));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Rec>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let renderer = Rec::new(out);
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    let cursor = mouse::Cursor::Available(screen);

    // First a CursorMoved so the widget applies initial_camera and tracks the
    // cursor, then the press that performs the hit-test and selection.
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

/// Drags a box-select from screen `p1` to `p2` over empty graph space (the only
/// node is far away) and returns the SDF primitives recorded by a final draw.
fn box_select_primitives(
    widget_origin: Vector,
    camera_zoom: f32,
    p1: Point,
    p2: Point,
) -> Vec<Rectangle> {
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .initial_camera(Point::ORIGIN, camera_zoom)
        .on_select(|_ids| {});
    // Node far from the drag so the press starts a box select, not a node click.
    graph.push_node(node(
        0_usize,
        Point::new(900.0, 900.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Rec>);
    let out = Rc::new(RefCell::new(Recorded::default()));
    let mut renderer = Rec::new(out.clone());
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(widget_origin, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;

    let send = |graph: &mut NodeGraph<'static, usize, usize, (), (), Theme, Rec>,
                tree: &mut Tree,
                shell: &mut iced_widget::core::Shell<'_, ()>,
                clipboard: &mut clipboard::Null,
                renderer: &Rec,
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

    // Move to p1, press (starts box select at p1), drag to p2.
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
fn box_select_renders_where_dragged_at_nonzero_origin() {
    // The selection box must render at the screen rectangle the user dragged,
    // regardless of widget origin or zoom. Box corners map back to the cursor
    // screen positions, so the select clip should span p1..p2 (plus AA padding).
    let origin = Vector::new(0.0, 100.0);
    let p1 = Point::new(40.0, 160.0);
    let p2 = Point::new(120.0, 240.0);
    let prims = box_select_primitives(origin, 2.0, p1, p2);

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
