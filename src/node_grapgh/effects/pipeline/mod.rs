use std::num::NonZeroU64;

use glam::vec4;
use iced::{
    Rectangle,
    wgpu::{
        BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
        BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
        BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, Device,
        FragmentState, FrontFace, LoadOp, MultisampleState, Operations, PipelineCompilationOptions,
        PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
        Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
        RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource, ShaderStages,
        StoreOp, TextureFormat, TextureView, VertexState,
    },
};

use super::{Layer, primitive::Primitive};

mod buffer;
mod types;

pub struct Pipeline {
    uniforms: Buffer,
    nodes: buffer::Buffer<types::Node>,
    pins: buffer::Buffer<types::Pin>,
    edges: buffer::Buffer<types::Edge>,

    pipeline_foreground: RenderPipeline,
    pipeline_background: RenderPipeline,

    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,
}

impl Pipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
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

        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("node fragment shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let pipeline_background =
            create_render_pipeline(device, format, Layer::Background, &layout, &module);
        let pipeline_foreground =
            create_render_pipeline(device, format, Layer::Foreground, &layout, &module);

        Self {
            uniforms,
            nodes,
            pins,
            edges,
            pipeline_foreground,
            pipeline_background,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn update(&mut self, device: &Device, queue: &Queue, primitive: &Primitive) {
        let mut pin_start = 0;
        let num_nodes = self.nodes.update(
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
                    _padding: 0,
                }
            }),
        );

        let num_pins = self.pins.update(
            device,
            queue,
            primitive
                .nodes
                .iter()
                .flat_map(|node| node.pins.iter())
                .map(|pin| types::Pin {
                    side: pin.side,
                    position: pin.offset,
                    radius: pin.radius,
                }),
        );

        let num_edges = self.edges.update(
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

        let uniforms = types::Uniforms {
            border_color: vec4(0.5, 0.6, 0.7, 1.0),
            fill_color: vec4(0.5, 0.3, 0.1, 1.0),
            num_nodes,
            num_pins,
            num_edges,
            _padding: 0,
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            self.uniforms.as_entire_binding(),
            self.nodes.as_entire_binding(),
            self.pins.as_entire_binding(),
            self.edges.as_entire_binding(),
        );

        println!(
            "nodes: {:?} ({:?}), pins: {:?} ({:?}), edges: {:?} ({:?})",
            self.nodes.len(),
            self.nodes.capacity(),
            self.pins.len(),
            self.pins.capacity(),
            self.edges.len(),
            self.edges.capacity(),
        );
    }

    pub fn update_echo(&mut self, queue: &Queue) {
        // Update the echo pipeline here
        println!(
            "echo - nodes: {:?} ({:?}), pins: {:?} ({:?}), edges: {:?} ({:?})",
            self.nodes.len(),
            self.nodes.capacity(),
            self.pins.len(),
            self.pins.capacity(),
            self.edges.len(),
            self.edges.capacity(),
        );
    }

    pub fn render(
        &self,
        target: &TextureView,
        encoder: &mut CommandEncoder,
        viewport: Rectangle<u32>,
        layer: Layer,
    ) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("pipeline.pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        pass.set_pipeline(match layer {
            Layer::Background => &self.pipeline_background,
            Layer::Foreground => &self.pipeline_foreground,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        // pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..6, 0..1);
    }
}

fn create_render_pipeline(
    device: &Device,
    format: TextureFormat,
    layer: Layer,
    layout: &PipelineLayout,
    module: &ShaderModule,
) -> RenderPipeline {
    let fragment_targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(match layer {
            Layer::Background => "node_graph background pipeline",
            Layer::Foreground => "node_graph foreground pipeline",
        }),
        layout: Some(layout),
        vertex: VertexState {
            module: module,
            entry_point: Some("vs_main"), // Vertex shader entry point
            buffers: &[],                 // No vertex buffer needed
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
            module: &module,
            entry_point: Some(match layer {
                Layer::Background => "fs_background",
                Layer::Foreground => "fs_foreground",
            }), // Fragment shader entry point
            targets: &fragment_targets,
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None, // Optional cache field
    })
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Node Pipeline Bind Group Layout"),
        entries: &[
            // Binding 0: Uniforms (uniform buffer)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
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
                visibility: ShaderStages::FRAGMENT,
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
                visibility: ShaderStages::FRAGMENT,
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
                visibility: ShaderStages::FRAGMENT,
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
