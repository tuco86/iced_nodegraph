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

mod nodes;

use iced::{
    Element, Length, Point, Subscription, Task, Theme, Vector,
    widget::{button, column, container, pick_list, row, slider, stack, text},
    window,
};
use iced_nodegraph::{NodeBorderStyle, NodeStyle, PinRef, node_graph};
use nodes::styled_node;
use std::collections::HashSet;

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
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    EdgeDisconnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    NodeMoved {
        node_index: usize,
        new_position: Point,
    },
    SelectionChanged(Vec<usize>),
    GroupMoved {
        indices: Vec<usize>,
        delta: Vector,
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
    edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    nodes: Vec<(Point, String, NodeStyle)>,
    current_theme: Theme,
    selected_node: Option<usize>,
    graph_selection: HashSet<usize>,

    // Control panel state
    corner_radius: f32,
    opacity: f32,
    border_width: f32,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: vec![
                (PinRef::new(0, 1), PinRef::new(1, 0)), // Input node output -> Process node input
                (PinRef::new(1, 1), PinRef::new(2, 0)), // Process node output -> Output node input
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
            graph_selection: HashSet::new(),
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
        window::frames().map(|_| Message::Tick)
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EdgeConnected { from, to } => {
                self.edges.push((from, to));
            }
            Message::EdgeDisconnected { from, to } => {
                self.edges.retain(|(f, t)| !(f == &from && t == &to));
            }
            Message::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((pos, _, _)) = self.nodes.get_mut(node_index) {
                    *pos = new_position;
                }
            }
            Message::SelectionChanged(indices) => {
                self.graph_selection = indices.into_iter().collect();
            }
            Message::GroupMoved { indices, delta } => {
                for idx in indices {
                    if let Some((pos, _, _)) = self.nodes.get_mut(idx) {
                        pos.x += delta.x;
                        pos.y += delta.y;
                    }
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
                    self.border_width = style.border.as_ref().map(|b| b.width).unwrap_or(1.0);
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
                        self.border_width = new_style.border.as_ref().map(|b| b.width).unwrap_or(1.0);
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
        if let Some(index) = self.selected_node
            && let Some((_, _, style)) = self.nodes.get_mut(index) {
                style.corner_radius = self.corner_radius;
                style.opacity = self.opacity;
                // Update border width in the border field
                if let Some(ref mut border) = style.border {
                    border.width = self.border_width;
                } else {
                    style.border = Some(NodeBorderStyle::new().width(self.border_width));
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

        let title = text("Style Controls").size(18).color(text_color);

        let selected_label = if let Some(index) = self.selected_node {
            let name = &self.nodes[index].1;
            text(format!("Selected: {}", name))
                .size(14)
                .color(text_color)
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
                text(format!("{:.1}", self.corner_radius))
                    .size(12)
                    .color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let opacity_slider = column![
            text("Opacity").size(12).color(text_color),
            row![
                slider(0.1..=1.0, self.opacity, Message::OpacityChanged).step(0.05),
                text(format!("{:.2}", self.opacity))
                    .size(12)
                    .color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let border_slider = column![
            text("Border Width").size(12).color(text_color),
            row![
                slider(0.5..=5.0, self.border_width, Message::BorderWidthChanged).step(0.5),
                text(format!("{:.1}", self.border_width))
                    .size(12)
                    .color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        // Preset buttons
        let preset_label = text("Apply Preset").size(12).color(text_color);
        let preset_buttons: Element<'_, Message> = row(NodePreset::ALL
            .iter()
            .map(|preset| {
                button(text(preset.to_string()).size(11))
                    .on_press(Message::ApplyPreset(*preset))
                    .padding([4, 8])
                    .into()
            })
            .collect::<Vec<_>>())
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
            .on_connect(|from, to| Message::EdgeConnected { from, to })
            .on_disconnect(|from, to| Message::EdgeDisconnected { from, to })
            .on_move(|node_index, new_position| Message::NodeMoved {
                node_index,
                new_position,
            })
            .on_select(Message::SelectionChanged)
            .on_group_move(|indices, delta| Message::GroupMoved { indices, delta })
            .selection(&self.graph_selection);

        for (index, (position, name, style)) in self.nodes.iter().enumerate() {
            // Convert NodeStyle to NodeConfig for API
            ng.push_node_styled(
                index,
                *position,
                styled_node(name, style, theme),
                style.clone().into(),
            );
        }

        for (from, to) in &self.edges {
            ng.push_edge(*from, *to);
        }

        ng.into()
    }
}
