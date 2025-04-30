use crate::node_grapgh::euclid::{WorldPoint, WorldSize, WorldVector};

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Uniforms {
    pub os_scale_factor: f32,       // e.g. 1.0, 1.5
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,

    pub border_color: glam::Vec4, // RGBA for node border
    pub fill_color: glam::Vec4,   // RGBA for node fill

    pub num_nodes: u32,
    pub num_pins: u32,
    pub num_edges: u32,
    pub _padding: u32, // <- für 16-Byte-Alignment
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Node {
    pub position: WorldVector,
    pub size: WorldSize,
    pub corner_radius: f32,
    pub pin_start: u32,
    pub pin_count: u32,
    pub _padding: u32, // <- für 16-Byte-Alignment
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Pin {
    pub position: WorldVector, // offset from top-left
    pub side: u32,
    pub radius: f32,
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Edge {
    pub from_node: u32,
    pub from_pin: u32,
    pub to_node: u32,
    pub to_pin: u32,
}
