//! # 500 Node Benchmark Demo
//!
//! Large-scale node graph demonstrating performance with 500+ nodes.
//! Simulates a procedural shader/material graph with multiple processing stages.

use iced::{
    Color, Length, Point, Subscription, Theme, window,
    widget::{column, container, stack, text},
};
use iced_nodegraph::{PinDirection, PinSide, node_graph, node_pin};

pub fn main() -> iced::Result {
    iced::application(Application::new, Application::update, Application::view)
        .subscription(Application::subscription)
        .title("500 Node Benchmark - iced_nodegraph")
        .theme(Application::theme)
        .run()
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
    Tick,
}

struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, NodeType)>,
    current_theme: Theme,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NodeType {
    // Input nodes (sources)
    TimeInput,
    UVInput,
    NormalInput,
    PositionInput,

    // Math operations
    Add,
    Multiply,
    Divide,
    Subtract,
    Power,

    // Noise generators
    PerlinNoise,
    VoronoiNoise,
    SimplexNoise,

    // Texture operations
    Sampler2D,
    ColorMix,
    Gradient,

    // Vector operations
    VectorSplit,
    VectorCombine,
    Normalize,
    DotProduct,
    CrossProduct,

    // Output nodes
    BaseColor,
    Roughness,
    Metallic,
    Emission,
    Normal,
}

impl Default for Application {
    fn default() -> Self {
        let (nodes, edges) = generate_procedural_graph();
        Self {
            edges,
            nodes,
            current_theme: Theme::CatppuccinMocha,
        }
    }
}

/// Generates a realistic procedural shader graph with ~500 nodes
fn generate_procedural_graph() -> (Vec<(Point, NodeType)>, Vec<((usize, usize), (usize, usize))>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut node_idx = 0;

    // Layout configuration
    let column_width = 250.0;
    let row_height = 100.0;
    let start_x = 100.0;
    let start_y = 100.0;

    // Stage 1: Input nodes (10 nodes) - Column 0
    let mut input_nodes = Vec::new();
    for i in 0..10 {
        let y = start_y + i as f32 * row_height;
        let node_type = match i % 4 {
            0 => NodeType::UVInput,
            1 => NodeType::TimeInput,
            2 => NodeType::NormalInput,
            _ => NodeType::PositionInput,
        };
        nodes.push((Point::new(start_x, y), node_type));
        input_nodes.push(node_idx);
        node_idx += 1;
    }

    // Stage 2: Noise generators (80 nodes) - Columns 1-2
    let mut noise_nodes = Vec::new();
    for col in 0..2 {
        for row in 0..40 {
            let x = start_x + (col + 1) as f32 * column_width;
            let y = start_y + row as f32 * row_height;
            let node_type = match row % 3 {
                0 => NodeType::PerlinNoise,
                1 => NodeType::VoronoiNoise,
                _ => NodeType::SimplexNoise,
            };
            nodes.push((Point::new(x, y), node_type));

            // Connect to random input
            let input_node = input_nodes[row % input_nodes.len()];
            edges.push(((input_node, 0), (node_idx, 0)));

            noise_nodes.push(node_idx);
            node_idx += 1;
        }
    }

    // Stage 3: Vector operations (100 nodes) - Columns 3-4
    let mut vector_nodes = Vec::new();
    for col in 0..2 {
        for row in 0..50 {
            let x = start_x + (col + 3) as f32 * column_width;
            let y = start_y + row as f32 * row_height;
            let node_type = match row % 5 {
                0 => NodeType::VectorSplit,
                1 => NodeType::VectorCombine,
                2 => NodeType::Normalize,
                3 => NodeType::DotProduct,
                _ => NodeType::CrossProduct,
            };
            nodes.push((Point::new(x, y), node_type));

            // Connect to noise nodes
            let noise_node = noise_nodes[row % noise_nodes.len()];
            edges.push(((noise_node, 0), (node_idx, 0)));

            vector_nodes.push(node_idx);
            node_idx += 1;
        }
    }

    // Stage 4: Math operations (150 nodes) - Columns 5-7
    let mut math_nodes = Vec::new();
    for col in 0..3 {
        for row in 0..50 {
            let x = start_x + (col + 5) as f32 * column_width;
            let y = start_y + row as f32 * row_height;
            let node_type = match row % 5 {
                0 => NodeType::Add,
                1 => NodeType::Multiply,
                2 => NodeType::Divide,
                3 => NodeType::Subtract,
                _ => NodeType::Power,
            };
            nodes.push((Point::new(x, y), node_type));

            // Connect to vector operations
            let vector_node = vector_nodes[row % vector_nodes.len()];
            edges.push(((vector_node, 0), (node_idx, 0)));

            // Some math nodes also connect to other math nodes in previous column
            if col > 0 {
                let prev_math = math_nodes[row % math_nodes.len().max(1)];
                edges.push(((prev_math, 0), (node_idx, 1)));
            }

            math_nodes.push(node_idx);
            node_idx += 1;
        }
    }

    // Stage 5: Texture operations (100 nodes) - Columns 8-9
    let mut texture_nodes = Vec::new();
    for col in 0..2 {
        for row in 0..50 {
            let x = start_x + (col + 8) as f32 * column_width;
            let y = start_y + row as f32 * row_height;
            let node_type = match row % 3 {
                0 => NodeType::Sampler2D,
                1 => NodeType::ColorMix,
                _ => NodeType::Gradient,
            };
            nodes.push((Point::new(x, y), node_type));

            // Connect to math operations
            let math_node = math_nodes[row % math_nodes.len()];
            edges.push(((math_node, 0), (node_idx, 0)));

            texture_nodes.push(node_idx);
            node_idx += 1;
        }
    }

    // Stage 6: More math/blending (50 nodes) - Column 10
    let mut blend_nodes = Vec::new();
    for row in 0..50 {
        let x = start_x + 10.0 * column_width;
        let y = start_y + row as f32 * row_height;
        let node_type = match row % 3 {
            0 => NodeType::Multiply,
            1 => NodeType::Add,
            _ => NodeType::ColorMix,
        };
        nodes.push((Point::new(x, y), node_type));

        // Connect to texture operations
        let tex_node = texture_nodes[row % texture_nodes.len()];
        edges.push(((tex_node, 0), (node_idx, 0)));

        // Also connect to another texture node for blending
        let tex_node2 = texture_nodes[(row + 1) % texture_nodes.len()];
        edges.push(((tex_node2, 0), (node_idx, 1)));

        blend_nodes.push(node_idx);
        node_idx += 1;
    }

    // Stage 7: Output nodes (10 nodes) - Column 11
    let output_types = [
        NodeType::BaseColor,
        NodeType::BaseColor,
        NodeType::Roughness,
        NodeType::Roughness,
        NodeType::Metallic,
        NodeType::Metallic,
        NodeType::Emission,
        NodeType::Emission,
        NodeType::Normal,
        NodeType::Normal,
    ];

    for (i, node_type) in output_types.iter().enumerate() {
        let x = start_x + 11.0 * column_width;
        let y = start_y + i as f32 * row_height * 2.0;
        nodes.push((Point::new(x, y), *node_type));

        // Connect to blend nodes
        let blend_node = blend_nodes[i * 5 % blend_nodes.len()];
        edges.push(((blend_node, 0), (node_idx, 0)));

        node_idx += 1;
    }

    println!("Generated {} nodes and {} edges", nodes.len(), edges.len());

    (nodes, edges)
}

impl Application {
    fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, message: ApplicationMessage) {
        match message {
            ApplicationMessage::Noop => (),
            ApplicationMessage::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges.push(((from_node, from_pin), (to_node, to_pin)));
            }
            ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(node_index) {
                    *position = new_position;
                }
            }
            ApplicationMessage::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges
                    .retain(|edge| *edge != ((from_node, from_pin), (to_node, to_pin)));
            }
            ApplicationMessage::Tick => {
                // Trigger redraw for animations
            }
        }
    }

    fn theme(&self) -> Theme {
        self.current_theme.clone()
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

        // Add all nodes
        for (position, node_type) in &self.nodes {
            ng.push_node(*position, create_node(*node_type, &self.current_theme));
        }

        // Add all edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }

        // Add stats overlay
        let stats = container(
            column![
                text(format!("Nodes: {}", self.nodes.len())).size(14),
                text(format!("Edges: {}", self.edges.len())).size(14),
                text("Scroll: Zoom | Middle-drag: Pan").size(12),
            ]
            .spacing(4)
            .padding(10)
        )
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(
                    Color::from_rgba(
                        palette.background.base.color.r,
                        palette.background.base.color.g,
                        palette.background.base.color.b,
                        0.9
                    )
                )),
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
        window::frames().map(|_| ApplicationMessage::Tick)
    }
}

fn create_node<'a, Message>(
    node_type: NodeType,
    theme: &'a Theme,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();

    let (title, pins, width) = match node_type {
        NodeType::TimeInput => (
            "Time",
            vec![("t", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.5, 0.2))],
            100.0,
        ),
        NodeType::UVInput => (
            "UV",
            vec![("uv", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.7, 0.3))],
            100.0,
        ),
        NodeType::NormalInput => (
            "Normal",
            vec![("N", PinSide::Right, PinDirection::Output, Color::from_rgb(0.5, 0.7, 0.9))],
            100.0,
        ),
        NodeType::PositionInput => (
            "Position",
            vec![("P", PinSide::Right, PinDirection::Output, Color::from_rgb(0.3, 0.9, 0.5))],
            100.0,
        ),
        NodeType::Add => (
            "Add",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::Multiply => (
            "Multiply",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::Divide => (
            "Divide",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::Subtract => (
            "Subtract",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::Power => (
            "Power",
            vec![
                ("val", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("exp", PinSide::Left, PinDirection::Input, Color::from_rgb(0.8, 0.8, 0.8)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::PerlinNoise => (
            "Perlin",
            vec![
                ("in", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.7, 0.3)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.7, 0.9, 0.7)),
            ],
            120.0,
        ),
        NodeType::VoronoiNoise => (
            "Voronoi",
            vec![
                ("in", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.7, 0.3)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.7, 0.9, 0.7)),
            ],
            120.0,
        ),
        NodeType::SimplexNoise => (
            "Simplex",
            vec![
                ("in", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.7, 0.3)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.7, 0.9, 0.7)),
            ],
            120.0,
        ),
        NodeType::Sampler2D => (
            "Texture",
            vec![
                ("uv", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.7, 0.3)),
                ("rgba", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.5, 0.9)),
            ],
            120.0,
        ),
        NodeType::ColorMix => (
            "Mix",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.5, 0.9)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.5, 0.9)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.5, 0.9)),
            ],
            120.0,
        ),
        NodeType::Gradient => (
            "Gradient",
            vec![
                ("t", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.5, 0.2)),
                ("col", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.5, 0.9)),
            ],
            120.0,
        ),
        NodeType::VectorSplit => (
            "Split",
            vec![
                ("vec", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("x", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.3, 0.3)),
                ("y", PinSide::Right, PinDirection::Output, Color::from_rgb(0.3, 0.9, 0.3)),
                ("z", PinSide::Right, PinDirection::Output, Color::from_rgb(0.3, 0.3, 0.9)),
            ],
            120.0,
        ),
        NodeType::VectorCombine => (
            "Combine",
            vec![
                ("x", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.3, 0.3)),
                ("y", PinSide::Left, PinDirection::Input, Color::from_rgb(0.3, 0.9, 0.3)),
                ("z", PinSide::Left, PinDirection::Input, Color::from_rgb(0.3, 0.3, 0.9)),
                ("vec", PinSide::Right, PinDirection::Output, Color::from_rgb(0.5, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::Normalize => (
            "Normalize",
            vec![
                ("in", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.5, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::DotProduct => (
            "Dot",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.9, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::CrossProduct => (
            "Cross",
            vec![
                ("A", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("B", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.9, 0.9)),
                ("out", PinSide::Right, PinDirection::Output, Color::from_rgb(0.5, 0.9, 0.9)),
            ],
            120.0,
        ),
        NodeType::BaseColor => (
            "Base Color",
            vec![("col", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.5, 0.9))],
            140.0,
        ),
        NodeType::Roughness => (
            "Roughness",
            vec![("val", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.9, 0.9))],
            140.0,
        ),
        NodeType::Metallic => (
            "Metallic",
            vec![("val", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.9, 0.9))],
            140.0,
        ),
        NodeType::Emission => (
            "Emission",
            vec![("col", PinSide::Left, PinDirection::Input, Color::from_rgb(0.9, 0.9, 0.3))],
            140.0,
        ),
        NodeType::Normal => (
            "Normal",
            vec![("N", PinSide::Left, PinDirection::Input, Color::from_rgb(0.5, 0.7, 0.9))],
            140.0,
        ),
    };

    let title_bar = container(text(title).size(12).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: None,
            text_color: Some(palette.background.base.text),
            ..container::Style::default()
        });

    let pin_list = pins.into_iter().fold(
        column![].spacing(1),
        |col, (label, side, direction, color)| {
            col.push(
                node_pin(
                    side,
                    container(text(label).size(10)).padding([0, 6])
                )
                .direction(direction)
                .color(color)
            )
        }
    );

    let pin_section = container(pin_list).padding([4, 0]);
    column![title_bar, pin_section].width(width).into()
}
