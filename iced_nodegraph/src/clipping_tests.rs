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

use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::{
    Background, Color, Element, Length, Pixels, Point, Rectangle, Size, Theme, Transformation,
};
use iced_widget::core::clipboard;
use iced_widget::core::image;
use iced_widget::core::text;

use crate::NodeGraph;

// ---------------------------------------------------------------------------
// Stub renderer: no-op implementations of every trait NodeGraph requires.
// The test never inspects pixel output, so all methods are empty.
// ---------------------------------------------------------------------------
struct Stub;

impl renderer::Renderer for Stub {
    fn start_layer(&mut self, _bounds: Rectangle) {}
    fn end_layer(&mut self) {}
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
    fn layout(&mut self, _tree: &mut Tree, _renderer: &Stub, limits: &layout::Limits) -> layout::Node {
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
    let mut graph: NodeGraph<'static, usize, usize, usize, (), Theme, Stub> =
        NodeGraph::default()
            .width(Length::Fixed(graph_w))
            .height(Length::Fixed(graph_h));
    graph.push_node(0_usize, node_world_pos, Element::from(recorder));
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

