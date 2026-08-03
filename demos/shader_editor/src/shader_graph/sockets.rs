#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocketType {
    Float,
    Vec2,
    Vec3,
    Vec4,
}

impl SocketType {
    pub fn can_connect_to(&self, other: &SocketType) -> bool {
        self == other
    }
}

#[derive(Debug, Clone)]
pub struct Socket {
    pub name: String,
    pub socket_type: SocketType,
    pub default_value: Option<String>,
}

impl Socket {
    pub fn new(name: impl Into<String>, socket_type: SocketType) -> Self {
        Self {
            name: name.into(),
            socket_type,
            default_value: None,
        }
    }

    pub fn with_default(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub from_node: usize,
    pub from_socket: usize,
    pub to_node: usize,
    pub to_socket: usize,
}
