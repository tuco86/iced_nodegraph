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
//! A color field is a [`ColorQuad`](super::ColorQuad); the band/stroke chains are
//! built by [`Style::quad_band`] and [`Style::quad_stroke`] in the SDF crate,
//! which consume a `ColorQuad` directly. This module only chooses distance
//! ranges and premultiplies node opacity onto the quad via
//! [`ColorQuad::with_opacity`](super::ColorQuad::with_opacity).
//!
//! Layer order matters: the first layer in a returned list is drawn closest to
//! the viewer (lowest SDF z-order), the last is deepest.

use iced::Color;
use iced_nodegraph_sdf::{Pattern, Stop, Style};

use crate::node_pin::PinDirection;

use super::{EdgeStyle, NodeStyle, PinStyle};

/// Same color with zero alpha.
fn transparent(c: Color) -> Color {
    Color { a: 0.0, ..c }
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
        Style::quad_band(&self.fill_color.with_opacity(opacity), -1e6, 0.0)
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
            let mut layers = vec![Style::quad_stroke(
                &self.border_color.with_opacity(opacity),
                self.border_pattern,
            )];
            if has_outline {
                let ow = self.border_outline_width;
                layers.push(Style::quad_band(
                    &self.border_outline_color.with_opacity(opacity),
                    0.0,
                    bw + ow * 2.0,
                ));
            }
            return layers;
        }

        // Solid border + outline as one outward chain on the silhouette.
        let (bs, be) = self.border_color.with_opacity(opacity).arc_pair();
        let mut stops = vec![
            Stop::grad(0.0, transparent(bs), transparent(be)),
            Stop::grad(0.0, bs, be),
        ];
        if has_outline {
            let outer = bw + self.border_outline_width * 2.0;
            let (os, oe) = self.border_outline_color.with_opacity(opacity).arc_pair();
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
            transfer: Default::default(),
        }]
    }

    /// Whether the shadow is visible (alpha and distance both nonzero).
    pub(crate) fn has_shadow(&self) -> bool {
        self.shadow_distance > 0.0 && self.shadow_color.a > 0.0
    }

    /// Shadow as a single stop chain over the node's (offset) SDF silhouette.
    ///
    /// Distance is negative inside the shape, positive outside. A solid core is
    /// held full below `-shadow_distance`; from there the colour gradients to
    /// transparent at `+shadow_distance`, so the blur is centred on the
    /// silhouette and spreads BOTH inward and outward (the edge reads ~50% at the
    /// silhouette, a soft blur rather than a hard core boundary). The solid core
    /// still reads through a translucent body and where an offset shadow peeks
    /// out. Being one entry, it has no internal boundary to double-antialias -
    /// the earlier multi-band tiling produced light seams where two coplanar
    /// same-color bands each half-antialiased a shared boundary and premultiplied
    /// compositing dipped below full alpha. Only `shadow_color`'s alpha sets the
    /// strength; `shadow_distance` is the half-fade width.
    pub(crate) fn shadow_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let d = self.shadow_distance.max(0.001);
        let full = Color {
            a: self.shadow_color.a * opacity,
            ..self.shadow_color
        };
        let none = transparent(full);
        // Solid core held below `-d`; gradient to transparent across `[-d, d]`,
        // so the blur spreads inward and outward from the silhouette.
        vec![Style {
            stops: vec![Stop::new(-d, full), Stop::new(d, none)],
            pattern: None,
            distance_field: false,
            transfer: Default::default(),
        }]
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::NodeStyle;

    /// The shadow is one chain (no separate composited bands to seam): a solid
    /// core held below `-shadow_distance`, gradient to transparent at
    /// `+shadow_distance`, centred on the silhouette.
    #[test]
    fn shadow_fills_interior_and_fades_out() {
        let style = NodeStyle::input();
        let layers = style.shadow_sdf_layers(1.0);

        assert_eq!(layers.len(), 1, "shadow must be a single entry");
        let stops = &layers[0].stops;
        assert_eq!(stops.len(), 2, "solid core -> transparent outside");
        assert_eq!(
            stops[0].dist, -style.shadow_distance,
            "gradient starts at -shadow_distance (solid core held below it)",
        );
        assert_eq!(
            stops[0].start.a, style.shadow_color.a,
            "full at the inner edge of the gradient",
        );
        assert_eq!(
            stops[1].dist, style.shadow_distance,
            "fades out at +shadow_distance"
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
            style: Style::quad_stroke(&self.stroke_color, self.pattern),
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
                style: Style::quad_stroke(&self.stroke_outline_color, outline_pat),
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
                style: Style::quad_stroke(&self.border_color, Pattern::solid(self.border_width)),
            });

            if self.border_outline_width > 0.0 {
                let outline_pat =
                    Pattern::solid(self.border_width + self.border_outline_width * 2.0);
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: Style::quad_stroke(&self.border_outline_color, outline_pat),
                });
            }

            // Border background fill (behind the border ring).
            if self.border_background.near_start.a > 0.0 || self.border_background.near_end.a > 0.0
            {
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: Style::quad_band(&self.border_background, -1e6, border_outer),
                });
            }
        }

        // Shadow (deepest): a soft blur band across the stroke edge.
        if self.shadow_blur > 0.0
            && (self.shadow_color.near_start.a > 0.0 || self.shadow_color.near_end.a > 0.0)
        {
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Shadow,
                style: Style::quad_band(
                    &self.shadow_color,
                    -self.shadow_expand,
                    self.shadow_expand + self.shadow_blur.max(0.001),
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
            Style::quad_stroke(&self.color, Pattern::solid(indicator_r * 0.8))
        } else {
            Style::quad_band(&self.color, -1e6, 0.0)
        };
        layers.push(fill);

        if self.border_width > 0.0
            && (self.border_color.near_start.a > 0.0 || self.border_color.near_end.a > 0.0)
        {
            layers.push(Style::quad_band(&self.border_color, -1e6, 0.0).expand(self.border_width));
        }
        layers
    }
}
