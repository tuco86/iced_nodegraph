use iced::Color;

use crate::colors;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum SocketType {
    Float,
    Vec2,
    Vec3,
    Vec4,
    Bool,
    Int,
}

#[allow(dead_code)]
impl SocketType {
    pub fn wgsl_type(&self) -> &'static str {
        match self {
            SocketType::Float => "f32",
            SocketType::Vec2 => "vec2<f32>",
            SocketType::Vec3 => "vec3<f32>",
            SocketType::Vec4 => "vec4<f32>",
            SocketType::Bool => "bool",
            SocketType::Int => "i32",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            SocketType::Float => colors::SOCKET_FLOAT,
            SocketType::Vec2 => colors::SOCKET_VEC2,
            SocketType::Vec3 => colors::SOCKET_VEC3,
            SocketType::Vec4 => colors::SOCKET_VEC4,
            SocketType::Bool => colors::SOCKET_BOOL,
            SocketType::Int => colors::SOCKET_INT,
        }
    }

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
