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
    /// Render group 0: draws, draw_entries, segments, styles, fine_counts,
    /// fine_slots, coarse_slots
    pub render_group0_layout: BindGroupLayout,
    /// Compute group 0: draws(read), draw_entries(read), segments(read),
    /// styles(read) + the sort/fine launch dims uniform (binding 7).
    pub compute_group0_layout: BindGroupLayout,
    /// Scatter-kernel group 1: coarse_counts(rw), coarse_slots(rw),
    /// cull_list(r), cull_meta(r). One layout for both scatter kernels; the
    /// bind group selects the list content (open triples vs closed pairs).
    pub compute_scatter_group1_layout: BindGroupLayout,
    /// Sort/fine-kernel group 1: coarse_counts(rw), coarse_slots(rw),
    /// fine_counts(rw), fine_slots(rw).
    ///
    /// Group 1 is split per kernel so every compute pipeline binds at most
    /// 4 + 4 storage buffers - the WebGPU spec-default
    /// `maxStorageBuffersPerShaderStage` (8), which wasm/WebGPU enforces.
    pub compute_sort_group1_layout: BindGroupLayout,
    _render_pipeline_layout: PipelineLayout,
    _scatter_pipeline_layout: PipelineLayout,
    _sort_pipeline_layout: PipelineLayout,
    pub render_pipeline: RenderPipeline,
    /// Scatter cull kernels (see plan/scatter-binning.md): per-open-segment
    /// scatter, per-closed-entry scatter, then per-coarse-tile sort + fine.
    pub scatter_open_pipeline: ComputePipeline,
    pub scatter_closed_pipeline: ComputePipeline,
    pub sort_fine_pipeline: ComputePipeline,
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
        let compute_scatter_group1_layout = create_compute_scatter_group1_layout(device);
        let compute_sort_group1_layout = create_compute_sort_group1_layout(device);

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Render Pipeline Layout"),
            bind_group_layouts: &[&render_group0_layout],
            ..Default::default()
        });

        let scatter_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Scatter Pipeline Layout"),
            bind_group_layouts: &[&compute_group0_layout, &compute_scatter_group1_layout],
            ..Default::default()
        });
        let sort_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Sort Pipeline Layout"),
            bind_group_layouts: &[&compute_group0_layout, &compute_sort_group1_layout],
            ..Default::default()
        });

        let render_pipeline =
            create_render_pipeline(device, format, &render_pipeline_layout, &shader_module);

        let compute = |label: &str, entry: &str, layout: &PipelineLayout| {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                module: &shader_module,
                entry_point: Some(entry),
                compilation_options: PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let scatter_open_pipeline = compute(
            "SDF Scatter Open",
            "cs_scatter_open",
            &scatter_pipeline_layout,
        );
        let scatter_closed_pipeline = compute(
            "SDF Scatter Closed",
            "cs_scatter_closed",
            &scatter_pipeline_layout,
        );
        let sort_fine_pipeline = compute("SDF Sort Fine", "cs_sort_fine", &sort_pipeline_layout);

        Self {
            _shader_module: shader_module,
            render_group0_layout,
            compute_group0_layout,
            compute_scatter_group1_layout,
            compute_sort_group1_layout,
            _render_pipeline_layout: render_pipeline_layout,
            _scatter_pipeline_layout: scatter_pipeline_layout,
            _sort_pipeline_layout: sort_pipeline_layout,
            render_pipeline,
            scatter_open_pipeline,
            scatter_closed_pipeline,
            sort_fine_pipeline,
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
            // binding 5: fine_slots (read)
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
            // binding 6: coarse_slots (read) - the fine 16-bit indices dereference here
            BindGroupLayoutEntry {
                binding: 6,
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
            // Sort/fine launch dims (live draw count, total coarse tiles):
            // a uniform, since the compute stage is at the 8-storage-buffer
            // limit. Binding 7 to stay clear of the render group's 4-6.
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(8).unwrap()),
                },
                count: None,
            },
        ],
    })
}

/// Scatter-kernel group 1: the coarse outputs (read_write) plus the read-only
/// work list and live-count meta. The draw index is read from the work list
/// items themselves.
fn create_compute_scatter_group1_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;
    let buf = |binding: u32, read_only: bool| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
        },
        count: None,
    };
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Compute Scatter Group 1"),
        entries: &[buf(0, false), buf(1, false), buf(2, true), buf(3, true)],
    })
}

/// Sort/fine-kernel group 1: the two-level index outputs, all read_write. The
/// kernel maps its flat workgroup id to (draw, coarse tile) via the
/// `cs_launch` uniform and the `coarse_base` prefix sums.
fn create_compute_sort_group1_layout(device: &Device) -> BindGroupLayout {
    use std::num::NonZeroU64;
    let buf = |binding: u32| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
        },
        count: None,
    };
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("SDF Compute Sort Group 1"),
        entries: &[buf(0), buf(1), buf(2), buf(3)],
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
