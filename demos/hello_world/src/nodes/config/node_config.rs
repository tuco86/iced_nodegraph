//! Node Configuration Node
//!
//! Builds a NodeConfig from individual field inputs with inheritance support.
//! Mirrors [`iced_nodegraph::NodeStyle`] field-for-field: Fill, Border (color,
//! width, outline), the border Pattern (dash/gap/angle/flow), and Shadow.
//! Color inputs are `ColorQuad`s; the shadow offset is a single 2D vector.

use demo_common::NodeContentStyle;
use iced_nodegraph::{ColorQuad, Pattern, pin};

use super::PatternType;
use crate::nodes::{
    collapsed_pin_row, color_swatch, fmt_float, node_title_bar, pin_row, pins, push_section,
    value_display,
};
use crate::style_overlay::NodeOverlay;

/// Section expansion state for NodeConfig nodes
#[derive(Debug, Clone, Default)]
pub struct NodeSections {
    pub fill: bool,
    pub border: bool,
    pub pattern: bool,
    pub shadow: bool,
}

impl NodeSections {
    pub fn new_all_expanded() -> Self {
        Self {
            fill: true,
            border: true,
            pattern: true,
            shadow: true,
        }
    }
}

/// Identifies which section to toggle in NodeConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSection {
    Fill,
    Border,
    Pattern,
    Shadow,
}

/// Collected inputs for NodeConfigNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeConfigInputs {
    /// Parent overlay to inherit from
    pub config_in: Option<NodeOverlay>,

    // --- Fill ---
    pub fill_color: Option<ColorQuad>,
    pub corner_radius: Option<f32>,
    pub opacity: Option<f32>,

    // --- Border ---
    pub border_color: Option<ColorQuad>,
    pub border_width: Option<f32>,
    pub border_outline_width: Option<f32>,
    pub border_outline_color: Option<ColorQuad>,

    // --- Border pattern ---
    pub pattern_type: Option<PatternType>,
    pub dash_length: Option<f32>,
    pub gap_length: Option<f32>,
    pub pattern_angle: Option<f32>,
    pub animation_speed: Option<f32>,

    // --- Shadow ---
    pub shadow_color: Option<ColorQuad>,
    pub shadow_distance: Option<f32>,
    pub shadow_offset: Option<(f32, f32)>,
}

impl NodeConfigInputs {
    /// Builds the final overlay by merging this node's fields over the parent.
    pub fn build(&self) -> NodeOverlay {
        let mut p = NodeOverlay::new();
        if let Some(c) = self.fill_color {
            p = p.fill_color(c);
        }
        if let Some(r) = self.corner_radius {
            p = p.corner_radius(r);
        }
        if let Some(o) = self.opacity {
            p = p.opacity(o);
        }
        if let Some(c) = self.border_color {
            p = p.border_color(c);
        }
        if let Some(pat) = self.build_pattern() {
            p = p.border_pattern(pat);
        }
        if let Some(w) = self.border_outline_width {
            p = p.border_outline_width(w);
        }
        if let Some(c) = self.border_outline_color {
            p = p.border_outline_color(c);
        }
        // NodeStyle::shadow_color is a plain Color; take the quad's near corner.
        if let Some(c) = self.shadow_color {
            p = p.shadow_color(c.near_start);
        }
        if let Some(d) = self.shadow_distance {
            p = p.shadow_distance(d);
        }
        if let Some(off) = self.shadow_offset {
            p = p.shadow_offset(off);
        }
        match &self.config_in {
            Some(parent) => p.merge(parent),
            None => p,
        }
    }

    /// Builds the border `Pattern` from the width plus the pattern fields.
    /// Returns None when no relevant field is set (inherit).
    fn build_pattern(&self) -> Option<Pattern> {
        let has_overrides = self.pattern_type.is_some()
            || self.border_width.is_some()
            || self.dash_length.is_some()
            || self.gap_length.is_some()
            || self.pattern_angle.is_some()
            || self.animation_speed.is_some();

        if !has_overrides {
            return None;
        }

        let pattern_type = self.pattern_type.unwrap_or(PatternType::Solid);
        let thickness = self.border_width.unwrap_or(1.0);
        let dash = self.dash_length.unwrap_or(12.0);
        let gap = self.gap_length.unwrap_or(6.0);
        let angle = self.pattern_angle.unwrap_or(0.0);
        let speed = self.animation_speed.unwrap_or(0.0);
        let dot_radius = 2.0;

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

/// Creates a NodeConfig configuration node with all field inputs and collapsible sections
pub fn node_config_node<'a, Message>(
    theme: &'a iced::Theme,
    inputs: &NodeConfigInputs,
    sections: &NodeSections,
    on_toggle: impl Fn(NodeSection) -> Message + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    use iced::widget::{column, container, row, text};

    let style = NodeContentStyle::output(theme);
    let result = inputs.build();

    // Config in/out row
    let config_row = row![
        pin!(
            Left,
            pins::cfg::CONFIG,
            text("in").size(10),
            Input,
            ::std::any::TypeId::of::<pins::NodeConfigData>()
        ),
        container(text("")).width(iced::Length::Fill),
        pin!(
            Right,
            pins::cfg::NODE_OUT,
            text("out").size(10),
            Output,
            ::std::any::TypeId::of::<pins::NodeConfigData>()
        ),
    ]
    .align_y(iced::Alignment::Center);

    let mut items: Vec<iced::Element<'_, Message>> = vec![config_row.into()];

    // Precompute display values
    let border_width = result.border_pattern.map(|p| p.thickness);
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

    // --- Fill section ---
    push_section(
        &mut items,
        "Fill",
        sections.fill,
        on_toggle(NodeSection::Fill),
        (!sections.fill).then(|| {
            collapsed_pin_row![
                (pins::node::FILL_COLOR, pins::ColorData),
                (pins::node::CORNER_RADIUS, pins::Float),
                (pins::node::OPACITY, pins::Float)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::node::FILL_COLOR,
                    text("fill").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(result.fill_color.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::node::CORNER_RADIUS,
                    text("radius").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(result.corner_radius, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::node::OPACITY,
                    text("opacity").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(
                    result
                        .opacity
                        .map_or("--".to_string(), |v| format!("{:.0}%", v * 100.0)),
                ),
            )
            .into(),
        ],
    );

    // --- Border section ---
    push_section(
        &mut items,
        "Border",
        sections.border,
        on_toggle(NodeSection::Border),
        (!sections.border).then(|| {
            collapsed_pin_row![
                (pins::node::BORDER_COLOR, pins::ColorData),
                (pins::node::BORDER_WIDTH, pins::Float),
                (pins::node::BORDER_OUTLINE_WIDTH, pins::Float),
                (pins::node::BORDER_OUTLINE_COLOR, pins::ColorData)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::node::BORDER_COLOR,
                    text("color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(result.border_color.map(|q| q.near_start)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::node::BORDER_WIDTH,
                    text("width").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(border_width, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::node::BORDER_OUTLINE_WIDTH,
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
                    pins::node::BORDER_OUTLINE_COLOR,
                    text("outline color").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::ColorData>()
                ),
                color_swatch(inputs.border_outline_color.map(|q| q.near_start)),
            )
            .into(),
        ],
    );

    // --- Border pattern section ---
    push_section(
        &mut items,
        "Pattern",
        sections.pattern,
        on_toggle(NodeSection::Pattern),
        (!sections.pattern).then(|| {
            collapsed_pin_row![
                (pins::node::PATTERN, pins::PatternTypeData),
                (pins::node::DASH, pins::Float),
                (pins::node::GAP, pins::Float),
                (pins::node::ANGLE, pins::Float),
                (pins::node::SPEED, pins::Float)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::node::PATTERN,
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
                    pins::node::DASH,
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
                    pins::node::GAP,
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
                    pins::node::ANGLE,
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
                    pins::node::SPEED,
                    text("speed").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.animation_speed, 0)),
            )
            .into(),
        ],
    );

    // --- Shadow section ---
    push_section(
        &mut items,
        "Shadow",
        sections.shadow,
        on_toggle(NodeSection::Shadow),
        (!sections.shadow).then(|| {
            collapsed_pin_row![
                (pins::node::SHADOW_COLOR, pins::ColorData),
                (pins::node::SHADOW_DISTANCE, pins::Float),
                (pins::node::SHADOW_OFFSET, pins::Vec2Data)
            ]
            .into()
        }),
        vec![
            pin_row(
                pin!(
                    Left,
                    pins::node::SHADOW_COLOR,
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
                    pins::node::SHADOW_DISTANCE,
                    text("distance").size(10),
                    Input,
                    ::std::any::TypeId::of::<pins::Float>()
                ),
                value_display(fmt_float(inputs.shadow_distance, 1)),
            )
            .into(),
            pin_row(
                pin!(
                    Left,
                    pins::node::SHADOW_OFFSET,
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
        node_title_bar("Node Config", style),
        container(content).padding([8, 10])
    ]
    .width(160.0)
    .into()
}
