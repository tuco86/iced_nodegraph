mod calendar;
mod color_picker;
mod email_parser;
mod email_trigger;
mod filter;
mod float_slider;
mod style_config;

pub use calendar::calendar_node;
pub use color_picker::{ColorPreset, color_picker_node, color_preset_node};
pub use email_parser::email_parser_node;
pub use email_trigger::email_trigger_node;
pub use filter::filter_node;
pub use float_slider::{FloatSliderConfig, float_slider_node};
pub use style_config::{
    border_width_config_node, corner_radius_config_node, edge_color_config_node,
    edge_thickness_config_node, fill_color_config_node, opacity_config_node,
};

use iced::{Color, Theme};

/// Node value types for data flow between nodes
#[derive(Debug, Clone)]
pub enum NodeValue {
    Float(f32),
    Color(Color),
    Bool(bool),
}

impl NodeValue {
    pub fn as_float(&self) -> Option<f32> {
        match self {
            NodeValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            NodeValue::Color(c) => Some(*c),
            _ => None,
        }
    }
}

/// Configuration node types that affect graph styling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigNodeType {
    CornerRadius,
    Opacity,
    BorderWidth,
    FillColor,
    EdgeThickness,
    EdgeColor,
}

impl ConfigNodeType {
    /// Returns the expected input pin type for this config node
    pub fn input_type(&self) -> &'static str {
        match self {
            Self::CornerRadius | Self::Opacity | Self::BorderWidth | Self::EdgeThickness => "float",
            Self::FillColor | Self::EdgeColor => "color",
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
    ColorPicker {
        color: Color,
    },
    ColorPreset {
        color: Color,
    },
}

impl InputNodeType {
    /// Returns the output value for this input node
    pub fn output_value(&self) -> NodeValue {
        match self {
            Self::FloatSlider { value, .. } => NodeValue::Float(*value),
            Self::ColorPicker { color } | Self::ColorPreset { color } => NodeValue::Color(*color),
        }
    }

    /// Returns the output pin type
    pub fn output_type(&self) -> &'static str {
        match self {
            Self::FloatSlider { .. } => "float",
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

impl NodeType {
    pub fn name(&self) -> &str {
        match self {
            Self::Workflow(name) => name.as_str(),
            Self::Input(input) => match input {
                InputNodeType::FloatSlider { config, .. } => config.label.as_str(),
                InputNodeType::ColorPicker { .. } => "Color Picker",
                InputNodeType::ColorPreset { .. } => "Color Preset",
            },
            Self::Config(config) => match config {
                ConfigNodeType::CornerRadius => "Corner Radius",
                ConfigNodeType::Opacity => "Opacity",
                ConfigNodeType::BorderWidth => "Border Width",
                ConfigNodeType::FillColor => "Fill Color",
                ConfigNodeType::EdgeThickness => "Edge Thickness",
                ConfigNodeType::EdgeColor => "Edge Color",
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
