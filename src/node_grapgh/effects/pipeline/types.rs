use crate::node_grapgh::euclid::{WorldPoint, WorldSize, WorldVector};

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Uniforms {
    pub os_scale_factor: f32,       // e.g. 1.0, 1.5
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,
    
    pub border_color: glam::Vec4,     // RGBA for node border
    pub fill_color: glam::Vec4,       // RGBA for node fill
    pub edge_color: glam::Vec4,       // RGBA for edges
    pub background_color: glam::Vec4, // RGBA for background
    pub drag_edge_color: glam::Vec4,  // RGBA for dragging edge (warning color)
    pub drag_edge_valid_color: glam::Vec4, // RGBA for valid connection (success color)
    
    pub cursor_position: WorldPoint, // in world coordinates

    pub num_nodes: u32,
    pub num_pins: u32,
    pub num_edges: u32,
    pub time: f32,              // Time in seconds for animations

    pub dragging: u32,
    pub _pad_uniforms0: u32,
    pub _pad_uniforms1: u32,
    pub _pad_uniforms2: u32,
    pub dragging_edge_from_node: u32,
    pub dragging_edge_from_pin: u32,
    pub dragging_edge_from_origin: WorldPoint,
    pub dragging_edge_to_node: u32,
    pub dragging_edge_to_pin: u32,
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Pin {
    pub position: WorldVector, // vec2<f32> = 8 bytes
    pub side: u32,             // 4 bytes
    pub radius: f32,           // 4 bytes (total 16 bytes - aligned)
    pub color: glam::Vec4,     // vec4<f32> = 16 bytes (total 32 bytes - aligned)
    pub direction: u32,        // 4 bytes
    pub flags: u32,            // 4 bytes
    pub _pad0: u32,            // 4 bytes
    pub _pad1: u32,            // 4 bytes (total 48 bytes - aligned to 16)
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Edge {
    pub from_node: u32,
    pub from_pin: u32,
    pub to_node: u32,
    pub to_pin: u32,
}
