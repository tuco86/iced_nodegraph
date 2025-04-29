mod node;
mod pin;

use iced::{
    Rectangle, wgpu,
    widget::shader::{self, Viewport},
};
pub use node::Node;
pub use pin::Pin;

use crate::node_grapgh::state::Dragging;

use super::pipeline::Pipeline;

#[derive(Debug, Clone, Copy)]
pub enum Layer {
    Background,
    Foreground,
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub layer: Layer,
    pub dragging: Dragging,
    pub nodes: Vec<Node>,
    pub edges: Vec<((usize, usize), (usize, usize))>, // (from_node, from_pin) -> (to_node, to_pin)
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if !storage.has::<Pipeline>() {
            storage.store(Pipeline::new(device, format));
        }
        let pipeline = storage.get_mut::<Pipeline>().unwrap();
        pipeline.update(device, queue, self);
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<Pipeline>().unwrap();
        pipeline.render(target, encoder, *clip_bounds, self.layer);
    }
}

#[derive(Debug)]
pub struct EchoPrimitive;

impl shader::Primitive for EchoPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if storage.has::<Pipeline>() {
            storage.store(Pipeline::new(device, format));
        }
        let pipeline = storage.get_mut::<Pipeline>().unwrap();
        pipeline.update_echo(queue);
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<Pipeline>().unwrap();
        pipeline.render(target, encoder, *clip_bounds, Layer::Foreground);
    }
}
