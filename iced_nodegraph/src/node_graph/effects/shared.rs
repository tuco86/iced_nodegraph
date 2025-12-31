//! Shared GPU resources for all NodeGraph primitives.
//!
//! Uses `OnceLock<Arc<...>>` pattern to ensure shader module and render pipelines
//! are created exactly once and shared across all primitive types (Grid, Node, Edges).

use std::sync::{Arc, OnceLock};

use encase::ShaderSize;
use iced::wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace,
    MultisampleState, PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor,
    ShaderModule, ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat, VertexState,
};

use super::pipeline::types;

/// Global shared resources instance.
/// Created once on first use, then shared across all NodeGraph primitives.
static SHARED_RESOURCES: OnceLock<Arc<SharedNodeGraphResources>> = OnceLock::new();

/// Shared GPU resources for all NodeGraph primitives.
///
/// Contains the compiled shader module, bind group layouts, and render pipelines
/// that are shared between GridPrimitive, NodePrimitive, and EdgesPrimitive.
#[allow(dead_code)]
pub struct SharedNodeGraphResources {
    /// Compiled shader module containing all entry points.
    pub shader_module: ShaderModule,

    /// Bind group layout shared by all pipelines.
    pub bind_group_layout: BindGroupLayout,

    /// Pipeline layout shared by all render pipelines.
    pub pipeline_layout: PipelineLayout,

    // Render pipelines for each primitive pass
    /// Background grid (fullscreen)
    pub grid_pipeline: RenderPipeline,
    /// Node fill + shadow (before widgets)
    pub node_fill_pipeline: RenderPipeline,
    /// Node border (after widgets)
    pub node_border_pipeline: RenderPipeline,
    /// Pin indicators
    pub pin_pipeline: RenderPipeline,
    /// Edge rendering
    pub edge_pipeline: RenderPipeline,
    /// Dragging edge / box select / edge cutting
    pub dragging_pipeline: RenderPipeline,
}

impl SharedNodeGraphResources {
    /// Get or initialize the shared GPU resources.
    ///
    /// This is called by each Pipeline::new() implementation. The first call
    /// creates the resources; subsequent calls return the cached Arc.
    pub fn get_or_init(device: &Device, format: TextureFormat) -> Arc<Self> {
        SHARED_RESOURCES
            .get_or_init(|| Arc::new(Self::new(device, format)))
            .clone()
    }

    /// Create all shared GPU resources.
    fn new(device: &Device, format: TextureFormat) -> Self {
        // Compile shader module once
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("nodegraph_shared_shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "pipeline/shader.wgsl"
            ))),
        });

        // Create bind group layout (shared by all pipelines)
        let bind_group_layout = create_bind_group_layout(device);

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("NodeGraph Shared Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        // Create all render pipelines
        let grid_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_background",
            "fs_background",
            "grid",
        );

        let node_fill_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_node",
            "fs_node_fill",
            "node_fill",
        );

        let node_border_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_node",
            "fs_node",
            "node_border",
        );

        let pin_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_pin",
            "fs_pin",
            "pins",
        );

        let edge_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_edge",
            "fs_edge",
            "edges",
        );

        let dragging_pipeline = create_render_pipeline(
            device,
            format,
            &pipeline_layout,
            &shader_module,
            "vs_dragging",
            "fs_dragging",
            "dragging",
        );

        Self {
            shader_module,
            bind_group_layout,
            pipeline_layout,
            grid_pipeline,
            node_fill_pipeline,
            node_border_pipeline,
            pin_pipeline,
            edge_pipeline,
            dragging_pipeline,
        }
    }
}

/// Create the bind group layout shared by all NodeGraph pipelines.
fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("NodeGraph Shared Bind Group Layout"),
        entries: &[
            // Binding 0: Uniforms (uniform buffer)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(<types::Uniforms as ShaderSize>::SHADER_SIZE),
                },
                count: None,
            },
            // Binding 1: Nodes (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Node as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Node SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
            // Binding 2: Pins (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Pin as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Pin SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
            // Binding 3: Edges (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Edge as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Edge SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
        ],
    })
}

/// Create a render pipeline with the given vertex/fragment entry points.
fn create_render_pipeline(
    device: &Device,
    format: TextureFormat,
    layout: &PipelineLayout,
    module: &ShaderModule,
    vs_entry: &str,
    fs_entry: &str,
    label: &str,
) -> RenderPipeline {
    let fragment_targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: VertexState {
            module,
            entry_point: Some(vs_entry),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(FragmentState {
            module,
            entry_point: Some(fs_entry),
            targets: &fragment_targets,
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}
