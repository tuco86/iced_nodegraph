// `SpawnNode` carries a whole `NodeType`, whose config variants hold every
// style field of a node, so the message enum is inherently lopsided.
#![allow(clippy::large_enum_variant)]
#![doc = include_str!("../README.md")]
#![doc = r#"<link rel="stylesheet" href="../gallery/pkg/demo.css"><script type="module" src="../gallery/pkg/demo-loader.js"></script>"#]

mod ids;
mod nodes;
#[cfg(not(target_arch = "wasm32"))]
mod persistence;
mod rig;
mod style_overlay;

// The trait methods are named directly only by the native `main`.
#[cfg(not(target_arch = "wasm32"))]
use demo_common::Demo;
use iced::{
    Color, Event, Length, Point, Size, Subscription, Task, Theme, Vector, event, keyboard,
    widget::{container, opaque, stack, text},
    window,
};
use iced_nodegraph::{
    AnchorStatus, AnchorStyle, ColorQuad, CuttingToolStyle, EdgeStatus, EdgeStyle, FocusOptions,
    FocusTarget, GraphStyle, Minimap, MinimapStyle, NodeStatus, NodeStyle, PinRef, PinStatus,
    PinStyle, SelectionBoxStyle, anchor as ng_anchor, default_anchor_style,
    default_cutting_tool_style, default_edge_style, default_graph_style, default_minimap_style,
    default_node_style, default_pin_style, default_selection_box_style, edge as ng_edge,
    node as ng_node,
};
use iced_nodegraph::{EdgeCurve, PinShape, TilingKind};
use iced_palette::{
    Command, Shortcut, command, command_palette, find_matching_shortcut, focus_input,
    get_filtered_command_index, get_filtered_count, is_toggle_shortcut, navigate_down, navigate_up,
};
use ids::{EdgeData, EdgeId, HelloIds, NodeId, PinLabel, generate_edge_id, generate_node_id};
use nodes::{
    AlphaNode, AnchorConfigInputs, BoolToggleConfig, ClassCandidate, ColorQuadNode, ConfigNodeType,
    CuttingToolConfigInputs, EdgeConfigInputs, EdgeSection, EdgeSections, FloatSliderConfig,
    GraphConfigInputs, InputNodeType, IntSliderConfig, MathNodeState, MathOperation,
    MinimapConfigInputs, NodeConfigInputs, NodeSection, NodeSections, NodeType, NodeValue,
    PatternType, PinConfigInputs, SelectionBoxConfigInputs, Vec2Node, alpha_node,
    anchor_config_node, bool_toggle_node, catalog_node, color_picker_node, color_preset_node,
    color_quad_node, cutting_tool_config_node, edge_config_node, edge_curve_selector_node,
    float_slider_node, frame_node, graph_config_node, int_slider_node, math_node,
    minimap_config_node, node, node_class_node, node_config_node, pattern_type_selector_node,
    pin_config_node, pin_shape_selector_node, selection_box_config_node, theme_extended_node,
    theme_node, tiling_kind_selector_node, vec2_node,
};
use std::collections::{HashMap, HashSet};
use style_overlay::{
    AnchorOverlay, CuttingToolOverlay, EdgeOverlay, GraphOverlay, MinimapOverlay, NodeOverlay,
    PinOverlay, SelectionBoxOverlay,
};

/// Runs the demo natively, restoring the persisted graph and window geometry.
///
/// The gallery boots the pristine scene through [`Demo::boot`] instead, so this
/// entry point and the `persistence` module it needs are native-only.
#[cfg(not(target_arch = "wasm32"))]
pub fn main() -> iced::Result {
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

    iced::application(
        Application::new_persisted,
        Application::update,
        Application::view,
    )
    .subscription(Application::subscription)
    .title("Hello World - iced_nodegraph Demo")
    .theme(Application::theme)
    .window(window_settings)
    .run()
}

#[derive(Debug, Clone)]
enum ApplicationMessage {
    EdgeConnected {
        from: PinRef<HelloIds>,
        to: PinRef<HelloIds>,
    },
    EdgeDisconnected {
        from: PinRef<HelloIds>,
        to: PinRef<HelloIds>,
    },
    /// Edges the cutting tool destroyed, named by their own ids.
    EdgesCut(Vec<EdgeId>),
    // Routing anchors
    AnchorCreated {
        edge: EdgeId,
        position: Point,
    },
    AnchorMoved {
        anchor: usize,
        position: Point,
    },
    AnchorDeleted(usize),
    RouteAttached {
        edge: EdgeId,
        anchor: usize,
    },
    RouteDetached {
        edge: EdgeId,
        anchor: usize,
    },
    /// Every gesture that can orphan an anchor ends here, which is when one no
    /// cable names any more is safe to drop: during a drag it is still a target
    /// the same drag may put the cable back onto.
    DragEnded,
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
    /// Flies the camera to the fixed demo node.
    FocusNode,
    WindowResized(iced::Size),
    WindowMoved(Point),
    WindowMaximizedChanged(bool),
    NavigateToSubmenu(String),
    // Selection-related messages
    SelectionChanged(Vec<NodeId>),
    CloneNodes(Vec<NodeId>),
    DeleteNodes(Vec<NodeId>),
    NodesMoved {
        delta: Vector,
        node_ids: Vec<NodeId>,
    },
    // State export for Claude
    ExportState,
    /// Reset the entire app to its initial state (clears graph, config, and the
    /// persisted save file).
    Reset,
    /// Move keyboard focus to the next / previous focusable widget (Tab /
    /// Shift+Tab), e.g. between a slider node's min/max/step fields.
    FocusNext,
    FocusPrevious,
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
    TilingKindChanged {
        node_id: NodeId,
        value: TilingKind,
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
    /// The Node Class node's pick list chose (or lost) its target node.
    NodeClassTargetChanged {
        node_id: NodeId,
        target: Option<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum PaletteView {
    Main,
    Submenu(String),
}

/// Output types from config nodes for propagation
#[derive(Debug, Clone)]
enum ConfigOutput {
    Node(NodeOverlay),
    Edge(EdgeOverlay),
    Pin(PinOverlay),
    Graph(GraphOverlay),
    Anchor(AnchorOverlay),
    SelectionBox(SelectionBoxOverlay),
    CuttingTool(CuttingToolOverlay),
    Minimap(MinimapOverlay),
}

/// Maps a pin's data-type marker to its semantic color. Pins carry no style;
/// the owning node colors them via `pin_style`, keyed on this.
fn pin_color_for(ty: std::any::TypeId) -> Color {
    use nodes::{colors, pins};
    use std::any::TypeId;
    if ty == TypeId::of::<pins::ColorData>() {
        colors::PIN_COLOR
    } else if ty == TypeId::of::<pins::Float>() || ty == TypeId::of::<pins::Int>() {
        colors::PIN_NUMBER
    } else if ty == TypeId::of::<pins::Bool>() {
        colors::PIN_BOOL
    } else if ty == TypeId::of::<pins::StringData>() {
        colors::PIN_STRING
    } else if ty == TypeId::of::<pins::Email>() {
        colors::PIN_EMAIL
    } else if ty == TypeId::of::<pins::DateTime>() {
        colors::PIN_DATETIME
    } else if ty == TypeId::of::<pins::NodeConfigData>()
        || ty == TypeId::of::<pins::EdgeConfigData>()
        || ty == TypeId::of::<pins::PinConfigData>()
        || ty == TypeId::of::<pins::GraphConfigData>()
        || ty == TypeId::of::<pins::AnchorConfigData>()
        || ty == TypeId::of::<pins::SelectionBoxConfigData>()
        || ty == TypeId::of::<pins::CuttingToolConfigData>()
        || ty == TypeId::of::<pins::MinimapConfigData>()
    {
        colors::PIN_CONFIG
    } else {
        colors::PIN_ANY
    }
}

/// The widget id of the demo's graph, addressed by the
/// [`iced_nodegraph::focus`] task the "Focus node" command runs.
fn graph_id() -> iced::widget::Id {
    iced::widget::Id::new("graph")
}

/// The overlays the config rig produced, one per `iced_nodegraph::Catalog`
/// class and status, resolved against the theme base at draw time. A status
/// overlay is layered over its idle base by the accessors, the way the
/// library's `Selected` default is a struct-update over the idle one.
#[derive(Debug, Clone, Default)]
struct ComputedCatalog {
    node: NodeOverlay,
    node_selected: NodeOverlay,
    pin: PinOverlay,
    pin_valid_target: PinOverlay,
    edge: EdgeOverlay,
    edge_pending_cut: EdgeOverlay,
    drag_edge: EdgeOverlay,
    anchor: AnchorOverlay,
    anchor_hovered: AnchorOverlay,
    anchor_valid_target: AnchorOverlay,
    graph: GraphOverlay,
    selection_box: SelectionBoxOverlay,
    cutting_tool: CuttingToolOverlay,
    minimap: MinimapOverlay,
    /// Per-node classes from `NodeClass` nodes, keyed by target node.
    node_classes: HashMap<NodeId, NodeOverlay>,
}

impl ComputedCatalog {
    fn node(&self, status: NodeStatus) -> NodeOverlay {
        match status {
            NodeStatus::Idle => self.node.clone(),
            NodeStatus::Selected => self.node_selected.merge(&self.node),
        }
    }

    fn pin(&self, status: PinStatus) -> PinOverlay {
        match status {
            PinStatus::Idle => self.pin.clone(),
            PinStatus::ValidTarget => self.pin_valid_target.merge(&self.pin),
        }
    }

    fn edge(&self, status: EdgeStatus) -> EdgeOverlay {
        match status {
            EdgeStatus::Idle => self.edge.clone(),
            EdgeStatus::PendingCut => self.edge_pending_cut.merge(&self.edge),
        }
    }

    fn anchor(&self, status: AnchorStatus) -> AnchorOverlay {
        match status {
            AnchorStatus::Idle => self.anchor.clone(),
            AnchorStatus::Hovered => self.anchor_hovered.merge(&self.anchor),
            AnchorStatus::ValidTarget => self.anchor_valid_target.merge(&self.anchor),
        }
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
    /// The overlays the config rig produced this propagation
    computed: ComputedCatalog,
    /// Config outputs queued per sink node (Catalog or Node Class) until phase 4
    pending_configs: HashMap<NodeId, Vec<(PinLabel, ConfigOutput)>>,
    /// Anchor id paired with its world position.
    anchors: Vec<(usize, Point)>,
    /// The next anchor id to mint. Only ever grows, so a deleted anchor's id is
    /// never handed to a different anchor.
    next_anchor: usize,
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

        let mut node_order = vec![
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
            EdgeData::new(
                node0_id.clone(),
                workflow::ON_EMAIL,
                node1_id.clone(),
                workflow::EMAIL,
            ),
        );
        edge_order.push(edge0_id);

        // Edge 1: email_parser "subject" -> filter "input"
        let edge1_id = generate_edge_id();
        edges.insert(
            edge1_id.clone(),
            EdgeData::new(
                node1_id.clone(),
                workflow::SUBJECT,
                node2_id.clone(),
                workflow::INPUT,
            ),
        );
        edge_order.push(edge1_id);

        // Edge 2: email_parser "datetime" -> calendar "datetime", wrapped
        // around the boot anchor so the anchor class is visible at boot.
        let edge2_id = generate_edge_id();
        edges.insert(
            edge2_id.clone(),
            EdgeData {
                route: vec![0],
                ..EdgeData::new(
                    node1_id.clone(),
                    workflow::DATETIME,
                    node3_id.clone(),
                    workflow::DATETIME,
                )
            },
        );
        edge_order.push(edge2_id);

        // Edge 3: filter "matches" -> calendar "title"
        let edge3_id = generate_edge_id();
        edges.insert(
            edge3_id.clone(),
            EdgeData::new(
                node2_id.clone(),
                workflow::MATCHES,
                node3_id.clone(),
                workflow::TITLE,
            ),
        );
        edge_order.push(edge3_id);

        // The config rig sits to the right of the workflow (which ends at x
        // about 900) and classes the calendar node.
        let rig = rig::build(Point::new(1000.0, 0.0), node3_id);
        for (id, position, node_type) in rig.nodes {
            nodes.insert(id.clone(), (position, node_type));
            node_order.push(id);
        }
        for (id, edge) in rig.edges {
            edges.insert(id.clone(), edge);
            edge_order.push(id);
        }

        let mut app = Self {
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
            computed: ComputedCatalog::default(),
            pending_configs: HashMap::new(),
            anchors: vec![(0, Point::new(560.0, 160.0))],
            next_anchor: 1,
            viewport_size: iced::Size::new(800.0, 600.0), // Default size
            camera_position: Point::ORIGIN,
            camera_zoom: 1.0,
            window_position: None,
            window_size: None,
            window_maximized: None,
        };
        // The rig is wired, so the catalog is resolved from the first frame.
        app.propagate_values();
        app
    }
}

impl Application {
    #[cfg(not(target_arch = "wasm32"))]
    fn new_persisted() -> Self {
        // Try to load saved state, fall back to default
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
                        anchors,
                        next_anchor,
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
                        anchors,
                        next_anchor,
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

    /// Resets the entire app to its initial state, keeping only the live window
    /// geometry so the OS window does not jump. Clears any stale config/style and
    /// overwrites the persisted save file with the fresh default.
    fn reset_to_default(&mut self) {
        let viewport_size = self.viewport_size;
        let window_position = self.window_position;
        let window_size = self.window_size;
        let window_maximized = self.window_maximized;
        *self = Self {
            viewport_size,
            window_position,
            window_size,
            window_maximized,
            ..Self::default()
        };
        self.propagate_values();
        self.save_state();
    }

    /// Saves current state to disk (native only). Tests drive `update`
    /// freely, so under `cfg(test)` nothing touches the user's save file.
    #[cfg(all(not(target_arch = "wasm32"), not(test)))]
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
            &self.anchors,
            self.next_anchor,
        );
        if let Err(e) = persistence::save_state(&saved) {
            eprintln!("Failed to save state: {}", e);
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn save_state(&self) {}

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
            && let Err(e) = std::fs::create_dir(out_dir)
        {
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
                    NodeType::ColorQuad(state) => {
                        output.push_str(&format!("  Type: ColorQuad({:?})\n", state));
                    }
                    NodeType::Vec2(state) => {
                        output.push_str(&format!("  Type: Vec2({:?})\n", state));
                    }
                    NodeType::Alpha(state) => {
                        output.push_str(&format!("  Type: Alpha({:?})\n", state));
                    }
                    NodeType::Theme => {
                        output.push_str("  Type: Theme\n");
                    }
                    NodeType::ThemeExtended => {
                        output.push_str("  Type: ThemeExtended\n");
                    }
                    NodeType::Frame { label, size } => {
                        output.push_str(&format!(
                            "  Type: Frame(\"{}\", {:.0}x{:.0})\n",
                            label, size.width, size.height
                        ));
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

    /// The theme the rig reads palette colors from: the palette preview while
    /// one is open, otherwise the applied theme.
    fn effective_theme(&self) -> &Theme {
        self.palette_preview_theme
            .as_ref()
            .unwrap_or(&self.current_theme)
    }

    /// The value a node emits on a given output pin. For most nodes this is the
    /// pin-agnostic `output_value()`; the Theme nodes resolve per-pin colors from
    /// the effective theme.
    fn pin_output_value(&self, node_id: &NodeId, pin: &PinLabel) -> Option<NodeValue> {
        match self.nodes.get(node_id) {
            Some((_, NodeType::Theme | NodeType::ThemeExtended)) => {
                theme_color(self.effective_theme(), pin).map(NodeValue::Color)
            }
            Some((_, node_type)) => node_type.output_value(),
            None => None,
        }
    }

    /// Recomputes every config node's inputs and the [`ComputedCatalog`] from
    /// the current edges, in four phases: reset, feed the combiner nodes until
    /// stable, feed the config nodes, then collect what reaches a sink.
    fn propagate_values(&mut self) {
        let mut new_computed = ComputedCatalog::default();
        self.pending_configs.clear();

        // Phase 1: Reset all config node and combiner node inputs to defaults
        for (_, node_type) in self.nodes.values_mut() {
            match node_type {
                NodeType::Config(config) => match config {
                    ConfigNodeType::NodeConfig(inputs) => *inputs = NodeConfigInputs::default(),
                    ConfigNodeType::EdgeConfig(inputs) => *inputs = EdgeConfigInputs::default(),
                    ConfigNodeType::PinConfig(inputs) => *inputs = PinConfigInputs::default(),
                    ConfigNodeType::GraphConfig(inputs) => *inputs = GraphConfigInputs::default(),
                    ConfigNodeType::AnchorConfig(inputs) => *inputs = AnchorConfigInputs::default(),
                    ConfigNodeType::SelectionBoxConfig(inputs) => {
                        *inputs = SelectionBoxConfigInputs::default()
                    }
                    ConfigNodeType::CuttingToolConfig(inputs) => {
                        *inputs = CuttingToolConfigInputs::default()
                    }
                    ConfigNodeType::MinimapConfig(inputs) => {
                        *inputs = MinimapConfigInputs::default()
                    }
                    ConfigNodeType::Catalog { connected } => connected.clear(),
                    // `target` is host state and survives; only the connection
                    // flag is recomputed.
                    ConfigNodeType::NodeClass {
                        has_node_config, ..
                    } => *has_node_config = false,
                },
                NodeType::Math(state) => {
                    state.input_a = None;
                    state.input_b = None;
                }
                NodeType::ColorQuad(state) => *state = ColorQuadNode::default(),
                NodeType::Vec2(state) => *state = Vec2Node::default(),
                NodeType::Alpha(state) => *state = AlphaNode::default(),
                _ => {}
            }
        }

        // Phase 1.5: Propagate values INTO combiner nodes (iteratively for chaining)
        // Combiners can be chained (e.g., (A+B)*C, palette -> Alpha), so we
        // iterate until stable
        let edges_snapshot: Vec<_> = self.edges.values().cloned().collect();

        // We need multiple passes because combiner chains require the source
        // to have computed its result before the target can use it
        const MAX_ITERATIONS: usize = 10;
        for _ in 0..MAX_ITERATIONS {
            let mut changed = false;

            for edge in &edges_snapshot {
                // Feed the target's combiner inputs from the source's output, in
                // both edge directions (edges connect either way).
                let forward = self.pin_output_value(&edge.from_node, &edge.from_pin);
                if let Some(value) = forward
                    && let Some((_, node)) = self.nodes.get_mut(&edge.to_node)
                    && feed_combiner_input(node, &edge.to_pin, &value)
                {
                    changed = true;
                }

                let reverse = self.pin_output_value(&edge.to_node, &edge.to_pin);
                if let Some(value) = reverse
                    && let Some((_, node)) = self.nodes.get_mut(&edge.from_node)
                    && feed_combiner_input(node, &edge.from_pin, &value)
                {
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        // Phase 2: Feed every value-emitting node (inputs, combiners, theme
        // pins) into the config node at the other end, in both edge directions.
        // Config nodes emit no pin value, so a config -> config edge is skipped.
        for edge in &edges_snapshot {
            if let Some(value) = self.pin_output_value(&edge.from_node, &edge.from_pin) {
                self.apply_value_to_config_node(&edge.to_node, &edge.to_pin, &value);
            }
            if let Some(value) = self.pin_output_value(&edge.to_node, &edge.to_pin) {
                self.apply_value_to_config_node(&edge.from_node, &edge.from_pin, &value);
            }
        }

        // Phase 3: After all inputs applied, process Config -> sink connections
        // Now config nodes have their updated inputs, so we can build configs
        for edge in &edges_snapshot {
            if self.is_sink(&edge.to_node) {
                self.connect_config_to_sink(&edge.from_node, &edge.to_node, &edge.to_pin);
            }
            if self.is_sink(&edge.from_node) {
                self.connect_config_to_sink(&edge.to_node, &edge.from_node, &edge.from_pin);
            }
        }

        // Phase 4: Collect what reached a sink into the computed catalog
        self.apply_sinks(&mut new_computed);

        self.computed = new_computed;
    }

    /// Whether `id` names a Catalog or Node Class node.
    fn is_sink(&self, id: &NodeId) -> bool {
        matches!(
            self.nodes.get(id),
            Some((
                _,
                NodeType::Config(ConfigNodeType::Catalog { .. } | ConfigNodeType::NodeClass { .. })
            ))
        )
    }

    /// Applies an input value to a specific pin on a config node
    fn apply_value_to_config_node(
        &mut self,
        node_id: &NodeId,
        pin_label: &PinLabel,
        value: &NodeValue,
    ) {
        use nodes::pins::{
            anchor as apin, cutting_tool as cpin, edge as epin, graph as gpin, minimap as mpin,
            node as npin, pin as ppin, selection_box as spin,
        };

        let Some((_, node_type)) = self.nodes.get_mut(node_id) else {
            return;
        };

        let NodeType::Config(config) = node_type else {
            return;
        };

        match config {
            ConfigNodeType::NodeConfig(inputs) => {
                // NodeConfig pin labels mirror NodeStyle: fill, border, pattern, shadow
                if *pin_label == npin::FILL_COLOR {
                    inputs.fill_color = value.as_color_quad();
                } else if *pin_label == npin::CORNER_RADIUS {
                    inputs.corner_radius = value.as_float();
                } else if *pin_label == npin::OPACITY {
                    inputs.opacity = value.as_float();
                } else if *pin_label == npin::BORDER_COLOR {
                    inputs.border_color = value.as_color_quad();
                } else if *pin_label == npin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                } else if *pin_label == npin::BORDER_OUTLINE_WIDTH {
                    inputs.border_outline_width = value.as_float();
                } else if *pin_label == npin::BORDER_OUTLINE_COLOR {
                    inputs.border_outline_color = value.as_color_quad();
                } else if *pin_label == npin::PATTERN {
                    inputs.pattern_type = value.as_pattern_type();
                } else if *pin_label == npin::DASH {
                    inputs.dash_length = value.as_float();
                } else if *pin_label == npin::GAP {
                    inputs.gap_length = value.as_float();
                } else if *pin_label == npin::ANGLE {
                    // Convert degrees from slider to radians for pattern angle
                    inputs.pattern_angle = value.as_float().map(|deg| deg.to_radians());
                } else if *pin_label == npin::SPEED {
                    inputs.animation_speed = value.as_float();
                } else if *pin_label == npin::SHADOW_COLOR {
                    inputs.shadow_color = value.as_color_quad();
                } else if *pin_label == npin::SHADOW_DISTANCE {
                    inputs.shadow_distance = value.as_float();
                } else if *pin_label == npin::SHADOW_OFFSET {
                    inputs.shadow_offset = value.as_vec2();
                }
            }
            ConfigNodeType::EdgeConfig(inputs) => {
                // EdgeConfig pin labels
                if *pin_label == epin::STROKE_COLOR {
                    inputs.stroke_color = value.as_color_quad();
                } else if *pin_label == epin::THICKNESS {
                    inputs.thickness = value.as_float();
                } else if *pin_label == epin::CURVE {
                    inputs.curve = value.as_edge_curve();
                } else if *pin_label == epin::PATTERN {
                    inputs.pattern_type = value.as_pattern_type();
                } else if *pin_label == epin::DASH {
                    inputs.dash_length = value.as_float();
                } else if *pin_label == epin::GAP {
                    inputs.gap_length = value.as_float();
                } else if *pin_label == epin::DOT_RADIUS {
                    inputs.dot_radius = value.as_float();
                } else if *pin_label == epin::ANGLE {
                    // Convert degrees from slider to radians for pattern angle
                    inputs.pattern_angle = value.as_float().map(|deg| deg.to_radians());
                } else if *pin_label == epin::SPEED {
                    inputs.animation_speed = value.as_float();
                // Stroke outline
                } else if *pin_label == epin::STROKE_OUTLINE_WIDTH {
                    inputs.stroke_outline_width = value.as_float();
                } else if *pin_label == epin::STROKE_OUTLINE_COLOR {
                    inputs.stroke_outline_color = value.as_color_quad();
                // Border settings
                } else if *pin_label == epin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                } else if *pin_label == epin::BORDER_GAP {
                    inputs.border_gap = value.as_float();
                } else if *pin_label == epin::BORDER_COLOR {
                    inputs.border_color = value.as_color_quad();
                } else if *pin_label == epin::BORDER_BACKGROUND {
                    inputs.border_background = value.as_color_quad();
                } else if *pin_label == epin::BORDER_OUTLINE_WIDTH {
                    inputs.border_outline_width = value.as_float();
                } else if *pin_label == epin::BORDER_OUTLINE_COLOR {
                    inputs.border_outline_color = value.as_color_quad();
                // Shadow settings
                } else if *pin_label == epin::SHADOW_BLUR {
                    inputs.shadow_blur = value.as_float();
                } else if *pin_label == epin::SHADOW_EXPAND {
                    inputs.shadow_expand = value.as_float();
                } else if *pin_label == epin::SHADOW_COLOR {
                    inputs.shadow_color = value.as_color_quad();
                } else if *pin_label == epin::SHADOW_OFFSET {
                    inputs.shadow_offset = value.as_vec2();
                }
            }
            ConfigNodeType::PinConfig(inputs) => {
                // PinConfig pin labels mirror PinStyle
                if *pin_label == ppin::COLOR {
                    inputs.color = value.as_color_quad();
                } else if *pin_label == ppin::RADIUS {
                    inputs.radius = value.as_float();
                } else if *pin_label == ppin::CUTOUT_RADIUS {
                    inputs.cutout_radius = value.as_float();
                } else if *pin_label == ppin::SHAPE {
                    inputs.shape = value.as_pin_shape();
                } else if *pin_label == ppin::BORDER_COLOR {
                    inputs.border_color = value.as_color_quad();
                } else if *pin_label == ppin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                }
            }
            ConfigNodeType::GraphConfig(inputs) => {
                // GraphConfig pin labels mirror GraphStyle + TilingBackground.
                if *pin_label == gpin::BACKGROUND {
                    inputs.background_color = value.as_color_quad();
                } else if *pin_label == gpin::TILING_KIND {
                    inputs.tiling_kind = value.as_tiling_kind();
                } else if *pin_label == gpin::SPACING {
                    inputs.tiling_spacing = value.as_float();
                } else if *pin_label == gpin::THICKNESS {
                    inputs.tiling_thickness = value.as_float();
                } else if *pin_label == gpin::LINE_COLOR {
                    inputs.tiling_color = value.as_color_quad();
                }
            }
            ConfigNodeType::AnchorConfig(inputs) => {
                if *pin_label == apin::CORE_SIZE {
                    inputs.core_size = value.as_float();
                } else if *pin_label == apin::CORE_RADIUS {
                    inputs.core_radius = value.as_float();
                } else if *pin_label == apin::CORE_COLOR {
                    inputs.core_color = value.as_color_quad();
                } else if *pin_label == apin::CORE_BORDER_COLOR {
                    inputs.core_border_color = value.as_color_quad();
                } else if *pin_label == apin::CORE_BORDER_WIDTH {
                    inputs.core_border_width = value.as_float();
                } else if *pin_label == apin::ORBIT_OFFSET {
                    inputs.orbit_offset = value.as_float();
                } else if *pin_label == apin::ORBIT_SPACING {
                    inputs.orbit_spacing = value.as_float();
                } else if *pin_label == apin::RING_COLOR {
                    inputs.ring_color = value.as_color_quad();
                } else if *pin_label == apin::RING_WIDTH {
                    inputs.ring_width = value.as_float();
                }
            }
            ConfigNodeType::SelectionBoxConfig(inputs) => {
                if *pin_label == spin::FILL {
                    inputs.fill = value.as_color_quad();
                } else if *pin_label == spin::BORDER_COLOR {
                    inputs.border_color = value.as_color_quad();
                } else if *pin_label == spin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                }
            }
            ConfigNodeType::CuttingToolConfig(inputs) => {
                if *pin_label == cpin::COLOR {
                    inputs.color = value.as_color_quad();
                } else if *pin_label == cpin::WIDTH {
                    inputs.width = value.as_float();
                }
            }
            ConfigNodeType::MinimapConfig(inputs) => {
                if *pin_label == mpin::BACKGROUND {
                    inputs.background = value.as_color_quad();
                } else if *pin_label == mpin::BORDER_COLOR {
                    inputs.border_color = value.as_color_quad();
                } else if *pin_label == mpin::BORDER_WIDTH {
                    inputs.border_width = value.as_float();
                } else if *pin_label == mpin::NODE_COLOR {
                    inputs.node_color = value.as_color_quad();
                } else if *pin_label == mpin::SELECTED_NODE_COLOR {
                    inputs.selected_node_color = value.as_color_quad();
                } else if *pin_label == mpin::VIEWPORT_FILL {
                    inputs.viewport_fill = value.as_color_quad();
                } else if *pin_label == mpin::VIEWPORT_BORDER_COLOR {
                    inputs.viewport_border_color = value.as_color_quad();
                } else if *pin_label == mpin::VIEWPORT_BORDER_WIDTH {
                    inputs.viewport_border_width = value.as_float();
                }
            }
            // The sinks take whole configs, not values (phase 3).
            ConfigNodeType::Catalog { .. } | ConfigNodeType::NodeClass { .. } => {}
        }
    }

    /// Whether a Catalog input pin takes the given output kind.
    fn catalog_input_accepts(pin: PinLabel, output: &ConfigOutput) -> bool {
        use nodes::pins::cfg;
        match output {
            ConfigOutput::Node(_) => pin == cfg::NODE_CONFIG || pin == cfg::NODE_SELECTED,
            ConfigOutput::Pin(_) => pin == cfg::PIN_CONFIG || pin == cfg::PIN_VALID_TARGET,
            ConfigOutput::Edge(_) => {
                pin == cfg::EDGE_CONFIG || pin == cfg::EDGE_PENDING_CUT || pin == cfg::DRAG_EDGE
            }
            ConfigOutput::Anchor(_) => {
                pin == cfg::ANCHOR || pin == cfg::ANCHOR_HOVERED || pin == cfg::ANCHOR_VALID_TARGET
            }
            ConfigOutput::Graph(_) => pin == cfg::GRAPH_CONFIG,
            ConfigOutput::SelectionBox(_) => pin == cfg::SELECTION_BOX,
            ConfigOutput::CuttingTool(_) => pin == cfg::CUTTING_TOOL,
            ConfigOutput::Minimap(_) => pin == cfg::MINIMAP,
        }
    }

    /// Queues a config node's built overlay on a sink's input pin, if the
    /// sink accepts that kind there, and marks the input as connected.
    fn connect_config_to_sink(
        &mut self,
        config_node_id: &NodeId,
        sink_node_id: &NodeId,
        sink_pin: &PinLabel,
    ) {
        use nodes::pins::cfg;

        // Build the config from the CURRENT state of the config node (not the snapshot)
        let built = match self.nodes.get(config_node_id) {
            Some((_, NodeType::Config(config))) => match config {
                ConfigNodeType::NodeConfig(inputs) => Some(ConfigOutput::Node(inputs.build())),
                ConfigNodeType::EdgeConfig(inputs) => Some(ConfigOutput::Edge(inputs.build())),
                ConfigNodeType::PinConfig(inputs) => Some(ConfigOutput::Pin(inputs.build())),
                ConfigNodeType::GraphConfig(inputs) => Some(ConfigOutput::Graph(inputs.build())),
                ConfigNodeType::AnchorConfig(inputs) => Some(ConfigOutput::Anchor(inputs.build())),
                ConfigNodeType::SelectionBoxConfig(inputs) => {
                    Some(ConfigOutput::SelectionBox(inputs.build()))
                }
                ConfigNodeType::CuttingToolConfig(inputs) => {
                    Some(ConfigOutput::CuttingTool(inputs.build()))
                }
                ConfigNodeType::MinimapConfig(inputs) => {
                    Some(ConfigOutput::Minimap(inputs.build()))
                }
                ConfigNodeType::Catalog { .. } | ConfigNodeType::NodeClass { .. } => None,
            },
            _ => None,
        };
        let Some(output) = built else {
            return;
        };

        let accepted = match self.nodes.get_mut(sink_node_id) {
            Some((_, NodeType::Config(ConfigNodeType::Catalog { connected }))) => {
                let ok = Self::catalog_input_accepts(sink_pin, &output);
                if ok {
                    connected.insert(sink_pin);
                }
                ok
            }
            Some((
                _,
                NodeType::Config(ConfigNodeType::NodeClass {
                    has_node_config, ..
                }),
            )) => {
                let ok = *sink_pin == cfg::NODE_CONFIG && matches!(output, ConfigOutput::Node(_));
                if ok {
                    *has_node_config = true;
                }
                ok
            }
            _ => false,
        };

        if accepted {
            self.pending_configs
                .entry(sink_node_id.clone())
                .or_default()
                .push((*sink_pin, output));
        }
    }

    /// Folds the queued sink inputs into the computed catalog. On the Catalog
    /// node each pin names one field; a Node Class node with a target adds a
    /// per-node class. A later config wins over an earlier one on the same pin.
    fn apply_sinks(&mut self, computed: &mut ComputedCatalog) {
        use nodes::pins::cfg;

        for (node_id, (_, node_type)) in &self.nodes {
            let Some(configs) = self.pending_configs.get(node_id) else {
                continue;
            };
            match node_type {
                NodeType::Config(ConfigNodeType::Catalog { .. }) => {
                    for (pin, output) in configs {
                        match output {
                            ConfigOutput::Node(o) => {
                                let slot = if *pin == cfg::NODE_SELECTED {
                                    &mut computed.node_selected
                                } else {
                                    &mut computed.node
                                };
                                *slot = o.merge(slot);
                            }
                            ConfigOutput::Pin(o) => {
                                let slot = if *pin == cfg::PIN_VALID_TARGET {
                                    &mut computed.pin_valid_target
                                } else {
                                    &mut computed.pin
                                };
                                *slot = o.merge(slot);
                            }
                            ConfigOutput::Edge(o) => {
                                let slot = if *pin == cfg::EDGE_PENDING_CUT {
                                    &mut computed.edge_pending_cut
                                } else if *pin == cfg::DRAG_EDGE {
                                    &mut computed.drag_edge
                                } else {
                                    &mut computed.edge
                                };
                                *slot = o.merge(slot);
                            }
                            ConfigOutput::Anchor(o) => {
                                let slot = if *pin == cfg::ANCHOR_HOVERED {
                                    &mut computed.anchor_hovered
                                } else if *pin == cfg::ANCHOR_VALID_TARGET {
                                    &mut computed.anchor_valid_target
                                } else {
                                    &mut computed.anchor
                                };
                                *slot = o.merge(slot);
                            }
                            ConfigOutput::Graph(o) => computed.graph = o.merge(&computed.graph),
                            ConfigOutput::SelectionBox(o) => {
                                computed.selection_box = o.merge(&computed.selection_box)
                            }
                            ConfigOutput::CuttingTool(o) => {
                                computed.cutting_tool = o.merge(&computed.cutting_tool)
                            }
                            ConfigOutput::Minimap(o) => {
                                computed.minimap = o.merge(&computed.minimap)
                            }
                        }
                    }
                }
                NodeType::Config(ConfigNodeType::NodeClass {
                    target: Some(target),
                    has_node_config: true,
                }) => {
                    for (_, output) in configs {
                        if let ConfigOutput::Node(o) = output {
                            computed
                                .node_classes
                                .entry(target.clone())
                                .and_modify(|c| *c = o.merge(c))
                                .or_insert_with(|| o.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        // Clear pending configs after application
        self.pending_configs.clear();
    }
}

impl demo_common::Demo for Application {
    type Message = ApplicationMessage;

    fn boot() -> (Self, Task<ApplicationMessage>) {
        (Self::default(), Task::none())
    }

    fn update(&mut self, message: ApplicationMessage) -> Task<ApplicationMessage> {
        match message {
            ApplicationMessage::EdgeConnected { from, to } => {
                let edge_id = generate_edge_id();
                self.edges.insert(
                    edge_id.clone(),
                    EdgeData::new(from.node_id, from.pin_id, to.node_id, to.pin_id),
                );
                self.edge_order.push(edge_id);
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::EdgesCut(ids) => {
                // The widget names the edges it cut, so no endpoint search is needed.
                for id in &ids {
                    self.edges.remove(id);
                }
                self.edge_order.retain(|id| !ids.contains(id));
                self.drop_unused_anchors();
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::AnchorCreated { edge, position } => {
                let id = self.next_anchor;
                self.next_anchor += 1;
                // An anchor nothing wraps is not something this host keeps, so
                // it is only pushed once an edge has taken it. The id stays
                // spent either way and can never name a second anchor.
                if let Some(data) = self.edges.get_mut(&edge) {
                    data.route.push(id);
                    self.anchors.push((id, position));
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::AnchorMoved { anchor, position } => {
                if let Some((_, p)) = self.anchors.iter_mut().find(|(id, _)| *id == anchor) {
                    *p = position;
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::AnchorDeleted(anchor) => {
                self.anchors.retain(|(id, _)| *id != anchor);
                for data in self.edges.values_mut() {
                    data.route.retain(|id| *id != anchor);
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::RouteAttached { edge, anchor } => {
                if let Some(data) = self.edges.get_mut(&edge)
                    && !data.route.contains(&anchor)
                {
                    data.route.push(anchor);
                }
                self.save_state();
                Task::none()
            }
            ApplicationMessage::RouteDetached { edge, anchor } => {
                if let Some(data) = self.edges.get_mut(&edge) {
                    data.route.retain(|id| *id != anchor);
                }
                // No collection here: a detach fires DURING the drag, and the
                // anchor just left is exactly the one the drag may put the
                // cable straight back onto. Dropping it now would delete a live
                // drop target mid-gesture.
                self.save_state();
                Task::none()
            }
            ApplicationMessage::DragEnded => {
                self.drop_unused_anchors();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::EdgeDisconnected { from, to } => {
                // Unsnap during a drag: no host edge exists to name, so match the
                // endpoint pair. Cuts arrive through `EdgesCut` instead.
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
                self.drop_unused_anchors();
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
                    self.propagate_values();
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
                    && submenu == "themes"
                {
                    let (_, commands) = self.build_palette_commands();
                    if let Some(original_idx) = get_filtered_command_index(
                        &self.command_input,
                        &commands,
                        self.palette_selected_index,
                    ) {
                        let themes = Self::get_available_themes();
                        if original_idx < themes.len() {
                            self.palette_preview_theme = Some(themes[original_idx].clone());
                            // The rig's palette pins follow the previewed theme.
                            self.propagate_values();
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
                                self.save_state();
                                Task::none()
                            }
                            ApplicationMessage::ChangeTheme(theme) => {
                                self.current_theme = theme;
                                self.palette_preview_theme = None;
                                self.palette_original_theme = None;
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                // Theme nodes feed palette colors into configs;
                                // recompute so styling follows the new theme.
                                self.propagate_values();
                                self.save_state();
                                Task::none()
                            }
                            ApplicationMessage::ExportState => {
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                self.export_state_to_file();
                                Task::none()
                            }
                            ApplicationMessage::Reset => {
                                self.reset_to_default();
                                Task::none()
                            }
                            ApplicationMessage::FocusNode => {
                                self.command_palette_open = false;
                                self.palette_view = PaletteView::Main;
                                self.focus_first_node()
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
                self.propagate_values();
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
                self.save_state();
                Task::none()
            }
            ApplicationMessage::FocusNode => self.focus_first_node(),
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
                // Theme nodes feed palette colors into configs; recompute so
                // styling follows the new theme.
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::NavigateToSubmenu(submenu) => {
                self.palette_view = PaletteView::Submenu(submenu);
                self.command_input.clear();
                focus_input()
            }
            ApplicationMessage::ExportState => {
                self.export_state_to_file();
                Task::none()
            }
            ApplicationMessage::Reset => {
                self.reset_to_default();
                Task::none()
            }
            ApplicationMessage::FocusNext => iced::widget::operation::focus_next(),
            ApplicationMessage::FocusPrevious => iced::widget::operation::focus_previous(),
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
                            EdgeData::new(
                                new_from.clone(),
                                edge.from_pin,
                                new_to.clone(),
                                edge.to_pin,
                            ),
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
                self.delete_nodes(&node_ids);
                self.propagate_values();
                self.save_state();
                Task::none()
            }
            ApplicationMessage::NodesMoved { delta, node_ids } => {
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
            ApplicationMessage::TilingKindChanged { node_id, value } => {
                if let Some((_, NodeType::Input(InputNodeType::TilingKindSelector { value: v }))) =
                    self.nodes.get_mut(&node_id)
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
                let sections = self
                    .edge_config_sections
                    .entry(node_id)
                    .or_insert_with(EdgeSections::new_all_expanded);
                match section {
                    EdgeSection::Stroke => sections.stroke = !sections.stroke,
                    EdgeSection::Pattern => sections.pattern = !sections.pattern,
                    EdgeSection::Border => sections.border = !sections.border,
                    EdgeSection::Shadow => sections.shadow = !sections.shadow,
                }
                Task::none()
            }
            ApplicationMessage::ToggleNodeSection { node_id, section } => {
                let sections = self
                    .node_config_sections
                    .entry(node_id)
                    .or_insert_with(NodeSections::new_all_expanded);
                match section {
                    NodeSection::Fill => sections.fill = !sections.fill,
                    NodeSection::Border => sections.border = !sections.border,
                    NodeSection::Pattern => sections.pattern = !sections.pattern,
                    NodeSection::Shadow => sections.shadow = !sections.shadow,
                }
                Task::none()
            }
            ApplicationMessage::NodeClassTargetChanged { node_id, target } => {
                if let Some((_, NodeType::Config(ConfigNodeType::NodeClass { target: t, .. }))) =
                    self.nodes.get_mut(&node_id)
                {
                    *t = target;
                    self.propagate_values();
                    self.save_state();
                }
                Task::none()
            }
        }
    }

    fn theme(&self) -> Theme {
        self.effective_theme().clone()
    }

    fn set_theme(&mut self, theme: Theme) {
        self.current_theme = theme;
        self.palette_preview_theme = None;
        // The rig's palette pins follow the theme.
        self.propagate_values();
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
        use iced_nodegraph::NodeGraph;

        let theme = self.effective_theme();

        // The Node Class pick list offers every workflow node; each Node Class
        // node takes its own copy.
        let candidates: Vec<ClassCandidate> = self
            .node_order
            .iter()
            .filter_map(|id| match self.nodes.get(id) {
                Some((_, NodeType::Workflow(name))) => Some(ClassCandidate {
                    id: id.clone(),
                    label: format!("{} {}", name, &id[..6.min(id.len())]),
                }),
                _ => None,
            })
            .collect();

        let mut ng: NodeGraph<'_, HelloIds, ApplicationMessage, iced::Theme, iced::Renderer> =
            NodeGraph::new()
                .id(graph_id())
                .on_connect(|from: PinRef<HelloIds>, to: PinRef<HelloIds>| {
                    ApplicationMessage::EdgeConnected { from, to }
                })
                .on_disconnect(|from: PinRef<HelloIds>, to: PinRef<HelloIds>| {
                    ApplicationMessage::EdgeDisconnected { from, to }
                })
                .on_edge_delete(ApplicationMessage::EdgesCut)
                .on_move(|delta, node_ids| ApplicationMessage::NodesMoved { delta, node_ids })
                .on_select(ApplicationMessage::SelectionChanged)
                .on_clone(ApplicationMessage::CloneNodes)
                .on_delete(ApplicationMessage::DeleteNodes)
                .on_camera(|position, zoom| ApplicationMessage::CameraChanged { position, zoom })
                .on_anchor_move(|anchor, position| ApplicationMessage::AnchorMoved {
                    anchor,
                    position,
                })
                .on_anchor_create(|edge, position| ApplicationMessage::AnchorCreated {
                    edge,
                    position,
                })
                .on_anchor_delete(ApplicationMessage::AnchorDeleted)
                .on_route_attach(|edge, anchor| ApplicationMessage::RouteAttached { edge, anchor })
                .on_route_detach(|edge, anchor| ApplicationMessage::RouteDetached { edge, anchor })
                .on_drag_end(|| ApplicationMessage::DragEnded)
                .camera(self.camera_position, self.camera_zoom)
                .minimap(Minimap::default())
                // A connection is valid only between opposite directions (output ->
                // input) carrying the same data type (the pin's TypeId marker). Color
                // and ColorQuad share the `ColorData` marker, so a color pin accepts
                // both a picker and the ColorQuad builder; Vec2 only matches Vec2.
                .can_connect(|from, to| {
                    from.direction() != to.direction() && from.info() == to.info()
                })
                // Every class the widget resolves is a method on `self`: the
                // computed catalog over the theme-derived base, see `node_style`
                // and its siblings.
                .selection_box_style(|theme| self.selection_box_style(theme))
                .cutting_tool_style(|theme| self.cutting_tool_style(theme))
                .minimap_style(|theme| self.minimap_style(theme))
                .dragging_edge_style(|theme, source| self.drag_edge_style(theme, *source.info()))
                .graph_style(|theme| self.graph_style(theme))
                .anchors(self.anchors.iter().map(|&(id, position)| {
                    ng_anchor(id, position).style(|theme, status| self.anchor_style(theme, status))
                }));

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
                    InputNodeType::TilingKindSelector { value } => {
                        let id = node_id_clone.clone();
                        tiling_kind_selector_node(theme, *value, move |v| {
                            ApplicationMessage::TilingKindChanged {
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
                        let sections = self
                            .node_config_sections
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(NodeSections::new_all_expanded);
                        node_config_node(theme, inputs, &sections, move |section| {
                            ApplicationMessage::ToggleNodeSection {
                                node_id: id.clone(),
                                section,
                            }
                        })
                    }
                    ConfigNodeType::EdgeConfig(inputs) => {
                        let id = node_id_clone.clone();
                        let sections = self
                            .edge_config_sections
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(EdgeSections::new_all_expanded);
                        edge_config_node(theme, inputs, &sections, move |section| {
                            ApplicationMessage::ToggleEdgeSection {
                                node_id: id.clone(),
                                section,
                            }
                        })
                    }
                    ConfigNodeType::PinConfig(inputs) => pin_config_node(theme, inputs),
                    ConfigNodeType::GraphConfig(inputs) => graph_config_node(theme, inputs),
                    ConfigNodeType::AnchorConfig(inputs) => anchor_config_node(theme, inputs),
                    ConfigNodeType::SelectionBoxConfig(inputs) => {
                        selection_box_config_node(theme, inputs)
                    }
                    ConfigNodeType::CuttingToolConfig(inputs) => {
                        cutting_tool_config_node(theme, inputs)
                    }
                    ConfigNodeType::MinimapConfig(inputs) => minimap_config_node(theme, inputs),
                    ConfigNodeType::Catalog { connected } => catalog_node(theme, connected),
                    ConfigNodeType::NodeClass {
                        target,
                        has_node_config,
                    } => {
                        let id = node_id_clone.clone();
                        node_class_node(
                            theme,
                            *has_node_config,
                            target.as_ref(),
                            candidates.clone(),
                            move |target| ApplicationMessage::NodeClassTargetChanged {
                                node_id: id.clone(),
                                target,
                            },
                        )
                    }
                },
                NodeType::Math(state) => math_node(theme, state),
                NodeType::ColorQuad(state) => color_quad_node(theme, state),
                NodeType::Vec2(state) => vec2_node(theme, state),
                NodeType::Alpha(state) => alpha_node(theme, state),
                NodeType::Theme => theme_node(theme),
                NodeType::ThemeExtended => theme_extended_node(theme),
                NodeType::Frame { label, size } => frame_node(theme, label, *size),
            };

            let mut node = ng_node(node_id.clone(), *position, element)
                .style(|theme, status| self.node_style(node_id, theme, status))
                .pin_style(|theme, pin, _other, status| self.pin_style(*pin.info(), theme, status));
            if matches!(node_type, NodeType::Frame { .. }) {
                node = node.frame();
            }
            ng = ng.push_node(node);
        }

        for edge_id in &self.edge_order {
            if let Some(edge_data) = self.edges.get(edge_id) {
                let from = PinRef::new(edge_data.from_node.clone(), edge_data.from_pin);
                let to = PinRef::new(edge_data.to_node.clone(), edge_data.to_pin);
                ng = ng.push_edge(
                    ng_edge(edge_id.clone(), from, to)
                        .route(edge_data.route.iter().copied())
                        .style(|theme, status, start, end| {
                            self.edge_style(theme, status, *start.info(), *end.info())
                        }),
                );
            }
        }

        let graph_view: iced::Element<'_, ApplicationMessage> = ng.into();

        // Always use the same widget structure to preserve NodeGraph state
        // The command palette is conditionally shown as an overlay
        let overlay: iced::Element<'_, ApplicationMessage> = if self.command_palette_open {
            let (_, commands) = self.build_palette_commands();
            // `opaque` blocks wheel events from reaching the NodeGraph behind
            // the palette; `command_palette`'s own `mouse_area` only captures
            // `on_press`, not scroll.
            opaque(command_palette(
                &self.command_input,
                &commands,
                self.palette_selected_index,
                ApplicationMessage::CommandPaletteInput,
                ApplicationMessage::CommandPaletteSelect,
                ApplicationMessage::CommandPaletteNavigate,
                || ApplicationMessage::CommandPaletteCancel,
            ))
        } else {
            // Invisible placeholder to maintain widget tree structure
            container(text("")).width(0).height(0).into()
        };

        stack!(graph_view, overlay)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        Subscription::batch(vec![
            event::listen_with(handle_keyboard_event),
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

/// Boots this demo for the gallery.
pub fn scene() -> (
    Box<dyn demo_common::Scene>,
    iced::Task<demo_common::SceneMessage>,
) {
    demo_common::erase::<Application>()
}

impl Application {
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

    /// Frames the first workflow node: the "Focus node" command's target, run
    /// with the default eased tween. `Home` and `f` cover All and Selection;
    /// this exercises [`FocusTarget::Node`].
    fn focus_first_node(&self) -> Task<ApplicationMessage> {
        match self.node_order.first() {
            Some(target) => iced_nodegraph::focus(
                graph_id(),
                FocusTarget::<HelloIds>::Node(target.clone()),
                FocusOptions::default(),
            ),
            None => Task::none(),
        }
    }

    // ---------------------------------------------------------------------
    // Style resolution: the computed catalog over the theme-derived base.
    // With nothing connected an overlay is empty and the base is exactly the
    // widget's own default.
    // ---------------------------------------------------------------------

    /// A node's style: its per-node class (a Node Class node targeting it)
    /// wins over the Catalog's status class, which is the selected overlay
    /// layered over the idle one; the theme base fills the rest.
    fn node_style(&self, node_id: &NodeId, theme: &Theme, status: NodeStatus) -> NodeStyle {
        let global = self.computed.node(status);
        let overlay = match self.computed.node_classes.get(node_id) {
            Some(class) => class.merge(&global),
            None => global,
        };
        overlay.resolve_over(default_node_style(theme, status))
    }

    /// A pin's style: the Catalog's pin class wins over the pin's data-type
    /// color, then the status default fills the rest.
    fn pin_style(&self, info: std::any::TypeId, theme: &Theme, status: PinStatus) -> PinStyle {
        self.computed
            .pin(status)
            .merge(&PinOverlay::new().color(pin_color_for(info)))
            .resolve_over(default_pin_style(theme, status))
    }

    /// An edge's style: the Catalog's edge class over an arc between its two
    /// pins' data-type colors.
    fn edge_style(
        &self,
        theme: &Theme,
        status: EdgeStatus,
        start: std::any::TypeId,
        end: std::any::TypeId,
    ) -> EdgeStyle {
        let base = EdgeStyle {
            stroke_color: ColorQuad::arc(pin_color_for(start), pin_color_for(end)),
            ..default_edge_style(theme, status)
        };
        self.computed.edge(status).resolve_over(base)
    }

    /// The loose edge's style: the Catalog's drag-edge class over the held
    /// pin's data-type color on both ends.
    fn drag_edge_style(&self, theme: &Theme, source: std::any::TypeId) -> EdgeStyle {
        let base = EdgeStyle {
            stroke_color: ColorQuad::solid(pin_color_for(source)),
            ..default_edge_style(theme, EdgeStatus::Idle)
        };
        self.computed.drag_edge.resolve_over(base)
    }

    fn anchor_style(&self, theme: &Theme, status: AnchorStatus) -> AnchorStyle {
        self.computed
            .anchor(status)
            .resolve_over(default_anchor_style(theme, status))
    }

    fn graph_style(&self, theme: &Theme) -> GraphStyle {
        self.computed.graph.resolve_over(default_graph_style(theme))
    }

    fn selection_box_style(&self, theme: &Theme) -> SelectionBoxStyle {
        self.computed
            .selection_box
            .resolve_over(default_selection_box_style(theme))
    }

    fn cutting_tool_style(&self, theme: &Theme) -> CuttingToolStyle {
        self.computed
            .cutting_tool
            .resolve_over(default_cutting_tool_style(theme))
    }

    fn minimap_style(&self, theme: &Theme) -> MinimapStyle {
        self.computed
            .minimap
            .resolve_over(default_minimap_style(theme))
    }

    /// Drops every anchor no route names any more.
    ///
    /// The library keeps an anchor as long as the host pushes it, cables or no
    /// cables - so "the last cable left, the anchor goes" is a policy, and this
    /// is where this host states it.
    fn drop_unused_anchors(&mut self) {
        let routed: HashSet<usize> = self
            .edges
            .values()
            .flat_map(|e| e.route.iter().copied())
            .collect();
        self.anchors.retain(|(id, _)| routed.contains(id));
    }

    /// Removes the nodes, the edges touching them, the anchors that leaves
    /// unrouted, and the selection; a Node Class whose target is among them
    /// points at nothing afterwards.
    fn delete_nodes(&mut self, node_ids: &[NodeId]) {
        for node_id in node_ids {
            self.nodes.remove(node_id);
            self.node_order.retain(|id| id != node_id);

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

        for (_, node_type) in self.nodes.values_mut() {
            if let NodeType::Config(ConfigNodeType::NodeClass {
                target: Some(target),
                ..
            }) = node_type
                && node_ids.contains(target)
            {
                *node_type = NodeType::Config(ConfigNodeType::NodeClass {
                    target: None,
                    has_node_config: false,
                });
            }
        }

        self.drop_unused_anchors();
        self.selected_nodes.clear();
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
                    command("reset", "Reset")
                        .description("Reset the app to its initial state")
                        .action(ApplicationMessage::Reset),
                    command("focus_node", "Focus node")
                        .description("Fly the camera to a fixed demo node (eased tween)")
                        .action(ApplicationMessage::FocusNode),
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
                        .description("Select pin shape (Circle, Square)")
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
                    command("tiling_kind", "Tiling Kind Selector")
                        .description("Select canvas tiling (Grid, Dots, Triangles, Hex)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Input(InputNodeType::TilingKindSelector {
                                value: TilingKind::Grid,
                            }),
                        }),
                    command("theme", "Theme")
                        .description("Active theme's basic palette as color outputs")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Theme,
                        }),
                    command("theme_extended", "Theme Extended")
                        .description("Extended palette (base/weak/strong) as color outputs")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::ThemeExtended,
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
                    command("pin_config", "Pin Config")
                        .description("Pin configuration with shape, color, radius")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::PinConfig(
                                PinConfigInputs::default(),
                            )),
                        }),
                    command("graph_config", "Graph Config")
                        .description("Canvas background and tiling (grid/dots/...)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::GraphConfig(
                                GraphConfigInputs::default(),
                            )),
                        }),
                    command("anchor_config", "Anchor Config")
                        .description("Anchor core, border and orbit ring")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::AnchorConfig(
                                AnchorConfigInputs::default(),
                            )),
                        }),
                    command("selection_box_config", "Selection Box Config")
                        .description("Selection box fill and border")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::SelectionBoxConfig(
                                SelectionBoxConfigInputs::default(),
                            )),
                        }),
                    command("cutting_tool_config", "Cutting Tool Config")
                        .description("Edge-cutting trail color and width")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::CuttingToolConfig(
                                CuttingToolConfigInputs::default(),
                            )),
                        }),
                    command("minimap_config", "Minimap Config")
                        .description("Minimap background, marks and viewport")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::MinimapConfig(
                                MinimapConfigInputs::default(),
                            )),
                        }),
                    // Builder nodes
                    command("color_quad", "Color Quad")
                        .description("Combine 4 corner colors into one ColorQuad")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::ColorQuad(ColorQuadNode::default()),
                        }),
                    command("vec2", "Vec2")
                        .description("Combine x and y into a 2D vector (e.g. offset)")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Vec2(Vec2Node::default()),
                        }),
                    command("alpha", "Alpha")
                        .description("Replace a color's alpha")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Alpha(AlphaNode::default()),
                        }),
                    // Sinks
                    command("catalog", "Catalog")
                        .description("One input per style class and status")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::Catalog {
                                connected: HashSet::new(),
                            }),
                        }),
                    command("node_class", "Node Class")
                        .description("Assign a node config to one node")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Config(ConfigNodeType::NodeClass {
                                target: None,
                                has_node_config: false,
                            }),
                        }),
                    // Layout
                    command("frame", "Frame")
                        .description("A titled region that moves the nodes laid over it")
                        .action(ApplicationMessage::SpawnNode {
                            node_type: NodeType::Frame {
                                label: "Frame".to_string(),
                                size: Size::new(400.0, 300.0),
                            },
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
}

/// Resolves a Theme node output pin to a color from the theme's extended palette.
/// Returns None for unknown pins.
/// Resolves a Theme / Theme Extended node output pin to its color. Basic
/// [`theme`](nodes::pins::theme) labels map to the flat [`Theme::palette`];
/// [`theme_ext`](nodes::pins::theme_ext) labels map to the graded
/// [`Theme::extended_palette`]. The two label sets are disjoint, so one lookup
/// serves both node kinds.
fn theme_color(theme: &Theme, pin: &PinLabel) -> Option<iced::Color> {
    use nodes::pins::{theme as t, theme_ext as x};

    // Basic palette (flat entries).
    let pal = theme.palette();
    if *pin == t::BACKGROUND {
        return Some(pal.background);
    } else if *pin == t::TEXT {
        return Some(pal.text);
    } else if *pin == t::PRIMARY {
        return Some(pal.primary);
    } else if *pin == t::SUCCESS {
        return Some(pal.success);
    } else if *pin == t::WARNING {
        return Some(pal.warning);
    } else if *pin == t::DANGER {
        return Some(pal.danger);
    }

    // Extended palette (base/weak/strong per accent group).
    let p = theme.extended_palette();
    let color = if *pin == x::BACKGROUND_BASE {
        p.background.base.color
    } else if *pin == x::BACKGROUND_WEAK {
        p.background.weak.color
    } else if *pin == x::BACKGROUND_STRONG {
        p.background.strong.color
    } else if *pin == x::PRIMARY_BASE {
        p.primary.base.color
    } else if *pin == x::PRIMARY_WEAK {
        p.primary.weak.color
    } else if *pin == x::PRIMARY_STRONG {
        p.primary.strong.color
    } else if *pin == x::SECONDARY_BASE {
        p.secondary.base.color
    } else if *pin == x::SECONDARY_WEAK {
        p.secondary.weak.color
    } else if *pin == x::SECONDARY_STRONG {
        p.secondary.strong.color
    } else if *pin == x::SUCCESS_BASE {
        p.success.base.color
    } else if *pin == x::SUCCESS_WEAK {
        p.success.weak.color
    } else if *pin == x::SUCCESS_STRONG {
        p.success.strong.color
    } else if *pin == x::WARNING_BASE {
        p.warning.base.color
    } else if *pin == x::WARNING_WEAK {
        p.warning.weak.color
    } else if *pin == x::WARNING_STRONG {
        p.warning.strong.color
    } else if *pin == x::DANGER_BASE {
        p.danger.base.color
    } else if *pin == x::DANGER_WEAK {
        p.danger.weak.color
    } else if *pin == x::DANGER_STRONG {
        p.danger.strong.color
    } else {
        return None;
    };
    Some(color)
}

/// Feeds a propagated `value` into a combiner node's input pin (Math, ColorQuad,
/// Vec2 or Alpha). Returns true if the stored input changed, so the propagation
/// loop knows to keep iterating. No-op for any other node type or unknown pin.
fn feed_combiner_input(node: &mut NodeType, pin: &PinLabel, value: &NodeValue) -> bool {
    use nodes::pins::{build as pin_build, math as pin_math};

    let mut changed = false;
    match node {
        NodeType::Math(state) => {
            if let Some(f) = value.as_float() {
                if *pin == pin_math::A && state.input_a != Some(f) {
                    state.input_a = Some(f);
                    changed = true;
                } else if *pin == pin_math::B && state.input_b != Some(f) {
                    state.input_b = Some(f);
                    changed = true;
                }
            }
        }
        NodeType::ColorQuad(state) => {
            if let Some(c) = value.as_color() {
                let slot = if *pin == pin_build::NEAR_START {
                    Some(&mut state.near_start)
                } else if *pin == pin_build::NEAR_END {
                    Some(&mut state.near_end)
                } else if *pin == pin_build::FAR_START {
                    Some(&mut state.far_start)
                } else if *pin == pin_build::FAR_END {
                    Some(&mut state.far_end)
                } else {
                    None
                };
                if let Some(slot) = slot
                    && *slot != Some(c)
                {
                    *slot = Some(c);
                    changed = true;
                }
            }
        }
        NodeType::Vec2(state) => {
            if let Some(f) = value.as_float() {
                if *pin == pin_build::X && state.x != Some(f) {
                    state.x = Some(f);
                    changed = true;
                } else if *pin == pin_build::Y && state.y != Some(f) {
                    state.y = Some(f);
                    changed = true;
                }
            }
        }
        NodeType::Alpha(state) => {
            if *pin == pin_build::ALPHA_COLOR {
                let c = value.as_color_quad().map(|q| q.near_start);
                if c.is_some() && state.color != c {
                    state.color = c;
                    changed = true;
                }
            } else if *pin == pin_build::ALPHA {
                let a = value.as_float();
                if a.is_some() && state.alpha != a {
                    state.alpha = a;
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
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
                keyboard::Key::Named(keyboard::key::Named::Tab) => {
                    if modifiers.shift() {
                        Some(ApplicationMessage::FocusPrevious)
                    } else {
                        Some(ApplicationMessage::FocusNext)
                    }
                }
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

    // === Propagation chain tests ===

    /// An application with an empty graph, ready for nodes and edges.
    fn empty_app() -> Application {
        let mut app = Application::default();
        app.nodes.clear();
        app.node_order.clear();
        app.edges.clear();
        app.edge_order.clear();
        app.anchors.clear();
        app
    }

    fn add(app: &mut Application, node_type: NodeType) -> NodeId {
        let id = generate_node_id();
        app.nodes.insert(id.clone(), (Point::ORIGIN, node_type));
        app.node_order.push(id.clone());
        id
    }

    fn wire(app: &mut Application, from: &NodeId, fp: PinLabel, to: &NodeId, tp: PinLabel) {
        let e = generate_edge_id();
        app.edges
            .insert(e.clone(), EdgeData::new(from.clone(), fp, to.clone(), tp));
        app.edge_order.push(e);
    }

    fn catalog(app: &mut Application) -> NodeId {
        add(
            app,
            NodeType::Config(ConfigNodeType::Catalog {
                connected: HashSet::new(),
            }),
        )
    }

    fn node_config(app: &mut Application) -> NodeId {
        add(
            app,
            NodeType::Config(ConfigNodeType::NodeConfig(NodeConfigInputs::default())),
        )
    }

    fn slider(app: &mut Application, value: f32) -> NodeId {
        add(
            app,
            NodeType::Input(InputNodeType::FloatSlider {
                config: FloatSliderConfig::default(),
                value,
            }),
        )
    }

    fn picker(app: &mut Application, color: Color) -> NodeId {
        add(app, NodeType::Input(InputNodeType::ColorPicker { color }))
    }

    /// The Catalog inputs the given node reports as connected.
    fn connected(app: &Application, catalog: &NodeId) -> HashSet<PinLabel> {
        match app.nodes.get(catalog) {
            Some((_, NodeType::Config(ConfigNodeType::Catalog { connected }))) => connected.clone(),
            _ => panic!("catalog node missing"),
        }
    }

    #[test]
    fn test_node_config_chain_applies_to_catalog() {
        // ColorPicker -> NodeConfig.fill_color, NodeConfig -> Catalog.node.
        use nodes::pins;
        let mut app = empty_app();
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let picker = picker(&mut app, red);
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &picker,
            pins::input::COLOR,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );

        app.propagate_values();

        assert!(connected(&app, &catalog).contains(pins::cfg::NODE_CONFIG));
        assert_eq!(
            app.computed.node.fill_color.map(|q| q.near_start),
            Some(red),
            "computed node class did not receive the config fill color",
        );
    }

    #[test]
    fn test_catalog_rejects_mismatched_kind() {
        // A node config wired into the `graph` input is not a graph config:
        // nothing is connected and nothing is computed.
        use nodes::pins;
        let mut app = empty_app();
        let picker = picker(&mut app, Color::WHITE);
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &picker,
            pins::input::COLOR,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::GRAPH_CONFIG,
        );

        app.propagate_values();

        assert!(connected(&app, &catalog).is_empty());
        assert!(app.computed.node.fill_color.is_none());
        assert!(app.computed.graph.background_color.is_none());
    }

    #[test]
    fn test_graph_config_chain_applies_to_catalog() {
        // ColorPicker -> GraphConfig.background and TilingKind ->
        // GraphConfig.tiling_kind, then GraphConfig -> Catalog.graph. The
        // computed overlay carries both and resolve_over installs them.
        use nodes::pins;
        let mut app = empty_app();
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let picker = picker(&mut app, blue);
        let kind = add(
            &mut app,
            NodeType::Input(InputNodeType::TilingKindSelector {
                value: TilingKind::Dots,
            }),
        );
        let cfg = add(
            &mut app,
            NodeType::Config(ConfigNodeType::GraphConfig(GraphConfigInputs::default())),
        );
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &picker,
            pins::input::COLOR,
            &cfg,
            pins::graph::BACKGROUND,
        );
        wire(
            &mut app,
            &kind,
            pins::input::VALUE,
            &cfg,
            pins::graph::TILING_KIND,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::GRAPH_OUT,
            &catalog,
            pins::cfg::GRAPH_CONFIG,
        );

        app.propagate_values();

        assert!(connected(&app, &catalog).contains(pins::cfg::GRAPH_CONFIG));
        assert_eq!(app.computed.graph.background_color, Some(blue));
        assert_eq!(app.computed.graph.tiling_kind, Some(TilingKind::Dots));
        let resolved = app
            .computed
            .graph
            .resolve_over(default_graph_style(&app.current_theme));
        assert_eq!(resolved.background_color, blue);
        assert_eq!(resolved.tiling.map(|t| t.kind), Some(TilingKind::Dots));
    }

    #[test]
    fn test_node_config_shadow_chain_with_vec2() {
        // ColorPicker -> shadow_color, slider -> shadow_distance, two sliders
        // -> Vec2 -> shadow_offset, NodeConfig -> Catalog.node.
        use nodes::pins;
        let mut app = empty_app();
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let picker = picker(&mut app, red);
        let dist = slider(&mut app, 8.0);
        let sx = slider(&mut app, 5.0);
        let sy = slider(&mut app, 7.0);
        let vec2 = add(&mut app, NodeType::Vec2(Vec2Node::default()));
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &picker,
            pins::input::COLOR,
            &cfg,
            pins::node::SHADOW_COLOR,
        );
        wire(
            &mut app,
            &dist,
            pins::input::VALUE,
            &cfg,
            pins::node::SHADOW_DISTANCE,
        );
        wire(&mut app, &sx, pins::input::VALUE, &vec2, pins::build::X);
        wire(&mut app, &sy, pins::input::VALUE, &vec2, pins::build::Y);
        wire(
            &mut app,
            &vec2,
            pins::build::VEC2_OUT,
            &cfg,
            pins::node::SHADOW_OFFSET,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );

        app.propagate_values();

        let node = &app.computed.node;
        assert_eq!(node.shadow_color, Some(red));
        assert_eq!(node.shadow_distance, Some(8.0));
        assert_eq!(node.shadow_offset, Some((5.0, 7.0)));
    }

    #[test]
    fn test_theme_node_feeds_config() {
        // Theme.primary -> NodeConfig.fill_color -> Catalog.node carries the
        // active theme's primary color.
        use nodes::pins;
        let mut app = empty_app();
        let theme = add(&mut app, NodeType::Theme);
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &theme,
            pins::theme::PRIMARY,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );

        let expected = app.current_theme.palette().primary;
        app.propagate_values();
        assert_eq!(
            app.computed.node.fill_color.map(|q| q.near_start),
            Some(expected),
        );
    }

    #[test]
    fn test_theme_extended_node_feeds_config() {
        // ThemeExtended.primary_strong -> NodeConfig.fill_color -> Catalog.node
        // carries the extended palette's strong primary.
        use nodes::pins;
        let mut app = empty_app();
        let theme = add(&mut app, NodeType::ThemeExtended);
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &theme,
            pins::theme_ext::PRIMARY_STRONG,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );

        let expected = app.current_theme.extended_palette().primary.strong.color;
        app.propagate_values();
        assert_eq!(
            app.computed.node.fill_color.map(|q| q.near_start),
            Some(expected),
        );
    }

    #[test]
    fn test_palette_preview_recolors_rig() {
        // With a preview theme open, the palette pins read the previewed
        // theme, not the applied one.
        use nodes::pins;
        let mut app = empty_app();
        let theme = add(&mut app, NodeType::Theme);
        let cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &theme,
            pins::theme::PRIMARY,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );

        app.current_theme = Theme::Dark;
        app.palette_preview_theme = Some(Theme::Light);
        app.propagate_values();
        assert_eq!(
            app.computed.node.fill_color.map(|q| q.near_start),
            Some(Theme::Light.palette().primary),
        );
    }

    #[test]
    fn catalog_status_input_layers_over_idle() {
        // Idle fill red on `node`, selected border blue on `node:selected`:
        // the selected class carries both, the idle class only the fill.
        use nodes::pins;
        let mut app = empty_app();
        let red = Color::from_rgb(1.0, 0.0, 0.0);
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let red_picker = picker(&mut app, red);
        let blue_picker = picker(&mut app, blue);
        let idle_cfg = node_config(&mut app);
        let selected_cfg = node_config(&mut app);
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &red_picker,
            pins::input::COLOR,
            &idle_cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &blue_picker,
            pins::input::COLOR,
            &selected_cfg,
            pins::node::BORDER_COLOR,
        );
        wire(
            &mut app,
            &idle_cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_CONFIG,
        );
        wire(
            &mut app,
            &selected_cfg,
            pins::cfg::NODE_OUT,
            &catalog,
            pins::cfg::NODE_SELECTED,
        );

        app.propagate_values();

        let selected = app.computed.node(NodeStatus::Selected);
        assert_eq!(selected.fill_color.map(|q| q.near_start), Some(red));
        assert_eq!(selected.border_color.map(|q| q.near_start), Some(blue));
        let idle = app.computed.node(NodeStatus::Idle);
        assert_eq!(idle.fill_color.map(|q| q.near_start), Some(red));
        assert!(idle.border_color.is_none());
    }

    #[test]
    fn node_class_targets_one_node() {
        // A Node Class with a target adds a per-node class and nothing to the
        // global one; without a target it contributes nothing.
        use nodes::pins;
        let mut app = empty_app();
        let green = Color::from_rgb(0.0, 1.0, 0.0);
        let workflow = add(&mut app, NodeType::Workflow("filter".to_string()));
        let picker = picker(&mut app, green);
        let cfg = node_config(&mut app);
        let class = add(
            &mut app,
            NodeType::Config(ConfigNodeType::NodeClass {
                target: Some(workflow.clone()),
                has_node_config: false,
            }),
        );
        wire(
            &mut app,
            &picker,
            pins::input::COLOR,
            &cfg,
            pins::node::FILL_COLOR,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::NODE_OUT,
            &class,
            pins::cfg::NODE_CONFIG,
        );

        app.propagate_values();

        assert_eq!(
            app.computed.node_classes[&workflow]
                .fill_color
                .map(|q| q.near_start),
            Some(green),
        );
        assert!(app.computed.node.fill_color.is_none());
        assert!(matches!(
            app.nodes.get(&class),
            Some((
                _,
                NodeType::Config(ConfigNodeType::NodeClass {
                    has_node_config: true,
                    ..
                })
            ))
        ));

        if let Some((_, NodeType::Config(ConfigNodeType::NodeClass { target, .. }))) =
            app.nodes.get_mut(&class)
        {
            *target = None;
        }
        app.propagate_values();
        assert!(app.computed.node_classes.is_empty());
    }

    #[test]
    fn deleting_the_target_clears_the_node_class() {
        let mut app = empty_app();
        let workflow = add(&mut app, NodeType::Workflow("filter".to_string()));
        let class = add(
            &mut app,
            NodeType::Config(ConfigNodeType::NodeClass {
                target: Some(workflow.clone()),
                has_node_config: false,
            }),
        );

        app.delete_nodes(&[workflow]);

        assert!(matches!(
            app.nodes.get(&class),
            Some((
                _,
                NodeType::Config(ConfigNodeType::NodeClass { target: None, .. })
            ))
        ));
    }

    #[test]
    fn alpha_builder_applies_alpha_to_palette_color() {
        // ThemeExtended.primary_base -> Alpha (alpha 0.25) -> SelectionBox.fill
        // -> Catalog.selection_box: the fill is the palette color at 0.25.
        use nodes::pins;
        let mut app = empty_app();
        let theme = add(&mut app, NodeType::ThemeExtended);
        let alpha_slider = slider(&mut app, 0.25);
        let alpha = add(&mut app, NodeType::Alpha(AlphaNode::default()));
        let cfg = add(
            &mut app,
            NodeType::Config(ConfigNodeType::SelectionBoxConfig(
                SelectionBoxConfigInputs::default(),
            )),
        );
        let catalog = catalog(&mut app);
        wire(
            &mut app,
            &theme,
            pins::theme_ext::PRIMARY_BASE,
            &alpha,
            pins::build::ALPHA_COLOR,
        );
        wire(
            &mut app,
            &alpha_slider,
            pins::input::VALUE,
            &alpha,
            pins::build::ALPHA,
        );
        wire(
            &mut app,
            &alpha,
            pins::build::ALPHA_OUT,
            &cfg,
            pins::selection_box::FILL,
        );
        wire(
            &mut app,
            &cfg,
            pins::cfg::SELECTION_BOX_OUT,
            &catalog,
            pins::cfg::SELECTION_BOX,
        );

        app.propagate_values();

        let fill = app.computed.selection_box.fill.expect("fill not computed");
        let base = app.current_theme.extended_palette().primary.base.color;
        assert_eq!(fill.a, 0.25);
        assert_eq!((fill.r, fill.g, fill.b), (base.r, base.g, base.b));
    }

    /// The four facts that prove the boot rig is complete and wired.
    fn assert_rig_complete(app: &Application) {
        use nodes::pins;
        let catalog_id = app
            .node_order
            .iter()
            .find(|id| {
                matches!(
                    app.nodes.get(*id),
                    Some((_, NodeType::Config(ConfigNodeType::Catalog { .. })))
                )
            })
            .expect("boot scene has no Catalog node");
        let connected = connected(app, catalog_id);
        for input in pins::cfg::CATALOG_INPUTS {
            assert!(connected.contains(input), "Catalog input {input} unwired");
        }
        assert_eq!(app.computed.node.corner_radius, Some(8.0));
        assert_eq!(app.computed.node.opacity, Some(0.88));
        assert_eq!(app.computed.graph.tiling_spacing, Some(40.0));
        assert_eq!(app.computed.node_classes.len(), 1);
    }

    #[test]
    fn boot_rig_wires_every_catalog_input() {
        let mut app = Application::default();
        app.propagate_values();
        assert_rig_complete(&app);
    }

    #[test]
    fn boot_rig_feeds_every_config_field() {
        // Every field pin of every config node in the rig has a source: the
        // rig is the demo's proof that the node system covers the Catalog.
        let mut app = Application::default();
        app.propagate_values();

        // Pins the rig deliberately leaves unwired to keep data-type coloring.
        let optional: &[(&str, PinLabel)] = &[
            ("Pin", nodes::pins::pin::COLOR),
            ("Edge", nodes::pins::edge::STROKE_COLOR),
        ];
        let unwired = |title: &str, label: PinLabel, is_none: bool| {
            assert!(
                !is_none || optional.contains(&(title, label)),
                "{title} config: {label} has no source"
            );
        };

        // The rig's "Node" frame (idle class) is the one whose config carries
        // the corner radius; the others are partial by design.
        let node_inputs: Vec<&NodeConfigInputs> = app
            .nodes
            .values()
            .filter_map(|(_, t)| match t {
                NodeType::Config(ConfigNodeType::NodeConfig(i)) => Some(i),
                _ => None,
            })
            .collect();
        let idle = node_inputs
            .iter()
            .find(|i| i.corner_radius.is_some())
            .expect("idle node config");
        for (label, none) in [
            (nodes::pins::node::FILL_COLOR, idle.fill_color.is_none()),
            (nodes::pins::node::OPACITY, idle.opacity.is_none()),
            (nodes::pins::node::BORDER_COLOR, idle.border_color.is_none()),
            (nodes::pins::node::BORDER_WIDTH, idle.border_width.is_none()),
            (
                nodes::pins::node::BORDER_OUTLINE_WIDTH,
                idle.border_outline_width.is_none(),
            ),
            (
                nodes::pins::node::BORDER_OUTLINE_COLOR,
                idle.border_outline_color.is_none(),
            ),
            (nodes::pins::node::PATTERN, idle.pattern_type.is_none()),
            (nodes::pins::node::DASH, idle.dash_length.is_none()),
            (nodes::pins::node::GAP, idle.gap_length.is_none()),
            (nodes::pins::node::ANGLE, idle.pattern_angle.is_none()),
            (nodes::pins::node::SPEED, idle.animation_speed.is_none()),
            (nodes::pins::node::SHADOW_COLOR, idle.shadow_color.is_none()),
            (
                nodes::pins::node::SHADOW_DISTANCE,
                idle.shadow_distance.is_none(),
            ),
            (
                nodes::pins::node::SHADOW_OFFSET,
                idle.shadow_offset.is_none(),
            ),
        ] {
            unwired("Node", label, none);
        }

        let edge = app
            .nodes
            .values()
            .find_map(|(_, t)| match t {
                NodeType::Config(ConfigNodeType::EdgeConfig(i)) if i.curve.is_some() => Some(i),
                _ => None,
            })
            .expect("idle edge config");
        for (label, none) in [
            (nodes::pins::edge::STROKE_COLOR, edge.stroke_color.is_none()),
            (nodes::pins::edge::THICKNESS, edge.thickness.is_none()),
            (
                nodes::pins::edge::STROKE_OUTLINE_WIDTH,
                edge.stroke_outline_width.is_none(),
            ),
            (
                nodes::pins::edge::STROKE_OUTLINE_COLOR,
                edge.stroke_outline_color.is_none(),
            ),
            (nodes::pins::edge::PATTERN, edge.pattern_type.is_none()),
            (nodes::pins::edge::DASH, edge.dash_length.is_none()),
            (nodes::pins::edge::GAP, edge.gap_length.is_none()),
            (nodes::pins::edge::DOT_RADIUS, edge.dot_radius.is_none()),
            (nodes::pins::edge::ANGLE, edge.pattern_angle.is_none()),
            (nodes::pins::edge::SPEED, edge.animation_speed.is_none()),
            (nodes::pins::edge::BORDER_WIDTH, edge.border_width.is_none()),
            (nodes::pins::edge::BORDER_GAP, edge.border_gap.is_none()),
            (nodes::pins::edge::BORDER_COLOR, edge.border_color.is_none()),
            (
                nodes::pins::edge::BORDER_BACKGROUND,
                edge.border_background.is_none(),
            ),
            (
                nodes::pins::edge::BORDER_OUTLINE_WIDTH,
                edge.border_outline_width.is_none(),
            ),
            (
                nodes::pins::edge::BORDER_OUTLINE_COLOR,
                edge.border_outline_color.is_none(),
            ),
            (nodes::pins::edge::SHADOW_COLOR, edge.shadow_color.is_none()),
            (nodes::pins::edge::SHADOW_BLUR, edge.shadow_blur.is_none()),
            (
                nodes::pins::edge::SHADOW_EXPAND,
                edge.shadow_expand.is_none(),
            ),
            (
                nodes::pins::edge::SHADOW_OFFSET,
                edge.shadow_offset.is_none(),
            ),
        ] {
            unwired("Edge", label, none);
        }

        let pin = app
            .nodes
            .values()
            .find_map(|(_, t)| match t {
                NodeType::Config(ConfigNodeType::PinConfig(i)) if i.radius.is_some() => Some(i),
                _ => None,
            })
            .expect("idle pin config");
        for (label, none) in [
            (nodes::pins::pin::COLOR, pin.color.is_none()),
            (nodes::pins::pin::CUTOUT_RADIUS, pin.cutout_radius.is_none()),
            (nodes::pins::pin::SHAPE, pin.shape.is_none()),
            (nodes::pins::pin::BORDER_COLOR, pin.border_color.is_none()),
            (nodes::pins::pin::BORDER_WIDTH, pin.border_width.is_none()),
        ] {
            unwired("Pin", label, none);
        }

        // The single-instance classes: every field is set.
        let anchor = app.computed.anchor(AnchorStatus::Idle);
        assert!(anchor.core_size.is_some() && anchor.ring_width.is_some());
        assert!(anchor.core_color.is_some() && anchor.ring_color.is_some());
        assert!(anchor.core_border_color.is_some() && anchor.core_border_width.is_some());
        assert!(anchor.orbit_offset.is_some() && anchor.orbit_spacing.is_some());
        assert!(anchor.core_radius.is_some());

        let g = &app.computed.graph;
        assert!(g.background_color.is_some() && g.tiling_kind.is_some());
        assert!(g.tiling_thickness.is_some() && g.tiling_color.is_some());

        let s = &app.computed.selection_box;
        assert!(s.fill.is_some() && s.border_color.is_some() && s.border_width.is_some());

        let c = &app.computed.cutting_tool;
        assert!(c.color.is_some() && c.width.is_some());

        let m = &app.computed.minimap;
        assert!(m.background.is_some() && m.border_color.is_some());
        assert!(m.border_width.is_some() && m.node_color.is_some());
        assert!(m.selected_node_color.is_some() && m.viewport_fill.is_some());
        assert!(m.viewport_border_color.is_some() && m.viewport_border_width.is_some());

        // Translucency arrives through the Alpha builders.
        assert!(s.fill.unwrap().a < 1.0);
        assert!(m.background.unwrap().a < 1.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn persistence_round_trips_the_rig() {
        let app = Application::default();
        let saved = persistence::SavedState::from_app(
            &app.nodes,
            &app.node_order,
            &app.edges,
            &app.edge_order,
            &app.current_theme,
            app.camera_position,
            app.camera_zoom,
            None,
            None,
            &app.edge_config_sections,
            &app.node_config_sections,
            None,
            &app.anchors,
            app.next_anchor,
        );
        let json = serde_json::to_string(&saved).expect("serialize");
        let loaded: persistence::SavedState = serde_json::from_str(&json).expect("parse");
        let (nodes, node_order, edges, edge_order, _, _, _, _, _, _, _, _, anchors, next_anchor) =
            loaded.to_app();

        let mut restored = Application {
            nodes,
            node_order,
            edges,
            edge_order,
            anchors,
            next_anchor,
            ..Application::default()
        };
        restored.propagate_values();
        assert_rig_complete(&restored);
        assert_eq!(restored.anchors, app.anchors);
        assert_eq!(restored.next_anchor, 1);
        let routed: Vec<_> = restored
            .edges
            .values()
            .filter(|e| !e.route.is_empty())
            .collect();
        assert_eq!(routed.len(), 1);
        assert_eq!(routed[0].route, vec![0]);
    }

    #[test]
    fn test_node_config_shadow_resolves() {
        use iced_nodegraph::{ColorQuad, NodeStatus, NodeStyle};

        // A NodeConfig with shadow fields set must carry them through build()
        // (overlay) and resolve_over() (concrete style) unchanged.
        let inputs = NodeConfigInputs {
            shadow_color: Some(ColorQuad::solid(Color::from_rgb(1.0, 0.0, 0.0))),
            shadow_distance: Some(12.0),
            shadow_offset: Some((4.0, 6.0)),
            ..Default::default()
        };
        let overlay = inputs.build();
        assert_eq!(overlay.shadow_color, Some(Color::from_rgb(1.0, 0.0, 0.0)));
        assert_eq!(overlay.shadow_distance, Some(12.0));
        assert_eq!(overlay.shadow_offset, Some((4.0, 6.0)));

        // The comment preset carries no shadow; the overlay must install one.
        let resolved = overlay.resolve_over(NodeStyle::comment(&Theme::Dark, NodeStatus::Idle));
        assert_eq!(resolved.shadow_color, Color::from_rgb(1.0, 0.0, 0.0));
        assert_eq!(resolved.shadow_distance, 12.0);
        assert_eq!(resolved.shadow_offset, (4.0, 6.0));
    }

    // === Boot scene: what the widget resolves ===

    /// The workflow node the boot rig's Node Class targets.
    fn class_target(app: &Application) -> NodeId {
        app.nodes
            .values()
            .find_map(|(_, t)| match t {
                NodeType::Config(ConfigNodeType::NodeClass {
                    target: Some(id), ..
                }) => Some(id.clone()),
                _ => None,
            })
            .expect("boot rig has a targeted Node Class")
    }

    #[test]
    fn classed_node_keeps_its_tint_while_selected() {
        let app = Application::default();
        let theme = app.effective_theme().clone();
        let palette = theme.extended_palette();
        let calendar = class_target(&app);

        let idle = app.node_style(&calendar, &theme, NodeStatus::Idle);
        let selected = app.node_style(&calendar, &theme, NodeStatus::Selected);
        // The class wins for what it sets (fill and border color)...
        assert_eq!(idle.fill_color.near_start, palette.warning.weak.color);
        assert_eq!(selected.fill_color.near_start, palette.warning.weak.color);
        assert_eq!(selected.border_color.near_start, palette.warning.base.color);
        // ...and the selected class supplies the rest.
        assert_eq!(selected.border_pattern.thickness, 2.0);
        assert_eq!(selected.border_outline_width, 3.0);
        assert_eq!(selected.shadow_distance, 11.0);
        assert_eq!(idle.border_pattern.thickness, 1.0);

        // An unclassed node takes the Catalog's selected class as a whole.
        let other = app.node_order[0].clone();
        let plain = app.node_style(&other, &theme, NodeStatus::Selected);
        assert_eq!(plain.fill_color.near_start, palette.primary.weak.color);
        assert_eq!(plain.border_color.near_start, palette.primary.base.color);
    }

    #[test]
    fn hovered_anchor_takes_the_accent() {
        let app = Application::default();
        let theme = app.effective_theme().clone();
        let primary = theme.extended_palette().primary.base.color;

        let idle = app.anchor_style(&theme, AnchorStatus::Idle);
        let hovered = app.anchor_style(&theme, AnchorStatus::Hovered);
        assert_eq!(idle.core_border_width, 1.0);
        assert_eq!(hovered.core_border_width, 1.5);
        let accent = hovered.core_border_color.near_start;
        assert_eq!(
            (accent.r, accent.g, accent.b),
            (primary.r, primary.g, primary.b)
        );
        assert_eq!(accent.a, 0.6);
        // Fields the hovered frame does not set come from the idle class.
        assert_eq!(hovered.orbit_offset, idle.orbit_offset);
        assert_eq!(hovered.core_size, 6.0);
    }

    #[test]
    fn chrome_resolves_from_the_rig() {
        let app = Application::default();
        let theme = app.effective_theme().clone();
        let palette = theme.extended_palette();

        assert_eq!(app.selection_box_style(&theme).fill.a, 0.15);
        assert_eq!(app.cutting_tool_style(&theme).width, 3.0);
        assert_eq!(
            app.cutting_tool_style(&theme).color,
            palette.danger.base.color
        );
        assert_eq!(app.minimap_style(&theme).background.a, 0.9);
        let graph = app.graph_style(&theme);
        assert_eq!(graph.background_color, palette.background.base.color);
        assert_eq!(graph.tiling.map(|t| t.spacing), Some(40.0));
        let float = TypeId::of::<nodes::pins::Float>();
        assert_eq!(app.pin_style(float, &theme, PinStatus::Idle).radius, 5.0);
        assert_eq!(
            app.edge_style(&theme, EdgeStatus::PendingCut, float, float)
                .stroke_color
                .near_start,
            palette.danger.base.color
        );
    }

    // === Headless interaction: real events through `view()` ===

    use iced::mouse;
    use iced_test::Simulator;
    use std::any::TypeId;

    type Sim<'a> = Simulator<'a, ApplicationMessage, Theme, iced::Renderer>;

    /// The boot scene at the identity camera, large enough to hold the rig's
    /// first frames, so layout coordinates are world coordinates.
    fn simulator(app: &Application) -> Sim<'_> {
        Simulator::with_size(
            iced::Settings::default(),
            Size::new(2000.0, 1600.0),
            app.view(),
        )
    }

    /// Runs the simulator's messages through `update` once the simulator has
    /// released its borrow of `app`.
    fn apply(app: &mut Application, msgs: Vec<ApplicationMessage>) {
        for m in msgs {
            let _ = app.update(m);
        }
    }

    fn moved(p: Point) -> iced::Event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position: p })
    }

    fn click(ui: &mut Sim<'_>, at: Point) {
        ui.point_at(at);
        ui.simulate([
            moved(at),
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    }

    #[test]
    fn clicking_the_corner_radius_slider_reshapes_every_node() {
        let mut app = Application::default();
        let theme = app.effective_theme().clone();
        let calendar = class_target(&app);
        assert_eq!(
            app.node_style(&calendar, &theme, NodeStatus::Idle)
                .corner_radius,
            8.0
        );

        let msgs: Vec<_> = {
            let mut ui = simulator(&app);
            // The slider node: title bar (text + 4px padding), then a body with
            // 10px padding holding the 100px-wide, 16px-tall slider.
            let title = ui.find("corner_radius").expect("corner_radius slider node");
            let bounds = title.bounds();
            let slider_left = bounds.x + 4.0 + 1.0;
            let slider_y = bounds.y + bounds.height + 4.0 + 10.0 + 8.0;
            click(&mut ui, Point::new(slider_left + 75.0, slider_y));
            ui.into_messages().collect()
        };
        apply(&mut app, msgs.clone());
        let moved_to = msgs
            .iter()
            .find_map(|m| match m {
                ApplicationMessage::SliderChanged { value, .. } => Some(*value),
                _ => None,
            })
            .expect("the click reached the slider");
        assert!((20.0..=28.0).contains(&moved_to), "slider value {moved_to}");
        assert_eq!(app.computed.node.corner_radius, Some(moved_to));
        for id in &app.node_order {
            if matches!(app.nodes[id].1, NodeType::Workflow(_)) {
                assert_eq!(
                    app.node_style(id, &theme, NodeStatus::Idle).corner_radius,
                    moved_to
                );
            }
        }
    }

    #[test]
    fn selection_box_alpha_slider_drives_its_opacity() {
        use nodes::pins;
        let mut app = Application::default();
        let theme = app.effective_theme().clone();
        assert_eq!(app.selection_box_style(&theme).fill.a, 0.15);

        // Walk the rig backwards: SelectionBoxConfig.fill <- Alpha <- slider.
        let cfg = app
            .nodes
            .iter()
            .find_map(|(id, (_, t))| {
                matches!(t, NodeType::Config(ConfigNodeType::SelectionBoxConfig(_)))
                    .then(|| id.clone())
            })
            .expect("selection box config");
        let alpha = app
            .edges
            .values()
            .find(|e| e.to_node == cfg && e.to_pin == pins::selection_box::FILL)
            .map(|e| e.from_node.clone())
            .expect("alpha node feeding fill");
        let slider = app
            .edges
            .values()
            .find(|e| e.to_node == alpha && e.to_pin == pins::build::ALPHA)
            .map(|e| e.from_node.clone())
            .expect("slider feeding alpha");

        let _ = app.update(ApplicationMessage::SliderChanged {
            node_id: slider,
            value: 0.6,
        });
        assert_eq!(app.selection_box_style(&theme).fill.a, 0.6);
    }

    #[test]
    fn node_class_pick_list_retargets_through_the_graph() {
        let mut app = Application::default();
        let theme = app.effective_theme().clone();
        let before = class_target(&app);
        let first_workflow = app.node_order[0].clone();
        assert_ne!(before, first_workflow);

        let msgs: Vec<_> = {
            let mut ui = simulator(&app);
            // The pick list sits right of the "target" label; opening it and
            // clicking the first row picks the first workflow node.
            let label = ui.find("target").expect("Node Class target row");
            let bounds = label.bounds();
            let picker = Point::new(bounds.x + bounds.width + 6.0 + 30.0, bounds.center_y());
            click(&mut ui, picker);
            click(&mut ui, Point::new(picker.x, picker.y + 24.0));
            ui.into_messages().collect()
        };
        apply(&mut app, msgs.clone());
        assert!(
            msgs.iter().any(|m| matches!(
                m,
                ApplicationMessage::NodeClassTargetChanged { target: Some(t), .. } if *t == first_workflow
            )),
            "pick list did not report the new target: {msgs:?}"
        );
        assert_eq!(class_target(&app), first_workflow);
        let palette = theme.extended_palette();
        assert_eq!(
            app.node_style(&first_workflow, &theme, NodeStatus::Idle)
                .fill_color
                .near_start,
            palette.warning.weak.color
        );
        assert_ne!(
            app.node_style(&before, &theme, NodeStatus::Idle)
                .fill_color
                .near_start,
            palette.warning.weak.color
        );
    }

    #[test]
    fn test_edge_config_inputs_pattern_type_dashed() {
        use iced_nodegraph::SdfPatternType;

        let inputs = EdgeConfigInputs {
            pattern_type: Some(PatternType::Dashed),
            thickness: Some(3.0),
            dash_length: Some(10.0),
            gap_length: Some(5.0),
            ..Default::default()
        };

        let config = inputs.build();
        let pattern = config.pattern.expect("pattern should be Some");
        assert_eq!(pattern.thickness, 3.0);
        assert!(
            matches!(pattern.pattern_type, SdfPatternType::Dashed { dash, gap, .. } if (dash - 10.0).abs() < 0.01 && (gap - 5.0).abs() < 0.01),
            "Expected Dashed pattern, got {:?}",
            pattern.pattern_type
        );
    }

    #[test]
    fn test_edge_config_inputs_pattern_type_arrowed() {
        use iced_nodegraph::SdfPatternType;

        let inputs = EdgeConfigInputs {
            pattern_type: Some(PatternType::Arrowed),
            ..Default::default()
        };

        let config = inputs.build();
        let pattern = config.pattern.expect("pattern should be Some");
        assert!(
            matches!(pattern.pattern_type, SdfPatternType::Arrowed { .. }),
            "Expected Arrowed pattern, got {:?}",
            pattern.pattern_type
        );
    }

    #[test]
    fn test_edge_config_inputs_pattern_type_dotted() {
        use iced_nodegraph::SdfPatternType;

        let inputs = EdgeConfigInputs {
            pattern_type: Some(PatternType::Dotted),
            dot_radius: Some(3.0),
            gap_length: Some(4.0),
            ..Default::default()
        };

        let config = inputs.build();
        let pattern = config.pattern.expect("pattern should be Some");
        assert!(
            matches!(pattern.pattern_type, SdfPatternType::Dotted { .. }),
            "Expected Dotted pattern, got {:?}",
            pattern.pattern_type
        );
    }

    #[test]
    fn test_edge_config_inputs_pattern_type_dash_dotted() {
        use iced_nodegraph::SdfPatternType;

        let inputs = EdgeConfigInputs {
            pattern_type: Some(PatternType::DashDotted),
            ..Default::default()
        };

        let config = inputs.build();
        let pattern = config.pattern.expect("pattern should be Some");
        assert!(
            matches!(pattern.pattern_type, SdfPatternType::DashDotted { .. }),
            "Expected DashDotted pattern, got {:?}",
            pattern.pattern_type
        );
    }

    #[test]
    fn test_edge_config_inputs_pattern_preserved_through_build() {
        // Verify the full pipeline: EdgeConfigInputs -> build() -> EdgeConfig
        // Pattern, border, shadow must all survive
        use iced_nodegraph::SdfPatternType;

        let inputs = EdgeConfigInputs {
            pattern_type: Some(PatternType::Dashed),
            thickness: Some(4.0),
            dash_length: Some(8.0),
            gap_length: Some(4.0),
            animation_speed: Some(50.0),
            stroke_color: Some(iced_nodegraph::ColorQuad::arc(
                Color::from_rgb(1.0, 0.0, 0.0),
                Color::from_rgb(0.0, 0.0, 1.0),
            )),
            border_width: Some(2.0),
            border_gap: Some(1.0),
            shadow_blur: Some(6.0),
            shadow_expand: Some(3.0),
            ..Default::default()
        };

        let config = inputs.build();

        // Pattern
        let pattern = config.pattern.expect("pattern must be present");
        assert_eq!(pattern.thickness, 4.0);
        assert!(matches!(
            pattern.pattern_type,
            SdfPatternType::Dashed { .. }
        ));
        assert!((pattern.flow_speed - 50.0).abs() < 0.01);

        // Colors: arc gradient start -> end
        let stroke = config.stroke_color.expect("stroke color present");
        assert_eq!(stroke.near_start, Color::from_rgb(1.0, 0.0, 0.0));
        assert_eq!(stroke.near_end, Color::from_rgb(0.0, 0.0, 1.0));

        // Border
        assert_eq!(config.border_width, Some(2.0));
        assert_eq!(config.border_gap, Some(1.0));

        // Shadow
        assert_eq!(config.shadow_blur, Some(6.0));
        assert_eq!(config.shadow_expand, Some(3.0));
    }
}
