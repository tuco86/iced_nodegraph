// Pre-existing warnings allowed (not part of StyleFn refactoring)
#![allow(clippy::large_enum_variant)]

//! # Hello World Demo
//!
//! Basic node graph with command palette (Cmd/Ctrl+Space) for adding nodes and changing themes.
//! Now includes interactive style configuration nodes!
//!
//! ## Interactive Demo
//!
//! <link rel="stylesheet" href="pkg/demo.css">
//! <div id="demo-container">
//!   <div id="demo-loading">
//!     <div class="demo-spinner"></div>
//!     <p>Loading demo...</p>
//!   </div>
//!   <div id="demo-canvas-container"></div>
//!   <div id="demo-error">
//!     <strong>Failed to load demo.</strong> WebGPU required.
//!   </div>
//! </div>
//! <script type="module" src="pkg/demo-loader.js"></script>
//!
//! ## Controls
//!
//! - **Cmd/Ctrl+Space** - Open command palette
//! - **Drag nodes** - Move nodes around the canvas
//! - **Drag from pins** - Create connections between nodes
//! - **Click edges** - Disconnect existing connections
//! - **Scroll** - Zoom in/out
//! - **Middle-drag** - Pan the canvas
//!
//! ## Style Configuration
//!
//! Add input nodes (sliders, color pickers) and connect them to config nodes
//! to dynamically adjust the graph's appearance!

mod ids;
mod nodes;
#[cfg(not(target_arch = "wasm32"))]
mod persistence;

use iced::{
    Color, Event, Length, Point, Subscription, Task, Theme, Vector, event, keyboard,
    widget::{container, stack, text},
    window,
};
use iced_nodegraph::{BackgroundConfig, EdgeConfig, NodeConfig, PinConfig, PinRef, ShadowConfig};
use iced_nodegraph::{EdgeCurve, PinShape};
use iced_palette::{
    Command, Shortcut, command, command_palette, find_matching_shortcut, focus_input,
    get_filtered_command_index, get_filtered_count, is_toggle_shortcut, navigate_down, navigate_up,
};
use ids::{EdgeId, NodeId, PinLabel, generate_edge_id, generate_node_id};
use nodes::{
    BackgroundConfigInputs, BoolToggleConfig, ConfigNodeType, EdgeConfigInputs, EdgeSection,
    EdgeSections, FloatSliderConfig, InputNodeType, IntSliderConfig, MathNodeState, MathOperation,
    NodeConfigInputs, NodeSection, NodeSections, NodeType, NodeValue, PatternType,
    PatternTypeSelection, PinConfigInputs, ShadowConfigInputs, apply_to_graph_node,
    apply_to_node_node, background_config_node, background_pattern_selector_node, bool_toggle_node,
    color_picker_node, color_preset_node, edge_config_node, edge_curve_selector_node,
    float_slider_node, int_slider_node, math_node, node, node_config_node,
    pattern_type_selector_node, pin_config_node, pin_shape_selector_node, shadow_config_node,
};
#[cfg(not(target_arch = "wasm32"))]
use persistence::EdgeData;
use std::collections::{HashMap, HashSet};

/// Edge data for in-memory representation (WASM version).
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct EdgeData {
    pub from_node: NodeId,
    pub from_pin: PinLabel,
    pub to_node: NodeId,
    pub to_pin: PinLabel,
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

pub fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    let window_settings = iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            target: Some(String::from("demo-canvas-container")),
        },
        ..Default::default()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let window_settings = {
        // Try to load saved window settings
        let (position, size, maximized) = persistence::load_state()
            .map(|s| (s.window_position, s.window_size, s.window_maximized))
            .unwrap_or((None, None, None));

        iced::window::Settings {
            position: position
                .map(|(x, y)| iced::window::Position::Specific(Point::new(x as f32, y as f32)))
                .unwrap_or(iced::window::Position::Centered),
            size: size
                .map(|(w, h)| iced::Size::new(w as f32, h as f32))
                .unwrap_or(iced::Size::new(1280.0, 800.0)),
            maximized: maximized.unwrap_or(false),
            ..Default::default()
        }
    };

    iced::application(Application::new, Application::update, Application::view)
        .subscription(Application::subscription)
        .title("Hello World - iced_nodegraph Demo")
        .theme(Application::theme)
        .window(window_settings)
        .run()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_demo() {
    let _ = main();
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ApplicationMessage {
    Noop,
    EdgeConnected {
        from: PinRef<NodeId, PinLabel>,
        to: PinRef<NodeId, PinLabel>,
    },
    NodeMoved {
        node_id: NodeId,
        new_position: Point,
    },
    EdgeDisconnected {
        from: PinRef<NodeId, PinLabel>,
        to: PinRef<NodeId, PinLabel>,
    },
    ToggleCommandPalette,
    CommandPaletteInput(String),
    CommandPaletteNavigateUp,
    CommandPaletteNavigateDown,
    CommandPaletteSelect(usize),
    CommandPaletteConfirm,
    CommandPaletteCancel,
    ExecuteShortcut(String),
    CommandPaletteNavigate(usize),
    SpawnNode {
        node_type: NodeType,
    },
    ChangeTheme(Theme),
    CameraChanged {
        position: Point,
        zoom: f32,
    },
    WindowResized(iced::Size),
    WindowMoved(Point),
    WindowMaximizedChanged(bool),
    NavigateToSubmenu(String),
    NavigateBack,
    Tick,
    // Selection-related messages
    SelectionChanged(Vec<NodeId>),
    CloneNodes(Vec<NodeId>),
    DeleteNodes(Vec<NodeId>),
    GroupMoved {
        node_ids: Vec<NodeId>,
        delta: Vector,
    },
    // State export for Claude
    ExportState,
    // Input node value changes
    SliderChanged {
        node_id: NodeId,
        value: f32,
    },
    IntSliderChanged {
        node_id: NodeId,
        value: i32,
    },
    BoolChanged {
        node_id: NodeId,
        value: bool,
    },
    EdgeCurveChanged {
        node_id: NodeId,
        value: EdgeCurve,
    },
    PinShapeChanged {
        node_id: NodeId,
        value: PinShape,
    },
    PatternTypeChanged {
        node_id: NodeId,
        value: PatternType,
    },
    BackgroundPatternChanged {
        node_id: NodeId,
        value: PatternTypeSelection,
    },
    ColorChanged {
        node_id: NodeId,
        color: Color,
    },
    // Collapsible node messages
    ToggleNodeExpanded {
        node_id: NodeId,
    },
    UpdateFloatSliderConfig {
        node_id: NodeId,
        config: FloatSliderConfig,
    },
    UpdateIntSliderConfig {
        node_id: NodeId,
        config: IntSliderConfig,
    },
    // Config section collapse/expand
    ToggleEdgeSection {
        node_id: NodeId,
        section: EdgeSection,
    },
    ToggleNodeSection {
        node_id: NodeId,
        section: NodeSection,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum PaletteView {
    Main,
    Submenu(String),
}

/// Output types from config nodes for propagation
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ConfigOutput {
    Node(NodeConfig),
    Edge(EdgeConfig),
    Pin(iced_nodegraph::PinConfig),
    Background(BackgroundConfig),
}

/// Computed style values from connected config nodes
#[derive(Debug, Clone, Default)]
struct ComputedStyle {
    corner_radius: Option<f32>,
    opacity: Option<f32>,
    border_width: Option<f32>,
    fill_color: Option<Color>,
    shadow: Option<ShadowConfig>,
    edge_thickness: Option<f32>,
    edge_color: Option<Color>,
    edge_curve: Option<EdgeCurve>,
    edge_pattern: Option<iced_nodegraph::StrokePattern>,
    edge_dash_cap: Option<iced_nodegraph::DashCap>,
    // Edge border and shadow
    edge_border: Option<iced_nodegraph::BorderConfig>,
    edge_shadow: Option<iced_nodegraph::EdgeShadowConfig>,
    // Pin config values
    pin_color: Option<Color>,
    pin_radius: Option<f32>,
    pin_shape: Option<iced_nodegraph::PinShape>,
    pin_border_color: Option<Color>,
    pin_border_width: Option<f32>,
    // Background config
    background: Option<BackgroundConfig>,
}

impl ComputedStyle {
    /// Builds a NodeConfig from computed values (partial overrides).
    /// Only properties that are explicitly set will override theme defaults.
    fn to_node_config(&self) -> NodeConfig {
        let mut config = NodeConfig::new();
        if let Some(r) = self.corner_radius {
            config = config.corner_radius(r);
        }
        if let Some(o) = self.opacity {
            config = config.opacity(o);
        }
        if let Some(w) = self.border_width {
            config = config.border_width(w);
        }
        if let Some(c) = self.fill_color {
            config = config.fill_color(c);
        }
        if let Some(ref s) = self.shadow {
            config = config.shadow(s.clone());
        }
        config
    }

    /// Builds an EdgeConfig from computed values
    fn to_edge_config(&self) -> EdgeConfig {
        let mut config = EdgeConfig::new();
        if let Some(t) = self.edge_thickness {
            config = config.thickness(t);
        }
        if let Some(c) = self.edge_color {
            config = config.solid_color(c);
        }
        if let Some(curve) = self.edge_curve {
            config = config.curve(curve);
        }
        if let Some(ref pattern) = self.edge_pattern {
            config = config.pattern(pattern.clone());
        }
        if let Some(ref dash_cap) = self.edge_dash_cap {
            config = config.dash_cap(*dash_cap);
        }
        if let Some(ref border) = self.edge_border {
            config.border = Some(border.clone());
        }
        if let Some(ref shadow) = self.edge_shadow {
            config.shadow = Some(shadow.clone());
        }
        config
    }

    /// Builds a PinConfig from computed values
    fn to_pin_config(&self) -> PinConfig {
        let mut config = PinConfig::new();
        if let Some(c) = self.pin_color {
            config = config.color(c);
        }
        if let Some(r) = self.pin_radius {
            config = config.radius(r);
        }
        if let Some(s) = self.pin_shape {
            config = config.shape(s);
        }
        if let Some(bc) = self.pin_border_color {
            config = config.border_color(bc);
        }
        if let Some(bw) = self.pin_border_width {
            config = config.border_width(bw);
        }
        config
    }
}

struct Application {
    /// Nodes stored by unique ID
    nodes: HashMap<NodeId, (Point, NodeType)>,
    /// Node order for deterministic iteration
    node_order: Vec<NodeId>,
    /// Edges stored by unique ID
    edges: HashMap<EdgeId, EdgeData>,
    /// Edge order for deterministic iteration
    edge_order: Vec<EdgeId>,
    /// Currently selected nodes
    selected_nodes: HashSet<NodeId>,
    /// Nodes with expanded options panels
    expanded_nodes: HashSet<NodeId>,
    /// Section expansion state for EdgeConfig nodes
    edge_config_sections: HashMap<NodeId, EdgeSections>,
    /// Section expansion state for NodeConfig nodes
    node_config_sections: HashMap<NodeId, NodeSections>,
    command_palette_open: bool,
    command_input: String,
    current_theme: Theme,
    palette_view: PaletteView,
    palette_selected_index: usize,
    palette_preview_theme: Option<Theme>,
    palette_original_theme: Option<Theme>,
    /// Computed style values from config node connections
    computed_style: ComputedStyle,
    /// Pending config outputs from config nodes to be applied by ApplyToGraph
    pending_configs: HashMap<NodeId, Vec<(PinLabel, ConfigOutput)>>,
    /// Current viewport size for spawn-at-center calculation
    viewport_size: iced::Size,
    /// Current camera position from NodeGraph
    camera_position: Point,
    /// Current camera zoom from NodeGraph
    camera_zoom: f32,
    /// Window position (x, y) for persistence
    window_position: Option<(i32, i32)>,
    /// Window size (width, height) for persistence
    window_size: Option<(u32, u32)>,
    /// Whether window is maximized
    window_maximized: Option<bool>,
}

impl Default for Application {
    fn default() -> Self {
        use nodes::pins::workflow;

        // Create nodes with stable NanoIDs
        let node0_id = generate_node_id();
        let node1_id = generate_node_id();
        let node2_id = generate_node_id();
        let node3_id = generate_node_id();

        let mut nodes = HashMap::new();
        nodes.insert(
            node0_id.clone(),
            (
                Point::new(45.5, 149.0),
                NodeType::Workflow("email_trigger".to_string()),
            ),
        );
        nodes.insert(
            node1_id.clone(),
            (
                Point::new(274.5, 227.5),
                NodeType::Workflow("email_parser".to_string()),
            ),
        );
        nodes.insert(
            node2_id.clone(),
            (
                Point::new(459.5, 432.5),
                NodeType::Workflow("filter".to_string()),
            ),
        );
        nodes.insert(
            node3_id.clone(),
            (
                Point::new(679.0, 252.5),
                NodeType::Workflow("calendar".to_string()),
            ),
        );

        let node_order = vec![
            node0_id.clone(),
            node1_id.clone(),
            node2_id.clone(),
            node3_id.clone(),
        ];

        // Create edges with stable NanoIDs and string pin labels
        let mut edges = HashMap::new();
        let mut edge_order = Vec::new();

        // Edge 0: email_trigger "on email" -> email_parser "email"
        let edge0_id = generate_edge_id();
        edges.insert(
            edge0_id.clone(),
            EdgeData {
                from_node: node0_id.clone(),
                from_pin: workflow::ON_EMAIL,
                to_node: node1_id.clone(),
                to_pin: workflow::EMAIL,
            },
        );
        edge_order.push(edge0_id);

        // Edge 1: email_parser "subject" -> filter "input"
        let edge1_id = generate_edge_id();
        edges.insert(
            edge1_id.clone(),
            EdgeData {
                from_node: node1_id.clone(),
                from_pin: workflow::SUBJECT,
                to_node: node2_id.clone(),
                to_pin: workflow::INPUT,
            },
        );
        edge_order.push(edge1_id);

        // Edge 2: email_parser "datetime" -> calendar "datetime"
        let edge2_id = generate_edge_id();
        edges.insert(
            edge2_id.clone(),
            EdgeData {
                from_node: node1_id.clone(),
                from_pin: workflow::DATETIME,
                to_node: node3_id.clone(),
                to_pin: workflow::DATETIME,
            },
        );
        edge_order.push(edge2_id);

        // Edge 3: filter "matches" -> calendar "title"
        let edge3_id = generate_edge_id();
        edges.insert(
            edge3_id.clone(),
            EdgeData {
                from_node: node2_id.clone(),
                from_pin: workflow::MATCHES,
                to_node: node3_id.clone(),
                to_pin: workflow::TITLE,
            },
        );
        edge_order.push(edge3_id);

        Self {
            nodes,
            node_order,
            edges,
            edge_order,
            selected_nodes: HashSet::new(),
            expanded_nodes: HashSet::new(),
            edge_config_sections: HashMap::new(),
            node_config_sections: HashMap::new(),
            command_palette_open: false,
            command_input: String::new(),
            current_theme: Theme::CatppuccinFrappe,
            palette_view: PaletteView::Main,
            palette_selected_index: 0,
            palette_preview_theme: None,
            palette_original_theme: None,
            computed_style: ComputedStyle::default(),
            pending_configs: HashMap::new(),
            viewport_size: iced::Size::new(800.0, 600.0), // Default size
            camera_position: Point::ORIGIN,
            camera_zoom: 1.0,
            window_position: None,
            window_size: None,
            window_maximized: None,
        }
    }
}

impl Application {
    fn new() -> Self {
        // Try to load saved state, fall back to default
        #[cfg(not(target_arch = "wasm32"))]
        {
            match persistence::load_state() {
                Ok(saved) => {
                    let (
                        nodes,
                        node_order,
                        edges,
                        edge_order,
                        theme,
                        camera_pos,
                        camera_zoom,
                        window_pos,
                        window_size,
                        edge_config_sections,
                        node_config_sections,
                        window_maximized,
                    ) = saved.to_app();
                    println!(
                        "Loaded saved state: {} nodes, {} edges",
                        nodes.len(),
                        edges.len()
                    );
                    let mut app = Self {
                        nodes,
                        node_order,
                        edges,
                        edge_order,
                        current_theme: theme,
                        camera_position: camera_pos,
                        camera_zoom,
                        window_position: window_pos,
                        window_size,
                        edge_config_sections,
                        node_config_sections,
                        window_maximized,
                        ..Self::default()
                    };
                    // Apply computed styles from config nodes immediately
                    app.propagate_values();
                    return app;
                }
                Err(e) => {
                    println!("No saved state found: {}", e);
                }
            }
        }
        Self::default()
    }

    /// Saves current state to disk (native only).
    #[cfg(not(target_arch = "wasm32"))]
    fn save_state(&self) {
        let saved = persistence::SavedState::from_app(
            &self.nodes,
            &self.node_order,
            &self.edges,
            &self.edge_order,
            &self.current_theme,
            self.camera_position,
            self.camera_zoom,
            self.window_position,
            self.window_size,
            &self.edge_config_sections,
            &self.node_config_sections,
            self.window_maximized,
        );
        if let Err(e) = persistence::save_state(&saved) {
            eprintln!("Failed to save state: {}", e);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save_state(&self) {
        // No-op on WASM
    }

    /// Calculate spawn position at screen center, converted to world coordinates.
    fn spawn_position(&self) -> Point {
        // Screen center
        let screen_center_x = self.viewport_size.width / 2.0;
        let screen_center_y = self.viewport_size.height / 2.0;

        // Convert to world coordinates: world = screen / zoom - camera_position
        let world_x = screen_center_x / self.camera_zoom - self.camera_position.x;
        let world_y = screen_center_y / self.camera_zoom - self.camera_position.y;

        // Offset for node size (approximate center, ~100x80 typical node)
        Point::new(world_x - 50.0, world_y - 40.0)
    }

    /// Export current graph state to a file for Claude to read and update demos.
    /// Format is designed to be human-readable and easily parseable.
    #[cfg(not(target_arch = "wasm32"))]
    fn export_state_to_file(&self) {
        use std::io::Write;

        // Create out/ directory if it doesn't exist
        let out_dir = std::path::Path::new("out");
        if !out_dir.exists()
            && let Err(e) = std::fs::create_dir(out_dir) {
                eprintln!("Failed to create out/ directory: {}", e);
                return;
            }

        // Generate random filename
        let filename = Self::generate_random_name();
        let path = out_dir.join(format!("{}.txt", filename));

        let mut output = String::new();
        output.push_str("# Graph State Export\n");
        output.push_str(
            "# Generated by hello_world demo - use this to update demo initial state\n\n",
        );

        // Export nodes
        output.push_str("## Nodes\n");
        output.push_str(&format!("# Total: {} nodes\n\n", self.nodes.len()));

        for node_id in &self.node_order {
            if let Some((pos, node_type)) = self.nodes.get(node_id) {
                output.push_str(&format!("Node {}: ({:.1}, {:.1})\n", node_id, pos.x, pos.y));
                match node_type {
                    NodeType::Workflow(name) => {
                        output.push_str(&format!("  Type: Workflow(\"{}\")\n", name));
                    }
                    NodeType::Input(input) => {
                        output.push_str(&format!("  Type: Input({:?})\n", input));
                    }
                    NodeType::Config(config) => {
                        output.push_str(&format!("  Type: Config({:?})\n", config));
                    }
                    NodeType::Math(state) => {
                        output.push_str(&format!("  Type: Math({:?})\n", state));
                    }
                }
                output.push('\n');
            }
        }

        // Export edges
        output.push_str("## Edges\n");
        output.push_str(&format!("# Total: {} edges\n\n", self.edges.len()));

        for edge_id in &self.edge_order {
            if let Some(edge) = self.edges.get(edge_id) {
                output.push_str(&format!(
                    "Edge {}: Node {}.Pin \"{}\" -> Node {}.Pin \"{}\"\n",
                    edge_id, edge.from_node, edge.from_pin, edge.to_node, edge.to_pin
                ));
            }
        }

        // Export JSON snippet for easy copy-paste
        output.push_str("\n## JSON Format (for state.json)\n\n");
        output.push_str("```json\n");
        output.push_str("{\n  \"nodes\": [\n");
        for (i, node_id) in self.node_order.iter().enumerate() {
            if let Some((pos, node_type)) = self.nodes.get(node_id) {
                let type_str = match node_type {
                    NodeType::Workflow(name) => {
                        format!("{{\"type\": \"Workflow\", \"name\": \"{}\"}}", name)
                    }
                    _ => format!("{:?}", node_type),
                };
                let comma = if i < self.node_order.len() - 1 {
                    ","
                } else {
                    ""
                };
                output.push_str(&format!(
                    "    {{\"id\": \"{}\", \"x\": {:.1}, \"y\": {:.1}, \"node_type\": {}}}{}\n",
                    node_id, pos.x, pos.y, type_str, comma
                ));
            }
        }
        output.push_str("  ],\n  \"edges\": [\n");
        for (i, edge_id) in self.edge_order.iter().enumerate() {
            if let Some(edge) = self.edges.get(edge_id) {
                let comma = if i < self.edge_order.len() - 1 {
                    ","
                } else {
                    ""
                };
                output.push_str(&format!(
                    "    {{\"id\": \"{}\", \"from_node\": \"{}\", \"from_pin\": \"{}\", \"to_node\": \"{}\", \"to_pin\": \"{}\"}}{}\n",
                    edge_id, edge.from_node, edge.from_pin, edge.to_node, edge.to_pin, comma
                ));
            }
        }
        output.push_str("  ]\n}\n");
        output.push_str("```\n");

        // Write to file
        match std::fs::File::create(&path) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(output.as_bytes()) {
                    eprintln!("Failed to write state export: {}", e);
                } else {
                    println!("State exported to: {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("Failed to create export file: {}", e);
            }
        }
    }

    /// Generate a random two-word name for export files
    #[cfg(not(target_arch = "wasm32"))]
    fn generate_random_name() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        const ADJECTIVES: &[&str] = &[
            "swift", "bright", "calm", "bold", "keen", "warm", "cool", "wild", "soft", "sharp",
            "quick", "slow", "deep", "wide", "tall", "tiny", "grand", "pure", "rare", "wise",
            "fair", "dark", "light", "fresh",
        ];
        const NOUNS: &[&str] = &[
            "river", "mountain", "forest", "ocean", "meadow", "valley", "canyon", "island",
            "sunset", "sunrise", "thunder", "breeze", "garden", "crystal", "shadow", "ember",
            "falcon", "phoenix", "dragon", "tiger", "wolf", "eagle", "raven", "fox",
        ];

        // Simple random using system time nanoseconds
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let adj_idx = (nanos % ADJECTIVES.len() as u128) as usize;
        let noun_idx = ((nanos / 7) % NOUNS.len() as u128) as usize;

        format!("{}-{}", ADJECTIVES[adj_idx], NOUNS[noun_idx])
    }

    #[cfg(target_arch = "wasm32")]
    fn export_state_to_file(&self) {
        // WASM: State export not available in browser
    }

    /// Propagates values from input nodes to connected config nodes
    fn propagate_values(&mut self) {
        use nodes::pins::{config as pin_config, math as pin_math};

        let mut new_computed = ComputedStyle::default();
        self.pending_configs.clear();

        // Phase 1: Reset all config node and math node inputs to defaults
        for (_, node_type) in self.nodes.values_mut() {
            match node_type {
                NodeType::Config(config) => match config {
                    ConfigNodeType::NodeConfig(inputs) => *inputs = NodeConfigInputs::default(),
                    ConfigNodeType::EdgeConfig(inputs) => *inputs = EdgeConfigInputs::default(),
                    ConfigNodeType::ShadowConfig(inputs) => *inputs = ShadowConfigInputs::default(),
                    ConfigNodeType::PinConfig(inputs) => *inputs = PinConfigInputs::default(),
                    ConfigNodeType::BackgroundConfig(inputs) => {
                        *inputs = BackgroundConfigInputs::default()
                    }
                    ConfigNodeType::ApplyToGraph {
                        has_node_config,
                        has_edge_config,
                        has_pin_config,
                        has_background_config,
                    } => {
                        *has_node_config = false;
                        *has_edge_config = false;
                        *has_pin_config = false;
                        *has_background_config = false;
                    }
                    ConfigNodeType::ApplyToNode {
                        has_node_config,
                        target_id,
                    } => {
                        *has_node_config = false;
                        *target_id = None;
                    }
                },
                NodeType::Math(state) => {
                    state.input_a = None;
                    state.input_b = None;
                }
                _ => {}
            }
        }

        // Phase 1.5: Propagate values INTO Math nodes (iteratively for chaining)
        // Math nodes can be chained (e.g., (A+B)*C), so we iterate until stable
        let edges_snapshot: Vec<_> = self.edges.values().cloned().collect();

        // We need multiple passes because Math→Math chains require the source
        // to have computed its result before the target can use it
        const MAX_ITERATIONS: usize = 10;
        for _ in 0..MAX_ITERATIONS {
            let mut changed = false;

            for edge in &edges_snapshot {
                // Get source node's output value
                let source_value = self
                    .nodes
                    .get(&edge.from_node)
                    .and_then(|(_, t)| t.output_value());

                if let Some(value) = source_value {
                    // Try to apply to target if it's a Math node
                    if let Some((_, NodeType::Math(state))) = self.nodes.get_mut(&edge.to_node) {
                        // Math pins: A, B (inputs), result (output)
                        if let Some(float_val) = value.as_float() {
                            if edge.to_pin == pin_math::A {
                                if state.input_a != Some(float_val) {
                                    state.input_a = Some(float_val);
                                    changed = true;
                                }
                            } else if edge.to_pin == pin_math::B
                                && state.input_b != Some(float_val) {
                                    state.input_b = Some(float_val);
                                    changed = true;
                                }
                        }
                    }
                }

                // Also check reverse direction (edges can connect either way)
                let source_value = self
                    .nodes
                    .get(&edge.to_node)
                    .and_then(|(_, t)| t.output_value());

                if let Some(value) = source_value
                    && let Some((_, NodeType::Math(state))) = self.nodes.get_mut(&edge.from_node)
                        && let Some(float_val) = value.as_float() {
                            if edge.from_pin == pin_math::A {
                                if state.input_a != Some(float_val) {
                                    state.input_a = Some(float_val);
                                    changed = true;
                                }
                            } else if edge.from_pin == pin_math::B
                                && state.input_b != Some(float_val) {
                                    state.input_b = Some(float_val);
                                    changed = true;
                                }
                        }
            }

            if !changed {
                break;
            }
        }

        // Phase 2: Apply Input → Config connections (in both edge directions)
        // Also apply Math → Config connections

        for edge in &edges_snapshot {
            let from_node_type = self.nodes.get(&edge.from_node).map(|(_, t)| t.clone());
            let to_node_type = self.nodes.get(&edge.to_node).map(|(_, t)| t.clone());

            if let (Some(from_type), Some(to_type)) = (from_node_type, to_node_type) {
                // Handle Input → Config connections
                if let (NodeType::Input(input), NodeType::Config(_)) = (&from_type, &to_type) {
                    let value = input.output_value();
                    self.apply_value_to_config_node(&edge.to_node, &edge.to_pin, &value);
                }
                // Handle Config → Input connections (reverse direction)
                if let (NodeType::Config(_), NodeType::Input(input)) = (&from_type, &to_type) {
                    let value = input.output_value();
                    self.apply_value_to_config_node(&edge.from_node, &edge.from_pin, &value);
                }
                // Handle Math → Config connections
                if let (NodeType::Math(state), NodeType::Config(_)) = (&from_type, &to_type)
                    && let Some(result) = state.result() {
                        let value = NodeValue::Float(result);
                        self.apply_value_to_config_node(&edge.to_node, &edge.to_pin, &value);
                    }
                // Handle Config → Math connections (reverse direction)
                if let (NodeType::Config(_), NodeType::Math(state)) = (&from_type, &to_type)
                    && let Some(result) = state.result() {
                        let value = NodeValue::Float(result);
                        self.apply_value_to_config_node(&edge.from_node, &edge.from_pin, &value);
                    }
            }
        }

        // Phase 2.5: Handle ShadowConfig → NodeConfig connections
        // ShadowConfig's output connects to NodeConfig's shadow input
        for edge in &edges_snapshot {
            let from_node_type = self.nodes.get(&edge.from_node).map(|(_, t)| t.clone());
            let to_node_type = self.nodes.get(&edge.to_node).map(|(_, t)| t.clone());

            if let (Some(from_type), Some(to_type)) = (from_node_type, to_node_type) {
                // ShadowConfig (output CONFIG) → NodeConfig (shadow input SHADOW)
                if let (
                    NodeType::Config(ConfigNodeType::ShadowConfig(shadow_inputs)),
                    NodeType::Config(ConfigNodeType::NodeConfig(_)),
                ) = (&from_type, &to_type)
                    && edge.from_pin == pin_config::CONFIG && edge.to_pin == pin_config::SHADOW {
                        let shadow_config = shadow_inputs.build();
                        let value = NodeValue::ShadowConfig(shadow_config);
                        self.apply_value_to_config_node(&edge.to_node, &edge.to_pin, &value);
                    }
                // Reverse: NodeConfig ← ShadowConfig
                if let (
                    NodeType::Config(ConfigNodeType::NodeConfig(_)),
                    NodeType::Config(ConfigNodeType::ShadowConfig(shadow_inputs)),
                ) = (&from_type, &to_type)
                    && edge.to_pin == pin_config::CONFIG && edge.from_pin == pin_config::SHADOW {
                        let shadow_config = shadow_inputs.build();
                        let value = NodeValue::ShadowConfig(shadow_config);
                        self.apply_value_to_config_node(&edge.from_node, &edge.from_pin, &value);
                    }
            }
        }

        // Phase 3: After all inputs applied, process Config → ApplyToGraph connections
        // Now config nodes have their updated inputs, so we can build configs
        for edge in &edges_snapshot {
            let from_node_type = self.nodes.get(&edge.from_node).map(|(_, t)| t.clone());
            let to_node_type = self.nodes.get(&edge.to_node).map(|(_, t)| t.clone());

            if let (Some(from_type), Some(to_type)) = (from_node_type, to_node_type) {
                // Handle Config → ApplyToGraph connections
                if let (
                    NodeType::Config(config),
                    NodeType::Config(ConfigNodeType::ApplyToGraph { .. }),
                ) = (&from_type, &to_type)
                {
                    self.connect_config_to_apply(
                        &edge.from_node,
                        config,
                        &edge.to_node,
                        &edge.to_pin,
                    );
                }
                // Handle ApplyToGraph → Config connections (reverse)
                if let (
                    NodeType::Config(ConfigNodeType::ApplyToGraph { .. }),
                    NodeType::Config(config),
                ) = (&from_type, &to_type)
                {
                    self.connect_config_to_apply(
                        &edge.to_node,
                        config,
                        &edge.from_node,
                        &edge.from_pin,
                    );
                }
            }
        }

        // Phase 4: Build configs from ApplyToGraph nodes and apply to computed style
        self.apply_graph_configs(&mut new_computed);

        self.computed_style = new_computed;
    }

    /// Applies an input value to a specific pin on a config node
    fn apply_value_to_config_node(
        &mut self,
        node_id: &NodeId,
        pin_label: &PinLabel,
        value: &NodeValue,
    ) {
        use nodes::pins::config as pin;

        let Some((_, node_type)) = self.nodes.get_mut(node_id) else {
            return;
        };

        let NodeType::Config(config) = node_type else {
            return;
        };

        match config {
            ConfigNodeType::NodeConfig(inputs) => {
                // NodeConfig pin labels: config, bg, color, width, radius, opacity, shadow
                if *pin_label == pin::BG_COLOR {
                    inputs.fill_color = value.as_color();
                } else if *pin_label == pin::COLOR {
                    inputs.border_color = value.as_color();
                } else if *pin_label == pin::WIDTH {
                    inputs.border_width = value.as_float();
                } else if *pin_label == pin::RADIUS {
                    inputs.corner_radius = value.as_float();
                } else if *pin_label == pin::OPACITY {
                    inputs.opacity = value.as_float();
                } else if *pin_label == pin::SHADOW
                    && let Some(shadow) = value.as_shadow_config() {
                        inputs.shadow = Some(shadow.clone());
                    }
            }
            ConfigNodeType::EdgeConfig(inputs) => {
                // EdgeConfig pin labels
                if *pin_label == pin::START {
                    inputs.start_color = value.as_color();
                } else if *pin_label == pin::END {
                    inputs.end_color = value.as_color();
                } else if *pin_label == pin::THICK {
                    inputs.thickness = value.as_float();
                } else if *pin_label == pin::CURVE {
                    inputs.curve = value.as_edge_curve();
                } else if *pin_label == pin::PATTERN {
                    inputs.pattern_type = value.as_pattern_type();
                } else if *pin_label == pin::DASH {
                    inputs.dash_length = value.as_float();
                } else if *pin_label == pin::GAP {
                    inputs.gap_length = value.as_float();
                } else if *pin_label == pin::ANGLE {
                    // Convert degrees from slider to radians for pattern angle
                    inputs.pattern_angle = value.as_float().map(|deg| deg.to_radians());
                } else if *pin_label == pin::SPEED {
                    inputs.animation_speed = value.as_float();
                // Border settings
                } else if *pin_label == pin::BORDER {
                    inputs.border_enabled = value.as_bool();
                } else if *pin_label == pin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                } else if *pin_label == pin::BORDER_GAP {
                    inputs.border_gap = value.as_float();
                } else if *pin_label == pin::BORDER_START_COLOR {
                    inputs.border_start_color = value.as_color();
                } else if *pin_label == pin::BORDER_END_COLOR {
                    inputs.border_end_color = value.as_color();
                // Outline settings
                } else if *pin_label == pin::INNER_OUTLINE {
                    inputs.inner_outline_enabled = value.as_bool();
                } else if *pin_label == pin::INNER_OUTLINE_WIDTH {
                    inputs.inner_outline_width = value.as_float();
                } else if *pin_label == pin::INNER_OUTLINE_COLOR {
                    inputs.inner_outline_color = value.as_color();
                } else if *pin_label == pin::OUTER_OUTLINE {
                    inputs.outer_outline_enabled = value.as_bool();
                } else if *pin_label == pin::OUTER_OUTLINE_WIDTH {
                    inputs.outer_outline_width = value.as_float();
                } else if *pin_label == pin::OUTER_OUTLINE_COLOR {
                    inputs.outer_outline_color = value.as_color();
                // Shadow settings
                } else if *pin_label == pin::SHADOW {
                    inputs.shadow_enabled = value.as_bool();
                } else if *pin_label == pin::SHADOW_BLUR {
                    inputs.shadow_blur = value.as_float();
                } else if *pin_label == pin::SHADOW_OFFSET {
                    // Single offset value sets both x and y
                    let offset = value.as_float();
                    inputs.shadow_offset_x = offset;
                    inputs.shadow_offset_y = offset;
                } else if *pin_label == pin::SHADOW_COLOR {
                    inputs.shadow_color = value.as_color();
                }
            }
            ConfigNodeType::ShadowConfig(inputs) => {
                // ShadowConfig pin labels
                if *pin_label == pin::SHADOW_OFFSET {
                    // For shadow config, offset sets both x and y
                    inputs.offset_x = value.as_float();
                    inputs.offset_y = value.as_float();
                } else if *pin_label == pin::SHADOW_OFFSET_X {
                    inputs.offset_x = value.as_float();
                } else if *pin_label == pin::SHADOW_OFFSET_Y {
                    inputs.offset_y = value.as_float();
                } else if *pin_label == pin::SHADOW_BLUR {
                    inputs.blur_radius = value.as_float();
                } else if *pin_label == pin::SHADOW_COLOR {
                    inputs.color = value.as_color();
                } else if *pin_label == pin::ON {
                    inputs.enabled = value.as_bool();
                }
            }
            ConfigNodeType::PinConfig(inputs) => {
                // PinConfig pin labels
                if *pin_label == pin::COLOR {
                    inputs.color = value.as_color();
                } else if *pin_label == pin::SIZE {
                    inputs.radius = value.as_float();
                } else if *pin_label == pin::SHAPE {
                    inputs.shape = value.as_pin_shape();
                } else if *pin_label == pin::GLOW {
                    inputs.border_color = value.as_color();
                } else if *pin_label == pin::PULSE {
                    inputs.border_width = value.as_float();
                }
            }
            ConfigNodeType::BackgroundConfig(inputs) => {
                // BackgroundConfig pin labels
                if *pin_label == pin::PATTERN {
                    inputs.pattern = value.as_background_pattern();
                } else if *pin_label == pin::BACKGROUND_COLOR {
                    inputs.background_color = value.as_color();
                } else if *pin_label == pin::PRIMARY_COLOR {
                    inputs.primary_color = value.as_color();
                } else if *pin_label == pin::MINOR_SPACING {
                    inputs.minor_spacing = value.as_float();
                } else if *pin_label == pin::ADAPTIVE_ZOOM {
                    inputs.adaptive_zoom = value.as_bool();
                }
            }
            ConfigNodeType::ApplyToNode { target_id, .. } => {
                if *pin_label == pin::TARGET {
                    *target_id = value.as_int();
                }
            }
            _ => {}
        }
    }

    /// Connects a config node's output to an ApplyToGraph node
    fn connect_config_to_apply(
        &mut self,
        config_node_id: &NodeId,
        _config_type: &ConfigNodeType, // Ignored - we read from current state
        apply_node_id: &NodeId,
        apply_pin_label: &PinLabel,
    ) {
        use nodes::pins::config as pin;

        // Build the config from the CURRENT state of the config node (not the snapshot)
        let built_config = match self.nodes.get(config_node_id) {
            Some((_, NodeType::Config(ConfigNodeType::NodeConfig(inputs)))) => {
                Some(ConfigOutput::Node(inputs.build()))
            }
            Some((_, NodeType::Config(ConfigNodeType::EdgeConfig(inputs)))) => {
                Some(ConfigOutput::Edge(inputs.build()))
            }
            Some((_, NodeType::Config(ConfigNodeType::PinConfig(inputs)))) => {
                Some(ConfigOutput::Pin(inputs.build()))
            }
            Some((_, NodeType::Config(ConfigNodeType::BackgroundConfig(inputs)))) => {
                Some(ConfigOutput::Background(inputs.build()))
            }
            _ => None,
        };

        let Some((_, node_type)) = self.nodes.get_mut(apply_node_id) else {
            return;
        };

        if let NodeType::Config(ConfigNodeType::ApplyToGraph {
            has_node_config,
            has_edge_config,
            has_pin_config,
            has_background_config,
        }) = node_type
        {
            // ApplyToGraph pin labels: node, edge, pin, background
            if *apply_pin_label == pin::NODE_CONFIG {
                if matches!(&built_config, Some(ConfigOutput::Node(_))) {
                    *has_node_config = true;
                }
            } else if *apply_pin_label == pin::EDGE_CONFIG {
                if matches!(&built_config, Some(ConfigOutput::Edge(_))) {
                    *has_edge_config = true;
                }
            } else if *apply_pin_label == pin::PIN_CONFIG {
                if matches!(&built_config, Some(ConfigOutput::Pin(_))) {
                    *has_pin_config = true;
                }
            } else if *apply_pin_label == pin::BACKGROUND_CONFIG
                && matches!(&built_config, Some(ConfigOutput::Background(_))) {
                    *has_background_config = true;
                }
        }

        // Store the config for later application
        if let Some(config) = built_config {
            self.pending_configs
                .entry(apply_node_id.clone())
                .or_default()
                .push((*apply_pin_label, config));
        }
    }

    /// Applies configs from ApplyToGraph nodes to the computed style
    fn apply_graph_configs(&mut self, computed: &mut ComputedStyle) {
        // Find ApplyToGraph nodes and apply their connected configs
        for (node_id, (_, node_type)) in &self.nodes {
            if let NodeType::Config(ConfigNodeType::ApplyToGraph {
                has_node_config,
                has_edge_config,
                has_pin_config,
                has_background_config,
            }) = node_type
                && let Some(configs) = self.pending_configs.get(node_id) {
                    for (_, config) in configs {
                        match config {
                            ConfigOutput::Node(node_config) => {
                                if *has_node_config {
                                    // Apply node config to computed style
                                    if let Some(r) = node_config.corner_radius {
                                        computed.corner_radius = Some(r);
                                    }
                                    if let Some(o) = node_config.opacity {
                                        computed.opacity = Some(o);
                                    }
                                    if let Some(w) = node_config.border_width {
                                        computed.border_width = Some(w);
                                    }
                                    if let Some(c) = node_config.fill_color {
                                        computed.fill_color = Some(c);
                                    }
                                    if node_config.shadow.is_some() {
                                        computed.shadow = node_config.shadow.clone();
                                    }
                                }
                            }
                            ConfigOutput::Edge(edge_config) => {
                                if *has_edge_config {
                                    // Apply edge config to computed style
                                    if let Some(stroke) = &edge_config.stroke {
                                        if let Some(t) = stroke.width {
                                            computed.edge_thickness = Some(t);
                                        }
                                        if let Some(c) = stroke.start_color {
                                            computed.edge_color = Some(c);
                                        }
                                        if let Some(ref p) = stroke.pattern {
                                            computed.edge_pattern = Some(p.clone());
                                        }
                                        if let Some(ref dc) = stroke.dash_cap {
                                            computed.edge_dash_cap = Some(*dc);
                                        }
                                    }
                                    if let Some(curve) = edge_config.curve {
                                        computed.edge_curve = Some(curve);
                                    }
                                    // Apply border config
                                    if let Some(ref border) = edge_config.border {
                                        computed.edge_border = Some(border.clone());
                                    }
                                    // Apply shadow config
                                    if let Some(ref shadow) = edge_config.shadow {
                                        computed.edge_shadow = Some(shadow.clone());
                                    }
                                }
                            }
                            ConfigOutput::Pin(pin_config) => {
                                if *has_pin_config {
                                    // Apply pin config to computed style
                                    if let Some(c) = pin_config.color {
                                        computed.pin_color = Some(c);
                                    }
                                    if let Some(r) = pin_config.radius {
                                        computed.pin_radius = Some(r);
                                    }
                                    if let Some(s) = pin_config.shape {
                                        computed.pin_shape = Some(s);
                                    }
                                    if let Some(c) = pin_config.border_color {
                                        computed.pin_border_color = Some(c);
                                    }
                                    if let Some(w) = pin_config.border_width {
                                        computed.pin_border_width = Some(w);
                                    }
                                }
                            }
                            ConfigOutput::Background(bg_config) => {
                                if *has_background_config {
                                    // Store the background config for later application
                                    computed.background = Some(bg_config.clone());
                                }
                            }
                        }
                    }
                }
        }
        // Clear pending configs after application
        self.pending_configs.clear();
    }

    fn update(&mut self, message: ApplicationMessage) -> Task<ApplicationMessage> {
        match message {
            ApplicationMessage::Noop => Task::none(),
            ApplicationMessage::EdgeConnected { from, to } => {
                let edge_id = generate_edge_id();
                self.edges.insert(
                    edge_id.clone(),
                    EdgeData {
                        from_node: from.node_id,
                        from_pin: from.pin_id,
                        to_node: to.node_id,
                        to_pin: to.pin_id,
                    },
                );
                self.edge_order.push(edge_id);
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::NodeMoved {
                node_id,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(&node_id) {
                    *position = new_position;
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::EdgeDisconnected { from, to } => {
                // Find and remove the edge by matching from/to
                let edge_to_remove: Option<EdgeId> = self
                    .edges
                    .iter()
                    .find(|(_, e)| {
                        e.from_node == from.node_id
                            && e.from_pin == from.pin_id
                            && e.to_node == to.node_id
                            && e.to_pin == to.pin_id
                    })
                    .map(|(id, _)| id.clone());

                if let Some(edge_id) = edge_to_remove {
                    self.edges.remove(&edge_id);
                    self.edge_order.retain(|id| id != &edge_id);
                }
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::ToggleCommandPalette => {
                self.command_palette_open = !self.command_palette_open;
                if !self.command_palette_open {
                    if let Some(original) = self.palette_original_theme.take() {
                        self.current_theme = original;
                    }
                    self.palette_preview_theme = None;
                    self.command_input.clear();
                    self.palette_view = PaletteView::Main;
                    self.palette_selected_index = 0;
                    Task::none()
                } else {
                    self.palette_original_theme = Some(self.current_theme.clone());
                    self.palette_view = PaletteView::Main;
                    self.palette_selected_index = 0;
                    focus_input()
                }
            }
            ApplicationMessage::CommandPaletteInput(input) => {
                self.command_input = input;
                self.palette_selected_index = 0;
                Task::none()
            }
            ApplicationMessage::ExecuteShortcut(cmd_id) => match cmd_id.as_str() {
                "add_node" => {
                    self.command_palette_open = true;
                    self.palette_original_theme = Some(self.current_theme.clone());
                    self.palette_view = PaletteView::Submenu("nodes".to_string());
                    self.palette_selected_index = 0;
                    self.command_input.clear();
                    focus_input()
                }
                "change_theme" => {
                    self.command_palette_open = true;
                    self.palette_original_theme = Some(self.current_theme.clone());
                    self.palette_view = PaletteView::Submenu("themes".to_string());
                    self.palette_selected_index = 0;
                    self.command_input.clear();
                    focus_input()
                }
                "export_state" => {
                    self.export_state_to_file();
                    Task::none()
                }
                _ => Task::none(),
            },
            ApplicationMessage::CommandPaletteNavigate(new_index) => {
                if !self.command_palette_open {
                    return Task::none();
                }
                self.palette_selected_index = new_index;

                if let PaletteView::Submenu(ref submenu) = self.palette_view
                    && submenu == "themes" {
                        let (_, commands) = self.build_palette_commands();
                        if let Some(original_idx) = get_filtered_command_index(
                            &self.command_input,
                            &commands,
                            self.palette_selected_index,
                        ) {
                            let themes = Self::get_available_themes();
                            if original_idx < themes.len() {
                                self.palette_preview_theme = Some(themes[original_idx].clone());
                            }
                        }
                    }
                Task::none()
            }
            ApplicationMessage::CommandPaletteNavigateUp => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let (_, commands) = self.build_palette_commands();
                let filtered_count = get_filtered_count(&self.command_input, &commands);
                let new_index = navigate_up(self.palette_selected_index, filtered_count);
                self.update(ApplicationMessage::CommandPaletteNavigate(new_index))
            }
            ApplicationMessage::CommandPaletteNavigateDown => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let (_, commands) = self.build_palette_commands();
                let filtered_count = get_filtered_count(&self.command_input, &commands);
                let new_index = navigate_down(self.palette_selected_index, filtered_count);
                self.update(ApplicationMessage::CommandPaletteNavigate(new_index))
            }
            ApplicationMessage::CommandPaletteSelect(index) => {
                if !self.command_palette_open {
                    return Task::none();
                }
                self.palette_selected_index = index;
                self.update(ApplicationMessage::CommandPaletteConfirm)
            }
            ApplicationMessage::CommandPaletteConfirm => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let (_, commands) = self.build_palette_commands();
                let Some(original_idx) = get_filtered_command_index(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                ) else {
                    return Task::none();
                };

                use iced_palette::CommandAction;
                let cmd = &commands[original_idx];
                match &cmd.action {
                    CommandAction::Message(msg) => {
                        let msg = msg.clone();
                        self.command_input.clear();
                        self.palette_selected_index = 0;
                        match msg {
                            ApplicationMessage::NavigateToSubmenu(submenu) => {
                                self.palette_view = PaletteView::Submenu(submenu);
                                focus_input()
                            }
                            ApplicationMessage::SpawnNode { node_type } => {
                                let new_id = generate_node_id();
                                let pos = self.spawn_position();
                                self.nodes.insert(new_id.clone(), (pos, node_type));
                                self.node_order.push(new_id.clone());
                                self.selected_nodes = HashSet::from([new_id]);
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                Task::none()
                            }
                            ApplicationMessage::ChangeTheme(theme) => {
                                self.current_theme = theme;
                                self.palette_preview_theme = None;
                                self.palette_original_theme = None;
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                Task::none()
                            }
                            ApplicationMessage::ExportState => {
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                self.export_state_to_file();
                                Task::none()
                            }
                            _ => Task::none(),
                        }
                    }
                    _ => Task::none(),
                }
            }
            ApplicationMessage::CommandPaletteCancel => {
                if !self.command_palette_open {
                    return Task::none();
                }
                if let Some(original) = self.palette_original_theme.take() {
                    self.current_theme = original;
                }
                self.palette_preview_theme = None;
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
                self.palette_selected_index = 0;
                Task::none()
            }
            ApplicationMessage::SpawnNode { node_type } => {
                let new_id = generate_node_id();
                let pos = self.spawn_position();
                self.nodes.insert(new_id.clone(), (pos, node_type));
                self.node_order.push(new_id.clone());
                self.selected_nodes = HashSet::from([new_id]);
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
                self.save_state();
                Task::none()
            }
            ApplicationMessage::CameraChanged { position, zoom } => {
                self.camera_position = position;
                self.camera_zoom = zoom;
                Task::none()
            }
            ApplicationMessage::WindowResized(size) => {
                self.viewport_size = size;
                self.window_size = Some((size.width as u32, size.height as u32));
                // Query maximize state on resize - it may have changed
                window::oldest()
                    .and_then(window::is_maximized)
                    .map(ApplicationMessage::WindowMaximizedChanged)
            }
            ApplicationMessage::WindowMoved(position) => {
                self.window_position = Some((position.x as i32, position.y as i32));
                self.save_state();
                Task::none()
            }
            ApplicationMessage::WindowMaximizedChanged(maximized) => {
                self.window_maximized = Some(maximized);
                self.save_state();
                Task::none()
            }
            ApplicationMessage::ChangeTheme(theme) => {
                self.current_theme = theme;
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
                self.save_state();
                Task::none()
            }
            ApplicationMessage::NavigateToSubmenu(submenu) => {
                self.palette_view = PaletteView::Submenu(submenu);
                self.command_input.clear();
                focus_input()
            }
            ApplicationMessage::NavigateBack => {
                self.palette_view = PaletteView::Main;
                self.command_input.clear();
                focus_input()
            }
            ApplicationMessage::Tick => Task::none(),
            ApplicationMessage::ExportState => {
                self.export_state_to_file();
                Task::none()
            }
            ApplicationMessage::SelectionChanged(node_ids) => {
                self.selected_nodes = node_ids.into_iter().collect();
                Task::none()
            }
            ApplicationMessage::CloneNodes(node_ids) => {
                let offset = Vector::new(50.0, 50.0);
                let mut id_map: HashMap<NodeId, NodeId> = HashMap::new();
                let mut new_ids = Vec::new();

                // Clone selected nodes
                for old_id in &node_ids {
                    if let Some((pos, node_type)) = self.nodes.get(old_id) {
                        let new_id = generate_node_id();
                        let new_pos = Point::new(pos.x + offset.x, pos.y + offset.y);
                        self.nodes
                            .insert(new_id.clone(), (new_pos, node_type.clone()));
                        self.node_order.push(new_id.clone());
                        id_map.insert(old_id.clone(), new_id.clone());
                        new_ids.push(new_id);
                    }
                }

                // Clone edges between selected nodes
                let edges_to_clone: Vec<_> = self
                    .edges
                    .iter()
                    .filter(|(_, e)| {
                        node_ids.contains(&e.from_node) && node_ids.contains(&e.to_node)
                    })
                    .map(|(_, e)| e.clone())
                    .collect();

                for edge in edges_to_clone {
                    if let (Some(new_from), Some(new_to)) =
                        (id_map.get(&edge.from_node), id_map.get(&edge.to_node))
                    {
                        let new_edge_id = generate_edge_id();
                        self.edges.insert(
                            new_edge_id.clone(),
                            EdgeData {
                                from_node: new_from.clone(),
                                from_pin: edge.from_pin,
                                to_node: new_to.clone(),
                                to_pin: edge.to_pin,
                            },
                        );
                        self.edge_order.push(new_edge_id);
                    }
                }

                self.selected_nodes = new_ids.into_iter().collect();
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::DeleteNodes(node_ids) => {
                // With NanoIDs, deletion is simple - no index remapping needed!
                for node_id in &node_ids {
                    // Remove node
                    self.nodes.remove(node_id);
                    self.node_order.retain(|id| id != node_id);

                    // Remove edges connected to this node
                    let edges_to_remove: Vec<_> = self
                        .edges
                        .iter()
                        .filter(|(_, e)| &e.from_node == node_id || &e.to_node == node_id)
                        .map(|(id, _)| id.clone())
                        .collect();

                    for edge_id in edges_to_remove {
                        self.edges.remove(&edge_id);
                        self.edge_order.retain(|id| id != &edge_id);
                    }
                }

                self.selected_nodes.clear();
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::GroupMoved { node_ids, delta } => {
                for node_id in node_ids {
                    if let Some((pos, _)) = self.nodes.get_mut(&node_id) {
                        pos.x += delta.x;
                        pos.y += delta.y;
                    }
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::SliderChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::FloatSlider { value: v, .. }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::IntSliderChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::IntSlider { value: v, .. }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::BoolChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::BoolToggle { value: v, .. }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::EdgeCurveChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::EdgeCurveSelector { value: v }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::PinShapeChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::PinShapeSelector { value: v }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::PatternTypeChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::PatternTypeSelector { value: v }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::BackgroundPatternChanged { node_id, value } => {
                if let Some((
                    _,
                    NodeType::Input(InputNodeType::BackgroundPatternSelector { value: v }),
                )) = self.nodes.get_mut(&node_id)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::ColorChanged { node_id, color } => {
                if let Some((_, node_type)) = self.nodes.get_mut(&node_id) {
                    match node_type {
                        NodeType::Input(InputNodeType::ColorPicker { color: c }) => {
                            *c = color;
                            self.propagate_values();
                        }
                        NodeType::Input(InputNodeType::ColorPreset { color: c }) => {
                            *c = color;
                            self.propagate_values();
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            ApplicationMessage::ToggleNodeExpanded { node_id } => {
                if self.expanded_nodes.contains(&node_id) {
                    self.expanded_nodes.remove(&node_id);
                } else {
                    self.expanded_nodes.insert(node_id);
                }
                Task::none()
            }
            ApplicationMessage::UpdateFloatSliderConfig { node_id, config } => {
                if let Some((_, NodeType::Input(InputNodeType::FloatSlider { config: c, value }))) =
                    self.nodes.get_mut(&node_id)
                {
                    // Clamp value to new range if needed
                    *value = value.clamp(config.min, config.max);
                    *c = config;
                }
                Task::none()
            }
            ApplicationMessage::UpdateIntSliderConfig { node_id, config } => {
                if let Some((_, NodeType::Input(InputNodeType::IntSlider { config: c, value }))) =
                    self.nodes.get_mut(&node_id)
                {
                    // Clamp value to new range if needed
                    *value = (*value).clamp(config.min, config.max);
                    *c = config;
                }
                Task::none()
            }
            ApplicationMessage::ToggleEdgeSection { node_id, section } => {
                let sections = self.edge_config_sections
                    .entry(node_id)
                    .or_insert_with(EdgeSections::new_all_expanded);
                match section {
                    EdgeSection::Stroke => sections.stroke = !sections.stroke,
                    EdgeSection::Pattern => sections.pattern = !sections.pattern,
                    EdgeSection::Border => sections.border = !sections.border,
                    EdgeSection::Outline => sections.outline = !sections.outline,
                    EdgeSection::Shadow => sections.shadow = !sections.shadow,
                }
                Task::none()
            }
            ApplicationMessage::ToggleNodeSection { node_id, section } => {
                let sections = self.node_config_sections
                    .entry(node_id)
                    .or_insert_with(NodeSections::new_all_expanded);
                match section {
                    NodeSection::Fill => sections.fill = !sections.fill,
                    NodeSection::Border => sections.border = !sections.border,
                    NodeSection::Shadow => sections.shadow = !sections.shadow,
                }
                Task::none()
            }
        }
    }

    fn theme(&self) -> Theme {
        self.palette_preview_theme
            .as_ref()
            .unwrap_or(&self.current_theme)
            .clone()
    }

    fn get_main_commands_with_shortcuts() -> Vec<Command<ApplicationMessage>> {
        vec![
            command("add_node", "Add Node")
                .description("Add a new node to the graph")
                .shortcut(Shortcut::cmd('n'))
                .action(ApplicationMessage::ExecuteShortcut("add_node".to_string())),
            command("change_theme", "Change Theme")
                .description("Switch to a different color theme")
                .shortcut(Shortcut::cmd('t'))
                .action(ApplicationMessage::ExecuteShortcut(
                    "change_theme".to_string(),
                )),
            command("export_state", "Export State")
                .description("Export graph state to file for Claude")
                .shortcut(Shortcut::cmd('e'))
                .action(ApplicationMessage::ExecuteShortcut(
                    "export_state".to_string(),
                )),
        ]
    }

    fn get_available_themes() -> Vec<Theme> {
        vec![
            Theme::Dark,
            Theme::Light,
            Theme::Dracula,
            Theme::Nord,
            Theme::SolarizedLight,
            Theme::SolarizedDark,
            Theme::GruvboxLight,
            Theme::GruvboxDark,
            Theme::CatppuccinLatte,
            Theme::CatppuccinFrappe,
            Theme::CatppuccinMacchiato,
            Theme::CatppuccinMocha,
            Theme::TokyoNight,
            Theme::TokyoNightStorm,
            Theme::TokyoNightLight,
            Theme::KanagawaWave,
            Theme::KanagawaDragon,
            Theme::KanagawaLotus,
            Theme::Moonfly,
            Theme::Nightfly,
            Theme::Oxocarbon,
            Theme::Ferra,
        ]
    }

    fn get_theme_name(theme: &Theme) -> &'static str {
        match theme {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::Dracula => "Dracula",
            Theme::Nord => "Nord",
            Theme::SolarizedLight => "Solarized Light",
            Theme::SolarizedDark => "Solarized Dark",
            Theme::GruvboxLight => "Gruvbox Light",
            Theme::GruvboxDark => "Gruvbox Dark",
            Theme::CatppuccinLatte => "Catppuccin Latte",
            Theme::CatppuccinFrappe => "Catppuccin Frappe",
            Theme::CatppuccinMacchiato => "Catppuccin Macchiato",
            Theme::CatppuccinMocha => "Catppuccin Mocha",
            Theme::TokyoNight => "Tokyo Night",
            Theme::TokyoNightStorm => "Tokyo Night Storm",
            Theme::TokyoNightLight => "Tokyo Night Light",
            Theme::KanagawaWave => "Kanagawa Wave",
            Theme::KanagawaDragon => "Kanagawa Dragon",
            Theme::KanagawaLotus => "Kanagawa Lotus",
            Theme::Moonfly => "Moonfly",
            Theme::Nightfly => "Nightfly",
            Theme::Oxocarbon => "Oxocarbon",
            Theme::Ferra => "Ferra",
            _ => "Unknown",
        }
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
        use iced_nodegraph::{GraphStyle, NodeGraph, PinRef};

        // Use preview theme if active (for theme selection), otherwise current theme
        let theme = self
            .palette_preview_theme
            .as_ref()
            .unwrap_or(&self.current_theme);

        // Graph-wide node defaults - combine with per-node configs using merge()
        let node_defaults = NodeConfig::new().corner_radius(8.0).opacity(0.88);

        // Pin defaults from connected config nodes
        let pin_defaults = self.computed_style.to_pin_config();

        let mut ng: NodeGraph<
            '_,
            NodeId,
            PinLabel,
            EdgeId,
            ApplicationMessage,
            Theme,
            iced::Renderer,
        > = NodeGraph::default();

        ng = ng
            .on_connect(
                |from: PinRef<NodeId, PinLabel>, to: PinRef<NodeId, PinLabel>| {
                    ApplicationMessage::EdgeConnected { from, to }
                },
            )
            .on_disconnect(
                |from: PinRef<NodeId, PinLabel>, to: PinRef<NodeId, PinLabel>| {
                    ApplicationMessage::EdgeDisconnected { from, to }
                },
            )
            .on_move(|node_id, new_position| ApplicationMessage::NodeMoved {
                node_id,
                new_position,
            })
            .on_select(ApplicationMessage::SelectionChanged)
            .on_clone(ApplicationMessage::CloneNodes)
            .on_delete(ApplicationMessage::DeleteNodes)
            .on_group_move(|node_ids, delta| ApplicationMessage::GroupMoved { node_ids, delta })
            .on_camera_change(|position, zoom| ApplicationMessage::CameraChanged { position, zoom })
            .pin_defaults(pin_defaults)
            // Style callbacks - user controls appearance based on status
            // The base style comes from per-element config or theme defaults
            .node_style(|_theme, status, base| {
                use iced_nodegraph::NodeStatus;
                match status {
                    NodeStatus::Selected => base
                        .border_color(iced::Color::from_rgb(0.3, 0.6, 1.0))
                        .border_width(2.5),
                    NodeStatus::Idle => base,
                }
            })
            .pin_style(|_theme, _status, base| {
                // Pin animation (pulsing) is handled internally via scaled_radius()
                base
            })
            .edge_style(|_theme, status, base| {
                use iced_nodegraph::{EdgeStatus, StrokeStyle};
                match status {
                    EdgeStatus::PendingCut => {
                        // Override stroke color to red for edges being cut
                        let cut_stroke = StrokeStyle::new().color(iced::Color::from_rgb(1.0, 0.3, 0.3));
                        base.stroke(cut_stroke)
                    }
                    EdgeStatus::Idle => base,
                }
            })
            .box_select_style(|_theme| {
                (
                    iced::Color::from_rgba(0.3, 0.6, 1.0, 0.15), // fill
                    iced::Color::from_rgb(0.3, 0.6, 1.0),        // border
                )
            })
            .cutting_tool_style(|_theme| iced::Color::from_rgb(1.0, 0.3, 0.3));

        // Add all nodes from state (in order)
        for node_id in &self.node_order {
            let Some((position, node_type)) = self.nodes.get(node_id) else {
                continue;
            };
            let node_id_clone = node_id.clone();
            let element: iced::Element<'_, ApplicationMessage> = match node_type {
                NodeType::Workflow(name) => node(name.as_str(), theme),
                NodeType::Input(input) => match input {
                    InputNodeType::FloatSlider { config, value } => {
                        let id = node_id_clone.clone();
                        let expanded = self.expanded_nodes.contains(node_id);
                        float_slider_node(
                            theme,
                            *value,
                            config,
                            expanded,
                            {
                                let id = id.clone();
                                move |v| ApplicationMessage::SliderChanged {
                                    node_id: id.clone(),
                                    value: v,
                                }
                            },
                            {
                                let id = id.clone();
                                move |cfg| ApplicationMessage::UpdateFloatSliderConfig {
                                    node_id: id.clone(),
                                    config: cfg,
                                }
                            },
                            ApplicationMessage::ToggleNodeExpanded { node_id: id },
                        )
                    }
                    InputNodeType::IntSlider { config, value } => {
                        let id = node_id_clone.clone();
                        let expanded = self.expanded_nodes.contains(node_id);
                        int_slider_node(
                            theme,
                            *value,
                            config,
                            expanded,
                            {
                                let id = id.clone();
                                move |v| ApplicationMessage::IntSliderChanged {
                                    node_id: id.clone(),
                                    value: v,
                                }
                            },
                            {
                                let id = id.clone();
                                move |cfg| ApplicationMessage::UpdateIntSliderConfig {
                                    node_id: id.clone(),
                                    config: cfg,
                                }
                            },
                            ApplicationMessage::ToggleNodeExpanded { node_id: id },
                        )
                    }
                    InputNodeType::BoolToggle { config, value } => {
                        let id = node_id_clone.clone();
                        bool_toggle_node(theme, *value, config, move |v| {
                            ApplicationMessage::BoolChanged {
                                node_id: id.clone(),
                                value: v,
                            }
                        })
                    }
                    InputNodeType::EdgeCurveSelector { value } => {
                        let id = node_id_clone.clone();
                        edge_curve_selector_node(theme, *value, move |v| {
                            ApplicationMessage::EdgeCurveChanged {
                                node_id: id.clone(),
                                value: v,
                            }
                        })
                    }
                    InputNodeType::PinShapeSelector { value } => {
                        let id = node_id_clone.clone();
                        pin_shape_selector_node(theme, *value, move |v| {
                            ApplicationMessage::PinShapeChanged {
                                node_id: id.clone(),
                                value: v,
                            }
                        })
                    }
                    InputNodeType::PatternTypeSelector { value } => {
                        let id = node_id_clone.clone();
                        pattern_type_selector_node(theme, *value, move |v| {
                            ApplicationMessage::PatternTypeChanged {
                                node_id: id.clone(),
                                value: v,
                            }
                        })
                    }
                    InputNodeType::BackgroundPatternSelector { value } => {
                        let id = node_id_clone.clone();
                        background_pattern_selector_node(theme, *value, move |v| {
                            ApplicationMessage::BackgroundPatternChanged {
                                node_id: id.clone(),
                                value: v,
                            }
                        })
                    }
                    InputNodeType::ColorPicker { color } => {
                        let id = node_id_clone.clone();
                        color_picker_node(theme, *color, move |c| {
                            ApplicationMessage::ColorChanged {
                                node_id: id.clone(),
                                color: c,
                            }
                        })
                    }
                    InputNodeType::ColorPreset { color } => {
                        let id = node_id_clone.clone();
                        color_preset_node(theme, *color, move |c| {
                            ApplicationMessage::ColorChanged {
                                node_id: id.clone(),
                                color: c,
                            }
                        })
                    }
                },
                NodeType::Config(config) => match config {
                    ConfigNodeType::NodeConfig(inputs) => {
                        let id = node_id_clone.clone();
                        let sections = self.node_config_sections
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(NodeSections::new_all_expanded);
                        node_config_node(theme, inputs, &sections, move |section| {
                            ApplicationMessage::ToggleNodeSection { node_id: id.clone(), section }
                        })
                    }
                    ConfigNodeType::EdgeConfig(inputs) => {
                        let id = node_id_clone.clone();
                        let sections = self.edge_config_sections
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(EdgeSections::new_all_expanded);
                        edge_config_node(theme, inputs, &sections, move |section| {
                            ApplicationMessage::ToggleEdgeSection { node_id: id.clone(), section }
                        })
                    }
                    ConfigNodeType::ShadowConfig(inputs) => shadow_config_node(theme, inputs),
                    ConfigNodeType::PinConfig(inputs) => pin_config_node(theme, inputs),
                    ConfigNodeType::BackgroundConfig(inputs) => {
                        background_config_node(theme, inputs)
                    }
                    ConfigNodeType::ApplyToGraph {
                        has_node_config,
                        has_edge_config,
                        has_pin_config,
                        has_background_config,
                    } => apply_to_graph_node(
                        theme,
                        *has_node_config,
                        *has_edge_config,
                        *has_pin_config,
                        *has_background_config,
                    ),
                    ConfigNodeType::ApplyToNode {
                        has_node_config,
                        target_id,
                    } => apply_to_node_node(theme, *has_node_config, *target_id),
                },
                NodeType::Math(state) => math_node(theme, state),
            };

            // Apply computed style to workflow nodes only (not to input/config nodes)
            // Merge per-node config with defaults (per-node takes priority)
            if matches!(node_type, NodeType::Workflow(_)) {
                let config = self.computed_style.to_node_config().merge(&node_defaults);
                ng.push_node_styled(node_id.clone(), *position, element, config);
            } else {
                ng.push_node_styled(node_id.clone(), *position, element, node_defaults.clone());
            }
        }

        // Add stored edges with computed config
        let edge_config = self.computed_style.to_edge_config();
        for edge_id in &self.edge_order {
            if let Some(edge) = self.edges.get(edge_id) {
                let from = PinRef::new(edge.from_node.clone(), edge.from_pin);
                let to = PinRef::new(edge.to_node.clone(), edge.to_pin);
                ng.push_edge_styled(from, to, edge_config.clone());
            }
        }

        // Set edge defaults for dragging edge preview
        ng = ng.edge_defaults(edge_config);

        // Apply background config to graph style
        if let Some(bg_config) = &self.computed_style.background {
            let graph_style = GraphStyle::default().background(bg_config.resolve());
            ng = ng.graph_style(graph_style);
        }

        let graph_view: iced::Element<'_, ApplicationMessage> = ng.into();

        // Always use the same widget structure to preserve NodeGraph state
        // The command palette is conditionally shown as an overlay
        let overlay: iced::Element<'_, ApplicationMessage> = if self.command_palette_open {
            let (_, commands) = self.build_palette_commands();
            command_palette(
                &self.command_input,
                &commands,
                self.palette_selected_index,
                ApplicationMessage::CommandPaletteInput,
                ApplicationMessage::CommandPaletteSelect,
                ApplicationMessage::CommandPaletteNavigate,
                || ApplicationMessage::CommandPaletteCancel,
            )
        } else {
            // Invisible placeholder to maintain widget tree structure
            container(text("")).width(0).height(0).into()
        };

        stack!(graph_view, overlay)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn build_palette_commands(&self) -> (&'static str, Vec<Command<ApplicationMessage>>) {
        match &self.palette_view {
            PaletteView::Main => {
                let commands = vec![
                    command("add_node", "Add Node")
                        .description("Add a new node to the graph")
                        .shortcut(Shortcut::cmd('n'))
                        .action(ApplicationMessage::NavigateToSubmenu("nodes".to_string())),
                    command("change_theme", "Change Theme")
                        .description("Switch to a different color theme")
                        .shortcut(Shortcut::cmd('t'))
                        .action(ApplicationMessage::NavigateToSubmenu("themes".to_string())),
                    command("export_state", "Export State")
                        .description("Export graph state to file for Claude")
                        .shortcut(Shortcut::cmd('e'))
                        .action(ApplicationMessage::ExportState),
                ];
                ("Command Palette", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "nodes" => {
                let commands = vec![
                    // Workflow nodes
                    command("workflow", "Workflow Nodes")
                        .description("Original demo nodes")
                        .action(ApplicationMessage::NavigateToSubmenu(
                            "workflow_nodes".to_string(),
                        )),
                    // Input nodes
                    command("inputs", "Input Nodes")
                        .description("Sliders, color pickers, etc.")
                        .action(ApplicationMessage::NavigateToSubmenu(
                            "input_nodes".to_string(),
                        )),
                    // Math nodes
                    command("math", "Math Nodes")
                        .description("Add, Subtract, Multiply, Divide")
                        .action(ApplicationMessage::NavigateToSubmenu(
                            "math_nodes".to_string(),
                        )),
                    // Config nodes
                    command("config", "Style Config Nodes")
                        .description("Configure node and edge styling")
                        .action(ApplicationMessage::NavigateToSubmenu(
                            "config_nodes".to_string(),
                        )),
                ];
                ("Add Node", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "workflow_nodes" => {
                let workflow_nodes = vec!["email_trigger", "email_parser", "filter", "calendar"];
                let commands = workflow_nodes
                    .into_iter()
                    .map(|name| {
                        command(name, name).action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Workflow(name.to_string()),
                        })
                    })
                    .collect();
                ("Workflow Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "input_nodes" => {
                let commands = vec![
                    command("float_slider", "Float Slider")
                        .description("Generic float slider (0-20)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::default(),
                                value: 5.0,
                            }),
                        }),
                    command("pattern_angle", "Pattern Angle")
                        .description("Angle for Arrowed/Angled patterns (-90 to 90 degrees)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::pattern_angle(),
                                value: 45.0,
                            }),
                        }),
                    command("color_picker", "Color Picker (RGB)")
                        .description("Full RGB color picker with sliders")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::ColorPicker {
                                color: Color::from_rgb(0.5, 0.5, 0.5),
                            }),
                        }),
                    command("color_preset", "Color Presets")
                        .description("Quick color selection from presets")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::ColorPreset {
                                color: Color::from_rgb(0.5, 0.5, 0.5),
                            }),
                        }),
                    command("int_slider", "Int Slider")
                        .description("Integer slider (0-100)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::IntSlider {
                                config: IntSliderConfig::default(),
                                value: 50,
                            }),
                        }),
                    command("bool_toggle", "Boolean Toggle")
                        .description("Toggle for boolean values")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::BoolToggle {
                                config: BoolToggleConfig::default(),
                                value: true,
                            }),
                        }),
                    command("edge_curve", "Edge Curve Selector")
                        .description("Select edge curve (Bezier, Line, Orthogonal)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::EdgeCurveSelector {
                                value: EdgeCurve::BezierCubic,
                            }),
                        }),
                    command("pin_shape", "Pin Shape Selector")
                        .description("Select pin shape (Circle, Square, Diamond)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::PinShapeSelector {
                                value: PinShape::Circle,
                            }),
                        }),
                    command("pattern_type", "Pattern Type Selector")
                        .description("Select edge pattern (Solid, Dashed, Dotted)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::PatternTypeSelector {
                                value: PatternType::Solid,
                            }),
                        }),
                    command("bg_pattern", "Background Pattern Selector")
                        .description("Select background pattern (Grid, Hex, Dots, etc.)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::BackgroundPatternSelector {
                                value: PatternTypeSelection::Grid,
                            }),
                        }),
                ];
                ("Input Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "math_nodes" => {
                let commands = vec![
                    command("add", "Add").description("A + B").action(
                        ApplicationMessage::SpawnNode {
                            node_type: NodeType::Math(MathNodeState::new(MathOperation::Add)),
                        },
                    ),
                    command("subtract", "Subtract").description("A - B").action(
                        ApplicationMessage::SpawnNode {
                            node_type: NodeType::Math(MathNodeState::new(MathOperation::Subtract)),
                        },
                    ),
                    command("multiply", "Multiply").description("A * B").action(
                        ApplicationMessage::SpawnNode {
                            node_type: NodeType::Math(MathNodeState::new(MathOperation::Multiply)),
                        },
                    ),
                    command("divide", "Divide").description("A / B").action(
                        ApplicationMessage::SpawnNode {
                            node_type: NodeType::Math(MathNodeState::new(MathOperation::Divide)),
                        },
                    ),
                ];
                ("Math Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "config_nodes" => {
                let commands = vec![
                    command("node_config", "Node Config")
                        .description("Node config with all fields and inheritance")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::NodeConfig(
                                NodeConfigInputs::default(),
                            )),
                        }),
                    command("edge_config", "Edge Config")
                        .description("Edge config with colors, thickness, type")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::EdgeConfig(
                                EdgeConfigInputs::default(),
                            )),
                        }),
                    command("shadow_config", "Shadow Config")
                        .description("Shadow configuration with offset, blur, color")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::ShadowConfig(
                                ShadowConfigInputs::default(),
                            )),
                        }),
                    command("pin_config", "Pin Config")
                        .description("Pin configuration with shape, color, radius")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::PinConfig(
                                PinConfigInputs::default(),
                            )),
                        }),
                    command("background_config", "Background Config")
                        .description("Background pattern, colors, spacing, adaptive zoom")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::BackgroundConfig(
                                BackgroundConfigInputs::default(),
                            )),
                        }),
                    // Apply nodes
                    command("apply_to_graph", "Apply to Graph")
                        .description("Apply configs to all nodes/edges in graph")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::ApplyToGraph {
                                has_node_config: false,
                                has_edge_config: false,
                                has_pin_config: false,
                                has_background_config: false,
                            }),
                        }),
                    command("apply_to_node", "Apply to Node")
                        .description("Apply config to a specific node by ID")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::ApplyToNode {
                                has_node_config: false,
                                target_id: None,
                            }),
                        }),
                ];
                ("Style Config Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "themes" => {
                let commands = Self::get_available_themes()
                    .iter()
                    .map(|theme| {
                        let name = Self::get_theme_name(theme);
                        command(name, name).action(ApplicationMessage::ChangeTheme(theme.clone()))
                    })
                    .collect();
                ("Choose Theme", commands)
            }
            _ => ("Command Palette", vec![]),
        }
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        Subscription::batch(vec![
            event::listen_with(handle_keyboard_event),
            window::frames().map(|_| ApplicationMessage::Tick),
            event::listen_with(|event, _, _| match event {
                Event::Window(window::Event::Resized(size)) => {
                    Some(ApplicationMessage::WindowResized(size))
                }
                Event::Window(window::Event::Moved(position)) => {
                    Some(ApplicationMessage::WindowMoved(position))
                }
                _ => None,
            }),
        ])
    }
}

fn handle_keyboard_event(
    event: Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<ApplicationMessage> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            if is_toggle_shortcut(&key, modifiers) {
                return Some(ApplicationMessage::ToggleCommandPalette);
            }

            if modifiers.command() {
                let main_commands = Application::get_main_commands_with_shortcuts();
                if let Some(cmd_id) = find_matching_shortcut(&main_commands, &key, modifiers) {
                    return Some(ApplicationMessage::ExecuteShortcut(cmd_id.to_string()));
                }
            }

            match key {
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                    Some(ApplicationMessage::CommandPaletteNavigateUp)
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                    Some(ApplicationMessage::CommandPaletteNavigateDown)
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    Some(ApplicationMessage::CommandPaletteConfirm)
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    Some(ApplicationMessage::CommandPaletteCancel)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodes::{MathNodeState, MathOperation, NodeType};

    // === Math Operation Tests ===

    #[test]
    fn test_math_add() {
        let op = MathOperation::Add;
        assert_eq!(op.compute(5.0, 3.0), 8.0);
        assert_eq!(op.symbol(), "+");
        assert_eq!(op.name(), "Add");
    }

    #[test]
    fn test_math_subtract() {
        let op = MathOperation::Subtract;
        assert_eq!(op.compute(5.0, 3.0), 2.0);
        assert_eq!(op.compute(3.0, 5.0), -2.0);
        assert_eq!(op.symbol(), "-");
    }

    #[test]
    fn test_math_multiply() {
        let op = MathOperation::Multiply;
        assert_eq!(op.compute(5.0, 3.0), 15.0);
        assert_eq!(op.compute(0.0, 100.0), 0.0);
        assert_eq!(op.symbol(), "*");
    }

    #[test]
    fn test_math_divide() {
        let op = MathOperation::Divide;
        assert_eq!(op.compute(6.0, 2.0), 3.0);
        assert_eq!(op.symbol(), "/");
    }

    #[test]
    fn test_math_divide_by_zero() {
        let op = MathOperation::Divide;
        let result = op.compute(5.0, 0.0);
        assert!(result.is_infinite());
    }

    // === MathNodeState Tests ===

    #[test]
    fn test_math_node_result_with_both_inputs() {
        let mut state = MathNodeState::new(MathOperation::Add);
        state.input_a = Some(10.0);
        state.input_b = Some(5.0);
        assert_eq!(state.result(), Some(15.0));
    }

    #[test]
    fn test_math_node_result_with_missing_a() {
        let mut state = MathNodeState::new(MathOperation::Add);
        state.input_a = None;
        state.input_b = Some(5.0);
        assert_eq!(state.result(), None);
    }

    #[test]
    fn test_math_node_result_with_missing_b() {
        let mut state = MathNodeState::new(MathOperation::Add);
        state.input_a = Some(10.0);
        state.input_b = None;
        assert_eq!(state.result(), None);
    }

    // === NodeType Output Value Tests ===

    #[test]
    fn test_math_node_output_value() {
        let mut state = MathNodeState::new(MathOperation::Multiply);
        state.input_a = Some(4.0);
        state.input_b = Some(3.0);
        let node_type = NodeType::Math(state);

        let output = node_type.output_value();
        assert!(output.is_some());
        if let Some(NodeValue::Float(f)) = output {
            assert_eq!(f, 12.0);
        } else {
            panic!("Expected Float value");
        }
    }

    #[test]
    fn test_math_node_output_value_no_result() {
        let state = MathNodeState::new(MathOperation::Add); // No inputs
        let node_type = NodeType::Math(state);
        assert!(node_type.output_value().is_none());
    }

    #[test]
    fn test_input_node_output_value() {
        let input = InputNodeType::FloatSlider {
            config: FloatSliderConfig::default(),
            value: 7.5,
        };
        let node_type = NodeType::Input(input);

        let output = node_type.output_value();
        assert!(output.is_some());
        if let Some(NodeValue::Float(f)) = output {
            assert!((f - 7.5).abs() < 0.001);
        } else {
            panic!("Expected Float value");
        }
    }

    // === ComputedStyle Tests ===

    #[test]
    fn test_computed_style_to_pin_config_empty() {
        let style = ComputedStyle::default();
        let config = style.to_pin_config();
        // Empty style should produce empty config
        assert!(config.color.is_none());
        assert!(config.radius.is_none());
        assert!(config.shape.is_none());
    }

    #[test]
    fn test_computed_style_to_pin_config_with_values() {
        let mut style = ComputedStyle::default();
        style.pin_color = Some(Color::from_rgb(1.0, 0.0, 0.0));
        style.pin_radius = Some(10.0);
        style.pin_shape = Some(PinShape::Diamond);

        let config = style.to_pin_config();
        assert_eq!(config.color, Some(Color::from_rgb(1.0, 0.0, 0.0)));
        assert_eq!(config.radius, Some(10.0));
        assert_eq!(config.shape, Some(PinShape::Diamond));
    }

    #[test]
    fn test_computed_style_to_node_config() {
        let mut style = ComputedStyle::default();
        style.corner_radius = Some(12.0);
        style.opacity = Some(0.8);
        style.fill_color = Some(Color::from_rgb(0.2, 0.3, 0.4));

        let config = style.to_node_config();
        assert_eq!(config.corner_radius, Some(12.0));
        assert_eq!(config.opacity, Some(0.8));
        assert_eq!(config.fill_color, Some(Color::from_rgb(0.2, 0.3, 0.4)));
    }

    #[test]
    fn test_computed_style_to_edge_config() {
        let mut style = ComputedStyle::default();
        style.edge_thickness = Some(3.0);
        style.edge_color = Some(Color::from_rgb(0.5, 0.5, 0.5));

        let config = style.to_edge_config();
        let stroke = config.stroke.as_ref().expect("should have stroke config");
        assert_eq!(stroke.width, Some(3.0));
        assert_eq!(stroke.start_color, Some(Color::from_rgb(0.5, 0.5, 0.5)));
        assert_eq!(stroke.end_color, Some(Color::from_rgb(0.5, 0.5, 0.5)));
    }
}
