use iced::Color;

use crate::node_grapgh::euclid::{WorldSize, WorldVector};

use super::Pin;

/// Node state flags for GPU rendering.
pub struct NodeFlags;

impl NodeFlags {
    pub const SELECTED: u32 = 1 << 1;
}

#[derive(Debug, Clone)]
pub struct Node {
    pub(crate) position: WorldVector,
    pub(crate) size: WorldSize,
    pub(crate) corner_radius: f32,
    pub(crate) border_width: f32,
    pub(crate) opacity: f32,
    pub(crate) fill_color: Color,
    pub(crate) border_color: Color,
    pub(crate) pins: Vec<Pin>,
    // Shadow properties
    pub(crate) shadow_offset: (f32, f32),
    pub(crate) shadow_blur: f32,
    pub(crate) shadow_color: Color,
    // State flags (hovered, selected)
    pub(crate) flags: u32,
}
