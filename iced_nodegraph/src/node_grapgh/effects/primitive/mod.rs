mod node;
mod pin;

use iced::{Rectangle, wgpu};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Primitive;
pub use node::Node;
pub use pin::Pin;

use crate::node_grapgh::{euclid::WorldPoint, state::Dragging};

use super::pipeline::Pipeline;

#[derive(Debug, Clone, Copy)]
pub enum Layer {
    Background,
    Foreground,
}

#[derive(Debug, Clone)]
pub struct NodeGraphPrimitive {
    pub layer: Layer,
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,
    pub cursor_position: WorldPoint,
    pub time: f32, // Time in seconds for animations
    pub dragging: Dragging,
    pub nodes: Vec<Node>,
    pub edges: Vec<((usize, usize), (usize, usize))>, // (from_node, from_pin) -> (to_node, to_pin)
    pub edge_color: glam::Vec4,
    pub background_color: glam::Vec4,
    pub border_color: glam::Vec4,
    pub fill_color: glam::Vec4,
    pub drag_edge_color: glam::Vec4,
    pub drag_edge_valid_color: glam::Vec4,
}

impl Primitive for NodeGraphPrimitive {
    type Renderer = Pipeline;

    fn initialize(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self::Renderer {
        Pipeline::new(device, format)
    }

    fn prepare(
        &self,
        renderer: &mut Self::Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        renderer.update_new(
            device,
            queue,
            viewport,
            self.camera_zoom,
            self.camera_position,
            self.cursor_position,
            self.time,
            &self.dragging,
            &self.nodes,
            &self.edges,
            self.edge_color,
            self.background_color,
            self.border_color,
            self.fill_color,
            self.drag_edge_color,
            self.drag_edge_valid_color,
        );
    }

    fn draw(
        &self,
        renderer: &Self::Renderer,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        // Use default viewport - this should come from the bounds in practice
        let viewport = Rectangle {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        renderer.render_pass(render_pass, viewport, self.layer);
        true // We handle the drawing ourselves
    }
}
