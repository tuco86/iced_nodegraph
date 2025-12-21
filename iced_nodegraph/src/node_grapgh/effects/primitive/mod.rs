mod node;
mod pin;

use std::collections::HashSet;

use iced::{Rectangle, wgpu};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Primitive;
pub use node::Node;
pub use pin::Pin;

use crate::node_grapgh::{euclid::WorldPoint, state::Dragging};
use crate::style::EdgeStyle;

use super::pipeline::Pipeline;

#[derive(Debug, Clone, Copy)]
pub enum Layer {
    Background,
    Foreground,
}

/// Edge data for GPU rendering, including style information.
#[derive(Debug, Clone)]
pub struct EdgeData {
    pub from_node: usize,
    pub from_pin: usize,
    pub to_node: usize,
    pub to_pin: usize,
    pub style: EdgeStyle,
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
    pub edges: Vec<EdgeData>,
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
        );
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
