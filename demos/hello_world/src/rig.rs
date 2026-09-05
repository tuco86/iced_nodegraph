//! The boot scene's config rig: one frame per `iced_nodegraph::Catalog` class
//! and status, each holding the source nodes that feed every field of that
//! class's config node, wired into one Catalog node. A thirteenth frame holds a
//! Node Class that tints one workflow node.
//!
//! The rig is a declarative table walked by [`build`]: every row is a field pin
//! plus the source that feeds it, and the walk lays the nodes out in fixed
//! columns so the scene is deterministic across boots.

use iced::{Color, Point, Size};
use iced_nodegraph::{EdgeCurve, PinShape, TilingKind};

use crate::ids::{EdgeData, EdgeId, NodeId, PinLabel, generate_edge_id, generate_node_id};
use crate::nodes::pins::{
    anchor, build, cfg, cutting_tool, edge, graph, input, minimap, node, pin, selection_box,
    theme_ext,
};
use crate::nodes::{
    AlphaNode, AnchorConfigInputs, ConfigNodeType, CuttingToolConfigInputs, EdgeConfigInputs,
    FloatSliderConfig, GraphConfigInputs, InputNodeType, MinimapConfigInputs, NodeConfigInputs,
    NodeType, PatternType, PinConfigInputs, SelectionBoxConfigInputs, Vec2Node,
};

/// The nodes and edges the rig adds to the boot scene.
pub struct Rig {
    pub nodes: Vec<(NodeId, Point, NodeType)>,
    pub edges: Vec<(EdgeId, EdgeData)>,
}

const FRAME_W: f32 = 640.0;
const FRAME_GAP: f32 = 40.0;
/// Pitch of a slider / selector / Alpha row.
const ROW: f32 = 84.0;
/// Pitch of a color picker row.
const ROW_PICKER: f32 = 176.0;
const COL_SOURCE: f32 = 16.0;
const COL_ALPHA: f32 = 200.0;
const COL_CONFIG: f32 = 400.0;
const HEADER: f32 = 48.0;
const FOOTER: f32 = 16.0;

/// A slider's range and start value.
#[derive(Clone, Copy)]
struct Slider {
    min: f32,
    max: f32,
    step: f32,
    value: f32,
}

const fn f(min: f32, max: f32, step: f32, value: f32) -> Slider {
    Slider {
        min,
        max,
        step,
        value,
    }
}

/// What feeds one field pin of a config node.
#[derive(Clone, Copy)]
enum Source {
    /// A float slider labelled with the pin.
    F(Slider),
    /// A float slider in degrees, converted to radians by the config pin.
    Angle(f32),
    Pattern(PatternType),
    Curve(EdgeCurve),
    Shape(PinShape),
    Tiling(TilingKind),
    /// An edge straight from the shared Theme Extended pin.
    Pal(PinLabel),
    /// Theme Extended pin -> Alpha (alpha from a 0..1 slider) -> config.
    PalA(PinLabel, f32),
    /// Color picker -> Alpha (alpha from a 0..1 slider) -> config.
    PickA(Color, f32),
    /// Two sliders -> Vec2 -> config.
    Vec2(Slider, Slider),
}

struct Row {
    pin: PinLabel,
    source: Source,
}

const fn row(pin: PinLabel, source: Source) -> Row {
    Row { pin, source }
}

/// Which config node a frame builds and the Catalog inputs its output feeds.
#[derive(Clone, Copy)]
enum Config {
    Node,
    Pin,
    Edge,
    Anchor,
    Graph,
    SelectionBox,
    CuttingTool,
    Minimap,
}

impl Config {
    fn node_type(self) -> NodeType {
        NodeType::Config(match self {
            Config::Node => ConfigNodeType::NodeConfig(NodeConfigInputs::default()),
            Config::Pin => ConfigNodeType::PinConfig(PinConfigInputs::default()),
            Config::Edge => ConfigNodeType::EdgeConfig(EdgeConfigInputs::default()),
            Config::Anchor => ConfigNodeType::AnchorConfig(AnchorConfigInputs::default()),
            Config::Graph => ConfigNodeType::GraphConfig(GraphConfigInputs::default()),
            Config::SelectionBox => {
                ConfigNodeType::SelectionBoxConfig(SelectionBoxConfigInputs::default())
            }
            Config::CuttingTool => {
                ConfigNodeType::CuttingToolConfig(CuttingToolConfigInputs::default())
            }
            Config::Minimap => ConfigNodeType::MinimapConfig(MinimapConfigInputs::default()),
        })
    }

    fn out_pin(self) -> PinLabel {
        match self {
            Config::Node => cfg::NODE_OUT,
            Config::Pin => cfg::PIN_OUT,
            Config::Edge => cfg::EDGE_OUT,
            Config::Anchor => cfg::ANCHOR_OUT,
            Config::Graph => cfg::GRAPH_OUT,
            Config::SelectionBox => cfg::SELECTION_BOX_OUT,
            Config::CuttingTool => cfg::CUTTING_TOOL_OUT,
            Config::Minimap => cfg::MINIMAP_OUT,
        }
    }

    /// Laid-out height of the config node with every section expanded, so a
    /// frame with few rows still encloses it.
    fn height(self) -> f32 {
        match self {
            Config::Node => 560.0,
            Config::Edge => 760.0,
            Config::Pin => 220.0,
            Config::Graph => 200.0,
            Config::Anchor => 300.0,
            Config::SelectionBox => 150.0,
            Config::CuttingTool => 130.0,
            Config::Minimap => 280.0,
        }
    }
}

struct Frame {
    title: &'static str,
    config: Config,
    /// Catalog inputs the config output is wired to.
    sinks: &'static [PinLabel],
    rows: &'static [Row],
}

const FRAMES: &[Frame] = &[
    Frame {
        title: "Node",
        config: Config::Node,
        sinks: &[cfg::NODE_CONFIG],
        rows: &[
            row(node::CORNER_RADIUS, Source::F(f(0.0, 32.0, 1.0, 8.0))),
            row(node::OPACITY, Source::F(f(0.0, 1.0, 0.01, 0.88))),
            row(node::FILL_COLOR, Source::Pal(theme_ext::BACKGROUND_WEAK)),
            row(
                node::BORDER_COLOR,
                Source::Pal(theme_ext::BACKGROUND_STRONG),
            ),
            row(node::BORDER_WIDTH, Source::F(f(0.0, 8.0, 0.5, 1.0))),
            row(node::BORDER_OUTLINE_WIDTH, Source::F(f(0.0, 8.0, 0.5, 0.0))),
            row(
                node::BORDER_OUTLINE_COLOR,
                Source::PalA(theme_ext::PRIMARY_BASE, 0.28),
            ),
            row(node::PATTERN, Source::Pattern(PatternType::Solid)),
            row(node::DASH, Source::F(f(0.0, 40.0, 1.0, 8.0))),
            row(node::GAP, Source::F(f(0.0, 40.0, 1.0, 4.0))),
            row(node::ANGLE, Source::Angle(0.0)),
            row(node::SPEED, Source::F(f(0.0, 10.0, 0.5, 0.0))),
            row(node::SHADOW_COLOR, Source::PickA(Color::BLACK, 0.38)),
            row(node::SHADOW_DISTANCE, Source::F(f(0.0, 30.0, 1.0, 7.0))),
            row(
                node::SHADOW_OFFSET,
                Source::Vec2(f(-20.0, 20.0, 1.0, 0.0), f(-20.0, 20.0, 1.0, 3.0)),
            ),
        ],
    },
    Frame {
        title: "Node: selected",
        config: Config::Node,
        sinks: &[cfg::NODE_SELECTED],
        rows: &[
            row(node::FILL_COLOR, Source::Pal(theme_ext::PRIMARY_WEAK)),
            row(node::BORDER_COLOR, Source::Pal(theme_ext::PRIMARY_BASE)),
            row(node::BORDER_WIDTH, Source::F(f(0.0, 8.0, 0.5, 2.0))),
            row(node::BORDER_OUTLINE_WIDTH, Source::F(f(0.0, 8.0, 0.5, 3.0))),
            row(
                node::BORDER_OUTLINE_COLOR,
                Source::PalA(theme_ext::PRIMARY_BASE, 0.28),
            ),
            row(node::SHADOW_DISTANCE, Source::F(f(0.0, 30.0, 1.0, 11.0))),
        ],
    },
    Frame {
        title: "Pin",
        config: Config::Pin,
        sinks: &[cfg::PIN_CONFIG],
        // `color` stays unwired: the pin's data-type color is the demo's own
        // semantic and a wired source would override it.
        rows: &[
            row(pin::RADIUS, Source::F(f(1.0, 12.0, 0.5, 5.0))),
            row(pin::CUTOUT_RADIUS, Source::F(f(1.0, 16.0, 0.5, 8.0))),
            row(pin::SHAPE, Source::Shape(PinShape::Circle)),
            row(pin::BORDER_COLOR, Source::Pal(theme_ext::BACKGROUND_STRONG)),
            row(pin::BORDER_WIDTH, Source::F(f(0.0, 6.0, 0.5, 0.0))),
        ],
    },
    Frame {
        title: "Pin: valid target",
        config: Config::Pin,
        sinks: &[cfg::PIN_VALID_TARGET],
        rows: &[
            row(pin::COLOR, Source::Pal(theme_ext::SUCCESS_BASE)),
            row(
                pin::BORDER_COLOR,
                Source::PalA(theme_ext::SUCCESS_BASE, 0.4),
            ),
            row(pin::BORDER_WIDTH, Source::F(f(0.0, 6.0, 0.5, 3.0))),
        ],
    },
    Frame {
        title: "Edge",
        config: Config::Edge,
        // One output, two Catalog inputs: the drag edge is an EdgeStyle too.
        sinks: &[cfg::EDGE_CONFIG, cfg::DRAG_EDGE],
        // `stroke_color` stays unwired for the same reason as the pin color.
        rows: &[
            row(edge::THICKNESS, Source::F(f(0.5, 10.0, 0.5, 2.0))),
            row(edge::CURVE, Source::Curve(EdgeCurve::BezierCubic)),
            row(edge::STROKE_OUTLINE_WIDTH, Source::F(f(0.0, 6.0, 0.5, 0.0))),
            row(
                edge::STROKE_OUTLINE_COLOR,
                Source::Pal(theme_ext::BACKGROUND_STRONG),
            ),
            row(edge::PATTERN, Source::Pattern(PatternType::Solid)),
            row(edge::DASH, Source::F(f(0.0, 40.0, 1.0, 10.0))),
            row(edge::GAP, Source::F(f(0.0, 40.0, 1.0, 6.0))),
            row(edge::DOT_RADIUS, Source::F(f(0.5, 6.0, 0.5, 1.5))),
            row(edge::ANGLE, Source::Angle(0.0)),
            row(edge::SPEED, Source::F(f(0.0, 10.0, 0.5, 0.0))),
            row(edge::BORDER_WIDTH, Source::F(f(0.0, 6.0, 0.5, 0.0))),
            row(edge::BORDER_GAP, Source::F(f(0.0, 4.0, 0.1, 0.5))),
            row(
                edge::BORDER_COLOR,
                Source::Pal(theme_ext::BACKGROUND_STRONG),
            ),
            row(
                edge::BORDER_BACKGROUND,
                Source::Pal(theme_ext::BACKGROUND_BASE),
            ),
            row(edge::BORDER_OUTLINE_WIDTH, Source::F(f(0.0, 6.0, 0.5, 0.0))),
            row(
                edge::BORDER_OUTLINE_COLOR,
                Source::Pal(theme_ext::PRIMARY_BASE),
            ),
            row(edge::SHADOW_COLOR, Source::PickA(Color::BLACK, 0.5)),
            row(edge::SHADOW_BLUR, Source::F(f(0.0, 20.0, 1.0, 0.0))),
            row(edge::SHADOW_EXPAND, Source::F(f(0.0, 10.0, 0.5, 0.0))),
            row(
                edge::SHADOW_OFFSET,
                Source::Vec2(f(-20.0, 20.0, 1.0, 0.0), f(-20.0, 20.0, 1.0, 0.0)),
            ),
        ],
    },
    Frame {
        title: "Edge: pending cut",
        config: Config::Edge,
        sinks: &[cfg::EDGE_PENDING_CUT],
        rows: &[
            row(edge::STROKE_COLOR, Source::Pal(theme_ext::DANGER_BASE)),
            row(edge::THICKNESS, Source::F(f(0.5, 10.0, 0.5, 3.0))),
        ],
    },
    Frame {
        title: "Anchor",
        config: Config::Anchor,
        sinks: &[cfg::ANCHOR],
        rows: &[
            row(anchor::CORE_SIZE, Source::F(f(2.0, 20.0, 1.0, 6.0))),
            row(anchor::CORE_RADIUS, Source::F(f(0.0, 10.0, 0.5, 3.0))),
            row(anchor::CORE_COLOR, Source::Pal(theme_ext::BACKGROUND_WEAK)),
            row(
                anchor::CORE_BORDER_COLOR,
                Source::Pal(theme_ext::BACKGROUND_STRONG),
            ),
            row(anchor::CORE_BORDER_WIDTH, Source::F(f(0.0, 4.0, 0.5, 1.0))),
            row(anchor::ORBIT_OFFSET, Source::F(f(4.0, 40.0, 1.0, 11.0))),
            row(anchor::ORBIT_SPACING, Source::F(f(2.0, 20.0, 1.0, 6.0))),
            row(
                anchor::RING_COLOR,
                Source::PalA(theme_ext::PRIMARY_WEAK, 0.35),
            ),
            row(anchor::RING_WIDTH, Source::F(f(0.0, 4.0, 0.5, 1.0))),
        ],
    },
    Frame {
        title: "Anchor: hovered",
        config: Config::Anchor,
        sinks: &[cfg::ANCHOR_HOVERED],
        rows: &[
            row(
                anchor::CORE_BORDER_COLOR,
                Source::PalA(theme_ext::PRIMARY_BASE, 0.6),
            ),
            row(anchor::CORE_BORDER_WIDTH, Source::F(f(0.0, 4.0, 0.5, 1.5))),
        ],
    },
    Frame {
        title: "Anchor: valid target",
        config: Config::Anchor,
        sinks: &[cfg::ANCHOR_VALID_TARGET],
        rows: &[
            row(anchor::CORE_COLOR, Source::Pal(theme_ext::SUCCESS_BASE)),
            row(
                anchor::RING_COLOR,
                Source::PalA(theme_ext::SUCCESS_BASE, 0.7),
            ),
        ],
    },
    Frame {
        title: "Graph",
        config: Config::Graph,
        sinks: &[cfg::GRAPH_CONFIG],
        rows: &[
            row(graph::BACKGROUND, Source::Pal(theme_ext::BACKGROUND_BASE)),
            row(graph::TILING_KIND, Source::Tiling(TilingKind::Grid)),
            row(graph::SPACING, Source::F(f(10.0, 120.0, 5.0, 40.0))),
            row(graph::THICKNESS, Source::F(f(0.5, 4.0, 0.5, 1.0))),
            row(graph::LINE_COLOR, Source::Pal(theme_ext::BACKGROUND_WEAK)),
        ],
    },
    Frame {
        title: "Selection box",
        config: Config::SelectionBox,
        sinks: &[cfg::SELECTION_BOX],
        rows: &[
            row(
                selection_box::FILL,
                Source::PalA(theme_ext::PRIMARY_BASE, 0.15),
            ),
            row(
                selection_box::BORDER_COLOR,
                Source::PalA(theme_ext::PRIMARY_BASE, 0.75),
            ),
            row(
                selection_box::BORDER_WIDTH,
                Source::F(f(0.5, 6.0, 0.5, 1.5)),
            ),
        ],
    },
    Frame {
        title: "Cutting tool",
        config: Config::CuttingTool,
        sinks: &[cfg::CUTTING_TOOL],
        rows: &[
            row(cutting_tool::COLOR, Source::Pal(theme_ext::DANGER_BASE)),
            row(cutting_tool::WIDTH, Source::F(f(0.5, 8.0, 0.5, 3.0))),
        ],
    },
    Frame {
        title: "Minimap",
        config: Config::Minimap,
        sinks: &[cfg::MINIMAP],
        rows: &[
            row(
                minimap::BACKGROUND,
                Source::PalA(theme_ext::BACKGROUND_WEAK, 0.9),
            ),
            row(
                minimap::BORDER_COLOR,
                Source::Pal(theme_ext::BACKGROUND_STRONG),
            ),
            row(minimap::BORDER_WIDTH, Source::F(f(0.0, 4.0, 0.5, 1.0))),
            row(minimap::NODE_COLOR, Source::Pal(theme_ext::PRIMARY_WEAK)),
            row(
                minimap::SELECTED_NODE_COLOR,
                Source::Pal(theme_ext::PRIMARY_BASE),
            ),
            row(
                minimap::VIEWPORT_FILL,
                Source::PalA(theme_ext::PRIMARY_STRONG, 0.12),
            ),
            row(
                minimap::VIEWPORT_BORDER_COLOR,
                Source::PalA(theme_ext::PRIMARY_STRONG, 0.8),
            ),
            row(
                minimap::VIEWPORT_BORDER_WIDTH,
                Source::F(f(0.5, 4.0, 0.5, 1.0)),
            ),
        ],
    },
];

/// The rows of the Node Class frame's own Node Config.
const NODE_CLASS_ROWS: &[Row] = &[
    row(node::FILL_COLOR, Source::Pal(theme_ext::WARNING_WEAK)),
    row(node::BORDER_COLOR, Source::Pal(theme_ext::WARNING_BASE)),
];

/// Accumulates nodes and edges while the layout walk runs.
struct Builder {
    rig: Rig,
    palette: NodeId,
}

impl Builder {
    fn node(&mut self, position: Point, node_type: NodeType) -> NodeId {
        let id = generate_node_id();
        self.rig.nodes.push((id.clone(), position, node_type));
        id
    }

    fn edge(&mut self, from: &NodeId, from_pin: PinLabel, to: &NodeId, to_pin: PinLabel) {
        self.rig.edges.push((
            generate_edge_id(),
            EdgeData::new(from.clone(), from_pin, to.clone(), to_pin),
        ));
    }

    fn slider(&mut self, position: Point, label: &str, s: Slider) -> NodeId {
        self.node(
            position,
            NodeType::Input(InputNodeType::FloatSlider {
                config: FloatSliderConfig {
                    min: s.min,
                    max: s.max,
                    step: s.step,
                    label: label.to_string(),
                    ..FloatSliderConfig::default()
                },
                value: s.value,
            }),
        )
    }

    /// An Alpha node at the row's alpha column fed by a 0..1 slider at the
    /// source column; returns the Alpha node.
    fn alpha(&mut self, x: f32, y: f32, a: f32) -> NodeId {
        let slider = self.slider(
            Point::new(x + COL_SOURCE, y),
            build::ALPHA,
            f(0.0, 1.0, 0.01, a),
        );
        let alpha = self.node(
            Point::new(x + COL_ALPHA, y),
            NodeType::Alpha(AlphaNode::default()),
        );
        self.edge(&slider, input::VALUE, &alpha, build::ALPHA);
        alpha
    }

    /// Lays out one row's sources at `(x, y)` inside a frame, wires them into
    /// `config`'s pin, and returns the row's height.
    fn row(&mut self, x: f32, y: f32, config: &NodeId, r: &Row) -> f32 {
        let at = Point::new(x + COL_SOURCE, y);
        match r.source {
            Source::F(s) => {
                let src = self.slider(at, r.pin, s);
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Angle(value) => {
                let src = self.node(
                    at,
                    NodeType::Input(InputNodeType::FloatSlider {
                        config: FloatSliderConfig::pattern_angle(),
                        value,
                    }),
                );
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Pattern(value) => {
                let src = self.node(
                    at,
                    NodeType::Input(InputNodeType::PatternTypeSelector { value }),
                );
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Curve(value) => {
                let src = self.node(
                    at,
                    NodeType::Input(InputNodeType::EdgeCurveSelector { value }),
                );
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Shape(value) => {
                let src = self.node(
                    at,
                    NodeType::Input(InputNodeType::PinShapeSelector { value }),
                );
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Tiling(value) => {
                let src = self.node(
                    at,
                    NodeType::Input(InputNodeType::TilingKindSelector { value }),
                );
                self.edge(&src, input::VALUE, config, r.pin);
                ROW
            }
            Source::Pal(palette_pin) => {
                let palette = self.palette.clone();
                self.edge(&palette, palette_pin, config, r.pin);
                0.0
            }
            Source::PalA(palette_pin, a) => {
                let alpha = self.alpha(x, y, a);
                let palette = self.palette.clone();
                self.edge(&palette, palette_pin, &alpha, build::ALPHA_COLOR);
                self.edge(&alpha, build::ALPHA_OUT, config, r.pin);
                ROW
            }
            Source::PickA(color, a) => {
                let picker = self.node(at, NodeType::Input(InputNodeType::ColorPicker { color }));
                let alpha = self.node(
                    Point::new(x + COL_ALPHA, y),
                    NodeType::Alpha(AlphaNode::default()),
                );
                let slider = self.slider(
                    Point::new(x + COL_ALPHA, y + ROW),
                    build::ALPHA,
                    f(0.0, 1.0, 0.01, a),
                );
                self.edge(&picker, input::COLOR, &alpha, build::ALPHA_COLOR);
                self.edge(&slider, input::VALUE, &alpha, build::ALPHA);
                self.edge(&alpha, build::ALPHA_OUT, config, r.pin);
                ROW_PICKER
            }
            Source::Vec2(sx, sy) => {
                let x_slider = self.slider(at, build::X, sx);
                let y_slider = self.slider(Point::new(x + COL_SOURCE, y + ROW), build::Y, sy);
                let vec2 = self.node(
                    Point::new(x + COL_ALPHA, y),
                    NodeType::Vec2(Vec2Node::default()),
                );
                self.edge(&x_slider, input::VALUE, &vec2, build::X);
                self.edge(&y_slider, input::VALUE, &vec2, build::Y);
                self.edge(&vec2, build::VEC2_OUT, config, r.pin);
                2.0 * ROW
            }
        }
    }

    /// Lays out one frame at `origin`: the frame node, its config node at the
    /// config column, and every row; returns the frame's height and the config
    /// node's id. The frame node is pushed before its contents.
    fn frame(&mut self, origin: Point, title: &str, config: Config, rows: &[Row]) -> (f32, NodeId) {
        let frame_index = self.rig.nodes.len();
        // Placeholder position/size; patched once the height is known.
        let frame_id = self.node(
            origin,
            NodeType::Frame {
                label: title.to_string(),
                size: Size::new(FRAME_W, 0.0),
            },
        );
        let config_id = self.node(
            Point::new(origin.x + COL_CONFIG, origin.y + HEADER),
            config.node_type(),
        );
        let mut y = origin.y + HEADER;
        for r in rows {
            y += self.row(origin.x, y, &config_id, r);
        }
        let rows_height = y - origin.y - HEADER;
        let height = HEADER + rows_height.max(config.height()) + FOOTER;
        if let NodeType::Frame { size, .. } = &mut self.rig.nodes[frame_index].2 {
            size.height = height;
        }
        debug_assert_eq!(self.rig.nodes[frame_index].0, frame_id);
        (height, config_id)
    }
}

/// Builds the rig with its top-left corner at `origin`; `class_target` is the
/// workflow node the Node Class frame tints.
pub fn build(origin: Point, class_target: NodeId) -> Rig {
    let mut b = Builder {
        rig: Rig {
            nodes: Vec::new(),
            edges: Vec::new(),
        },
        palette: NodeId::new(),
    };

    // One Theme Extended node above the stack feeds every palette color.
    b.palette = b.node(
        Point::new(origin.x, origin.y - 400.0),
        NodeType::ThemeExtended,
    );

    // The Catalog sits in its own frame to the right of the stack.
    let catalog_x = origin.x + FRAME_W + FRAME_GAP;
    let catalog_frame_h = HEADER + 400.0 + FOOTER;
    b.node(
        Point::new(catalog_x, origin.y),
        NodeType::Frame {
            label: "Catalog".to_string(),
            size: Size::new(260.0, catalog_frame_h),
        },
    );
    let catalog = b.node(
        Point::new(catalog_x + COL_SOURCE, origin.y + HEADER),
        NodeType::Config(ConfigNodeType::Catalog {
            connected: Default::default(),
        }),
    );

    // The class frames, stacked vertically.
    let mut y = origin.y;
    for frame in FRAMES {
        let (height, config) = b.frame(
            Point::new(origin.x, y),
            frame.title,
            frame.config,
            frame.rows,
        );
        for sink in frame.sinks {
            b.edge(&config, frame.config.out_pin(), &catalog, sink);
        }
        y += height + FRAME_GAP;
    }

    // The Node Class frame below the Catalog: its own Node Config tints the
    // target workflow node.
    let class_y = origin.y + catalog_frame_h + FRAME_GAP;
    let (_, class_config) = b.frame(
        Point::new(catalog_x, class_y),
        "Node class",
        Config::Node,
        NODE_CLASS_ROWS,
    );
    let node_class = b.node(
        Point::new(catalog_x + COL_SOURCE, class_y + HEADER),
        NodeType::Config(ConfigNodeType::NodeClass {
            target: Some(class_target),
            has_node_config: false,
        }),
    );
    b.edge(&class_config, cfg::NODE_OUT, &node_class, cfg::NODE_CONFIG);

    b.rig
}
