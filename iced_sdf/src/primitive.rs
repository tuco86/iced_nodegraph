//! SDF rendering primitive for Iced.
//!
//! Each `SdfPrimitive` represents a single SDF shape. Multiple primitives
//! share the same `SdfPipeline`, which batches them into shared GPU buffers
//! automatically via Iced's prepare/draw cycle.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages, Device, Queue,
    TextureFormat,
};
use iced::Rectangle;
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use crate::compile::compile;
use crate::layer::Layer;
use crate::pipeline::{buffer, types};
use crate::shape::Sdf;
use crate::shared::SharedSdfResources;

/// A single SDF shape primitive for rendering.
///
/// Each primitive carries its SDF tree and styling. During `prepare()`,
/// the shape is compiled to RPN ops and pushed into the pipeline's shared
/// GPU buffers. During `draw()`, the shape's instanced quad is rendered.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    /// The SDF shape to render.
    pub shape: Sdf,
    /// Rendering layers (back to front).
    pub layers: Vec<Layer>,
    /// Screen-space bounding box [x, y, width, height] for the instanced quad.
    pub screen_bounds: [f32; 4],
    /// Camera position (world origin offset).
    pub camera_position: (f32, f32),
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Animation time in seconds.
    pub time: f32,
}

impl SdfPrimitive {
    /// Create a new SDF primitive with default settings.
    pub fn new(shape: Sdf) -> Self {
        Self {
            shape,
            layers: vec![Layer::solid(iced::Color::WHITE)],
            screen_bounds: [0.0, 0.0, 100.0, 100.0],
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

    /// Set the screen-space bounding box for the instanced quad.
    pub fn screen_bounds(mut self, bounds: [f32; 4]) -> Self {
        self.screen_bounds = bounds;
        self
    }

    /// Set camera position and zoom.
    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    /// Set animation time.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }
}

/// Shared pipeline for all SDF primitives.
///
/// Accumulates shape data from multiple `prepare()` calls into shared GPU
/// buffers, then renders all shapes via instanced draw calls.
pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    uniform_buffer: iced::wgpu::Buffer,
    shapes_buffer: buffer::Buffer<types::ShapeInstance>,
    ops_buffer: buffer::Buffer<types::SdfOp>,
    layers_buffer: buffer::Buffer<types::SdfLayer>,
    bind_group: BindGroup,
    bind_group_generations: (u64, u64, u64),
    /// Slot counter incremented during prepare() to track shape index.
    prepare_slot: u32,
    /// Atomic slot counter for draw() to match prepare order.
    draw_slot: AtomicU32,
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shapes_buffer = buffer::Buffer::new(
            device,
            Some("sdf_shapes_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let ops_buffer = buffer::Buffer::new(
            device,
            Some("sdf_ops_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let layers_buffer = buffer::Buffer::new(
            device,
            Some("sdf_layers_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

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
                    resource: shapes_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: ops_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: layers_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            shared,
            uniform_buffer,
            shapes_buffer,
            ops_buffer,
            layers_buffer,
            bind_group,
            bind_group_generations: (0, 0, 0),
            prepare_slot: 0,
            draw_slot: AtomicU32::new(0),
        }
    }

    fn trim(&mut self) {
        self.shapes_buffer.clear();
        self.ops_buffer.clear();
        self.layers_buffer.clear();
        self.prepare_slot = 0;
        self.draw_slot.store(0, Ordering::Relaxed);
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
        // Compile SDF tree to RPN ops
        let compiled_ops = compile(self.shape.node());
        let ops_offset = pipeline.ops_buffer.len() as u32;
        let ops_count = compiled_ops.len() as u32;

        for op in &compiled_ops {
            let _ = pipeline.ops_buffer.push(device, queue, op.clone());
        }

        // Convert and push layers
        let layers_offset = pipeline.layers_buffer.len() as u32;
        let layers_count = self.layers.len() as u32;

        for layer in &self.layers {
            let _ = pipeline.layers_buffer.push(device, queue, layer.to_gpu());
        }

        // Push shape instance
        let _ = pipeline.shapes_buffer.push(
            device,
            queue,
            types::ShapeInstance {
                bounds: glam::Vec4::new(
                    self.screen_bounds[0],
                    self.screen_bounds[1],
                    self.screen_bounds[2],
                    self.screen_bounds[3],
                ),
                ops_offset,
                ops_count,
                layers_offset,
                layers_count,
            },
        );

        pipeline.prepare_slot += 1;

        // Write uniforms every prepare (bounds may change per-primitive)
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            camera_position: glam::Vec2::new(
                self.camera_position.0,
                self.camera_position.1,
            ),
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            num_ops: pipeline.ops_buffer.len() as u32,
            num_layers: pipeline.layers_buffer.len() as u32,
            _pad: 0,
        };

        let mut uniform_buffer = encase::UniformBuffer::new(Vec::new());
        uniform_buffer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniform_buffer, 0, uniform_buffer.as_ref());

        // Recreate bind group if any buffer was resized
        let current_generations = (
            pipeline.shapes_buffer.generation(),
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
                        resource: pipeline.shapes_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.ops_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
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
        let slot = pipeline.draw_slot.fetch_add(1, Ordering::Relaxed);

        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        // Draw one instance (6 vertices for the quad) at the correct instance offset
        render_pass.draw(0..6, slot..slot + 1);

        true
    }
}
