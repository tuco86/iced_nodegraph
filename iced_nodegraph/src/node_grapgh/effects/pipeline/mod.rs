use std::num::NonZeroU64;

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
use iced_wgpu::graphics::Viewport;

use crate::node_grapgh::{effects::Node, euclid::WorldPoint, state::Dragging};

use super::{Layer, primitive::NodeGraphPrimitive};

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
    pipeline_foreground: RenderPipeline,

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
            label: Some("node shaders"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        // Create all 5 pipelines
        let pipeline_background = create_pipeline_custom(device, format, &layout, &module, "vs_background", "fs_background", "background");
        let pipeline_edges = create_pipeline_custom(device, format, &layout, &module, "vs_edge", "fs_edge", "edges");
        let pipeline_nodes = create_pipeline_custom(device, format, &layout, &module, "vs_node", "fs_node", "nodes");
        let pipeline_pins = create_pipeline_custom(device, format, &layout, &module, "vs_pin", "fs_pin", "pins");
        let pipeline_dragging = create_pipeline_custom(device, format, &layout, &module, "vs_dragging", "fs_dragging", "dragging");
        let pipeline_foreground = create_pipeline_custom(device, format, &layout, &module, "vs_main", "fs_foreground", "foreground_legacy");

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
        }
    }

    #[allow(dead_code)]
    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        viewport: &Viewport,
        primitive: &NodeGraphPrimitive,
    ) {
        self.update_new(
            device,
            queue,
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
        );
    }

    pub fn update_new(
        &mut self,
        device: &Device,
        queue: &Queue,
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
    ) {
        // Calculate viewport bounds in world coordinates for frustum culling
        // IMPORTANT: Use LOGICAL pixels, not physical pixels!
        // camera.screen_to_world() works with logical coordinates (ignores scale_factor)
        // Shader transforms: screen_physical = (world + position) * zoom * scale_factor
        // But input coordinates are logical: screen_logical = screen_physical / scale_factor
        // So: world = screen_logical / zoom - position (matches camera.rs formula!)
        let scale_factor = viewport.scale_factor();
        let viewport_width = viewport.physical_width() as f32 / scale_factor;
        let viewport_height = viewport.physical_height() as f32 / scale_factor;

        let inv_zoom = 1.0 / camera_zoom;

        // Transform logical screen corners to world space (same formula as camera.screen_to_world)
        let world_min_x = (0.0 * inv_zoom) - camera_position.x;
        let world_max_x = (viewport_width * inv_zoom) - camera_position.x;
        let world_min_y = (0.0 * inv_zoom) - camera_position.y;
        let world_max_y = (viewport_height * inv_zoom) - camera_position.y;

        // Add padding to avoid culling nodes near screen edges (in world units)
        let padding = 100.0 / camera_zoom;
        let world_min_x = world_min_x - padding;
        let world_max_x = world_max_x + padding;
        let world_min_y = world_min_y - padding;
        let world_max_y = world_max_y + padding;

        // Filter visible nodes (bounding box intersection test)
        let visible_nodes: Vec<(usize, &Node)> = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                let node_min_x = node.position.x;
                let node_max_x = node.position.x + node.size.width;
                let node_min_y = node.position.y;
                let node_max_y = node.position.y + node.size.height;

                // AABB intersection test
                node_max_x >= world_min_x
                    && node_min_x <= world_max_x
                    && node_max_y >= world_min_y
                    && node_min_y <= world_max_y
            })
            .collect();

        // Create mapping from original node indices to new shader buffer indices
        // This is CRITICAL: after culling, node indices change!
        // Example: if we cull and keep nodes [5, 10, 15], they become [0, 1, 2] in shader
        let node_index_map: std::collections::HashMap<usize, u32> = visible_nodes
            .iter()
            .enumerate()
            .map(|(new_idx, (orig_idx, _))| (*orig_idx, new_idx as u32))
            .collect();

        // Filter edges: calculate bounding box for each Bezier curve and test intersection
        // with viewport. This matches the shader's edge rendering (cubic Bezier with
        // control points offset by seg_len in pin direction).
        const SEG_LEN: f32 = 80.0; // Must match shader.wgsl fs_edge

        let visible_edges: Vec<&((usize, usize), (usize, usize))> = edges
            .iter()
            .filter(|((from_node_idx, from_pin_idx), (to_node_idx, to_pin_idx))| {
                // CRITICAL: Both nodes must be in the visible set!
                // Otherwise the shader will reference invalid node indices
                if !node_index_map.contains_key(from_node_idx) || !node_index_map.contains_key(to_node_idx) {
                    return false;
                }

                let from_node = &nodes[*from_node_idx];
                let to_node = &nodes[*to_node_idx];
                let from_pin = &from_node.pins[*from_pin_idx];
                let to_pin = &to_node.pins[*to_pin_idx];

                // Calculate Bezier control points (same as shader)
                let p0_x = from_node.position.x + from_pin.offset.x;
                let p0_y = from_node.position.y + from_pin.offset.y;

                let p3_x = to_node.position.x + to_pin.offset.x;
                let p3_y = to_node.position.y + to_pin.offset.y;

                // Direction vectors (0=Left, 1=Right, 2=Top, 3=Bottom)
                let (dir_from_x, dir_from_y) = match from_pin.side {
                    0 => (-1.0, 0.0),  // Left
                    1 => (1.0, 0.0),   // Right
                    2 => (0.0, -1.0),  // Top
                    _ => (0.0, 1.0),   // Bottom
                };
                let (dir_to_x, dir_to_y) = match to_pin.side {
                    0 => (-1.0, 0.0),
                    1 => (1.0, 0.0),
                    2 => (0.0, -1.0),
                    _ => (0.0, 1.0),
                };

                let p1_x = p0_x + dir_from_x * SEG_LEN;
                let p1_y = p0_y + dir_from_y * SEG_LEN;
                let p2_x = p3_x + dir_to_x * SEG_LEN;
                let p2_y = p3_y + dir_to_y * SEG_LEN;

                // Conservative bounding box for cubic Bezier (covers all 4 control points)
                let edge_min_x = p0_x.min(p1_x).min(p2_x).min(p3_x);
                let edge_max_x = p0_x.max(p1_x).max(p2_x).max(p3_x);
                let edge_min_y = p0_y.min(p1_y).min(p2_y).min(p3_y);
                let edge_max_y = p0_y.max(p1_y).max(p2_y).max(p3_y);

                // AABB intersection test with viewport
                edge_max_x >= world_min_x
                    && edge_min_x <= world_max_x
                    && edge_max_y >= world_min_y
                    && edge_min_y <= world_max_y
            })
            .collect();

        #[cfg(debug_assertions)]
        if nodes.len() > 100 {
            println!(
                "Frustum culling: {}/{} nodes, {}/{} edges visible (zoom: {:.2}x)",
                visible_nodes.len(),
                nodes.len(),
                visible_edges.len(),
                edges.len(),
                camera_zoom
            );
        }

        let mut pin_start = 0;
        let num_nodes = self.nodes.update(
            device,
            queue,
            visible_nodes.iter().map(|(_, node)| {
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
            visible_nodes.iter().flat_map(|(_, node)| node.pins.iter()).map(|pin| {
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
            visible_edges
                .iter()
                .map(|((from_node, from_pin), (to_node, to_pin))| {
                    // CRITICAL: Remap node indices from original to new culled buffer indices!
                    // After culling, nodes are reindexed in the shader buffer
                    let from_node_new = *node_index_map.get(from_node).expect("from_node must be in visible set");
                    let to_node_new = *node_index_map.get(to_node).expect("to_node must be in visible set");

                    types::Edge {
                        from_node: from_node_new,
                        from_pin: *from_pin as _,
                        to_node: to_node_new,
                        to_pin: *to_pin as _,
                    }
                }),
        );

        let dragging_type: u32 = match dragging {
            Dragging::None => 0,
            Dragging::Graph(_) => 1,
            Dragging::Node(_, _) => 2,
            Dragging::Edge(_, _, _) => 3,
            Dragging::EdgeOver(_, _, _, _) => 4,
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
                _ => (0, 0, WorldPoint::zero(), 0, 0),
            }
        };

        let uniforms = types::Uniforms {
            os_scale_factor: viewport.scale_factor() as _,
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
            viewport_size: glam::Vec2::new(viewport.physical_width() as f32, viewport.physical_height() as f32),
            _pad_viewport0: 0,
            _pad_viewport1: 0,
        };
        // println!("uniforms: {:?}", uniforms);
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            self.uniforms.as_entire_binding(),
            self.nodes.as_entire_binding(),
            self.pins.as_entire_binding(),
            self.edges.as_entire_binding(),
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_grapgh::euclid::ScreenPoint;
    use crate::node_grapgh::camera::Camera2D;

    const EPSILON: f32 = 0.1;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    /// Test that our culling viewport calculation matches camera.screen_to_world()
    #[test]
    fn test_culling_viewport_matches_camera() {
        // Simulated viewport (800x600 LOGICAL pixels)
        let viewport_width = 800.0;
        let viewport_height = 600.0;

        // Camera state
        let camera_zoom = 1.0;
        let camera_position = WorldPoint::new(0.0, 0.0);

        // Create camera for comparison
        let camera = Camera2D::with_zoom_and_position(camera_zoom, camera_position);

        // Culling calculation (what we do in update_new) - uses LOGICAL pixels
        let inv_zoom = 1.0 / camera_zoom;
        let world_min_x = (0.0 * inv_zoom) - camera_position.x;
        let world_max_x = (viewport_width * inv_zoom) - camera_position.x;
        let world_min_y = (0.0 * inv_zoom) - camera_position.y;
        let world_max_y = (viewport_height * inv_zoom) - camera_position.y;

        // Camera calculation (what camera.screen_to_world does)
        // camera.rs: world = screen / zoom - position
        let camera_top_left = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(0.0, 0.0));
        let camera_bottom_right = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(viewport_width, viewport_height));

        // They should match exactly (both use logical pixels, ignore scale_factor)
        assert!(
            approx_eq(world_min_x, camera_top_left.x),
            "Min X mismatch: culling={}, camera={}",
            world_min_x,
            camera_top_left.x
        );
        assert!(
            approx_eq(world_min_y, camera_top_left.y),
            "Min Y mismatch: culling={}, camera={}",
            world_min_y,
            camera_top_left.y
        );
        assert!(
            approx_eq(world_max_x, camera_bottom_right.x),
            "Max X mismatch: culling={}, camera={}",
            world_max_x,
            camera_bottom_right.x
        );
        assert!(
            approx_eq(world_max_y, camera_bottom_right.y),
            "Max Y mismatch: culling={}, camera={}",
            world_max_y,
            camera_bottom_right.y
        );
    }

    /// Test culling with zoom > 1.0 (zoomed in)
    #[test]
    fn test_culling_viewport_zoomed_in() {
        let viewport_width = 800.0;
        let viewport_height = 600.0;
        let camera_zoom = 2.0; // Zoomed in 2x
        let camera_position = WorldPoint::new(0.0, 0.0);

        let camera = Camera2D::with_zoom_and_position(camera_zoom, camera_position);

        let inv_zoom = 1.0 / camera_zoom;
        let world_min_x = (0.0 * inv_zoom) - camera_position.x;
        let world_max_x = (viewport_width * inv_zoom) - camera_position.x;
        let world_min_y = (0.0 * inv_zoom) - camera_position.y;
        let world_max_y = (viewport_height * inv_zoom) - camera_position.y;

        let camera_top_left = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(0.0, 0.0));
        let camera_bottom_right = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(viewport_width, viewport_height));

        // At zoom 2.0, we should see half the world space (400x300 instead of 800x600)
        assert!(
            approx_eq(world_min_x, camera_top_left.x),
            "Zoomed: Min X mismatch: culling={}, camera={}",
            world_min_x,
            camera_top_left.x
        );
        assert!(
            approx_eq(world_max_x, camera_bottom_right.x),
            "Zoomed: Max X mismatch: culling={}, camera={}, expected ~400",
            world_max_x,
            camera_bottom_right.x
        );
    }

    /// Test culling with camera panned away from origin
    #[test]
    fn test_culling_viewport_with_pan() {
        let viewport_width = 800.0;
        let viewport_height = 600.0;
        let camera_zoom = 1.0;
        let camera_position = WorldPoint::new(-200.0, -150.0);

        let camera = Camera2D::with_zoom_and_position(camera_zoom, camera_position);

        let inv_zoom = 1.0 / camera_zoom;
        let world_min_x = (0.0 * inv_zoom) - camera_position.x;
        let world_max_x = (viewport_width * inv_zoom) - camera_position.x;
        let world_min_y = (0.0 * inv_zoom) - camera_position.y;
        let world_max_y = (viewport_height * inv_zoom) - camera_position.y;

        let camera_top_left = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(0.0, 0.0));
        let camera_bottom_right = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(viewport_width, viewport_height));

        assert!(
            approx_eq(world_min_x, camera_top_left.x),
            "Panned: Min X mismatch: culling={}, camera={}",
            world_min_x,
            camera_top_left.x
        );
        assert!(
            approx_eq(world_max_x, camera_bottom_right.x),
            "Panned: Max X mismatch: culling={}, camera={}",
            world_max_x,
            camera_bottom_right.x
        );
    }

    /// Test culling with combined zoom and pan
    #[test]
    fn test_culling_viewport_combined() {
        let viewport_width = 800.0;
        let viewport_height = 600.0;
        let camera_zoom = 1.5;
        let camera_position = WorldPoint::new(-100.0, -75.0);

        let camera = Camera2D::with_zoom_and_position(camera_zoom, camera_position);

        let inv_zoom = 1.0 / camera_zoom;
        let world_min_x = (0.0 * inv_zoom) - camera_position.x;
        let world_max_x = (viewport_width * inv_zoom) - camera_position.x;
        let world_min_y = (0.0 * inv_zoom) - camera_position.y;
        let world_max_y = (viewport_height * inv_zoom) - camera_position.y;

        let camera_top_left = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(0.0, 0.0));
        let camera_bottom_right = camera
            .screen_to_world()
            .transform_point(ScreenPoint::new(viewport_width, viewport_height));

        // With combined zoom and pan, culling should still match camera exactly
        assert!(
            approx_eq(world_min_x, camera_top_left.x),
            "Combined: Min X mismatch: culling={}, camera={}",
            world_min_x,
            camera_top_left.x
        );
        assert!(
            approx_eq(world_min_y, camera_top_left.y),
            "Combined: Min Y mismatch: culling={}, camera={}",
            world_min_y,
            camera_top_left.y
        );
        assert!(
            approx_eq(world_max_x, camera_bottom_right.x),
            "Combined: Max X mismatch: culling={}, camera={}",
            world_max_x,
            camera_bottom_right.x
        );
        assert!(
            approx_eq(world_max_y, camera_bottom_right.y),
            "Combined: Max Y mismatch: culling={}, camera={}",
            world_max_y,
            camera_bottom_right.y
        );
    }
}
