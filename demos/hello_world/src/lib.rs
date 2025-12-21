//! # Hello World Demo
//!
//! Basic node graph with command palette (Cmd/Ctrl+K) for adding nodes and changing themes.
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
//! - **Cmd/Ctrl+K** - Open command palette
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

mod nodes;

use iced::{
    Color, Event, Length, Point, Subscription, Task, Theme, Vector, event, keyboard,
    widget::stack,
    window,
};
use iced_nodegraph::{node_graph, EdgeStyle, NodeStyle, PinReference};
use iced_palette::{
    Command, Shortcut, command, command_palette, find_matching_shortcut, focus_input,
    get_filtered_command_index, get_filtered_count, is_toggle_shortcut, navigate_down, navigate_up,
};
use nodes::{
    border_width_config_node, color_picker_node, color_preset_node, corner_radius_config_node,
    edge_color_config_node, edge_thickness_config_node, fill_color_config_node,
    float_slider_node, node, opacity_config_node, ConfigNodeType, FloatSliderConfig,
    InputNodeType, NodeType, NodeValue,
};
use std::collections::{HashMap, HashSet};

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
    let window_settings = iced::window::Settings::default();
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
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    NodeMoved {
        node_index: usize,
        new_position: Point,
    },
    EdgeDisconnected {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
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
        x: f32,
        y: f32,
        node_type: NodeType,
    },
    ChangeTheme(Theme),
    NavigateToSubmenu(String),
    NavigateBack,
    Tick,
    // Selection-related messages
    SelectionChanged(Vec<usize>),
    CloneNodes(Vec<usize>),
    DeleteNodes(Vec<usize>),
    GroupMoved {
        indices: Vec<usize>,
        delta: Vector,
    },
    // Input node value changes
    SliderChanged {
        node_index: usize,
        value: f32,
    },
    ColorChanged {
        node_index: usize,
        color: Color,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum PaletteView {
    Main,
    Submenu(String),
}

/// Computed style values from connected config nodes
#[derive(Debug, Clone, Default)]
struct ComputedStyle {
    corner_radius: Option<f32>,
    opacity: Option<f32>,
    border_width: Option<f32>,
    fill_color: Option<Color>,
    edge_thickness: Option<f32>,
    edge_color: Option<Color>,
}

impl ComputedStyle {
    /// Builds a NodeStyle from computed values, using defaults where not set
    fn to_node_style(&self) -> NodeStyle {
        let mut style = NodeStyle::default();
        if let Some(r) = self.corner_radius {
            style = style.corner_radius(r);
        }
        if let Some(o) = self.opacity {
            style = style.opacity(o);
        }
        if let Some(w) = self.border_width {
            style = style.border_width(w);
        }
        if let Some(c) = self.fill_color {
            style = style.fill_color(c);
        }
        style
    }

    /// Builds an EdgeStyle from computed values, returns None if no edge styling is set
    fn to_edge_style(&self) -> Option<EdgeStyle> {
        if self.edge_color.is_none() && self.edge_thickness.is_none() {
            return None;
        }

        let mut style = EdgeStyle::default();
        if let Some(t) = self.edge_thickness {
            style = style.thickness(t);
        }
        if let Some(c) = self.edge_color {
            style = style.color(c);
        }
        Some(style)
    }
}

struct Application {
    edges: Vec<(PinReference, PinReference)>,
    nodes: Vec<(Point, NodeType)>,
    selected_nodes: HashSet<usize>,
    command_palette_open: bool,
    command_input: String,
    current_theme: Theme,
    palette_view: PaletteView,
    palette_selected_index: usize,
    palette_preview_theme: Option<Theme>,
    palette_original_theme: Option<Theme>,
    /// Computed style values from config node connections
    computed_style: ComputedStyle,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: vec![
                (PinReference::new(0, 0), PinReference::new(1, 0)), // trigger.output -> parser.email
                (PinReference::new(1, 1), PinReference::new(2, 0)), // parser.subject -> filter.input
                (PinReference::new(1, 2), PinReference::new(3, 0)), // parser.datetime -> calendar.datetime
                (PinReference::new(2, 1), PinReference::new(3, 1)), // filter.matches -> calendar.title
            ],
            nodes: vec![
                (
                    Point::new(100.0, 150.0),
                    NodeType::Workflow("email_trigger".to_string()),
                ),
                (
                    Point::new(350.0, 150.0),
                    NodeType::Workflow("email_parser".to_string()),
                ),
                (
                    Point::new(350.0, 350.0),
                    NodeType::Workflow("filter".to_string()),
                ),
                (
                    Point::new(650.0, 250.0),
                    NodeType::Workflow("calendar".to_string()),
                ),
            ],
            selected_nodes: HashSet::new(),
            command_palette_open: false,
            command_input: String::new(),
            current_theme: Theme::CatppuccinFrappe,
            palette_view: PaletteView::Main,
            palette_selected_index: 0,
            palette_preview_theme: None,
            palette_original_theme: None,
            computed_style: ComputedStyle::default(),
        }
    }
}

impl Application {
    fn new() -> Self {
        Self::default()
    }

    /// Propagates values from input nodes to connected config nodes
    fn propagate_values(&mut self) {
        let mut new_computed = ComputedStyle::default();

        // For each edge, check if it connects an input to a config node
        for (from, to) in &self.edges {
            let from_node = self.nodes.get(from.node_id);
            let to_node = self.nodes.get(to.node_id);

            if let (Some((_, from_type)), Some((_, to_type))) = (from_node, to_node) {
                // Check if from is an input node and to is a config node
                if let (NodeType::Input(input), NodeType::Config(config)) = (from_type, to_type) {
                    let value = input.output_value();

                    match config {
                        ConfigNodeType::CornerRadius => {
                            if let Some(v) = value.as_float() {
                                new_computed.corner_radius = Some(v);
                            }
                        }
                        ConfigNodeType::Opacity => {
                            if let Some(v) = value.as_float() {
                                new_computed.opacity = Some(v);
                            }
                        }
                        ConfigNodeType::BorderWidth => {
                            if let Some(v) = value.as_float() {
                                new_computed.border_width = Some(v);
                            }
                        }
                        ConfigNodeType::FillColor => {
                            if let Some(c) = value.as_color() {
                                new_computed.fill_color = Some(c);
                            }
                        }
                        ConfigNodeType::EdgeThickness => {
                            if let Some(v) = value.as_float() {
                                new_computed.edge_thickness = Some(v);
                            }
                        }
                        ConfigNodeType::EdgeColor => {
                            if let Some(c) = value.as_color() {
                                new_computed.edge_color = Some(c);
                            }
                        }
                    }
                }
            }
        }

        self.computed_style = new_computed;
    }

    /// Gets the value connected to a config node (if any)
    fn get_config_input_value(&self, node_index: usize) -> Option<NodeValue> {
        // Find edges where this node is the target
        for (from, to) in &self.edges {
            if to.node_id == node_index {
                if let Some((_, NodeType::Input(input))) = self.nodes.get(from.node_id) {
                    return Some(input.output_value());
                }
            }
        }
        None
    }

    fn update(&mut self, message: ApplicationMessage) -> Task<ApplicationMessage> {
        match message {
            ApplicationMessage::Noop => Task::none(),
            ApplicationMessage::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges.push((
                    PinReference::new(from_node, from_pin),
                    PinReference::new(to_node, to_pin),
                ));
                self.propagate_values();
                Task::none()
            }
            ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(node_index) {
                    *position = new_position;
                }
                Task::none()
            }
            ApplicationMessage::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges.retain(|(from, to)| {
                    !(from.node_id == from_node
                        && from.pin_id == from_pin
                        && to.node_id == to_node
                        && to.pin_id == to_pin)
                });
                self.propagate_values();
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
                _ => Task::none(),
            },
            ApplicationMessage::CommandPaletteNavigate(new_index) => {
                if !self.command_palette_open {
                    return Task::none();
                }
                self.palette_selected_index = new_index;

                if let PaletteView::Submenu(ref submenu) = self.palette_view {
                    if submenu == "themes" {
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
                            ApplicationMessage::SpawnNode { x, y, node_type } => {
                                let new_idx = self.nodes.len();
                                self.nodes.push((Point::new(x, y), node_type));
                                self.selected_nodes = HashSet::from([new_idx]);
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
            ApplicationMessage::SpawnNode { x, y, node_type } => {
                let new_idx = self.nodes.len();
                self.nodes.push((Point::new(x, y), node_type));
                self.selected_nodes = HashSet::from([new_idx]);
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
                Task::none()
            }
            ApplicationMessage::ChangeTheme(theme) => {
                self.current_theme = theme;
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
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
            ApplicationMessage::SelectionChanged(indices) => {
                self.selected_nodes = indices.into_iter().collect();
                Task::none()
            }
            ApplicationMessage::CloneNodes(indices) => {
                let offset = Vector::new(50.0, 50.0);
                let mut index_map: HashMap<usize, usize> = HashMap::new();
                let mut new_indices = Vec::new();

                for &idx in &indices {
                    if let Some((pos, node_type)) = self.nodes.get(idx) {
                        let new_pos = Point::new(pos.x + offset.x, pos.y + offset.y);
                        let new_idx = self.nodes.len();
                        self.nodes.push((new_pos, node_type.clone()));
                        index_map.insert(idx, new_idx);
                        new_indices.push(new_idx);
                    }
                }

                let edges_to_clone: Vec<_> = self
                    .edges
                    .iter()
                    .filter(|(from, to)| {
                        indices.contains(&from.node_id) && indices.contains(&to.node_id)
                    })
                    .cloned()
                    .collect();

                for (from, to) in edges_to_clone {
                    if let (Some(&new_from), Some(&new_to)) =
                        (index_map.get(&from.node_id), index_map.get(&to.node_id))
                    {
                        self.edges.push((
                            PinReference::new(new_from, from.pin_id),
                            PinReference::new(new_to, to.pin_id),
                        ));
                    }
                }

                self.selected_nodes = new_indices.into_iter().collect();
                self.propagate_values();
                Task::none()
            }
            ApplicationMessage::DeleteNodes(indices) => {
                let mut sorted_indices: Vec<_> = indices.into_iter().collect();
                sorted_indices.sort_by(|a, b| b.cmp(a));

                for idx in sorted_indices {
                    self.edges
                        .retain(|(from, to)| from.node_id != idx && to.node_id != idx);

                    for (from, to) in &mut self.edges {
                        if from.node_id > idx {
                            from.node_id -= 1;
                        }
                        if to.node_id > idx {
                            to.node_id -= 1;
                        }
                    }

                    if idx < self.nodes.len() {
                        self.nodes.remove(idx);
                    }
                }

                self.selected_nodes.clear();
                self.propagate_values();
                Task::none()
            }
            ApplicationMessage::GroupMoved { indices, delta } => {
                for idx in indices {
                    if let Some((pos, _)) = self.nodes.get_mut(idx) {
                        pos.x += delta.x;
                        pos.y += delta.y;
                    }
                }
                Task::none()
            }
            ApplicationMessage::SliderChanged { node_index, value } => {
                if let Some((_, NodeType::Input(InputNodeType::FloatSlider { value: v, .. }))) =
                    self.nodes.get_mut(node_index)
                {
                    *v = value;
                    self.propagate_values();
                }
                Task::none()
            }
            ApplicationMessage::ColorChanged { node_index, color } => {
                if let Some((_, node_type)) = self.nodes.get_mut(node_index) {
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
        let computed_style = self.computed_style.to_node_style();

        let mut ng = node_graph()
            .on_connect(
                |from_node, from_pin, to_node, to_pin| ApplicationMessage::EdgeConnected {
                    from_node,
                    from_pin,
                    to_node,
                    to_pin,
                },
            )
            .on_disconnect(|from_node, from_pin, to_node, to_pin| {
                ApplicationMessage::EdgeDisconnected {
                    from_node,
                    from_pin,
                    to_node,
                    to_pin,
                }
            })
            .on_move(|node_index, new_position| ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            })
            .on_select(ApplicationMessage::SelectionChanged)
            .on_clone(ApplicationMessage::CloneNodes)
            .on_delete(ApplicationMessage::DeleteNodes)
            .on_group_move(|indices, delta| ApplicationMessage::GroupMoved { indices, delta })
            .selection(&self.selected_nodes);

        // Add all nodes from state
        for (idx, (position, node_type)) in self.nodes.iter().enumerate() {
            let element: iced::Element<'_, ApplicationMessage> = match node_type {
                NodeType::Workflow(name) => node(name.as_str(), &self.current_theme),
                NodeType::Input(input) => match input {
                    InputNodeType::FloatSlider { config, value } => {
                        let idx = idx;
                        float_slider_node(
                            &self.current_theme,
                            *value,
                            config,
                            move |v| ApplicationMessage::SliderChanged {
                                node_index: idx,
                                value: v,
                            },
                        )
                    }
                    InputNodeType::ColorPicker { color } => {
                        let idx = idx;
                        color_picker_node(&self.current_theme, *color, move |c| {
                            ApplicationMessage::ColorChanged {
                                node_index: idx,
                                color: c,
                            }
                        })
                    }
                    InputNodeType::ColorPreset { color } => {
                        let idx = idx;
                        color_preset_node(&self.current_theme, *color, move |c| {
                            ApplicationMessage::ColorChanged {
                                node_index: idx,
                                color: c,
                            }
                        })
                    }
                },
                NodeType::Config(config) => {
                    let input_value = self.get_config_input_value(idx);
                    match config {
                        ConfigNodeType::CornerRadius => {
                            corner_radius_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_float()),
                            )
                        }
                        ConfigNodeType::Opacity => {
                            opacity_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_float()),
                            )
                        }
                        ConfigNodeType::BorderWidth => {
                            border_width_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_float()),
                            )
                        }
                        ConfigNodeType::FillColor => {
                            fill_color_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_color()),
                            )
                        }
                        ConfigNodeType::EdgeThickness => {
                            edge_thickness_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_float()),
                            )
                        }
                        ConfigNodeType::EdgeColor => {
                            edge_color_config_node(
                                &self.current_theme,
                                input_value.and_then(|v| v.as_color()),
                            )
                        }
                    }
                }
            };

            // Apply computed style to workflow nodes only (not to input/config nodes)
            if matches!(node_type, NodeType::Workflow(_)) {
                ng.push_node_styled(*position, element, computed_style.clone());
            } else {
                ng.push_node(*position, element);
            }
        }

        // Add stored edges with optional computed style
        let edge_style = self.computed_style.to_edge_style();
        for (from, to) in &self.edges {
            if let Some(ref style) = edge_style {
                ng.push_edge_styled(*from, *to, style.clone());
            } else {
                ng.push_edge(*from, *to);
            }
        }

        let graph_view = ng.into();

        if self.command_palette_open {
            let (_, commands) = self.build_palette_commands();

            stack!(
                graph_view,
                command_palette(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                    ApplicationMessage::CommandPaletteInput,
                    ApplicationMessage::CommandPaletteSelect,
                    ApplicationMessage::CommandPaletteNavigate,
                    || ApplicationMessage::CommandPaletteCancel
                )
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            graph_view
        }
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
                        .action(ApplicationMessage::NavigateToSubmenu("input_nodes".to_string())),
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
                            x: 400.0,
                            y: 300.0,
                            node_type: NodeType::Workflow(name.to_string()),
                        })
                    })
                    .collect();
                ("Workflow Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "input_nodes" => {
                let commands = vec![
                    command("corner_radius_slider", "Corner Radius Slider")
                        .description("Float slider for corner radius (0-20)")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 100.0,
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::corner_radius(),
                                value: 5.0,
                            }),
                        }),
                    command("opacity_slider", "Opacity Slider")
                        .description("Float slider for opacity (0.1-1.0)")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 200.0,
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::opacity(),
                                value: 0.75,
                            }),
                        }),
                    command("border_width_slider", "Border Width Slider")
                        .description("Float slider for border width (0.5-5)")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 300.0,
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::border_width(),
                                value: 1.0,
                            }),
                        }),
                    command("thickness_slider", "Edge Thickness Slider")
                        .description("Float slider for edge thickness (0.5-8)")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 400.0,
                            node_type: NodeType::Input(InputNodeType::FloatSlider {
                                config: FloatSliderConfig::thickness(),
                                value: 2.0,
                            }),
                        }),
                    command("color_picker", "Color Picker (RGB)")
                        .description("Full RGB color picker with sliders")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 500.0,
                            node_type: NodeType::Input(InputNodeType::ColorPicker {
                                color: Color::from_rgb(0.5, 0.5, 0.5),
                            }),
                        }),
                    command("color_preset", "Color Presets")
                        .description("Quick color selection from presets")
                        .action(ApplicationMessage::SpawnNode {
                            x: 100.0,
                            y: 600.0,
                            node_type: NodeType::Input(InputNodeType::ColorPreset {
                                color: Color::from_rgb(0.5, 0.5, 0.5),
                            }),
                        }),
                ];
                ("Input Nodes", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "config_nodes" => {
                let commands = vec![
                    command("cfg_corner_radius", "Node Corner Radius")
                        .description("Apply corner radius to all workflow nodes")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 100.0,
                            node_type: NodeType::Config(ConfigNodeType::CornerRadius),
                        }),
                    command("cfg_opacity", "Node Opacity")
                        .description("Apply opacity to all workflow nodes")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 200.0,
                            node_type: NodeType::Config(ConfigNodeType::Opacity),
                        }),
                    command("cfg_border_width", "Node Border Width")
                        .description("Apply border width to all workflow nodes")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 300.0,
                            node_type: NodeType::Config(ConfigNodeType::BorderWidth),
                        }),
                    command("cfg_fill_color", "Node Fill Color")
                        .description("Apply fill color to all workflow nodes")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 400.0,
                            node_type: NodeType::Config(ConfigNodeType::FillColor),
                        }),
                    command("cfg_edge_thickness", "Edge Thickness")
                        .description("Apply thickness to all edges")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 500.0,
                            node_type: NodeType::Config(ConfigNodeType::EdgeThickness),
                        }),
                    command("cfg_edge_color", "Edge Color")
                        .description("Apply color to all edges")
                        .action(ApplicationMessage::SpawnNode {
                            x: 400.0,
                            y: 600.0,
                            node_type: NodeType::Config(ConfigNodeType::EdgeColor),
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
