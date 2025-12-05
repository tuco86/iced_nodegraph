//! # Visual Shader Editor Demo
//!
//! A visual shader editor demonstrating complex node graph functionality.
//! Create WGSL shaders by connecting nodes visually.
//!
//! ## Interactive Demo
//!
//! <div id="demo-container" style="margin: 2em 0;">
//!   <style>
//!     #demo-container canvas,
//!     #demo-container #demo-canvas-container {
//!       display: block !important;
//!       position: absolute !important;
//!       top: 0 !important;
//!       left: 0 !important;
//!       width: 100% !important;
//!       height: 100% !important;
//!       pointer-events: auto !important;
//!     }
//!     #demo-loading {
//!       position: absolute;
//!       top: 50%;
//!       left: 50%;
//!       transform: translate(-50%, -50%);
//!       text-align: center;
//!       color: #89b4fa;
//!     }
//!     .demo-spinner {
//!       width: 40px;
//!       height: 40px;
//!       border: 3px solid #313244;
//!       border-top-color: #89b4fa;
//!       border-radius: 50%;
//!       animation: demo-spin 1s linear infinite;
//!       margin: 0 auto 1em;
//!     }
//!     @keyframes demo-spin {
//!       to { transform: rotate(360deg); }
//!     }
//!     #demo-info {
//!       position: absolute;
//!       bottom: 15px;
//!       right: 15px;
//!       background: rgba(30, 30, 46, 0.95);
//!       border: 1px solid #45475a;
//!       border-radius: 8px;
//!       padding: 0.75rem 1rem;
//!       font-size: 0.75rem;
//!       color: #cdd6f4;
//!     }
//!     #demo-info h4 {
//!       color: #89b4fa;
//!       font-size: 0.875rem;
//!       margin-bottom: 0.5rem;
//!     }
//!     #demo-info ul {
//!       list-style: none;
//!       line-height: 1.6;
//!       margin: 0;
//!       padding: 0;
//!     }
//!     #demo-info li:before {
//!       content: "- ";
//!       color: #89b4fa;
//!     }
//!     #demo-error {
//!       display: none;
//!       padding: 1.5rem;
//!       background: #f38ba8;
//!       color: #1e1e2e;
//!       border-radius: 8px;
//!       margin: 1em 0;
//!     }
//!   </style>
//!
//!   <div style="position: relative; width: 100%; height: 600px; background: #1e1e2e; border-radius: 12px; overflow: hidden; box-shadow: 0 8px 32px rgba(0,0,0,0.3);">
//!     <div id="demo-loading">
//!       <div class="demo-spinner"></div>
//!       <p>Loading demo...</p>
//!     </div>
//!     <div id="demo-canvas-container"></div>
//!     <div id="demo-info">
//!       <h4>Controls</h4>
//!       <ul>
//!         <li>Cmd/Ctrl+K: Command palette</li>
//!         <li>Drag nodes to move</li>
//!         <li>Drag pins to connect</li>
//!         <li>Scroll to zoom</li>
//!         <li>Middle-drag to pan</li>
//!       </ul>
//!     </div>
//!   </div>
//!
//!   <div id="demo-error">
//!     <strong>Failed to load demo.</strong> WebGPU required.
//!   </div>
//!
//!   <script type="module">
//!     let demoInitialized = false;
//!
//!     async function initDemo() {
//!       if (demoInitialized) return;
//!
//!       try {
//!         const demo = await import('./pkg/demo_shader_editor.js');
//!         await demo.default();
//!
//!         document.getElementById('demo-loading').style.display = 'none';
//!
//!         demoInitialized = true;
//!         demo.run_demo();
//!
//!         setTimeout(() => {
//!           const canvas = document.querySelector('#demo-canvas-container canvas');
//!           if (canvas) {
//!             canvas.setAttribute('tabindex', '0');
//!             canvas.focus();
//!           }
//!         }, 100);
//!
//!       } catch (error) {
//!         console.error('Demo error:', error);
//!         document.getElementById('demo-loading').style.display = 'none';
//!         document.getElementById('demo-error').style.display = 'block';
//!       }
//!     }
//!
//!     initDemo();
//!   </script>
//! </div>

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
    widget::{column, container, stack, text},
    keyboard, event,
    Color, Element, Event, Length, Point, Subscription, Task, Theme,
};
use iced_nodegraph::{node_graph, PinDirection, PinSide};
use iced_palette::{
    command_palette, command, get_filtered_command_index, get_filtered_count,
    navigate_up, navigate_down, focus_input, Command,
};
use shader_graph::ShaderGraph;
use shader_graph::nodes::ShaderNodeType;

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
    // Command palette messages
    ToggleCommandPalette,
    CommandPaletteInput(String),
    CommandPaletteNavigateUp,
    CommandPaletteNavigateDown,
    CommandPaletteSelect(usize),
    CommandPaletteConfirm,
    CommandPaletteCancel,
    // Node spawning
    SpawnNode(ShaderNodeType),
    // Theme
    ChangeTheme(Theme),
}

struct Application {
    shader_graph: ShaderGraph,
    compiled_shader: Option<String>,
    compilation_error: Option<String>,
    visual_edges: Vec<((usize, usize), (usize, usize))>,
    current_theme: Theme,
    // Command palette state
    command_palette_open: bool,
    command_input: String,
    palette_selected_index: usize,
}

impl Application {
    fn new() -> (Self, iced::Task<Message>) {
        let shader_graph = default_shader::create_default_graph();

        // Convert shader graph connections to visual edges
        // NodeGraph widget uses flat pin indices: [input0, input1, ..., output0, output1, ...]
        // ShaderGraph uses separate indices: from_socket = output index, to_socket = input index
        let visual_edges: Vec<((usize, usize), (usize, usize))> = shader_graph
            .connections
            .iter()
            .filter_map(|conn| {
                // Get the nodes to find their input/output counts
                let from_node = shader_graph.nodes.iter().find(|n| n.id == conn.from_node)?;
                let to_node = shader_graph.nodes.iter().find(|n| n.id == conn.to_node)?;

                // Find node indices (position in nodes vec)
                let from_node_idx = shader_graph.nodes.iter().position(|n| n.id == conn.from_node)?;
                let to_node_idx = shader_graph.nodes.iter().position(|n| n.id == conn.to_node)?;

                // from_socket is an output index -> visual pin = num_inputs + output_index
                let from_visual_pin = from_node.inputs.len() + conn.from_socket;

                // to_socket is an input index -> visual pin = input_index (inputs come first)
                let to_visual_pin = conn.to_socket;

                println!(
                    "Edge: {}:{} (out {}) -> {}:{} (in {})",
                    from_node.node_type.name(), from_visual_pin, conn.from_socket,
                    to_node.node_type.name(), to_visual_pin, conn.to_socket
                );

                Some(((from_node_idx, from_visual_pin), (to_node_idx, to_visual_pin)))
            })
            .collect();

        let mut app = Self {
            shader_graph,
            compiled_shader: None,
            compilation_error: None,
            visual_edges,
            current_theme: Theme::CatppuccinMocha,
            command_palette_open: false,
            command_input: String::new(),
            palette_selected_index: 0,
        };

        app.recompile();

        (app, iced::Task::none())
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                // Store visual edge as-is
                self.visual_edges.push(((from_node, from_pin), (to_node, to_pin)));

                // Convert visual pin indices to shader socket indices
                // First, gather the info we need
                let connection_info = {
                    let from = self.shader_graph.nodes.get(from_node);
                    let to = self.shader_graph.nodes.get(to_node);

                    if let (Some(from), Some(to)) = (from, to) {
                        // from_pin is visual index, output starts after inputs
                        let from_socket = from_pin.saturating_sub(from.inputs.len());
                        // to_pin is visual index, inputs come first so it's direct
                        let to_socket = to_pin;

                        if from_socket < from.outputs.len() && to_socket < to.inputs.len() {
                            Some((from.id, from_socket, to.id, to_socket,
                                  from.node_type.name().to_string(),
                                  to.node_type.name().to_string()))
                        } else {
                            println!(
                                "Invalid connection: pin {} (outputs: {}) -> pin {} (inputs: {})",
                                from_pin, from.outputs.len(), to_pin, to.inputs.len()
                            );
                            None
                        }
                    } else {
                        None
                    }
                };

                // Now apply the connection
                if let Some((from_id, from_socket, to_id, to_socket, from_name, to_name)) = connection_info {
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
            Message::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.visual_edges.retain(|e| {
                    !(e.0.0 == from_node && e.0.1 == from_pin && e.1.0 == to_node && e.1.1 == to_pin)
                });
                self.shader_graph.connections.retain(|c| {
                    !(c.from_node == from_node
                        && c.from_socket == from_pin
                        && c.to_node == to_node
                        && c.to_socket == to_pin)
                });
                self.recompile();
                return Task::none();
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
                // Spawn node in center of view
                let position = Point::new(400.0, 300.0);
                self.shader_graph.add_node(node_type, position);
                println!("Spawned node: {}", node_type.name());
            }
            Message::ChangeTheme(theme) => {
                self.current_theme = theme;
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        // Build node graph
        let mut graph = node_graph()
            .on_connect(|from_node, from_pin, to_node, to_pin| Message::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            })
            .on_move(|node_index, new_position| Message::NodeMoved {
                node_index,
                new_position,
            })
            .on_disconnect(|from_node, from_pin, to_node, to_pin| Message::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            });

        // Add all shader graph nodes
        for node in &self.shader_graph.nodes {
            let node_content = create_node_widget(&node.node_type, &self.current_theme);
            graph.push_node(node.position, node_content);
        }

        // Add all edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.visual_edges {
            graph.push_edge(*from_node, *from_pin, *to_node, *to_pin);
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

        event::listen_with(|event, _status, _id| {
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                // Ctrl+Space or Ctrl+K to toggle palette
                if modifiers.command() {
                    if key == keyboard::Key::Named(Named::Space)
                        || key == keyboard::Key::Character("k".into())
                    {
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
        })
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
    let pin_section = if inputs.is_empty() && outputs.is_empty() {
        // No pins - minimal output indicator
        container(
            column![
                node_pin(
                    PinSide::Right,
                    container(text("out").size(11)).padding([0, 8])
                )
                .direction(PinDirection::Output)
                .color(Color::from_rgb(0.6, 0.6, 0.6))
            ]
            .spacing(2),
        )
        .padding([6, 0])
    } else {
        // Build pins dynamically but wrap in container same way
        let mut pin_elements: Vec<iced::Element<'a, Message>> = Vec::new();

        for input in inputs {
            let socket_color = get_socket_color(&input.socket_type);
            let label = input.name.clone();
            pin_elements.push(
                node_pin(
                    PinSide::Left,
                    container(text(label).size(11)).padding([0, 8]),
                )
                .direction(PinDirection::Input)
                .pin_type(format!("{:?}", input.socket_type))
                .color(socket_color)
                .into(),
            );
        }

        for output in outputs {
            let socket_color = get_socket_color(&output.socket_type);
            let label = output.name.clone();
            pin_elements.push(
                node_pin(
                    PinSide::Right,
                    container(text(label).size(11)).padding([0, 8]),
                )
                .direction(PinDirection::Output)
                .pin_type(format!("{:?}", output.socket_type))
                .color(socket_color)
                .into(),
            );
        }

        container(
            iced::widget::Column::with_children(pin_elements).spacing(2),
        )
        .padding([6, 0])
    };

    column![title_bar, pin_section].width(160.0).into()
}

fn get_socket_color(socket_type: &shader_graph::sockets::SocketType) -> Color {
    use shader_graph::sockets::SocketType;
    match socket_type {
        SocketType::Float => Color::from_rgb(0.6, 0.8, 0.6),  // Light green
        SocketType::Vec2 => Color::from_rgb(0.6, 0.8, 0.9),   // Light cyan
        SocketType::Vec3 => Color::from_rgb(0.9, 0.8, 0.5),   // Light yellow
        SocketType::Vec4 => Color::from_rgb(0.9, 0.6, 0.7),   // Light pink
        SocketType::Bool => Color::from_rgb(0.9, 0.5, 0.5),   // Light red
        SocketType::Int => Color::from_rgb(0.7, 0.7, 0.9),    // Light purple
    }
}
