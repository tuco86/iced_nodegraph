//! Distance-stop style system.
//!
//! A style colours a shape along two axes:
//! - Arc-length axis (0..1): each stop's `start` -> `end` follows the contour.
//! - Distance axis: a chain of [`Stop`]s, ascending by `dist` (negative inside
//!   the shape, positive outside), evaluated as one piecewise-`smoothstep`
//!   gradient in a single fragment pass.
//!
//! Evaluation at signed distance `d`:
//! - `d <= stops[0].dist`: hold `stops[0]` (clamped).
//! - between consecutive stops: `smoothstep`-blend, the transition window
//!   widened to at least one pixel so a zero-width step is a crisp antialiased
//!   edge and a wide band is a soft gradient.
//! - `d >= stops[last].dist`: hold `stops[last]` (clamped).
//!
//! A region disappears by ending at a transparent stop; a gap is a transparent
//! stop between opaque ones. Because the whole profile is one entry, bands never
//! composite against each other, so abutting bands cannot seam.

use iced::Color;

use crate::color::ColorQuad;
use crate::pattern::Pattern;

/// Largest stop chain the GPU style supports. Keep in sync with `shader.wgsl`.
pub const MAX_STOPS: usize = 8;

/// Same colour with zero alpha.
fn transparent(c: Color) -> Color {
    Color { a: 0.0, ..c }
}

/// One stop in a style's distance profile: an arc-length colour pair (`start`
/// at arc 0, `end` at arc 1) placed at signed distance `dist`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    /// Signed distance (negative inside the shape, positive outside).
    pub dist: f32,
    /// Colour at arc 0.
    pub start: Color,
    /// Colour at arc 1.
    pub end: Color,
}

impl Stop {
    /// Flat-colour stop (same colour across the arc).
    pub fn new(dist: f32, color: Color) -> Self {
        Self {
            dist,
            start: color,
            end: color,
        }
    }

    /// Arc-gradient stop (`start` at arc 0, `end` at arc 1).
    pub fn grad(dist: f32, start: Color, end: Color) -> Self {
        Self { dist, start, end }
    }
}

/// A unary warp on the post-smoothstep blend parameter `t` in the distance-stop
/// fold (A3 transfer, variant B). It is COLOR-domain - it reshapes how one stop
/// eases into the next without moving any stop or touching `dist`. [`Transfer::Linear`]
/// is the identity and the default, so adding a transfer never changes an existing
/// style's output.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Transfer {
    /// Identity: `t' = t` (no change).
    #[default]
    Linear,
    /// Smoothstep easing `3t^2 - 2t^3`, softening both ends of each blend.
    Smoothstep,
    /// `t' = t^exponent`. Exponent > 1 biases the blend toward the near stop,
    /// < 1 toward the far stop. Used for perceptual/radial falloff shaping.
    Gamma(f32),
}

/// Rendering style: a distance-stop chain + optional pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Distance stops, ascending by `dist`. Never empty.
    pub stops: Vec<Stop>,
    /// Optional pattern (stroke layout along the contour).
    pub pattern: Option<Pattern>,
    /// Color-domain warp on the stop-blend parameter. Defaults to
    /// [`Transfer::Linear`] (identity).
    pub transfer: Transfer,
}

impl Style {
    /// Solid color fill (interior of closed shape): opaque inside, antialiased
    /// silhouette at distance 0, transparent outside.
    pub fn solid(color: Color) -> Self {
        Self::bare(vec![
            Stop::new(0.0, color),
            Stop::new(0.0, transparent(color)),
        ])
    }

    /// Stroke with uniform color and thickness.
    pub fn stroke(color: Color, pattern: Pattern) -> Self {
        Self {
            stops: vec![Stop::new(0.0, color)],
            pattern: Some(pattern),
            transfer: Transfer::Linear,
        }
    }

    /// Arc-length gradient (start -> end) over a fill.
    pub fn arc_gradient(start: Color, end: Color) -> Self {
        Self::bare(vec![
            Stop::grad(0.0, start, end),
            Stop::grad(0.0, transparent(start), transparent(end)),
        ])
    }

    /// Arc-length gradient stroke with pattern.
    pub fn arc_gradient_stroke(start: Color, end: Color, pattern: Pattern) -> Self {
        Self {
            stops: vec![Stop::grad(0.0, start, end)],
            pattern: Some(pattern),
            transfer: Transfer::Linear,
        }
    }

    /// A clipped color band over `[from, to]` from a [`ColorQuad`]: transparent
    /// outside, the quad's near colors at `from` and far colors at `to`,
    /// antialiased at both edges. Built as a four-stop chain so it is one
    /// self-contained entry (no inter-band seam). Apply opacity to the quad with
    /// [`ColorQuad::with_opacity`] before calling if needed.
    pub fn quad_band(quad: &ColorQuad, from: f32, to: f32) -> Self {
        Self::bare(vec![
            Stop::grad(
                from,
                transparent(quad.near_start),
                transparent(quad.near_end),
            ),
            Stop::grad(from, quad.near_start, quad.near_end),
            Stop::grad(to, quad.far_start, quad.far_end),
            Stop::grad(to, transparent(quad.far_start), transparent(quad.far_end)),
        ])
    }

    /// Stroke whose arc colors come from a [`ColorQuad`]'s near pair; the pattern
    /// lays the stroke out along the contour.
    pub fn quad_stroke(quad: &ColorQuad, pattern: Pattern) -> Self {
        let (start, end) = quad.arc_pair();
        Self {
            stops: vec![Stop::grad(0.0, start, end)],
            pattern: Some(pattern),
            transfer: Transfer::Linear,
        }
    }

    /// Outward glow: nothing inside, full color at the silhouette, fading to
    /// transparent at `radius`.
    pub fn shadow(color: Color, radius: f32) -> Self {
        Self::bare(vec![
            Stop::new(0.0, transparent(color)),
            Stop::new(0.0, color),
            Stop::new(radius.max(0.001), transparent(color)),
        ])
    }

    /// Blur helper: color fades to transparent across both sides of the edge.
    pub fn blur(color: Color, radius: f32) -> Self {
        let r = radius.max(0.001);
        Self::bare(vec![
            Stop::new(-r, transparent(color)),
            Stop::new(-r, color),
            Stop::new(r, transparent(color)),
        ])
    }

    /// Set pattern (turns the style into a stroke laid out along the contour).
    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Set the color-domain [`Transfer`] warp on the stop-blend parameter.
    pub fn transfer(mut self, transfer: Transfer) -> Self {
        self.transfer = transfer;
        self
    }

    /// Replace the chain with a clipped band `[from, to]` of the current first
    /// color: transparent outside the band, antialiased at both edges.
    pub fn dist_range(mut self, from: f32, to: f32) -> Self {
        let c = self.stops.first().map_or(Color::WHITE, |s| s.start);
        self.stops = vec![
            Stop::new(from, transparent(c)),
            Stop::new(from, c),
            Stop::new(to, c),
            Stop::new(to, transparent(c)),
        ];
        self
    }

    /// Expand the profile outward: each stop moves away from distance 0.
    pub fn expand(mut self, amount: f32) -> Self {
        for s in &mut self.stops {
            s.dist += if s.dist < 0.0 { -amount } else { amount };
        }
        self
    }

    /// Style with no pattern.
    fn bare(stops: Vec<Stop>) -> Self {
        Self {
            stops,
            pattern: None,
            transfer: Transfer::Linear,
        }
    }

    /// Whether this style has active animations.
    pub fn is_animated(&self) -> bool {
        self.pattern.as_ref().is_some_and(|p| p.is_animated())
    }

    /// Whether this style fills the interior (no pattern, opaque innermost stop).
    pub fn is_fill(&self) -> bool {
        self.pattern.is_none()
            && self
                .stops
                .first()
                .is_some_and(|s| s.start.a > 0.0 || s.end.a > 0.0)
    }

    /// World-space extent this style draws beyond the shape boundary.
    ///
    /// Sizes cull/clip padding so a layer is never clipped early. For a `closed`
    /// (filled) shape only the outward stops count; the interior is fill, not
    /// overdraw. For an open stroke both sides of the curve lie outside the
    /// shape, so the larger magnitude bound applies. A pattern adds its half
    /// thickness.
    pub fn extent(&self, closed: bool) -> f32 {
        let pat = self.pattern.as_ref().map_or(0.0, |p| p.thickness * 0.5);
        let max_d = self.stops.iter().map(|s| s.dist).fold(0.0_f32, f32::max);
        let min_d = self.stops.iter().map(|s| s.dist).fold(0.0_f32, f32::min);
        if closed {
            max_d.max(0.0) + pat
        } else {
            max_d.max(-min_d).max(0.0) + pat
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_is_fill() {
        let s = Style::solid(Color::WHITE);
        assert!(s.is_fill());
    }

    #[test]
    fn stroke_is_not_fill() {
        let s = Style::stroke(Color::WHITE, Pattern::solid(2.0));
        assert!(!s.is_fill());
    }

    #[test]
    fn shadow_extent() {
        let s = Style::shadow(Color::BLACK, 10.0);
        // Closed shape: only the outward fade band counts.
        assert_eq!(s.extent(true), 10.0);
    }

    #[test]
    fn stroke_extent_uses_both_sides() {
        // An open stroke extends to both sides of the curve; extent is the half
        // thickness regardless of which sign bound is larger.
        let s = Style::stroke(Color::WHITE, Pattern::solid(4.0));
        assert_eq!(s.extent(false), 2.0);
    }
}
