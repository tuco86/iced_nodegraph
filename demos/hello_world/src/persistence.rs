//! State persistence for the hello_world demo.
//!
//! Saves graph state to OS-appropriate directories:
//! - Windows: `%APPDATA%\iced_nodegraph\demo\state.json`
//! - Linux: `~/.local/share/iced_nodegraph/demo/state.json`
//! - macOS: `~/Library/Application Support/iced_nodegraph/demo/state.json`
//!
//! Uses NanoID-based string IDs for nodes and edges, with string labels for pins.

use iced::{Point, Theme};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::ids::{EdgeId, NodeId};
use crate::nodes::{
    BackgroundConfigInputs, BoolToggleConfig, ConfigNodeType, EdgeConfigInputs, FloatSliderConfig,
    InputNodeType, IntSliderConfig, MathNodeState, MathOperation, NodeConfigInputs, NodeType,
    PatternType, PatternTypeSelection, PinConfigInputs, ShadowConfigInputs,
};
use iced_nodegraph::{EdgeCurve, PinShape};

/// Saved state format for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub nodes: Vec<SavedNode>,
    pub edges: Vec<SavedEdge>,
    pub theme: String,
    pub camera_position: (f32, f32),
    pub camera_zoom: f32,
    /// Window position (x, y) - None for old save files
    #[serde(default)]
    pub window_position: Option<(i32, i32)>,
    /// Window size (width, height) - None for old save files
    #[serde(default)]
    pub window_size: Option<(u32, u32)>,
}

/// Saved node with ID, position, and type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedNode {
    /// Unique node identifier (NanoID)
    pub id: NodeId,
    pub x: f32,
    pub y: f32,
    pub node_type: SavedNodeType,
}

/// Serializable node type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SavedNodeType {
    Workflow {
        name: String,
    },
    FloatSlider {
        min: f32,
        max: f32,
        step: f32,
        label: String,
        value: f32,
    },
    IntSlider {
        min: i32,
        max: i32,
        label: String,
        value: i32,
    },
    BoolToggle {
        label: String,
        toggle_label: String,
        value: bool,
    },
    ColorPicker {
        r: f32,
        g: f32,
        b: f32,
    },
    ColorPreset {
        r: f32,
        g: f32,
        b: f32,
    },
    EdgeCurveSelector {
        curve: String,
    },
    PinShapeSelector {
        shape: String,
    },
    PatternTypeSelector {
        pattern: String,
    },
    BackgroundPatternSelector {
        pattern: String,
    },
    NodeConfig,
    EdgeConfig,
    ShadowConfig,
    PinConfig,
    BackgroundConfig,
    ApplyToGraph,
    ApplyToNode,
    Math {
        operation: String,
    },
}

/// Saved edge connection with stable IDs.
/// Uses String for serialization - converted to/from &'static str at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEdge {
    /// Unique edge identifier (NanoID)
    pub id: EdgeId,
    /// Source node ID
    pub from_node: NodeId,
    /// Source pin label (unique within source node)
    pub from_pin: String,
    /// Target node ID
    pub to_node: NodeId,
    /// Target pin label (unique within target node)
    pub to_pin: String,
}

/// Edge data for in-memory representation.
/// Uses &'static str for pin labels to match the compile-time pin constants.
#[derive(Debug, Clone)]
pub struct EdgeData {
    pub from_node: NodeId,
    pub from_pin: &'static str,
    pub to_node: NodeId,
    pub to_pin: &'static str,
}

/// Maps a string pin label to its static equivalent.
/// Returns the static label if found, or leaks the string to create a &'static str.
/// This is safe because pin labels are a fixed set defined at compile time.
pub fn to_static_pin_label(label: &str) -> &'static str {
    use crate::nodes::pins::*;

    // Check all known pin labels
    match label {
        // Workflow pins
        s if s == workflow::ON_EMAIL => workflow::ON_EMAIL,
        s if s == workflow::EMAIL => workflow::EMAIL,
        s if s == workflow::SUBJECT => workflow::SUBJECT,
        s if s == workflow::DATETIME => workflow::DATETIME,
        s if s == workflow::BODY => workflow::BODY,
        s if s == workflow::INPUT => workflow::INPUT,
        s if s == workflow::MATCHES => workflow::MATCHES,
        s if s == workflow::TITLE => workflow::TITLE,
        s if s == workflow::DESCRIPTION => workflow::DESCRIPTION,
        // Input pins
        s if s == input::VALUE => input::VALUE,
        s if s == input::COLOR => input::COLOR,
        // Config pins
        s if s == config::CONFIG => config::CONFIG,
        s if s == config::START => config::START,
        s if s == config::END => config::END,
        s if s == config::THICK => config::THICK,
        s if s == config::CURVE => config::CURVE,
        s if s == config::PATTERN => config::PATTERN,
        s if s == config::DASH => config::DASH,
        s if s == config::GAP => config::GAP,
        s if s == config::ANGLE => config::ANGLE,
        s if s == config::ANIMATED => config::ANIMATED,
        s if s == config::SPEED => config::SPEED,
        // Border config pins
        s if s == config::BORDER => config::BORDER,
        s if s == config::BORDER_WIDTH => config::BORDER_WIDTH,
        s if s == config::BORDER_GAP => config::BORDER_GAP,
        s if s == config::BORDER_COLOR => config::BORDER_COLOR,
        // Shadow config pins
        s if s == config::SHADOW => config::SHADOW,
        s if s == config::SHADOW_BLUR => config::SHADOW_BLUR,
        s if s == config::SHADOW_OFFSET => config::SHADOW_OFFSET,
        s if s == config::SHADOW_COLOR => config::SHADOW_COLOR,
        // Node config pins
        s if s == config::BG_COLOR => config::BG_COLOR,
        s if s == config::RADIUS => config::RADIUS,
        s if s == config::WIDTH => config::WIDTH,
        s if s == config::COLOR => config::COLOR,
        s if s == config::OPACITY => config::OPACITY,
        // Pin config pins
        s if s == config::SIZE => config::SIZE,
        s if s == config::SHAPE => config::SHAPE,
        s if s == config::GLOW => config::GLOW,
        s if s == config::PULSE => config::PULSE,
        // Apply node pins
        s if s == config::NODE_CONFIG => config::NODE_CONFIG,
        s if s == config::EDGE_CONFIG => config::EDGE_CONFIG,
        s if s == config::PIN_CONFIG => config::PIN_CONFIG,
        s if s == config::ON => config::ON,
        s if s == config::TARGET => config::TARGET,
        // Typed config output pins
        s if s == config::NODE_OUT => config::NODE_OUT,
        s if s == config::EDGE_OUT => config::EDGE_OUT,
        s if s == config::PIN_OUT => config::PIN_OUT,
        s if s == config::SHADOW_OUT => config::SHADOW_OUT,
        // Math pins
        s if s == math::A => math::A,
        s if s == math::B => math::B,
        s if s == math::RESULT => math::RESULT,
        // Unknown - leak the string (should not happen in normal use)
        _ => Box::leak(label.to_string().into_boxed_str()),
    }
}

impl SavedState {
    /// Creates a saved state from application state.
    ///
    /// Takes nodes as a HashMap with NodeId keys, and edges as a HashMap with EdgeId keys.
    pub fn from_app(
        nodes: &HashMap<NodeId, (Point, NodeType)>,
        node_order: &[NodeId],
        edges: &HashMap<EdgeId, EdgeData>,
        edge_order: &[EdgeId],
        theme: &Theme,
        camera_position: Point,
        camera_zoom: f32,
        window_position: Option<(i32, i32)>,
        window_size: Option<(u32, u32)>,
    ) -> Self {
        Self {
            nodes: node_order
                .iter()
                .filter_map(|id| {
                    nodes.get(id).map(|(pos, node_type)| SavedNode {
                        id: id.clone(),
                        x: pos.x,
                        y: pos.y,
                        node_type: SavedNodeType::from(node_type),
                    })
                })
                .collect(),
            edges: edge_order
                .iter()
                .filter_map(|id| {
                    edges.get(id).map(|e| SavedEdge {
                        id: id.clone(),
                        from_node: e.from_node.clone(),
                        from_pin: e.from_pin.to_string(),
                        to_node: e.to_node.clone(),
                        to_pin: e.to_pin.to_string(),
                    })
                })
                .collect(),
            theme: theme_to_string(theme),
            camera_position: (camera_position.x, camera_position.y),
            camera_zoom,
            window_position,
            window_size,
        }
    }

    /// Converts saved state back to application types.
    ///
    /// Returns nodes as HashMap, node order, edges as HashMap, edge order, and other settings.
    #[allow(clippy::type_complexity)]
    pub fn to_app(
        &self,
    ) -> (
        HashMap<NodeId, (Point, NodeType)>,
        Vec<NodeId>,
        HashMap<EdgeId, EdgeData>,
        Vec<EdgeId>,
        Theme,
        Point,
        f32,
        Option<(i32, i32)>,
        Option<(u32, u32)>,
    ) {
        let mut nodes = HashMap::new();
        let mut node_order = Vec::new();

        for n in &self.nodes {
            nodes.insert(
                n.id.clone(),
                (Point::new(n.x, n.y), n.node_type.to_node_type()),
            );
            node_order.push(n.id.clone());
        }

        let mut edges = HashMap::new();
        let mut edge_order = Vec::new();

        for e in &self.edges {
            edges.insert(
                e.id.clone(),
                EdgeData {
                    from_node: e.from_node.clone(),
                    from_pin: to_static_pin_label(&e.from_pin),
                    to_node: e.to_node.clone(),
                    to_pin: to_static_pin_label(&e.to_pin),
                },
            );
            edge_order.push(e.id.clone());
        }

        let theme = string_to_theme(&self.theme);
        let camera_pos = Point::new(self.camera_position.0, self.camera_position.1);

        (
            nodes,
            node_order,
            edges,
            edge_order,
            theme,
            camera_pos,
            self.camera_zoom,
            self.window_position,
            self.window_size,
        )
    }
}

impl SavedNodeType {
    fn from(node_type: &NodeType) -> Self {
        match node_type {
            NodeType::Workflow(name) => SavedNodeType::Workflow { name: name.clone() },
            NodeType::Input(input) => match input {
                InputNodeType::FloatSlider { config, value } => SavedNodeType::FloatSlider {
                    min: config.min,
                    max: config.max,
                    step: config.step,
                    label: config.label.clone(),
                    value: *value,
                },
                InputNodeType::IntSlider { config, value } => SavedNodeType::IntSlider {
                    min: config.min,
                    max: config.max,
                    label: config.label.clone(),
                    value: *value,
                },
                InputNodeType::BoolToggle { config, value } => SavedNodeType::BoolToggle {
                    label: config.label.clone(),
                    toggle_label: config.toggle_label.clone(),
                    value: *value,
                },
                InputNodeType::ColorPicker { color } => SavedNodeType::ColorPicker {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                },
                InputNodeType::ColorPreset { color } => SavedNodeType::ColorPreset {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                },
                InputNodeType::EdgeCurveSelector { value } => SavedNodeType::EdgeCurveSelector {
                    curve: edge_curve_to_string(*value),
                },
                InputNodeType::PinShapeSelector { value } => SavedNodeType::PinShapeSelector {
                    shape: pin_shape_to_string(*value),
                },
                InputNodeType::PatternTypeSelector { value } => {
                    SavedNodeType::PatternTypeSelector {
                        pattern: pattern_type_to_string(value),
                    }
                }
                InputNodeType::BackgroundPatternSelector { value } => {
                    SavedNodeType::BackgroundPatternSelector {
                        pattern: background_pattern_to_string(value),
                    }
                }
            },
            NodeType::Config(config) => match config {
                ConfigNodeType::NodeConfig(_) => SavedNodeType::NodeConfig,
                ConfigNodeType::EdgeConfig(_) => SavedNodeType::EdgeConfig,
                ConfigNodeType::ShadowConfig(_) => SavedNodeType::ShadowConfig,
                ConfigNodeType::PinConfig(_) => SavedNodeType::PinConfig,
                ConfigNodeType::BackgroundConfig(_) => SavedNodeType::BackgroundConfig,
                ConfigNodeType::ApplyToGraph { .. } => SavedNodeType::ApplyToGraph,
                ConfigNodeType::ApplyToNode { .. } => SavedNodeType::ApplyToNode,
            },
            NodeType::Math(state) => SavedNodeType::Math {
                operation: math_op_to_string(&state.operation),
            },
        }
    }

    fn to_node_type(&self) -> NodeType {
        match self {
            SavedNodeType::Workflow { name } => NodeType::Workflow(name.clone()),
            SavedNodeType::FloatSlider {
                min,
                max,
                step,
                label,
                value,
            } => NodeType::Input(InputNodeType::FloatSlider {
                config: FloatSliderConfig {
                    min: *min,
                    max: *max,
                    step: *step,
                    label: label.clone(),
                    min_edit: None,
                    max_edit: None,
                    step_edit: None,
                },
                value: *value,
            }),
            SavedNodeType::IntSlider {
                min,
                max,
                label,
                value,
            } => NodeType::Input(InputNodeType::IntSlider {
                config: IntSliderConfig {
                    min: *min,
                    max: *max,
                    label: label.clone(),
                },
                value: *value,
            }),
            SavedNodeType::BoolToggle {
                label,
                toggle_label,
                value,
            } => NodeType::Input(InputNodeType::BoolToggle {
                config: BoolToggleConfig {
                    label: label.clone(),
                    toggle_label: toggle_label.clone(),
                },
                value: *value,
            }),
            SavedNodeType::ColorPicker { r, g, b } => NodeType::Input(InputNodeType::ColorPicker {
                color: iced::Color::from_rgb(*r, *g, *b),
            }),
            SavedNodeType::ColorPreset { r, g, b } => NodeType::Input(InputNodeType::ColorPreset {
                color: iced::Color::from_rgb(*r, *g, *b),
            }),
            SavedNodeType::EdgeCurveSelector { curve } => {
                NodeType::Input(InputNodeType::EdgeCurveSelector {
                    value: string_to_edge_curve(curve),
                })
            }
            SavedNodeType::PinShapeSelector { shape } => {
                NodeType::Input(InputNodeType::PinShapeSelector {
                    value: string_to_pin_shape(shape),
                })
            }
            SavedNodeType::PatternTypeSelector { pattern } => {
                NodeType::Input(InputNodeType::PatternTypeSelector {
                    value: string_to_pattern_type(pattern),
                })
            }
            SavedNodeType::BackgroundPatternSelector { pattern } => {
                NodeType::Input(InputNodeType::BackgroundPatternSelector {
                    value: string_to_background_pattern(pattern),
                })
            }
            SavedNodeType::NodeConfig => {
                NodeType::Config(ConfigNodeType::NodeConfig(NodeConfigInputs::default()))
            }
            SavedNodeType::EdgeConfig => {
                NodeType::Config(ConfigNodeType::EdgeConfig(EdgeConfigInputs::default()))
            }
            SavedNodeType::ShadowConfig => {
                NodeType::Config(ConfigNodeType::ShadowConfig(ShadowConfigInputs::default()))
            }
            SavedNodeType::PinConfig => {
                NodeType::Config(ConfigNodeType::PinConfig(PinConfigInputs::default()))
            }
            SavedNodeType::BackgroundConfig => NodeType::Config(ConfigNodeType::BackgroundConfig(
                BackgroundConfigInputs::default(),
            )),
            SavedNodeType::ApplyToGraph => NodeType::Config(ConfigNodeType::ApplyToGraph {
                has_node_config: false,
                has_edge_config: false,
                has_pin_config: false,
                has_background_config: false,
            }),
            SavedNodeType::ApplyToNode => NodeType::Config(ConfigNodeType::ApplyToNode {
                has_node_config: false,
                target_id: None,
            }),
            SavedNodeType::Math { operation } => {
                NodeType::Math(MathNodeState::new(string_to_math_op(operation)))
            }
        }
    }
}

/// Returns the path to the state file.
pub fn state_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "iced_nodegraph")
        .map(|dirs| dirs.data_dir().join("demo").join("state.json"))
}

/// Saves state to disk.
pub fn save_state(state: &SavedState) -> Result<(), String> {
    let path = state_file_path().ok_or("Could not determine data directory")?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let json =
        serde_json::to_string_pretty(state).map_err(|e| format!("Serialization error: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

/// Loads state from disk.
pub fn load_state() -> Result<SavedState, String> {
    let path = state_file_path().ok_or("Could not determine data directory")?;

    if !path.exists() {
        return Err("No saved state found".to_string());
    }

    let json = fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    serde_json::from_str(&json).map_err(|e| format!("Deserialization error: {}", e))
}

// Theme serialization helpers
fn theme_to_string(theme: &Theme) -> String {
    match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
        Theme::Dracula => "Dracula",
        Theme::Nord => "Nord",
        Theme::SolarizedLight => "SolarizedLight",
        Theme::SolarizedDark => "SolarizedDark",
        Theme::GruvboxLight => "GruvboxLight",
        Theme::GruvboxDark => "GruvboxDark",
        Theme::CatppuccinLatte => "CatppuccinLatte",
        Theme::CatppuccinFrappe => "CatppuccinFrappe",
        Theme::CatppuccinMacchiato => "CatppuccinMacchiato",
        Theme::CatppuccinMocha => "CatppuccinMocha",
        Theme::TokyoNight => "TokyoNight",
        Theme::TokyoNightStorm => "TokyoNightStorm",
        Theme::TokyoNightLight => "TokyoNightLight",
        Theme::KanagawaWave => "KanagawaWave",
        Theme::KanagawaDragon => "KanagawaDragon",
        Theme::KanagawaLotus => "KanagawaLotus",
        Theme::Moonfly => "Moonfly",
        Theme::Nightfly => "Nightfly",
        Theme::Oxocarbon => "Oxocarbon",
        Theme::Ferra => "Ferra",
        _ => "CatppuccinFrappe",
    }
    .to_string()
}

fn string_to_theme(s: &str) -> Theme {
    match s {
        "Dark" => Theme::Dark,
        "Light" => Theme::Light,
        "Dracula" => Theme::Dracula,
        "Nord" => Theme::Nord,
        "SolarizedLight" => Theme::SolarizedLight,
        "SolarizedDark" => Theme::SolarizedDark,
        "GruvboxLight" => Theme::GruvboxLight,
        "GruvboxDark" => Theme::GruvboxDark,
        "CatppuccinLatte" => Theme::CatppuccinLatte,
        "CatppuccinFrappe" => Theme::CatppuccinFrappe,
        "CatppuccinMacchiato" => Theme::CatppuccinMacchiato,
        "CatppuccinMocha" => Theme::CatppuccinMocha,
        "TokyoNight" => Theme::TokyoNight,
        "TokyoNightStorm" => Theme::TokyoNightStorm,
        "TokyoNightLight" => Theme::TokyoNightLight,
        "KanagawaWave" => Theme::KanagawaWave,
        "KanagawaDragon" => Theme::KanagawaDragon,
        "KanagawaLotus" => Theme::KanagawaLotus,
        "Moonfly" => Theme::Moonfly,
        "Nightfly" => Theme::Nightfly,
        "Oxocarbon" => Theme::Oxocarbon,
        "Ferra" => Theme::Ferra,
        _ => Theme::CatppuccinFrappe,
    }
}

// Enum serialization helpers
fn edge_curve_to_string(curve: EdgeCurve) -> String {
    match curve {
        EdgeCurve::BezierCubic => "BezierCubic",
        EdgeCurve::BezierQuadratic => "BezierQuadratic",
        EdgeCurve::Orthogonal => "Orthogonal",
        EdgeCurve::OrthogonalSmooth { radius } => return format!("OrthogonalSmooth:{}", radius),
        EdgeCurve::Line => "Line",
    }
    .to_string()
}

fn string_to_edge_curve(s: &str) -> EdgeCurve {
    if s.starts_with("OrthogonalSmooth:") {
        let radius = s
            .strip_prefix("OrthogonalSmooth:")
            .and_then(|r| r.parse::<f32>().ok())
            .unwrap_or(10.0);
        return EdgeCurve::OrthogonalSmooth { radius };
    }
    match s {
        "BezierCubic" => EdgeCurve::BezierCubic,
        "BezierQuadratic" => EdgeCurve::BezierQuadratic,
        "Orthogonal" => EdgeCurve::Orthogonal,
        "Line" => EdgeCurve::Line,
        _ => EdgeCurve::BezierCubic,
    }
}

fn pin_shape_to_string(shape: PinShape) -> String {
    match shape {
        PinShape::Circle => "Circle",
        PinShape::Square => "Square",
        PinShape::Diamond => "Diamond",
        PinShape::Triangle => "Triangle",
    }
    .to_string()
}

fn string_to_pin_shape(s: &str) -> PinShape {
    match s {
        "Circle" => PinShape::Circle,
        "Square" => PinShape::Square,
        "Diamond" => PinShape::Diamond,
        "Triangle" => PinShape::Triangle,
        _ => PinShape::Circle,
    }
}

fn pattern_type_to_string(pattern: &PatternType) -> String {
    match pattern {
        PatternType::Solid => "Solid",
        PatternType::Dashed => "Dashed",
        PatternType::Arrowed => "Arrowed",
        PatternType::Angled => "Angled",
        PatternType::Dotted => "Dotted",
        PatternType::DashDotted => "DashDotted",
    }
    .to_string()
}

fn string_to_pattern_type(s: &str) -> PatternType {
    match s {
        "Solid" => PatternType::Solid,
        "Dashed" => PatternType::Dashed,
        "Arrowed" => PatternType::Arrowed,
        "Angled" => PatternType::Angled,
        "Dotted" => PatternType::Dotted,
        "DashDotted" => PatternType::DashDotted,
        _ => PatternType::Solid,
    }
}

fn background_pattern_to_string(pattern: &PatternTypeSelection) -> String {
    match pattern {
        PatternTypeSelection::None => "None",
        PatternTypeSelection::Grid => "Grid",
        PatternTypeSelection::Hex => "Hex",
        PatternTypeSelection::Triangle => "Triangle",
        PatternTypeSelection::Dots => "Dots",
        PatternTypeSelection::Lines => "Lines",
        PatternTypeSelection::Crosshatch => "Crosshatch",
    }
    .to_string()
}

fn string_to_background_pattern(s: &str) -> PatternTypeSelection {
    match s {
        "None" => PatternTypeSelection::None,
        "Grid" => PatternTypeSelection::Grid,
        "Hex" => PatternTypeSelection::Hex,
        "Triangle" => PatternTypeSelection::Triangle,
        "Dots" => PatternTypeSelection::Dots,
        "Lines" => PatternTypeSelection::Lines,
        "Crosshatch" => PatternTypeSelection::Crosshatch,
        _ => PatternTypeSelection::Grid,
    }
}

fn math_op_to_string(op: &MathOperation) -> String {
    match op {
        MathOperation::Add => "Add",
        MathOperation::Subtract => "Subtract",
        MathOperation::Multiply => "Multiply",
        MathOperation::Divide => "Divide",
    }
    .to_string()
}

fn string_to_math_op(s: &str) -> MathOperation {
    match s {
        "Add" => MathOperation::Add,
        "Subtract" => MathOperation::Subtract,
        "Multiply" => MathOperation::Multiply,
        "Divide" => MathOperation::Divide,
        _ => MathOperation::Add,
    }
}
