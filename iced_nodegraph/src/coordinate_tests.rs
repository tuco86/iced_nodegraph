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
use iced::keyboard;
use iced::{
    Background, Color, Element, Length, Pixels, Point, Rectangle, Size, Theme, Transformation,
    Vector,
};
use iced_wgpu::core::clipboard;
use iced_wgpu::core::image;
use iced_wgpu::core::text;

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
    /// Unified draw-call stream in call order, across both `fill_quad`
    /// (hosted content) and `draw_primitive` (SDF layers). Lets a test assert
    /// the per-node SDF/content/SDF sandwich order the host integration relies
    /// on, which the two separate vecs above lose.
    events: Vec<DrawEvent>,
}

/// One ordered draw call captured by [`Rec`], tagged by source.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DrawEvent {
    /// A hosted-content quad (`fill_quad`) at this absolute rect.
    Content(Rectangle),
    /// An SDF layer (`draw_primitive`) at this absolute clip rect.
    Sdf(Rectangle),
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
        let mut out = self.out.borrow_mut();
        out.quads.push(abs);
        out.events.push(DrawEvent::Content(abs));
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
    // Real (GPU-free) types: iced_core's `()` impls are debug_assertions-gated
    // and break release test builds; these tests never lay out text.
    type Paragraph = iced_wgpu::graphics::text::Paragraph;
    type Editor = iced_wgpu::graphics::text::Editor;

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
        let mut out = self.out.borrow_mut();
        out.primitives.push(abs);
        out.events.push(DrawEvent::Sdf(abs));
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
        .view(camera_pos, camera_zoom);
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

    // One update syncs `view()` into the widget camera (the host value differs
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

    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .view(camera_pos, camera_zoom)
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
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    let cursor = mouse::Cursor::Available(screen);

    // First a CursorMoved so the widget syncs `view()` and tracks the cursor,
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
        .view(Point::ORIGIN, camera_zoom)
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
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;

    let send = |graph: &mut NodeGraph<'static, usize, usize, (), (), Theme, Rec>,
                tree: &mut Tree,
                shell: &mut iced_wgpu::core::Shell<'_, ()>,
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
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .view(Point::ORIGIN, 1.0);
    for (i, p) in [(30.0, 40.0), (140.0, 90.0), (60.0, 220.0)]
        .into_iter()
        .enumerate()
    {
        graph.push_node(node(i, Point::new(p.0, p.1), Element::from(ContentProbe)));
    }

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Rec>);
    let layout_node = graph.layout(
        &mut tree,
        &Rec::new(Rc::new(RefCell::new(Recorded::default()))),
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::with_offset(Vector::ZERO, &layout_node);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    let mut reference: Option<Vec<u32>> = None;
    for frame in 0..120 {
        let out = Rc::new(RefCell::new(Recorded::default()));
        let mut renderer = Rec::new(out.clone());

        // A no-op cursor move per frame both syncs `view()` and lets the widget
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
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .view(Point::ORIGIN, 1.0);
    graph.push_node(node(
        0_usize,
        Point::new(40.0, 40.0),
        Element::from(ContentProbe),
    ));
    graph.push_node(node(
        1_usize,
        Point::new(180.0, 180.0),
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

// ---------------------------------------------------------------------------
// Keymap wiring: the widget resolves keyboard shortcuts and the pan button
// through `NodeGraph::keymap` (host-rebindable). Resolver-only coverage lives
// in `node_graph::input`; these tests prove the widget event path honors a
// rebound or disabled binding end to end, reusing this file's mock renderer.
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

/// Builds a two-node graph, feeds it `events` (each with its cursor), and
/// returns every message the widget published.
fn run_events<Msg: 'static>(
    mut graph: NodeGraph<'static, usize, usize, (), Msg, Theme, Rec>,
    events: &[(iced::Event, mouse::Cursor)],
) -> Vec<Msg> {
    graph.push_node(node(
        0_usize,
        Point::new(10.0, 10.0),
        Element::from(ContentProbe),
    ));
    graph.push_node(node(
        1_usize,
        Point::new(120.0, 10.0),
        Element::from(ContentProbe),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<Msg, Theme, Rec>);
    let renderer = Rec::new(Rc::new(RefCell::new(Recorded::default())));
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);
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
    let graph: NodeGraph<'static, usize, usize, (), Vec<usize>, Theme, Rec> = NodeGraph::default()
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
    let keymap = crate::Keymap {
        select_all: Some(crate::KeyCombo::command('l')),
        ..crate::Keymap::default()
    };
    let graph: NodeGraph<'static, usize, usize, (), Vec<usize>, Theme, Rec> = NodeGraph::default()
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
    let graph: NodeGraph<'static, usize, usize, (), Vec<usize>, Theme, Rec> = NodeGraph::default()
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(400.0))
        .keymap(crate::Keymap::none())
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
    let default_graph: NodeGraph<'static, usize, usize, (), (Point, f32), Theme, Rec> =
        NodeGraph::default()
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(400.0))
            .on_pan(|position, zoom| (position, zoom));
    let msgs = run_events(default_graph, &events(mouse::Button::Middle));
    assert!(
        msgs.is_empty(),
        "unbound middle button committed a pan: {msgs:?}"
    );

    // Rebound to middle: the same press/release pair commits a pan.
    let keymap = crate::Keymap {
        pan_button: mouse::Button::Middle,
        ..crate::Keymap::default()
    };
    let rebound_graph: NodeGraph<'static, usize, usize, (), (Point, f32), Theme, Rec> =
        NodeGraph::default()
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(400.0))
            .keymap(keymap)
            .on_pan(|position, zoom| (position, zoom));
    let msgs = run_events(rebound_graph, &events(mouse::Button::Middle));
    assert_eq!(
        msgs.len(),
        1,
        "rebound pan button must commit exactly one pan: {msgs:?}"
    );
}
