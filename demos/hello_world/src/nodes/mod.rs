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
    BackgroundConfigInputs, EdgeConfigInputs, NodeConfigInputs, PatternType, PatternTypeSelection,
    PinConfigInputs, ShadowConfigInputs, apply_to_graph_node, apply_to_node_node,
    background_config_node, edge_config_node, node_config_node, pin_config_node,
    shadow_config_node,
};
pub use email_parser::email_parser_node;
pub use email_trigger::email_trigger_node;
pub use enum_selector::{
    background_pattern_selector_node, edge_curve_selector_node, pattern_type_selector_node,
    pin_shape_selector_node,
};
pub use filter::filter_node;
pub use float_slider::{FloatSliderConfig, float_slider_node};
pub use int_slider::{IntSliderConfig, int_slider_node};
pub use math::math_node;

use iced::{
    Color, Padding, Theme,
    widget::{Container, container, text},
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
    BackgroundPattern(PatternTypeSelection),
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

    pub fn as_background_pattern(&self) -> Option<PatternTypeSelection> {
        match self {
            NodeValue::BackgroundPattern(p) => Some(*p),
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
    BackgroundConfig(BackgroundConfigInputs),
    // Apply nodes
    ApplyToGraph {
        has_node_config: bool,
        has_edge_config: bool,
        has_pin_config: bool,
        has_background_config: bool,
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
    BackgroundPatternSelector {
        value: PatternTypeSelection,
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
            Self::BackgroundPatternSelector { value } => NodeValue::BackgroundPattern(*value),
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
            Self::BackgroundPatternSelector { .. } => "background_pattern",
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
                InputNodeType::BackgroundPatternSelector { .. } => "Bg Pattern",
                InputNodeType::ColorPicker { .. } => "Color Picker",
                InputNodeType::ColorPreset { .. } => "Color Preset",
            },
            Self::Config(config) => match config {
                ConfigNodeType::NodeConfig(_) => "Node Config",
                ConfigNodeType::EdgeConfig(_) => "Edge Config",
                ConfigNodeType::ShadowConfig(_) => "Shadow Config",
                ConfigNodeType::PinConfig(_) => "Pin Config",
                ConfigNodeType::BackgroundConfig(_) => "Background Config",
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
