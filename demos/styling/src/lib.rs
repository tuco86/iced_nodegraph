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
//! - **Right-drag** - Pan the canvas
//!
//! ## Routing anchors
//!
//! Two anchors sit under the Transform node, with three cables on each. Two of
//! those cables come from different inputs and run through both anchors, and the
//! angular interval each subtends contains the other's at one anchor and is
//! contained by it at the other. Taking each anchor on its own therefore seats
//! the two in opposite orders, which puts them on opposite sides of the stretch
//! between the anchors and crosses them there. It does not happen: candidate
//! ring orders are measured and the one that crosses least is kept, so a cable
//! that flies the stretch between two anchors keeps to its own side of it.
//! Clearing it moves the third cable at the first anchor off the innermost ring
//! its interval asks for, out to the widest of the three. That is a consequence
//! of the exchange rather than a price weighed against it: only the pair flying
//! the stretch is built and counted, so nothing measured the third cable at all.
//! The whole lifecycle is host logic:
//!
//! - **Drag a cable mid-run** - places a new anchor where you release
//! - **Drag a cable where it wraps** - pulls it off that anchor
//! - **Drop either drag on an anchor** - attaches during the drag, and every
//!   cable at that anchor is reseated, the newcomer included; it can land on any
//!   ring, the innermost among them
//! - **Right-click an anchor core** - deletes it and strips it from every route
//! - **Right-click a wrap** - detaches that one cable
//! - **Drag an anchor core** - moves it
//!
//! An anchor whose last cable leaves is dropped: nothing in the library says
//! so, it is what `update` below does with the routes it owns.

mod nodes;

use iced::{
    Element, Length, Point, Subscription, Task, Theme, Vector,
    widget::{button, column, container, opaque, pick_list, row, slider, stack, text},
};
use iced_nodegraph::{
    GraphStyle, NodeStatus, NodeStyle, Pattern, PinDirection, PinInfo, PinRef, PinStatus, PinStyle,
    TilingBackground, anchor, default_node_style, default_pin_style, edge, node,
};
use nodes::styled_node;
use std::collections::HashSet;

/// Pin style for the styling demo: blue inputs, orange outputs.
fn styling_pin_style(
    theme: &Theme,
    pin: &PinInfo<'_, usize, ::std::any::TypeId>,
    _other: Option<&PinInfo<'_, usize, ::std::any::TypeId>>,
    status: PinStatus,
) -> PinStyle {
    let color = match pin.direction() {
        PinDirection::Output => iced::Color::from_rgb(0.9, 0.7, 0.5),
        _ => iced::Color::from_rgb(0.5, 0.7, 0.9),
    };
    PinStyle {
        color: color.into(),
        ..default_pin_style(theme, status)
    }
}

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
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

#[cfg(feature = "wasm")]
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
    SelectionChanged(Vec<usize>),
    NodesMoved {
        delta: Vector,
        indices: Vec<usize>,
    },

    // Routing anchors
    AnchorCreated {
        edge: usize,
        position: Point,
    },
    AnchorMoved {
        anchor: usize,
        position: Point,
    },
    AnchorDeleted(usize),
    RouteAttached {
        edge: usize,
        anchor: usize,
    },
    RouteDetached {
        edge: usize,
        anchor: usize,
    },
    /// Every gesture that can orphan an anchor ends here, which is when one no
    /// cable names any more is safe to drop: during a drag it is still a target
    /// the same drag may put the cable back onto.
    DragEnded,

    // Style controls
    CornerRadiusChanged(f32),
    OpacityChanged(f32),
    BorderWidthChanged(f32),
    SelectNode(usize),
    ApplyPreset(NodePreset),
    ChangeTheme(Theme),
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

/// One edge the host owns: a minted id, its two pins, and the anchors it wraps.
///
/// The route is a set of anchor ids in no particular order - the widget derives
/// which way round the cable meets them - so `update` only ever adds to it or
/// removes from it.
///
/// The id is minted rather than taken from the edge's position in `edges`,
/// because a position is not a name: one `view` produces a BATCH of messages
/// and the runtime applies them in order, so an earlier `EdgeDisconnected`
/// shifts every later index and a `RouteAttached` behind it would land on the
/// wrong cable.
#[derive(Debug, Clone)]
struct EdgeModel {
    id: usize,
    from: PinRef<usize, usize>,
    to: PinRef<usize, usize>,
    route: Vec<usize>,
}

struct Application {
    edges: Vec<EdgeModel>,
    /// The next edge id to mint, on the same never-reused footing as
    /// `next_anchor`.
    next_edge: usize,
    /// Anchor id paired with its world position.
    anchors: Vec<(usize, Point)>,
    /// The next anchor id to mint. Only ever grows, so a deleted anchor's id is
    /// never handed to a different anchor.
    next_anchor: usize,
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
                // Input -> Transform, dipping under the run to wrap anchor A.
                EdgeModel {
                    id: 0,
                    from: PinRef::new(0, 1),
                    to: PinRef::new(1, 0),
                    route: vec![0],
                },
                // Transform -> Output Result, wrapping anchor B.
                EdgeModel {
                    id: 1,
                    from: PinRef::new(1, 1),
                    to: PinRef::new(2, 0),
                    route: vec![1],
                },
                // Input -> the comment node, through both anchors.
                EdgeModel {
                    id: 2,
                    from: PinRef::new(0, 1),
                    to: PinRef::new(3, 0),
                    route: vec![0, 1],
                },
                // Aux Input -> Output Log, through both anchors as well, so the
                // stretch from A to B carries two cables. This pair is the point
                // of the scene: at A edge 3's angular interval is contained in
                // edge 2's and at B it is the other way round, so each anchor
                // taken alone would seat them in opposite orders and the two
                // would cross between A and B. They do not, and clearing that
                // moves edge 0 at A. Its interval is the smallest of A's three,
                // so containment seats it innermost, and the exchange that
                // clears the corridor puts it on the widest of the three rings
                // instead. Edge 0 wraps one anchor, so it is never one of the
                // cables a candidate builds and counts: its ring is fallout from
                // the pair's fix, not a cost weighed against it. Edge 1 is the
                // third cable at B.
                EdgeModel {
                    id: 3,
                    from: PinRef::new(5, 1),
                    to: PinRef::new(4, 0),
                    route: vec![0, 1],
                },
            ],
            next_edge: 4,
            // Each anchor sits ON the run of the cables that wrap it: every
            // cable here meets its anchors in increasing x, between its two
            // pins. An anchor placed off the far end of a run still works - the
            // visiting order is a projection onto the run, so it resolves - but
            // the cable has to double back to reach it.
            anchors: vec![(0, Point::new(300.0, 300.0)), (1, Point::new(550.0, 300.0))],
            next_anchor: 2,
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
                    Point::new(600.0, 420.0),
                    "Note: This is a comment".to_string(),
                    NodeStyle::comment(),
                ),
                (
                    Point::new(600.0, 280.0),
                    "Output Log".to_string(),
                    NodeStyle::output(),
                ),
                (
                    Point::new(120.0, 60.0),
                    "Aux Input".to_string(),
                    NodeStyle::input(),
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
        Subscription::none()
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EdgeConnected { from, to } => {
                let id = self.next_edge;
                self.next_edge += 1;
                self.edges.push(EdgeModel {
                    id,
                    from,
                    to,
                    route: Vec::new(),
                });
            }
            Message::EdgeDisconnected { from, to } => {
                self.edges.retain(|e| !(e.from == from && e.to == to));
                self.drop_unused_anchors();
            }
            Message::AnchorCreated { edge, position } => {
                let id = self.next_anchor;
                self.next_anchor += 1;
                // An anchor nothing wraps is not something this host keeps, so
                // it is only pushed once an edge has taken it. The id stays
                // spent either way and can never name a second anchor.
                if let Some(model) = self.edges.iter_mut().find(|e| e.id == edge) {
                    model.route.push(id);
                    self.anchors.push((id, position));
                }
            }
            Message::AnchorMoved { anchor, position } => {
                if let Some((_, p)) = self.anchors.iter_mut().find(|(id, _)| *id == anchor) {
                    *p = position;
                }
            }
            Message::AnchorDeleted(anchor) => {
                self.anchors.retain(|(id, _)| *id != anchor);
                for model in &mut self.edges {
                    model.route.retain(|id| *id != anchor);
                }
            }
            Message::RouteAttached { edge, anchor } => {
                if let Some(model) = self.edges.iter_mut().find(|e| e.id == edge)
                    && !model.route.contains(&anchor)
                {
                    model.route.push(anchor);
                }
            }
            Message::RouteDetached { edge, anchor } => {
                if let Some(model) = self.edges.iter_mut().find(|e| e.id == edge) {
                    model.route.retain(|id| *id != anchor);
                }
                // No collection here: a detach fires DURING the drag, and the
                // anchor just left is exactly the one the drag may put the
                // cable straight back onto. Dropping it now would delete a live
                // drop target mid-gesture.
            }
            Message::DragEnded => self.drop_unused_anchors(),
            Message::SelectionChanged(indices) => {
                self.graph_selection = indices.into_iter().collect();
            }
            Message::NodesMoved { delta, indices } => {
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
                    self.border_width = style.border_pattern.thickness;
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
                        self.border_width = new_style.border_pattern.thickness;
                    }
                }
            }
            Message::ChangeTheme(theme) => {
                self.current_theme = theme;
            }
        }
        Task::none()
    }

    /// Drops every anchor no route names any more.
    ///
    /// The library keeps an anchor as long as the host pushes it, cables or no
    /// cables - so "the last cable left, the anchor goes" is a policy, and this
    /// is where this host states it.
    fn drop_unused_anchors(&mut self) {
        let routed: HashSet<usize> = self
            .edges
            .iter()
            .flat_map(|model| model.route.iter().copied())
            .collect();
        self.anchors.retain(|(id, _)| routed.contains(id));
    }

    fn apply_style_to_selected(&mut self) {
        if let Some(index) = self.selected_node
            && let Some((_, _, style)) = self.nodes.get_mut(index)
        {
            style.corner_radius = self.corner_radius;
            style.opacity = self.opacity;
            style.border_pattern = Pattern::solid(self.border_width);
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let control_panel = self.build_control_panel();
        let graph = self.build_graph();

        // Use stack to overlay control panel on right side of graph.
        // `opaque` claims mouse interaction for the menu's 280px strip so the
        // NodeGraph below doesn't receive wheel/click events that fall on it.
        // The outer Fill x Fill container is just for alignment and must stay
        // transparent.
        stack![
            graph,
            container(opaque(
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
            ))
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
                slider(0.1..=1.0, self.opacity, Message::OpacityChanged).step(0.05_f32),
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
                slider(0.5..=5.0, self.border_width, Message::BorderWidthChanged).step(0.5_f32),
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

        // The edge id parameter is `usize`: each edge carries a minted id, so
        // the anchor callbacks name one that survives a removal elsewhere.
        let mut ng: ::iced_nodegraph::NodeGraph<
            usize,
            usize,
            usize,
            usize,
            ::std::any::TypeId,
            _,
            _,
        > = ::iced_nodegraph::NodeGraph::default()
            .on_connect(|from, to| Message::EdgeConnected { from, to })
            .on_disconnect(|from, to| Message::EdgeDisconnected { from, to })
            .on_move(|delta, indices| Message::NodesMoved { delta, indices })
            .on_select(Message::SelectionChanged)
            .on_anchor_move(|anchor, position| Message::AnchorMoved { anchor, position })
            .on_anchor_create(|edge, position| Message::AnchorCreated { edge, position })
            .on_anchor_delete(Message::AnchorDeleted)
            .on_route_attach(|edge, anchor| Message::RouteAttached { edge, anchor })
            .on_route_detach(|edge, anchor| Message::RouteDetached { edge, anchor })
            .on_drag_end(|| Message::DragEnded)
            .graph_style(|theme: &Theme| {
                let line = theme.extended_palette().background.strong.color;
                GraphStyle {
                    tiling: Some(TilingBackground::grid(
                        40.0,
                        1.0,
                        iced::Color { a: 0.5, ..line },
                    )),
                    ..GraphStyle::from_theme(theme)
                }
            });

        for (index, (position, name, style)) in self.nodes.iter().enumerate() {
            // The demo stores a fully resolved style per node; the callback
            // returns it and layers the selection feedback on top when selected.
            let node_style = style.clone();
            ng.push_node(
                node(index, *position, styled_node(name, style, theme))
                    .selected(self.graph_selection.contains(&index))
                    .style(move |theme, status| {
                        let mut resolved = node_style.clone();
                        if status == NodeStatus::Selected {
                            // Take the library's selection feedback verbatim, so a
                            // hand-styled node highlights like every other node.
                            let sel = default_node_style(theme, status);
                            resolved.border_color = sel.border_color;
                            resolved.border_pattern = sel.border_pattern;
                            resolved.border_outline_width = sel.border_outline_width;
                            resolved.border_outline_color = sel.border_outline_color;
                            resolved.opacity = sel.opacity;
                        }
                        resolved
                    })
                    .pin_style(styling_pin_style),
            );
        }

        // Anchors before edges only for readability; the widget resolves ids
        // through its own lookups, so push order between the two is free.
        for &(id, position) in &self.anchors {
            ng.push_anchor(anchor(id, position));
        }

        for model in &self.edges {
            ng.push_edge(edge(model.from, model.to, model.id).route(model.route.iter().copied()));
        }

        ng.into()
    }
}
