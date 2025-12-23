use std::num::NonZeroU64;

use iced::{
    Rectangle,
    wgpu::{
        BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
        BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
        BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device,
        FragmentState, FrontFace, MultisampleState, PipelineCompilationOptions,
        PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
        Queue, RenderPipeline, RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor,
        ShaderSource, ShaderStages, TextureFormat, VertexState,
    },
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Pipeline as PipelineTrait;

use crate::node_grapgh::{effects::Node, euclid::WorldPoint, state::Dragging};

use super::{EdgeData, primitive::NodeGraphPrimitive};

mod buffer;
mod types;

pub struct Pipeline {
    uniforms: Buffer,
    nodes: buffer::Buffer<types::Node>,
    pins: buffer::Buffer<types::Pin>,
    edges: buffer::Buffer<types::Edge>,

    pipeline_background: RenderPipeline,
    pipeline_edges: RenderPipeline,
    pipeline_nodes: RenderPipeline,
    pipeline_pins: RenderPipeline,
    pipeline_dragging: RenderPipeline,
    #[allow(dead_code)]
    pipeline_foreground: RenderPipeline,

    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,

    /// Cached buffer generations to avoid recreating bind groups unnecessarily.
    /// This is critical for WebGPU/WASM where bind group creation can exhaust memory.
    bind_group_generations: (u64, u64, u64),
}

impl PipelineTrait for Pipeline {
    fn new(
        device: &iced::wgpu::Device,
        _queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_shader(device, format, None)
    }
}

impl Pipeline {
    pub fn new_with_shader(
        device: &Device,
        format: TextureFormat,
        custom_shader_wgsl: Option<&str>,
    ) -> Self {
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("uniform buffer"),
            size: std::mem::size_of::<types::Uniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let nodes = buffer::Buffer::new(
            device,
            Some("nodes buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let pins = buffer::Buffer::new(
            device,
            Some("pins buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let edges = buffer::Buffer::new(
            device,
            Some("edges buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let bind_group_layout = create_bind_group_layout(device);
        let bind_group = create_bind_group(
            device,
            &bind_group_layout,
            uniforms.as_entire_binding(),
            nodes.as_entire_binding(),
            pins.as_entire_binding(),
            edges.as_entire_binding(),
        );

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Node Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        // Use custom shader if provided, otherwise use default
        let shader_source = custom_shader_wgsl.unwrap_or(include_str!("shader.wgsl"));
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("node shaders"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        // Create all 5 pipelines
        let pipeline_background = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_background",
            "fs_background",
            "background",
        );
        let pipeline_edges = create_pipeline_custom(
            device, format, &layout, &module, "vs_edge", "fs_edge", "edges",
        );
        let pipeline_nodes = create_pipeline_custom(
            device, format, &layout, &module, "vs_node", "fs_node", "nodes",
        );
        let pipeline_pins =
            create_pipeline_custom(device, format, &layout, &module, "vs_pin", "fs_pin", "pins");
        let pipeline_dragging = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_dragging",
            "fs_dragging",
            "dragging",
        );
        let pipeline_foreground = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_main",
            "fs_foreground",
            "foreground_legacy",
        );

        Self {
            uniforms,
            nodes,
            pins,
            edges,
            pipeline_background,
            pipeline_edges,
            pipeline_nodes,
            pipeline_pins,
            pipeline_dragging,
            pipeline_foreground,
            bind_group_layout,
            bind_group,
            bind_group_generations: (0, 0, 0),
        }
    }

    #[allow(dead_code)]
    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle<f32>,
        viewport: &Viewport,
        primitive: &NodeGraphPrimitive,
    ) {
        self.update_new(
            device,
            queue,
            bounds,
            viewport,
            primitive.camera_zoom,
            primitive.camera_position,
            primitive.cursor_position,
            primitive.time,
            &primitive.dragging,
            &primitive.nodes,
            &primitive.edges,
            primitive.edge_color,
            primitive.background_color,
            primitive.border_color,
            primitive.fill_color,
            primitive.drag_edge_color,
            primitive.drag_edge_valid_color,
            &primitive.selected_nodes,
            primitive.selected_edge_color,
            primitive.edge_thickness,
        );
    }

    pub fn update_new(
        &mut self,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle<f32>,
        viewport: &Viewport,
        camera_zoom: f32,
        camera_position: WorldPoint,
        cursor_position: WorldPoint,
        time: f32,
        dragging: &Dragging,
        nodes: &[Node],
        edges: &[EdgeData],
        edge_color: glam::Vec4,
        background_color: glam::Vec4,
        border_color: glam::Vec4,
        fill_color: glam::Vec4,
        drag_edge_color: glam::Vec4,
        drag_edge_valid_color: glam::Vec4,
        selected_nodes: &std::collections::HashSet<usize>,
        selected_edge_color: glam::Vec4,
        edge_thickness: f32,
    ) {
        let mut pin_start = 0;
        let num_nodes = self.nodes.update(
            device,
            queue,
            nodes.iter().map(|node| {
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
                    border_width: node.border_width,
                    opacity: node.opacity,
                    pin_start,
                    pin_count,
                    shadow_blur: node.shadow_blur,
                    shadow_offset: glam::Vec2::new(node.shadow_offset.0, node.shadow_offset.1),
                    fill_color: glam::Vec4::new(
                        node.fill_color.r,
                        node.fill_color.g,
                        node.fill_color.b,
                        node.fill_color.a,
                    ),
                    border_color: glam::Vec4::new(
                        node.border_color.r,
                        node.border_color.g,
                        node.border_color.b,
                        node.border_color.a,
                    ),
                    shadow_color: glam::Vec4::new(
                        node.shadow_color.r,
                        node.shadow_color.g,
                        node.shadow_color.b,
                        node.shadow_color.a,
                    ),
                    flags: node.flags,
                    _pad_flags0: 0,
                    _pad_flags1: 0,
                    _pad_flags2: 0,
                }
            }),
        );

        let num_pins = self.pins.update(
            device,
            queue,
            nodes.iter().flat_map(|node| node.pins.iter()).map(|pin| {
                use crate::node_pin::PinDirection;
                use crate::style::PinShape;
                types::Pin {
                    position: pin.offset,
                    color: glam::Vec4::new(pin.color.r, pin.color.g, pin.color.b, pin.color.a),
                    border_color: glam::Vec4::new(
                        pin.border_color.r,
                        pin.border_color.g,
                        pin.border_color.b,
                        pin.border_color.a,
                    ),
                    side: pin.side,
                    radius: pin.radius,
                    direction: match pin.direction {
                        PinDirection::Input => 0,
                        PinDirection::Output => 1,
                        PinDirection::Both => 2,
                    },
                    shape: match pin.shape {
                        PinShape::Circle => 0,
                        PinShape::Square => 1,
                        PinShape::Diamond => 2,
                        PinShape::Triangle => 3,
                    },
                    border_width: pin.border_width,
                    flags: 0,
                }
            }),
        );

        // Extract pending cuts for edge cutting highlight
        let pending_cuts = if let Dragging::EdgeCutting { pending_cuts, .. } = dragging {
            Some(pending_cuts)
        } else {
            None
        };

        let num_edges = self.edges.update(
            device,
            queue,
            edges.iter().enumerate().map(|(edge_idx, edge_data)| {
                // Highlight edges where both ends are selected
                let is_highlighted = selected_nodes.contains(&edge_data.from_node)
                    && selected_nodes.contains(&edge_data.to_node);

                // Check if edge is pending cut
                let is_pending_cut = pending_cuts
                    .as_ref()
                    .map(|cuts| cuts.contains(&edge_idx))
                    .unwrap_or(false);

                let style = &edge_data.style;

                // Resolve edge gradient colors:
                // - TRANSPARENT (alpha < 0.01) = use pin color at that end
                // - Explicit color = use it
                // - Selected edges override everything with selection color
                let (start_color, end_color) = if is_highlighted {
                    // Selected edges use selection color (solid)
                    (selected_edge_color, selected_edge_color)
                } else {
                    // Get pin colors for fallback
                    let from_pin = &nodes[edge_data.from_node].pins[edge_data.from_pin];
                    let to_pin = &nodes[edge_data.to_node].pins[edge_data.to_pin];

                    // Resolve start color: explicit or pin color
                    let start = if style.start_color.a > 0.01 {
                        glam::Vec4::new(
                            style.start_color.r,
                            style.start_color.g,
                            style.start_color.b,
                            style.start_color.a,
                        )
                    } else {
                        glam::Vec4::new(
                            from_pin.color.r,
                            from_pin.color.g,
                            from_pin.color.b,
                            from_pin.color.a,
                        )
                    };

                    // Resolve end color: explicit or pin color
                    let end = if style.end_color.a > 0.01 {
                        glam::Vec4::new(
                            style.end_color.r,
                            style.end_color.g,
                            style.end_color.b,
                            style.end_color.a,
                        )
                    } else {
                        glam::Vec4::new(
                            to_pin.color.r,
                            to_pin.color.g,
                            to_pin.color.b,
                            to_pin.color.a,
                        )
                    };

                    (start, end)
                };

                // Extract dash pattern values
                let (dash_length, gap_length) = style
                    .dash_pattern
                    .map(|d| (d.dash_length, d.gap_length))
                    .unwrap_or((0.0, 0.0));

                // Set bit 3 (value 8) for pending cut highlight
                let flags = style.animation_flags() | if is_pending_cut { 8 } else { 0 };

                types::Edge {
                    from_node: edge_data.from_node as _,
                    from_pin: edge_data.from_pin as _,
                    to_node: edge_data.to_node as _,
                    to_pin: edge_data.to_pin as _,
                    start_color,
                    end_color,
                    thickness: style.thickness,
                    edge_type: style.edge_type as u32,
                    dash_length,
                    gap_length,
                    flow_speed: style.flow_speed(),
                    flags,
                    _pad0: 0.0,
                    _pad1: 0.0,
                }
            }),
        );

        let dragging_type: u32 = match dragging {
            Dragging::None => 0,
            Dragging::Graph(_) => 1,
            Dragging::Node(_, _) => 2,
            Dragging::Edge(_, _, _) => 3,
            Dragging::EdgeOver(_, _, _, _) => 4,
            Dragging::BoxSelect(_, _) => 5,
            Dragging::GroupMove(_) => 6,
            Dragging::EdgeCutting { .. } => 7,
        };

        let (
            dragging_edge_from_node,
            dragging_edge_from_pin,
            dragging_edge_from_origin,
            dragging_edge_to_node,
            dragging_edge_to_pin,
        ) = {
            match dragging {
                Dragging::Edge(from_node, from_pin, position) => {
                    (*from_node as _, *from_pin as _, *position, 0, 0)
                }
                Dragging::EdgeOver(from_node, from_pin, to_node, to_pin) => (
                    *from_node as _,
                    *from_pin as _,
                    WorldPoint::zero(),
                    *to_node as _,
                    *to_pin as _,
                ),
                // BoxSelect: start point in from_origin, end point is cursor_position
                Dragging::BoxSelect(start, _end) => (0, 0, *start, 0, 0),
                // EdgeCutting: first trail point in from_origin
                Dragging::EdgeCutting { trail, .. } => {
                    let origin = trail.first().copied().unwrap_or(WorldPoint::zero());
                    (0, 0, origin, 0, 0)
                }
                _ => (0, 0, WorldPoint::zero(), 0, 0),
            }
        };

        // Compute dragging edge color (solid color from connected pin)
        let (dragging_edge_start_color, dragging_edge_end_color) = match dragging {
            Dragging::Edge(from_node, from_pin, _) => {
                // Dragging from pin: use solid pin color
                let pin_color = &nodes[*from_node].pins[*from_pin].color;
                let color = glam::Vec4::new(pin_color.r, pin_color.g, pin_color.b, pin_color.a);
                (color, color)
            }
            Dragging::EdgeOver(from_node, from_pin, _to_node, _to_pin) => {
                // Over valid target: still use source pin color (solid)
                let pin_color = &nodes[*from_node].pins[*from_pin].color;
                let color = glam::Vec4::new(pin_color.r, pin_color.g, pin_color.b, 1.0);
                (color, color)
            }
            _ => {
                // Not dragging edge: use defaults (won't be rendered anyway)
                (edge_color, edge_color)
            }
        };

        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom,
            camera_position,
            border_color,
            fill_color,
            edge_color,
            background_color,
            drag_edge_color,
            drag_edge_valid_color,
            cursor_position,
            num_nodes,
            num_pins,
            num_edges,
            time,
            dragging: dragging_type,
            _pad_uniforms0: 0,
            _pad_uniforms1: 0,
            _pad_uniforms2: 0,
            dragging_edge_from_node,
            dragging_edge_from_pin,
            dragging_edge_from_origin,
            dragging_edge_to_node,
            dragging_edge_to_pin,
            dragging_edge_start_color,
            dragging_edge_end_color,
            // Theme-derived visual parameters (computed in Rust, no hardcodes in shader)
            grid_color: glam::Vec4::new(
                border_color.x * 1.3,
                border_color.y * 1.3,
                border_color.z * 1.3,
                1.0,
            ),
            hover_glow_color: glam::Vec4::new(0.5, 0.7, 1.0, 1.0), // Soft blue glow
            selection_box_color: glam::Vec4::new(0.3, 0.6, 1.0, 1.0), // Selection blue
            edge_cutting_color: glam::Vec4::new(1.0, 0.3, 0.3, 1.0), // Warning red
            hover_glow_radius: 6.0,
            edge_thickness,
            _pad_theme1: 0.0,
            _pad_theme2: 0.0,
            viewport_size: glam::Vec2::new(
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ),
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            _pad_end0: 0,
            _pad_end1: 0,
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Only recreate bind group if buffer generations changed.
        // This is critical for WebGPU/WASM where bind group creation can exhaust GPU memory.
        let current_generations = (
            self.nodes.generation(),
            self.pins.generation(),
            self.edges.generation(),
        );
        if current_generations != self.bind_group_generations {
            self.bind_group = create_bind_group(
                device,
                &self.bind_group_layout,
                self.uniforms.as_entire_binding(),
                self.nodes.as_entire_binding(),
                self.pins.as_entire_binding(),
                self.edges.as_entire_binding(),
            );
            self.bind_group_generations = current_generations;
        }
    }

    pub fn render_pass(
        &self,
        pass: &mut iced::wgpu::RenderPass<'_>,
        _viewport: Rectangle<u32>,
    ) {
        let num_nodes = self.nodes.len();
        let num_pins = self.pins.len();
        let num_edges = self.edges.len();

        pass.set_bind_group(0, &self.bind_group, &[]);

        // Pass 1: Background grid (fullscreen)
        pass.set_pipeline(&self.pipeline_background);
        pass.draw(0..3, 0..1);

        // Pass 2: Edges (instanced - behind nodes)
        if num_edges > 0 {
            pass.set_pipeline(&self.pipeline_edges);
            pass.draw(0..6, 0..num_edges as u32);
        }

        // Pass 3: Nodes (instanced)
        if num_nodes > 0 {
            pass.set_pipeline(&self.pipeline_nodes);
            pass.draw(0..6, 0..num_nodes as u32);
        }

        // Pass 4: Pin indicators (instanced)
        if num_pins > 0 {
            pass.set_pipeline(&self.pipeline_pins);
            pass.draw(0..6, 0..num_pins as u32);
        }

        // Pass 5: Dragging edge / box select (if active)
        pass.set_pipeline(&self.pipeline_dragging);
        pass.draw(0..6, 0..1);
    }
}

fn create_pipeline_custom(
    device: &Device,
    format: TextureFormat,
    layout: &PipelineLayout,
    module: &ShaderModule,
    vs_entry: &str,
    fs_entry: &str,
    label: &str,
) -> RenderPipeline {
    let fragment_targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: VertexState {
            module,
            entry_point: Some(vs_entry),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(FragmentState {
            module,
            entry_point: Some(fs_entry),
            targets: &fragment_targets,
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Node Pipeline Bind Group Layout"),
        entries: &[
            // Binding 0: Uniforms (uniform buffer)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::Uniforms>() as u64).unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 1: Nodes (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::Node>() as u64 * 10).unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 2: Pins (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::Pin>() as u64 * 10).unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 3: Edges (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::Edge>() as u64 * 10).unwrap(),
                    ),
                },
                count: None,
            },
        ],
    })
}

fn create_bind_group(
    device: &Device,
    bind_group_layout: &BindGroupLayout,
    uniforms: BindingResource,
    nodes: BindingResource,
    pins: BindingResource,
    edges: BindingResource,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("Node Pipeline Bind Group"),
        layout: bind_group_layout,
        entries: &[
            // Entry 0: Uniforms
            BindGroupEntry {
                binding: 0,
                resource: uniforms,
            },
            // Entry 1: Nodes
            BindGroupEntry {
                binding: 1,
                resource: nodes,
            },
            // Entry 2: Pins
            BindGroupEntry {
                binding: 2,
                resource: pins,
            },
            // Entry 3: Edges
            BindGroupEntry {
                binding: 3,
                resource: edges,
            },
        ],
    })
}
