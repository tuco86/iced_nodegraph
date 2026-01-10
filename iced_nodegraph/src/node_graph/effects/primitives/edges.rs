//! Batched edges primitive for NodeGraph.
//!
//! Renders all edges in a single batched draw call.
//! No layer parameter - the layer is determined by where the widget calls draw_primitive().

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
use crate::node_pin::PinDirection;
use crate::style::EdgeStyle;

use super::super::pipeline::{buffer, types};
use super::super::shared::SharedNodeGraphResources;

/// Pre-resolved edge data for GPU rendering.
///
/// Contains world-space positions (already resolved from node/pin indices)
/// and all styling information needed for rendering.
#[derive(Debug, Clone)]
pub struct EdgeRenderData {
    /// Start position in world coordinates
    pub start_pos: WorldPoint,
    /// End position in world coordinates
    pub end_pos: WorldPoint,
    /// Start pin side (0=Left, 1=Right, 2=Top, 3=Bottom)
    pub start_side: u32,
    /// End pin side
    pub end_side: u32,
    /// Start pin direction (Input/Output/Both)
    pub start_direction: PinDirection,
    /// End pin direction (Input/Output/Both)
    pub end_direction: PinDirection,
    /// Edge style
    pub style: EdgeStyle,
    /// Whether edge is highlighted (both ends selected)
    pub is_selected: bool,
    /// Whether edge is pending cut
    pub is_pending_cut: bool,
    /// Start pin color (for gradient resolution)
    pub start_pin_color: iced::Color,
    /// End pin color (for gradient resolution)
    pub end_pin_color: iced::Color,
}

/// Primitive for batched edge rendering.
#[derive(Debug, Clone)]
pub struct EdgesPrimitive {
    /// All edges to render
    pub edges: Vec<EdgeRenderData>,
    /// Camera zoom level
    pub camera_zoom: f32,
    /// Camera position in world coordinates
    pub camera_position: WorldPoint,
    /// Time for animations
    pub time: f32,
    /// Color for selected edges
    pub selected_edge_color: glam::Vec4,
}

/// Pipeline for EdgesPrimitive rendering.
pub struct EdgesPipeline {
    /// Shared resources (shader, pipelines, layouts)
    shared: Arc<SharedNodeGraphResources>,
    /// Uniform buffer
    uniforms: Buffer,
    /// Dummy node buffer (required by bind group layout)
    dummy_nodes: Buffer,
    /// Dummy pin buffer (required by bind group layout)
    dummy_pins: Buffer,
    /// Edge storage buffer
    edges: buffer::Buffer<types::Edge>,
    /// Bind group for rendering
    bind_group: BindGroup,
    /// Bind group generation for recreation tracking
    bind_group_generation: u64,
}

impl Pipeline for EdgesPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("edges_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy buffers
        let dummy_nodes = device.create_buffer(&BufferDescriptor {
            label: Some("edges_dummy_nodes"),
            size: <types::Node as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_pins = device.create_buffer(&BufferDescriptor {
            label: Some("edges_dummy_pins"),
            size: <types::Pin as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create edge buffer (dynamic)
        let edges = buffer::Buffer::new(
            device,
            Some("edges_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("edges_bind_group"),
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
                    resource: edges.as_entire_binding(),
                },
            ],
        });

        Self {
            shared,
            uniforms,
            dummy_nodes,
            dummy_pins,
            edges,
            bind_group,
            bind_group_generation: 0,
        }
    }
}

impl Primitive for EdgesPrimitive {
    type Pipeline = EdgesPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Update edge buffer
        let num_edges = pipeline.edges.update(
            device,
            queue,
            self.edges.iter().map(|edge| {
                let style = &edge.style;

                // Get stroke layer
                let stroke = style.stroke.as_ref();
                let (stroke_start, stroke_end, thickness) = stroke
                    .map(|s| (s.start_color, s.end_color, s.width))
                    .unwrap_or((iced::Color::TRANSPARENT, iced::Color::TRANSPARENT, 2.0));

                // Resolve edge gradient colors
                let (start_color, end_color) = if edge.is_selected {
                    (self.selected_edge_color, self.selected_edge_color)
                } else {
                    // Resolve start color: explicit or pin color
                    let start = if stroke_start.a > 0.01 {
                        glam::Vec4::new(
                            stroke_start.r,
                            stroke_start.g,
                            stroke_start.b,
                            stroke_start.a,
                        )
                    } else {
                        glam::Vec4::new(
                            edge.start_pin_color.r,
                            edge.start_pin_color.g,
                            edge.start_pin_color.b,
                            edge.start_pin_color.a,
                        )
                    };

                    // Resolve end color: explicit or pin color
                    let end = if stroke_end.a > 0.01 {
                        glam::Vec4::new(stroke_end.r, stroke_end.g, stroke_end.b, stroke_end.a)
                    } else {
                        glam::Vec4::new(
                            edge.end_pin_color.r,
                            edge.end_pin_color.g,
                            edge.end_pin_color.b,
                            edge.end_pin_color.a,
                        )
                    };

                    (start, end)
                };

                // Extract pattern info
                let (
                    pattern_type,
                    dash_length,
                    gap_length,
                    dash_cap,
                    dash_cap_angle,
                    pattern_angle,
                ) = stroke
                    .map(|s| {
                        let pattern_type = s.pattern.type_id();
                        let (param1, param2) = s.pattern.params();
                        let cap_type = s.dash_cap.type_id();
                        let cap_angle = s.dash_cap.angle();
                        let pattern_angle = s.pattern.angle();
                        (
                            pattern_type,
                            param1,
                            param2,
                            cap_type,
                            cap_angle,
                            pattern_angle,
                        )
                    })
                    .unwrap_or((0, 0.0, 0.0, 0, 0.0, 0.0));

                // Compute arc length (simplified - just distance for now)
                let start_vec = glam::Vec2::new(edge.start_pos.x, edge.start_pos.y);
                let end_vec = glam::Vec2::new(edge.end_pos.x, edge.end_pos.y);
                let curve_length = (end_vec - start_vec).length();

                // Build flags
                let mut flags = style.flags();
                if edge.is_pending_cut {
                    flags |= 8; // bit 3 for pending cut highlight
                }

                // Extract border layer info
                let (border_width, border_gap, border_color) = style
                    .border
                    .as_ref()
                    .map(|b| {
                        (
                            b.width,
                            b.gap,
                            glam::Vec4::new(b.color.r, b.color.g, b.color.b, b.color.a),
                        )
                    })
                    .unwrap_or((0.0, 0.0, glam::Vec4::ZERO));

                // Extract shadow layer info
                let (shadow_blur, shadow_color, shadow_offset) = style
                    .shadow
                    .as_ref()
                    .map(|s| {
                        (
                            s.blur,
                            glam::Vec4::new(s.color.r, s.color.g, s.color.b, s.color.a),
                            glam::Vec2::new(s.offset.0, s.offset.1),
                        )
                    })
                    .unwrap_or((0.0, glam::Vec4::ZERO, glam::Vec2::ZERO));

                // Determine if edge is "reversed" (from Input to Output instead of Output to Input)
                let is_reversed = matches!(
                    (edge.start_direction, edge.end_direction),
                    (PinDirection::Input, PinDirection::Output)
                );

                // Animation direction: positive speed moves pattern Output→Input
                // For reversed edges, flip the speed to maintain consistent visual flow
                let base_speed = style.motion_speed();
                let flow_speed = if is_reversed { base_speed } else { -base_speed };

                // Arrow direction: flip pattern_angle for reversed edges
                // This keeps arrows pointing in consistent direction regardless of how edge was drawn
                let pattern_angle = if is_reversed {
                    -pattern_angle
                } else {
                    pattern_angle
                };

                types::Edge {
                    start: start_vec,
                    end: end_vec,
                    start_direction: edge.start_side,
                    end_direction: edge.end_side,
                    edge_type: style.curve.type_id(),
                    pattern_type,
                    start_color,
                    end_color,
                    thickness,
                    curve_length,
                    dash_length,
                    gap_length,
                    flow_speed,
                    dash_cap,
                    dash_cap_angle,
                    pattern_angle,
                    flags,
                    border_width,
                    border_gap,
                    shadow_blur,
                    border_color,
                    shadow_color,
                    shadow_offset,
                    _pad0: 0.0,
                    _pad1: 0.0,
                }
            }),
        );

        // Update uniforms
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom: self.camera_zoom,
            camera_position: glam::Vec2::new(self.camera_position.x, self.camera_position.y),
            border_color: glam::Vec4::ZERO,
            fill_color: glam::Vec4::ZERO,
            edge_color: glam::Vec4::ZERO,
            background_color: glam::Vec4::ZERO,
            drag_edge_color: glam::Vec4::ZERO,
            drag_edge_valid_color: glam::Vec4::ZERO,
            cursor_position: glam::Vec2::ZERO,
            num_nodes: 0,
            num_pins: 0,
            num_edges,
            time: self.time,
            dragging: 0,
            dragging_edge_from_node: 0,
            dragging_edge_from_pin: 0,
            dragging_edge_from_origin: glam::Vec2::ZERO,
            dragging_edge_to_node: 0,
            dragging_edge_to_pin: 0,
            grid_color: glam::Vec4::ZERO,
            hover_glow_color: glam::Vec4::ZERO,
            selection_box_color: glam::Vec4::ZERO,
            edge_cutting_color: glam::Vec4::ZERO,
            hover_glow_radius: 0.0,
            edge_thickness: 2.0,
            render_mode: 0,
            viewport_size: glam::Vec2::new(
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ),
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            bg_pattern_type: 0,
            bg_flags: 0,
            bg_minor_spacing: 0.0,
            bg_major_ratio: 0.0,
            bg_line_widths: glam::Vec2::ZERO,
            bg_opacities: glam::Vec2::ZERO,
            bg_primary_color: glam::Vec4::ZERO,
            bg_secondary_color: glam::Vec4::ZERO,
            bg_pattern_params: glam::Vec4::ZERO,
            bg_adaptive_params: glam::Vec4::ZERO,
        };

        let mut uniform_buffer = encase::UniformBuffer::new(Vec::new());
        uniform_buffer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniforms, 0, uniform_buffer.as_ref());

        // Recreate bind group if edge buffer was resized
        let current_generation = pipeline.edges.generation();
        if current_generation != pipeline.bind_group_generation {
            pipeline.bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("edges_bind_group"),
                layout: &pipeline.shared.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniforms.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: pipeline.dummy_nodes.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.dummy_pins.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: pipeline.edges.as_entire_binding(),
                    },
                ],
            });
            pipeline.bind_group_generation = current_generation;
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        if self.edges.is_empty() {
            return true;
        }

        render_pass.set_pipeline(&pipeline.shared.edge_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..6, 0..self.edges.len() as u32);
        true
    }
}
