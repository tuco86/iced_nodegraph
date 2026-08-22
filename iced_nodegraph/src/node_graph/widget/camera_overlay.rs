//! [`CameraOverlay`]: camera-aware wrapper for node pop-out overlays.

use super::*;

/// Camera-aware wrapper for node pop-out overlays (combo box menus, tooltips).
///
/// Node elements lay out — and produce their overlays — in the widget's
/// layout-absolute space, while node content is drawn through the camera
/// transform. This wrapper applies that same transform to the pop-out so it
/// stays anchored to and scales with the node beneath it, and maps the screen
/// cursor back into layout-absolute space for the wrapped overlay's
/// hit-testing (the inverse of the draw transform, mirroring
/// [`Camera2D::cursor_screen_to_layout`]).
pub(super) struct CameraOverlay<'a, Message, Renderer> {
    pub(super) content: overlay::Element<'a, Message, Theme, Renderer>,
    pub(super) camera: crate::node_graph::camera::Camera2D,
}

impl<Message, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for CameraOverlay<'_, Message, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    /// Lays the content out against the window expressed in layout-absolute
    /// units, while the node this returns keeps the window's own size.
    ///
    /// The two halves answer to different consumers. The content places itself
    /// in layout-absolute space, so the room it has is the window divided by
    /// zoom - a menu deciding whether it fits below its anchor has to be told
    /// that, or at zoom 2 it believes it has twice the space. The runtime, on
    /// the other hand, clips each overlay by the bounds of the node returned
    /// here and does so outside this wrapper's transform
    /// (`overlay::Nested::draw`), so that size has to stay the untransformed
    /// window or the clip would cut the pop-out down to a fraction of the
    /// screen.
    ///
    /// The region's ORIGIN is the accepted limitation: `Overlay::layout` carries
    /// only a `Size`, so under pan the content is told how much room it has but
    /// not where the room starts, and an edge-flip decision can misjudge by the
    /// pan distance.
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let zoom = self.camera.zoom();
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
        let cursor = self.camera.cursor_screen_to_layout(cursor);
        renderer.with_transformation(self.camera.layer_transformation(), |renderer| {
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
        let cursor = self.camera.cursor_screen_to_layout(cursor);
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
        let cursor = self.camera.cursor_screen_to_layout(cursor);
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
        let camera = self.camera;
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| {
                overlay::Element::new(Box::new(CameraOverlay { content, camera })
                    as Box<dyn overlay::Overlay<Message, Theme, Renderer>>)
            })
    }
}
