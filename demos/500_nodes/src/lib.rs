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
//! - **Right-drag** - Pan the canvas
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
    Color, Element, Length, Point, Rectangle, Subscription, Theme, Vector, mouse,
    widget::{canvas, checkbox, column, container, opaque, row, stack, text},
    window,
};
use iced_nodegraph::{
    Counts, GraphInfo, PinInfo, PinRef, PinStatus, PinStyle, SdfDebug, default_pin_style, edge,
    node,
};
use nodes::NodeType;

/// Colors a node's pins by their data-type marker.
fn pin_style(
    theme: &iced::Theme,
    pin: &PinInfo<'_, usize, ::std::any::TypeId>,
    _other: Option<&PinInfo<'_, usize, ::std::any::TypeId>>,
    status: PinStatus,
) -> PinStyle {
    use nodes::colors;
    use std::any::TypeId;
    let ty = *pin.info();
    let color = if ty == TypeId::of::<colors::Float>() {
        colors::PIN_FLOAT
    } else if ty == TypeId::of::<colors::Vec2>() {
        colors::PIN_VEC2
    } else if ty == TypeId::of::<colors::Vec3>() {
        colors::PIN_VEC3
    } else if ty == TypeId::of::<colors::Vec4>() {
        colors::PIN_VEC4
    } else {
        colors::PIN_GENERIC_IN
    };
    PinStyle {
        color: color.into(),
        ..default_pin_style(theme, status)
    }
}
use std::collections::{HashSet, VecDeque};

/// How many recent frames the live timing chart keeps.
const HIST_CAP: usize = 160;

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
    EdgeDisconnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    SelectionChanged(Vec<usize>),
    NodesMoved {
        delta: Vector,
        indices: Vec<usize>,
    },
    Tick,
    ToggleDebugEdges,
    ToggleDebugShadows,
    ToggleDebugFill,
    ToggleDebugForeground,
    Info(GraphInfo),
}

struct Application {
    edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    nodes: Vec<(Point, NodeType)>,
    current_theme: Theme,
    selected_nodes: HashSet<usize>,
    sdf_debug: SdfDebug,
    /// Most recent per-frame diagnostics from the graph widget.
    latest_info: Option<GraphInfo>,
    /// Per-op CPU time (microseconds) for the last `HIST_CAP` frames, oldest
    /// first. Each entry mirrors `GraphInfo::timings` order.
    history: VecDeque<Vec<f32>>,
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
            latest_info: None,
            history: VecDeque::with_capacity(HIST_CAP),
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
            ApplicationMessage::EdgeDisconnected { from, to } => {
                self.edges.retain(|(f, t)| !(f == &from && t == &to));
            }
            ApplicationMessage::SelectionChanged(indices) => {
                self.selected_nodes = indices.into_iter().collect();
            }
            ApplicationMessage::NodesMoved { delta, indices } => {
                for idx in indices {
                    if let Some((pos, _)) = self.nodes.get_mut(idx) {
                        pos.x += delta.x;
                        pos.y += delta.y;
                    }
                }
            }
            ApplicationMessage::Tick => {}
            ApplicationMessage::ToggleDebugEdges => self.sdf_debug.edges = !self.sdf_debug.edges,
            ApplicationMessage::ToggleDebugShadows => {
                self.sdf_debug.shadows = !self.sdf_debug.shadows
            }
            ApplicationMessage::ToggleDebugFill => {
                self.sdf_debug.node_fill = !self.sdf_debug.node_fill
            }
            ApplicationMessage::ToggleDebugForeground => {
                self.sdf_debug.node_foreground = !self.sdf_debug.node_foreground
            }
            ApplicationMessage::Info(info) => {
                let frame: Vec<f32> = info
                    .timings
                    .iter()
                    .map(|t| t.duration.as_secs_f32() * 1_000_000.0)
                    .collect();
                if self.history.len() == HIST_CAP {
                    self.history.pop_front();
                }
                self.history.push_back(frame);
                self.latest_info = Some(info);
            }
        }
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
        let mut ng: ::iced_nodegraph::NodeGraph<usize, usize, ::std::any::TypeId, _, _, _> =
            ::iced_nodegraph::NodeGraph::default()
                .on_connect(|from, to| ApplicationMessage::EdgeConnected { from, to })
                .on_disconnect(|from, to| ApplicationMessage::EdgeDisconnected { from, to })
                .on_move(|delta, indices| ApplicationMessage::NodesMoved { delta, indices })
                .on_select(ApplicationMessage::SelectionChanged)
                .selection(&self.selected_nodes)
                .sdf_debug(self.sdf_debug)
                .info(ApplicationMessage::Info);

        // Add all nodes
        for (index, (position, node_type)) in self.nodes.iter().enumerate() {
            ng.push_node(
                node(index, *position, node_type.create_node(&self.current_theme))
                    .pin_style(pin_style),
            );
        }

        // Add all edges
        for (from, to) in &self.edges {
            ng.push_edge(edge!(*from, *to));
        }

        let stats = self.stats_panel();

        let graph_view: iced::Element<'_, ApplicationMessage> = ng.into();

        // `opaque` ensures the stats panel claims wheel/click events for its
        // own area so the NodeGraph below doesn't react through it.
        stack![
            graph_view,
            container(opaque(stats))
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

    fn stats_panel(&self) -> Element<'_, ApplicationMessage> {
        let palette = self.current_theme.extended_palette();

        let counts_line = |label: &str, c: Counts| {
            text(format!(
                "{label}: {}  ({} in view, {} culled)",
                c.total, c.in_view, c.culled
            ))
            .size(12)
        };

        let (nodes_c, pins_c, edges_c, entries, tiles) = match &self.latest_info {
            Some(i) => (i.nodes, i.pins, i.edges, i.sdf_entries, i.sdf_tiles),
            None => (
                Counts::default(),
                Counts::default(),
                Counts::default(),
                0,
                0,
            ),
        };

        // Stack and legend follow execution order (the order ops run each frame:
        // geometry, shadows, edges, foreground, sdf_prepare).
        let ops = self.latest_info.as_ref().map_or(0, |i| i.timings.len());
        let order: Vec<usize> = (0..ops).collect();

        let legend: Element<'_, ApplicationMessage> = match &self.latest_info {
            Some(info) if !order.is_empty() => column(order.iter().map(|&k| {
                let t = &info.timings[k];
                let us = t.duration.as_secs_f32() * 1_000_000.0;
                row![
                    swatch(op_color(palette, k)),
                    text(format!("{us:>5.0} µs   {}", t.label)).size(11),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into()
            }))
            .spacing(3)
            .into(),
            _ => text("collecting…").size(11).into(),
        };

        let chart = canvas(TimingChart {
            history: &self.history,
            order,
        })
        .width(Length::Fill)
        .height(Length::Fixed(110.0));

        let debug = column![
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
        .spacing(4);

        let body = column![
            text("Frame CPU — stacked by operation").size(13),
            chart,
            legend,
            counts_line("Nodes", nodes_c),
            counts_line("Pins", pins_c),
            counts_line("Edges", edges_c),
            text(format!("SDF: {entries} entries · {tiles} tiles")).size(12),
            text("Scroll: Zoom   ·   Right-drag: Pan").size(11),
            debug,
        ]
        .spacing(8)
        .padding(12)
        .width(Length::Fixed(248.0));

        container(body)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                let bg = palette.background.base.color;
                container::Style {
                    background: Some(iced::Background::Color(Color { a: 0.92, ..bg })),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..container::Style::default()
                }
            })
            .into()
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        // Enable continuous animation for NodeGraph animations
        Subscription::batch(vec![window::frames().map(|_| ApplicationMessage::Tick)])
    }
}

/// Palette color for stacked-timing op `i`, from the theme's extended palette.
fn op_color(palette: &iced::theme::palette::Extended, i: usize) -> Color {
    match i {
        0 => palette.primary.base.color,
        1 => palette.secondary.base.color,
        2 => palette.success.base.color,
        3 => palette.danger.base.color,
        _ => palette.background.strong.color,
    }
}

/// A small color swatch for the legend.
fn swatch(color: Color) -> Element<'static, ApplicationMessage> {
    container(text(""))
        .width(Length::Fixed(12.0))
        .height(Length::Fixed(12.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced::Background::Color(color)),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// Live stacked-area chart of per-operation CPU time over recent frames.
struct TimingChart<'a> {
    history: &'a VecDeque<Vec<f32>>,
    /// Op indices bottom-to-top; execution order (geometry first, at the base).
    order: Vec<usize>,
}

impl canvas::Program<ApplicationMessage, Theme> for TimingChart<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let n = self.history.len();
        if n == 0 {
            return vec![frame.into_geometry()];
        }
        let palette = theme.extended_palette();
        let max_total = self
            .history
            .iter()
            .map(|f| f.iter().sum::<f32>())
            .fold(1.0_f32, f32::max);
        let w = bounds.width;
        let h = bounds.height;
        let dx = w / HIST_CAP as f32;
        let x_of = |i: usize| w - (n - 1 - i) as f32 * dx;
        let y_of = |v: f32| h - (v / max_total) * h;

        // Stacked areas, ordered with the largest-average op at the base.
        let cum = |vals: &[f32], upto: usize| -> f32 {
            self.order[..upto.min(self.order.len())]
                .iter()
                .map(|&j| vals.get(j).copied().unwrap_or(0.0))
                .sum()
        };
        for (p, &k) in self.order.iter().enumerate() {
            let path = canvas::Path::new(|b| {
                let mut started = false;
                for i in 0..n {
                    let pt = iced::Point::new(x_of(i), y_of(cum(&self.history[i], p + 1)));
                    if started {
                        b.line_to(pt);
                    } else {
                        b.move_to(pt);
                        started = true;
                    }
                }
                for i in (0..n).rev() {
                    b.line_to(iced::Point::new(x_of(i), y_of(cum(&self.history[i], p))));
                }
                b.close();
            });
            frame.fill(&path, op_color(palette, k));
        }
        vec![frame.into_geometry()]
    }
}
