use crate::node_grapgh::euclid::{WorldSize, WorldVector};

use super::Pin;

#[derive(Debug, Clone)]
pub struct Node {
    pub(crate) position: WorldVector,
    pub(crate) size: WorldSize,
    pub(crate) corner_radius: f32,
    pub(crate) pins: Vec<Pin>,
}
