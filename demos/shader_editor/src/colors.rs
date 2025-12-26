//! Semantic pin colors for the shader_editor demo.
//!
//! Colors are organized by GLSL/WGSL data type for consistent visual language.

use iced::Color;

// === Socket Type Colors ===

/// Scalar float values
pub const SOCKET_FLOAT: Color = Color::from_rgb(0.6, 0.8, 0.6); // Light green

/// 2D vector (UV coordinates)
pub const SOCKET_VEC2: Color = Color::from_rgb(0.6, 0.8, 0.9); // Light cyan

/// 3D vector (positions, normals)
pub const SOCKET_VEC3: Color = Color::from_rgb(0.9, 0.8, 0.5); // Light yellow

/// 4D vector / RGBA colors
pub const SOCKET_VEC4: Color = Color::from_rgb(0.9, 0.6, 0.7); // Light pink

/// Boolean values
pub const SOCKET_BOOL: Color = Color::from_rgb(0.9, 0.5, 0.5); // Light red

/// Integer values
pub const SOCKET_INT: Color = Color::from_rgb(0.7, 0.7, 0.9); // Light purple

// === UI Colors ===

/// Muted text color
pub const TEXT_MUTED: Color = Color::from_rgb(0.6, 0.6, 0.6);

// === Spacing Constants ===

/// Spacing between pin rows
pub const SPACING_PIN: f32 = 6.0;
