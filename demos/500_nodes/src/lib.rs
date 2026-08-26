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
//! - **Stats toggle** (top right) - show/hide the live timing panel; while
//!   hidden the demo renders no frames between interactions
//!
//! ## About This Benchmark
//!
//! This demo generates a procedural shader graph with 500 nodes arranged in stages:
//! input sources, noise generators, vector operations, math operations,
//! texture sampling, blending, and material outputs.

mod graph;
mod nodes;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

use graph::generate_procedural_graph;
use iced::{
    Color, Element, Length, Point, Rectangle, Subscription, Theme, Vector, mouse,
    widget::{canvas, column, container, opaque, row, stack, text, toggler},
};
use iced_nodegraph::{
    Counts, GraphInfo, GraphStyle, PinInfo, PinRef, PinStatus, PinStyle, default_pin_style, edge,
    node,
};
use nodes::NodeType;
use web_time::Instant;

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

/// How many recent frame intervals the `NG_REPORT` line summarises.
const INTERVAL_CAP: usize = 120;
/// Frames between `NG_REPORT` lines.
const REPORT_EVERY: u32 = 60;

/// Reads an environment knob, `None` when unset or unparseable. Always `None`
/// on wasm (`std::env` is empty there), so the browser build keeps the defaults.
fn env_var<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.parse().ok()
}

/// True when the knob is set to `1`.
fn env_flag(key: &str) -> bool {
    env_var::<u32>(key) == Some(1)
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

    // `NG_SCALE` is the fragment-count axis: physical pixels - and with them
    // the SDF pipeline's fragment work - scale with `scale^2`. iced's own
    // default is 1.0, so an unset knob leaves behaviour unchanged.
    let scale: f32 = env_var("NG_SCALE").unwrap_or(1.0);
    iced::application(Application::new, Application::update, Application::view)
        .subscription(Application::subscription)
        .title("500 Node Benchmark - iced_nodegraph")
        .theme(Application::theme)
        .window(window_settings)
        .scale_factor(move |_| scale)
        .run()
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn run_demo() {
    let _ = main();
}

#[derive(Debug, Clone)]
enum ApplicationMessage {
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
    Info(GraphInfo),
    /// Stats-panel toggle (top right). Hiding the panel also drops the
    /// `on_info` subscription, so the demo stops rendering while idle.
    StatsToggled(bool),
    /// Camera reported by the widget on pan/zoom release (uncontrolled camera).
    /// Used only to read off exact coordinates when reproducing the pan/zoom
    /// float-collapse display bug.
    CameraReport {
        pos: Point,
        zoom: f32,
    },
}

struct Application {
    edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    nodes: Vec<(Point, NodeType)>,
    current_theme: Theme,
    selected_nodes: HashSet<usize>,
    /// Last camera (world position, zoom) reported by the widget on pan/zoom
    /// release. Shown in the stats panel to capture float-collapse repro coords.
    camera: (Point, f32),
    /// Most recent per-frame diagnostics from the graph widget.
    latest_info: Option<GraphInfo>,
    /// Per-op CPU time (microseconds) for the last `HIST_CAP` frames, oldest
    /// first. Each entry mirrors `GraphInfo::timings` order.
    history: VecDeque<Vec<f32>>,
    /// Whether the live stats panel (and with it the `on_info`-driven frame
    /// stream) is active. Off = the demo only redraws on interaction.
    stats_visible: bool,
    /// `NG_NO_GRID=1`: drop the tiling background, removing one full-canvas
    /// SDF layer. Set once at startup.
    no_grid: bool,
    /// `NG_REPORT=1`: force the index probe on and print a report line every
    /// [`REPORT_EVERY`] frames.
    report: bool,
    /// Wall clock of the previous [`ApplicationMessage::Info`], for intervals.
    last_frame: Option<Instant>,
    /// Recent frame intervals in milliseconds, oldest first.
    intervals: VecDeque<f32>,
    /// Frames since the last printed report line.
    since_report: u32,
    /// Report lines printed so far; 0 prints the header.
    reports: u32,
}

impl Default for Application {
    fn default() -> Self {
        let (mut nodes, mut edges) = generate_procedural_graph();
        // `NG_NODES` truncates rather than reconfiguring the generator (whose
        // stage sizes are hardcoded loop bounds): keep the first N nodes and
        // drop every edge that referenced a dropped one.
        if let Some(n) = env_var::<usize>("NG_NODES") {
            nodes.truncate(n);
            edges.retain(|(from, to)| from.node_id < n && to.node_id < n);
        }
        if env_flag("NG_NO_EDGES") {
            edges.clear();
        }
        let report = env_flag("NG_REPORT");
        let stats_visible = true;
        // The fine-slot readback costs 4 bytes per fine tile per culled frame,
        // so it is only armed while something is actually reading it.
        iced_nodegraph_sdf::set_index_probe(stats_visible || report);
        Self {
            edges,
            nodes,
            current_theme: Theme::CatppuccinMocha,
            selected_nodes: HashSet::new(),
            camera: (Point::ORIGIN, 1.0),
            latest_info: None,
            history: VecDeque::with_capacity(HIST_CAP),
            stats_visible,
            no_grid: env_flag("NG_NO_GRID"),
            report,
            last_frame: None,
            intervals: VecDeque::with_capacity(INTERVAL_CAP),
            since_report: 0,
            reports: 0,
        }
    }
}

impl Application {
    fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, message: ApplicationMessage) {
        match message {
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
            ApplicationMessage::CameraReport { pos, zoom } => {
                self.camera = (pos, zoom);
            }
            ApplicationMessage::StatsToggled(on) => {
                self.stats_visible = on;
                iced_nodegraph_sdf::set_index_probe(on || self.report);
                if !on {
                    // Drop stale data so a re-enabled panel starts fresh
                    // instead of presenting an old chart as current.
                    self.latest_info = None;
                    self.history.clear();
                }
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
                let now = Instant::now();
                if let Some(prev) = self.last_frame.replace(now) {
                    if self.intervals.len() == INTERVAL_CAP {
                        self.intervals.pop_front();
                    }
                    self.intervals
                        .push_back(now.duration_since(prev).as_secs_f32() * 1000.0);
                }
                self.latest_info = Some(info);
                if self.report {
                    self.since_report += 1;
                    if self.since_report >= REPORT_EVERY {
                        self.since_report = 0;
                        self.print_report();
                    }
                }
            }
        }
    }

    /// Prints one `NG_REPORT` line: the frame-interval summary plus the GPU
    /// work and memory counters from [`GraphInfo`].
    ///
    /// Intervals are VSYNC-CAPPED, so the absolute value is meaningless while
    /// the renderer keeps up. The signal is the configuration at which the
    /// interval LEAVES the vsync floor, and how it grows past it.
    fn print_report(&mut self) {
        let Some(i) = self.latest_info.as_ref() else {
            return;
        };
        if self.reports == 0 {
            println!(
                "NG_REPORT: frame intervals are vsync-capped - read the point at which \
                 mean/p95 leave the vsync floor, not the absolute value."
            );
            println!(
                "frames  mean ms  p95 ms  draws  shaded Mpx  evals M  fine max  dropped  \
                 gpu MiB  index MiB  upload KiB  traffic KiB  cull_skipped"
            );
        }
        let mut sorted: Vec<f32> = self.intervals.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let n = sorted.len();
        let mean = if n == 0 {
            0.0
        } else {
            sorted.iter().sum::<f32>() / n as f32
        };
        let p95 = sorted
            .get((n as f32 * 0.95) as usize)
            .or(sorted.last())
            .copied()
            .unwrap_or(0.0);
        const MIB: f64 = 1024.0 * 1024.0;
        println!(
            "{n:>6}  {mean:>7.2}  {p95:>6.2}  {:>5}  {:>10.2}  {:>7.2}  {:>8}  {:>7}  \
             {:>7.2}  {:>9.2}  {:>10.1}  {:>11.1}  {}",
            i.sdf_draws,
            i.sdf_shaded_px as f64 / 1e6,
            i.sdf_segment_evals as f64 / 1e6,
            i.sdf_fine_slots_max,
            i.sdf_fine_evicted_tiles,
            i.sdf_gpu_bytes as f64 / MIB,
            i.sdf_index_bytes as f64 / MIB,
            i.sdf_upload_bytes as f64 / 1024.0,
            i.sdf_index_traffic_bytes as f64 / 1024.0,
            i.sdf_cull_skipped,
        );
        self.reports += 1;
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
        let mut ng: ::iced_nodegraph::NodeGraph<usize, usize, (), usize, ::std::any::TypeId, _, _> =
            ::iced_nodegraph::NodeGraph::default()
                .on_connect(|from, to| ApplicationMessage::EdgeConnected { from, to })
                .on_disconnect(|from, to| ApplicationMessage::EdgeDisconnected { from, to })
                .on_move(|delta, indices| ApplicationMessage::NodesMoved { delta, indices })
                .on_select(ApplicationMessage::SelectionChanged)
                .on_pan(|pos, zoom| ApplicationMessage::CameraReport { pos, zoom });
        // The `on_info` frame stream exists only while the stats panel is
        // shown: live per-frame diagnostics force continuous redraws, so with
        // the panel hidden the demo is fully idle between interactions.
        if self.stats_visible {
            ng = ng.on_info(ApplicationMessage::Info);
        }
        // `NG_NO_GRID=1` removes the tiling layer the theme default carries,
        // dropping one full-canvas SDF draw from every frame.
        if self.no_grid {
            ng = ng.graph_style(|theme| GraphStyle {
                tiling: None,
                ..GraphStyle::from_theme(theme)
            });
        }

        // Add all nodes
        for (index, (position, node_type)) in self.nodes.iter().enumerate() {
            ng.push_node(
                node(index, *position, node_type.create_node(&self.current_theme))
                    .selected(self.selected_nodes.contains(&index))
                    .pin_style(pin_style),
            );
        }

        // Add all edges
        for (from, to) in &self.edges {
            ng.push_edge(edge!(*from, *to));
        }

        // Top-right overlay: the toggle chip, plus the stats panel while shown.
        // `opaque` ensures the overlay claims wheel/click events for its own
        // area so the NodeGraph below doesn't react through it.
        let toggle = container(
            toggler(self.stats_visible)
                .label("stats")
                .size(16.0)
                .text_size(11)
                .on_toggle(ApplicationMessage::StatsToggled),
        )
        .style(panel_style)
        .padding([4, 10]);

        let mut overlay = column![toggle].spacing(8).align_x(iced::Alignment::End);
        if self.stats_visible {
            overlay = overlay.push(self.stats_panel());
        }

        let graph_view: iced::Element<'_, ApplicationMessage> = ng.into();

        stack![
            graph_view,
            container(opaque(overlay))
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

        let info = self.latest_info.as_ref();
        let (nodes_c, pins_c, edges_c, entries, tiles) = match info {
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

        let body = column![
            text("Frame CPU — stacked by operation").size(13),
            chart,
            legend,
            counts_line("Nodes", nodes_c),
            counts_line("Pins", pins_c),
            counts_line("Edges", edges_c),
            text(format!("SDF: {entries} entries · {tiles} tiles")).size(12),
            gpu_rows(info),
            text(format!(
                "cam: ({:.1}, {:.1})  zoom: {:.5}",
                self.camera.0.x, self.camera.0.y, self.camera.1
            ))
            .size(12),
            text("Scroll: Zoom   ·   Right-drag: Pan").size(11),
        ]
        .spacing(8)
        .padding(12)
        .width(Length::Fixed(248.0));

        container(body).style(panel_style).into()
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        // The widget self-drives redraws while anything animates; no frame clock needed.
        Subscription::none()
    }
}

/// The GPU work/memory block of the stats panel: the counters that bound SDF
/// GPU cost. `evals` and `fine max` need the index probe, which the panel arms
/// while it is visible; `upload` is the RAM->GPU traffic the "shared VRAM
/// bandwidth" hypothesis predicts, and is 0 on a fully resident idle frame.
fn gpu_rows(info: Option<&GraphInfo>) -> Element<'_, ApplicationMessage> {
    let Some(i) = info else {
        return text("GPU: collecting…").size(12).into();
    };
    const MIB: f64 = 1024.0 * 1024.0;
    let row = |s: String| text(s).size(11).into();
    column(
        [
            format!(
                "draws {}   shaded {:.2} Mpx",
                i.sdf_draws,
                i.sdf_shaded_px as f64 / 1e6
            ),
            format!(
                "evals {:.2} M   fine max {}/64",
                i.sdf_segment_evals as f64 / 1e6,
                i.sdf_fine_slots_max
            ),
            format!("dropped tiles {}", i.sdf_fine_evicted_tiles),
            format!(
                "gpu {:.2} MiB   index {:.2} MiB",
                i.sdf_gpu_bytes as f64 / MIB,
                i.sdf_index_bytes as f64 / MIB
            ),
            format!(
                "upload {:.1} KiB   traffic {:.1} KiB",
                i.sdf_upload_bytes as f64 / 1024.0,
                i.sdf_index_traffic_bytes as f64 / 1024.0
            ),
            format!("cull skipped: {}", i.sdf_cull_skipped),
        ]
        .map(row),
    )
    .spacing(2)
    .into()
}

/// Translucent chip/panel background shared by the stats panel and its toggle.
fn panel_style(theme: &Theme) -> container::Style {
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
