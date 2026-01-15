//! Box selection primitive for NodeGraph.
//!
//! Renders the selection rectangle during drag selection.

use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages, Device,
    Queue, TextureFormat,
};
use iced::{Color, Rectangle};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use crate::node_graph::euclid::WorldPoint;

use super::super::pipeline::types;
use super::super::shared::SharedNodeGraphResources;
use super::RenderContext;

/// Primitive for rendering the box selection rectangle.
#[derive(Debug, Clone)]
pub struct BoxSelectPrimitive {
    /// Shared rendering context
    pub context: RenderContext,
    /// Start corner of selection box (in world coordinates)
    pub start: WorldPoint,
    /// End corner of selection box (in world coordinates)
    pub end: WorldPoint,
    /// Selection box color (border and fill)
    pub color: Color,
}

/// Pipeline for BoxSelectPrimitive rendering.
pub struct BoxSelectPipeline {
    /// Shared resources (shader, pipelines, layouts)
    shared: Arc<SharedNodeGraphResources>,
    /// Uniform buffer
    uniforms: Buffer,
    /// Dummy node buffer (required by bind group layout)
    #[allow(dead_code)]
    dummy_nodes: Buffer,
    /// Dummy pin buffer (required by bind group layout)
    #[allow(dead_code)]
    dummy_pins: Buffer,
    /// Dummy edge buffer (required by bind group layout)
    #[allow(dead_code)]
    dummy_edges: Buffer,
    /// Dummy grids buffer (required by bind group layout)
    #[allow(dead_code)]
    dummy_grids: Buffer,
    /// Bind group for rendering
    bind_group: BindGroup,
}

impl Pipeline for BoxSelectPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("select_box_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy buffers (required by bind group layout)
        let dummy_nodes = device.create_buffer(&BufferDescriptor {
            label: Some("select_box_dummy_nodes"),
            size: <types::Node as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_pins = device.create_buffer(&BufferDescriptor {
            label: Some("select_box_dummy_pins"),
            size: <types::Pin as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_edges = device.create_buffer(&BufferDescriptor {
            label: Some("select_box_dummy_edges"),
            size: <types::Edge as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_grids = device.create_buffer(&BufferDescriptor {
            label: Some("select_box_dummy_grids"),
            size: <types::Grid as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("select_box_bind_group"),
            layout: &shared.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: dummy_nodes.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: dummy_pins.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: dummy_edges.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: dummy_grids.as_entire_binding(),
                },
            ],
        });

        Self {
            shared,
            uniforms,
            dummy_nodes,
            dummy_pins,
            dummy_edges,
            dummy_grids,
            bind_group,
        }
    }
}

impl Primitive for BoxSelectPrimitive {
    type Pipeline = BoxSelectPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        _device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor();

        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom: self.context.camera_zoom,
            camera_position: glam::Vec2::new(
                self.context.camera_position.x,
                self.context.camera_position.y,
            ),
            cursor_position: glam::Vec2::new(self.end.x, self.end.y),
            num_nodes: 0,
            time: self.context.time,
            overlay_type: 5, // BoxSelect
            overlay_start: glam::Vec2::new(self.start.x, self.start.y),
            overlay_color: glam::Vec4::new(
                self.color.r,
                self.color.g,
                self.color.b,
                self.color.a,
            ),
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
        };

        let mut uniform_buffer = encase::UniformBuffer::new(Vec::new());
        uniform_buffer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniforms, 0, uniform_buffer.as_ref());
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        render_pass.set_pipeline(&pipeline.shared.overlay_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..6, 0..1); // Fullscreen quad
        true
    }
}
