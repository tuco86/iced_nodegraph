use crate::shader_graph::{ShaderGraph, ShaderNodeType};
use iced::Point;

pub fn create_default_graph() -> ShaderGraph {
    let mut graph = ShaderGraph::new();

    // Simple default: Animated pulsing circular edges

    // UV Input at (100, 100)
    let uv_node = graph.add_node(ShaderNodeType::UV, Point::new(100.0, 100.0));

    // Time Input at (100, 200)
    let time_node = graph.add_node(ShaderNodeType::Time, Point::new(100.0, 200.0));

    // Sin(Time) at (300, 200) - for pulsing animation
    let sin_node = graph.add_node(ShaderNodeType::Sin, Point::new(300.0, 200.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: time_node,
        from_socket: 0,
        to_node: sin_node,
        to_socket: 0,
    });

    // Multiply Sin * 0.5 for radius modulation at (500, 200)
    let mul_node = graph.add_node(ShaderNodeType::Mul, Point::new(500.0, 200.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: sin_node,
        from_socket: 0,
        to_node: mul_node,
        to_socket: 0,
    });
    // Note: Second input (0.5) would be set via socket default value

    // Add base radius: 2.0 + (Sin * 0.5) at (700, 150)
    let add_node = graph.add_node(ShaderNodeType::Add, Point::new(700.0, 150.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: mul_node,
        from_socket: 0,
        to_node: add_node,
        to_socket: 1,
    });
    // First input would be constant 2.0

    // Circle SDF at (500, 100)
    let circle_node = graph.add_node(ShaderNodeType::SDF_Circle, Point::new(500.0, 100.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: uv_node,
        from_socket: 0,
        to_node: circle_node,
        to_socket: 0, // Position
    });
    graph.add_connection(crate::shader_graph::Connection {
        from_node: add_node,
        from_socket: 0,
        to_node: circle_node,
        to_socket: 1, // Radius (animated)
    });

    // Smoothstep for anti-aliased edge at (700, 100)
    let smoothstep_node = graph.add_node(ShaderNodeType::Smoothstep, Point::new(700.0, 100.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: circle_node,
        from_socket: 0,
        to_node: smoothstep_node,
        to_socket: 2, // X value
    });
    // Edge0 = 0.0, Edge1 = 1.0 (defaults)

    // Create color: combine R, G, B channels at (900, 100)
    let _color_r = graph.add_node(ShaderNodeType::VecCombine3, Point::new(900.0, 50.0));
    // Would connect RGB values here

    // Alpha from smoothstep
    let color_node = graph.add_node(ShaderNodeType::VecCombine4, Point::new(1100.0, 100.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: smoothstep_node,
        from_socket: 0,
        to_node: color_node,
        to_socket: 3, // Alpha
    });

    // Output node at (1300, 100)
    let output_node = graph.add_node(ShaderNodeType::OutputEdge, Point::new(1300.0, 100.0));
    graph.add_connection(crate::shader_graph::Connection {
        from_node: color_node,
        from_socket: 0,
        to_node: output_node,
        to_socket: 0,
    });

    graph
}
