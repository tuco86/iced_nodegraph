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

    // Dragging edge gradient colors (resolved in Rust from pin colors)
    pub dragging_edge_start_color: glam::Vec4, // Color at source pin end
    pub dragging_edge_end_color: glam::Vec4,   // Color at cursor/target end

    // Theme-derived visual parameters (computed in Rust, no hardcodes in shader)
    pub grid_color: glam::Vec4,           // Pre-computed grid line color
    pub hover_glow_color: glam::Vec4,     // Node hover glow color
    pub selection_box_color: glam::Vec4,  // Box selection fill/border color
    pub edge_cutting_color: glam::Vec4,   // Edge cutting line color
    pub hover_glow_radius: f32,           // Node hover glow radius in world units
    pub edge_thickness: f32,              // Default edge thickness for dragging
    pub _pad_theme1: f32,
    pub _pad_theme2: f32,

    pub viewport_size: glam::Vec2, // viewport size for clip space transform
    pub bounds_origin: glam::Vec2, // widget bounds origin in physical pixels
    pub bounds_size: glam::Vec2,   // widget bounds size in physical pixels
    pub _pad_end0: u32,            // padding for 16-byte alignment
    pub _pad_end1: u32,
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Node {
    pub position: WorldVector,     // 8 bytes @ 0
    pub size: WorldSize,           // 8 bytes @ 8 (total 16)
    pub corner_radius: f32,        // 4 bytes @ 16
    pub border_width: f32,         // 4 bytes @ 20
    pub opacity: f32,              // 4 bytes @ 24
    pub pin_start: u32,            // 4 bytes @ 28 (total 32)
    pub pin_count: u32,            // 4 bytes @ 32
    pub shadow_blur: f32,          // 4 bytes @ 36
    pub shadow_offset: glam::Vec2, // 8 bytes @ 40 (total 48)
    pub fill_color: glam::Vec4,    // 16 bytes @ 48 (16-byte aligned)
    pub border_color: glam::Vec4,  // 16 bytes @ 64 (total 80)
    pub shadow_color: glam::Vec4,  // 16 bytes @ 80 (total 96)
    pub flags: u32,                // 4 bytes @ 96 (bit 0: hovered, bit 1: selected)
    pub _pad_flags0: u32,          // 4 bytes @ 100
    pub _pad_flags1: u32,          // 4 bytes @ 104
    pub _pad_flags2: u32,          // 4 bytes @ 108 (total 112, aligned to 16)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Pin {
    pub position: WorldVector,    // vec2<f32> = 8 bytes @ 0
    pub side: u32,                // 4 bytes @ 8
    pub radius: f32,              // 4 bytes @ 12 (total 16 bytes)
    pub color: glam::Vec4,        // vec4<f32> = 16 bytes @ 16 (total 32 bytes)
    pub border_color: glam::Vec4, // vec4<f32> = 16 bytes @ 32 (total 48 bytes)
    pub direction: u32,           // 4 bytes @ 48
    pub shape: u32,               // 4 bytes @ 52 (0=Circle, 1=Square, 2=Diamond, 3=Triangle)
    pub border_width: f32,        // 4 bytes @ 56
    pub flags: u32,               // 4 bytes @ 60 (total 64 bytes - aligned to 16)
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Edge {
    pub from_node: u32,        // 4 bytes @ 0
    pub from_pin: u32,         // 4 bytes @ 4
    pub to_node: u32,          // 4 bytes @ 8
    pub to_pin: u32,           // 4 bytes @ 12 (total 16)
    pub start_color: glam::Vec4, // 16 bytes @ 16 - color at source pin (t=0)
    pub end_color: glam::Vec4,   // 16 bytes @ 32 - color at target pin (t=1)
    pub thickness: f32,        // 4 bytes @ 48
    pub edge_type: u32,        // 4 bytes @ 52 (0=Bezier, 1=Straight, 2=SmoothStep, 3=Step)
    pub dash_length: f32,      // 4 bytes @ 56 (0.0 = solid line)
    pub gap_length: f32,       // 4 bytes @ 60 (total 64)
    pub flow_speed: f32,       // 4 bytes @ 64 (pixels per second, 0.0 = no animation)
    pub flags: u32,            // 4 bytes @ 68 (bit 0: animated dash, bit 1: glow, bit 2: pulse)
    pub _pad0: f32,            // 4 bytes @ 72
    pub _pad1: f32,            // 4 bytes @ 76 (total 80)
}
