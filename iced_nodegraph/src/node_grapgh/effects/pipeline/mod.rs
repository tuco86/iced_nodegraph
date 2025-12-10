use std::num::NonZeroU64;

use iced::{
    Rectangle,
    wgpu::{
        BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
        BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
        BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder,
        CommandEncoderDescriptor, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor,
        Device, FragmentState, FrontFace, LoadOp, MultisampleState, Operations,
        PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor, PolygonMode,
        PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor,
        RenderPipeline, RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor,
        ShaderSource, ShaderStages, StoreOp, TextureFormat, TextureView, VertexState,
    },
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Pipeline as PipelineTrait;

use crate::node_grapgh::{effects::Node, euclid::WorldPoint, state::Dragging};

use super::{Layer, primitive::NodeGraphPrimitive};

mod buffer;
pub mod cache;
pub mod types;

pub struct Pipeline {
    uniforms: Buffer,
    nodes: buffer::Buffer<types::Node>,
    pins: buffer::Buffer<types::Pin>,
    edges: buffer::Buffer<types::Edge>,
    /// Physics vertices buffer for polyline edge rendering.
    vertices: buffer::Buffer<types::PhysicsVertex>,

    pipeline_background: RenderPipeline,
    pipeline_edges: RenderPipeline,
    pipeline_nodes: RenderPipeline,
    pipeline_pins: RenderPipeline,
    pipeline_dragging: RenderPipeline,
    #[allow(dead_code)]
    pipeline_foreground: RenderPipeline,

    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,

    /// Generation counters for bind group caching.
    /// Only recreate bind group when buffer generations change.
    /// Format: (nodes_gen, pins_gen, edges_gen, vertices_gen)
    bind_group_generation: (u64, u64, u64, u64),

    // ============================================================================
    // PHYSICS COMPUTE RESOURCES
    // ============================================================================

    /// Compute pipeline for physics simulation.
    compute_pipeline: ComputePipeline,

    /// Physics uniforms buffer (spring stiffness, damping, etc.).
    physics_uniforms: Buffer,

    /// Edge metadata buffer for physics (vertex ranges, anchor positions).
    physics_edges_meta: buffer::Buffer<types::PhysicsEdgeMeta>,

    /// Ping-pong vertex buffers for physics simulation.
    /// Buffer A: current state (read), Buffer B: next state (write), then swap.
    vertices_a: buffer::Buffer<types::PhysicsVertex>,
    vertices_b: buffer::Buffer<types::PhysicsVertex>,

    /// Compute bind group layout for group 0 (uniforms, nodes, edges_meta).
    compute_bind_group_layout_0: BindGroupLayout,

    /// Compute bind group layout for group 1 (vertices_in, vertices_out).
    compute_bind_group_layout_1: BindGroupLayout,

    /// Compute bind group 0: uniforms, nodes, edges_meta.
    compute_bind_group_0: Option<BindGroup>,

    /// Compute bind group for A->B direction (read A, write B).
    compute_bind_group_a_to_b: Option<BindGroup>,

    /// Compute bind group for B->A direction (read B, write A).
    compute_bind_group_b_to_a: Option<BindGroup>,

    /// Which buffer has the current physics state.
    /// false = A is current, true = B is current.
    current_buffer_is_b: bool,

    /// Whether GPU physics buffers have been initialized with vertex data.
    /// Only re-initialize when structure changes or on first use.
    gpu_physics_initialized: bool,

    /// Number of vertices in the GPU physics buffers.
    gpu_physics_vertex_count: usize,

    /// Generation counter for compute bind group 0.
    compute_bind_group_0_generation: (u64, u64),

    /// Generation counter for compute bind group 1.
    compute_bind_group_1_generation: (u64, u64),
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

        let vertices = buffer::Buffer::new(
            device,
            Some("physics vertices buffer"),
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
            vertices.as_entire_binding(),
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

        // ====================================================================
        // PHYSICS COMPUTE PIPELINE SETUP
        // ====================================================================

        // Load physics compute shader
        let physics_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("physics compute shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("physics.wgsl"))),
        });

        // Physics uniforms buffer
        let physics_uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("physics uniforms buffer"),
            size: std::mem::size_of::<types::PhysicsUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Physics edge metadata buffer
        let physics_edges_meta = buffer::Buffer::new(
            device,
            Some("physics edges meta buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        // Ping-pong vertex buffers (need read_write for compute shader output)
        // COPY_SRC needed for copying physics results to render buffer
        let vertices_a = buffer::Buffer::new(
            device,
            Some("physics vertices A buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        );
        let vertices_b = buffer::Buffer::new(
            device,
            Some("physics vertices B buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        );

        // Compute bind group layout 0: uniforms, nodes (reuse existing), edges_meta
        let compute_bind_group_layout_0 = create_compute_bind_group_layout_0(device);

        // Compute bind group layout 1: vertices_in (read), vertices_out (read_write)
        let compute_bind_group_layout_1 = create_compute_bind_group_layout_1(device);

        // Create compute pipeline layout
        let compute_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Physics Compute Pipeline Layout"),
            bind_group_layouts: &[&compute_bind_group_layout_0, &compute_bind_group_layout_1],
            ..Default::default()
        });

        // Create compute pipeline
        let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("physics compute pipeline"),
            layout: Some(&compute_layout),
            module: &physics_module,
            entry_point: Some("physics_step"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            uniforms,
            nodes,
            pins,
            edges,
            vertices,
            pipeline_background,
            pipeline_edges,
            pipeline_nodes,
            pipeline_pins,
            pipeline_dragging,
            pipeline_foreground,
            bind_group_layout,
            bind_group,
            bind_group_generation: (0, 0, 0, 0),
            // Physics compute resources
            compute_pipeline,
            physics_uniforms,
            physics_edges_meta,
            vertices_a,
            vertices_b,
            compute_bind_group_layout_0,
            compute_bind_group_layout_1,
            compute_bind_group_0: None,
            compute_bind_group_a_to_b: None,
            compute_bind_group_b_to_a: None,
            current_buffer_is_b: false,
            gpu_physics_initialized: false,
            gpu_physics_vertex_count: 0,
            compute_bind_group_0_generation: (0, 0),
            compute_bind_group_1_generation: (0, 0),
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
            &primitive.physics_vertices,
            &primitive.physics_edges,
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
        edges: &[((usize, usize), (usize, usize))],
        edge_color: glam::Vec4,
        background_color: glam::Vec4,
        border_color: glam::Vec4,
        fill_color: glam::Vec4,
        drag_edge_color: glam::Vec4,
        drag_edge_valid_color: glam::Vec4,
        selected_nodes: &std::collections::HashSet<usize>,
        selected_edge_color: glam::Vec4,
        physics_vertices: &[super::primitive::PhysicsVertexData],
        physics_edges: &[super::primitive::PhysicsEdgeData],
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
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
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
                }
            }),
        );

        let num_pins = self.pins.update(
            device,
            queue,
            nodes.iter().flat_map(|node| node.pins.iter()).map(|pin| {
                use crate::node_pin::PinDirection;
                types::Pin {
                    position: pin.offset,
                    color: glam::Vec4::new(pin.color.r, pin.color.g, pin.color.b, pin.color.a),
                    side: pin.side,
                    radius: pin.radius,
                    direction: match pin.direction {
                        PinDirection::Input => 0,
                        PinDirection::Output => 1,
                        PinDirection::Both => 2,
                    },
                    flags: 0,
                    _pad0: 0,
                    _pad1: 0,
                }
            }),
        );

        let num_edges = self.edges.update(
            device,
            queue,
            edges
                .iter()
                .map(|((from_node, from_pin), (to_node, to_pin))| {
                    // Highlight edges where both ends are selected
                    let is_highlighted =
                        selected_nodes.contains(from_node) && selected_nodes.contains(to_node);
                    let color = if is_highlighted {
                        selected_edge_color
                    } else {
                        edge_color
                    };

                    types::Edge {
                        from_node: *from_node as _,
                        from_pin: *from_pin as _,
                        to_node: *to_node as _,
                        to_pin: *to_pin as _,
                        color,
                        thickness: 2.0,
                        _pad0: 0.0,
                        _pad1: 0.0,
                        _pad2: 0.0,
                    }
                }),
        );

        // Upload physics vertices (for polyline edge rendering)
        let _num_vertices = self.vertices.update(
            device,
            queue,
            physics_vertices.iter().map(|v| {
                types::PhysicsVertex {
                    position: v.position,
                    velocity: crate::node_grapgh::euclid::WorldVector::zero(),
                    mass: 1.0,
                    flags: 0, // Not used for rendering
                    edge_index: v.edge_index as u32,
                    vertex_index: v.vertex_index as u32,
                }
            }),
        );

        // Unused for now but available for future use
        let _ = physics_edges;

        let dragging_type: u32 = match dragging {
            Dragging::None => 0,
            Dragging::Graph(_) => 1,
            Dragging::Node(_, _) => 2,
            Dragging::Edge(_, _, _) => 3,
            Dragging::EdgeOver(_, _, _, _) => 4,
            Dragging::BoxSelect(_, _) => 5,
            Dragging::GroupMove(_) => 6,
            Dragging::EdgeCutting(_) => 7,
            Dragging::EdgeVertex { .. } => 8, // Physics vertex drag
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
                Dragging::EdgeCutting(trail) => {
                    let origin = trail.first().copied().unwrap_or(WorldPoint::zero());
                    (0, 0, origin, 0, 0)
                }
                _ => (0, 0, WorldPoint::zero(), 0, 0),
            }
        };

        let scale = viewport.scale_factor() as f32;
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
            viewport_size: glam::Vec2::new(
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ),
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            _pad_end0: 0,
            _pad_end1: 0,
        };
        // println!("uniforms: {:?}", uniforms);
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Only recreate bind group if buffer generations changed
        let current_gen = (
            self.nodes.generation(),
            self.pins.generation(),
            self.edges.generation(),
            self.vertices.generation(),
        );
        if current_gen != self.bind_group_generation {
            self.bind_group = create_bind_group(
                device,
                &self.bind_group_layout,
                self.uniforms.as_entire_binding(),
                self.nodes.as_entire_binding(),
                self.pins.as_entire_binding(),
                self.edges.as_entire_binding(),
                self.vertices.as_entire_binding(),
            );
            self.bind_group_generation = current_gen;
        }

        // println!(
        //     "nodes: {:?} ({:?}), pins: {:?} ({:?}), edges: {:?} ({:?})",
        //     self.nodes.len(),
        //     self.nodes.capacity(),
        //     self.pins.len(),
        //     self.pins.capacity(),
        //     self.edges.len(),
        //     self.edges.capacity(),
        // );
    }

    // ========================================================================
    // PHYSICS COMPUTE METHODS
    // ========================================================================

    /// Check if GPU physics needs initialization.
    /// Returns true if buffers haven't been initialized or structure changed.
    pub fn needs_physics_init(&self, vertex_count: usize) -> bool {
        !self.gpu_physics_initialized || self.gpu_physics_vertex_count != vertex_count
    }

    /// Update only the anchor vertex positions (first and last vertex of each edge).
    /// Call this every frame to ensure anchors track node movement.
    pub fn update_anchor_positions(
        &mut self,
        queue: &Queue,
        anchor_updates: impl Iterator<Item = (usize, WorldPoint)>, // (vertex_index, new_position)
    ) {
        for (idx, pos) in anchor_updates {
            // Write position directly to the current read buffer
            // Position is first 8 bytes of PhysicsVertex struct
            let offset = idx * std::mem::size_of::<types::PhysicsVertex>();
            let pos_array: [f32; 2] = [pos.x, pos.y];
            let pos_bytes = bytemuck::bytes_of(&pos_array);

            if self.current_buffer_is_b {
                queue.write_buffer(self.vertices_b.wgpu_buffer(), offset as u64, pos_bytes);
            } else {
                queue.write_buffer(self.vertices_a.wgpu_buffer(), offset as u64, pos_bytes);
            }
        }
    }

    /// Update physics buffers with vertex and edge metadata.
    /// Call this before dispatch_physics() to upload the latest state.
    /// Only call when needs_physics_init() returns true.
    pub fn update_physics_buffers(
        &mut self,
        device: &Device,
        queue: &Queue,
        vertices: impl Iterator<Item = types::PhysicsVertex>,
        edges_meta: impl Iterator<Item = types::PhysicsEdgeMeta>,
    ) {
        // Update vertices into the current read buffer
        let vertices_vec: Vec<_> = vertices.collect();

        // Track vertex count for structure change detection
        self.gpu_physics_vertex_count = vertices_vec.len();

        if self.current_buffer_is_b {
            let _ = self.vertices_b.update(device, queue, vertices_vec.iter().copied());
        } else {
            let _ = self.vertices_a.update(device, queue, vertices_vec.iter().copied());
        }

        // Also update the other buffer with same data (initial state for ping-pong)
        if self.current_buffer_is_b {
            let _ = self.vertices_a.update(device, queue, vertices_vec.iter().copied());
        } else {
            let _ = self.vertices_b.update(device, queue, vertices_vec.iter().copied());
        }

        // Update edge metadata
        let _ = self.physics_edges_meta.update(device, queue, edges_meta);

        // Recreate compute bind groups if buffer generations changed
        self.update_compute_bind_groups(device);

        // Mark as initialized
        self.gpu_physics_initialized = true;
    }

    /// Recreate compute bind groups if needed.
    fn update_compute_bind_groups(&mut self, device: &Device) {
        // Check if group 0 needs recreation (nodes, edges_meta changed)
        let gen_0 = (self.nodes.generation(), self.physics_edges_meta.generation());
        if self.compute_bind_group_0.is_none() || gen_0 != self.compute_bind_group_0_generation {
            self.compute_bind_group_0 = Some(device.create_bind_group(&BindGroupDescriptor {
                label: Some("Compute Bind Group 0"),
                layout: &self.compute_bind_group_layout_0,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: self.physics_uniforms.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: self.nodes.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: self.physics_edges_meta.as_entire_binding(),
                    },
                ],
            }));
            self.compute_bind_group_0_generation = gen_0;
        }

        // Check if group 1 needs recreation (vertices_a, vertices_b changed)
        let gen_1 = (self.vertices_a.generation(), self.vertices_b.generation());
        if self.compute_bind_group_a_to_b.is_none()
            || self.compute_bind_group_b_to_a.is_none()
            || gen_1 != self.compute_bind_group_1_generation
        {
            // A -> B bind group (read A, write B)
            self.compute_bind_group_a_to_b = Some(device.create_bind_group(&BindGroupDescriptor {
                label: Some("Compute Bind Group A->B"),
                layout: &self.compute_bind_group_layout_1,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: self.vertices_a.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: self.vertices_b.as_entire_binding(),
                    },
                ],
            }));

            // B -> A bind group (read B, write A)
            self.compute_bind_group_b_to_a = Some(device.create_bind_group(&BindGroupDescriptor {
                label: Some("Compute Bind Group B->A"),
                layout: &self.compute_bind_group_layout_1,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: self.vertices_b.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: self.vertices_a.as_entire_binding(),
                    },
                ],
            }));

            self.compute_bind_group_1_generation = gen_1;
        }
    }

    /// Dispatch physics compute shader for the specified number of steps.
    /// Returns the buffer that contains the final physics state.
    pub fn dispatch_physics(
        &mut self,
        encoder: &mut CommandEncoder,
        queue: &Queue,
        config: &crate::node_grapgh::physics::PhysicsConfig,
        steps: u32,
        num_nodes: u32,
    ) {
        if steps == 0 {
            return;
        }

        let num_vertices = if self.current_buffer_is_b {
            self.vertices_b.len()
        } else {
            self.vertices_a.len()
        } as u32;

        let num_edges = self.physics_edges_meta.len() as u32;

        if num_vertices == 0 || num_edges == 0 {
            return;
        }

        // Upload physics uniforms
        let uniforms = types::PhysicsUniforms {
            spring_stiffness: config.spring_stiffness,
            damping: config.damping,
            rest_length: config.rest_length * config.tension_factor,
            node_repulsion: config.node_repulsion,
            edge_repulsion: config.edge_repulsion,
            repulsion_radius: config.max_interaction_range,
            max_velocity: config.max_velocity,
            dt: config.fixed_dt,
            num_vertices,
            num_edges,
            num_nodes,
            gravity: config.gravity,
            bending_stiffness: config.bending_stiffness,
            pin_suction: config.pin_suction,
            path_attraction: config.path_attraction,
            // Improved segment model
            contraction_strength: config.contraction_strength,
            curvature_contraction: config.curvature_contraction,
            node_wrap_distance: config.node_wrap_distance,
            edge_bundle_distance: config.edge_bundle_distance,
            edge_attraction_range: config.max_interaction_range,
            min_segment_length: config.min_segment_length,
            edge_attraction: config.edge_attraction,
            _pad0: 0,
            _pad1: 0,
        };
        queue.write_buffer(&self.physics_uniforms, 0, bytemuck::bytes_of(&uniforms));

        let workgroup_count = (num_vertices + 63) / 64;

        // Run physics steps
        for _ in 0..steps {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("physics compute pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, self.compute_bind_group_0.as_ref().unwrap(), &[]);

            // Select bind group based on current buffer direction
            if self.current_buffer_is_b {
                // Current is B, so read B -> write A
                pass.set_bind_group(1, self.compute_bind_group_b_to_a.as_ref().unwrap(), &[]);
            } else {
                // Current is A, so read A -> write B
                pass.set_bind_group(1, self.compute_bind_group_a_to_b.as_ref().unwrap(), &[]);
            }

            pass.dispatch_workgroups(workgroup_count, 1, 1);
            drop(pass);

            // Swap buffers for next iteration
            self.current_buffer_is_b = !self.current_buffer_is_b;
        }

        // After physics, update render bind group to use the output buffer
        self.update_render_bind_group_for_physics();
    }

    /// Dispatch physics compute shader immediately with own encoder.
    /// This creates a CommandEncoder, runs physics steps, and submits.
    /// Call this during prepare() before update_new().
    ///
    /// NOTE: Before calling this, the physics buffers should be populated
    /// via update_physics_buffers() at least once (when structure changes).
    /// This method assumes buffers are already filled with vertex data.
    pub fn dispatch_physics_immediate(
        &mut self,
        device: &Device,
        queue: &Queue,
        config: &crate::node_grapgh::physics::PhysicsConfig,
        steps: u32,
        num_nodes: u32,
    ) {
        if steps == 0 {
            return;
        }

        // Check if we have any vertices to simulate
        let num_verts = if self.current_buffer_is_b {
            self.vertices_b.len()
        } else {
            self.vertices_a.len()
        };

        if num_verts == 0 {
            // No vertices loaded yet - skip dispatch
            return;
        }

        // Create encoder for compute pass
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("physics compute encoder"),
        });

        // Dispatch physics
        self.dispatch_physics(&mut encoder, queue, config, steps, num_nodes);

        // Copy physics output to render vertices buffer
        // After dispatch, the output is in the current buffer (swapped during dispatch)
        let (source_buffer, source_len) = if self.current_buffer_is_b {
            (self.vertices_b.wgpu_buffer(), self.vertices_b.len())
        } else {
            (self.vertices_a.wgpu_buffer(), self.vertices_a.len())
        };

        // Ensure render buffer has capacity and copy from physics buffer
        let copy_size = source_len * std::mem::size_of::<types::PhysicsVertex>();
        if copy_size > 0 {
            // The vertices buffer might be smaller - resize if needed via the update method
            // For now, we ensure it's large enough by checking capacity
            let render_buffer = self.vertices.wgpu_buffer();
            let render_capacity = self.vertices.capacity() * std::mem::size_of::<types::PhysicsVertex>();

            if render_capacity >= copy_size {
                encoder.copy_buffer_to_buffer(
                    source_buffer,
                    0,
                    render_buffer,
                    0,
                    copy_size as u64,
                );
            }
        }

        // Submit compute work and buffer copy
        queue.submit(std::iter::once(encoder.finish()));

        // Update render bind group to use the physics output buffer
        self.update_render_bind_group_for_physics();
    }

    /// Update render bind group to use the current physics output buffer.
    fn update_render_bind_group_for_physics(&mut self) {
        // The render pipeline's vertices buffer should now point to the
        // current physics output. We need to copy the data or use the
        // physics buffer directly for rendering.
        //
        // For now, we update the `vertices` buffer reference in the bind group
        // by pointing to whichever ping-pong buffer is current.
        //
        // Note: This requires the render bind group to be recreated with the
        // correct buffer. For simplicity, we force regeneration by changing
        // the bind_group_generation tracker.
        //
        // A more efficient approach would be to have two render bind groups
        // and swap between them, but this works for initial implementation.

        // Force bind group regeneration on next update_new() call
        // by invalidating the generation counter
        self.bind_group_generation = (u64::MAX, u64::MAX, u64::MAX, u64::MAX);
    }

    /// Get a reference to the current physics vertices buffer for rendering.
    pub fn current_physics_vertices(&self) -> &buffer::Buffer<types::PhysicsVertex> {
        if self.current_buffer_is_b {
            &self.vertices_b
        } else {
            &self.vertices_a
        }
    }

    #[allow(dead_code)]
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
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        self.render_pass(&mut pass, viewport, layer);
    }

    pub fn render_pass(
        &self,
        pass: &mut iced::wgpu::RenderPass<'_>,
        _viewport: Rectangle<u32>,
        layer: Layer,
    ) {
        let num_nodes = self.nodes.len();
        let num_pins = self.pins.len();
        let num_edges = self.edges.len();

        pass.set_bind_group(0, &self.bind_group, &[]);

        match layer {
            Layer::Background => {
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
            }
            Layer::Foreground => {
                // Pass 5: Dragging edge (if active)
                pass.set_pipeline(&self.pipeline_dragging);
                pass.draw(0..6, 0..1);
            }
        }
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
            module: module,
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
            module: &module,
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
            // Binding 4: Physics vertices (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::PhysicsVertex>() as u64 * 10)
                            .unwrap(),
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
    vertices: BindingResource,
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
            // Entry 4: Physics vertices
            BindGroupEntry {
                binding: 4,
                resource: vertices,
            },
        ],
    })
}

// ============================================================================
// COMPUTE SHADER BIND GROUP LAYOUTS
// ============================================================================

/// Create compute bind group layout 0: uniforms, nodes, edges_meta.
/// Matches @group(0) in physics.wgsl.
fn create_compute_bind_group_layout_0(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Compute Bind Group Layout 0"),
        entries: &[
            // Binding 0: PhysicsUniforms (uniform buffer)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::PhysicsUniforms>() as u64)
                            .unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 1: Nodes (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::Node>() as u64 * 10).unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 2: PhysicsEdgeMeta (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::PhysicsEdgeMeta>() as u64 * 10)
                            .unwrap(),
                    ),
                },
                count: None,
            },
        ],
    })
}

/// Create compute bind group layout 1: vertices_in (read), vertices_out (read_write).
/// Matches @group(1) in physics.wgsl.
fn create_compute_bind_group_layout_1(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Compute Bind Group Layout 1"),
        entries: &[
            // Binding 0: vertices_in (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::PhysicsVertex>() as u64 * 10)
                            .unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 1: vertices_out (storage buffer, read_write)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<types::PhysicsVertex>() as u64 * 10)
                            .unwrap(),
                    ),
                },
                count: None,
            },
        ],
    })
}
