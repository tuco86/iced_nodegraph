//! Edge Configuration Node
//!
//! Builds an EdgeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    widget::{column, container, row, text},
};
use iced_nodegraph::{
    EdgeBorder, EdgeConfig, EdgeCurve, EdgeShadow, NodeContentStyle, Pattern, pin,
};

use crate::nodes::{
    collapsed_pin_row, color_swatch, colors, fmt_float, node_title_bar, pin_row, pins,
    push_section, value_display,
};

/// Section expansion state for EdgeConfig nodes
#[derive(Debug, Clone, Default)]
pub struct EdgeSections {
    pub stroke: bool,
    pub pattern: bool,
    pub border: bool,
    pub shadow: bool,
    pub debug: bool,
}

impl EdgeSections {
    pub fn new_all_expanded() -> Self {
        Self {
            stroke: true,
            pattern: true,
            border: true,
            shadow: true,
            debug: false,
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
    Debug,
}

/// Pattern type for simple selection (maps to iced_sdf::Pattern)
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
    /// Parent config to inherit from
    pub config_in: Option<EdgeConfig>,

    // --- Stroke ---
    pub start_color: Option<Color>,
    pub end_color: Option<Color>,
    pub thickness: Option<f32>,
    pub curve: Option<EdgeCurve>,
    pub stroke_outline_thickness: Option<f32>,
    pub stroke_outline_color: Option<Color>,

    // --- Pattern ---
    pub pattern_type: Option<PatternType>,
    pub dash_length: Option<f32>,
    pub gap_length: Option<f32>,
    pub pattern_angle: Option<f32>,
    pub dot_radius: Option<f32>,
    pub animation_speed: Option<f32>,

    // --- Border ---
    pub border_thickness: Option<f32>,
    pub border_gap: Option<f32>,
    pub border_color: Option<Color>,
    pub border_color_end: Option<Color>,
    pub border_background: Option<Color>,
    pub border_background_end: Option<Color>,
    pub border_outline_thickness: Option<f32>,
    pub border_outline_color: Option<Color>,

    // --- Shadow ---
    pub shadow_expand: Option<f32>,
    pub shadow_blur: Option<f32>,
    pub shadow_color: Option<Color>,
    pub shadow_color_end: Option<Color>,
    pub shadow_offset_x: Option<f32>,
    pub shadow_offset_y: Option<f32>,

    // --- Debug ---
    pub tile_debug: bool,
}

impl EdgeConfigInputs {
    /// Builds the final EdgeConfig by merging with parent
    pub fn build(&self) -> EdgeConfig {
        let parent = self.config_in.clone().unwrap_or_default();

        // Build pattern from inputs
        let pattern = self.build_pattern(&parent);

        // Colors
        let start_color = self.start_color.or(parent.start_color);
        let end_color = self.end_color.or(parent.end_color);

        // Stroke outline
        let has_stroke_outline =
            self.stroke_outline_thickness.is_some() || self.stroke_outline_color.is_some();
        let stroke_outline = if has_stroke_outline {
            let parent_ol = parent.stroke_outline.unwrap_or((1.0, Color::WHITE));
            Some((
                self.stroke_outline_thickness.unwrap_or(parent_ol.0),
                self.stroke_outline_color.unwrap_or(parent_ol.1),
            ))
        } else {
            parent.stroke_outline
        };

        // Build border config
        let has_border_overrides = self.border_thickness.is_some()
            || self.border_gap.is_some()
            || self.border_color.is_some()
            || self.border_color_end.is_some()
            || self.border_background.is_some()
            || self.border_background_end.is_some()
            || self.border_outline_thickness.is_some()
            || self.border_outline_color.is_some();

        let border = if has_border_overrides {
            let pb = parent.border.unwrap_or_default();
            let has_ol =
                self.border_outline_thickness.is_some() || self.border_outline_color.is_some();
            let outline = if has_ol {
                let parent_ol = pb.outline.unwrap_or((1.0, Color::WHITE));
                Some((
                    self.border_outline_thickness.unwrap_or(parent_ol.0),
                    self.border_outline_color.unwrap_or(parent_ol.1),
                ))
            } else {
                pb.outline
            };
            Some(EdgeBorder {
                start_color: self.border_color.unwrap_or(pb.start_color),
                end_color: self.border_color_end.unwrap_or(pb.end_color),
                width: self.border_thickness.unwrap_or(pb.width),
                gap: self.border_gap.unwrap_or(pb.gap),
                outline,
                background: self.border_background.unwrap_or(pb.background),
                background_end: self.border_background_end.unwrap_or(pb.background_end),
            })
        } else {
            parent.border
        };

        // Build shadow config
        let has_shadow_overrides = self.shadow_blur.is_some()
            || self.shadow_expand.is_some()
            || self.shadow_color.is_some()
            || self.shadow_color_end.is_some()
            || self.shadow_offset_x.is_some()
            || self.shadow_offset_y.is_some();

        let shadow = if has_shadow_overrides {
            let ps = parent.shadow.unwrap_or_default();
            Some(EdgeShadow {
                color: self.shadow_color.unwrap_or(ps.color),
                end_color: self.shadow_color_end.unwrap_or(ps.end_color),
                expand: self.shadow_expand.unwrap_or(ps.expand),
                blur: self.shadow_blur.unwrap_or(ps.blur),
                offset: (
                    self.shadow_offset_x.unwrap_or(ps.offset.0),
                    self.shadow_offset_y.unwrap_or(ps.offset.1),
                ),
            })
        } else {
            parent.shadow
        };

        EdgeConfig {
            start_color,
            end_color,
            pattern,
            stroke_outline,
            border,
            shadow,
            curve: self.curve.or(parent.curve),
        }
    }

    /// Builds the Pattern from individual inputs, aligned with iced_sdf gallery.
    fn build_pattern(&self, parent: &EdgeConfig) -> Option<Pattern> {
        let pattern_type = self.pattern_type.unwrap_or(PatternType::Solid);
        let thickness = self.thickness.unwrap_or(2.0);
        let dash = self.dash_length.unwrap_or(12.0);
        let gap = self.gap_length.unwrap_or(6.0);
        let angle = self.pattern_angle.unwrap_or(0.0);
        let dot_radius = self.dot_radius.unwrap_or(2.0);
        let speed = self.animation_speed.unwrap_or(0.0);

        let has_overrides = self.pattern_type.is_some()
            || self.thickness.is_some()
            || self.dash_length.is_some()
            || self.gap_length.is_some()
            || self.pattern_angle.is_some()
            || self.dot_radius.is_some()
            || self.animation_speed.is_some();

        if !has_overrides {
            return parent.pattern;
        }

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
            pins::config::CONFIG,
            text("in").size(10),
            Input,
            pins::EdgeConfigData,
            colors::PIN_CONFIG
        ),
        container(text("")).width(Length::Fill),
        pin!(
            Right,
            pins::config::EDGE_OUT,
            text("out").size(10),
            Output,
            pins::EdgeConfigData,
            colors::PIN_CONFIG
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

    // --- Stroke section ---
    push_section(
        &mut items,
        "Stroke",
        sections.stroke,
        on_toggle(EdgeSection::Stroke),
        (!sections.stroke).then(|| {
            collapsed_pin_row![
                (pins::config::START, pins::ColorData, colors::PIN_COLOR),
                (pins::config::END, pins::ColorData, colors::PIN_COLOR),
                (pins::config::THICK, pins::Float, colors::PIN_NUMBER),
                (pins::config::CURVE, pins::EdgeCurveData, colors::PIN_ANY),
                (
                    pins::config::STROKE_OL_THICK,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                (
                    pins::config::STROKE_OL_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                )
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::config::START,
                    text("start").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(result.start_color),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::END,
                    text("end").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(result.end_color),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::THICK,
                    text("thick").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(thickness, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::CURVE,
                    text("curve").size(10),
                    Input,
                    pins::EdgeCurveData,
                    colors::PIN_ANY
                ),
                value_display(curve_label),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::STROKE_OL_THICK,
                    text("s.ol.w").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.stroke_outline_thickness, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::STROKE_OL_COLOR,
                    text("s.ol.c").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.stroke_outline_color),
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
                (
                    pins::config::PATTERN,
                    pins::PatternTypeData,
                    colors::PIN_ANY
                ),
                (pins::config::DASH, pins::Float, colors::PIN_NUMBER),
                (pins::config::GAP, pins::Float, colors::PIN_NUMBER),
                (pins::config::ANGLE, pins::Float, colors::PIN_NUMBER),
                (pins::config::SPEED, pins::Float, colors::PIN_NUMBER)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::config::PATTERN,
                    text("pattern").size(10),
                    Input,
                    pins::PatternTypeData,
                    colors::PIN_ANY
                ),
                value_display(pattern_label),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::DASH,
                    text("dash").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.dash_length, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::GAP,
                    text("gap").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.gap_length, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::ANGLE,
                    text("angle").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(angle_display),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SPEED,
                    text("speed").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
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
                (pins::config::BORDER_WIDTH, pins::Float, colors::PIN_NUMBER),
                (pins::config::BORDER_GAP, pins::Float, colors::PIN_NUMBER),
                (
                    pins::config::BORDER_START_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                (
                    pins::config::BORDER_END_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                (pins::config::BORDER_BG, pins::ColorData, colors::PIN_COLOR),
                (
                    pins::config::BORDER_BG_END,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                (
                    pins::config::BORDER_OL_THICK,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                (
                    pins::config::BORDER_OL_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                )
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_WIDTH,
                    text("b.thick").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.border_thickness, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_GAP,
                    text("b.gap").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.border_gap, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_START_COLOR,
                    text("b.start").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.border_color),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_END_COLOR,
                    text("b.end").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.border_color_end),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_BG,
                    text("b.bg").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.border_background),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_BG_END,
                    text("b.bge").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.border_background_end),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_OL_THICK,
                    text("bo.w").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.border_outline_thickness, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::BORDER_OL_COLOR,
                    text("bo.c").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.border_outline_color),
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
                (pins::config::SHADOW_BLUR, pins::Float, colors::PIN_NUMBER),
                (pins::config::SHADOW_EXPAND, pins::Float, colors::PIN_NUMBER),
                (
                    pins::config::SHADOW_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                (
                    pins::config::SHADOW_END_COLOR,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                (
                    pins::config::SHADOW_OFFSET_X,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                (
                    pins::config::SHADOW_OFFSET_Y,
                    pins::Float,
                    colors::PIN_NUMBER
                )
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_BLUR,
                    text("s.blur").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.shadow_blur, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_EXPAND,
                    text("s.exp").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.shadow_expand, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_COLOR,
                    text("s.color").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.shadow_color),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_END_COLOR,
                    text("s.cend").size(10),
                    Input,
                    pins::ColorData,
                    colors::PIN_COLOR
                ),
                color_swatch(inputs.shadow_color_end),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_OFFSET_X,
                    text("s.off.x").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.shadow_offset_x, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::config::SHADOW_OFFSET_Y,
                    text("s.off.y").size(10),
                    Input,
                    pins::Float,
                    colors::PIN_NUMBER
                ),
                value_display(fmt_float(inputs.shadow_offset_y, 1)),
            )
            .into(),
        ],
    );

    // --- Debug section ---
    push_section(
        &mut items,
        "Debug",
        sections.debug,
        on_toggle(EdgeSection::Debug),
        None,
        vec![
            row![
                text("tile debug").size(10).width(Length::Fill),
                text(if inputs.tile_debug { "ON" } else { "off" }).size(10),
            ]
            .align_y(iced::Alignment::Center)
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
