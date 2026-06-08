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
//! A color field is a [`ColorQuad`]; a draw layer is an `iced_nodegraph_sdf::Style`
//! whose distance profile is a chain of stops (a continuous cross-section
//! evaluated in one pass), so an effect's bands never composite against each
//! other and cannot seam at a shared boundary.
//!
//! Layer order matters: the first layer in a returned list is drawn closest to
//! the viewer (lowest SDF z-order), the last is deepest.

use iced::Color;
use iced_nodegraph_sdf::{Pattern, Stop, Style};

use crate::node_pin::PinDirection;

use super::color::ColorQuad;
use super::{EdgeStyle, NodeStyle, PinStyle};

/// Apply opacity to a color by multiplying its alpha channel.
pub(crate) fn color_with_opacity(c: Color, opacity: f32) -> Color {
    Color {
        a: c.a * opacity,
        ..c
    }
}

/// Same color with zero alpha.
fn transparent(c: Color) -> Color {
    Color { a: 0.0, ..c }
}

/// A quad's arc-color pair (`start` at arc 0, `end` at arc 1) at its near edge.
fn quad_arc(q: &ColorQuad, opacity: f32) -> (Color, Color) {
    (
        color_with_opacity(q.near_start, opacity),
        color_with_opacity(q.near_end, opacity),
    )
}

/// A clipped color band over `[from, to]`: transparent outside, the quad's near
/// colors at `from` and far colors at `to`, antialiased at both edges. Built as
/// a four-stop chain so it is one self-contained entry (no inter-band seam).
fn quad_band(q: &ColorQuad, from: f32, to: f32, opacity: f32) -> Style {
    let ns = color_with_opacity(q.near_start, opacity);
    let ne = color_with_opacity(q.near_end, opacity);
    let fs = color_with_opacity(q.far_start, opacity);
    let fe = color_with_opacity(q.far_end, opacity);
    Style {
        stops: vec![
            Stop::grad(from, transparent(ns), transparent(ne)),
            Stop::grad(from, ns, ne),
            Stop::grad(to, fs, fe),
            Stop::grad(to, transparent(fs), transparent(fe)),
        ],
        pattern: None,
        distance_field: false,
    }
}

/// Stroke style from a quad: the pattern lays the stroke out along the contour.
fn quad_stroke(q: &ColorQuad, pattern: Pattern, opacity: f32) -> Style {
    let (start, end) = quad_arc(q, opacity);
    Style {
        stops: vec![Stop::grad(0.0, start, end)],
        pattern: Some(pattern),
        distance_field: false,
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

impl NodeStyle {
    /// Solid fill layer for the node body, premultiplied by `opacity`.
    pub(crate) fn fill_sdf_style(&self, opacity: f32) -> Style {
        quad_band(&self.fill_color, -1e6, 0.0, opacity)
    }

    /// Border layers, front-to-back. Empty when the border pattern thickness is
    /// zero.
    ///
    /// The border sits on the OUTSIDE of the node silhouette so it never eats
    /// into the body or content. A solid border plus its outline form one
    /// continuous outward profile, so they are emitted as a single stop chain
    /// (`border` over `[0, bw]`, `outline` over `[bw, bw + 2*ow]`): one entry,
    /// no inter-band seam. A patterned border keeps its dash/dot layout centered
    /// on the contour, so it stays a pattern stroke with the outline behind it.
    pub(crate) fn border_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let bw = self.border_pattern.thickness;
        if bw <= 0.0 {
            return Vec::new();
        }
        let has_outline = self.border_outline_width > 0.0;

        if !self.border_pattern.is_solid() {
            // Patterned border: dashed/dotted stroke centered on the contour,
            // with a solid outline band behind it.
            let mut layers = vec![quad_stroke(
                &self.border_color,
                self.border_pattern,
                opacity,
            )];
            if has_outline {
                let ow = self.border_outline_width;
                layers.push(quad_band(
                    &self.border_outline_color,
                    0.0,
                    bw + ow * 2.0,
                    opacity,
                ));
            }
            return layers;
        }

        // Solid border + outline as one outward chain on the silhouette.
        let (bs, be) = quad_arc(&self.border_color, opacity);
        let mut stops = vec![
            Stop::grad(0.0, transparent(bs), transparent(be)),
            Stop::grad(0.0, bs, be),
        ];
        if has_outline {
            let outer = bw + self.border_outline_width * 2.0;
            let (os, oe) = quad_arc(&self.border_outline_color, opacity);
            stops.push(Stop::grad(bw, bs, be));
            stops.push(Stop::grad(bw, os, oe));
            stops.push(Stop::grad(outer, os, oe));
            stops.push(Stop::grad(outer, transparent(os), transparent(oe)));
        } else {
            stops.push(Stop::grad(bw, bs, be));
            stops.push(Stop::grad(bw, transparent(bs), transparent(be)));
        }
        vec![Style {
            stops,
            pattern: None,
            distance_field: false,
        }]
    }

    /// Whether the shadow is visible (alpha and distance both nonzero).
    pub(crate) fn has_shadow(&self) -> bool {
        self.shadow_distance > 0.0 && self.shadow_color.a > 0.0
    }

    /// Shadow as a single stop chain over the node's (offset) SDF silhouette.
    ///
    /// Distance is negative inside the shape, positive outside. The interior is
    /// held at full shadow (so a translucent body floats over a solid core, and
    /// an offset shadow reads solid where it peeks out), fading to transparent
    /// at `shadow_distance`. Being one entry, it has no internal boundary to
    /// double-antialias - the earlier multi-band tiling produced light seams
    /// where two coplanar same-color bands each half-antialiased a shared
    /// boundary and premultiplied compositing dipped below full alpha. Only
    /// `shadow_color`'s alpha sets the strength; `shadow_distance` is the fade
    /// width.
    pub(crate) fn shadow_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let d = self.shadow_distance.max(0.001);
        let full = Color {
            a: self.shadow_color.a * opacity,
            ..self.shadow_color
        };
        let none = transparent(full);
        // Full inside (held below the first stop), fading to transparent at `d`.
        vec![Style {
            stops: vec![Stop::new(0.0, full), Stop::new(d, none)],
            pattern: None,
            distance_field: false,
        }]
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::NodeStyle;

    /// The shadow is one chain (no separate composited bands to seam): full
    /// inside the silhouette (stop 0, held below), transparent at `d`.
    #[test]
    fn shadow_fills_interior_and_fades_out() {
        let style = NodeStyle::input();
        let layers = style.shadow_sdf_layers(1.0);

        assert_eq!(layers.len(), 1, "shadow must be a single entry");
        let stops = &layers[0].stops;
        assert_eq!(stops.len(), 2, "full inside -> transparent outside");
        assert_eq!(stops[0].dist, 0.0);
        assert_eq!(
            stops[0].start.a, style.shadow_color.a,
            "full at and inside the silhouette",
        );
        assert_eq!(
            stops[1].dist, style.shadow_distance,
            "fades out at distance"
        );
        assert_eq!(stops[1].start.a, 0.0, "transparent at the outer edge");
    }
}

impl EdgeStyle {
    /// Decompose into SDF layers front-to-back: stroke, optional stroke outline,
    /// optional border (ring, outline, background), then shadow deepest.
    ///
    /// Colors are in arc-length order (`stroke_color.near_start` at arc 0,
    /// `near_end` at arc 1). No reversal: the caller lays the edge out in the
    /// intended direction, so gradient, arrow pattern and flow all follow the
    /// arc-length as-is.
    pub(crate) fn sdf_layers(&self) -> Vec<EdgeLayer> {
        let mut layers = Vec::with_capacity(6);

        // Stroke (front).
        layers.push(EdgeLayer {
            geometry: EdgeGeometry::Stroke,
            style: quad_stroke(&self.stroke_color, self.pattern, 1.0),
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

        // Border ring (behind stroke): a solid pattern stroke centered on the
        // contour, thickness `border_width`.
        if self.border_width > 0.0 {
            let border_center =
                self.pattern.thickness * 0.5 + self.border_gap + self.border_width * 0.5;
            let border_outer = border_center + self.border_width * 0.5;

            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Stroke,
                style: quad_stroke(&self.border_color, Pattern::solid(self.border_width), 1.0),
            });

            if self.border_outline_width > 0.0 {
                let outline_pat =
                    Pattern::solid(self.border_width + self.border_outline_width * 2.0);
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: quad_stroke(&self.border_outline_color, outline_pat, 1.0),
                });
            }

            // Border background fill (behind the border ring).
            if self.border_background.near_start.a > 0.0 || self.border_background.near_end.a > 0.0
            {
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: quad_band(&self.border_background, -1e6, border_outer, 1.0),
                });
            }
        }

        // Shadow (deepest): a soft blur band across the stroke edge.
        if self.shadow_blur > 0.0
            && (self.shadow_color.near_start.a > 0.0 || self.shadow_color.near_end.a > 0.0)
        {
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Shadow,
                style: quad_band(
                    &self.shadow_color,
                    -self.shadow_expand,
                    self.shadow_expand + self.shadow_blur.max(0.001),
                    1.0,
                ),
            });
        }

        layers
    }
}

impl PinStyle {
    /// SDF layers for a pin indicator, front-to-back: fill then optional border.
    /// `indicator_r` is the drawn radius (the widget may scale it for pulses).
    pub(crate) fn sdf_layers(&self, direction: PinDirection, indicator_r: f32) -> Vec<Style> {
        let mut layers = Vec::with_capacity(2);
        let fill = if direction == PinDirection::Input {
            // Hollow ring for inputs.
            quad_stroke(&self.color, Pattern::solid(indicator_r * 0.8), 1.0)
        } else {
            quad_band(&self.color, -1e6, 0.0, 1.0)
        };
        layers.push(fill);

        if self.border_width > 0.0
            && (self.border_color.near_start.a > 0.0 || self.border_color.near_end.a > 0.0)
        {
            layers.push(quad_band(&self.border_color, -1e6, 0.0, 1.0).expand(self.border_width));
        }
        layers
    }
}
