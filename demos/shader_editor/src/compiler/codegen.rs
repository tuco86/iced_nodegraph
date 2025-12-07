use crate::shader_graph::{ShaderGraph, ShaderNode, ShaderNodeType};

pub struct CodeGenerator;

impl CodeGenerator {
    pub fn generate_node_function(graph: &ShaderGraph, node: &ShaderNode) -> String {
        let func_name = format!("node_{}", node.id);
        let node_type = &node.node_type;

        match node_type {
            // Input nodes - directly access uniforms
            ShaderNodeType::UV => {
                format!(
                    "fn {}() -> vec2<f32> {{ return in.world_uv; }}\n",
                    func_name
                )
            }
            ShaderNodeType::Time => {
                format!("fn {}() -> f32 {{ return uniforms.time; }}\n", func_name)
            }
            ShaderNodeType::MousePos => {
                format!(
                    "fn {}() -> vec2<f32> {{ return uniforms.cursor_position; }}\n",
                    func_name
                )
            }
            ShaderNodeType::Resolution => {
                format!(
                    "fn {}() -> vec2<f32> {{ return uniforms.viewport_size; }}\n",
                    func_name
                )
            }
            ShaderNodeType::CameraZoom => {
                format!(
                    "fn {}() -> f32 {{ return uniforms.camera_zoom; }}\n",
                    func_name
                )
            }
            ShaderNodeType::CameraPosition => {
                format!(
                    "fn {}() -> vec2<f32> {{ return uniforms.camera_position; }}\n",
                    func_name
                )
            }

            // Math operations - binary
            ShaderNodeType::Add => Self::gen_binary_op(graph, node, "+"),
            ShaderNodeType::Sub => Self::gen_binary_op(graph, node, "-"),
            ShaderNodeType::Mul => Self::gen_binary_op(graph, node, "*"),
            ShaderNodeType::Div => Self::gen_binary_op(graph, node, "/"),

            // Math operations - functions
            ShaderNodeType::Pow => Self::gen_binary_func(graph, node, "pow"),
            ShaderNodeType::Min => Self::gen_binary_func(graph, node, "min"),
            ShaderNodeType::Max => Self::gen_binary_func(graph, node, "max"),
            ShaderNodeType::Atan2 => Self::gen_binary_func(graph, node, "atan2"),
            ShaderNodeType::Mod => Self::gen_binary_func(graph, node, "mod"),
            ShaderNodeType::Step => Self::gen_binary_func(graph, node, "step"),

            // Math operations - unary
            ShaderNodeType::Sqrt => Self::gen_unary_func(graph, node, "sqrt"),
            ShaderNodeType::Abs => Self::gen_unary_func(graph, node, "abs"),
            ShaderNodeType::Sin => Self::gen_unary_func(graph, node, "sin"),
            ShaderNodeType::Cos => Self::gen_unary_func(graph, node, "cos"),
            ShaderNodeType::Tan => Self::gen_unary_func(graph, node, "tan"),
            ShaderNodeType::Asin => Self::gen_unary_func(graph, node, "asin"),
            ShaderNodeType::Acos => Self::gen_unary_func(graph, node, "acos"),
            ShaderNodeType::Atan => Self::gen_unary_func(graph, node, "atan"),
            ShaderNodeType::Floor => Self::gen_unary_func(graph, node, "floor"),
            ShaderNodeType::Ceil => Self::gen_unary_func(graph, node, "ceil"),
            ShaderNodeType::Fract => Self::gen_unary_func(graph, node, "fract"),
            ShaderNodeType::Sign => Self::gen_unary_func(graph, node, "sign"),

            // Clamp (ternary)
            ShaderNodeType::Clamp => Self::gen_ternary_func(graph, node, "clamp"),

            // Vector split
            ShaderNodeType::VecSplit2 => {
                let input = Self::get_input_value(graph, node, 0, "vec2(0.0, 0.0)");
                format!(
                    "fn {}_x() -> f32 {{ return ({}).x; }}\nfn {}_y() -> f32 {{ return ({}).y; }}\n",
                    func_name, input, func_name, input
                )
            }
            ShaderNodeType::VecSplit3 => {
                let input = Self::get_input_value(graph, node, 0, "vec3(0.0, 0.0, 0.0)");
                format!(
                    "fn {}_x() -> f32 {{ return ({}).x; }}\nfn {}_y() -> f32 {{ return ({}).y; }}\nfn {}_z() -> f32 {{ return ({}).z; }}\n",
                    func_name, input, func_name, input, func_name, input
                )
            }
            ShaderNodeType::VecSplit4 => {
                let input = Self::get_input_value(graph, node, 0, "vec4(0.0, 0.0, 0.0, 0.0)");
                format!(
                    "fn {}_x() -> f32 {{ return ({}).x; }}\nfn {}_y() -> f32 {{ return ({}).y; }}\nfn {}_z() -> f32 {{ return ({}).z; }}\nfn {}_w() -> f32 {{ return ({}).w; }}\n",
                    func_name, input, func_name, input, func_name, input, func_name, input
                )
            }

            // Vector combine
            ShaderNodeType::VecCombine2 => {
                let x = Self::get_input_value(graph, node, 0, "0.0");
                let y = Self::get_input_value(graph, node, 1, "0.0");
                format!(
                    "fn {}() -> vec2<f32> {{ return vec2({}, {}); }}\n",
                    func_name, x, y
                )
            }
            ShaderNodeType::VecCombine3 => {
                let x = Self::get_input_value(graph, node, 0, "0.0");
                let y = Self::get_input_value(graph, node, 1, "0.0");
                let z = Self::get_input_value(graph, node, 2, "0.0");
                format!(
                    "fn {}() -> vec3<f32> {{ return vec3({}, {}, {}); }}\n",
                    func_name, x, y, z
                )
            }
            ShaderNodeType::VecCombine4 => {
                let x = Self::get_input_value(graph, node, 0, "0.0");
                let y = Self::get_input_value(graph, node, 1, "0.0");
                let z = Self::get_input_value(graph, node, 2, "0.0");
                let w = Self::get_input_value(graph, node, 3, "0.0");
                format!(
                    "fn {}() -> vec4<f32> {{ return vec4({}, {}, {}, {}); }}\n",
                    func_name, x, y, z, w
                )
            }

            // Vector operations
            ShaderNodeType::Dot => Self::gen_binary_func(graph, node, "dot"),
            ShaderNodeType::Length => Self::gen_unary_func(graph, node, "length"),
            ShaderNodeType::Distance => Self::gen_binary_func(graph, node, "distance"),
            ShaderNodeType::Normalize => Self::gen_unary_func(graph, node, "normalize"),
            ShaderNodeType::Mix => Self::gen_ternary_func(graph, node, "mix"),
            ShaderNodeType::Smoothstep => Self::gen_ternary_func(graph, node, "smoothstep"),

            // SDF Circle
            ShaderNodeType::SDF_Circle => {
                let pos = Self::get_input_value(graph, node, 0, "vec2(0.0, 0.0)");
                let r = Self::get_input_value(graph, node, 1, "1.0");
                format!(
                    "fn {}() -> f32 {{ return sdCircle({}, {}); }}\n",
                    func_name, pos, r
                )
            }

            // SDF Box
            ShaderNodeType::SDF_Box => {
                let pos = Self::get_input_value(graph, node, 0, "vec2(0.0, 0.0)");
                let size = Self::get_input_value(graph, node, 1, "vec2(1.0, 1.0)");
                format!(
                    "fn {}() -> f32 {{ return sdBox({}, {}); }}\n",
                    func_name, pos, size
                )
            }

            // SDF Operations
            ShaderNodeType::SDF_Union => Self::gen_binary_func(graph, node, "opUnion"),
            ShaderNodeType::SDF_Subtraction => Self::gen_binary_func(graph, node, "opSubtraction"),
            ShaderNodeType::SDF_Intersection => {
                Self::gen_binary_func(graph, node, "opIntersection")
            }
            ShaderNodeType::SDF_SmoothUnion => Self::gen_ternary_func(graph, node, "opSmoothUnion"),
            ShaderNodeType::SDF_SmoothSubtraction => {
                Self::gen_ternary_func(graph, node, "opSmoothSubtraction")
            }
            ShaderNodeType::SDF_SmoothIntersection => {
                Self::gen_ternary_func(graph, node, "opSmoothIntersection")
            }

            // Output nodes don't generate functions
            ShaderNodeType::OutputEdge
            | ShaderNodeType::OutputBackground
            | ShaderNodeType::OutputNode
            | ShaderNodeType::OutputPin
            | ShaderNodeType::OutputFinal => String::new(),

            // Default
            _ => {
                format!(
                    "fn {}() -> f32 {{ return 0.0; }} // TODO: Implement {}\n",
                    func_name,
                    node_type.name()
                )
            }
        }
    }

    fn get_input_value(
        graph: &ShaderGraph,
        node: &ShaderNode,
        socket_index: usize,
        default: &str,
    ) -> String {
        if let Some((from_node, from_socket)) = graph.get_connected_input(node.id, socket_index) {
            let from_node_data = graph.get_node(from_node).unwrap();
            let func_name = format!("node_{}", from_node);

            // Handle vector split nodes which have multiple output functions
            match from_node_data.node_type {
                ShaderNodeType::VecSplit2 => match from_socket {
                    0 => format!("{}_x()", func_name),
                    1 => format!("{}_y()", func_name),
                    _ => default.to_string(),
                },
                ShaderNodeType::VecSplit3 => match from_socket {
                    0 => format!("{}_x()", func_name),
                    1 => format!("{}_y()", func_name),
                    2 => format!("{}_z()", func_name),
                    _ => default.to_string(),
                },
                ShaderNodeType::VecSplit4 => match from_socket {
                    0 => format!("{}_x()", func_name),
                    1 => format!("{}_y()", func_name),
                    2 => format!("{}_z()", func_name),
                    3 => format!("{}_w()", func_name),
                    _ => default.to_string(),
                },
                _ => format!("{}()", func_name),
            }
        } else {
            // Use socket default value if available
            node.inputs
                .get(socket_index)
                .and_then(|s| s.default_value.as_ref())
                .map(|v| v.clone())
                .unwrap_or_else(|| default.to_string())
        }
    }

    fn gen_binary_op(graph: &ShaderGraph, node: &ShaderNode, op: &str) -> String {
        let a = Self::get_input_value(graph, node, 0, "0.0");
        let b = Self::get_input_value(graph, node, 1, "0.0");
        let func_name = format!("node_{}", node.id);
        format!(
            "fn {}() -> f32 {{ return ({}) {} ({}); }}\n",
            func_name, a, op, b
        )
    }

    fn gen_binary_func(graph: &ShaderGraph, node: &ShaderNode, func: &str) -> String {
        let a = Self::get_input_value(graph, node, 0, "0.0");
        let b = Self::get_input_value(graph, node, 1, "0.0");
        let func_name = format!("node_{}", node.id);
        format!(
            "fn {}() -> f32 {{ return {}({}, {}); }}\n",
            func_name, func, a, b
        )
    }

    fn gen_unary_func(graph: &ShaderGraph, node: &ShaderNode, func: &str) -> String {
        let a = Self::get_input_value(graph, node, 0, "0.0");
        let func_name = format!("node_{}", node.id);
        format!("fn {}() -> f32 {{ return {}({}); }}\n", func_name, func, a)
    }

    fn gen_ternary_func(graph: &ShaderGraph, node: &ShaderNode, func: &str) -> String {
        let a = Self::get_input_value(graph, node, 0, "0.0");
        let b = Self::get_input_value(graph, node, 1, "0.0");
        let c = Self::get_input_value(graph, node, 2, "0.0");
        let func_name = format!("node_{}", node.id);
        format!(
            "fn {}() -> f32 {{ return {}({}, {}, {}); }}\n",
            func_name, func, a, b, c
        )
    }

    pub fn find_output_node(
        graph: &ShaderGraph,
        output_type: ShaderNodeType,
    ) -> Option<&ShaderNode> {
        graph.nodes.iter().find(|n| n.node_type == output_type)
    }
}
