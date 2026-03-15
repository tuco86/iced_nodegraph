//! # 500 Node Benchmark Demo
//!
//! Large-scale node graph demonstrating performance with 500+ nodes.
//! Simulates a procedural shader/material graph with multiple processing stages.
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
//! - **Scroll** - Zoom in/out (zoom out to see all 500 nodes)
//! - **Middle-drag** - Pan the canvas
//! - **Drag nodes** - Move individual nodes
//!
//! ## About This Benchmark
//!
//! This demo generates a procedural shader graph with 500 nodes arranged in stages:
//! input sources, noise generators, vector operations, math operations,
//! texture sampling, blending, and material outputs.

mod graph;
mod nodes;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

use graph::generate_procedural_graph;
use iced::{
    Color, Length, Point, Subscription, Theme, Vector,
    widget::{checkbox, column, container, stack, text},
    window,
};
use iced_nodegraph::{PinRef, SdfDebug, node_graph};
use nodes::NodeType;
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
        .title("500 Node Benchmark - iced_nodegraph")
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
    Tick,
    ToggleDebugEdges,
    ToggleDebugShadows,
    ToggleDebugFill,
    ToggleDebugForeground,
}

struct Application {
    edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    nodes: Vec<(Point, NodeType)>,
    current_theme: Theme,
    selected_nodes: HashSet<usize>,
    sdf_debug: SdfDebug,
}

impl Default for Application {
    fn default() -> Self {
        let (nodes, edges) = generate_procedural_graph();
        Self {
            edges,
            nodes,
            current_theme: Theme::CatppuccinMocha,
            selected_nodes: HashSet::new(),
            sdf_debug: SdfDebug::default(),
        }
    }
}

impl Application {
    fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, message: ApplicationMessage) {
        match message {
            ApplicationMessage::Noop => (),
            ApplicationMessage::EdgeConnected { from, to } => {
                self.edges.push((from, to));
            }
            ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(node_index) {
                    *position = new_position;
                }
            }
            ApplicationMessage::EdgeDisconnected { from, to } => {
                self.edges.retain(|(f, t)| !(f == &from && t == &to));
            }
            ApplicationMessage::SelectionChanged(indices) => {
                self.selected_nodes = indices.into_iter().collect();
            }
            ApplicationMessage::GroupMoved { indices, delta } => {
                for idx in indices {
                    if let Some((pos, _)) = self.nodes.get_mut(idx) {
                        pos.x += delta.x;
                        pos.y += delta.y;
                    }
                }
            }
            ApplicationMessage::Tick => {}
            ApplicationMessage::ToggleDebugEdges => self.sdf_debug.edges = !self.sdf_debug.edges,
            ApplicationMessage::ToggleDebugShadows => self.sdf_debug.shadows = !self.sdf_debug.shadows,
            ApplicationMessage::ToggleDebugFill => self.sdf_debug.node_fill = !self.sdf_debug.node_fill,
            ApplicationMessage::ToggleDebugForeground => self.sdf_debug.node_foreground = !self.sdf_debug.node_foreground,
        }
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
        let mut ng = node_graph()
            .on_connect(|from, to| ApplicationMessage::EdgeConnected { from, to })
            .on_disconnect(|from, to| ApplicationMessage::EdgeDisconnected { from, to })
            .on_move(|node_index, new_position| ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            })
            .on_select(ApplicationMessage::SelectionChanged)
            .on_group_move(|indices, delta| ApplicationMessage::GroupMoved { indices, delta })
            .selection(&self.selected_nodes)
            .sdf_debug(self.sdf_debug);

        // Add all nodes
        for (index, (position, node_type)) in self.nodes.iter().enumerate() {
            ng.push_node(index, *position, node_type.create_node(&self.current_theme));
        }

        // Add all edges
        for (from, to) in &self.edges {
            ng.push_edge(*from, *to);
        }

        // Add stats overlay with SDF pipeline metrics
        let sdf = iced_sdf::sdf_stats();
        let stats = container(
            column![
                text(format!("Nodes: {}", self.nodes.len())).size(14),
                text(format!("Edges: {}", self.edges.len())).size(14),
                text(format!(
                    "SDF: {} shapes, {} tiles, {}us CPU",
                    sdf.shape_count, sdf.tile_count, sdf.prepare_cpu_us
                ))
                .size(12),
                text("Scroll: Zoom | Middle-drag: Pan").size(12),
                text("Tile Debug").size(12),
                checkbox(self.sdf_debug.edges)
                    .label("Edges")
                    .on_toggle(|_| ApplicationMessage::ToggleDebugEdges)
                    .size(14)
                    .text_size(12),
                checkbox(self.sdf_debug.shadows)
                    .label("Shadows")
                    .on_toggle(|_| ApplicationMessage::ToggleDebugShadows)
                    .size(14)
                    .text_size(12),
                checkbox(self.sdf_debug.node_fill)
                    .label("Node Fill")
                    .on_toggle(|_| ApplicationMessage::ToggleDebugFill)
                    .size(14)
                    .text_size(12),
                checkbox(self.sdf_debug.node_foreground)
                    .label("Foreground")
                    .on_toggle(|_| ApplicationMessage::ToggleDebugForeground)
                    .size(14)
                    .text_size(12),
            ]
            .spacing(4)
            .padding(10),
        )
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    palette.background.base.color.r,
                    palette.background.base.color.g,
                    palette.background.base.color.b,
                    0.9,
                ))),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            }
        });

        let graph_view: iced::Element<'_, ApplicationMessage> = ng.into();

        stack![
            graph_view,
            container(stats)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(10)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Top)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        // Enable continuous animation for NodeGraph animations
        Subscription::batch(vec![window::frames().map(|_| ApplicationMessage::Tick)])
    }
}
