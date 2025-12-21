use crate::node_grapgh::euclid::WorldVector;
use crate::node_pin::PinDirection;
use crate::style::PinShape;
use iced::Color;

#[derive(Debug, Clone, Copy)]
pub struct Pin {
    pub side: u32,
    pub offset: WorldVector, // offset from top-left
    pub radius: f32,
    pub color: Color,
    pub direction: PinDirection,
    /// Shape of the pin indicator (Circle, Square, Diamond, Triangle)
    pub shape: PinShape,
    /// Optional border color (if any component > 0, border is rendered)
    pub border_color: Color,
    /// Border width in world-space pixels
    pub border_width: f32,
}

impl Default for Pin {
    fn default() -> Self {
        Self {
            side: 0,
            offset: WorldVector::new(0.0, 0.0),
            radius: 6.0,
            color: Color::from_rgb(0.5, 0.5, 0.5),
            direction: PinDirection::Both,
            shape: PinShape::Circle,
            border_color: Color::TRANSPARENT,
            border_width: 1.0,
        }
    }
}
