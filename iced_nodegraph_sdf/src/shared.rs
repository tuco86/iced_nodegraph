//! Shared GPU resources for segment-based SDF rendering.
//!
//! Uses lazy initialization: OnceLock on native, thread_local on WASM.

use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, ComputePipeline, ComputePipelineDescriptor,
    Device, FragmentState, FrontFace, MultisampleState, PipelineCompilationOptions, PipelineLayout,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline,
    RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    TextureFormat, VertexState,
};

use crate::pipeline::types;

#[cfg(not(target_arch = "wasm32"))]
static SHARED_RESOURCES: std::sync::OnceLock<Arc<SharedSdfResources>> = std::sync::OnceLock::new();

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SHARED_RESOURCES: std::cell::RefCell<Option<Arc<SharedSdfResources>>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) struct SharedSdfResources {
    _shader_module: ShaderModule,
    /// Render group 0: draws, draw_entries, segments, styles, tile_counts, tile_entries
    pub render_group0_layout: BindGroupLayout,
    /// Compute group 0: draw_entries(read), segments(read), styles(read)
    pub compute_group0_layout: BindGroupLayout,
    /// Compute group 1: uniforms, tile_counts(rw), tile_entries(rw)
    pub compute_group1_layout: BindGroupLayout,
    _render_pipeline_layout: PipelineLayout,
    _compute_pipeline_layout: PipelineLayout,
    pub render_pipeline: RenderPipeline,
    pub compute_pipeline: ComputePipeline,
}

impl SharedSdfResources {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_or_init(device: &Device, format: TextureFormat) -> Arc<Self> {
        SHARED_RESOURCES
            .get_or_init(|| Arc::new(Self::new(device, format)))
            .clone()
    }

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

    fn new(device: &Device, format: TextureFormat) -> Self {
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("iced_nodegraph_sdf_shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "pipeline/shader.wgsl"
            ))),
        });

        let render_group0_layout = create_render_group0_layout(device);
        let compute_group0_layout = create_compute_group0_layout(device);
        let compute_group1_layout = create_compute_group1_layout(device);

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Render Pipeline Layout"),
            bind_group_layouts: &[&render_group0_layout],
            ..Default::default()
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Compute Pipeline Layout"),
            bind_group_layouts: &[&compute_group0_layout, &compute_group1_layout],
            ..Default::default()
        });

        let render_pipeline =
            create_render_pipeline(device, format, &render_pipeline_layout, &shader_module);

        let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("SDF Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &shader_module,
            entry_point: Some("cs_build_index"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            _shader_module: shader_module,
            render_group0_layout,
            compute_group0_layout,
            compute_group1_layout,
            _render_pipeline_layout: render_pipeline_layout,
            _compute_pipeline_layout: compute_pipeline_layout,
            render_pipeline,
            compute_pipeline,
        }
    }
}

/// Render group 0: draws + draw_entries + segments + styles + tile_counts(read) + tile_entries(read)
fn create_render_group0_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Render Group 0"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::DrawData as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuDrawEntry as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuSegment as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuStyle as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            // binding 4: tile_counts (read)
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                count: None,
            },
            // binding 5: tile_entries (read)
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                count: None,
            },
        ],
    })
}

/// Compute group 0: draws(read) + draw_entries(read) + segments(read) + styles(read)
/// Shares the same data buffers as render group 0 (read-only access).
fn create_compute_group0_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Compute Group 0"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::DrawData as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuDrawEntry as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuSegment as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::GpuStyle as ShaderSize>::SHADER_SIZE.get())
                            .unwrap(),
                    ),
                },
                count: None,
            },
        ],
    })
}

/// Compute group 1: tile_counts(rw) + tile_entries(rw). The draw index is read
/// from the dispatch z-axis (`workgroup_id.z`), so no per-draw uniform is bound.
fn create_compute_group1_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Compute Group 1"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                count: None,
            },
        ],
    })
}

fn create_render_pipeline(
    device: &Device,
    format: TextureFormat,
    layout: &PipelineLayout,
    module: &ShaderModule,
) -> RenderPipeline {
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
            front_face: FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: PolygonMode::Fill,
            ..Default::default()
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
            targets: &[Some(ColorTargetState {
                format,
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}
