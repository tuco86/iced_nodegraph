//! Edge Configuration Node
//!
//! Builds an EdgeConfig from individual field inputs with inheritance support.
//! Each color is a single `ColorQuad` pin (the start/end gradient is encoded in
//! the quad), and the shadow offset is a single 2D vector pin.

use demo_common::NodeContentStyle;
use iced::{
    Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{ColorQuad, EdgeCurve, Pattern, pin};

use crate::nodes::{
    collapsed_pin_row, color_swatch, fmt_float, node_title_bar, pin_row, pins, push_section,
    value_display,
};
use crate::style_overlay::EdgeOverlay;

/// Section expansion state for EdgeConfig nodes
#[derive(Debug, Clone, Default)]
pub struct EdgeSections {
    pub stroke: bool,
    pub pattern: bool,
    pub border: bool,
    pub shadow: bool,
}

impl EdgeSections {
    pub fn new_all_expanded() -> Self {
        Self {
            stroke: true,
            pattern: true,
            border: true,
            shadow: true,
        }
    }
}

/// Identifies which section to toggle in EdgeConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSection {
    Stroke,
    Pattern,
    Border,
    Shadow,
}

/// Pattern type for simple selection (maps to iced_nodegraph_sdf::Pattern)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternType {
    #[default]
    Solid,
    /// Dashes with configurable angle (0 = perpendicular caps)
    Dashed,
    /// Arrow-like marks (///) crossing the edge
    Arrowed,
    Dotted,
    DashDotted,
}

/// Collected inputs for EdgeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<EdgeOverlay>,

    // --- Stroke ---
    pub stroke_color: Option<ColorQuad>,
    pub thickness: Option<f32>,
    pub curve: Option<EdgeCurve>,
    pub stroke_outline_width: Option<f32>,
    pub stroke_outline_color: Option<ColorQuad>,

    // --- Pattern ---
    pub pattern_type: Option<PatternType>,
    pub dash_length: Option<f32>,
    pub gap_length: Option<f32>,
    pub pattern_angle: Option<f32>,
    pub dot_radius: Option<f32>,
    pub animation_speed: Option<f32>,

    // --- Border ---
    pub border_width: Option<f32>,
    pub border_gap: Option<f32>,
    pub border_color: Option<ColorQuad>,
    pub border_background: Option<ColorQuad>,
    pub border_outline_width: Option<f32>,
    pub border_outline_color: Option<ColorQuad>,

    // --- Shadow ---
    pub shadow_expand: Option<f32>,
    pub shadow_blur: Option<f32>,
    pub shadow_color: Option<ColorQuad>,
    pub shadow_offset: Option<(f32, f32)>,
}

impl EdgeConfigInputs {
    /// Builds the overlay by setting this node's fields, then merging over the parent.
    pub fn build(&self) -> EdgeOverlay {
        let mut p = EdgeOverlay::new();

        if let Some(c) = self.stroke_color {
            p = p.stroke_color(c);
        }
        if let Some(pat) = self.build_pattern() {
            p = p.pattern(pat);
        }
        if let Some(c) = self.curve {
            p = p.curve(c);
        }
        // Stroke outline
        if let Some(w) = self.stroke_outline_width {
            p = p.stroke_outline_width(w);
        }
        if let Some(c) = self.stroke_outline_color {
            p = p.stroke_outline_color(c);
        }
        // Border ring
        if let Some(w) = self.border_width {
            p = p.border_width(w);
        }
        if let Some(g) = self.border_gap {
            p = p.border_gap(g);
        }
        if let Some(c) = self.border_color {
            p = p.border_color(c);
        }
        if let Some(c) = self.border_background {
            p = p.border_background(c);
        }
        if let Some(w) = self.border_outline_width {
            p = p.border_outline_width(w);
        }
        if let Some(c) = self.border_outline_color {
            p = p.border_outline_color(c);
        }
        // Shadow
        if let Some(c) = self.shadow_color {
            p = p.shadow_color(c);
        }
        if let Some(v) = self.shadow_expand {
            p = p.shadow_expand(v);
        }
        if let Some(v) = self.shadow_blur {
            p = p.shadow_blur(v);
        }
        if let Some(off) = self.shadow_offset {
            p = p.shadow_offset(off);
        }

        match &self.config_in {
            Some(parent) => p.merge(parent),
            None => p,
        }
    }

    /// Builds the Pattern from individual inputs, aligned with iced_nodegraph_sdf gallery.
    /// Returns None when no pattern field is set (inherit).
    fn build_pattern(&self) -> Option<Pattern> {
        let has_overrides = self.pattern_type.is_some()
            || self.thickness.is_some()
            || self.dash_length.is_some()
            || self.gap_length.is_some()
            || self.pattern_angle.is_some()
            || self.dot_radius.is_some()
            || self.animation_speed.is_some();

        if !has_overrides {
            return None;
        }

        let pattern_type = self.pattern_type.unwrap_or(PatternType::Solid);
        let thickness = self.thickness.unwrap_or(2.0);
        let dash = self.dash_length.unwrap_or(12.0);
        let gap = self.gap_length.unwrap_or(6.0);
        let angle = self.pattern_angle.unwrap_or(0.0);
        let dot_radius = self.dot_radius.unwrap_or(2.0);
        let speed = self.animation_speed.unwrap_or(0.0);

        let mut p = match pattern_type {
            PatternType::Solid => Pattern::solid(thickness),
            PatternType::Dashed => Pattern::dashed_angle(thickness, dash, gap, angle),
            PatternType::Arrowed => Pattern::arrowed_angle(thickness, dash, gap, angle),
            PatternType::Dotted => Pattern::dotted(gap + dot_radius * 2.0, dot_radius),
            PatternType::DashDotted => Pattern::dash_dotted(thickness, dash, gap, dot_radius),
        };

        if speed != 0.0 {
            p = p.flow(speed);
        }

        Some(p)
    }

    /// Returns the current pattern type
    pub fn get_pattern_type(&self) -> PatternType {
        self.pattern_type.unwrap_or(PatternType::Solid)
    }
}

/// Creates an EdgeConfig configuration node with all field inputs and collapsible sections
pub fn edge_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &EdgeConfigInputs,
    sections: &EdgeSections,
    on_toggle: impl Fn(EdgeSection) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let result = inputs.build();

    // Config in/out row
    let config_row = row![
        pin!(
            Left,
            pins::cfg::CONFIG,
            text("in").size(10),
            Input,
            ::std::any::TypeId::of::<pins::EdgeConfigData>()
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::cfg::EDGE_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::EdgeConfigData>()
        ),
    ]
    .align_y(iced::Alignment::Center);

    let mut items: Vec<iced::Element<'_, Message>> = vec![config_row.into()];

    // Precompute display values
    let thickness = result.pattern.map(|p| p.thickness);
    let curve_label = match result.curve {
        Some(EdgeCurve::BezierCubic) => "bezier",
        Some(EdgeCurve::Line) => "line",
        None => "--",
    };
    let pattern_label = match inputs.get_pattern_type() {
        PatternType::Solid => "solid",
        PatternType::Dashed => "dashed",
        PatternType::Arrowed => "arrowed",
        PatternType::Dotted => "dotted",
        PatternType::DashDotted => "dash-dot",
    };
    let angle_display = inputs
        .pattern_angle
        .map(|v| format!("{:.0} deg", v.to_degrees()))
        .unwrap_or_else(|| "--".to_string());
    let offset_display = inputs
        .shadow_offset
        .map(|(x, y)| format!("{:.1}, {:.1}", x, y))
        .unwrap_or_else(|| "--".to_string());

    // --- Stroke section ---
    push_section(
        &mut items,
        "Stroke",
        sections.stroke,
        on_toggle(EdgeSection::Stroke),
        (!sections.stroke).then(|| {
            collapsed_pin_row![
                (pins::edge::STROKE_COLOR, pins::ColorData),
                (pins::edge::THICKNESS, pins::Float),
                (pins::edge::CURVE, pins::EdgeCurveData),
                (pins::edge::STROKE_OUTLINE_WIDTH, pins::Float),
                (pins::edge::STROKE_OUTLINE_COLOR, pins::ColorData)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::edge::STROKE_COLOR,
                    text("color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(result.stroke_color.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::THICKNESS,
                    text("thickness").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(thickness, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::CURVE,
                    text("curve").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::EdgeCurveData>()
                ),
                value_display(curve_label),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::STROKE_OUTLINE_WIDTH,
                    text("outline width").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.stroke_outline_width, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::STROKE_OUTLINE_COLOR,
                    text("outline color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.stroke_outline_color.map(|q| q.near_start)),
            )
            .into(),
        ],
    );

    // --- Pattern section ---
    push_section(
        &mut items,
        "Pattern",
        sections.pattern,
        on_toggle(EdgeSection::Pattern),
        (!sections.pattern).then(|| {
            collapsed_pin_row![
                (pins::edge::PATTERN, pins::PatternTypeData),
                (pins::edge::DASH, pins::Float),
                (pins::edge::GAP, pins::Float),
                (pins::edge::ANGLE, pins::Float),
                (pins::edge::SPEED, pins::Float)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::edge::PATTERN,
                    text("pattern").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::PatternTypeData>()
                ),
                value_display(pattern_label),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::DASH,
                    text("dash").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.dash_length, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::GAP,
                    text("gap").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.gap_length, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::ANGLE,
                    text("angle").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(angle_display),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::SPEED,
                    text("speed").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.animation_speed, 0)),
            )
            .into(),
        ],
    );

    // --- Border section ---
    push_section(
        &mut items,
        "Border",
        sections.border,
        on_toggle(EdgeSection::Border),
        (!sections.border).then(|| {
            collapsed_pin_row![
                (pins::edge::BORDER_WIDTH, pins::Float),
                (pins::edge::BORDER_GAP, pins::Float),
                (pins::edge::BORDER_COLOR, pins::ColorData),
                (pins::edge::BORDER_BACKGROUND, pins::ColorData),
                (pins::edge::BORDER_OUTLINE_WIDTH, pins::Float),
                (pins::edge::BORDER_OUTLINE_COLOR, pins::ColorData)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_WIDTH,
                    text("width").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.border_width, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_GAP,
                    text("gap").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.border_gap, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_COLOR,
                    text("color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.border_color.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_BACKGROUND,
                    text("background").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.border_background.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_OUTLINE_WIDTH,
                    text("outline width").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.border_outline_width, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::BORDER_OUTLINE_COLOR,
                    text("outline color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.border_outline_color.map(|q| q.near_start)),
            )
            .into(),
        ],
    );

    // --- Shadow section ---
    push_section(
        &mut items,
        "Shadow",
        sections.shadow,
        on_toggle(EdgeSection::Shadow),
        (!sections.shadow).then(|| {
            collapsed_pin_row![
                (pins::edge::SHADOW_BLUR, pins::Float),
                (pins::edge::SHADOW_EXPAND, pins::Float),
                (pins::edge::SHADOW_COLOR, pins::ColorData),
                (pins::edge::SHADOW_OFFSET, pins::Vec2Data)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::edge::SHADOW_BLUR,
                    text("blur").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.shadow_blur, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::SHADOW_EXPAND,
                    text("expand").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.shadow_expand, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::SHADOW_COLOR,
                    text("color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.shadow_color.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::edge::SHADOW_OFFSET,
                    text("offset").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Vec2Data>()
                ),
                value_display(offset_display),
            )
            .into(),
        ],
    );

    let content = iced::widget::Column::with_children(items).spacing(4);

    column![
        node_title_bar("Edge Config", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
