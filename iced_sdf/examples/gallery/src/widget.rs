//! Custom widget that renders an SDF shape using iced_sdf.

use iced::widget::container;
use iced::{Element, Fill, Length, Rectangle, Size, Theme};
use iced_sdf::SdfPrimitive;

use crate::shapes::ShapeEntry;

/// Create an SDF canvas element that renders a shape entry.
pub fn sdf_canvas<'a>(entry: &ShapeEntry, time: f32) -> Element<'a, crate::Message> {
    let shape = (entry.build)();
    let layers = (entry.layers)();

    let canvas = SdfCanvas {
        shape,
        layers,
        time,
        extent: entry.extent,
    };

    container(canvas)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}

/// Widget that renders a single SDF shape centered in its bounds.
struct SdfCanvas {
    shape: iced_sdf::Sdf,
    layers: Vec<iced_sdf::Layer>,
    time: f32,
    extent: f32,
}

impl<Message, Renderer> iced::advanced::Widget<Message, Theme, Renderer> for SdfCanvas
where
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        let size = limits
            .width(Length::Fill)
            .height(Length::Fill)
            .resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let center_x = bounds.x + bounds.width / 2.0;
        let center_y = bounds.y + bounds.height / 2.0;

        // Auto-zoom: fit shape extent into 2/3 of the smaller viewport dimension
        let viewport_min = bounds.width.min(bounds.height);
        let zoom = viewport_min * 0.333 / self.extent;

        let primitive = SdfPrimitive::new(self.shape.clone())
            .layers(self.layers.clone())
            .screen_bounds([bounds.x, bounds.y, bounds.width, bounds.height])
            .camera(center_x / zoom, center_y / zoom, zoom)
            .time(self.time);

        renderer.draw_primitive(bounds, primitive);
    }
}

impl<'a, Message: 'a, Renderer> From<SdfCanvas> for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(canvas: SdfCanvas) -> Self {
        Element::new(canvas)
    }
}
