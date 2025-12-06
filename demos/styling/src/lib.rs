//! # Styling Demo
//!
//! Interactive demonstration of node graph styling capabilities.
//!
//! This demo showcases:
//! - Per-node styling with NodeStyle
//! - Live style controls (corner radius, opacity, border width)
//! - Theme switching
//! - Different node type presets (Input, Process, Output, Comment)
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
//! - **Select nodes** - Click nodes in the right panel to select
//! - **Style sliders** - Adjust corner radius, opacity, border width
//! - **Presets** - Apply Input/Process/Output/Comment presets
//! - **Theme picker** - Switch between color themes
//! - **Scroll** - Zoom in/out
//! - **Middle-drag** - Pan the canvas

use iced::{
    Color, Element, Length, Point, Subscription, Task, Theme,
    widget::{button, column, container, row, slider, stack, text, pick_list},
};
use iced_nodegraph::{
    NodeStyle, NodeContentStyle, PinDirection, PinSide,
    node_graph, node_pin, node_title_bar,
};

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
        .title("Styling Demo - iced_nodegraph")
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
    // Graph events
    EdgeConnected {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    EdgeDisconnected {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    NodeMoved {
        node_index: usize,
        new_position: Point,
    },

    // Style controls
    CornerRadiusChanged(f32),
    OpacityChanged(f32),
    BorderWidthChanged(f32),
    SelectNode(usize),
    ApplyPreset(NodePreset),
    ChangeTheme(Theme),
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodePreset {
    Input,
    Process,
    Output,
    Comment,
}

impl std::fmt::Display for NodePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodePreset::Input => write!(f, "Input"),
            NodePreset::Process => write!(f, "Process"),
            NodePreset::Output => write!(f, "Output"),
            NodePreset::Comment => write!(f, "Comment"),
        }
    }
}

impl NodePreset {
    const ALL: [NodePreset; 4] = [
        NodePreset::Input,
        NodePreset::Process,
        NodePreset::Output,
        NodePreset::Comment,
    ];
}

struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, String, NodeStyle)>,
    current_theme: Theme,
    selected_node: Option<usize>,

    // Control panel state
    corner_radius: f32,
    opacity: f32,
    border_width: f32,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: vec![
                ((0, 0), (1, 0)), // Input -> Process
                ((1, 0), (2, 0)), // Process -> Output
            ],
            nodes: vec![
                (
                    Point::new(100.0, 150.0),
                    "Input Data".to_string(),
                    NodeStyle::input(),
                ),
                (
                    Point::new(350.0, 200.0),
                    "Transform".to_string(),
                    NodeStyle::process(),
                ),
                (
                    Point::new(600.0, 150.0),
                    "Output Result".to_string(),
                    NodeStyle::output(),
                ),
                (
                    Point::new(350.0, 400.0),
                    "Note: This is a comment".to_string(),
                    NodeStyle::comment(),
                ),
            ],
            current_theme: Theme::CatppuccinFrappe,
            selected_node: Some(0),
            corner_radius: 5.0,
            opacity: 0.75,
            border_width: 1.5,
        }
    }
}

impl Application {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick)
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges.push(((from_node, from_pin), (to_node, to_pin)));
            }
            Message::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges
                    .retain(|edge| *edge != ((from_node, from_pin), (to_node, to_pin)));
            }
            Message::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((pos, _, _)) = self.nodes.get_mut(node_index) {
                    *pos = new_position;
                }
            }
            Message::CornerRadiusChanged(value) => {
                self.corner_radius = value;
                self.apply_style_to_selected();
            }
            Message::OpacityChanged(value) => {
                self.opacity = value;
                self.apply_style_to_selected();
            }
            Message::BorderWidthChanged(value) => {
                self.border_width = value;
                self.apply_style_to_selected();
            }
            Message::SelectNode(index) => {
                self.selected_node = Some(index);
                // Load the selected node's style into controls
                if let Some((_, _, style)) = self.nodes.get(index) {
                    self.corner_radius = style.corner_radius;
                    self.opacity = style.opacity;
                    self.border_width = style.border_width;
                }
            }
            Message::ApplyPreset(preset) => {
                if let Some(index) = self.selected_node {
                    let new_style = match preset {
                        NodePreset::Input => NodeStyle::input(),
                        NodePreset::Process => NodeStyle::process(),
                        NodePreset::Output => NodeStyle::output(),
                        NodePreset::Comment => NodeStyle::comment(),
                    };
                    if let Some((_, _, style)) = self.nodes.get_mut(index) {
                        *style = new_style.clone();
                        self.corner_radius = new_style.corner_radius;
                        self.opacity = new_style.opacity;
                        self.border_width = new_style.border_width;
                    }
                }
            }
            Message::ChangeTheme(theme) => {
                self.current_theme = theme;
            }
            Message::Tick => {
                // Animation tick - handled by the widget
            }
        }
        Task::none()
    }

    fn apply_style_to_selected(&mut self) {
        if let Some(index) = self.selected_node {
            if let Some((_, _, style)) = self.nodes.get_mut(index) {
                style.corner_radius = self.corner_radius;
                style.opacity = self.opacity;
                style.border_width = self.border_width;
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let control_panel = self.build_control_panel();
        let graph = self.build_graph();

        // Use stack to overlay control panel on right side of graph
        // This avoids wrapping the graph in a container which breaks coordinates
        stack![
            graph,
            container(
                container(control_panel)
                    .width(Length::Fixed(280.0))
                    .height(Length::Fill)
                    .padding(15)
                    .style(|theme: &Theme| {
                        let palette = theme.extended_palette();
                        container::Style {
                            background: Some(palette.background.weak.color.into()),
                            ..Default::default()
                        }
                    })
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
        ]
        .into()
    }

    fn build_control_panel(&self) -> Element<'_, Message> {
        let theme = &self.current_theme;
        let palette = theme.extended_palette();
        let text_color = palette.background.base.text;

        let title = text("Style Controls")
            .size(18)
            .color(text_color);

        let selected_label = if let Some(index) = self.selected_node {
            let name = &self.nodes[index].1;
            text(format!("Selected: {}", name)).size(14).color(text_color)
        } else {
            text("No node selected").size(14).color(text_color)
        };

        // Node selection buttons
        let node_buttons: Element<'_, Message> = column(
            self.nodes
                .iter()
                .enumerate()
                .map(|(i, (_, name, _))| {
                    let is_selected = self.selected_node == Some(i);
                    button(text(name.clone()).size(12))
                        .on_press(Message::SelectNode(i))
                        .style(move |theme: &Theme, status| {
                            if is_selected {
                                button::primary(theme, status)
                            } else {
                                button::secondary(theme, status)
                            }
                        })
                        .width(Length::Fill)
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(5)
        .into();

        // Style sliders
        let corner_slider = column![
            text("Corner Radius").size(12).color(text_color),
            row![
                slider(0.0..=20.0, self.corner_radius, Message::CornerRadiusChanged),
                text(format!("{:.1}", self.corner_radius)).size(12).color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let opacity_slider = column![
            text("Opacity").size(12).color(text_color),
            row![
                slider(0.1..=1.0, self.opacity, Message::OpacityChanged).step(0.05),
                text(format!("{:.2}", self.opacity)).size(12).color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let border_slider = column![
            text("Border Width").size(12).color(text_color),
            row![
                slider(0.5..=5.0, self.border_width, Message::BorderWidthChanged).step(0.5),
                text(format!("{:.1}", self.border_width)).size(12).color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        // Preset buttons
        let preset_label = text("Apply Preset").size(12).color(text_color);
        let preset_buttons: Element<'_, Message> = row(
            NodePreset::ALL
                .iter()
                .map(|preset| {
                    button(text(preset.to_string()).size(11))
                        .on_press(Message::ApplyPreset(*preset))
                        .padding([4, 8])
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(5)
        .wrap()
        .into();

        // Theme selector
        let theme_label = text("Theme").size(12).color(text_color);
        let themes = vec![
            Theme::Dark,
            Theme::Light,
            Theme::CatppuccinFrappe,
            Theme::CatppuccinMocha,
            Theme::Dracula,
            Theme::Nord,
            Theme::SolarizedDark,
            Theme::SolarizedLight,
            Theme::GruvboxDark,
            Theme::GruvboxLight,
        ];
        let theme_picker = pick_list(
            themes,
            Some(self.current_theme.clone()),
            Message::ChangeTheme,
        )
        .width(Length::Fill);

        column![
            title,
            text("").height(Length::Fixed(10.0)), // Spacer
            selected_label,
            text("").height(Length::Fixed(10.0)),
            text("Select Node").size(12).color(text_color),
            node_buttons,
            text("").height(Length::Fixed(20.0)),
            corner_slider,
            text("").height(Length::Fixed(10.0)),
            opacity_slider,
            text("").height(Length::Fixed(10.0)),
            border_slider,
            text("").height(Length::Fixed(20.0)),
            preset_label,
            preset_buttons,
            text("").height(Length::Fixed(20.0)),
            theme_label,
            theme_picker,
        ]
        .spacing(5)
        .into()
    }

    fn build_graph(&self) -> Element<'_, Message> {
        let theme = &self.current_theme;

        let mut ng = node_graph()
            .on_connect(|from_node, from_pin, to_node, to_pin| Message::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            })
            .on_disconnect(|from_node, from_pin, to_node, to_pin| Message::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            })
            .on_move(|node_index, new_position| Message::NodeMoved {
                node_index,
                new_position,
            });

        for (position, name, style) in &self.nodes {
            let content_style = if style.fill_color.b > style.fill_color.r && style.fill_color.b > style.fill_color.g {
                NodeContentStyle::input(theme)
            } else if style.fill_color.g > style.fill_color.r && style.fill_color.g > style.fill_color.b {
                NodeContentStyle::process(theme)
            } else if style.fill_color.r > style.fill_color.g {
                NodeContentStyle::output(theme)
            } else {
                NodeContentStyle::comment(theme)
            };

            let node_content = column![
                node_title_bar(name.clone(), content_style),
                container(
                    column![
                        container(
                            node_pin(PinSide::Left, text!("input").size(11))
                                .direction(PinDirection::Input)
                                .color(Color::from_rgb(0.5, 0.7, 0.9))
                        )
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Left),
                        container(
                            node_pin(PinSide::Right, text!("output").size(11))
                                .direction(PinDirection::Output)
                                .color(Color::from_rgb(0.9, 0.7, 0.5))
                        )
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                    ]
                    .spacing(8)
                )
                .padding([8, 10]),
            ]
            .width(160.0);

            ng.push_node_styled(*position, node_content, style.clone());
        }

        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }

        ng.into()
    }
}
