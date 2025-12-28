//! Semantic pin colors for the 500_nodes shader graph demo.
//!
//! Colors are organized by GLSL data type for consistent visual language.

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

/// Normal vectors (special vec3)
pub const PIN_NORMAL: Color = Color::from_rgb(0.5, 0.7, 0.9); // Light Blue

/// Position vectors (special vec3)
pub const PIN_POSITION: Color = Color::from_rgb(0.3, 0.9, 0.5); // Green

/// Generic input pin (math operations)
pub const PIN_GENERIC_IN: Color = Color::from_rgb(0.8, 0.8, 0.8); // Light Gray

/// Generic output pin (math operations)
pub const PIN_GENERIC_OUT: Color = Color::from_rgb(0.9, 0.9, 0.9); // White-ish

/// Noise output values
pub const PIN_NOISE: Color = Color::from_rgb(0.7, 0.9, 0.7); // Light Green

/// Emission output
pub const PIN_EMISSION: Color = Color::from_rgb(0.9, 0.9, 0.3); // Yellow

// === Vector Component Colors (XYZ/RGB) ===

/// X component / Red channel
pub const PIN_X: Color = Color::from_rgb(0.9, 0.3, 0.3); // Red

/// Y component / Green channel
pub const PIN_Y: Color = Color::from_rgb(0.3, 0.9, 0.3); // Green

/// Z component / Blue channel
pub const PIN_Z: Color = Color::from_rgb(0.3, 0.3, 0.9); // Blue

// === Spacing Constants ===

/// Spacing between pin rows
pub const SPACING_PIN: f32 = 6.0;
