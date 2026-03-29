//! Tiling primitives for infinite repeating patterns.
//!
//! Tilings are backgrounds like grids and dot patterns that repeat
//! infinitely. They are always present in every tile of the spatial index.

use crate::drawable::{Drawable, TilingType};

/// Infinite repeating pattern for backgrounds.
#[derive(Debug, Clone, Copy)]
pub enum Tiling {
    /// Rectangular grid.
    Grid { spacing_x: f32, spacing_y: f32, thickness: f32 },
    /// Dot array.
    Dots { spacing_x: f32, spacing_y: f32, radius: f32 },
    /// Equilateral triangle grid.
    Triangles { spacing: f32, thickness: f32 },
    /// Regular hexagonal grid.
    Hex { spacing: f32, thickness: f32 },
}

impl Tiling {
    /// Rectangular grid with given spacing and line thickness.
    pub fn grid(spacing_x: f32, spacing_y: f32, thickness: f32) -> Drawable {
        Drawable::new_tiling(TilingType::Grid, [spacing_x, spacing_y, thickness, 0.0])
    }

    /// Dot array with given spacing and dot radius.
    pub fn dots(spacing_x: f32, spacing_y: f32, radius: f32) -> Drawable {
        Drawable::new_tiling(TilingType::Dots, [spacing_x, spacing_y, radius, 0.0])
    }

    /// Equilateral triangle grid. Spacing = triangle edge length.
    pub fn triangles(spacing: f32, thickness: f32) -> Drawable {
        Drawable::new_tiling(TilingType::Triangles, [spacing, 0.0, thickness, 0.0])
    }

    /// Regular hexagonal grid. Spacing = flat-to-flat distance.
    pub fn hex(spacing: f32, thickness: f32) -> Drawable {
        Drawable::new_tiling(TilingType::Hex, [spacing, 0.0, thickness, 0.0])
    }
}
