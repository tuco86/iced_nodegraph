//! Graph Configuration Node
//!
//! Builds a [`GraphOverlay`] from individual field inputs: the canvas background
//! color plus the optional [`iced_nodegraph::TilingBackground`] (kind, spacing,
//! thickness, color). Unlike the per-target config nodes it has no inheritance
//! input; there is a single canvas, so the overlay flows straight into the
//! Catalog node's `graph` pin. Color inputs are `ColorQuad`s (the near corner
//! is taken, since the canvas fields are plain `Color`).

use demo_common::NodeContentStyle;
use iced::{
    Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, TilingKind, pin};

use crate::nodes::{color_swatch, fmt_float, node_title_bar, pin_row, pins, value_display};
use crate::style_overlay::GraphOverlay;

/// Collected inputs for the Graph Config node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphConfigInputs {
    pub background_color: Option<ColorQuad>,
    pub tiling_kind: Option<TilingKind>,
    pub tiling_spacing: Option<f32>,
    pub tiling_thickness: Option<f32>,
    pub tiling_color: Option<ColorQuad>,
}

impl GraphConfigInputs {
    /// Builds the canvas overlay from this node's fields. Colors collapse the
    /// `ColorQuad` to its near corner, matching `GraphStyle`'s plain `Color`.
    pub fn build(&self) -> GraphOverlay {
        let mut g = GraphOverlay::new();
        if let Some(c) = self.background_color {
            g = g.background_color(c.near_start);
        }
        if let Some(k) = self.tiling_kind {
            g = g.tiling_kind(k);
        }
        if let Some(s) = self.tiling_spacing {
            g = g.tiling_spacing(s);
        }
        if let Some(t) = self.tiling_thickness {
            g = g.tiling_thickness(t);
        }
        if let Some(c) = self.tiling_color {
            g = g.tiling_color(c.near_start);
        }
        g
    }
}

/// Display label for a tiling kind, or "--" when unset (inherit).
fn tiling_kind_label(kind: Option<TilingKind>) -> &'static str {
    match kind {
        Some(TilingKind::Grid) => "grid",
        Some(TilingKind::Dots) => "dots",
        Some(TilingKind::Triangles) => "triangles",
        Some(TilingKind::Hex) => "hex",
        None => "--",
    }
}

/// Creates a Graph Config node with the canvas background and tiling field inputs.
pub fn graph_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &GraphConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    // Output-only row: a single canvas means no inheritance input.
    let out_row = row![
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::GRAPH_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::GraphConfigData>()
        ),
    ]
    .align_y(iced::Alignment::Center);

    let background_row = pin_row(
        pin!(
            Left,
            pins::graph::BACKGROUND,
            text("background").size(10),
            Input,
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
        color_swatch(inputs.background_color.map(|q| q.near_start)),
    );

    let kind_row = pin_row(
        pin!(
            Left,
            pins::graph::TILING_KIND,
            text("tiling").size(10),
            Input,
            ::std::any::TypeId::of::<pins::TilingKindData>()
        ),
        value_display(tiling_kind_label(inputs.tiling_kind)),
    );

    let spacing_row = pin_row(
        pin!(
            Left,
            pins::graph::SPACING,
            text("spacing").size(10),
            Input,
            ::std::any::TypeId::of::<pins::Float>()
        ),
        value_display(fmt_float(inputs.tiling_spacing, 1)),
    );

    let thickness_row = pin_row(
        pin!(
            Left,
            pins::graph::THICKNESS,
            text("thickness").size(10),
            Input,
            ::std::any::TypeId::of::<pins::Float>()
        ),
        value_display(fmt_float(inputs.tiling_thickness, 1)),
    );

    let line_color_row = pin_row(
        pin!(
            Left,
            pins::graph::LINE_COLOR,
            text("line color").size(10),
            Input,
            ::std::any::TypeId::of::<pins::ColorData>()
        ),
        color_swatch(inputs.tiling_color.map(|q| q.near_start)),
    );

    let content = column![
        out_row,
        background_row,
        kind_row,
        spacing_row,
        thickness_row,
        line_color_row,
    ]
    .spacing(4);

    column![
        node_title_bar("Graph Config", style),
        container(content).padding([8, 10])
    ]
    .width(170.0)
    .into()
}
