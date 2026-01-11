//! Grid background primitive for NodeGraph.
//!
//! Renders the background pattern (grid, dots, hex, etc.) behind all other elements.

use std::sync::Arc;

use encase::ShaderSize;
use iced::Rectangle;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages, Device,
    Queue, TextureFormat,
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use crate::node_graph::euclid::WorldPoint;
use crate::style::BackgroundStyle;

use super::super::pipeline::types;
use super::super::shared::SharedNodeGraphResources;

/// Primitive for rendering the background grid pattern.
#[derive(Debug, Clone)]
pub struct GridPrimitive {
    /// Camera zoom level
    pub camera_zoom: f32,
    /// Camera position in world coordinates
    pub camera_position: WorldPoint,
    /// Background style configuration
    pub background_style: BackgroundStyle,
}

/// Pipeline for GridPrimitive rendering.
///
/// Holds shared GPU resources and grid-specific buffers.
pub struct GridPipeline {
    /// Shared resources (shader, pipelines, layouts)
    shared: Arc<SharedNodeGraphResources>,
    /// Uniform buffer for grid-specific data
    uniforms: Buffer,
    /// Dummy node buffer (required by bind group layout but not read)
    #[allow(dead_code)]
    dummy_nodes: Buffer,
    /// Dummy pin buffer (required by bind group layout but not read)
    #[allow(dead_code)]
    dummy_pins: Buffer,
    /// Dummy edge buffer (required by bind group layout but not read)
    #[allow(dead_code)]
    dummy_edges: Buffer,
    /// Bind group for rendering
    bind_group: BindGroup,
}

impl Pipeline for GridPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("grid_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy buffers (required by bind group layout but not used)
        let dummy_nodes = device.create_buffer(&BufferDescriptor {
            label: Some("grid_dummy_nodes"),
            size: <types::Node as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_pins = device.create_buffer(&BufferDescriptor {
            label: Some("grid_dummy_pins"),
            size: <types::Pin as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_edges = device.create_buffer(&BufferDescriptor {
            label: Some("grid_dummy_edges"),
            size: <types::Edge as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("grid_bind_group"),
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
            ],
        });

        Self {
            shared,
            uniforms,
            dummy_nodes,
            dummy_pins,
            dummy_edges,
            bind_group,
        }
    }
}

impl Primitive for GridPrimitive {
    type Pipeline = GridPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        _device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor();
        let style = &self.background_style;

        // Build uniforms with only the fields needed for grid rendering
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom: self.camera_zoom,
            camera_position: glam::Vec2::new(self.camera_position.x, self.camera_position.y),
            background_color: glam::Vec4::new(
                style.primary_color.r,
                style.primary_color.g,
                style.primary_color.b,
                style.primary_color.a,
            ),
            cursor_position: glam::Vec2::ZERO,
            num_nodes: 0,
            time: 0.0,
            overlay_type: 0,
            overlay_start: glam::Vec2::ZERO,
            hover_glow_color: glam::Vec4::ZERO,
            selection_box_color: glam::Vec4::ZERO,
            edge_cutting_color: glam::Vec4::ZERO,
            hover_glow_radius: 0.0,
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),

            // Background pattern configuration
            bg_pattern_type: style.pattern.type_id(),
            bg_flags: (if style.adaptive_zoom { 1u32 } else { 0 })
                | (if style.hex_pointy_top { 2u32 } else { 0 }),
            bg_minor_spacing: style.minor_spacing,
            bg_major_ratio: style
                .major_spacing
                .map(|m| m / style.minor_spacing)
                .unwrap_or(0.0),
            bg_line_widths: glam::Vec2::new(style.minor_width, style.major_width),
            bg_opacities: glam::Vec2::new(style.minor_opacity, style.major_opacity),
            bg_primary_color: glam::Vec4::new(
                style.primary_color.r,
                style.primary_color.g,
                style.primary_color.b,
                style.primary_color.a,
            ),
            bg_secondary_color: glam::Vec4::new(
                style.secondary_color.r,
                style.secondary_color.g,
                style.secondary_color.b,
                style.secondary_color.a,
            ),
            bg_pattern_params: glam::Vec4::new(
                style.dot_radius,
                style.line_angle,
                style.crosshatch_angle,
                0.0,
            ),
            bg_adaptive_params: glam::Vec4::new(
                style.adaptive_min_spacing,
                style.adaptive_max_spacing,
                style.adaptive_fade_range,
                0.0,
            ),
        };

        // Write uniforms using encase
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
        render_pass.set_pipeline(&pipeline.shared.grid_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1); // Fullscreen triangle
        true
    }
}
