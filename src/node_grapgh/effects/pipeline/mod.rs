use glam::vec4;
use iced::{
    Rectangle,
    wgpu::{
        Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device, Queue, TextureFormat,
        TextureView,
    },
};

use super::primitive::Primitive;

mod buffer;
mod types;

pub struct Pipeline {
    uniforms: Buffer,
    nodes: buffer::Buffer<types::Node>,
    pins: buffer::Buffer<types::Pin>,
    edges: buffer::Buffer<types::Edge>,
}

impl Pipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Create the pipeline here
        Self {
            uniforms: device.create_buffer(&BufferDescriptor {
                label: Some("uniform buffer"),
                size: std::mem::size_of::<types::Uniforms>() as u64,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            nodes: buffer::Buffer::new(
                device,
                Some("nodes buffer"),
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            pins: buffer::Buffer::new(
                device,
                Some("pins buffer"),
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            edges: buffer::Buffer::new(
                device,
                Some("edges buffer"),
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
        }
    }

    pub fn update(&mut self, device: &Device, queue: &Queue, primitive: &Primitive) {
        let uniforms = types::Uniforms {
            border_color: vec4(0.5, 0.6, 0.7, 1.0),
            fill_color: vec4(0.5, 0.3, 0.1, 1.0),
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let mut pin_start = 0;
        self.nodes.update(
            device,
            queue,
            primitive.nodes.iter().map(|node| {
                let (pin_start, pin_count) = {
                    let count = node.pins.len() as u32;
                    let start = pin_start;
                    pin_start += count;
                    (start, count)
                };
                types::Node {
                    position: node.position,
                    size: node.size,
                    corner_radius: node.corner_radius,
                    pin_start,
                    pin_count,
                }
            }),
        );

        self.pins.update(
            device,
            queue,
            primitive
                .nodes
                .iter()
                .flat_map(|node| node.pins.iter())
                .map(|pin| types::Pin {
                    side: pin.side,
                    offset: pin.offset,
                    radius: pin.radius,
                }),
        );

        self.edges.update(
            device,
            queue,
            primitive
                .edges
                .iter()
                .map(|((from_node, from_pin), (to_node, to_pin))| types::Edge {
                    from_node: *from_node as _,
                    from_pin: *from_pin as _,
                    to_node: *to_node as _,
                    to_pin: *to_pin as _,
                }),
        );

        println!(
            "nodes: {:?}, pins: {:?}, edges: {:?}",
            self.nodes.len(),
            self.pins.len(),
            self.edges.len()
        );
    }

    pub fn update_echo(&mut self, queue: &Queue) {
        // Update the echo pipeline here

        println!("Echo pipeline updated");
    }

    pub fn render(
        &self,
        target: &TextureView,
        encoder: &mut CommandEncoder,
        clip_bounds: Rectangle<u32>,
    ) {
        // Render the pipeline here
    }
}
