#![allow(clippy::too_many_arguments)]

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
    BoolToggleConfig, ColorQuadNode, ConfigNodeType, EdgeConfigInputs, EdgeSections,
    FloatSliderConfig, GraphConfigInputs, InputNodeType, IntSliderConfig, MathNodeState,
    MathOperation, NodeConfigInputs, NodeSections, NodeType, PatternType, PinConfigInputs,
    Vec2Node,
};
use iced_nodegraph::{EdgeCurve, PinShape, TilingKind};

/// Saved section expansion state for EdgeConfig nodes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedEdgeSections {
    pub stroke: bool,
    pub pattern: bool,
    pub border: bool,
    pub shadow: bool,
}

impl From<&EdgeSections> for SavedEdgeSections {
    fn from(s: &EdgeSections) -> Self {
        Self {
            stroke: s.stroke,
            pattern: s.pattern,
            border: s.border,
            shadow: s.shadow,
        }
    }
}

impl SavedEdgeSections {
    fn to_edge_sections(&self) -> EdgeSections {
        EdgeSections {
            stroke: self.stroke,
            pattern: self.pattern,
            border: self.border,
            shadow: self.shadow,
        }
    }
}

/// Saved section expansion state for NodeConfig nodes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedNodeSections {
    pub fill: bool,
    pub border: bool,
    #[serde(default)]
    pub pattern: bool,
    #[serde(default)]
    pub shadow: bool,
}

impl From<&NodeSections> for SavedNodeSections {
    fn from(s: &NodeSections) -> Self {
        Self {
            fill: s.fill,
            border: s.border,
            pattern: s.pattern,
            shadow: s.shadow,
        }
    }
}

impl SavedNodeSections {
    fn to_node_sections(&self) -> NodeSections {
        NodeSections {
            fill: self.fill,
            border: self.border,
            pattern: self.pattern,
            shadow: self.shadow,
        }
    }
}

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
    /// Section expansion states for EdgeConfig nodes
    #[serde(default)]
    pub edge_config_sections: HashMap<NodeId, SavedEdgeSections>,
    /// Section expansion states for NodeConfig nodes
    #[serde(default)]
    pub node_config_sections: HashMap<NodeId, SavedNodeSections>,
    /// Whether window was maximized - None for old save files
    #[serde(default)]
    pub window_maximized: Option<bool>,
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
    TilingKindSelector {
        kind: String,
    },
    NodeConfig,
    EdgeConfig,
    PinConfig,
    GraphConfig,
    ApplyToGraph,
    ApplyToNode,
    Math {
        operation: String,
    },
    ColorQuad,
    Vec2,
    Theme,
    ThemeExtended,
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
        // Shared config plumbing + apply node pins
        s if s == cfg::CONFIG => cfg::CONFIG,
        s if s == cfg::NODE_OUT => cfg::NODE_OUT,
        s if s == cfg::EDGE_OUT => cfg::EDGE_OUT,
        s if s == cfg::PIN_OUT => cfg::PIN_OUT,
        s if s == cfg::NODE_CONFIG => cfg::NODE_CONFIG,
        s if s == cfg::EDGE_CONFIG => cfg::EDGE_CONFIG,
        s if s == cfg::PIN_CONFIG => cfg::PIN_CONFIG,
        s if s == cfg::GRAPH_OUT => cfg::GRAPH_OUT,
        s if s == cfg::GRAPH_CONFIG => cfg::GRAPH_CONFIG,
        s if s == cfg::ON => cfg::ON,
        s if s == cfg::TARGET => cfg::TARGET,
        // Graph config field pins
        s if s == graph::BACKGROUND => graph::BACKGROUND,
        s if s == graph::TILING_KIND => graph::TILING_KIND,
        s if s == graph::SPACING => graph::SPACING,
        s if s == graph::THICKNESS => graph::THICKNESS,
        s if s == graph::LINE_COLOR => graph::LINE_COLOR,
        // Node config field pins (snake_case). Several label strings are shared
        // across node/pin/edge nodes (e.g. "border_color", "pattern"); the first
        // matching arm wins and returns the same string, so the duplicates below
        // are harmless.
        s if s == node::FILL_COLOR => node::FILL_COLOR,
        s if s == node::CORNER_RADIUS => node::CORNER_RADIUS,
        s if s == node::OPACITY => node::OPACITY,
        s if s == node::BORDER_COLOR => node::BORDER_COLOR,
        s if s == node::BORDER_WIDTH => node::BORDER_WIDTH,
        s if s == node::BORDER_OUTLINE_WIDTH => node::BORDER_OUTLINE_WIDTH,
        s if s == node::BORDER_OUTLINE_COLOR => node::BORDER_OUTLINE_COLOR,
        s if s == node::PATTERN => node::PATTERN,
        s if s == node::DASH => node::DASH,
        s if s == node::GAP => node::GAP,
        s if s == node::ANGLE => node::ANGLE,
        s if s == node::SPEED => node::SPEED,
        s if s == node::SHADOW_COLOR => node::SHADOW_COLOR,
        s if s == node::SHADOW_DISTANCE => node::SHADOW_DISTANCE,
        s if s == node::SHADOW_OFFSET => node::SHADOW_OFFSET,
        // Pin config field pins
        s if s == pin::COLOR => pin::COLOR,
        s if s == pin::RADIUS => pin::RADIUS,
        s if s == pin::SHAPE => pin::SHAPE,
        // Edge config field pins
        s if s == edge::STROKE_COLOR => edge::STROKE_COLOR,
        s if s == edge::THICKNESS => edge::THICKNESS,
        s if s == edge::CURVE => edge::CURVE,
        s if s == edge::STROKE_OUTLINE_WIDTH => edge::STROKE_OUTLINE_WIDTH,
        s if s == edge::STROKE_OUTLINE_COLOR => edge::STROKE_OUTLINE_COLOR,
        s if s == edge::BORDER_GAP => edge::BORDER_GAP,
        s if s == edge::BORDER_BACKGROUND => edge::BORDER_BACKGROUND,
        s if s == edge::SHADOW_BLUR => edge::SHADOW_BLUR,
        s if s == edge::SHADOW_EXPAND => edge::SHADOW_EXPAND,
        // Theme node output pins (basic palette)
        s if s == theme::BACKGROUND => theme::BACKGROUND,
        s if s == theme::TEXT => theme::TEXT,
        s if s == theme::PRIMARY => theme::PRIMARY,
        s if s == theme::SUCCESS => theme::SUCCESS,
        s if s == theme::WARNING => theme::WARNING,
        s if s == theme::DANGER => theme::DANGER,
        // Theme Extended node output pins (extended palette)
        s if s == theme_ext::BACKGROUND_BASE => theme_ext::BACKGROUND_BASE,
        s if s == theme_ext::BACKGROUND_WEAK => theme_ext::BACKGROUND_WEAK,
        s if s == theme_ext::BACKGROUND_STRONG => theme_ext::BACKGROUND_STRONG,
        s if s == theme_ext::PRIMARY_BASE => theme_ext::PRIMARY_BASE,
        s if s == theme_ext::PRIMARY_WEAK => theme_ext::PRIMARY_WEAK,
        s if s == theme_ext::PRIMARY_STRONG => theme_ext::PRIMARY_STRONG,
        s if s == theme_ext::SECONDARY_BASE => theme_ext::SECONDARY_BASE,
        s if s == theme_ext::SECONDARY_WEAK => theme_ext::SECONDARY_WEAK,
        s if s == theme_ext::SECONDARY_STRONG => theme_ext::SECONDARY_STRONG,
        s if s == theme_ext::SUCCESS_BASE => theme_ext::SUCCESS_BASE,
        s if s == theme_ext::SUCCESS_WEAK => theme_ext::SUCCESS_WEAK,
        s if s == theme_ext::SUCCESS_STRONG => theme_ext::SUCCESS_STRONG,
        s if s == theme_ext::WARNING_BASE => theme_ext::WARNING_BASE,
        s if s == theme_ext::WARNING_WEAK => theme_ext::WARNING_WEAK,
        s if s == theme_ext::WARNING_STRONG => theme_ext::WARNING_STRONG,
        s if s == theme_ext::DANGER_BASE => theme_ext::DANGER_BASE,
        s if s == theme_ext::DANGER_WEAK => theme_ext::DANGER_WEAK,
        s if s == theme_ext::DANGER_STRONG => theme_ext::DANGER_STRONG,
        // Builder node pins
        s if s == build::NEAR_START => build::NEAR_START,
        s if s == build::NEAR_END => build::NEAR_END,
        s if s == build::FAR_START => build::FAR_START,
        s if s == build::FAR_END => build::FAR_END,
        s if s == build::QUAD_OUT => build::QUAD_OUT,
        s if s == build::X => build::X,
        s if s == build::Y => build::Y,
        s if s == build::VEC2_OUT => build::VEC2_OUT,
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
        edge_config_sections: &HashMap<NodeId, EdgeSections>,
        node_config_sections: &HashMap<NodeId, NodeSections>,
        window_maximized: Option<bool>,
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
            edge_config_sections: edge_config_sections
                .iter()
                .map(|(id, s)| (id.clone(), SavedEdgeSections::from(s)))
                .collect(),
            node_config_sections: node_config_sections
                .iter()
                .map(|(id, s)| (id.clone(), SavedNodeSections::from(s)))
                .collect(),
            window_maximized,
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
        HashMap<NodeId, EdgeSections>,
        HashMap<NodeId, NodeSections>,
        Option<bool>,
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

        let edge_config_sections = self
            .edge_config_sections
            .iter()
            .map(|(id, s)| (id.clone(), s.to_edge_sections()))
            .collect();

        let node_config_sections = self
            .node_config_sections
            .iter()
            .map(|(id, s)| (id.clone(), s.to_node_sections()))
            .collect();

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
            edge_config_sections,
            node_config_sections,
            self.window_maximized,
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
                InputNodeType::TilingKindSelector { value } => SavedNodeType::TilingKindSelector {
                    kind: tiling_kind_to_string(*value),
                },
            },
            NodeType::Config(config) => match config {
                ConfigNodeType::NodeConfig(_) => SavedNodeType::NodeConfig,
                ConfigNodeType::EdgeConfig(_) => SavedNodeType::EdgeConfig,
                ConfigNodeType::PinConfig(_) => SavedNodeType::PinConfig,
                ConfigNodeType::GraphConfig(_) => SavedNodeType::GraphConfig,
                ConfigNodeType::ApplyToGraph { .. } => SavedNodeType::ApplyToGraph,
                ConfigNodeType::ApplyToNode { .. } => SavedNodeType::ApplyToNode,
            },
            NodeType::Math(state) => SavedNodeType::Math {
                operation: math_op_to_string(&state.operation),
            },
            NodeType::ColorQuad(_) => SavedNodeType::ColorQuad,
            NodeType::Vec2(_) => SavedNodeType::Vec2,
            NodeType::Theme => SavedNodeType::Theme,
            NodeType::ThemeExtended => SavedNodeType::ThemeExtended,
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
            SavedNodeType::TilingKindSelector { kind } => {
                NodeType::Input(InputNodeType::TilingKindSelector {
                    value: string_to_tiling_kind(kind),
                })
            }
            SavedNodeType::NodeConfig => {
                NodeType::Config(ConfigNodeType::NodeConfig(NodeConfigInputs::default()))
            }
            SavedNodeType::EdgeConfig => {
                NodeType::Config(ConfigNodeType::EdgeConfig(EdgeConfigInputs::default()))
            }
            SavedNodeType::PinConfig => {
                NodeType::Config(ConfigNodeType::PinConfig(PinConfigInputs::default()))
            }
            SavedNodeType::GraphConfig => {
                NodeType::Config(ConfigNodeType::GraphConfig(GraphConfigInputs::default()))
            }
            SavedNodeType::ApplyToGraph => NodeType::Config(ConfigNodeType::ApplyToGraph {
                has_node_config: false,
                has_edge_config: false,
                has_pin_config: false,
                has_graph_config: false,
            }),
            SavedNodeType::ApplyToNode => NodeType::Config(ConfigNodeType::ApplyToNode {
                has_node_config: false,
                target_id: None,
            }),
            SavedNodeType::Math { operation } => {
                NodeType::Math(MathNodeState::new(string_to_math_op(operation)))
            }
            SavedNodeType::ColorQuad => NodeType::ColorQuad(ColorQuadNode::default()),
            SavedNodeType::Vec2 => NodeType::Vec2(Vec2Node::default()),
            SavedNodeType::Theme => NodeType::Theme,
            SavedNodeType::ThemeExtended => NodeType::ThemeExtended,
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
        EdgeCurve::Line => "Line",
    }
    .to_string()
}

fn string_to_edge_curve(s: &str) -> EdgeCurve {
    match s {
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
        PatternType::Dotted => "Dotted",
        PatternType::DashDotted => "DashDotted",
    }
    .to_string()
}

fn string_to_pattern_type(s: &str) -> PatternType {
    match s {
        "Solid" => PatternType::Solid,
        "Dashed" | "DashCapped" | "Angled" => PatternType::Dashed,
        "Arrowed" => PatternType::Arrowed,
        "Dotted" => PatternType::Dotted,
        "DashDotted" => PatternType::DashDotted,
        _ => PatternType::Solid,
    }
}

fn tiling_kind_to_string(kind: TilingKind) -> String {
    match kind {
        TilingKind::Grid => "Grid",
        TilingKind::Dots => "Dots",
        TilingKind::Triangles => "Triangles",
        TilingKind::Hex => "Hex",
    }
    .to_string()
}

fn string_to_tiling_kind(s: &str) -> TilingKind {
    match s {
        "Dots" => TilingKind::Dots,
        "Triangles" => TilingKind::Triangles,
        "Hex" => TilingKind::Hex,
        _ => TilingKind::Grid,
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
