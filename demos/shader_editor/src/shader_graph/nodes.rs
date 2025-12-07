use super::sockets::{Socket, SocketType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum ShaderNodeType {
    // Inputs (10 nodes)
    UV,
    Time,
    MousePos,
    Resolution,
    CameraZoom,
    CameraPosition,
    NodePosition,
    NodeSize,
    PinPosition,
    EdgeData,

    // Math Operations (23 nodes)
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Sqrt,
    Abs,
    Min,
    Max,
    Clamp,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Floor,
    Ceil,
    Fract,
    Mod,
    Sign,
    Step,

    // Vector Operations (15 nodes)
    VecSplit2,
    VecSplit3,
    VecSplit4,
    VecCombine2,
    VecCombine3,
    VecCombine4,
    Dot,
    Cross,
    Length,
    Distance,
    Normalize,
    Reflect,
    Refract,
    Mix,
    Smoothstep,

    // Color Operations (8 nodes)
    RGBtoHSV,
    HSVtoRGB,
    ColorMix,
    Desaturate,
    Invert,
    Gamma,
    ColorRamp,
    Palette,

    // IQ 2D SDF Primitives (34 nodes)
    SDF_Circle,
    SDF_Box,
    SDF_RoundedBox,
    SDF_OrientedBox,
    SDF_Segment,
    SDF_Rhombus,
    SDF_Trapezoid,
    SDF_Parallelogram,
    SDF_EquilateralTriangle,
    SDF_IsoscelesTriangle,
    SDF_Triangle,
    SDF_UnevenCapsule,
    SDF_Pentagon,
    SDF_Hexagon,
    SDF_Octogon,
    SDF_Hexagram,
    SDF_Star5,
    SDF_Star,
    SDF_Pie,
    SDF_CutDisk,
    SDF_Arc,
    SDF_Ring,
    SDF_Horseshoe,
    SDF_Vesica,
    SDF_Moon,
    SDF_RoundedCross,
    SDF_Egg,
    SDF_Heart,
    SDF_Cross,
    SDF_RoundedX,
    SDF_Polygon,
    SDF_Ellipse,
    SDF_Parabola,
    SDF_ParabolaSegment,

    // SDF Operations (12 nodes)
    SDF_Union,
    SDF_Subtraction,
    SDF_Intersection,
    SDF_SmoothUnion,
    SDF_SmoothSubtraction,
    SDF_SmoothIntersection,
    SDF_Onion,
    SDF_Round,
    SDF_Annular,
    SDF_Extrusion,
    SDF_Revolution,
    SDF_Elongation,

    // Logic & Comparison (8 nodes)
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,

    // Outputs (5 nodes)
    OutputBackground,
    OutputNode,
    OutputPin,
    OutputEdge,
    OutputFinal,
}

impl ShaderNodeType {
    pub fn name(&self) -> &'static str {
        match self {
            // Inputs
            Self::UV => "UV",
            Self::Time => "Time",
            Self::MousePos => "Mouse Position",
            Self::Resolution => "Resolution",
            Self::CameraZoom => "Camera Zoom",
            Self::CameraPosition => "Camera Position",
            Self::NodePosition => "Node Position",
            Self::NodeSize => "Node Size",
            Self::PinPosition => "Pin Position",
            Self::EdgeData => "Edge Data",

            // Math
            Self::Add => "Add",
            Self::Sub => "Subtract",
            Self::Mul => "Multiply",
            Self::Div => "Divide",
            Self::Pow => "Power",
            Self::Sqrt => "Square Root",
            Self::Abs => "Absolute",
            Self::Min => "Minimum",
            Self::Max => "Maximum",
            Self::Clamp => "Clamp",
            Self::Sin => "Sine",
            Self::Cos => "Cosine",
            Self::Tan => "Tangent",
            Self::Asin => "Arcsine",
            Self::Acos => "Arccosine",
            Self::Atan => "Arctangent",
            Self::Atan2 => "Arctangent2",
            Self::Floor => "Floor",
            Self::Ceil => "Ceiling",
            Self::Fract => "Fractional",
            Self::Mod => "Modulo",
            Self::Sign => "Sign",
            Self::Step => "Step",

            // Vector
            Self::VecSplit2 => "Split Vec2",
            Self::VecSplit3 => "Split Vec3",
            Self::VecSplit4 => "Split Vec4",
            Self::VecCombine2 => "Combine Vec2",
            Self::VecCombine3 => "Combine Vec3",
            Self::VecCombine4 => "Combine Vec4",
            Self::Dot => "Dot Product",
            Self::Cross => "Cross Product",
            Self::Length => "Length",
            Self::Distance => "Distance",
            Self::Normalize => "Normalize",
            Self::Reflect => "Reflect",
            Self::Refract => "Refract",
            Self::Mix => "Mix",
            Self::Smoothstep => "Smoothstep",

            // Color
            Self::RGBtoHSV => "RGB to HSV",
            Self::HSVtoRGB => "HSV to RGB",
            Self::ColorMix => "Color Mix",
            Self::Desaturate => "Desaturate",
            Self::Invert => "Invert",
            Self::Gamma => "Gamma",
            Self::ColorRamp => "Color Ramp",
            Self::Palette => "Palette",

            // SDF Primitives
            Self::SDF_Circle => "Circle",
            Self::SDF_Box => "Box",
            Self::SDF_RoundedBox => "Rounded Box",
            Self::SDF_OrientedBox => "Oriented Box",
            Self::SDF_Segment => "Segment",
            Self::SDF_Rhombus => "Rhombus",
            Self::SDF_Trapezoid => "Trapezoid",
            Self::SDF_Parallelogram => "Parallelogram",
            Self::SDF_EquilateralTriangle => "Equilateral Triangle",
            Self::SDF_IsoscelesTriangle => "Isosceles Triangle",
            Self::SDF_Triangle => "Triangle",
            Self::SDF_UnevenCapsule => "Uneven Capsule",
            Self::SDF_Pentagon => "Pentagon",
            Self::SDF_Hexagon => "Hexagon",
            Self::SDF_Octogon => "Octogon",
            Self::SDF_Hexagram => "Hexagram",
            Self::SDF_Star5 => "Star 5",
            Self::SDF_Star => "Star",
            Self::SDF_Pie => "Pie",
            Self::SDF_CutDisk => "Cut Disk",
            Self::SDF_Arc => "Arc",
            Self::SDF_Ring => "Ring",
            Self::SDF_Horseshoe => "Horseshoe",
            Self::SDF_Vesica => "Vesica",
            Self::SDF_Moon => "Moon",
            Self::SDF_RoundedCross => "Rounded Cross",
            Self::SDF_Egg => "Egg",
            Self::SDF_Heart => "Heart",
            Self::SDF_Cross => "Cross",
            Self::SDF_RoundedX => "Rounded X",
            Self::SDF_Polygon => "Polygon",
            Self::SDF_Ellipse => "Ellipse",
            Self::SDF_Parabola => "Parabola",
            Self::SDF_ParabolaSegment => "Parabola Segment",

            // SDF Operations
            Self::SDF_Union => "Union",
            Self::SDF_Subtraction => "Subtraction",
            Self::SDF_Intersection => "Intersection",
            Self::SDF_SmoothUnion => "Smooth Union",
            Self::SDF_SmoothSubtraction => "Smooth Subtraction",
            Self::SDF_SmoothIntersection => "Smooth Intersection",
            Self::SDF_Onion => "Onion",
            Self::SDF_Round => "Round",
            Self::SDF_Annular => "Annular",
            Self::SDF_Extrusion => "Extrusion",
            Self::SDF_Revolution => "Revolution",
            Self::SDF_Elongation => "Elongation",

            // Logic
            Self::Equal => "Equal",
            Self::NotEqual => "Not Equal",
            Self::Less => "Less",
            Self::Greater => "Greater",
            Self::LessEqual => "Less or Equal",
            Self::GreaterEqual => "Greater or Equal",
            Self::And => "And",
            Self::Or => "Or",

            // Outputs
            Self::OutputBackground => "Background Output",
            Self::OutputNode => "Node Output",
            Self::OutputPin => "Pin Output",
            Self::OutputEdge => "Edge Output",
            Self::OutputFinal => "Final Output",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::UV
            | Self::Time
            | Self::MousePos
            | Self::Resolution
            | Self::CameraZoom
            | Self::CameraPosition
            | Self::NodePosition
            | Self::NodeSize
            | Self::PinPosition
            | Self::EdgeData => "Input",

            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Pow
            | Self::Sqrt
            | Self::Abs
            | Self::Min
            | Self::Max
            | Self::Clamp
            | Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Asin
            | Self::Acos
            | Self::Atan
            | Self::Atan2
            | Self::Floor
            | Self::Ceil
            | Self::Fract
            | Self::Mod
            | Self::Sign
            | Self::Step => "Math",

            Self::VecSplit2
            | Self::VecSplit3
            | Self::VecSplit4
            | Self::VecCombine2
            | Self::VecCombine3
            | Self::VecCombine4
            | Self::Dot
            | Self::Cross
            | Self::Length
            | Self::Distance
            | Self::Normalize
            | Self::Reflect
            | Self::Refract
            | Self::Mix
            | Self::Smoothstep => "Vector",

            Self::RGBtoHSV
            | Self::HSVtoRGB
            | Self::ColorMix
            | Self::Desaturate
            | Self::Invert
            | Self::Gamma
            | Self::ColorRamp
            | Self::Palette => "Color",

            Self::SDF_Circle
            | Self::SDF_Box
            | Self::SDF_RoundedBox
            | Self::SDF_OrientedBox
            | Self::SDF_Segment
            | Self::SDF_Rhombus
            | Self::SDF_Trapezoid
            | Self::SDF_Parallelogram
            | Self::SDF_EquilateralTriangle
            | Self::SDF_IsoscelesTriangle
            | Self::SDF_Triangle
            | Self::SDF_UnevenCapsule
            | Self::SDF_Pentagon
            | Self::SDF_Hexagon
            | Self::SDF_Octogon
            | Self::SDF_Hexagram
            | Self::SDF_Star5
            | Self::SDF_Star
            | Self::SDF_Pie
            | Self::SDF_CutDisk
            | Self::SDF_Arc
            | Self::SDF_Ring
            | Self::SDF_Horseshoe
            | Self::SDF_Vesica
            | Self::SDF_Moon
            | Self::SDF_RoundedCross
            | Self::SDF_Egg
            | Self::SDF_Heart
            | Self::SDF_Cross
            | Self::SDF_RoundedX
            | Self::SDF_Polygon
            | Self::SDF_Ellipse
            | Self::SDF_Parabola
            | Self::SDF_ParabolaSegment => "SDF",

            Self::SDF_Union
            | Self::SDF_Subtraction
            | Self::SDF_Intersection
            | Self::SDF_SmoothUnion
            | Self::SDF_SmoothSubtraction
            | Self::SDF_SmoothIntersection
            | Self::SDF_Onion
            | Self::SDF_Round
            | Self::SDF_Annular
            | Self::SDF_Extrusion
            | Self::SDF_Revolution
            | Self::SDF_Elongation => "SDF Ops",

            Self::Equal
            | Self::NotEqual
            | Self::Less
            | Self::Greater
            | Self::LessEqual
            | Self::GreaterEqual
            | Self::And
            | Self::Or => "Logic",

            Self::OutputBackground
            | Self::OutputNode
            | Self::OutputPin
            | Self::OutputEdge
            | Self::OutputFinal => "Output",
        }
    }

    pub fn inputs(&self) -> Vec<Socket> {
        match self {
            // Input nodes have no inputs
            Self::UV
            | Self::Time
            | Self::MousePos
            | Self::Resolution
            | Self::CameraZoom
            | Self::CameraPosition => vec![],

            // Math - binary operations
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Pow
            | Self::Min
            | Self::Max
            | Self::Atan2
            | Self::Mod
            | Self::Step => vec![
                Socket::new("A", SocketType::Float),
                Socket::new("B", SocketType::Float),
            ],

            // Math - unary operations
            Self::Sqrt
            | Self::Abs
            | Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Asin
            | Self::Acos
            | Self::Atan
            | Self::Floor
            | Self::Ceil
            | Self::Fract
            | Self::Sign => vec![Socket::new("Value", SocketType::Float)],

            // Clamp
            Self::Clamp => vec![
                Socket::new("Value", SocketType::Float),
                Socket::new("Min", SocketType::Float),
                Socket::new("Max", SocketType::Float),
            ],

            // Vector split
            Self::VecSplit2 => vec![Socket::new("Vector", SocketType::Vec2)],
            Self::VecSplit3 => vec![Socket::new("Vector", SocketType::Vec3)],
            Self::VecSplit4 => vec![Socket::new("Vector", SocketType::Vec4)],

            // Vector combine
            Self::VecCombine2 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
            ],
            Self::VecCombine3 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
                Socket::new("Z", SocketType::Float),
            ],
            Self::VecCombine4 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
                Socket::new("Z", SocketType::Float),
                Socket::new("W", SocketType::Float),
            ],

            // Vector operations
            Self::Dot => vec![
                Socket::new("A", SocketType::Vec2),
                Socket::new("B", SocketType::Vec2),
            ],
            Self::Length | Self::Normalize => vec![Socket::new("Vector", SocketType::Vec2)],
            Self::Distance => vec![
                Socket::new("A", SocketType::Vec2),
                Socket::new("B", SocketType::Vec2),
            ],
            Self::Mix => vec![
                Socket::new("A", SocketType::Vec4),
                Socket::new("B", SocketType::Vec4),
                Socket::new("T", SocketType::Float),
            ],
            Self::Smoothstep => vec![
                Socket::new("Edge0", SocketType::Float),
                Socket::new("Edge1", SocketType::Float),
                Socket::new("X", SocketType::Float),
            ],

            // SDF Circle
            Self::SDF_Circle => vec![
                Socket::new("Position", SocketType::Vec2),
                Socket::new("Radius", SocketType::Float).with_default("1.0"),
            ],

            // SDF Box
            Self::SDF_Box => vec![
                Socket::new("Position", SocketType::Vec2),
                Socket::new("Size", SocketType::Vec2).with_default("vec2(1.0, 1.0)"),
            ],

            // SDF Operations
            Self::SDF_Union | Self::SDF_Subtraction | Self::SDF_Intersection => vec![
                Socket::new("D1", SocketType::Float),
                Socket::new("D2", SocketType::Float),
            ],

            Self::SDF_SmoothUnion | Self::SDF_SmoothSubtraction | Self::SDF_SmoothIntersection => {
                vec![
                    Socket::new("D1", SocketType::Float),
                    Socket::new("D2", SocketType::Float),
                    Socket::new("K", SocketType::Float).with_default("0.1"),
                ]
            }

            // Outputs
            Self::OutputEdge => vec![Socket::new("Color", SocketType::Vec4)],

            // Default
            _ => vec![],
        }
    }

    pub fn outputs(&self) -> Vec<Socket> {
        match self {
            // Input nodes
            Self::UV => vec![Socket::new("UV", SocketType::Vec2)],
            Self::Time => vec![Socket::new("Time", SocketType::Float)],
            Self::MousePos => vec![Socket::new("Position", SocketType::Vec2)],
            Self::Resolution => vec![Socket::new("Resolution", SocketType::Vec2)],
            Self::CameraZoom => vec![Socket::new("Zoom", SocketType::Float)],
            Self::CameraPosition => vec![Socket::new("Position", SocketType::Vec2)],

            // Math operations
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Pow
            | Self::Min
            | Self::Max
            | Self::Atan2
            | Self::Mod
            | Self::Step
            | Self::Sqrt
            | Self::Abs
            | Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Asin
            | Self::Acos
            | Self::Atan
            | Self::Floor
            | Self::Ceil
            | Self::Fract
            | Self::Sign
            | Self::Clamp => vec![Socket::new("Result", SocketType::Float)],

            // Vector split
            Self::VecSplit2 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
            ],
            Self::VecSplit3 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
                Socket::new("Z", SocketType::Float),
            ],
            Self::VecSplit4 => vec![
                Socket::new("X", SocketType::Float),
                Socket::new("Y", SocketType::Float),
                Socket::new("Z", SocketType::Float),
                Socket::new("W", SocketType::Float),
            ],

            // Vector combine
            Self::VecCombine2 => vec![Socket::new("Vector", SocketType::Vec2)],
            Self::VecCombine3 => vec![Socket::new("Vector", SocketType::Vec3)],
            Self::VecCombine4 => vec![Socket::new("Vector", SocketType::Vec4)],

            // Vector operations
            Self::Dot | Self::Length => vec![Socket::new("Result", SocketType::Float)],
            Self::Normalize => vec![Socket::new("Vector", SocketType::Vec2)],
            Self::Distance => vec![Socket::new("Distance", SocketType::Float)],
            Self::Mix => vec![Socket::new("Result", SocketType::Vec4)],
            Self::Smoothstep => vec![Socket::new("Result", SocketType::Float)],

            // All SDF primitives output distance
            Self::SDF_Circle
            | Self::SDF_Box
            | Self::SDF_RoundedBox
            | Self::SDF_OrientedBox
            | Self::SDF_Segment
            | Self::SDF_Rhombus
            | Self::SDF_Trapezoid
            | Self::SDF_Parallelogram
            | Self::SDF_EquilateralTriangle
            | Self::SDF_IsoscelesTriangle
            | Self::SDF_Triangle
            | Self::SDF_UnevenCapsule
            | Self::SDF_Pentagon
            | Self::SDF_Hexagon
            | Self::SDF_Octogon
            | Self::SDF_Hexagram
            | Self::SDF_Star5
            | Self::SDF_Star
            | Self::SDF_Pie
            | Self::SDF_CutDisk
            | Self::SDF_Arc
            | Self::SDF_Ring
            | Self::SDF_Horseshoe
            | Self::SDF_Vesica
            | Self::SDF_Moon
            | Self::SDF_RoundedCross
            | Self::SDF_Egg
            | Self::SDF_Heart
            | Self::SDF_Cross
            | Self::SDF_RoundedX
            | Self::SDF_Polygon
            | Self::SDF_Ellipse
            | Self::SDF_Parabola
            | Self::SDF_ParabolaSegment => vec![Socket::new("Distance", SocketType::Float)],

            // SDF operations output distance
            Self::SDF_Union
            | Self::SDF_Subtraction
            | Self::SDF_Intersection
            | Self::SDF_SmoothUnion
            | Self::SDF_SmoothSubtraction
            | Self::SDF_SmoothIntersection
            | Self::SDF_Onion
            | Self::SDF_Round
            | Self::SDF_Annular => vec![Socket::new("Distance", SocketType::Float)],

            // Output nodes have no outputs
            Self::OutputBackground
            | Self::OutputNode
            | Self::OutputPin
            | Self::OutputEdge
            | Self::OutputFinal => vec![],

            // Default
            _ => vec![],
        }
    }

    /// Returns all available node types for the command palette.
    pub fn all() -> &'static [ShaderNodeType] {
        &[
            // Inputs
            Self::UV,
            Self::Time,
            Self::MousePos,
            Self::Resolution,
            Self::CameraZoom,
            Self::CameraPosition,
            Self::NodePosition,
            Self::NodeSize,
            Self::PinPosition,
            Self::EdgeData,
            // Math
            Self::Add,
            Self::Sub,
            Self::Mul,
            Self::Div,
            Self::Pow,
            Self::Sqrt,
            Self::Abs,
            Self::Min,
            Self::Max,
            Self::Clamp,
            Self::Sin,
            Self::Cos,
            Self::Tan,
            Self::Asin,
            Self::Acos,
            Self::Atan,
            Self::Atan2,
            Self::Floor,
            Self::Ceil,
            Self::Fract,
            Self::Mod,
            Self::Sign,
            Self::Step,
            // Vector
            Self::VecSplit2,
            Self::VecSplit3,
            Self::VecSplit4,
            Self::VecCombine2,
            Self::VecCombine3,
            Self::VecCombine4,
            Self::Dot,
            Self::Cross,
            Self::Length,
            Self::Distance,
            Self::Normalize,
            Self::Reflect,
            Self::Refract,
            Self::Mix,
            Self::Smoothstep,
            // Color
            Self::RGBtoHSV,
            Self::HSVtoRGB,
            Self::ColorMix,
            Self::Desaturate,
            Self::Invert,
            Self::Gamma,
            Self::ColorRamp,
            Self::Palette,
            // SDF Primitives
            Self::SDF_Circle,
            Self::SDF_Box,
            Self::SDF_RoundedBox,
            Self::SDF_OrientedBox,
            Self::SDF_Segment,
            Self::SDF_Rhombus,
            Self::SDF_Trapezoid,
            Self::SDF_Parallelogram,
            Self::SDF_EquilateralTriangle,
            Self::SDF_IsoscelesTriangle,
            Self::SDF_Triangle,
            Self::SDF_UnevenCapsule,
            Self::SDF_Pentagon,
            Self::SDF_Hexagon,
            Self::SDF_Octogon,
            Self::SDF_Hexagram,
            Self::SDF_Star5,
            Self::SDF_Star,
            Self::SDF_Pie,
            Self::SDF_CutDisk,
            Self::SDF_Arc,
            Self::SDF_Ring,
            Self::SDF_Horseshoe,
            Self::SDF_Vesica,
            Self::SDF_Moon,
            Self::SDF_RoundedCross,
            Self::SDF_Egg,
            Self::SDF_Heart,
            Self::SDF_Cross,
            Self::SDF_RoundedX,
            Self::SDF_Polygon,
            Self::SDF_Ellipse,
            Self::SDF_Parabola,
            Self::SDF_ParabolaSegment,
            // SDF Ops
            Self::SDF_Union,
            Self::SDF_Subtraction,
            Self::SDF_Intersection,
            Self::SDF_SmoothUnion,
            Self::SDF_SmoothSubtraction,
            Self::SDF_SmoothIntersection,
            Self::SDF_Onion,
            Self::SDF_Round,
            Self::SDF_Annular,
            Self::SDF_Extrusion,
            Self::SDF_Revolution,
            Self::SDF_Elongation,
            // Logic
            Self::Equal,
            Self::NotEqual,
            Self::Less,
            Self::Greater,
            Self::LessEqual,
            Self::GreaterEqual,
            Self::And,
            Self::Or,
            // Outputs
            Self::OutputBackground,
            Self::OutputNode,
            Self::OutputPin,
            Self::OutputEdge,
            Self::OutputFinal,
        ]
    }
}
