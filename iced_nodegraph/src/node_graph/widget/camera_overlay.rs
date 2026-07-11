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
    pub(super) content: overlay::Element<'a, Message, iced::Theme, Renderer>,
    pub(super) camera: crate::node_graph::camera::Camera2D,
}

impl<Message, Renderer> overlay::Overlay<Message, iced::Theme, Renderer>
    for CameraOverlay<'_, Message, Renderer>
where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.content.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &iced::Theme,
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
    ) -> Option<overlay::Element<'c, Message, iced::Theme, Renderer>> {
        let camera = self.camera;
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| {
                overlay::Element::new(Box::new(CameraOverlay { content, camera })
                    as Box<dyn overlay::Overlay<Message, iced::Theme, Renderer>>)
            })
    }
}
