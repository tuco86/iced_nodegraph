//! Semantic pin colors for the 500_nodes shader graph demo, keyed by GLSL data
//! type. `pin_style` resolves a pin's marker `TypeId` to one of these.

use iced::Color;

// === Pin Data Type Markers ===
// These marker types are used with the pin! macro for TypeId-based matching

/// Scalar float data type marker
pub struct Float;

/// 2D vector data type marker
pub struct Vec2;

/// 3D vector data type marker
pub struct Vec3;

/// 4D vector / RGBA data type marker
pub struct Vec4;

// === Data Type Colors ===

/// Scalar float values (time, parameters)
pub const PIN_FLOAT: Color = Color::from_rgb(0.9, 0.5, 0.2); // Orange

/// 2D vector (UV coordinates)
pub const PIN_VEC2: Color = Color::from_rgb(0.9, 0.7, 0.3); // Amber

/// 3D vector (positions, directions)
pub const PIN_VEC3: Color = Color::from_rgb(0.5, 0.9, 0.9); // Cyan

/// 4D vector / RGBA colors
pub const PIN_VEC4: Color = Color::from_rgb(0.9, 0.5, 0.9); // Magenta

/// Fallback for pins whose marker is none of the four data types above.
pub const PIN_GENERIC_IN: Color = Color::from_rgb(0.8, 0.8, 0.8); // Light Gray

// === Spacing Constants ===

/// Spacing between pin rows
pub const SPACING_PIN: f32 = 6.0;
