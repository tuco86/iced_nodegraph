pub mod colors;
mod input;
mod math;
mod noise;
mod output;
mod texture;
mod vector;

use iced::Theme;

/// All available node types in the shader graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeType {
    // Input nodes (sources)
    TimeInput,
    UVInput,
    NormalInput,
    PositionInput,

    // Math operations
    Add,
    Multiply,
    Divide,
    Subtract,
    Power,

    // Noise generators
    PerlinNoise,
    VoronoiNoise,
    SimplexNoise,

    // Texture operations
    Sampler2D,
    ColorMix,
    Gradient,

    // Vector operations
    VectorSplit,
    VectorCombine,
    Normalize,
    DotProduct,
    CrossProduct,

    // Output nodes
    BaseColor,
    Roughness,
    Metallic,
    Emission,
    Normal,
}

impl NodeType {
    /// Returns the primary output pin index. None for output-only nodes.
    pub fn output_pin(&self) -> Option<usize> {
        match self {
            // Input nodes: output at pin 0
            Self::TimeInput | Self::UVInput | Self::NormalInput | Self::PositionInput => Some(0),

            // Most processing nodes: output at pin 1
            Self::PerlinNoise
            | Self::VoronoiNoise
            | Self::SimplexNoise
            | Self::Add
            | Self::Multiply
            | Self::Divide
            | Self::Subtract
            | Self::Power
            | Self::VectorCombine
            | Self::Normalize
            | Self::DotProduct
            | Self::CrossProduct
            | Self::Sampler2D
            | Self::ColorMix
            | Self::Gradient
            | Self::VectorSplit => Some(1),

            // Output nodes: no output
            Self::BaseColor | Self::Roughness | Self::Metallic | Self::Emission | Self::Normal => {
                None
            }
        }
    }

    /// Returns input pin index for given slot (0=primary, 1=secondary).
    pub fn input_pin(&self, slot: usize) -> Option<usize> {
        match self {
            // Input nodes: no inputs
            Self::TimeInput | Self::UVInput | Self::NormalInput | Self::PositionInput => None,

            // Single-input nodes: slot 0 -> pin 0
            Self::PerlinNoise
            | Self::VoronoiNoise
            | Self::SimplexNoise
            | Self::Sampler2D
            | Self::Gradient
            | Self::Normalize
            | Self::VectorSplit => {
                if slot == 0 {
                    Some(0)
                } else {
                    None
                }
            }

            // Dual-input nodes: slot 0 -> pin 0, slot 1 -> pin 2
            Self::Add
            | Self::Multiply
            | Self::Divide
            | Self::Subtract
            | Self::Power
            | Self::ColorMix
            | Self::DotProduct
            | Self::CrossProduct => match slot {
                0 => Some(0),
                1 => Some(2),
                _ => None,
            },

            // VectorCombine: x=0, y=2, z=3
            Self::VectorCombine => match slot {
                0 => Some(0),
                1 => Some(2),
                2 => Some(3),
                _ => None,
            },

            // Output nodes: single input at pin 0
            Self::BaseColor | Self::Roughness | Self::Metallic | Self::Emission | Self::Normal => {
                if slot == 0 {
                    Some(0)
                } else {
                    None
                }
            }
        }
    }

    /// Creates a node element for this node type.
    pub fn create_node<'a, Message>(&self, theme: &'a Theme) -> iced::Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        match self {
            // Input
            Self::TimeInput => input::time_input_node(theme),
            Self::UVInput => input::uv_input_node(theme),
            Self::NormalInput => input::normal_input_node(theme),
            Self::PositionInput => input::position_input_node(theme),

            // Math
            Self::Add => math::add_node(theme),
            Self::Multiply => math::multiply_node(theme),
            Self::Divide => math::divide_node(theme),
            Self::Subtract => math::subtract_node(theme),
            Self::Power => math::power_node(theme),

            // Noise
            Self::PerlinNoise => noise::perlin_noise_node(theme),
            Self::VoronoiNoise => noise::voronoi_noise_node(theme),
            Self::SimplexNoise => noise::simplex_noise_node(theme),

            // Texture
            Self::Sampler2D => texture::sampler2d_node(theme),
            Self::ColorMix => texture::color_mix_node(theme),
            Self::Gradient => texture::gradient_node(theme),

            // Vector
            Self::VectorSplit => vector::vector_split_node(theme),
            Self::VectorCombine => vector::vector_combine_node(theme),
            Self::Normalize => vector::normalize_node(theme),
            Self::DotProduct => vector::dot_product_node(theme),
            Self::CrossProduct => vector::cross_product_node(theme),

            // Output
            Self::BaseColor => output::base_color_node(theme),
            Self::Roughness => output::roughness_node(theme),
            Self::Metallic => output::metallic_node(theme),
            Self::Emission => output::emission_node(theme),
            Self::Normal => output::normal_output_node(theme),
        }
    }
}
