//! Single edge primitive for NodeGraph.
//!
//! Each edge is rendered individually with slot-based batching (like NodePrimitive).
//! Style comes fully resolved from the widget - no business logic here.

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
use crate::node_pin::PinDirection;
use crate::style::EdgeStyle;

use super::super::pipeline::{buffer, types};
use super::super::shared::SharedNodeGraphResources;
use super::RenderContext;

/// Primitive for rendering a single edge.
#[derive(Debug, Clone)]
pub struct EdgePrimitive {
    /// Shared rendering context
    pub context: RenderContext,
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
    /// Edge style (fully resolved, no status)
    pub style: EdgeStyle,
    /// Start pin color (for gradient resolution)
    pub start_pin_color: Color,
    /// End pin color (for gradient resolution)
    pub end_pin_color: Color,
}

/// Pipeline for EdgePrimitive rendering.
pub struct EdgePipeline {
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
    /// Dummy grids buffer (required by bind group layout)
    dummy_grids: Buffer,
    /// Bind group for rendering
    bind_group: BindGroup,
    /// Bind group generation for recreation tracking
    bind_group_generation: u64,
    /// Current slot for prepare() - incremented each prepare(), reset in trim()
    prepare_slot: usize,
    /// Current slot for draw() - AtomicUsize because draw() only has &self
    draw_slot: std::sync::atomic::AtomicUsize,
}

impl Pipeline for EdgePipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("edge_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy buffers
        let dummy_nodes = device.create_buffer(&BufferDescriptor {
            label: Some("edge_dummy_nodes"),
            size: <types::Node as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_pins = device.create_buffer(&BufferDescriptor {
            label: Some("edge_dummy_pins"),
            size: <types::Pin as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create edge buffer (dynamic)
        let edges = buffer::Buffer::new(
            device,
            Some("edge_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create minimal dummy grids buffer
        let dummy_grids = device.create_buffer(&BufferDescriptor {
            label: Some("edge_dummy_grids"),
            size: <types::Grid as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("edge_bind_group"),
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
            edges,
            dummy_grids,
            bind_group,
            bind_group_generation: 0,
            prepare_slot: 0,
            draw_slot: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn trim(&mut self) {
        // Reset counters for next frame
        self.prepare_slot = 0;
        self.draw_slot.store(0, std::sync::atomic::Ordering::SeqCst);
        // Clear buffer
        self.edges.clear();
    }
}

impl Primitive for EdgePrimitive {
    type Pipeline = EdgePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let style = &self.style;

        // Get stroke layer
        let stroke = style.stroke.as_ref();
        let (stroke_start, stroke_end, thickness) = stroke
            .map(|s| (s.start_color, s.end_color, s.width))
            .unwrap_or((Color::TRANSPARENT, Color::TRANSPARENT, 2.0));

        // Resolve edge gradient colors
        // Start color: explicit or pin color
        let start_color = if stroke_start.a > 0.01 {
            glam::Vec4::new(stroke_start.r, stroke_start.g, stroke_start.b, stroke_start.a)
        } else {
            glam::Vec4::new(
                self.start_pin_color.r,
                self.start_pin_color.g,
                self.start_pin_color.b,
                self.start_pin_color.a,
            )
        };

        // End color: explicit or pin color
        let end_color = if stroke_end.a > 0.01 {
            glam::Vec4::new(stroke_end.r, stroke_end.g, stroke_end.b, stroke_end.a)
        } else {
            glam::Vec4::new(
                self.end_pin_color.r,
                self.end_pin_color.g,
                self.end_pin_color.b,
                self.end_pin_color.a,
            )
        };

        // Extract pattern info
        let (pattern_type, dash_length, gap_length, dash_cap, dash_cap_angle, pattern_angle) =
            stroke
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
        let start_vec = glam::Vec2::new(self.start_pos.x, self.start_pos.y);
        let end_vec = glam::Vec2::new(self.end_pos.x, self.end_pos.y);
        let curve_length = (end_vec - start_vec).length();

        // Animation type comes from style (already resolved by widget)
        let animation_type = if style.has_motion() { 1 } else { 0 };

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
            (self.start_direction, self.end_direction),
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

        // Push edge to buffer
        let _ = pipeline.edges.push(
            device,
            queue,
            types::Edge {
                start: start_vec,
                end: end_vec,
                start_direction: self.start_side,
                end_direction: self.end_side,
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
                animation_type,
                border_width,
                border_gap,
                shadow_blur,
                border_color,
                shadow_color,
                shadow_offset,
                _pad0: 0.0,
                _pad1: 0.0,
            },
        );

        // Increment prepare slot
        pipeline.prepare_slot += 1;

        // Update uniforms
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom: self.context.camera_zoom,
            camera_position: glam::Vec2::new(
                self.context.camera_position.x,
                self.context.camera_position.y,
            ),
            cursor_position: glam::Vec2::ZERO,
            num_nodes: 0,
            time: self.context.time,
            overlay_type: 0,
            overlay_start: glam::Vec2::ZERO,
            overlay_color: glam::Vec4::ZERO,
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
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
                label: Some("edge_bind_group"),
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
                    BindGroupEntry {
                        binding: 4,
                        resource: pipeline.dummy_grids.as_entire_binding(),
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
        // Get current slot and increment for next draw call
        let slot = pipeline
            .draw_slot
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        render_pass.set_pipeline(&pipeline.shared.edge_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..6, slot as u32..(slot + 1) as u32);
        true
    }
}
