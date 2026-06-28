//! Tiling primitives for infinite repeating patterns.
//!
//! Tilings are backgrounds like grids and dot patterns that repeat
//! infinitely. They are always present in every tile of the spatial index.

use crate::drawable::TilingType;

/// Infinite repeating pattern for backgrounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tiling {
    /// Rectangular grid.
    Grid {
        spacing_x: f32,
        spacing_y: f32,
        thickness: f32,
    },
    /// Dot array.
    Dots {
        spacing_x: f32,
        spacing_y: f32,
        radius: f32,
    },
    /// Equilateral triangle grid.
    Triangles { spacing: f32, thickness: f32 },
    /// Regular hexagonal grid.
    Hex { spacing: f32, thickness: f32 },
}

impl Tiling {
    /// Rectangular grid with given spacing and line thickness.
    pub fn grid(spacing_x: f32, spacing_y: f32, thickness: f32) -> Self {
        Tiling::Grid {
            spacing_x,
            spacing_y,
            thickness,
        }
    }

    /// Dot array with given spacing and dot radius.
    pub fn dots(spacing_x: f32, spacing_y: f32, radius: f32) -> Self {
        Tiling::Dots {
            spacing_x,
            spacing_y,
            radius,
        }
    }

    /// Equilateral triangle grid. Spacing = triangle edge length.
    pub fn triangles(spacing: f32, thickness: f32) -> Self {
        Tiling::Triangles { spacing, thickness }
    }

    /// Regular hexagonal grid. Spacing = flat-to-flat distance.
    pub fn hex(spacing: f32, thickness: f32) -> Self {
        Tiling::Hex { spacing, thickness }
    }

    /// The GPU tiling type and its four packed params (the form the shader reads).
    pub(crate) fn to_gpu(self) -> (TilingType, [f32; 4]) {
        match self {
            Tiling::Grid {
                spacing_x,
                spacing_y,
                thickness,
            } => (TilingType::Grid, [spacing_x, spacing_y, thickness, 0.0]),
            Tiling::Dots {
                spacing_x,
                spacing_y,
                radius,
            } => (TilingType::Dots, [spacing_x, spacing_y, radius, 0.0]),
            Tiling::Triangles { spacing, thickness } => {
                (TilingType::Triangles, [spacing, 0.0, thickness, 0.0])
            }
            Tiling::Hex { spacing, thickness } => (TilingType::Hex, [spacing, 0.0, thickness, 0.0]),
        }
    }
}
