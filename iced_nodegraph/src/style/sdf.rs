//! Decomposition of node-graph style types into `iced_sdf` draw layers.
//!
//! Each visual element (edge, node, pin) is composited from several SDF draws
//! stacked front-to-back. This module is the single place that translates the
//! high-level style types ([`EdgeStyle`], [`NodeStyle`], [`PinStyle`]) into the
//! flat list of [`iced_sdf::Style`] layers the renderer pushes. The widget owns
//! geometry (where pins are, how an edge curves); this module owns appearance
//! (which layers exist and their distance bands).
//!
//! Layer order matters: the first layer in a returned list is drawn closest to
//! the viewer (lowest SDF z-order), the last is deepest.

use iced::Color;
use iced_sdf::{Pattern, Style};

use crate::node_pin::PinDirection;

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

/// Build a soft shadow style with optional gradient: opaque at the boundary,
/// fading to transparent over a band of width `expand + blur` outside the shape.
fn shadow_style(c0: Color, c1: Color, expand: f32, blur: f32) -> Style {
    let t0 = Color { a: 0.0, ..c0 };
    let t1 = Color { a: 0.0, ..c1 };
    Style {
        near_start: c0,
        near_end: c1,
        far_start: t0,
        far_end: t1,
        dist_from: -expand,
        dist_to: expand + blur.max(0.001),
        pattern: None,
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

impl EdgeStyle {
    /// Decompose this edge style into SDF layers, front-to-back: stroke,
    /// optional stroke outline, optional border (ring, outline, background),
    /// then the shadow deepest.
    ///
    /// `start_*`/`end_*` describe the two endpoints; transparent edge colors
    /// inherit the pin color, and an Input->Output edge is drawn reversed so the
    /// gradient and flow animation always run source-to-target.
    pub(crate) fn sdf_layers(
        &self,
        start_pin_color: Color,
        end_pin_color: Color,
        start_direction: PinDirection,
        end_direction: PinDirection,
    ) -> Vec<EdgeLayer> {
        let is_reversed = matches!(
            (start_direction, end_direction),
            (PinDirection::Input, PinDirection::Output)
        );

        let mut layers = Vec::with_capacity(6);

        // Stroke (front).
        let stroke_start = resolve_edge_color(self.start_color, start_pin_color);
        let stroke_end = resolve_edge_color(self.end_color, end_pin_color);
        let (c0, c1) = if is_reversed {
            (stroke_end, stroke_start)
        } else {
            (stroke_start, stroke_end)
        };
        let pattern = if is_reversed && self.pattern.flow_speed.abs() > 0.001 {
            let mut p = self.pattern;
            p.flow_speed = -p.flow_speed;
            p
        } else {
            self.pattern
        };
        layers.push(EdgeLayer {
            geometry: EdgeGeometry::Stroke,
            style: Style::arc_gradient_stroke(c0, c1, pattern),
        });

        // Stroke outline (behind the stroke, visible as a halo).
        if let Some((w, c)) = self.stroke_outline
            && w > 0.0
            && c.a > 0.0
        {
            let outline_pat = Pattern::solid(pattern.thickness + w * 2.0);
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Stroke,
                style: Style::stroke(c, outline_pat),
            });
        }

        // Border ring (behind stroke).
        if let Some(border) = &self.border
            && border.width > 0.0
        {
            let border_center = self.pattern.thickness * 0.5 + border.gap + border.width * 0.5;
            let border_outer = border_center + border.width * 0.5;

            let border_start = resolve_edge_color(border.start_color, start_pin_color);
            let border_end = resolve_edge_color(border.end_color, end_pin_color);
            let (c0, c1) = if is_reversed {
                (border_end, border_start)
            } else {
                (border_start, border_end)
            };

            let mut border_style = Style::arc_gradient_stroke(c0, c1, Pattern::solid(border.width));
            border_style.dist_from = -border_outer;
            border_style.dist_to = -border_center + border.width * 0.5;
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Stroke,
                style: border_style,
            });

            if let Some((w, oc)) = border.outline
                && w > 0.0
                && oc.a > 0.0
            {
                let outline_pat = Pattern::solid(border.width + w * 2.0);
                let mut outline_style = Style::stroke(oc, outline_pat);
                outline_style.dist_from = -border_outer - w;
                outline_style.dist_to = -border_center + (border.width * 0.5) + w;
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: outline_style,
                });
            }

            // Border background fill (behind the border stroke).
            if border.background.a > 0.0 || border.background_end.a > 0.0 {
                let (bg0, bg1) = if is_reversed {
                    (border.background_end, border.background)
                } else {
                    (border.background, border.background_end)
                };
                let mut bg_style = Style::arc_gradient(bg0, bg1);
                bg_style.dist_to = border_outer;
                layers.push(EdgeLayer {
                    geometry: EdgeGeometry::Stroke,
                    style: bg_style,
                });
            }
        }

        // Shadow (deepest).
        if let Some(shadow) = &self.shadow
            && (shadow.color.a > 0.0 || shadow.end_color.a > 0.0)
        {
            layers.push(EdgeLayer {
                geometry: EdgeGeometry::Shadow,
                style: shadow_style(shadow.color, shadow.end_color, shadow.expand, shadow.blur),
            });
        }

        layers
    }
}

impl NodeStyle {
    /// Solid fill layer for the node body, premultiplied by `opacity`.
    pub(crate) fn fill_sdf_style(&self, opacity: f32) -> Style {
        Style::solid(color_with_opacity(self.fill_color, opacity))
    }

    /// Border layers, front-to-back: main stroke then optional outline halo.
    /// Empty when there is no border or its width is zero.
    pub(crate) fn border_sdf_layers(&self, opacity: f32) -> Vec<Style> {
        let mut layers = Vec::new();
        if let Some(border) = &self.border {
            let bw = border.pattern.thickness;
            if bw > 0.0 {
                layers.push(Style::stroke(
                    color_with_opacity(border.color, opacity),
                    border.pattern,
                ));
                if let Some((ow, oc)) = border.outline
                    && ow > 0.0
                    && oc.a > 0.0
                {
                    layers.push(Style::stroke(
                        color_with_opacity(oc, opacity),
                        Pattern::solid(bw + ow * 2.0),
                    ));
                }
            }
        }
        layers
    }
}

impl PinStyle {
    /// SDF layers for a pin indicator, front-to-back: fill then optional border
    /// ring. `pin_color` is the per-pin color, `indicator_r` the drawn radius.
    pub(crate) fn sdf_layers(
        &self,
        pin_color: Color,
        direction: PinDirection,
        indicator_r: f32,
    ) -> Vec<Style> {
        let mut layers = Vec::with_capacity(2);
        let fill = if direction == PinDirection::Input {
            // Hollow ring for inputs.
            Style::stroke(pin_color, Pattern::solid(indicator_r * 0.8))
        } else {
            Style::solid(pin_color)
        };
        layers.push(fill);

        let border_color = self.border_color.unwrap_or(Color::TRANSPARENT);
        if border_color.a > 0.0 && self.border_width > 0.0 {
            layers.push(Style::solid(border_color).expand(self.border_width));
        }
        layers
    }
}
