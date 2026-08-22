//! Recording fake renderer for argument-level assertions.
//!
//! `NodeGraph` requires `core::Renderer + iced_wgpu::primitive::Renderer`, but
//! the guarantees these tests defend - clipped viewports, world->screen
//! placement, culling, draw-call order - live in the *argument values* handed to
//! the renderer, not in pixel output. A fake renderer records those arguments
//! and needs no GPU; the pixel oracles that genuinely need one use the headless
//! harness in the parent module instead.
//!
//! The non-obvious part is the transformation stack: `iced_graphics` composes a
//! pushed transformation onto the current one (`child = current * transformation`),
//! so multiplying a drawn rect by the current entry maps it back to absolute
//! screen pixels. Clips go through the same composition: `push_clip` bakes the
//! transformation active at entry into the pushed rect and REPLACES the parent
//! clip rather than intersecting it, so a recorded clip is an absolute screen
//! rect, the innermost entry is the effective clip, and any intersection with an
//! ancestor must have been done by the widget under test.

use std::cell::RefCell;
use std::rc::Rc;

use iced::advanced::renderer;
use iced::{Background, Color, Pixels, Point, Rectangle, Transformation};
use iced_wgpu::core::image;
use iced_wgpu::core::text;

/// Everything a [`Recorder`] captures while it is drawn into.
#[derive(Debug, Default, Clone)]
pub struct Recorded {
    /// Absolute screen rects of `fill_quad` calls (transformation applied).
    pub quads: Vec<Rectangle>,
    /// Bounds handed to `draw_primitive` (SDF layers), in order.
    pub primitives: Vec<Rectangle>,
    /// Unified draw-call stream in call order, across both `fill_quad`
    /// (hosted content) and `draw_primitive` (SDF layers). Lets a test assert
    /// the per-node SDF/content/SDF sandwich order the host integration relies
    /// on, which the two separate vecs above lose.
    pub events: Vec<DrawEvent>,
}

/// One ordered draw call captured by [`Recorder`], tagged by source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawEvent {
    /// A hosted-content quad (`fill_quad`) at this absolute rect.
    Content(Rectangle),
    /// An SDF layer (`draw_primitive`) at this absolute clip rect.
    Sdf(Rectangle),
}

/// Fake renderer satisfying every trait bound `NodeGraph` imposes, recording the
/// draw calls it receives into a shared [`Recorded`].
pub struct Recorder {
    transformations: Vec<Transformation>,
    layers: Vec<Rectangle>,
    out: Rc<RefCell<Recorded>>,
}

impl Recorder {
    /// A recorder writing into `out`, which the test keeps a handle to.
    pub fn new(out: Rc<RefCell<Recorded>>) -> Self {
        Self {
            transformations: vec![Transformation::IDENTITY],
            layers: Vec::new(),
            out,
        }
    }

    /// A recorder whose output nobody reads, for tests that assert on what a
    /// probe widget was handed rather than on what was drawn.
    pub fn detached() -> Self {
        Self::new(Rc::default())
    }

    /// The innermost active clip, or `None` outside any layer. A probe widget
    /// calls this from its own `draw` to capture the clip it paints under.
    pub fn clip(&self) -> Option<Rectangle> {
        self.layers.last().copied()
    }

    fn current(&self) -> Transformation {
        *self.transformations.last().unwrap()
    }
}

impl renderer::Renderer for Recorder {
    fn start_layer(&mut self, bounds: Rectangle) {
        self.layers.push(bounds * self.current());
    }
    fn end_layer(&mut self) {
        self.layers.pop();
    }
    fn start_transformation(&mut self, transformation: Transformation) {
        self.transformations.push(self.current() * transformation);
    }
    fn end_transformation(&mut self) {
        self.transformations.pop();
        if self.transformations.is_empty() {
            self.transformations.push(Transformation::IDENTITY);
        }
    }
    fn reset(&mut self, _new_bounds: Rectangle) {}
    fn fill_quad(&mut self, quad: renderer::Quad, _background: impl Into<Background>) {
        let abs = quad.bounds * self.current();
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

impl text::Renderer for Recorder {
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

impl iced_wgpu::primitive::Renderer for Recorder {
    fn draw_primitive(&mut self, bounds: Rectangle, _primitive: impl iced_wgpu::Primitive) {
        let abs = bounds * self.current();
        let mut out = self.out.borrow_mut();
        out.primitives.push(abs);
        out.events.push(DrawEvent::Sdf(abs));
    }
}
