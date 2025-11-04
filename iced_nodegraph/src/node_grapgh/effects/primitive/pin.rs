use crate::node_grapgh::euclid::WorldVector;
use crate::node_pin::PinDirection;
use iced::Color;

#[derive(Debug, Clone, Copy)]
pub struct Pin {
    pub side: u32,
    pub offset: WorldVector, // offset from top-left
    pub radius: f32,
    pub color: Color,
    pub direction: PinDirection,
}
