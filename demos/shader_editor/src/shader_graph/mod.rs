pub mod nodes;
pub mod sockets;

use iced::Point;
pub use nodes::ShaderNodeType;
pub use sockets::{Connection, Socket};

#[derive(Debug, Clone)]
pub struct ShaderNode {
    pub id: usize,
    pub node_type: ShaderNodeType,
    pub position: Point,
    pub inputs: Vec<Socket>,
    pub outputs: Vec<Socket>,
}

impl ShaderNode {
    pub fn new(id: usize, node_type: ShaderNodeType, position: Point) -> Self {
        Self {
            id,
            node_type,
            position,
            inputs: node_type.inputs(),
            outputs: node_type.outputs(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShaderGraph {
    pub nodes: Vec<ShaderNode>,
    pub connections: Vec<Connection>,
    next_id: usize,
}

impl Default for ShaderGraph {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            connections: Vec::new(),
            next_id: 0,
        }
    }
}

#[allow(dead_code)]
impl ShaderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node_type: ShaderNodeType, position: Point) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(ShaderNode::new(id, node_type, position));
        id
    }

    pub fn add_connection(&mut self, connection: Connection) {
        self.connections.push(connection);
    }

    pub fn remove_node(&mut self, id: usize) {
        self.nodes.retain(|n| n.id != id);
        self.connections
            .retain(|c| c.from_node != id && c.to_node != id);
    }

    pub fn get_node(&self, id: usize) -> Option<&ShaderNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: usize) -> Option<&mut ShaderNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn get_node_by_index(&self, index: usize) -> Option<&ShaderNode> {
        self.nodes.get(index)
    }

    pub fn get_node_by_index_mut(&mut self, index: usize) -> Option<&mut ShaderNode> {
        self.nodes.get_mut(index)
    }

    pub fn get_connected_input(
        &self,
        node_id: usize,
        socket_index: usize,
    ) -> Option<(usize, usize)> {
        self.connections
            .iter()
            .find(|c| c.to_node == node_id && c.to_socket == socket_index)
            .map(|c| (c.from_node, c.from_socket))
    }

    pub fn get_connections_from(&self, node_id: usize, socket_index: usize) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.from_node == node_id && c.from_socket == socket_index)
            .collect()
    }
}
