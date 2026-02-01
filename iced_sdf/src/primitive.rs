//! SDF rendering primitive for Iced.
//!
//! Implements the `iced_wgpu::primitive::Primitive` trait for rendering
//! SDF shapes with the GPU pipeline.

use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages, Device, Queue,
    TextureFormat,
};
use iced::{Color, Rectangle};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use crate::compile::compile;
use crate::layer::Layer;
use crate::pipeline::{buffer, types};
use crate::shape::Sdf;
use crate::shared::SharedSdfResources;

/// Primitive for rendering an SDF shape with layers.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    /// The SDF shape to render.
    pub shape: Sdf,
    /// Rendering layers (back to front).
    pub layers: Vec<Layer>,
    /// Camera position (world origin offset).
    pub camera_position: (f32, f32),
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Animation time in seconds.
    pub time: f32,
}

impl SdfPrimitive {
    /// Create a new SDF primitive.
    pub fn new(shape: Sdf) -> Self {
        Self {
            shape,
            layers: vec![Layer::solid(Color::WHITE)],
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
        }
    }

    /// Set rendering layers.
    pub fn layers(mut self, layers: Vec<Layer>) -> Self {
        self.layers = layers;
        self
    }

    /// Set camera position.
    pub fn camera_position(mut self, x: f32, y: f32) -> Self {
        self.camera_position = (x, y);
        self
    }

    /// Set camera zoom.
    pub fn camera_zoom(mut self, zoom: f32) -> Self {
        self.camera_zoom = zoom;
        self
    }

    /// Set animation time.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }
}

/// Pipeline for SDF rendering.
pub struct SdfPipeline {
    /// Shared resources (shader, pipeline, layouts).
    shared: Arc<SharedSdfResources>,
    /// Uniform buffer.
    uniform_buffer: iced::wgpu::Buffer,
    /// SDF operations buffer.
    ops_buffer: buffer::Buffer<types::SdfOp>,
    /// Layers buffer.
    layers_buffer: buffer::Buffer<types::SdfLayer>,
    /// Bind group.
    bind_group: BindGroup,
    /// Bind group generation tracking.
    bind_group_generations: (u64, u64),
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create ops buffer
        let ops_buffer = buffer::Buffer::new(
            device,
            Some("sdf_ops_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create layers buffer
        let layers_buffer = buffer::Buffer::new(
            device,
            Some("sdf_layers_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create initial bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf_bind_group"),
            layout: &shared.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: ops_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: layers_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            shared,
            uniform_buffer,
            ops_buffer,
            layers_buffer,
            bind_group,
            bind_group_generations: (0, 0),
        }
    }

    fn trim(&mut self) {
        self.ops_buffer.clear();
        self.layers_buffer.clear();
    }
}

impl Primitive for SdfPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Compile SDF to RPN operations
        let ops = compile(self.shape.node());

        // Push operations to buffer
        for op in &ops {
            let _ = pipeline.ops_buffer.push(device, queue, op.clone());
        }

        // Convert and push layers
        for layer in &self.layers {
            let _ = pipeline.layers_buffer.push(device, queue, layer.to_gpu());
        }

        // Update uniforms
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            viewport_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            camera_position: glam::Vec2::new(self.camera_position.0, self.camera_position.1),
            camera_zoom: self.camera_zoom,
            time: self.time,
            num_ops: ops.len() as u32,
            num_layers: self.layers.len() as u32,
        };

        let mut uniform_buffer = encase::UniformBuffer::new(Vec::new());
        uniform_buffer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniform_buffer, 0, uniform_buffer.as_ref());

        // Recreate bind group if buffers were resized
        let current_generations = (
            pipeline.ops_buffer.generation(),
            pipeline.layers_buffer.generation(),
        );
        if current_generations != pipeline.bind_group_generations {
            pipeline.bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("sdf_bind_group"),
                layout: &pipeline.shared.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniform_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: pipeline.ops_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.layers_buffer.as_entire_binding(),
                    },
                ],
            });
            pipeline.bind_group_generations = current_generations;
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        // Draw fullscreen triangle
        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        true
    }
}
