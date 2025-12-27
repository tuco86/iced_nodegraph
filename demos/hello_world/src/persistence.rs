//! State persistence for the hello_world demo.
//!
//! Saves graph state to OS-appropriate directories:
//! - Windows: `%APPDATA%\iced_nodegraph\demo\state.json`
//! - Linux: `~/.local/share/iced_nodegraph/demo/state.json`
//! - macOS: `~/Library/Application Support/iced_nodegraph/demo/state.json`

use iced::{Point, Theme};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::nodes::{
    BoolToggleConfig, ConfigNodeType, EdgeConfigInputs, FloatSliderConfig, InputNodeType,
    IntSliderConfig, MathNodeState, MathOperation, NodeConfigInputs, NodeType, PatternType,
    PinConfigInputs, ShadowConfigInputs,
};
use iced_nodegraph::{EdgeCurve, PinReference, PinShape};

/// Saved state format for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub nodes: Vec<SavedNode>,
    pub edges: Vec<SavedEdge>,
    pub theme: String,
    pub camera_position: (f32, f32),
    pub camera_zoom: f32,
}

/// Saved node with position and type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedNode {
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
    NodeConfig,
    EdgeConfig,
    ShadowConfig,
    PinConfig,
    ApplyToGraph,
    ApplyToNode,
    Math {
        operation: String,
    },
}

/// Saved edge connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEdge {
    pub from_node: usize,
    pub from_pin: usize,
    pub to_node: usize,
    pub to_pin: usize,
}

impl SavedState {
    /// Creates a saved state from application state.
    pub fn from_app(
        nodes: &[(Point, NodeType)],
        edges: &[(PinReference, PinReference)],
        theme: &Theme,
        camera_position: Point,
        camera_zoom: f32,
    ) -> Self {
        Self {
            nodes: nodes
                .iter()
                .map(|(pos, node_type)| SavedNode {
                    x: pos.x,
                    y: pos.y,
                    node_type: SavedNodeType::from(node_type),
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(from, to)| SavedEdge {
                    from_node: from.node_id,
                    from_pin: from.pin_id,
                    to_node: to.node_id,
                    to_pin: to.pin_id,
                })
                .collect(),
            theme: theme_to_string(theme),
            camera_position: (camera_position.x, camera_position.y),
            camera_zoom,
        }
    }

    /// Converts saved state back to application types.
    pub fn to_app(
        &self,
    ) -> (
        Vec<(Point, NodeType)>,
        Vec<(PinReference, PinReference)>,
        Theme,
        Point,
        f32,
    ) {
        let nodes = self
            .nodes
            .iter()
            .map(|n| (Point::new(n.x, n.y), n.node_type.to_node_type()))
            .collect();

        let edges = self
            .edges
            .iter()
            .map(|e| {
                (
                    PinReference::new(e.from_node, e.from_pin),
                    PinReference::new(e.to_node, e.to_pin),
                )
            })
            .collect();

        let theme = string_to_theme(&self.theme);
        let camera_pos = Point::new(self.camera_position.0, self.camera_position.1);

        (nodes, edges, theme, camera_pos, self.camera_zoom)
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
            },
            NodeType::Config(config) => match config {
                ConfigNodeType::NodeConfig(_) => SavedNodeType::NodeConfig,
                ConfigNodeType::EdgeConfig(_) => SavedNodeType::EdgeConfig,
                ConfigNodeType::ShadowConfig(_) => SavedNodeType::ShadowConfig,
                ConfigNodeType::PinConfig(_) => SavedNodeType::PinConfig,
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
            SavedNodeType::ApplyToGraph => NodeType::Config(ConfigNodeType::ApplyToGraph {
                has_node_config: false,
                has_edge_config: false,
                has_pin_config: false,
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
