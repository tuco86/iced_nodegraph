use super::layout::ForceDirectedLayout;
use crate::nodes::NodeType;
use iced::Point;
use iced_nodegraph::PinRef;

/// Generates a realistic procedural shader graph with ~500 nodes.
///
/// The graph is organized in 7 stages:
/// 1. Input nodes (10) - sources like UV, Time, Normal, Position
/// 2. Noise generators (80) - Perlin, Voronoi, Simplex
/// 3. Vector operations (100) - Split, Combine, Normalize, Dot, Cross
/// 4. Math operations (150) - Add, Multiply, Divide, Subtract, Power
/// 5. Texture operations (100) - Sampler2D, ColorMix, Gradient
/// 6. Blending (50) - Mix and blend nodes
/// 7. Output nodes (10) - BaseColor, Roughness, Metallic, Emission, Normal
pub fn generate_procedural_graph() -> (
    Vec<(Point, NodeType)>,
    Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut node_idx = 0;

    // Layout configuration
    let column_width = 250.0;
    let row_height = 100.0;
    let start_x = 100.0;
    let start_y = 100.0;

    // Stage 1: Input nodes (10 nodes) - Column 0
    let mut input_nodes: Vec<(usize, NodeType)> = Vec::new();
    for i in 0..10 {
        let y = start_y + i as f32 * row_height;
        let node_type = match i % 4 {
            0 => NodeType::UVInput,
            1 => NodeType::TimeInput,
            2 => NodeType::NormalInput,
            _ => NodeType::PositionInput,
        };
        nodes.push((Point::new(start_x, y), node_type));
        input_nodes.push((node_idx, node_type));
        node_idx += 1;
    }

    // Stage 2: Noise generators (80 nodes) - Columns 1-2
    let mut noise_nodes: Vec<(usize, NodeType)> = Vec::new();
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
            let (src_idx, src_type) = input_nodes[row % input_nodes.len()];
            edges.push((
                PinRef::new(src_idx, src_type.output_pin().unwrap()),
                PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
            ));

            noise_nodes.push((node_idx, node_type));
            node_idx += 1;
        }
    }

    // Stage 3: Vector operations (100 nodes) - Columns 3-4
    let mut vector_nodes: Vec<(usize, NodeType)> = Vec::new();
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
            let (src_idx, src_type) = noise_nodes[row % noise_nodes.len()];
            edges.push((
                PinRef::new(src_idx, src_type.output_pin().unwrap()),
                PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
            ));

            vector_nodes.push((node_idx, node_type));
            node_idx += 1;
        }
    }

    // Stage 4: Math operations (150 nodes) - Columns 5-7
    let mut math_nodes: Vec<(usize, NodeType)> = Vec::new();
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
            let (src_idx, src_type) = vector_nodes[row % vector_nodes.len()];
            edges.push((
                PinRef::new(src_idx, src_type.output_pin().unwrap()),
                PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
            ));

            // Some math nodes also connect to other math nodes in previous column
            if col > 0 {
                let (prev_idx, prev_type) = math_nodes[row % math_nodes.len().max(1)];
                edges.push((
                    PinRef::new(prev_idx, prev_type.output_pin().unwrap()),
                    PinRef::new(node_idx, node_type.input_pin(1).unwrap()),
                ));
            }

            math_nodes.push((node_idx, node_type));
            node_idx += 1;
        }
    }

    // Stage 5: Texture operations (100 nodes) - Columns 8-9
    let mut texture_nodes: Vec<(usize, NodeType)> = Vec::new();
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
            let (src_idx, src_type) = math_nodes[row % math_nodes.len()];
            edges.push((
                PinRef::new(src_idx, src_type.output_pin().unwrap()),
                PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
            ));

            texture_nodes.push((node_idx, node_type));
            node_idx += 1;
        }
    }

    // Stage 6: More math/blending (50 nodes) - Column 10
    let mut blend_nodes: Vec<(usize, NodeType)> = Vec::new();
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
        let (tex_idx, tex_type) = texture_nodes[row % texture_nodes.len()];
        edges.push((
            PinRef::new(tex_idx, tex_type.output_pin().unwrap()),
            PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
        ));

        // Also connect to another texture node for blending
        let (tex_idx2, tex_type2) = texture_nodes[(row + 1) % texture_nodes.len()];
        edges.push((
            PinRef::new(tex_idx2, tex_type2.output_pin().unwrap()),
            PinRef::new(node_idx, node_type.input_pin(1).unwrap()),
        ));

        blend_nodes.push((node_idx, node_type));
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
        let (blend_idx, blend_type) = blend_nodes[i * 5 % blend_nodes.len()];
        edges.push((
            PinRef::new(blend_idx, blend_type.output_pin().unwrap()),
            PinRef::new(node_idx, node_type.input_pin(0).unwrap()),
        ));

        node_idx += 1;
    }

    println!("Generated {} nodes and {} edges", nodes.len(), edges.len());

    // Apply force-directed layout
    let positions: Vec<Point> = nodes.iter().map(|(pos, _)| *pos).collect();
    let node_types: Vec<NodeType> = nodes.iter().map(|(_, t)| *t).collect();

    let mut layout = ForceDirectedLayout::new(positions, &edges);
    let optimized_positions = layout.simulate();

    let optimized_nodes: Vec<(Point, NodeType)> =
        optimized_positions.into_iter().zip(node_types).collect();

    // Validate edges in debug builds
    #[cfg(debug_assertions)]
    validate_edges(&optimized_nodes, &edges);

    (optimized_nodes, edges)
}

/// Validates that all edges connect outputs to inputs correctly.
#[allow(dead_code)]
fn validate_edges(
    nodes: &[(Point, NodeType)],
    edges: &[(PinRef<usize, usize>, PinRef<usize, usize>)],
) {
    let mut error_count = 0;
    for (from, to) in edges {
        let from_type = &nodes[from.node_id].1;
        let to_type = &nodes[to.node_id].1;

        let valid_out = from_type.output_pin() == Some(from.pin_id);
        let valid_in = (0..4).any(|s| to_type.input_pin(s) == Some(to.pin_id));

        if !valid_out {
            eprintln!(
                "INVALID OUTPUT: {:?} -> {:?} (node {:?} has output at {:?}, not {})",
                from,
                to,
                from_type,
                from_type.output_pin(),
                from.pin_id
            );
            error_count += 1;
        }
        if !valid_in {
            eprintln!(
                "INVALID INPUT: {:?} -> {:?} (node {:?} has no input at pin {})",
                from, to, to_type, to.pin_id
            );
            error_count += 1;
        }
    }
    if error_count > 0 {
        eprintln!("Edge validation found {} errors", error_count);
    } else {
        println!(
            "Edge validation passed: all {} edges are valid",
            edges.len()
        );
    }
}
