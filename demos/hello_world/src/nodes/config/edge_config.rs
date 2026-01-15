//! Edge Configuration Node
//!
//! Builds an EdgeConfig from individual field inputs with inheritance support.

use iced::{
    Color, Length,
    alignment::Horizontal,
    widget::{column, container, row, text},
};
use iced_nodegraph::{
    BorderConfig, DashCap, EdgeConfig, EdgeCurve, EdgeShadowConfig, NodeContentStyle, StrokeConfig,
    StrokePattern, pin,
};

use crate::nodes::{colors, node_title_bar, pins};

/// Pattern type for simple selection (maps to StrokePattern)
/// IDs: 0=Solid, 1=Dashed, 2=Arrowed, 3=Angled, 4=Dotted, 5=DashDotted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternType {
    #[default]
    Solid,
    Dashed,
    /// Arrow-like marks (///) crossing the edge
    Arrowed,
    /// Dashed with angled/parallelogram ends
    Angled,
    Dotted,
    DashDotted,
}

/// Collected inputs for EdgeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeConfigInputs {
    /// Parent config to inherit from
    pub config_in: Option<EdgeConfig>,
    /// Individual field overrides
    pub start_color: Option<Color>,
    pub end_color: Option<Color>,
    pub thickness: Option<f32>,
    pub curve: Option<EdgeCurve>,
    /// Pattern settings
    pub pattern_type: Option<PatternType>,
    pub dash_length: Option<f32>,
    pub gap_length: Option<f32>,
    pub pattern_angle: Option<f32>, // Angle in radians for Arrowed/Angled patterns
    pub dot_radius: Option<f32>,    // Dot radius for Dotted pattern
    /// Animation speed (0.0 = no animation, > 0.0 = animated)
    pub animation_speed: Option<f32>,
    /// Border settings (outline with gap around stroke)
    pub border_enabled: Option<bool>,
    pub border_width: Option<f32>,
    pub border_gap: Option<f32>,
    pub border_color: Option<Color>,
    /// Shadow settings
    pub shadow_enabled: Option<bool>,
    pub shadow_blur: Option<f32>,
    pub shadow_offset_x: Option<f32>,
    pub shadow_offset_y: Option<f32>,
    pub shadow_color: Option<Color>,
}

impl EdgeConfigInputs {
    /// Builds the final EdgeConfig by merging with parent
    pub fn build(&self) -> EdgeConfig {
        let parent = self.config_in.clone().unwrap_or_default();
        let parent_stroke = parent.stroke.clone().unwrap_or_default();

        // Build pattern from inputs
        let pattern = self.build_pattern(&parent_stroke);

        // Check if we have any stroke overrides
        let has_stroke_overrides = self.start_color.is_some()
            || self.end_color.is_some()
            || self.thickness.is_some()
            || self.pattern_type.is_some();

        // Set dash_cap to Angled for PatternType::Angled
        let dash_cap = if self.pattern_type == Some(PatternType::Angled) {
            let angle = self.pattern_angle.unwrap_or(std::f32::consts::FRAC_PI_4);
            Some(DashCap::Angled { angle_rad: angle })
        } else {
            parent_stroke.dash_cap
        };

        let stroke_config = if has_stroke_overrides {
            Some(StrokeConfig {
                start_color: self.start_color.or(parent_stroke.start_color),
                end_color: self.end_color.or(parent_stroke.end_color),
                width: self.thickness.or(parent_stroke.width),
                pattern,
                cap: parent_stroke.cap,
                dash_cap,
            })
        } else {
            parent.stroke.clone()
        };

        // Build border config if any border settings provided
        let has_border_overrides = self.border_enabled.is_some()
            || self.border_width.is_some()
            || self.border_gap.is_some()
            || self.border_color.is_some();

        let border_config = if has_border_overrides {
            let parent_border = parent.border.clone().unwrap_or_default();
            // If border explicitly disabled, return None
            if self.border_enabled == Some(false) {
                None
            } else {
                Some(BorderConfig {
                    width: self.border_width.or(parent_border.width),
                    gap: self.border_gap.or(parent_border.gap),
                    color: self.border_color.or(parent_border.color),
                    enabled: self.border_enabled.or(parent_border.enabled),
                })
            }
        } else {
            parent.border.clone()
        };

        // Build shadow config if any shadow settings provided
        let has_shadow_overrides = self.shadow_enabled.is_some()
            || self.shadow_blur.is_some()
            || self.shadow_offset_x.is_some()
            || self.shadow_offset_y.is_some()
            || self.shadow_color.is_some();

        let shadow_config = if has_shadow_overrides {
            let parent_shadow = parent.shadow.clone().unwrap_or_default();
            // If shadow explicitly disabled, return None
            if self.shadow_enabled == Some(false) {
                None
            } else {
                Some(EdgeShadowConfig {
                    blur: self.shadow_blur.or(parent_shadow.blur),
                    color: self.shadow_color.or(parent_shadow.color),
                    offset_x: self.shadow_offset_x.or(parent_shadow.offset_x),
                    offset_y: self.shadow_offset_y.or(parent_shadow.offset_y),
                    enabled: self.shadow_enabled.or(parent_shadow.enabled),
                })
            }
        } else {
            parent.shadow.clone()
        };

        EdgeConfig {
            stroke: stroke_config,
            border: border_config,
            shadow: shadow_config,
            curve: self.curve.or(parent.curve),
        }
    }

    /// Builds the StrokePattern from individual inputs
    fn build_pattern(&self, parent_stroke: &StrokeConfig) -> Option<StrokePattern> {
        use iced_nodegraph::DashMotion;

        let pattern_type = self.pattern_type.unwrap_or(PatternType::Solid);
        let dash = self.dash_length.unwrap_or(12.0);
        let gap = self.gap_length.unwrap_or(6.0);
        let angle = self.pattern_angle.unwrap_or(std::f32::consts::FRAC_PI_4); // 45 degrees default
        let dot_radius = self.dot_radius.unwrap_or(2.0);
        let speed = self.animation_speed.unwrap_or(0.0);

        // Animation enabled if speed != 0.0 (negative = reverse)
        let motion = if speed != 0.0 {
            Some(DashMotion::new(speed))
        } else {
            None
        };

        match pattern_type {
            PatternType::Solid => {
                // Keep parent pattern if solid selected and parent has pattern
                if self.pattern_type.is_none() {
                    parent_stroke.pattern.clone()
                } else {
                    Some(StrokePattern::Solid)
                }
            }
            PatternType::Dashed => Some(StrokePattern::Dashed {
                dash,
                gap,
                phase: 0.0,
                motion,
            }),
            PatternType::Arrowed => Some(StrokePattern::Arrowed {
                segment: dash,
                gap,
                angle,
                phase: 0.0,
                motion,
            }),
            PatternType::Angled => {
                // Angled = Dashed with angled/parallelogram ends
                // The dash_cap is set in build() based on pattern_type
                Some(StrokePattern::Dashed {
                    dash,
                    gap,
                    phase: 0.0,
                    motion,
                })
            }
            PatternType::Dotted => Some(StrokePattern::Dotted {
                spacing: gap,       // gap_length = spacing between dots
                radius: dot_radius, // dot_radius = size of each dot
                phase: 0.0,
                motion,
            }),
            PatternType::DashDotted => Some(StrokePattern::DashDotted {
                dash,
                gap,
                dot_radius,
                dot_gap: gap * 0.5,
                phase: 0.0,
                motion,
            }),
        }
    }

    /// Returns the current pattern type
    pub fn get_pattern_type(&self) -> PatternType {
        self.pattern_type.unwrap_or(PatternType::Solid)
    }
}

/// Creates an EdgeConfig configuration node with all field inputs
pub fn edge_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &EdgeConfigInputs,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::process(theme);
    let result = inputs.build();

    // Config row: input left, typed output right
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

    // Helper to create separator lines
    let make_separator = || {
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_: &_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    1.0, 1.0, 1.0, 0.1,
                ))),
                ..Default::default()
            })
    };

    // Get stroke values for display
    let stroke = result.stroke.as_ref();
    let start_color = stroke.and_then(|s| s.start_color);
    let end_color = stroke.and_then(|s| s.end_color);
    let thickness = stroke.and_then(|s| s.width);

    // Start color row
    let start_display: iced::Element<'a, Message> = if let Some(c) = start_color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let start_row = row![
        pin!(
            Left,
            pins::config::START,
            text("start").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(start_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // End color row
    let end_display: iced::Element<'a, Message> = if let Some(c) = end_color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let end_row = row![
        pin!(
            Left,
            pins::config::END,
            text("end").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(end_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Thickness row
    let thick_row = row![
        pin!(
            Left,
            pins::config::THICK,
            text("thick").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(text(thickness.map_or("--".to_string(), |v| format!("{:.1}", v))).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Curve type row
    let curve_label = match result.curve {
        Some(EdgeCurve::BezierCubic) => "bezier",
        Some(EdgeCurve::BezierQuadratic) => "quadratic",
        Some(EdgeCurve::Line) => "line",
        Some(EdgeCurve::Orthogonal) => "step",
        Some(EdgeCurve::OrthogonalSmooth { .. }) => "smooth",
        None => "--",
    };
    let curve_row = row![
        pin!(
            Left,
            pins::config::CURVE,
            text("curve").size(10),
            Input,
            pins::EdgeCurveData,
            colors::PIN_ANY
        ),
        container(text(curve_label).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Pattern type row
    let pattern_label = match inputs.get_pattern_type() {
        PatternType::Solid => "solid",
        PatternType::Dashed => "dashed",
        PatternType::Arrowed => "arrowed",
        PatternType::Angled => "angled",
        PatternType::Dotted => "dotted",
        PatternType::DashDotted => "dash-dot",
    };
    let pattern_row = row![
        pin!(
            Left,
            pins::config::PATTERN,
            text("pattern").size(10),
            Input,
            pins::PatternTypeData,
            colors::PIN_ANY
        ),
        container(text(pattern_label).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Dash length row
    let dash_row = row![
        pin!(
            Left,
            pins::config::DASH,
            text("dash").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .dash_length
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Gap length row
    let gap_row = row![
        pin!(
            Left,
            pins::config::GAP,
            text("gap").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .gap_length
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Pattern angle row (for Arrowed and Angled patterns)
    let angle_display = inputs
        .pattern_angle
        .map_or("--".to_string(), |v| format!("{:.0}°", v.to_degrees()));
    let angle_row = row![
        pin!(
            Left,
            pins::config::ANGLE,
            text("angle").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(text(angle_display).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Animation speed row (0 = off, > 0 = animated)
    let speed_row = row![
        pin!(
            Left,
            pins::config::SPEED,
            text("speed").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .animation_speed
                    .map_or("0".to_string(), |v| format!("{:.0}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border enabled row
    let border_label = match inputs.border_enabled {
        Some(true) => "yes",
        Some(false) => "no",
        None => "--",
    };
    let border_enabled_row = row![
        pin!(
            Left,
            pins::config::BORDER,
            text("border").size(10),
            Input,
            pins::Bool,
            colors::PIN_BOOL
        ),
        container(text(border_label).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border width row
    let border_width_row = row![
        pin!(
            Left,
            pins::config::BORDER_WIDTH,
            text("b.width").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .border_width
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border gap row
    let border_gap_row = row![
        pin!(
            Left,
            pins::config::BORDER_GAP,
            text("b.gap").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .border_gap
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Border color row
    let border_color_display: iced::Element<'a, Message> = if let Some(c) = inputs.border_color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let border_color_row = row![
        pin!(
            Left,
            pins::config::BORDER_COLOR,
            text("b.color").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(border_color_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Shadow enabled row
    let shadow_label = match inputs.shadow_enabled {
        Some(true) => "yes",
        Some(false) => "no",
        None => "--",
    };
    let shadow_enabled_row = row![
        pin!(
            Left,
            pins::config::SHADOW,
            text("shadow").size(10),
            Input,
            pins::Bool,
            colors::PIN_BOOL
        ),
        container(text(shadow_label).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Shadow blur row
    let shadow_blur_row = row![
        pin!(
            Left,
            pins::config::SHADOW_BLUR,
            text("s.blur").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(
            text(
                inputs
                    .shadow_blur
                    .map_or("--".to_string(), |v| format!("{:.1}", v))
            )
            .size(9)
        )
        .width(Length::Fill)
        .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Shadow offset row (combined x,y display)
    let offset_display = match (inputs.shadow_offset_x, inputs.shadow_offset_y) {
        (Some(x), Some(y)) => format!("{:.0},{:.0}", x, y),
        (Some(x), None) => format!("{:.0},--", x),
        (None, Some(y)) => format!("--,{:.0}", y),
        (None, None) => "--".to_string(),
    };
    let shadow_offset_row = row![
        pin!(
            Left,
            pins::config::SHADOW_OFFSET,
            text("s.offs").size(10),
            Input,
            pins::Float,
            colors::PIN_NUMBER
        ),
        container(text(offset_display).size(9))
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    // Shadow color row
    let shadow_color_display: iced::Element<'a, Message> = if let Some(c) = inputs.shadow_color {
        container(text(""))
            .width(20)
            .height(12)
            .style(move |_: &_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    color: colors::PIN_ANY,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        text("--").size(9).into()
    };
    let shadow_color_row = row![
        pin!(
            Left,
            pins::config::SHADOW_COLOR,
            text("s.color").size(10),
            Input,
            pins::ColorData,
            colors::PIN_COLOR
        ),
        container(shadow_color_display)
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![
        config_row,
        make_separator(),
        start_row,
        end_row,
        thick_row,
        curve_row,
        make_separator(),
        pattern_row,
        dash_row,
        gap_row,
        angle_row,
        speed_row,
        make_separator(),
        border_enabled_row,
        border_width_row,
        border_gap_row,
        border_color_row,
        make_separator(),
        shadow_enabled_row,
        shadow_blur_row,
        shadow_offset_row,
        shadow_color_row,
    ]
    .spacing(4);

    column![
        node_title_bar("Edge Config", style),
        container(content).padding([8, 10])
    ]
    .width(150.0)
    .into()
}
