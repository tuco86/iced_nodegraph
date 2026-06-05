//! Decomposition of node-graph style types into `iced_nodegraph_sdf` draw layers.
//!
//! Each visual element (edge, node, pin) is composited from several SDF draws
//! stacked front-to-back. This module is the single place that translates the
//! resolved style types ([`EdgeStyle`], [`NodeStyle`], [`PinStyle`] in their
//! `Resolved` form) into the flat list of [`iced_nodegraph_sdf::Style`] layers the
//! renderer pushes. The widget owns geometry (where pins are, how an edge
//! curves); this module owns appearance (which layers exist and their distance
//! bands).
//!
//! A color field is a [`ColorQuad`]: its four corners map directly onto the four
//! corners of an `iced_nodegraph_sdf::Style` (arc-length axis start->end crossed with the
//! distance axis near->far).
//!
//! Layer order matters: the first layer in a returned list is drawn closest to
//! the viewer (lowest SDF z-order), the last is deepest.

use iced::Color;
use iced_nodegraph_sdf::{Pattern, Style};

use crate::node_pin::PinDirection;

use super::color::ColorQuad;
use super::mode::Resolved;
use super::{EdgeStyle, NodeStyle, PinStyle};

/// Apply opacity to a color by multiplying its alpha channel.
pub(crate) fn color_with_opacity(c: Color, opacity: f32) -> Color {
    Color {
        a: c.a * opacity,
        ..c
    }
}

/// Resolve an edge color, falling back to the pin color when transparent.
fn resolve_edge_color(color: Color, pin_color: Color) -> Color {
    if color.a > 0.01 { color } else { pin_color }
}

/// Build a `Style` from a color quad over a distance band, premultiplying alpha.
fn quad_style(
    q: &ColorQuad,
    dist_from: f32,
    dist_to: f32,
    pattern: Option<Pattern>,
    opacity: f32,
) -> Style {
    Style {
        near_start: color_with_opacity(q.near_start, opacity),
        near_end: color_with_opacity(q.near_end, opacity),
        far_start: color_with_opacity(q.far_start, opacity),
        far_end: color_with_opacity(q.far_end, opacity),
        dist_from,
        dist_to,
        pattern,
        distance_field: false,
    }
}

/// Stroke style from a quad: distance band is the pattern half-thickness.
fn quad_stroke(q: &ColorQuad, pattern: Pattern, opacity: f32) -> Style {
    let ht = pattern.thickness * 0.5;
    quad_style(q, -ht, ht, Some(pattern), opacity)
}

/// Replace transparent quad corners with the connected pin color, per arc side.
fn quad_resolve_pins(q: &ColorQuad, start_pin: Color, end_pin: Color) -> ColorQuad {
    ColorQuad {
        near_start: resolve_edge_color(q.near_start, start_pin),
        far_start: resolve_edge_color(q.far_start, start_pin),
        near_end: resolve_edge_color(q.near_end, end_pin),
        far_end: resolve_edge_color(q.far_end, end_pin),
    }
}

/// Which geometry an edge layer is drawn on.
///
/// An edge is built from one stroke drawable plus a possibly-offset shadow
/// drawable; each layer names which of the two it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeGeometry {
    /// The edge stroke path.
    Stroke,
    /// The shadow path (stroke shifted by the shadow offset).
    Shadow,
}

/// A single SDF draw making up an edge: which geometry, what style.
#[derive(Debug, Clone)]
pub(crate) struct EdgeLayer {
    pub geometry: EdgeGeometry,
    pub style: Style,
}

impl NodeStyle<Resolved> {
    /// Solid fill layer for the node body, premultiplied by `opacity`.
    pub(crate) fn fill_sdf_style(&self, opacity: f32) -> Style {
        quad_style(&self.fill_color, -1e6, 0.0, None, opacity)
    }

    /// Border layers, front-to-back: main stroke then optional outline halo.
    /// Empty when the border pattern thickness is zero.
    ///
    /// The border sits on the OUTSIDE of the node silhouette so it never eats
    /// into the body or content: a solid border is a plain outward band
    /// `[0, bw]` (the renderer honours distance bands for unpatterned styles),
    /// drawn flush against the body edge. A patterned border keeps its
    /// dash/dot layout centered on the contour, since the pattern's cross
    /// section is fixed to the silhouette.
    pub(crate) fn border_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let mut layers = Vec::new();
        let bw = self.border_pattern.thickness;
        if bw > 0.0 {
            if self.border_pattern.is_solid() {
                layers.push(quad_style(&self.border_color, 0.0, bw, None, opacity));
            } else {
                layers.push(quad_stroke(
                    &self.border_color,
                    self.border_pattern,
                    opacity,
                ));
            }
            if self.border_outline_width > 0.0 {
                let ow = self.border_outline_width;
                layers.push(quad_style(
                    &self.border_outline_color,
                    0.0,
                    bw + ow * 2.0,
                    None,
                    opacity,
                ));
            }
        }
        layers
    }

    /// Whether the shadow is visible (alpha and distance both nonzero).
    pub(crate) fn has_shadow(&self) -> bool {
        self.shadow_distance > 0.0 && self.shadow_color.a > 0.0
    }

    /// Shadow as three distance bands over the node's (offset) SDF silhouette.
    ///
    /// Distance is negative inside the shape, positive outside. The interior is
    /// full shadow; a ramp blurs across the edge (alpha 1.0 just inside -> 0.5
    /// at the boundary -> 0.0 just outside), so the node reads as floating with
    /// a crisp-yet-soft edge that follows pin cutouts. Only `shadow_color`'s
    /// alpha sets the strength; the bands modulate it. `shadow_distance` is the
    /// blur half-width. Bands are coplanar and tile the distance axis without
    /// overlap, meeting at matching alpha so the composite is continuous.
    pub(crate) fn shadow_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let d = self.shadow_distance.max(0.001);
        let base = self.shadow_color;
        let alpha = base.a * opacity;
        // `shadow_color` scaled to a fraction of the full shadow alpha.
        let shade = |mul: f32| Color {
            a: alpha * mul,
            ..base
        };
        // A flat-color band: `near` at `from`, `far` at `to` (see Style axes).
        let band = |near_mul: f32, far_mul: f32, from: f32, to: f32| Style {
            near_start: shade(near_mul),
            near_end: shade(near_mul),
            far_start: shade(far_mul),
            far_end: shade(far_mul),
            dist_from: from,
            dist_to: to,
            pattern: None,
            distance_field: false,
        };
        vec![
            band(0.5, 0.0, 0.0, d),   // outside edge: 0.5 -> 0
            band(1.0, 0.5, -d, 0.0),  // inside edge: 1.0 -> 0.5
            band(1.0, 1.0, -1e6, -d), // interior: full shadow
        ]
    }
}

impl EdgeStyle<Resolved> {
    /// Decompose into SDF layers front-to-back: stroke, optional stroke outline,
    /// optional border (ring, outline, background), then shadow deepest.
    ///
    /// Colors are in arc-length order (`start_color` at arc 0, `end_color` at arc
    /// 1); transparent quad corners inherit the given pin colors. No reversal: the
    /// caller lays the edge out in the intended direction, so gradient, arrow
    /// pattern and flow all follow the arc-length as-is.
    pub(crate) fn sdf_layers(&self, start_color: Color, end_color: Color) -> Vec<EdgeLayer> {
        let mut layers = Vec::with_capacity(6);

        // Stroke (front).
        let stroke_q = quad_resolve_pins(&self.stroke_color, start_color, end_color);
        layers.push(EdgeLayer {
            geometry: EdgeGeometry::Stroke,
            style: quad_stroke(&stroke_q, self.pattern, 1.0),
        });

        // Stroke outline (halo behind the stroke).
        if self.stroke_outline_width > 0.0
            && (self.stroke_outline_color.near_start.a > 0.0
                || self.stroke_outline_color.near_end.a > 0.0)
        {
            let outline_pat =
                Pattern::solid(self.pattern.thickness + self.stroke_outline_width * 2.0);
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Stroke,
                style: quad_stroke(&self.stroke_outline_color, outline_pat, 1.0),
            });
        }

        // Border ring (behind stroke).
        if self.border_width > 0.0 {
            let border_center =
                self.pattern.thickness * 0.5 + self.border_gap + self.border_width * 0.5;
            let border_outer = border_center + self.border_width * 0.5;

            let bq = quad_resolve_pins(&self.border_color, start_color, end_color);
            let mut border_style = quad_stroke(&bq, Pattern::solid(self.border_width), 1.0);
            border_style.dist_from = -border_outer;
            border_style.dist_to = -border_center + self.border_width * 0.5;
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Stroke,
                style: border_style,
            });

            if self.border_outline_width > 0.0 {
                let outline_pat =
                    Pattern::solid(self.border_width + self.border_outline_width * 2.0);
                let mut outline_style = quad_stroke(&self.border_outline_color, outline_pat, 1.0);
                outline_style.dist_from = -border_outer - self.border_outline_width;
                outline_style.dist_to =
                    -border_center + self.border_width * 0.5 + self.border_outline_width;
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: outline_style,
                });
            }

            // Border background fill (behind the border ring).
            if self.border_background.near_start.a > 0.0 || self.border_background.near_end.a > 0.0
            {
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: quad_style(&self.border_background, -1e6, border_outer, None, 1.0),
                });
            }
        }

        // Shadow (deepest).
        if self.shadow_blur > 0.0
            && (self.shadow_color.near_start.a > 0.0 || self.shadow_color.near_end.a > 0.0)
        {
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Shadow,
                style: quad_style(
                    &self.shadow_color,
                    -self.shadow_expand,
                    self.shadow_expand + self.shadow_blur.max(0.001),
                    None,
                    1.0,
                ),
            });
        }

        layers
    }
}

impl PinStyle<Resolved> {
    /// SDF layers for a pin indicator, front-to-back: fill then optional border.
    /// `indicator_r` is the drawn radius (the widget may scale it for pulses).
    pub(crate) fn sdf_layers(&self, direction: PinDirection, indicator_r: f32) -> Vec<Style> {
        let mut layers = Vec::with_capacity(2);
        let fill = if direction == PinDirection::Input {
            // Hollow ring for inputs.
            quad_stroke(&self.color, Pattern::solid(indicator_r * 0.8), 1.0)
        } else {
            quad_style(&self.color, -1e6, 0.0, None, 1.0)
        };
        layers.push(fill);

        if self.border_width > 0.0
            && (self.border_color.near_start.a > 0.0 || self.border_color.near_end.a > 0.0)
        {
            layers.push(
                quad_style(&self.border_color, -1e6, 0.0, None, 1.0).expand(self.border_width),
            );
        }
        layers
    }
}
