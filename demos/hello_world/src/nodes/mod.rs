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

pub use bool_toggle::{BoolToggleConfig, bool_toggle_node};
pub use calendar::calendar_node;
pub use color_picker::{color_picker_node, color_preset_node};
pub use config::{
    EdgeConfigInputs, NodeConfigInputs, PinConfigInputs, ShadowConfigInputs,
    apply_to_graph_node, apply_to_node_node, edge_config_node, node_config_node, pin_config_node,
    shadow_config_node,
};
pub use email_parser::email_parser_node;
pub use email_trigger::email_trigger_node;
pub use enum_selector::{edge_type_selector_node, pin_shape_selector_node};
pub use filter::filter_node;
pub use float_slider::{FloatSliderConfig, float_slider_node};
pub use int_slider::{IntSliderConfig, int_slider_node};

use iced::{Color, Theme};
use iced_nodegraph::{EdgeConfig, EdgeType, NodeConfig, PinConfig, PinShape, ShadowConfig};

/// Semantic pin colors for consistent visual language across nodes.
/// Based on "Floating Workbench" design system.
pub mod colors {
    use iced::Color;

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
}

/// Node value types for data flow between nodes
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum NodeValue {
    Float(f32),
    Int(i32),
    Color(Color),
    Bool(bool),
    EdgeType(EdgeType),
    PinShape(PinShape),
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

    pub fn as_edge_type(&self) -> Option<EdgeType> {
        match self {
            NodeValue::EdgeType(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_pin_shape(&self) -> Option<PinShape> {
        match self {
            NodeValue::PinShape(s) => Some(*s),
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
    EdgeTypeSelector {
        value: EdgeType,
    },
    PinShapeSelector {
        value: PinShape,
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
            Self::EdgeTypeSelector { value } => NodeValue::EdgeType(*value),
            Self::PinShapeSelector { value } => NodeValue::PinShape(*value),
            Self::ColorPicker { color } | Self::ColorPreset { color } => NodeValue::Color(*color),
        }
    }

    /// Returns the output pin type
    pub fn output_type(&self) -> &'static str {
        match self {
            Self::FloatSlider { .. } => "float",
            Self::IntSlider { .. } => "int",
            Self::BoolToggle { .. } => "bool",
            Self::EdgeTypeSelector { .. } => "edge_type",
            Self::PinShapeSelector { .. } => "pin_shape",
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
                InputNodeType::EdgeTypeSelector { .. } => "Edge Type",
                InputNodeType::PinShapeSelector { .. } => "Pin Shape",
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
