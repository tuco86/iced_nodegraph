mod node;
mod pin;

use std::collections::HashSet;

use iced::{Rectangle, wgpu};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Primitive;
pub use node::Node;
pub use pin::Pin;

use crate::node_grapgh::{euclid::WorldPoint, state::Dragging};

use super::pipeline::Pipeline;

#[derive(Debug, Clone, Copy)]
pub enum Layer {
    Background,
    Foreground,
}

/// A physics vertex for rendering edge polylines.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsVertexData {
    pub position: WorldPoint,
    pub edge_index: usize,
    pub vertex_index: usize,
}

/// Edge data that includes physics vertex range.
#[derive(Debug, Clone)]
pub struct PhysicsEdgeData {
    pub from_node: usize,
    pub from_pin: usize,
    pub to_node: usize,
    pub to_pin: usize,
    pub vertex_start: usize,
    pub vertex_count: usize,
}

/// GPU physics dispatch request.
/// If present, the GPU compute shader will be dispatched during prepare().
#[derive(Debug, Clone)]
pub struct GpuPhysicsRequest {
    /// Number of physics steps to run on GPU.
    pub steps: u32,
    /// Physics configuration for the shader.
    pub config: crate::node_grapgh::physics::PhysicsConfig,
}

#[derive(Debug, Clone)]
pub struct NodeGraphPrimitive {
    pub layer: Layer,
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,
    pub cursor_position: WorldPoint,
    pub time: f32, // Time in seconds for animations
    pub dragging: Dragging,
    pub nodes: Vec<Node>,
    pub edges: Vec<((usize, usize), (usize, usize))>, // (from_node, from_pin) -> (to_node, to_pin)
    pub edge_color: glam::Vec4,
    pub background_color: glam::Vec4,
    pub border_color: glam::Vec4,
    pub fill_color: glam::Vec4,
    pub drag_edge_color: glam::Vec4,
    pub drag_edge_valid_color: glam::Vec4,
    /// Currently selected nodes (for edge highlighting)
    pub selected_nodes: HashSet<usize>,
    /// Color for edges between selected nodes
    pub selected_edge_color: glam::Vec4,
    /// Physics vertices for edge polyline rendering (optional).
    pub physics_vertices: Vec<PhysicsVertexData>,
    /// Physics edge metadata with vertex ranges (optional).
    pub physics_edges: Vec<PhysicsEdgeData>,
    /// GPU physics dispatch request (if using GPU physics).
    pub gpu_physics: Option<GpuPhysicsRequest>,
}

impl Primitive for NodeGraphPrimitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        use super::pipeline::types::{PhysicsEdgeMeta, PhysicsVertex};

        // First, update all other data (nodes, edges, uniforms, etc.)
        // This also uploads physics_vertices as fallback, but GPU physics will override
        pipeline.update_new(
            device,
            queue,
            bounds,
            viewport,
            self.camera_zoom,
            self.camera_position,
            self.cursor_position,
            self.time,
            &self.dragging,
            &self.nodes,
            &self.edges,
            self.edge_color,
            self.background_color,
            self.border_color,
            self.fill_color,
            self.drag_edge_color,
            self.drag_edge_valid_color,
            &self.selected_nodes,
            self.selected_edge_color,
            &self.physics_vertices,
            &self.physics_edges,
        );

        // Then, if GPU physics is requested, dispatch compute shader
        // This runs AFTER update_new() so the GPU results override the CPU fallback
        if let Some(ref gpu_request) = self.gpu_physics {
            if gpu_request.steps > 0 && !self.physics_vertices.is_empty() {
                // Only initialize GPU buffers if structure changed or first time
                if pipeline.needs_physics_init(self.physics_vertices.len()) {
                    // Convert physics vertices to GPU format
                    let gpu_vertices = self.physics_vertices.iter().enumerate().map(|(i, v)| {
                        // First and last vertex of each edge are anchored
                        let edge_data = self.physics_edges.iter().find(|e| {
                            i >= e.vertex_start && i < e.vertex_start + e.vertex_count
                        });
                        let is_anchored = if let Some(edge) = edge_data {
                            let local_idx = i - edge.vertex_start;
                            local_idx == 0 || local_idx == edge.vertex_count - 1
                        } else {
                            false
                        };

                        PhysicsVertex::new(
                            v.position,
                            v.edge_index as u32,
                            v.vertex_index as u32,
                            is_anchored,
                        )
                    });

                    // Convert edge metadata to GPU format
                    let gpu_edges = self.physics_edges.iter().map(|e| {
                        PhysicsEdgeMeta {
                            vertex_start: e.vertex_start as u32,
                            vertex_count: e.vertex_count as u32,
                            from_node: e.from_node as u32,
                            from_pin: e.from_pin as u32,
                            to_node: e.to_node as u32,
                            to_pin: e.to_pin as u32,
                            _pad0: 0,
                            _pad1: 0,
                            color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0), // Default white
                            thickness: 2.0, // Default thickness
                            _pad2: 0.0,
                            start_anchor: glam::Vec2::ZERO, // TODO: Fill from actual pin positions
                            end_anchor: glam::Vec2::ZERO,
                            _pad3: 0.0,
                            _pad4: 0.0,
                        }
                    });

                    // Upload vertices and edge metadata to GPU physics buffers (only on init)
                    pipeline.update_physics_buffers(device, queue, gpu_vertices, gpu_edges);
                } else {
                    // Update anchor positions every frame (first and last vertex of each edge)
                    let anchor_updates = self.physics_edges.iter().flat_map(|e| {
                        let first_idx = e.vertex_start;
                        let last_idx = e.vertex_start + e.vertex_count - 1;

                        let first_pos = self.physics_vertices.get(first_idx).map(|v| v.position);
                        let last_pos = self.physics_vertices.get(last_idx).map(|v| v.position);

                        let mut updates = Vec::new();
                        if let Some(pos) = first_pos {
                            updates.push((first_idx, pos));
                        }
                        if let Some(pos) = last_pos {
                            updates.push((last_idx, pos));
                        }
                        updates
                    });

                    pipeline.update_anchor_positions(queue, anchor_updates);
                }

                // Dispatch physics compute shader (this also copies results to render buffer)
                pipeline.dispatch_physics_immediate(
                    device,
                    queue,
                    &gpu_request.config,
                    gpu_request.steps,
                    self.nodes.len() as u32,
                );
            }
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        // Use default viewport - this should come from the bounds in practice
        let viewport = Rectangle {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        pipeline.render_pass(render_pass, viewport, self.layer);
        true // We handle the drawing ourselves
    }
}
