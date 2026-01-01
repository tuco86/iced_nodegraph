//! # Visual Shader Editor Demo
//!
//! A visual shader editor demonstrating complex node graph functionality.
//! Create WGSL shaders by connecting nodes visually.
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
//! - **Cmd/Ctrl+Space** - Open command palette to add shader nodes
//! - **Drag nodes** - Move nodes around the canvas
//! - **Drag from pins** - Create connections between compatible sockets
//! - **Scroll** - Zoom in/out
//! - **Middle-drag** - Pan the canvas
//!
//! ## Available Nodes
//!
//! Use the command palette to add nodes from categories: Math, Vector, Color,
//! Texture, Input, and Output. Connect them to build WGSL fragment shaders.

pub mod colors;
mod compiler;
mod default_shader;
mod shader_graph;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

use compiler::ShaderCompiler;
use iced::{
    Color, Element, Event, Length, Point, Subscription, Task, Theme, Vector, event, keyboard,
    widget::{column, container, stack, text},
    window,
};
use iced_nodegraph::{PinDirection, PinRef, PinSide, node_graph, node_pin};
use iced_palette::{
    Command, command, command_palette, focus_input, get_filtered_command_index, get_filtered_count,
    navigate_down, navigate_up,
};
use shader_graph::ShaderGraph;
use shader_graph::nodes::ShaderNodeType;
use std::collections::HashSet;

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
        .title("Visual Shader Editor - iced_nodegraph")
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
enum Message {
    EdgeConnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    NodeMoved {
        node_index: usize,
        new_position: Point,
    },
    EdgeDisconnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    SelectionChanged(Vec<usize>),
    GroupMoved {
        indices: Vec<usize>,
        delta: Vector,
    },
    // Command palette messages
    ToggleCommandPalette,
    CommandPaletteInput(String),
    CommandPaletteNavigateUp,
    CommandPaletteNavigateDown,
    CommandPaletteNavigate(usize),
    CommandPaletteSelect(usize),
    CommandPaletteConfirm,
    CommandPaletteCancel,
    // Node spawning
    SpawnNode(ShaderNodeType),
    // Theme
    ChangeTheme(Theme),
    // Animation
    Tick,
    // Camera/viewport tracking
    CameraChanged {
        position: Point,
        zoom: f32,
    },
    WindowResized(iced::Size),
}

struct Application {
    shader_graph: ShaderGraph,
    compiled_shader: Option<String>,
    compilation_error: Option<String>,
    visual_edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    current_theme: Theme,
    graph_selection: HashSet<usize>,
    // Command palette state
    command_palette_open: bool,
    command_input: String,
    palette_selected_index: usize,
    // Camera/viewport tracking for spawn-at-center
    viewport_size: iced::Size,
    camera_position: Point,
    camera_zoom: f32,
}

impl Application {
    fn new() -> (Self, iced::Task<Message>) {
        let shader_graph = default_shader::create_default_graph();

        // Convert shader graph connections to visual edges
        // NodeGraph widget uses flat pin indices: [input0, input1, ..., output0, output1, ...]
        // ShaderGraph uses separate indices: from_socket = output index, to_socket = input index
        let visual_edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)> = shader_graph
            .connections
            .iter()
            .filter_map(|conn| {
                // Get the nodes to find their input/output counts
                let from_node = shader_graph.nodes.iter().find(|n| n.id == conn.from_node)?;
                let to_node = shader_graph.nodes.iter().find(|n| n.id == conn.to_node)?;

                // Find node indices (position in nodes vec)
                let from_node_idx = shader_graph
                    .nodes
                    .iter()
                    .position(|n| n.id == conn.from_node)?;
                let to_node_idx = shader_graph
                    .nodes
                    .iter()
                    .position(|n| n.id == conn.to_node)?;

                // from_socket is an output index -> visual pin = num_inputs + output_index
                let from_visual_pin = from_node.inputs.len() + conn.from_socket;

                // to_socket is an input index -> visual pin = input_index (inputs come first)
                let to_visual_pin = conn.to_socket;

                println!(
                    "Edge: {}:{} (out {}) -> {}:{} (in {})",
                    from_node.node_type.name(),
                    from_visual_pin,
                    conn.from_socket,
                    to_node.node_type.name(),
                    to_visual_pin,
                    conn.to_socket
                );

                Some((
                    PinRef::new(from_node_idx, from_visual_pin),
                    PinRef::new(to_node_idx, to_visual_pin),
                ))
            })
            .collect();

        let mut app = Self {
            shader_graph,
            compiled_shader: None,
            compilation_error: None,
            visual_edges,
            current_theme: Theme::CatppuccinMocha,
            graph_selection: HashSet::new(),
            command_palette_open: false,
            command_input: String::new(),
            palette_selected_index: 0,
            viewport_size: iced::Size::new(800.0, 600.0),
            camera_position: Point::ORIGIN,
            camera_zoom: 1.0,
        };

        app.recompile();

        (app, iced::Task::none())
    }

    /// Calculate spawn position at screen center, converted to world coordinates.
    fn spawn_position(&self) -> Point {
        // Screen center
        let screen_center_x = self.viewport_size.width / 2.0;
        let screen_center_y = self.viewport_size.height / 2.0;

        // Convert to world coordinates: world = screen / zoom - camera_position
        let world_x = screen_center_x / self.camera_zoom - self.camera_position.x;
        let world_y = screen_center_y / self.camera_zoom - self.camera_position.y;

        // Offset for node size (approximate center)
        Point::new(world_x - 60.0, world_y - 40.0)
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::EdgeConnected { from, to } => {
                // Store visual edge as-is
                self.visual_edges.push((from, to));

                // Convert visual pin indices to shader socket indices
                // First, gather the info we need
                let connection_info = {
                    let from_node_data = self.shader_graph.nodes.get(from.node_id);
                    let to_node_data = self.shader_graph.nodes.get(to.node_id);

                    if let (Some(from_node_data), Some(to_node_data)) =
                        (from_node_data, to_node_data)
                    {
                        // from.pin_id is visual index, output starts after inputs
                        let from_socket = from.pin_id.saturating_sub(from_node_data.inputs.len());
                        // to.pin_id is visual index, inputs come first so it's direct
                        let to_socket = to.pin_id;

                        if from_socket < from_node_data.outputs.len()
                            && to_socket < to_node_data.inputs.len()
                        {
                            Some((
                                from_node_data.id,
                                from_socket,
                                to_node_data.id,
                                to_socket,
                                from_node_data.node_type.name().to_string(),
                                to_node_data.node_type.name().to_string(),
                            ))
                        } else {
                            println!(
                                "Invalid connection: pin {} (outputs: {}) -> pin {} (inputs: {})",
                                from.pin_id,
                                from_node_data.outputs.len(),
                                to.pin_id,
                                to_node_data.inputs.len()
                            );
                            None
                        }
                    } else {
                        None
                    }
                };

                // Now apply the connection
                if let Some((from_id, from_socket, to_id, to_socket, from_name, to_name)) =
                    connection_info
                {
                    self.shader_graph.add_connection(shader_graph::Connection {
                        from_node: from_id,
                        from_socket,
                        to_node: to_id,
                        to_socket,
                    });
                    println!(
                        "Connected: {} output {} -> {} input {}",
                        from_name, from_socket, to_name, to_socket
                    );
                }
                self.recompile();
            }
            Message::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some(node) = self.shader_graph.get_node_by_index_mut(node_index) {
                    node.position = new_position;
                }
            }
            Message::EdgeDisconnected { from, to } => {
                self.visual_edges.retain(|(f, t)| !(f == &from && t == &to));
                self.shader_graph.connections.retain(|c| {
                    !(c.from_node == from.node_id
                        && c.from_socket == from.pin_id
                        && c.to_node == to.node_id
                        && c.to_socket == to.pin_id)
                });
                self.recompile();
                return Task::none();
            }
            Message::SelectionChanged(indices) => {
                self.graph_selection = indices.into_iter().collect();
            }
            Message::GroupMoved { indices, delta } => {
                for idx in indices {
                    if let Some(node) = self.shader_graph.get_node_by_index_mut(idx) {
                        node.position.x += delta.x;
                        node.position.y += delta.y;
                    }
                }
            }
            // Command palette
            Message::ToggleCommandPalette => {
                self.command_palette_open = !self.command_palette_open;
                if self.command_palette_open {
                    self.command_input.clear();
                    self.palette_selected_index = 0;
                    return focus_input();
                }
            }
            Message::CommandPaletteInput(input) => {
                self.command_input = input;
                self.palette_selected_index = 0;
            }
            Message::CommandPaletteNavigateUp => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let commands = self.build_palette_commands();
                let count = get_filtered_count(&self.command_input, &commands);
                self.palette_selected_index = navigate_up(self.palette_selected_index, count);
            }
            Message::CommandPaletteNavigateDown => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let commands = self.build_palette_commands();
                let count = get_filtered_count(&self.command_input, &commands);
                self.palette_selected_index = navigate_down(self.palette_selected_index, count);
            }
            Message::CommandPaletteNavigate(index) => {
                if !self.command_palette_open {
                    return Task::none();
                }
                self.palette_selected_index = index;
            }
            Message::CommandPaletteSelect(index) => {
                self.palette_selected_index = index;
                return self.update(Message::CommandPaletteConfirm);
            }
            Message::CommandPaletteConfirm => {
                if !self.command_palette_open {
                    return Task::none();
                }
                let commands = self.build_palette_commands();
                if let Some(original_idx) = get_filtered_command_index(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                ) {
                    use iced_palette::CommandAction;
                    if let CommandAction::Message(msg) = &commands[original_idx].action {
                        let msg = msg.clone();
                        self.command_palette_open = false;
                        self.command_input.clear();
                        self.palette_selected_index = 0;
                        return self.update(msg);
                    }
                }
            }
            Message::CommandPaletteCancel => {
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_selected_index = 0;
            }
            Message::SpawnNode(node_type) => {
                // Spawn node at screen center (converted to world coordinates)
                let position = self.spawn_position();
                self.shader_graph.add_node(node_type, position);
                println!("Spawned node: {}", node_type.name());
            }
            Message::ChangeTheme(theme) => {
                self.current_theme = theme;
            }
            Message::CameraChanged { position, zoom } => {
                self.camera_position = position;
                self.camera_zoom = zoom;
            }
            Message::WindowResized(size) => {
                self.viewport_size = size;
            }
            Message::Tick => {
                // Animation frame - handled by the widget
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        // Build node graph
        let mut graph = node_graph()
            .on_connect(|from, to| Message::EdgeConnected { from, to })
            .on_move(|node_index: usize, new_position| Message::NodeMoved {
                node_index,
                new_position,
            })
            .on_disconnect(|from, to| Message::EdgeDisconnected { from, to })
            .on_select(Message::SelectionChanged)
            .on_group_move(|indices, delta| Message::GroupMoved { indices, delta })
            .on_camera_change(|position, zoom| Message::CameraChanged { position, zoom })
            .selection(&self.graph_selection);

        // Add all shader graph nodes
        for (node_idx, node) in self.shader_graph.nodes.iter().enumerate() {
            let node_content = create_node_widget(&node.node_type, &self.current_theme);
            graph.push_node(node_idx, node.position, node_content);
        }

        // Add all edges
        for (from, to) in &self.visual_edges {
            graph.push_edge(*from, *to);
        }

        let graph_element: Element<Message> = graph.into();

        // Show command palette overlay if open
        if self.command_palette_open {
            let commands = self.build_palette_commands();
            stack![
                graph_element,
                command_palette(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                    Message::CommandPaletteInput,
                    Message::CommandPaletteSelect,
                    Message::CommandPaletteNavigate,
                    || Message::CommandPaletteCancel,
                )
            ]
            .into()
        } else {
            graph_element
        }
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        use iced::keyboard::key::Named;

        Subscription::batch(vec![
            // Keyboard events for command palette
            event::listen_with(|event, _status, _id| {
                if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                    // Ctrl+Space or Ctrl+Space to toggle palette
                    if modifiers.command() {
                        if key == keyboard::Key::Named(Named::Space) {
                            return Some(Message::ToggleCommandPalette);
                        }
                    }

                    // When palette is open, handle navigation
                    match key {
                        keyboard::Key::Named(Named::ArrowUp) => {
                            return Some(Message::CommandPaletteNavigateUp);
                        }
                        keyboard::Key::Named(Named::ArrowDown) => {
                            return Some(Message::CommandPaletteNavigateDown);
                        }
                        keyboard::Key::Named(Named::Enter) => {
                            return Some(Message::CommandPaletteConfirm);
                        }
                        keyboard::Key::Named(Named::Escape) => {
                            return Some(Message::CommandPaletteCancel);
                        }
                        _ => {}
                    }
                }
                None
            }),
            // Animation frames for NodeGraph
            window::frames().map(|_| Message::Tick),
            // Window resize events
            event::listen_with(|event, _, _| match event {
                Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(size)),
                _ => None,
            }),
        ])
    }

    fn build_palette_commands(&self) -> Vec<Command<Message>> {
        let mut commands = Vec::new();

        // Add node spawning commands for all shader node types
        for node_type in ShaderNodeType::all() {
            let category = node_type.category();
            commands.push(
                command(node_type.name(), node_type.name())
                    .description(format!("{} node", category))
                    .action(Message::SpawnNode(*node_type)),
            );
        }

        // Add theme switching commands
        commands.push(
            command("theme-dark", "Dark Theme")
                .description("Switch to dark theme")
                .action(Message::ChangeTheme(Theme::Dark)),
        );
        commands.push(
            command("theme-light", "Light Theme")
                .description("Switch to light theme")
                .action(Message::ChangeTheme(Theme::Light)),
        );
        commands.push(
            command("theme-catppuccin", "Catppuccin Mocha")
                .description("Switch to Catppuccin Mocha theme")
                .action(Message::ChangeTheme(Theme::CatppuccinMocha)),
        );
        commands.push(
            command("theme-dracula", "Dracula")
                .description("Switch to Dracula theme")
                .action(Message::ChangeTheme(Theme::Dracula)),
        );
        commands.push(
            command("theme-nord", "Nord")
                .description("Switch to Nord theme")
                .action(Message::ChangeTheme(Theme::Nord)),
        );

        commands
    }

    fn recompile(&mut self) {
        match ShaderCompiler::compile(&self.shader_graph) {
            Ok(shader) => {
                self.compiled_shader = Some(shader);
                self.compilation_error = None;
            }
            Err(err) => {
                self.compiled_shader = None;
                self.compilation_error = Some(format!("{:?}", err));
            }
        }
    }
}

fn create_node_widget<'a>(
    node_type: &shader_graph::nodes::ShaderNodeType,
    theme: &'a Theme,
) -> iced::Element<'a, Message> {
    use iced_nodegraph::node_pin;

    let name = node_type.name();
    let inputs = node_type.inputs();
    let outputs = node_type.outputs();

    let palette = theme.extended_palette();

    // Title bar - matching hello_world pattern exactly
    let title_bar = container(text(name).size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    // Build pin list - must match hello_world's column![] macro structure
    // Pin IDs use sequential indices: inputs first (0..n), then outputs (n..n+m)
    let pin_section = if inputs.is_empty() && outputs.is_empty() {
        // No pins - minimal output indicator
        container(
            column![
                node_pin(
                    PinSide::Right,
                    0usize,
                    container(text("out").size(11)).padding([0, 8])
                )
                .direction(PinDirection::Output)
                .color(colors::TEXT_MUTED)
            ]
            .spacing(2),
        )
        .padding([6, 0])
    } else {
        // Build pins dynamically but wrap in container same way
        let mut pin_elements: Vec<iced::Element<'a, Message>> = Vec::new();
        let num_inputs = inputs.len();

        for (i, input) in inputs.into_iter().enumerate() {
            let socket_color = get_socket_color(&input.socket_type);
            let label = input.name.clone();
            pin_elements.push(create_typed_pin(
                PinSide::Left,
                i, // Pin ID = input index
                label,
                PinDirection::Input,
                &input.socket_type,
                socket_color,
            ));
        }

        for (i, output) in outputs.into_iter().enumerate() {
            let socket_color = get_socket_color(&output.socket_type);
            let label = output.name.clone();
            pin_elements.push(create_typed_pin(
                PinSide::Right,
                num_inputs + i, // Pin ID = num_inputs + output index
                label,
                PinDirection::Output,
                &output.socket_type,
                socket_color,
            ));
        }

        container(iced::widget::Column::with_children(pin_elements).spacing(2)).padding([6, 0])
    };

    column![title_bar, pin_section].width(160.0).into()
}

fn get_socket_color(socket_type: &shader_graph::sockets::SocketType) -> Color {
    use shader_graph::sockets::SocketType;
    match socket_type {
        SocketType::Float => colors::SOCKET_FLOAT, // Light green
        SocketType::Vec2 => colors::SOCKET_VEC2,   // Light cyan
        SocketType::Vec3 => colors::SOCKET_VEC3,   // Light yellow
        SocketType::Vec4 => colors::SOCKET_VEC4,   // Light pink
        SocketType::Bool => colors::SOCKET_BOOL,   // Light red
        SocketType::Int => colors::SOCKET_INT,     // Light purple
    }
}

/// Creates a typed pin element based on the socket type.
/// Uses marker types for TypeId-based connection matching.
fn create_typed_pin<'a, Message: Clone + 'a>(
    side: PinSide,
    pin_id: usize,
    label: String,
    direction: PinDirection,
    socket_type: &shader_graph::sockets::SocketType,
    color: Color,
) -> Element<'a, Message> {
    use shader_graph::sockets::SocketType;

    let content = container(text(label).size(11)).padding([0, 8]);

    match socket_type {
        SocketType::Float => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Float>()
            .color(color)
            .into(),
        SocketType::Vec2 => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Vec2>()
            .color(color)
            .into(),
        SocketType::Vec3 => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Vec3>()
            .color(color)
            .into(),
        SocketType::Vec4 => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Vec4>()
            .color(color)
            .into(),
        SocketType::Bool => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Bool>()
            .color(color)
            .into(),
        SocketType::Int => node_pin(side, pin_id, content)
            .direction(direction)
            .data_type::<colors::Int>()
            .color(color)
            .into(),
    }
}
