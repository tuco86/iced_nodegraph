//! Single node primitive for NodeGraph.
//!
//! Each node is rendered in two passes:
//! - Background (NodeLayer::Background): Fill + Shadow, rendered before widgets
//! - Foreground (NodeLayer::Foreground): Border + Pins, rendered after widgets
//!
//! This enables correct compositing when nodes overlap.

use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages, Device,
    Queue, TextureFormat,
};
use iced::{Color, Rectangle, Size};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use crate::node_graph::euclid::WorldPoint;
use crate::node_pin::PinDirection;
use crate::style::PinShape;

use super::super::pipeline::{buffer, types};
use super::super::shared::SharedNodeGraphResources;

/// Layer for two-phase node rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLayer {
    /// Fill + Shadow (rendered before widgets)
    Background,
    /// Border + Pins (rendered after widgets)
    Foreground,
}

/// Pin data for rendering.
#[derive(Debug, Clone)]
pub struct PinRenderData {
    /// Offset from node top-left (in world coordinates)
    pub offset: WorldPoint,
    /// Pin side (0=Left, 1=Right, 2=Top, 3=Bottom)
    pub side: u32,
    /// Pin radius (pre-computed with animation scale by widget)
    pub radius: f32,
    /// Pin color
    pub color: Color,
    /// Pin direction (Input/Output/Both)
    pub direction: PinDirection,
    /// Pin shape
    pub shape: PinShape,
    /// Border color
    pub border_color: Color,
    /// Border width
    pub border_width: f32,
}

/// Primitive for rendering a single node.
#[derive(Debug, Clone)]
pub struct NodePrimitive {
    /// Which layer to render (Background or Foreground)
    pub layer: NodeLayer,
    /// Node position in world coordinates
    pub position: WorldPoint,
    /// Node size
    pub size: Size,
    /// Corner radius
    pub corner_radius: f32,
    /// Border width
    pub border_width: f32,
    /// Opacity
    pub opacity: f32,
    /// Fill color
    pub fill_color: Color,
    /// Border color
    pub border_color: Color,
    /// Shadow offset
    pub shadow_offset: (f32, f32),
    /// Shadow blur
    pub shadow_blur: f32,
    /// Shadow color
    pub shadow_color: Color,
    /// Glow color (set when node has active glow effect)
    pub glow_color: Color,
    /// Glow radius in world units (0.0 = no glow)
    pub glow_radius: f32,
    /// Node's pins
    pub pins: Vec<PinRenderData>,
    /// Camera zoom level
    pub camera_zoom: f32,
    /// Camera position
    pub camera_position: WorldPoint,
    /// Time for animations
    pub time: f32,
}

/// Pipeline for NodePrimitive rendering.
pub struct NodePipeline {
    /// Shared resources (shader, pipelines, layouts)
    shared: Arc<SharedNodeGraphResources>,
    /// Uniform buffer
    uniforms: Buffer,
    /// Node storage buffer
    nodes: buffer::Buffer<types::Node>,
    /// Pin storage buffer
    pins: buffer::Buffer<types::Pin>,
    /// Dummy edge buffer (required by bind group layout)
    dummy_edges: Buffer,
    /// Dummy grids buffer (required by bind group layout)
    dummy_grids: Buffer,
    /// Bind group for rendering
    bind_group: BindGroup,
    /// Bind group generation for recreation tracking
    bind_group_generations: (u64, u64),
    /// Current slot for prepare() - incremented each prepare(), reset in trim()
    prepare_slot: usize,
    /// Current slot for draw() - AtomicUsize because draw() only has &self
    draw_slot: std::sync::atomic::AtomicUsize,
    /// Pin start index for each node slot (pin_starts[slot] = first pin index)
    pin_starts: Vec<usize>,
    /// Pin count for each node slot
    pin_counts: Vec<usize>,
}

impl Pipeline for NodePipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedNodeGraphResources::get_or_init(device, format);

        // Create uniform buffer
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("node_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create node buffer (starts with capacity for 1 node)
        let nodes = buffer::Buffer::new(
            device,
            Some("node_nodes_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create pin buffer
        let pins = buffer::Buffer::new(
            device,
            Some("node_pins_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Create minimal dummy edge buffer
        let dummy_edges = device.create_buffer(&BufferDescriptor {
            label: Some("node_dummy_edges"),
            size: <types::Edge as ShaderSize>::SHADER_SIZE.get() * 10,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create minimal dummy grids buffer
        let dummy_grids = device.create_buffer(&BufferDescriptor {
            label: Some("node_dummy_grids"),
            size: <types::Grid as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("node_bind_group"),
            layout: &shared.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: nodes.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: pins.as_entire_binding(),
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
            nodes,
            pins,
            dummy_edges,
            dummy_grids,
            bind_group,
            bind_group_generations: (0, 0),
            prepare_slot: 0,
            draw_slot: std::sync::atomic::AtomicUsize::new(0),
            pin_starts: Vec::new(),
            pin_counts: Vec::new(),
        }
    }

    fn trim(&mut self) {
        // Reset counters for next frame
        self.prepare_slot = 0;
        self.draw_slot.store(0, std::sync::atomic::Ordering::SeqCst);
        // Clear buffers
        self.nodes.clear();
        self.pins.clear();
        self.pin_starts.clear();
        self.pin_counts.clear();
    }
}

impl Primitive for NodePrimitive {
    type Pipeline = NodePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Track pin_start and pin_count for this node
        let pin_start = pipeline.pins.len();
        pipeline.pin_starts.push(pin_start);
        pipeline.pin_counts.push(self.pins.len());

        // Push node to buffer
        let _node_slot = pipeline.nodes.push(
            device,
            queue,
            types::Node {
                position: glam::Vec2::new(self.position.x, self.position.y),
                size: glam::Vec2::new(self.size.width, self.size.height),
                corner_radius: self.corner_radius,
                border_width: self.border_width,
                opacity: self.opacity,
                pin_start: pin_start as u32,
                pin_count: self.pins.len() as u32,
                shadow_blur: self.shadow_blur,
                shadow_offset: glam::Vec2::new(self.shadow_offset.0, self.shadow_offset.1),
                fill_color: glam::Vec4::new(
                    self.fill_color.r,
                    self.fill_color.g,
                    self.fill_color.b,
                    self.fill_color.a,
                ),
                border_color: glam::Vec4::new(
                    self.border_color.r,
                    self.border_color.g,
                    self.border_color.b,
                    self.border_color.a,
                ),
                shadow_color: glam::Vec4::new(
                    self.shadow_color.r,
                    self.shadow_color.g,
                    self.shadow_color.b,
                    self.shadow_color.a,
                ),
                glow_color: glam::Vec4::new(
                    self.glow_color.r,
                    self.glow_color.g,
                    self.glow_color.b,
                    self.glow_color.a,
                ),
                glow_radius: self.glow_radius,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            },
        );

        // Push pins to buffer
        for pin in &self.pins {
            let pin_direction = match pin.direction {
                PinDirection::Input => 0,
                PinDirection::Output => 1,
                PinDirection::Both => 2,
            };

            let _ = pipeline.pins.push(
                device,
                queue,
                types::Pin {
                    position: glam::Vec2::new(pin.offset.x, pin.offset.y),
                    color: glam::Vec4::new(pin.color.r, pin.color.g, pin.color.b, pin.color.a),
                    border_color: glam::Vec4::new(
                        pin.border_color.r,
                        pin.border_color.g,
                        pin.border_color.b,
                        pin.border_color.a,
                    ),
                    side: pin.side,
                    radius: pin.radius, // Pre-computed with animation scale by widget
                    direction: pin_direction,
                    shape: match pin.shape {
                        PinShape::Circle => 0,
                        PinShape::Square => 1,
                        PinShape::Diamond => 2,
                        PinShape::Triangle => 3,
                    },
                    border_width: pin.border_width,
                    _pad0: 0,
                },
            );
        }

        // Increment prepare slot
        pipeline.prepare_slot += 1;

        // Update uniforms (global values, same for all nodes)
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom: self.camera_zoom,
            camera_position: glam::Vec2::new(self.camera_position.x, self.camera_position.y),
            cursor_position: glam::Vec2::ZERO,
            num_nodes: pipeline.nodes.len() as u32,
            time: self.time,
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

        // Recreate bind group if buffers were resized
        let current_generations = (pipeline.nodes.generation(), pipeline.pins.generation());
        if current_generations != pipeline.bind_group_generations {
            pipeline.bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("node_bind_group"),
                layout: &pipeline.shared.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniforms.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: pipeline.nodes.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.pins.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: pipeline.dummy_edges.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: pipeline.dummy_grids.as_entire_binding(),
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
        // Get current slot and increment for next draw call
        let slot = pipeline
            .draw_slot
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Get pin range for this node slot
        let pin_start = pipeline.pin_starts.get(slot).copied().unwrap_or(0) as u32;
        let pin_count = pipeline.pin_counts.get(slot).copied().unwrap_or(0) as u32;

        match self.layer {
            NodeLayer::Background => {
                // Node fill + shadow
                render_pass.set_pipeline(&pipeline.shared.node_fill_pipeline);
                render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
                render_pass.draw(0..6, slot as u32..(slot + 1) as u32);
            }
            NodeLayer::Foreground => {
                // Node border
                render_pass.set_pipeline(&pipeline.shared.node_border_pipeline);
                render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
                render_pass.draw(0..6, slot as u32..(slot + 1) as u32);

                // Pins
                if pin_count > 0 {
                    render_pass.set_pipeline(&pipeline.shared.pin_pipeline);
                    render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
                    render_pass.draw(0..6, pin_start..pin_start + pin_count);
                }
            }
        }

        true
    }
}
