use crate::node_grapgh::euclid::WorldVector;

#[derive(Debug)]
pub struct Pin {
    pub side: u32,
    pub offset: WorldVector, // offset from top-left
    pub radius: f32,
}
