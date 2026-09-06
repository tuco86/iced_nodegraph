//! [`CameraOverlay`]: camera-aware wrapper for node pop-out overlays.

use super::*;
use iced_widget::core::Transformation;

/// Camera-aware wrapper for node pop-out overlays (combo box menus, tooltips).
///
/// The pop-outs live in ZOOMED-SCREEN space: screen pixels divided by the
/// zoom, with the window's top-left corner at the origin. Node elements lay
/// out in the widget's layout-absolute space, so [`NodeGraph::overlay`] hands
/// them [`Camera2D::overlay_translation`], the offset that carries a layout
/// position into this space; a pop-out anchors at `layout.position() +
/// translation`, as every iced pop-out does. From there the wrapper only has
/// to scale: it lays the content out against `window / zoom`, draws it under
/// `scale(zoom)`, and maps the screen cursor by `1 / zoom` for hit-testing.
///
/// This is the one space in which iced's own pop-outs place themselves
/// correctly: a menu decides whether it fits below its anchor and how wide it
/// may be from `bounds - position`, which needs the anchor and the room to
/// share an origin. The layout-absolute space cannot offer that under pan: a
/// node far from the world origin has layout coordinates beyond `window /
/// zoom` however centered it is on screen, and the menu concludes it has no
/// room at all.
///
/// [`NodeGraph::overlay`]: crate::NodeGraph
pub(super) struct CameraOverlay<'a, Message, Theme, Renderer> {
    pub(super) content: overlay::Element<'a, Message, Theme, Renderer>,
    pub(super) zoom: f32,
}

impl<Message, Theme, Renderer> CameraOverlay<'_, Message, Theme, Renderer> {
    /// Screen cursor to zoomed-screen space.
    fn cursor(&self, cursor: mouse::Cursor) -> mouse::Cursor {
        let inv = 1.0 / self.zoom;
        let map = |p: Point| Point::new(p.x * inv, p.y * inv);
        match cursor {
            mouse::Cursor::Available(p) => mouse::Cursor::Available(map(p)),
            mouse::Cursor::Levitating(p) => mouse::Cursor::Levitating(map(p)),
            mouse::Cursor::Unavailable => mouse::Cursor::Unavailable,
        }
    }
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for CameraOverlay<'_, Message, Theme, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    /// Lays the content out against the window in zoomed-screen units, while
    /// the node this returns keeps the window's own size.
    ///
    /// The two halves answer to different consumers. The content places itself
    /// in zoomed-screen space, so the room it has is the window divided by
    /// zoom - a menu deciding whether it fits below its anchor has to be told
    /// that, or at zoom 2 it believes it has twice the space. The runtime, on
    /// the other hand, clips each overlay by the bounds of the node returned
    /// here and does so outside this wrapper's transform
    /// (`overlay::Nested::draw`), so that size has to stay the untransformed
    /// window or the clip would cut the pop-out down to a fraction of the
    /// screen.
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let zoom = self.zoom;
        let content = self.content.as_overlay_mut().layout(
            renderer,
            Size::new(bounds.width / zoom, bounds.height / zoom),
        );
        layout::Node::with_children(bounds, content.children().to_vec())
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let cursor = self.cursor(cursor);
        renderer.with_transformation(Transformation::scale(self.zoom), |renderer| {
            self.content
                .as_overlay()
                .draw(renderer, theme, style, layout, cursor);
        });
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let cursor = self.cursor(cursor);
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let cursor = self.cursor(cursor);
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        let zoom = self.zoom;
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| {
                overlay::Element::new(Box::new(CameraOverlay { content, zoom })
                    as Box<dyn overlay::Overlay<Message, Theme, Renderer>>)
            })
    }
}
