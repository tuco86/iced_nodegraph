//! Edge cutting tool primitive for NodeGraph.
//!
//! Renders the cutting line during edge cutting mode.

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

/// Primitive for rendering the edge cutting line.
#[derive(Debug, Clone)]
pub struct CuttingToolPrimitive {
    /// Start point of cutting line (in world coordinates)
    pub start: WorldPoint,
    /// End point of cutting line (in world coordinates)
    pub end: WorldPoint,
    /// Cutting line color
    pub color: Color,
    /// Camera zoom level
    pub camera_zoom: f32,
    /// Camera position in world coordinates
    pub camera_position: WorldPoint,
}

/// Pipeline for CuttingToolPrimitive rendering.
pub struct CuttingToolPipeline {
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

impl Pipeline for CuttingToolPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("cutting_tool_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy buffers (required by bind group layout)
        let dummy_nodes = device.create_buffer(&BufferDescriptor {
            label: Some("cutting_tool_dummy_nodes"),
            size: <types::Node as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_pins = device.create_buffer(&BufferDescriptor {
            label: Some("cutting_tool_dummy_pins"),
            size: <types::Pin as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_edges = device.create_buffer(&BufferDescriptor {
            label: Some("cutting_tool_dummy_edges"),
            size: <types::Edge as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_grids = device.create_buffer(&BufferDescriptor {
            label: Some("cutting_tool_dummy_grids"),
            size: <types::Grid as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("cutting_tool_bind_group"),
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

impl Primitive for CuttingToolPrimitive {
    type Pipeline = CuttingToolPipeline;

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
            camera_zoom: self.camera_zoom,
            camera_position: glam::Vec2::new(self.camera_position.x, self.camera_position.y),
            cursor_position: glam::Vec2::new(self.end.x, self.end.y),
            num_nodes: 0,
            time: 0.0,
            overlay_type: 7, // EdgeCutting
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
