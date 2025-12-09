use crate::nodes::NodeType;
use iced::Point;
use iced_nodegraph::PinReference;

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
pub fn generate_procedural_graph() -> (Vec<(Point, NodeType)>, Vec<(PinReference, PinReference)>) {
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
            edges.push((
                PinReference::new(input_node, 0),
                PinReference::new(node_idx, 0),
            ));

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
            edges.push((
                PinReference::new(noise_node, 0),
                PinReference::new(node_idx, 0),
            ));

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
            edges.push((
                PinReference::new(vector_node, 0),
                PinReference::new(node_idx, 0),
            ));

            // Some math nodes also connect to other math nodes in previous column
            if col > 0 {
                let prev_math = math_nodes[row % math_nodes.len().max(1)];
                edges.push((
                    PinReference::new(prev_math, 0),
                    PinReference::new(node_idx, 1),
                ));
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
            edges.push((
                PinReference::new(math_node, 0),
                PinReference::new(node_idx, 0),
            ));

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
        edges.push((
            PinReference::new(tex_node, 0),
            PinReference::new(node_idx, 0),
        ));

        // Also connect to another texture node for blending
        let tex_node2 = texture_nodes[(row + 1) % texture_nodes.len()];
        edges.push((
            PinReference::new(tex_node2, 0),
            PinReference::new(node_idx, 1),
        ));

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
        edges.push((
            PinReference::new(blend_node, 0),
            PinReference::new(node_idx, 0),
        ));

        node_idx += 1;
    }

    println!("Generated {} nodes and {} edges", nodes.len(), edges.len());

    (nodes, edges)
}
