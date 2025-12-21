use crate::node_grapgh::euclid::{WorldPoint, WorldSize, WorldVector};

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Uniforms {
    pub os_scale_factor: f32, // e.g. 1.0, 1.5
    pub camera_zoom: f32,
    pub camera_position: WorldPoint,

    pub border_color: glam::Vec4,          // RGBA for node border
    pub fill_color: glam::Vec4,            // RGBA for node fill
    pub edge_color: glam::Vec4,            // RGBA for edges
    pub background_color: glam::Vec4,      // RGBA for background
    pub drag_edge_color: glam::Vec4,       // RGBA for dragging edge (warning color)
    pub drag_edge_valid_color: glam::Vec4, // RGBA for valid connection (success color)

    pub cursor_position: WorldPoint, // in world coordinates

    pub num_nodes: u32,
    pub num_pins: u32,
    pub num_edges: u32,
    pub time: f32, // Time in seconds for animations

    pub dragging: u32,
    pub _pad_uniforms0: u32,
    pub _pad_uniforms1: u32,
    pub _pad_uniforms2: u32,
    pub dragging_edge_from_node: u32,
    pub dragging_edge_from_pin: u32,
    pub dragging_edge_from_origin: WorldPoint,
    pub dragging_edge_to_node: u32,
    pub dragging_edge_to_pin: u32,

    pub viewport_size: glam::Vec2, // viewport size for clip space transform
    pub bounds_origin: glam::Vec2, // widget bounds origin in physical pixels
    pub bounds_size: glam::Vec2,   // widget bounds size in physical pixels
    pub _pad_end0: u32,            // padding for 16-byte alignment
    pub _pad_end1: u32,
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Node {
    pub position: WorldVector,    // 8 bytes @ 0
    pub size: WorldSize,          // 8 bytes @ 8 (total 16)
    pub corner_radius: f32,       // 4 bytes @ 16
    pub border_width: f32,        // 4 bytes @ 20
    pub opacity: f32,             // 4 bytes @ 24
    pub pin_start: u32,           // 4 bytes @ 28 (total 32)
    pub pin_count: u32,           // 4 bytes @ 32
    pub _pad0: u32,               // 4 bytes @ 36
    pub _pad1: u32,               // 4 bytes @ 40
    pub _pad2: u32,               // 4 bytes @ 44 (total 48)
    pub fill_color: glam::Vec4,   // 16 bytes @ 48 (16-byte aligned)
    pub border_color: glam::Vec4, // 16 bytes @ 64 (total 80)
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
    pub from_node: u32,    // 4 bytes @ 0
    pub from_pin: u32,     // 4 bytes @ 4
    pub to_node: u32,      // 4 bytes @ 8
    pub to_pin: u32,       // 4 bytes @ 12 (total 16)
    pub color: glam::Vec4, // 16 bytes @ 16 (16-byte aligned)
    pub thickness: f32,    // 4 bytes @ 32
    pub edge_type: u32,    // 4 bytes @ 36 (0=Bezier, 1=Straight, 2=SmoothStep, 3=Step)
    pub dash_length: f32,  // 4 bytes @ 40 (0.0 = solid line)
    pub gap_length: f32,   // 4 bytes @ 44 (total 48)
    pub flow_speed: f32,   // 4 bytes @ 48 (pixels per second, 0.0 = no animation)
    pub flags: u32,        // 4 bytes @ 52 (bit 0: animated dash, bit 1: glow, bit 2: pulse)
    pub _pad0: f32,        // 4 bytes @ 56
    pub _pad1: f32,        // 4 bytes @ 60 (total 64)
}
