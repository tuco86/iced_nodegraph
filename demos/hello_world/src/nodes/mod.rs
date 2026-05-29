#![allow(clippy::large_enum_variant)]

mod bool_toggle;
mod calendar;
mod color_picker;
pub mod config;
mod email_parser;
mod email_trigger;
mod enum_selector;
mod filter;
mod float_slider;
mod int_slider;
mod math;
pub mod pins;

pub use bool_toggle::{BoolToggleConfig, bool_toggle_node};
pub use calendar::calendar_node;
pub use color_picker::{color_picker_node, color_preset_node};
pub use config::{
    EdgeConfigInputs, EdgeSection, EdgeSections, NodeConfigInputs, NodeSection, NodeSections,
    PatternType, PinConfigInputs, ShadowConfigInputs, apply_to_graph_node, apply_to_node_node,
    edge_config_node, node_config_node, pin_config_node, shadow_config_node,
};
pub use email_parser::email_parser_node;
pub use email_trigger::email_trigger_node;
pub use enum_selector::{
    edge_curve_selector_node, pattern_type_selector_node, pin_shape_selector_node,
};
pub use filter::filter_node;
pub use float_slider::{FloatSliderConfig, float_slider_node};
pub use int_slider::{IntSliderConfig, int_slider_node};
pub use math::math_node;

use iced::{
    Color, Element, Length, Padding, Theme,
    alignment::Horizontal,
    widget::{Container, Row, container, row, text},
};
use iced_nodegraph::{
    EdgeConfig, EdgeCurve, NodeConfig, NodeContentStyle, PinConfig, PinShape, ShadowConfig,
    node_header,
};

/// Semantic pin colors for consistent visual language across nodes.
/// Based on "Industrial Precision" design system.
pub mod colors {
    use iced::Color;

    // === Pin Type Colors ===

    /// Email/Message data - Sky Blue (#38BDF8)
    pub const PIN_EMAIL: Color = Color::from_rgb(0.22, 0.74, 0.97);

    /// String/Text data - Amber (#FBBF24)
    pub const PIN_STRING: Color = Color::from_rgb(0.98, 0.75, 0.14);

    /// Number/Float data - Emerald (#34D399)
    pub const PIN_NUMBER: Color = Color::from_rgb(0.20, 0.83, 0.60);

    /// DateTime data - Violet (#A78BFA)
    pub const PIN_DATETIME: Color = Color::from_rgb(0.65, 0.55, 0.98);

    /// Color data - Pink (#F472B6)
    pub const PIN_COLOR: Color = Color::from_rgb(0.96, 0.45, 0.71);

    /// Boolean/Logic data - Orange (#FB923C)
    pub const PIN_BOOL: Color = Color::from_rgb(0.98, 0.57, 0.24);

    /// Generic/Any data - Slate (#94A3B8)
    pub const PIN_ANY: Color = Color::from_rgb(0.58, 0.64, 0.72);

    /// Config data - Cyan (#22D3EE)
    pub const PIN_CONFIG: Color = Color::from_rgb(0.13, 0.83, 0.93);

    // === Surface Colors (Industrial Precision Theme) ===

    /// Elevated surface for controls - Deep slate (#2A2A3C)
    pub const SURFACE_ELEVATED: Color = Color::from_rgb(0.165, 0.165, 0.235);

    /// Subtle border color (#3A3A4C)
    pub const BORDER_SUBTLE: Color = Color::from_rgb(0.227, 0.227, 0.298);

    /// Primary text color (#E4E4E7)
    pub const TEXT_PRIMARY: Color = Color::from_rgb(0.894, 0.894, 0.906);

    /// Muted/secondary text (#A1A1AA)
    pub const TEXT_MUTED: Color = Color::from_rgb(0.631, 0.631, 0.667);
}

/// Node value types for data flow between nodes
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum NodeValue {
    Float(f32),
    Int(i32),
    Color(Color),
    Bool(bool),
    EdgeCurve(EdgeCurve),
    PinShape(PinShape),
    PatternType(PatternType),
    // Config types for config-node chains
    NodeConfig(NodeConfig),
    EdgeConfig(EdgeConfig),
    PinConfig(PinConfig),
    ShadowConfig(ShadowConfig),
}

#[allow(dead_code)]
impl NodeValue {
    pub fn as_float(&self) -> Option<f32> {
        match self {
            NodeValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            NodeValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            NodeValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            NodeValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_edge_curve(&self) -> Option<EdgeCurve> {
        match self {
            NodeValue::EdgeCurve(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_pin_shape(&self) -> Option<PinShape> {
        match self {
            NodeValue::PinShape(s) => Some(*s),
            _ => None,
        }
    }

    pub fn as_pattern_type(&self) -> Option<PatternType> {
        match self {
            NodeValue::PatternType(p) => Some(*p),
            _ => None,
        }
    }

    pub fn as_node_config(&self) -> Option<&NodeConfig> {
        match self {
            NodeValue::NodeConfig(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_edge_config(&self) -> Option<&EdgeConfig> {
        match self {
            NodeValue::EdgeConfig(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_pin_config(&self) -> Option<&PinConfig> {
        match self {
            NodeValue::PinConfig(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_shadow_config(&self) -> Option<&ShadowConfig> {
        match self {
            NodeValue::ShadowConfig(c) => Some(c),
            _ => None,
        }
    }
}

/// Configuration node types that affect graph styling
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigNodeType {
    NodeConfig(NodeConfigInputs),
    EdgeConfig(EdgeConfigInputs),
    ShadowConfig(ShadowConfigInputs),
    PinConfig(PinConfigInputs),
    // Apply nodes
    ApplyToGraph {
        has_node_config: bool,
        has_edge_config: bool,
        has_pin_config: bool,
    },
    ApplyToNode {
        has_node_config: bool,
        target_id: Option<i32>,
    },
}

/// Mathematical operations for math nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl MathOperation {
    /// Returns the display symbol for this operation
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
        }
    }

    /// Returns the display name for this operation
    pub fn name(&self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
        }
    }

    /// Computes the result of this operation
    pub fn compute(&self, a: f32, b: f32) -> f32 {
        match self {
            Self::Add => a + b,
            Self::Subtract => a - b,
            Self::Multiply => a * b,
            Self::Divide => {
                if b != 0.0 {
                    a / b
                } else {
                    f32::INFINITY
                }
            }
        }
    }
}

/// State for a math node
#[derive(Debug, Clone, PartialEq)]
pub struct MathNodeState {
    pub operation: MathOperation,
    pub input_a: Option<f32>,
    pub input_b: Option<f32>,
}

impl MathNodeState {
    pub fn new(operation: MathOperation) -> Self {
        Self {
            operation,
            input_a: None,
            input_b: None,
        }
    }

    /// Computes the result if both inputs are available
    pub fn result(&self) -> Option<f32> {
        match (self.input_a, self.input_b) {
            (Some(a), Some(b)) => Some(self.operation.compute(a, b)),
            _ => None,
        }
    }
}

/// Input node types that produce values
#[derive(Debug, Clone, PartialEq)]
pub enum InputNodeType {
    FloatSlider {
        config: FloatSliderConfig,
        value: f32,
    },
    IntSlider {
        config: IntSliderConfig,
        value: i32,
    },
    BoolToggle {
        config: BoolToggleConfig,
        value: bool,
    },
    EdgeCurveSelector {
        value: EdgeCurve,
    },
    PinShapeSelector {
        value: PinShape,
    },
    PatternTypeSelector {
        value: PatternType,
    },
    ColorPicker {
        color: Color,
    },
    ColorPreset {
        color: Color,
    },
}

#[allow(dead_code)]
impl InputNodeType {
    /// Returns the output value for this input node
    pub fn output_value(&self) -> NodeValue {
        match self {
            Self::FloatSlider { value, .. } => NodeValue::Float(*value),
            Self::IntSlider { value, .. } => NodeValue::Int(*value),
            Self::BoolToggle { value, .. } => NodeValue::Bool(*value),
            Self::EdgeCurveSelector { value } => NodeValue::EdgeCurve(*value),
            Self::PinShapeSelector { value } => NodeValue::PinShape(*value),
            Self::PatternTypeSelector { value } => NodeValue::PatternType(*value),
            Self::ColorPicker { color } | Self::ColorPreset { color } => NodeValue::Color(*color),
        }
    }

    /// Returns the output pin type
    pub fn output_type(&self) -> &'static str {
        match self {
            Self::FloatSlider { .. } => "float",
            Self::IntSlider { .. } => "int",
            Self::BoolToggle { .. } => "bool",
            Self::EdgeCurveSelector { .. } => "edge_curve",
            Self::PinShapeSelector { .. } => "pin_shape",
            Self::PatternTypeSelector { .. } => "pattern_type",
            Self::ColorPicker { .. } | Self::ColorPreset { .. } => "color",
        }
    }
}

/// Extended node type for the demo
#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    /// Original workflow nodes
    Workflow(String),
    /// Input nodes that produce values
    Input(InputNodeType),
    /// Config nodes that consume values and affect styling
    Config(ConfigNodeType),
    /// Math nodes that compute values from inputs
    Math(MathNodeState),
}

#[allow(dead_code)]
impl NodeType {
    pub fn name(&self) -> &str {
        match self {
            Self::Workflow(name) => name.as_str(),
            Self::Input(input) => match input {
                InputNodeType::FloatSlider { config, .. } => config.label.as_str(),
                InputNodeType::IntSlider { config, .. } => config.label.as_str(),
                InputNodeType::BoolToggle { config, .. } => config.label.as_str(),
                InputNodeType::EdgeCurveSelector { .. } => "Edge Curve",
                InputNodeType::PinShapeSelector { .. } => "Pin Shape",
                InputNodeType::PatternTypeSelector { .. } => "Pattern Type",
                InputNodeType::ColorPicker { .. } => "Color Picker",
                InputNodeType::ColorPreset { .. } => "Color Preset",
            },
            Self::Config(config) => match config {
                ConfigNodeType::NodeConfig(_) => "Node Config",
                ConfigNodeType::EdgeConfig(_) => "Edge Config",
                ConfigNodeType::ShadowConfig(_) => "Shadow Config",
                ConfigNodeType::PinConfig(_) => "Pin Config",
                ConfigNodeType::ApplyToGraph { .. } => "Apply to Graph",
                ConfigNodeType::ApplyToNode { .. } => "Apply to Node",
            },
            Self::Math(state) => state.operation.name(),
        }
    }

    /// Returns the output value for this node, if it produces one
    pub fn output_value(&self) -> Option<NodeValue> {
        match self {
            Self::Input(input) => Some(input.output_value()),
            Self::Math(state) => state.result().map(NodeValue::Float),
            Self::Workflow(_) | Self::Config(_) => None,
        }
    }
}

/// Creates a node element based on the node type name (legacy support).
pub fn node<'a, Message>(node_type: &str, theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    match node_type {
        "email_trigger" => email_trigger_node(theme),
        "email_parser" => email_parser_node(theme),
        "filter" => filter_node(theme),
        "calendar" => calendar_node(theme),
        _ => email_trigger_node(theme), // fallback
    }
}

/// Creates a themed title bar container for nodes.
///
/// Uses `node_header` from the library as the base container with proper
/// rounded corners, then adds title text with appropriate padding.
pub fn node_title_bar<'a, Message>(
    title: impl Into<String>,
    style: NodeContentStyle,
) -> Container<'a, Message, Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    let title_text = text(title.into()).size(13).color(style.title_text);

    // Use node_header for the rounded corner container
    node_header(
        container(title_text).padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 8.0,
            right: 8.0,
        }),
        style.title_background,
        style.corner_radius,
        style.border_width,
    )
}

/// Creates a collapsible section header with optional collapsed pins inline.
/// Format when expanded: "──── Label - ────"
/// Format when collapsed with pins: "[pins] ── Label + ──"
pub fn section_header_with_pins<'a, Message: Clone + 'a>(
    title: &'a str,
    expanded: bool,
    on_toggle: Message,
    collapsed_pins: Option<iced::Element<'a, Message>>,
) -> iced::widget::Button<'a, Message, Theme, iced::Renderer> {
    use iced::widget::{button, row};

    let indicator = if expanded { "-" } else { "+" };
    let label_text = format!("{} {}", title, indicator);

    // Separator line style
    let separator_style = |_: &_| container::Style {
        background: Some(iced::Background::Color(Color::from_rgba(
            1.0, 1.0, 1.0, 0.1,
        ))),
        ..Default::default()
    };

    // Build the row content based on whether we have collapsed pins
    let row_content: iced::Element<'a, Message> = if !expanded {
        if let Some(pins) = collapsed_pins {
            // Collapsed with pins: [pins] ── Label + ──
            row![
                pins,
                container(text(""))
                    .width(Length::Fill)
                    .height(1)
                    .style(separator_style),
                text(label_text).size(9).color(colors::TEXT_MUTED),
                container(text(""))
                    .width(Length::Fill)
                    .height(1)
                    .style(separator_style),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            // Collapsed without pins: ──── Label + ────
            row![
                container(text(""))
                    .width(Length::Fill)
                    .height(1)
                    .style(separator_style),
                text(label_text).size(9).color(colors::TEXT_MUTED),
                container(text(""))
                    .width(Length::Fill)
                    .height(1)
                    .style(separator_style),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .into()
        }
    } else {
        // Expanded: ──── Label - ────
        row![
            container(text(""))
                .width(Length::Fill)
                .height(1)
                .style(separator_style),
            text(label_text).size(9).color(colors::TEXT_MUTED),
            container(text(""))
                .width(Length::Fill)
                .height(1)
                .style(separator_style),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
    };

    button(row_content)
        .width(Length::Fill)
        .on_press(on_toggle)
        .padding([4, 0])
        .style(|_theme: &Theme, status| {
            let background = match status {
                button::Status::Hovered | button::Status::Pressed => Some(iced::Background::Color(
                    Color::from_rgba(1.0, 1.0, 1.0, 0.03),
                )),
                _ => None,
            };
            button::Style {
                background,
                text_color: colors::TEXT_MUTED,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
}

// ============================================================================
// Shared helpers for config node UI (used by edge_config, node_config, etc.)
// ============================================================================

/// Renders an `Option<Color>` as a small colored swatch or "--" placeholder.
pub fn color_swatch<'a, Message: 'a>(
    color: Option<Color>,
) -> Element<'a, Message, Theme, iced::Renderer> {
    if let Some(c) = color {
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
    }
}

/// Renders a value string right-aligned in a container.
pub fn value_display<'a, Message: 'a>(
    display: impl Into<String>,
) -> Element<'a, Message, Theme, iced::Renderer> {
    container(text(display.into()).size(9))
        .width(Length::Fill)
        .align_x(Horizontal::Right)
        .into()
}

/// Formats an optional float for display, or "--" if None.
pub fn fmt_float(value: Option<f32>, decimals: usize) -> String {
    match value {
        Some(v) => format!("{:.prec$}", v, prec = decimals),
        None => "--".to_string(),
    }
}

/// Creates a standard pin input row: [pin on left] [display value right-aligned].
pub fn pin_row<'a, Message: Clone + 'a>(
    pin_element: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    display: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
) -> Row<'a, Message, Theme, iced::Renderer> {
    row![
        pin_element.into(),
        container(display.into())
            .width(Length::Fill)
            .align_x(Horizontal::Right),
    ]
    .align_y(iced::Alignment::Center)
}

/// Creates a row of disabled collapsed pins (used when a section is collapsed).
macro_rules! collapsed_pin_row {
    ( $( ($id:expr, $dt:ty, $color:expr) ),+ $(,)? ) => {
        iced::widget::row![
            $( iced_nodegraph::pin!(Left, $id, iced::widget::text("").size(1), Input, $dt, $color).disable_interactions() ),+
        ].spacing(2)
    };
}
pub(crate) use collapsed_pin_row;

/// Pushes a collapsible section (header + optional expanded rows) into a content list.
pub fn push_section<'a, Message: Clone + 'a>(
    items: &mut Vec<Element<'a, Message, Theme, iced::Renderer>>,
    title: &'a str,
    expanded: bool,
    on_toggle: Message,
    collapsed_pins: Option<Element<'a, Message, Theme, iced::Renderer>>,
    rows: Vec<Element<'a, Message, Theme, iced::Renderer>>,
) {
    items.push(section_header_with_pins(title, expanded, on_toggle, collapsed_pins).into());
    if expanded {
        items.extend(rows);
    }
}
