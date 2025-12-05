use crate::shader_graph::ShaderGraph;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ValidationError {
    CyclicDependency,
    TypeMismatch,
    InvalidConnection,
    MissingOutputNode,
}

pub struct Validator;

impl Validator {
    pub fn validate(graph: &ShaderGraph) -> Result<(), ValidationError> {
        Self::check_cycles(graph)?;
        Self::check_types(graph)?;
        Self::check_output_nodes(graph)?;
        Ok(())
    }

    fn check_cycles(graph: &ShaderGraph) -> Result<(), ValidationError> {
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        let mut adj_list: HashMap<usize, Vec<usize>> = HashMap::new();

        // Build adjacency list and in-degree count
        for node in &graph.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        for conn in &graph.connections {
            *in_degree.get_mut(&conn.to_node).unwrap() += 1;
            adj_list.get_mut(&conn.from_node).unwrap().push(conn.to_node);
        }

        // Kahn's algorithm for topological sort
        let mut queue: VecDeque<usize> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();

        let mut processed = 0;

        while let Some(node_id) = queue.pop_front() {
            processed += 1;

            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        if processed != graph.nodes.len() {
            return Err(ValidationError::CyclicDependency);
        }

        Ok(())
    }

    fn check_types(graph: &ShaderGraph) -> Result<(), ValidationError> {
        for conn in &graph.connections {
            let from_node = graph
                .get_node(conn.from_node)
                .ok_or(ValidationError::InvalidConnection)?;

            let to_node = graph
                .get_node(conn.to_node)
                .ok_or(ValidationError::InvalidConnection)?;

            let from_socket = from_node
                .outputs
                .get(conn.from_socket)
                .ok_or(ValidationError::InvalidConnection)?;

            let to_socket = to_node
                .inputs
                .get(conn.to_socket)
                .ok_or(ValidationError::InvalidConnection)?;

            if !from_socket.socket_type.can_connect_to(&to_socket.socket_type) {
                return Err(ValidationError::TypeMismatch);
            }
        }

        Ok(())
    }

    fn check_output_nodes(graph: &ShaderGraph) -> Result<(), ValidationError> {
        use crate::shader_graph::ShaderNodeType;

        let has_output = graph.nodes.iter().any(|node| {
            matches!(
                node.node_type,
                ShaderNodeType::OutputBackground
                    | ShaderNodeType::OutputNode
                    | ShaderNodeType::OutputPin
                    | ShaderNodeType::OutputEdge
                    | ShaderNodeType::OutputFinal
            )
        });

        if !has_output {
            return Err(ValidationError::MissingOutputNode);
        }

        Ok(())
    }

    pub fn topological_sort(graph: &ShaderGraph) -> Result<Vec<usize>, ValidationError> {
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        let mut adj_list: HashMap<usize, Vec<usize>> = HashMap::new();

        for node in &graph.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        for conn in &graph.connections {
            *in_degree.get_mut(&conn.to_node).unwrap() += 1;
            adj_list.get_mut(&conn.from_node).unwrap().push(conn.to_node);
        }

        let mut queue: VecDeque<usize> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();

        let mut result = Vec::new();

        while let Some(node_id) = queue.pop_front() {
            result.push(node_id);

            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        if result.len() != graph.nodes.len() {
            return Err(ValidationError::CyclicDependency);
        }

        Ok(result)
    }
}
