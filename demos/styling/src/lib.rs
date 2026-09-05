#![doc = include_str!("../README.md")]
#![doc = r#"<link rel="stylesheet" href="../gallery/pkg/demo.css"><script type="module" src="../gallery/pkg/demo-loader.js"></script>"#]

mod nodes;

use demo_common::{Demo, NodeContentStyle, node_title_bar};
use iced::{
    Element, Length, Point, Rectangle, Size, Task, Theme, Vector,
    widget::{Space, button, column, container, opaque, pick_list, row, slider, stack, text},
};
use iced_nodegraph::{
    GraphStyle, Ids, NodeStatus, NodeStyle, Pattern, PinDirection, PinInfo, PinRef, PinStatus,
    PinStyle, TilingBackground, anchor, default_graph_style, default_pin_style, edge, node,
};
use nodes::styled_node;
use std::collections::HashSet;

/// Node id of the frame. Every other node is named by its index in `nodes`, so
/// a sentinel outside that space keeps the two apart in `on_move`.
const FRAME_NODE: usize = usize::MAX;

/// Id vocabulary of this demo: usize node, pin, edge and anchor ids, and a
/// `TypeId` pin payload naming the data kind a pin carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StylingIds;

impl Ids for StylingIds {
    type NodeId = usize;
    type PinId = usize;
    type EdgeId = usize;
    type AnchorId = usize;
    type Payload = ::std::any::TypeId;
}

/// Pin style for the styling demo: blue inputs, orange outputs.
fn styling_pin_style(
    theme: &Theme,
    pin: &PinInfo<'_, StylingIds>,
    _other: Option<&PinInfo<'_, StylingIds>>,
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

pub fn main() -> iced::Result {
    iced::application(Application::boot, Application::update, Application::view)
        .subscription(Application::subscription)
        .title("Styling Demo - iced_nodegraph")
        .theme(Application::theme)
        .window(iced::window::Settings::default())
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    // Graph events
    EdgeConnected {
        from: PinRef<StylingIds>,
        to: PinRef<StylingIds>,
    },
    EdgeDisconnected {
        from: PinRef<StylingIds>,
        to: PinRef<StylingIds>,
    },
    SelectionChanged(Vec<usize>),
    NodesMoved {
        delta: Vector,
        indices: Vec<usize>,
    },
    FrameResized(Size),

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

    /// The library preset this variant names, resolved for a theme and status.
    fn style(self, theme: &Theme, status: NodeStatus) -> NodeStyle {
        match self {
            NodePreset::Input => NodeStyle::input(theme, status),
            NodePreset::Process => NodeStyle::process(theme, status),
            NodePreset::Output => NodeStyle::output(theme, status),
            NodePreset::Comment => NodeStyle::comment(theme, status),
        }
    }
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
    from: PinRef<StylingIds>,
    to: PinRef<StylingIds>,
    route: Vec<usize>,
}

/// The style fields the sliders drive, held per node.
///
/// A preset is a function of theme and status, so a node keeps the preset it
/// was given plus these overrides and both resolve in the node's style
/// closure.
#[derive(Debug, Clone, Copy)]
struct NodeOverrides {
    corner_radius: f32,
    opacity: f32,
    border_width: f32,
}

impl NodeOverrides {
    /// The values every node starts on.
    const START: Self = Self {
        corner_radius: 5.0,
        opacity: 0.75,
        border_width: 1.5,
    };

    /// `base` with the slider-driven fields replaced. Only the border's
    /// thickness is a slider, so a preset's dash pattern survives.
    fn apply(self, base: NodeStyle) -> NodeStyle {
        NodeStyle {
            corner_radius: self.corner_radius,
            opacity: self.opacity,
            border_pattern: Pattern {
                thickness: self.border_width,
                ..base.border_pattern
            },
            ..base
        }
    }
}

/// One node the host owns: where it sits, what it is called, and the preset
/// plus overrides its style resolves from.
struct StyledNode {
    position: Point,
    name: String,
    preset: NodePreset,
    overrides: NodeOverrides,
}

impl StyledNode {
    fn new(position: Point, name: &str, preset: NodePreset) -> Self {
        Self {
            position,
            name: name.to_string(),
            preset,
            overrides: NodeOverrides::START,
        }
    }
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
    nodes: Vec<StyledNode>,
    current_theme: Theme,
    selected_node: Option<usize>,
    graph_selection: HashSet<usize>,
    /// The frame behind the two input nodes, in world units. Host state like
    /// every node's position: the widget only reports the drag and the grip.
    frame: Rectangle,
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
                StyledNode::new(Point::new(100.0, 150.0), "Input Data", NodePreset::Input),
                StyledNode::new(Point::new(350.0, 200.0), "Transform", NodePreset::Process),
                StyledNode::new(
                    Point::new(600.0, 150.0),
                    "Output Result",
                    NodePreset::Output,
                ),
                StyledNode::new(
                    Point::new(600.0, 420.0),
                    "Note: This is a comment",
                    NodePreset::Comment,
                ),
                StyledNode::new(Point::new(600.0, 280.0), "Output Log", NodePreset::Output),
                StyledNode::new(Point::new(120.0, 60.0), "Aux Input", NodePreset::Input),
            ],
            current_theme: Theme::CatppuccinFrappe,
            selected_node: Some(0),
            graph_selection: HashSet::new(),
            frame: Rectangle::new(Point::new(60.0, 20.0), Size::new(270.0, 270.0)),
        }
    }
}

impl Demo for Application {
    type Message = Message;

    fn boot() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn set_theme(&mut self, theme: Theme) {
        self.current_theme = theme;
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
                    if idx == FRAME_NODE {
                        self.frame.x += delta.x;
                        self.frame.y += delta.y;
                    } else if let Some(styled) = self.nodes.get_mut(idx) {
                        styled.position.x += delta.x;
                        styled.position.y += delta.y;
                    }
                }
            }
            // The frame is the only node carrying a grip, so the report can
            // only be about its size.
            Message::FrameResized(size) => {
                self.frame.width = size.width;
                self.frame.height = size.height;
            }
            Message::CornerRadiusChanged(value) => {
                if let Some(overrides) = self.selected_overrides_mut() {
                    overrides.corner_radius = value;
                }
            }
            Message::OpacityChanged(value) => {
                if let Some(overrides) = self.selected_overrides_mut() {
                    overrides.opacity = value;
                }
            }
            Message::BorderWidthChanged(value) => {
                if let Some(overrides) = self.selected_overrides_mut() {
                    overrides.border_width = value;
                }
            }
            Message::SelectNode(index) => {
                self.selected_node = Some(index);
            }
            Message::ApplyPreset(preset) => {
                if let Some(index) = self.selected_node
                    && let Some(styled) = self.nodes.get_mut(index)
                {
                    styled.preset = preset;
                }
            }
            Message::ChangeTheme(theme) => {
                self.current_theme = theme;
            }
        }
        Task::none()
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
}

/// Boots this demo for the gallery.
pub fn scene() -> (
    Box<dyn demo_common::Scene>,
    iced::Task<demo_common::SceneMessage>,
) {
    demo_common::erase::<Application>()
}

impl Application {
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

    /// The slider values of the selected node, or the starting values when no
    /// node is selected.
    fn selected_overrides(&self) -> NodeOverrides {
        self.selected_node
            .and_then(|index| self.nodes.get(index))
            .map(|styled| styled.overrides)
            .unwrap_or(NodeOverrides::START)
    }

    fn selected_overrides_mut(&mut self) -> Option<&mut NodeOverrides> {
        let index = self.selected_node?;
        self.nodes
            .get_mut(index)
            .map(|styled| &mut styled.overrides)
    }

    fn build_control_panel(&self) -> Element<'_, Message> {
        let theme = &self.current_theme;
        let palette = theme.extended_palette();
        let text_color = palette.background.base.text;

        let title = text("Style Controls").size(18).color(text_color);

        let selected = self.selected_node.and_then(|index| self.nodes.get(index));
        let selected_label = if let Some(styled) = selected {
            text(format!("Selected: {} ({})", styled.name, styled.preset))
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
                .map(|(i, styled)| {
                    let is_selected = self.selected_node == Some(i);
                    button(text(styled.name.clone()).size(12))
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

        // Style sliders, driving the selected node's overrides
        let overrides = self.selected_overrides();
        let corner_slider = column![
            text("Corner Radius").size(12).color(text_color),
            row![
                slider(
                    0.0..=20.0,
                    overrides.corner_radius,
                    Message::CornerRadiusChanged
                ),
                text(format!("{:.1}", overrides.corner_radius))
                    .size(12)
                    .color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let opacity_slider = column![
            text("Opacity").size(12).color(text_color),
            row![
                slider(0.1..=1.0, overrides.opacity, Message::OpacityChanged).step(0.05_f32),
                text(format!("{:.2}", overrides.opacity))
                    .size(12)
                    .color(text_color),
            ]
            .spacing(10),
        ]
        .spacing(4);

        let border_slider = column![
            text("Border Width").size(12).color(text_color),
            row![
                slider(
                    0.5..=5.0,
                    overrides.border_width,
                    Message::BorderWidthChanged
                )
                .step(0.5_f32),
                text(format!("{:.1}", overrides.border_width))
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

        // `StylingIds::EdgeId` is `usize`: each edge carries a minted id, so
        // the anchor callbacks name one that survives a removal elsewhere.
        ::iced_nodegraph::NodeGraph::<StylingIds, _, _, _>::new()
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
            .on_resize(|_, size| Message::FrameResized(size))
            .graph_style(|theme: &Theme| {
                let line = theme.extended_palette().background.strong.color;
                GraphStyle {
                    tiling: Some(TilingBackground::grid(
                        40.0,
                        1.0,
                        iced::Color { a: 0.5, ..line },
                    )),
                    ..default_graph_style(theme)
                }
            })
            .nodes(self.nodes.iter().enumerate().map(|(index, styled)| {
                let preset = styled.preset;
                let overrides = styled.overrides;
                // The hosted content mirrors the idle style, so the title bar
                // takes the body's hue and the node's geometry.
                let idle = overrides.apply(preset.style(theme, NodeStatus::Idle));
                node(
                    index,
                    styled.position,
                    styled_node(&styled.name, &idle, theme),
                )
                .selected(self.graph_selection.contains(&index))
                .style(move |theme, status| overrides.apply(preset.style(theme, status)))
                .pin_style(styling_pin_style)
            }))
            .push_node(
                node(FRAME_NODE, self.frame.position(), self.frame_content())
                    .frame()
                    .resizable(true),
            )
            // Anchors before edges only for readability; the widget resolves
            // ids through its own lookups, so the order between the two is free.
            .anchors(
                self.anchors
                    .iter()
                    .map(|&(id, position)| anchor(id, position)),
            )
            .edges(self.edges.iter().map(|model| {
                edge(model.id, model.from, model.to).route(model.route.iter().copied())
            }))
            .into()
    }

    /// The frame's own content: a title bar over an empty body, laid out at the
    /// size the host holds so the grip reports against what is drawn.
    fn frame_content(&self) -> Element<'_, Message> {
        container(column![
            node_title_bar("Inputs", NodeContentStyle::comment(&self.current_theme)),
            Space::new().width(Length::Fill).height(Length::Fill),
        ])
        .width(self.frame.width)
        .height(self.frame.height)
        .into()
    }
}
