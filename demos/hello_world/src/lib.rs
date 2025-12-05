//! # Hello World Demo
//!
//! Basic node graph with command palette (Cmd/Ctrl+K) for adding nodes and changing themes.
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
//!       content: "▸ ";
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
//!         <li>Click edges to disconnect</li>
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
//!         const demo = await import('./pkg/demo_hello_world.js');
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
//!
//! ## Description
//!
//! **Controls:**
//! - Drag nodes to move
//! - Drag from pins to connect  
//! - Click edges to disconnect
//! - Scroll to zoom, middle-drag to pan
//! - Cmd/Ctrl+K for command palette

use iced::{
    Color, Event, Length, Point, Subscription, Task, Theme, event, keyboard, window,
    widget::{column, container, stack, text},
};
use iced_nodegraph::{PinDirection, PinSide, node_graph, node_pin};
use iced_palette::{command_palette, command, get_filtered_command_index, get_filtered_count, is_toggle_shortcut, find_matching_shortcut, navigate_up, navigate_down, focus_input, Command, Shortcut};

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
        name: String,
    },
    ChangeTheme(Theme),
    NavigateToSubmenu(String),
    NavigateBack,
    Tick,
}

#[derive(Debug, Clone, PartialEq)]
enum PaletteView {
    Main,
    Submenu(String),
}

struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, String)>,
    command_palette_open: bool,
    command_input: String,
    current_theme: Theme,
    palette_view: PaletteView,
    palette_selected_index: usize,
    palette_preview_theme: Option<Theme>,
    palette_original_theme: Option<Theme>,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: vec![
                ((0, 0), (1, 0)), // Email Trigger -> Email Parser
                ((1, 0), (2, 0)), // Email Parser subject -> Filter
                ((1, 1), (3, 0)), // Email Parser datetime -> Calendar
                ((2, 0), (3, 1)), // Filter -> Calendar title
            ],
            nodes: vec![
                (Point::new(100.0, 150.0), "email_trigger".to_string()),
                (Point::new(350.0, 150.0), "email_parser".to_string()),
                (Point::new(350.0, 350.0), "filter".to_string()),
                (Point::new(650.0, 250.0), "calendar".to_string()),
            ],
            command_palette_open: false,
            command_input: String::new(),
            current_theme: Theme::CatppuccinFrappe,
            palette_view: PaletteView::Main,
            palette_selected_index: 0,
            palette_preview_theme: None,
            palette_original_theme: None,
        }
    }
}

impl Application {
    fn new() -> Self {
        Self::default()
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
                println!(
                    "Edge connected: node {} pin {} -> node {} pin {}",
                    from_node, from_pin, to_node, to_pin
                );
                self.edges.push(((from_node, from_pin), (to_node, to_pin)));
                Task::none()
            }
            ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(node_index) {
                    *position = new_position;
                    println!("Node {} moved to {:?}", node_index, new_position);
                }
                Task::none()
            }
            ApplicationMessage::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges
                    .retain(|edge| *edge != ((from_node, from_pin), (to_node, to_pin)));
                println!(
                    "Edge disconnected: node {} pin {} -> node {} pin {}",
                    from_node, from_pin, to_node, to_pin
                );
                Task::none()
            }
            ApplicationMessage::ToggleCommandPalette => {
                self.command_palette_open = !self.command_palette_open;
                if !self.command_palette_open {
                    // Closing - restore original theme if cancelled
                    if let Some(original) = self.palette_original_theme.take() {
                        self.current_theme = original;
                    }
                    self.palette_preview_theme = None;
                    self.command_input.clear();
                    self.palette_view = PaletteView::Main;
                    self.palette_selected_index = 0;
                    Task::none()
                } else {
                    // Opening - save original theme and focus input
                    self.palette_original_theme = Some(self.current_theme.clone());
                    self.palette_view = PaletteView::Main;
                    self.palette_selected_index = 0;
                    focus_input()
                }
            }
            ApplicationMessage::CommandPaletteInput(input) => {
                // Full replacement from text_input widget
                self.command_input = input;
                self.palette_selected_index = 0; // Reset selection on input change
                Task::none()
            }
            ApplicationMessage::ExecuteShortcut(cmd_id) => {
                // Execute command by shortcut ID - opens palette and focuses input
                match cmd_id.as_str() {
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
                }
            }
            ApplicationMessage::CommandPaletteNavigate(new_index) => {
                if !self.command_palette_open {
                    return Task::none();
                }
                self.palette_selected_index = new_index;

                // Apply live preview for theme submenu
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
                                self.palette_preview_theme =
                                    Some(themes[original_idx].clone());
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
                // Immediately confirm
                self.update(ApplicationMessage::CommandPaletteConfirm)
            }
            ApplicationMessage::CommandPaletteConfirm => {
                if !self.command_palette_open {
                    return Task::none(); // Ignore if palette is closed
                }
                let (_, commands) = self.build_palette_commands();
                let Some(original_idx) = get_filtered_command_index(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                ) else {
                    return Task::none();
                };

                // Get the action from the command and execute it
                use iced_palette::CommandAction;
                let cmd = &commands[original_idx];
                match &cmd.action {
                    CommandAction::Message(msg) => {
                        // Clone the message and process it
                        let msg = msg.clone();
                        // Reset palette state
                        self.command_input.clear();
                        self.palette_selected_index = 0;
                        // Execute the command's message
                        match msg {
                            ApplicationMessage::NavigateToSubmenu(submenu) => {
                                self.palette_view = PaletteView::Submenu(submenu);
                                focus_input() // Re-focus input after navigating to submenu
                            }
                            ApplicationMessage::SpawnNode { x, y, name } => {
                                self.nodes.push((Point::new(x, y), name));
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
                    return Task::none(); // Ignore if palette is closed
                }
                // Restore original theme
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
            ApplicationMessage::SpawnNode { x, y, name } => {
                // Use node type directly
                self.nodes.push((Point::new(x, y), name));
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
                focus_input() // Focus input when navigating to submenu
            }
            ApplicationMessage::NavigateBack => {
                self.palette_view = PaletteView::Main;
                self.command_input.clear();
                focus_input() // Focus input when going back
            }
            ApplicationMessage::Tick => {
                // Just trigger a redraw for animations
                Task::none()
            }
        }
    }

    fn theme(&self) -> Theme {
        // Use preview theme if available (live preview)
        self.palette_preview_theme
            .as_ref()
            .unwrap_or(&self.current_theme)
            .clone()
    }

    /// Returns all commands with shortcuts for subscription handling.
    /// This includes main commands and any nested submenu commands with shortcuts.
    fn get_main_commands_with_shortcuts() -> Vec<Command<ApplicationMessage>> {
        vec![
            command("add_node", "Add Node")
                .description("Add a new node to the graph")
                .shortcut(Shortcut::cmd('n'))
                .action(ApplicationMessage::ExecuteShortcut("add_node".to_string())),
            command("change_theme", "Change Theme")
                .description("Switch to a different color theme")
                .shortcut(Shortcut::cmd('t'))
                .action(ApplicationMessage::ExecuteShortcut("change_theme".to_string())),
        ]
    }

    fn get_node_types() -> Vec<&'static str> {
        vec!["email_trigger", "email_parser", "filter", "calendar"]
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
            });

        // Add all nodes from state
        for (position, name) in &self.nodes {
            ng.push_node(*position, node(name.as_str(), &self.current_theme));
        }

        // Add stored edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }

        let graph_view = ng.into();

        if self.command_palette_open {
            // Build commands based on current palette view
            let (_, commands) = self.build_palette_commands();

            stack!(
                graph_view,
                command_palette(
                    &self.command_input,
                    &commands,
                    self.palette_selected_index,
                    ApplicationMessage::CommandPaletteInput,
                    ApplicationMessage::CommandPaletteSelect,
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
                let commands = Self::get_node_types()
                    .iter()
                    .map(|name| {
                        command(*name, *name)
                            .action(ApplicationMessage::SpawnNode {
                                x: 400.0,
                                y: 300.0,
                                name: name.to_string(),
                            })
                    })
                    .collect();
                ("Add Node", commands)
            }
            PaletteView::Submenu(submenu) if submenu == "themes" => {
                let commands = Self::get_available_themes()
                    .iter()
                    .map(|theme| {
                        let name = Self::get_theme_name(theme);
                        command(name, name)
                            .action(ApplicationMessage::ChangeTheme(theme.clone()))
                    })
                    .collect();
                ("Choose Theme", commands)
            }
            _ => ("Command Palette", vec![]),
        }
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        Subscription::batch(vec![
            // Global keyboard events - using listen_with to filter properly
            event::listen_with(handle_keyboard_event),
            // Animation frames for NodeGraph and theme preview
            window::frames().map(|_| ApplicationMessage::Tick),
        ])
    }
}

/// Keyboard event handler for subscriptions.
/// Returns Some(message) only for events we want to handle, None otherwise.
/// This allows text_input to receive character events normally.
fn handle_keyboard_event(
    event: Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<ApplicationMessage> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            // Ctrl+Space: Toggle palette
            if is_toggle_shortcut(&key, modifiers) {
                return Some(ApplicationMessage::ToggleCommandPalette);
            }

            // Global shortcuts with Ctrl/Cmd (Ctrl+N, Ctrl+T, etc.)
            if modifiers.command() {
                let main_commands = Application::get_main_commands_with_shortcuts();
                if let Some(cmd_id) = find_matching_shortcut(&main_commands, &key, modifiers) {
                    return Some(ApplicationMessage::ExecuteShortcut(cmd_id.to_string()));
                }
            }

            // Navigation keys - only handle these, let text_input handle characters
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
                // Don't handle character keys - let text_input widget handle them
                _ => None,
            }
        }
        _ => None,
    }
}

// Email Trigger Node - Only outputs
fn email_trigger_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();

    let title_bar = container(text("Email Trigger").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    let pin_list = column![
        node_pin(
            PinSide::Right,
            container(text!("on email").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Output)
        .pin_type("email")
        .color(Color::from_rgb(0.3, 0.7, 0.9)), // Blue for email data
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

// Email Parser Node - Input + multiple outputs
fn email_parser_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();

    let title_bar = container(text("Email Parser").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    let pin_list = column![
        node_pin(
            PinSide::Left,
            container(text!("email").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Input)
        .pin_type("email")
        .color(Color::from_rgb(0.3, 0.7, 0.9)), // Blue for email data
        node_pin(
            PinSide::Right,
            container(text!("subject").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Output)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
        node_pin(
            PinSide::Right,
            container(text!("datetime").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Output)
        .pin_type("datetime")
        .color(Color::from_rgb(0.7, 0.3, 0.9)), // Purple for datetime
        node_pin(
            PinSide::Right,
            container(text!("body").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Output)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

// Filter Node - Input + output
fn filter_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();

    let title_bar = container(text("Filter").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    let pin_list = column![
        node_pin(
            PinSide::Left,
            container(text!("input").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Input)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
        node_pin(
            PinSide::Right,
            container(text!("matches").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Output)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(140.0).into()
}

// Calendar Node - Only inputs
fn calendar_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();

    let title_bar = container(text("Create Event").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    let pin_list = column![
        node_pin(
            PinSide::Left,
            container(text!("datetime").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Input)
        .pin_type("datetime")
        .color(Color::from_rgb(0.7, 0.3, 0.9)), // Purple for datetime
        node_pin(
            PinSide::Left,
            container(text!("title").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Input)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
        node_pin(
            PinSide::Left,
            container(text!("description").size(11)).padding([0, 8])
        )
        .direction(PinDirection::Input)
        .pin_type("string")
        .color(Color::from_rgb(0.9, 0.7, 0.3)), // Orange for strings
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

fn node<'a, Message>(node_type: &str, theme: &'a Theme) -> iced::Element<'a, Message>
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

