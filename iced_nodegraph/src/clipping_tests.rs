//! Regression test for #1: NodeGraph must clip the viewport it forwards to
//! child widgets to its own layout bounds. Otherwise widgets nested in nodes
//! (text, sliders, buttons, ...) paint past the graph edge into neighbouring
//! widgets in a `row!`, `pane_grid`, etc.
//!
//! We assert on the `viewport` argument that reaches a leaf child widget. To
//! invoke `Widget::draw` / `Widget::update` we need *something* that satisfies
//! NodeGraph's renderer bounds (`core::Renderer + iced_wgpu::primitive::Renderer`),
//! but we do not need a real renderer: the bug is observable in a single
//! argument value, not in pixel output.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::{
    Background, Color, Element, Length, Pixels, Point, Rectangle, Size, Theme, Transformation,
};
use iced_widget::core::clipboard;
use iced_widget::core::image;
use iced_widget::core::text;

use crate::{NodeGraph, node};

// ---------------------------------------------------------------------------
// Stub renderer: no-op implementations of every trait NodeGraph requires.
// The test never inspects pixel output, so all methods are empty.
// ---------------------------------------------------------------------------
struct Stub;

thread_local! {
    // Records active clip layers so a child can inspect the innermost clip it
    // is drawn under (push_clip in iced replaces, not intersects, parent clips).
    static LAYER_STACK: RefCell<Vec<Rectangle>> = const { RefCell::new(Vec::new()) };
    // Snapshot of the innermost clip at the moment the recorder child is drawn.
    static CHILD_CLIP: Cell<Option<Rectangle>> = const { Cell::new(None) };
}

impl renderer::Renderer for Stub {
    fn start_layer(&mut self, bounds: Rectangle) {
        LAYER_STACK.with(|s| s.borrow_mut().push(bounds));
    }
    fn end_layer(&mut self) {
        LAYER_STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
    fn start_transformation(&mut self, _t: Transformation) {}
    fn end_transformation(&mut self) {}
    fn reset(&mut self, _new_bounds: Rectangle) {}
    fn fill_quad(&mut self, _quad: renderer::Quad, _background: impl Into<Background>) {}
    fn allocate_image(
        &mut self,
        _handle: &image::Handle,
        _callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
    ) {
    }
}

impl text::Renderer for Stub {
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
    fn fill_paragraph(
        &mut self,
        _paragraph: &Self::Paragraph,
        _position: Point,
        _color: Color,
        _clip_bounds: Rectangle,
    ) {
    }
    fn fill_editor(
        &mut self,
        _editor: &Self::Editor,
        _position: Point,
        _color: Color,
        _clip_bounds: Rectangle,
    ) {
    }
    fn fill_text(
        &mut self,
        _text: text::Text,
        _position: Point,
        _color: Color,
        _clip_bounds: Rectangle,
    ) {
    }
}

impl iced_wgpu::primitive::Renderer for Stub {
    fn draw_primitive(&mut self, _bounds: Rectangle, _primitive: impl iced_wgpu::Primitive) {}
}

// ---------------------------------------------------------------------------
// Leaf child widget that records the viewport it was last drawn / updated with.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Capture(Rc<Cell<Option<Rectangle>>>);

struct ViewportRecorder {
    on_draw: Capture,
    on_update: Capture,
}

impl<Message> Widget<Message, Theme, Stub> for ViewportRecorder {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(40.0), Length::Fixed(20.0))
    }
    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Stub,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(40.0), Length::Fixed(20.0), Size::ZERO))
    }
    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Stub,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.on_draw.0.set(Some(*viewport));
        CHILD_CLIP.with(|c| c.set(LAYER_STACK.with(|s| s.borrow().last().copied())));
    }
    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Stub,
        _clipboard: &mut dyn clipboard::Clipboard,
        _shell: &mut iced_widget::core::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.on_update.0.set(Some(*viewport));
    }
}

impl<'a, Message: 'a> From<ViewportRecorder> for Element<'a, Message, Theme, Stub> {
    fn from(value: ViewportRecorder) -> Self {
        Element::new(value)
    }
}

// ---------------------------------------------------------------------------
// The actual regression test.
// ---------------------------------------------------------------------------

fn build_graph_with_recorder(
    graph_w: f32,
    graph_h: f32,
    node_world_pos: Point,
) -> (
    NodeGraph<'static, usize, usize, usize, (), Theme, Stub>,
    Capture, // on_draw
    Capture, // on_update
) {
    let on_draw = Capture(Rc::new(Cell::new(None)));
    let on_update = Capture(Rc::new(Cell::new(None)));
    let recorder = ViewportRecorder {
        on_draw: on_draw.clone(),
        on_update: on_update.clone(),
    };
    let mut graph: NodeGraph<'static, usize, usize, usize, (), Theme, Stub> = NodeGraph::default()
        .width(Length::Fixed(graph_w))
        .height(Length::Fixed(graph_h));
    graph.push_node(node(0_usize, node_world_pos, Element::from(recorder)));
    (graph, on_draw, on_update)
}

#[test]
fn draw_clips_child_viewport_to_graph_bounds() {
    // NodeGraph is 200x200; outer viewport (parent window) is 1024x768. A node
    // sits at world (500, 500), outside the graph's screen bounds. We expect
    // the viewport the child sees to be bounded by the graph, not by the
    // outer window.
    let (mut graph, on_draw, _on_update) =
        build_graph_with_recorder(200.0, 200.0, Point::new(500.0, 500.0));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Stub>);
    let mut renderer = Stub;
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);

    let outer_viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Light,
        &renderer::Style {
            text_color: Color::BLACK,
        },
        layout,
        mouse::Cursor::Unavailable,
        &outer_viewport,
    );

    let recorded = on_draw
        .0
        .get()
        .expect("recorder was never drawn — NodeGraph::draw did not reach child");

    // With the fix: clipped_viewport = layout.bounds() ∩ outer_viewport = 200x200
    // (camera default zoom=1, position=0,0, so world dims == screen dims).
    // Without the fix: child sees the full 1024x768 viewport transformed to world.
    assert!(
        recorded.width <= 200.0 && recorded.height <= 200.0,
        "child draw viewport {recorded:?} should be clipped to NodeGraph bounds (200x200)",
    );
}

#[test]
fn draw_clips_node_content_layer_to_graph_bounds() {
    // A node straddling the graph's right edge: its rect extends past x=200,
    // but the layer the node content (title bar, widgets) is drawn under must
    // be bounded by the graph, not by the node rect. iced's push_clip replaces
    // the parent clip, so the node-content clip has to be intersected with the
    // graph viewport explicitly. The recorder is 40 wide; placed at world
    // x=180 it spans [180, 220], 20px past the 200px graph edge.
    CHILD_CLIP.with(|c| c.set(None));
    let (mut graph, _on_draw, _on_update) =
        build_graph_with_recorder(200.0, 200.0, Point::new(180.0, 50.0));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Stub>);
    let mut renderer = Stub;
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);

    let outer_viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    graph.draw(
        &tree,
        &mut renderer,
        &Theme::Light,
        &renderer::Style {
            text_color: Color::BLACK,
        },
        layout,
        mouse::Cursor::Unavailable,
        &outer_viewport,
    );

    let clip = CHILD_CLIP
        .with(|c| c.get())
        .expect("node content was never drawn under a clip layer");

    // Default camera (zoom 1, no pan, graph at origin): layout space == screen
    // space, so the clip must stay within the 0..200 graph bounds.
    assert!(
        clip.x + clip.width <= 200.5,
        "node content clip {clip:?} must not extend past graph right edge (200)",
    );
}

// ---------------------------------------------------------------------------
// Event-propagation regression: NodeGraph must not consume wheel-scroll events
// when the cursor isn't actually over its bounds. `is_over` returns false
// both for `Levitating` cursors (covered by a sibling above in a `stack`) and
// for cursors geometrically outside the bounds.
// ---------------------------------------------------------------------------

fn wheel_event() -> iced::Event {
    iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: 5.0 },
    })
}

fn run_update_with_cursor(graph_w: f32, graph_h: f32, cursor: mouse::Cursor) -> Rc<Cell<bool>> {
    let mut base_graph: NodeGraph<'static, usize, usize, usize, (), Theme, Stub> =
        NodeGraph::default()
            .width(Length::Fixed(graph_w))
            .height(Length::Fixed(graph_h));
    base_graph.push_node(node(
        0_usize,
        Point::new(0.0, 0.0),
        Element::<(), _, _>::from(EmptyLeaf),
    ));

    let camera_changed = Rc::new(Cell::new(false));
    let cc = camera_changed.clone();
    let mut graph = base_graph.on_camera_change(move |_pos, _zoom| cc.set(true));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Stub>);
    let renderer = Stub;
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);

    let mut messages: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut messages);
    let mut clipboard = clipboard::Null;
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));

    graph.update(
        &mut tree,
        &wheel_event(),
        layout,
        cursor,
        &renderer,
        &mut clipboard,
        &mut shell,
        &viewport,
    );

    camera_changed
}

// Minimal no-op leaf (we only need a node to exist so push_node doesn't panic).
struct EmptyLeaf;
impl<Message> Widget<Message, Theme, Stub> for EmptyLeaf {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(10.0), Length::Fixed(10.0))
    }
    fn layout(&mut self, _: &mut Tree, _: &Stub, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(10.0), Length::Fixed(10.0), Size::ZERO))
    }
    fn draw(
        &self,
        _: &Tree,
        _: &mut Stub,
        _: &Theme,
        _: &renderer::Style,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
    }
}
impl<'a, Message: 'a> From<EmptyLeaf> for Element<'a, Message, Theme, Stub> {
    fn from(w: EmptyLeaf) -> Self {
        Element::new(w)
    }
}

#[test]
fn wheel_scroll_with_levitating_cursor_does_not_zoom() {
    // Levitating cursor = a sibling above (e.g. an `opaque` overlay) has
    // claimed mouse interaction at this position. NodeGraph must not zoom.
    let changed = run_update_with_cursor(
        200.0,
        200.0,
        mouse::Cursor::Levitating(Point::new(100.0, 100.0)),
    );
    assert!(
        !changed.get(),
        "wheel scroll under a levitating cursor must not change the camera",
    );
}

#[test]
fn wheel_scroll_outside_graph_bounds_does_not_zoom() {
    // Cursor is geometrically outside the NodeGraph's layout bounds (200x200).
    let changed = run_update_with_cursor(
        200.0,
        200.0,
        mouse::Cursor::Available(Point::new(500.0, 500.0)),
    );
    assert!(
        !changed.get(),
        "wheel scroll outside the graph bounds must not change the camera",
    );
}

#[test]
fn wheel_scroll_inside_graph_bounds_zooms() {
    // Happy-path control test: an Available cursor inside the bounds must
    // still drive the camera.
    let changed = run_update_with_cursor(
        200.0,
        200.0,
        mouse::Cursor::Available(Point::new(100.0, 100.0)),
    );
    assert!(
        changed.get(),
        "wheel scroll over the graph must still change the camera",
    );
}

#[test]
fn update_clips_child_viewport_to_graph_bounds() {
    let (mut graph, _on_draw, on_update) =
        build_graph_with_recorder(200.0, 200.0, Point::new(500.0, 500.0));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Stub>);
    let renderer = Stub;
    let layout_node = graph.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1024.0, 768.0)),
    );
    let layout = Layout::new(&layout_node);

    let outer_viewport = Rectangle::new(Point::ORIGIN, Size::new(1024.0, 768.0));
    let mut shell_messages: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut shell_messages);
    let mut clipboard = clipboard::Null;
    let event = iced::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(50.0, 50.0),
    });

    graph.update(
        &mut tree,
        &event,
        layout,
        mouse::Cursor::Available(Point::new(50.0, 50.0)),
        &renderer,
        &mut clipboard,
        &mut shell,
        &outer_viewport,
    );

    let recorded = on_update
        .0
        .get()
        .expect("recorder did not see an update event from NodeGraph");

    assert!(
        recorded.width <= 200.0 && recorded.height <= 200.0,
        "child update viewport {recorded:?} should be clipped to NodeGraph bounds (200x200)",
    );
}
