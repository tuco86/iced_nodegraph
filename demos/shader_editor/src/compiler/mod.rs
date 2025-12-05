pub mod codegen;
pub mod validation;

use crate::shader_graph::{ShaderGraph, ShaderNodeType};
use codegen::CodeGenerator;
use validation::{ValidationError, Validator};

pub struct ShaderCompiler;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CompileError {
    Validation(ValidationError),
    CodeGeneration(String),
}

impl From<ValidationError> for CompileError {
    fn from(err: ValidationError) -> Self {
        CompileError::Validation(err)
    }
}

impl ShaderCompiler {
    pub fn compile(graph: &ShaderGraph) -> Result<String, CompileError> {
        // Validate graph
        Validator::validate(graph)?;

        // Get execution order
        let order = Validator::topological_sort(graph)?;

        let mut wgsl = String::new();

        // Include SDF library
        wgsl.push_str(include_str!("../sdf_library.wgsl"));
        wgsl.push_str("\n\n// Generated node functions\n\n");

        // Generate node functions in dependency order
        for node_id in &order {
            if let Some(node) = graph.get_node(*node_id) {
                let func_code = CodeGenerator::generate_node_function(graph, node);
                if !func_code.is_empty() {
                    wgsl.push_str(&func_code);
                }
            }
        }

        // Generate fragment shader entry points
        wgsl.push_str("\n// Edge Fragment Shader\n");
        wgsl.push_str(&Self::generate_edge_shader(graph)?);

        Ok(wgsl)
    }

    fn generate_edge_shader(graph: &ShaderGraph) -> Result<String, CompileError> {
        // Find OutputEdge node
        let output_node = CodeGenerator::find_output_node(graph, ShaderNodeType::OutputEdge);

        if let Some(output) = output_node {
            // Get connected color input
            let color_input = if let Some((from_node, _from_socket)) =
                graph.get_connected_input(output.id, 0)
            {
                format!("node_{}()", from_node)
            } else {
                "vec4(1.0, 0.0, 1.0, 1.0)".to_string() // Magenta default
            };

            Ok(format!(
                r#"
@fragment
fn fs_edge(in: EdgeVertexOutput) -> @location(0) vec4<f32> {{
    return {};
}}
"#,
                color_input
            ))
        } else {
            // Default edge shader if no output node
            Ok(r#"
@fragment
fn fs_edge(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    let edge = edges[in.instance_id];
    let from_node = nodes[edge.from_node];
    let from_pin = pins[from_node.pin_start + edge.from_pin];
    let to_node = nodes[edge.to_node];
    let to_pin = pins[to_node.pin_start + edge.to_pin];

    let dir_from = get_pin_direction(from_pin.side);
    let dir_to = get_pin_direction(to_pin.side);
    let seg_len = 80.0;

    let p0 = from_pin.position;
    let p1 = p0 + dir_from * seg_len;
    let p3 = to_pin.position;
    let p2 = p3 + dir_to * seg_len;

    let edge_color = select_edge_color(from_pin, to_pin);
    let dist = sdCubicBezier(in.world_uv, p0, p1, p2, p3);
    let edge_thickness = 2.0 / uniforms.camera_zoom;
    let aa = 1.0 / uniforms.camera_zoom;
    let alpha = 1.0 - smoothstep(edge_thickness, edge_thickness + aa, dist);

    return vec4(edge_color, alpha);
}
"#.to_string())
        }
    }
}
