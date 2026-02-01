//! Shared GPU resources for SDF rendering.
//!
//! Uses lazy initialization to ensure shader module and render pipeline
//! are created exactly once.
//!
//! On native: Uses `OnceLock<Arc<...>>` for thread-safe global storage.
//! On WASM: Uses `thread_local!` because WGPU types contain JsValue which is not Send+Sync.

use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace,
    MultisampleState, PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor,
    ShaderModule, ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat, VertexState,
};

use crate::pipeline::types;

// Native: Use OnceLock for thread-safe global storage
#[cfg(not(target_arch = "wasm32"))]
static SHARED_RESOURCES: std::sync::OnceLock<Arc<SharedSdfResources>> = std::sync::OnceLock::new();

// WASM: Use thread_local because WGPU types contain JsValue (not Send+Sync)
#[cfg(target_arch = "wasm32")]
thread_local! {
    static SHARED_RESOURCES: std::cell::RefCell<Option<Arc<SharedSdfResources>>> =
        const { std::cell::RefCell::new(None) };
}

/// Shared GPU resources for SDF rendering.
#[allow(dead_code)]
pub struct SharedSdfResources {
    /// Compiled shader module.
    pub shader_module: ShaderModule,

    /// Bind group layout for all SDF pipelines.
    pub bind_group_layout: BindGroupLayout,

    /// Pipeline layout.
    pub pipeline_layout: PipelineLayout,

    /// Main render pipeline.
    pub render_pipeline: RenderPipeline,
}

impl SharedSdfResources {
    /// Get or initialize the shared GPU resources.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_or_init(device: &Device, format: TextureFormat) -> Arc<Self> {
        SHARED_RESOURCES
            .get_or_init(|| Arc::new(Self::new(device, format)))
            .clone()
    }

    /// Get or initialize the shared GPU resources (WASM version).
    #[cfg(target_arch = "wasm32")]
    pub fn get_or_init(device: &Device, format: TextureFormat) -> Arc<Self> {
        SHARED_RESOURCES.with(|cell| {
            let mut opt = cell.borrow_mut();
            if opt.is_none() {
                *opt = Some(Arc::new(Self::new(device, format)));
            }
            opt.as_ref().unwrap().clone()
        })
    }

    /// Create all shared GPU resources.
    fn new(device: &Device, format: TextureFormat) -> Self {
        // Compile shader module
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("iced_sdf_shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "pipeline/shader.wgsl"
            ))),
        });

        // Create bind group layout
        let bind_group_layout = create_bind_group_layout(device);

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        // Create render pipeline
        let render_pipeline =
            create_render_pipeline(device, format, &pipeline_layout, &shader_module);

        Self {
            shader_module,
            bind_group_layout,
            pipeline_layout,
            render_pipeline,
        }
    }
}

/// Create the bind group layout for SDF rendering.
fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Bind Group Layout"),
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
            // Binding 1: SDF Operations (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::SdfOp as ShaderSize>::SHADER_SIZE.get() * 4)
                            .expect("SdfOp SHADER_SIZE * 4 must be non-zero"),
                    ),
                },
                count: None,
            },
            // Binding 2: Layers (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::SdfLayer as ShaderSize>::SHADER_SIZE.get())
                            .expect("SdfLayer SHADER_SIZE must be non-zero"),
                    ),
                },
                count: None,
            },
        ],
    })
}

/// Create the render pipeline.
fn create_render_pipeline(
    device: &Device,
    format: TextureFormat,
    layout: &PipelineLayout,
    module: &ShaderModule,
) -> RenderPipeline {
    let fragment_targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("SDF Render Pipeline"),
        layout: Some(layout),
        vertex: VertexState {
            module,
            entry_point: Some("vs_main"),
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
            entry_point: Some("fs_main"),
            targets: &fragment_targets,
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}
