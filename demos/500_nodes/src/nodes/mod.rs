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
