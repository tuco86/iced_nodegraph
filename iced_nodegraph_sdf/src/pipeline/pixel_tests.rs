//! Headless pixel-level tests for SDF rendering.
//!
//! Renders predefined shapes to an offscreen texture and checks specific pixels.
//! Catches tile culling bugs, sign leaks, and pattern artifacts.

#![cfg(test)]

use std::sync::{Mutex, MutexGuard, OnceLock};

use encase::{ShaderSize, ShaderType, StorageBuffer, UniformBuffer, internal::WriteInto};
use wgpu::util::DeviceExt;
use wgpu::*;

use crate::compile::compile_drawable;
use crate::curve::Curve;
use crate::pattern::Pattern;
use crate::pipeline::types::*;
use crate::style::Style;

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 128;
const SLOT_STRIDE: u32 = MAX_SLOTS_PER_TILE * 2;

/// One significant tiled-vs-untiled pixel mismatch: `(x, y, tiled, untiled, delta)`.
type PixelDiff = (u32, u32, [u8; 4], [u8; 4], i32);

/// Shared headless renderer for all pixel tests.
///
/// A real application owns exactly one wgpu device (iced creates it once); every
/// `SdfPrimitive` draws through it. The tests mirror that: one device, created
/// lazily and reused, rather than a fresh `Instance`/`Adapter`/`Device` per test.
/// Creating many independent devices concurrently (the default `cargo test`
/// thread pool) deadlocks some drivers; sharing one and serializing GPU work
/// behind the mutex removes that footgun so the suite runs under the default
/// parallel runner. The lock is poison-tolerant so one failing test does not
/// cascade into the rest.
fn shared_renderer() -> MutexGuard<'static, TestRenderer> {
    static SHARED: OnceLock<Mutex<TestRenderer>> = OnceLock::new();
    SHARED
        .get_or_init(|| Mutex::new(TestRenderer::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Headless SDF renderer for pixel-level testing.
struct TestRenderer {
    device: Device,
    queue: Queue,
    render_pipeline: RenderPipeline,
    compute_pipeline: ComputePipeline,
    render_group0_layout: BindGroupLayout,
    compute_group0_layout: BindGroupLayout,
    compute_group1_layout: BindGroupLayout,
}

impl TestRenderer {
    fn new() -> Self {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No GPU adapter found");

        // Request TIMESTAMP_QUERY when the adapter supports it (R3): lets the
        // GPU-work measurement isolate compute+render time from CPU/submit
        // overhead. Absent (e.g. on WASM/WebGPU) the tests fall back to
        // wall-clock, per the plan's R3 note.
        let timestamps = adapter.features().contains(Features::TIMESTAMP_QUERY);
        let required_features = if timestamps {
            Features::TIMESTAMP_QUERY
        } else {
            Features::empty()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("sdf_test_device"),
            required_features,
            required_limits: Limits::default(),
            ..Default::default()
        }))
        .expect("Failed to create device");

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sdf_test_shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let render_group0_layout = Self::create_render_layout(&device);
        let compute_group0_layout = Self::create_compute_layout0(&device);
        let compute_group1_layout = Self::create_compute_layout1(&device);

        let render_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&render_group0_layout],
            ..Default::default()
        });
        let compute_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&compute_group0_layout, &compute_group1_layout],
            ..Default::default()
        });

        let format = TextureFormat::Rgba8Unorm;
        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_layout),
            vertex: VertexState {
                module: &shader,
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
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
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
        });
        let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: None,
            layout: Some(&compute_layout),
            module: &shader,
            entry_point: Some("cs_build_index"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            device,
            queue,
            render_pipeline,
            compute_pipeline,
            render_group0_layout,
            compute_group0_layout,
            compute_group1_layout,
        }
    }

    /// Render drawables to an RGBA pixel buffer.
    /// Camera is centered at origin.
    /// When `use_tiles` is false, grid_cols=0 forces the fallback path (no spatial index).
    fn render_opts(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        use_tiles: bool,
    ) -> Vec<[u8; 4]> {
        self.render_full(drawables, width, height, zoom, 1.0, use_tiles)
    }

    /// Render with non-zero `bounds_origin` (logical pixels). The tile grid
    /// covers `[bounds_origin, bounds_origin + (grid_w, grid_h)]` and the
    /// caller is responsible for adjusting `camera_position` so world coords
    /// land at the intended screen pixels.
    #[allow(clippy::too_many_arguments)]
    fn render_with_origin(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        bounds_origin_logical: [f32; 2],
        grid_w: u32,
        grid_h: u32,
        camera_position: [f32; 2],
    ) -> Vec<[u8; 4]> {
        let mut gpu_segments = Vec::new();
        let mut gpu_entries = Vec::new();
        let mut gpu_styles = Vec::new();

        for (i, (drawable, style)) in drawables.iter().enumerate() {
            let seg_offset = gpu_segments.len() as u32;
            let (mut entry, gpu_style) =
                compile_drawable(drawable, style, i as u32, 0, &mut gpu_segments);
            entry.style_idx = gpu_styles.len() as u32;
            entry.segment_start = seg_offset;
            gpu_entries.push(entry);
            gpu_styles.push(gpu_style);
        }

        let scale = 1.0_f32;
        let grid_cols = (grid_w as f32 / TILE_SIZE).ceil() as u32;
        let grid_rows = (grid_h as f32 / TILE_SIZE).ceil() as u32;
        let total_tiles = grid_cols * grid_rows;

        let draw_data = DrawData {
            bounds_origin: GpuVec2::new(
                bounds_origin_logical[0] * scale,
                bounds_origin_logical[1] * scale,
            ),
            camera_position: GpuVec2::new(camera_position[0], camera_position[1]),
            camera_zoom: zoom,
            scale_factor: scale,
            time: 0.0,
            debug_flags: 0,
            entry_count: gpu_entries.len() as u32,
            entry_start: 0,
            grid_cols,
            grid_rows,
            tile_base: 0,
            _pad0: 0,
            mouse_px: GpuVec2::ZERO,
        };

        self.execute_render(
            &gpu_entries,
            &gpu_segments,
            &gpu_styles,
            draw_data,
            total_tiles,
            width,
            height,
            grid_cols,
            grid_rows,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_render(
        &self,
        gpu_entries: &[crate::pipeline::types::GpuDrawEntry],
        gpu_segments: &[crate::pipeline::types::GpuSegment],
        gpu_styles: &[crate::pipeline::types::GpuStyle],
        draw_data: DrawData,
        total_tiles: u32,
        width: u32,
        height: u32,
        grid_cols: u32,
        grid_rows: u32,
    ) -> Vec<[u8; 4]> {
        let draws_buf = self.create_storage(&[draw_data]);
        let entries_buf = self.create_storage(gpu_entries);
        let segments_buf = self.create_storage(gpu_segments);
        let styles_buf = self.create_storage(gpu_styles);

        let tile_counts_buf = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (total_tiles.max(1) as u64) * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tile_slots_buf = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (total_tiles.max(1) as u64) * (SLOT_STRIDE as u64) * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_uniforms = ComputeUniforms {
            draw_index: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let cu_buf = self.create_uniform(&compute_uniforms);

        let render_bg = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.render_group0_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: draws_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: entries_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: segments_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: styles_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: tile_counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: tile_slots_buf.as_entire_binding(),
                },
            ],
        });
        let compute_bg0 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group0_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: draws_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: entries_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: segments_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: styles_buf.as_entire_binding(),
                },
            ],
        });
        let compute_bg1 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group1_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: cu_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: tile_counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: tile_slots_buf.as_entire_binding(),
                },
            ],
        });

        let texture = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());

        let row_bytes = width * 4;
        let padded_row = (row_bytes + 255) & !255;
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (padded_row * height) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        if grid_cols > 0 && grid_rows > 0 {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &compute_bg0, &[]);
            pass.set_bind_group(1, &compute_bg1, &[]);
            pass.dispatch_workgroups(grid_cols.div_ceil(16), grid_rows.div_ceil(16), 1);
        }
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &render_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let sub_idx = self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(sub_idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        let data = slice.get_mapped_range();
        let mut pixels = vec![[0u8; 4]; (width * height) as usize];
        for y in 0..height {
            let src_offset = (y * padded_row) as usize;
            let dst_offset = (y * width) as usize;
            for x in 0..width as usize {
                let i = src_offset + x * 4;
                pixels[dst_offset + x] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
            }
        }
        pixels
    }

    /// Run ONLY the compute cull for `drawables` and read its two output buffers
    /// (the spatial index) straight back, so a test can inspect what the two-level
    /// cull placed in each tile - the validation step BEFORE pixels. Camera is
    /// `camera_position` (world); the tile grid covers the whole viewport. The
    /// drawables are world-baked (translate `(0,0)`), so the slot's second u32 is
    /// the draw-entry index = the index into `drawables` - which is what lets a
    /// test ask "did edge N reach any tile?".
    /// Run the PRODUCTION compute cull and read its spatial index straight back.
    fn cull_readback(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        camera_position: [f32; 2],
    ) -> CullReadback {
        self.cull_readback_with(
            &self.compute_pipeline,
            MAX_SLOTS_PER_TILE,
            drawables,
            width,
            height,
            zoom,
            camera_position,
        )
    }

    /// Run ONLY the compute cull with an explicit `compute_pipeline` and per-tile
    /// `slots_per_tile`, reading its two output buffers straight back. The pipeline
    /// is a parameter so a test can sweep `MAX_SLOTS_PER_TILE` (recompiled into a
    /// variant pipeline via [`compute_pipeline_capped`]) and measure when the cull
    /// stops dropping entries.
    #[allow(clippy::too_many_arguments)]
    fn cull_readback_with(
        &self,
        compute_pipeline: &ComputePipeline,
        slots_per_tile: u32,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        camera_position: [f32; 2],
    ) -> CullReadback {
        let mut gpu_segments = Vec::new();
        let mut gpu_entries = Vec::new();
        let mut gpu_styles = Vec::new();
        for (i, (drawable, style)) in drawables.iter().enumerate() {
            let seg_offset = gpu_segments.len() as u32;
            let (mut entry, gpu_style) =
                compile_drawable(drawable, style, i as u32, 0, &mut gpu_segments);
            entry.style_idx = gpu_styles.len() as u32;
            entry.segment_start = seg_offset;
            gpu_entries.push(entry);
            gpu_styles.push(gpu_style);
        }

        let grid_cols = (width as f32 / TILE_SIZE).ceil() as u32;
        let grid_rows = (height as f32 / TILE_SIZE).ceil() as u32;
        let total_tiles = (grid_cols * grid_rows).max(1);

        let draw_data = DrawData {
            bounds_origin: GpuVec2::ZERO,
            camera_position: GpuVec2::new(camera_position[0], camera_position[1]),
            camera_zoom: zoom,
            scale_factor: 1.0,
            time: 0.0,
            debug_flags: 0,
            entry_count: gpu_entries.len() as u32,
            entry_start: 0,
            grid_cols,
            grid_rows,
            tile_base: 0,
            _pad0: 0,
            mouse_px: GpuVec2::ZERO,
        };

        let draws_buf = self.create_storage(&[draw_data]);
        let entries_buf = self.create_storage(&gpu_entries);
        let segments_buf = self.create_storage(&gpu_segments);
        let styles_buf = self.create_storage(&gpu_styles);

        let slot_stride = slots_per_tile * 2;
        let counts_bytes = total_tiles as u64 * 4;
        let slots_bytes = total_tiles as u64 * slot_stride as u64 * 4;
        let usage = BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC;
        let tile_counts_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("cull_counts"),
            size: counts_bytes,
            usage,
            mapped_at_creation: false,
        });
        let tile_slots_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("cull_slots"),
            size: slots_bytes,
            usage,
            mapped_at_creation: false,
        });

        let cu_buf = self.create_uniform(&ComputeUniforms {
            draw_index: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        });
        let compute_bg0 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group0_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: draws_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: entries_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: segments_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: styles_buf.as_entire_binding(),
                },
            ],
        });
        let compute_bg1 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group1_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: cu_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: tile_counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: tile_slots_buf.as_entire_binding(),
                },
            ],
        });

        let counts_rb = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: counts_bytes,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let slots_rb = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: slots_bytes,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(compute_pipeline);
            pass.set_bind_group(0, &compute_bg0, &[]);
            pass.set_bind_group(1, &compute_bg1, &[]);
            pass.dispatch_workgroups(grid_cols.div_ceil(16), grid_rows.div_ceil(16), 1);
        }
        encoder.copy_buffer_to_buffer(&tile_counts_buf, 0, &counts_rb, 0, counts_bytes);
        encoder.copy_buffer_to_buffer(&tile_slots_buf, 0, &slots_rb, 0, slots_bytes);
        let idx = self.queue.submit(Some(encoder.finish()));

        let cs = counts_rb.slice(..);
        let ss = slots_rb.slice(..);
        cs.map_async(MapMode::Read, |_| {});
        ss.map_async(MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        let counts: Vec<u32> = bytemuck::cast_slice(&cs.get_mapped_range()).to_vec();
        let slots: Vec<[u32; 2]> = bytemuck::cast_slice::<_, u32>(&ss.get_mapped_range())
            .chunks_exact(2)
            .map(|c| [c[0], c[1]])
            .collect();
        counts_rb.unmap();
        slots_rb.unmap();

        CullReadback {
            grid_cols,
            grid_rows,
            slots_per_tile,
            counts,
            slots,
        }
    }

    /// A compute pipeline identical to the production cull but with
    /// `MAX_SLOTS_PER_TILE` (and the derived `SLOT_STRIDE`) recompiled to `cap`,
    /// so a test can sweep the per-tile slot budget. Only the two `const` lines are
    /// rewritten; the slot arrays size off `MAX_SLOTS_PER_TILE`, so they follow.
    fn compute_pipeline_capped(&self, cap: u32) -> ComputePipeline {
        let src = with_u32_const(include_str!("shader.wgsl"), "MAX_SLOTS_PER_TILE", cap);
        let src = with_u32_const(&src, "SLOT_STRIDE", cap * 2);
        let module = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sdf_cull_capped"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Owned(src)),
        });
        let layout = self
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&self.compute_group0_layout, &self.compute_group1_layout],
                ..Default::default()
            });
        self.device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("sdf_cull_capped"),
                layout: Some(&layout),
                module: &module,
                entry_point: Some("cs_build_index"),
                compilation_options: PipelineCompilationOptions::default(),
                cache: None,
            })
    }

    fn render_full(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        scale: f32,
        use_tiles: bool,
    ) -> Vec<[u8; 4]> {
        self.render_full_t(
            drawables, width, height, zoom, scale, use_tiles, 0.0, None, false,
        )
    }

    /// Like [`render_full`] but with an explicit animation `time` (so animated
    /// scenes can be pinned to a fixed value for deterministic diffing) and an
    /// optional world-space `camera` override (so far-from-origin scenes can be
    /// brought into view). `camera` is `camera_position`; `None` auto-centers
    /// world origin at the viewport center.
    #[allow(clippy::too_many_arguments)]
    fn render_full_t(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        scale: f32,
        use_tiles: bool,
        time: f32,
        camera: Option<[f32; 2]>,
        localize: bool,
    ) -> Vec<[u8; 4]> {
        // Compile Rust -> GPU data
        let mut gpu_segments = Vec::new();
        let mut gpu_entries = Vec::new();
        let mut gpu_styles = Vec::new();

        for (i, (drawable, style)) in drawables.iter().enumerate() {
            let seg_offset = gpu_segments.len() as u32;
            // When localizing (the v3 keystone), store geometry in a frame
            // around the drawable's bounds-center and carry that origin as the
            // per-segment translate. Tilings have no segments, so leave them.
            let origin = if localize && drawable.segment_count() > 0 {
                let b = drawable.bounds();
                [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5]
            } else {
                [0.0, 0.0]
            };
            let (mut entry, gpu_style) = crate::compile::compile_drawable_at(
                drawable,
                style,
                i as u32,
                origin,
                0,
                &mut gpu_segments,
            );
            entry.style_idx = gpu_styles.len() as u32;
            // Fix segment_start: compile uses segment_base=0, offset is already correct
            entry.segment_start = seg_offset;
            gpu_entries.push(entry);
            gpu_styles.push(gpu_style);
        }

        let (grid_cols, grid_rows, total_tiles) = if use_tiles {
            let c = (width as f32 / TILE_SIZE).ceil() as u32;
            let r = (height as f32 / TILE_SIZE).ceil() as u32;
            (c, r, c * r)
        } else {
            (0, 0, 1) // Fallback path: no spatial index
        };

        let cs = zoom * scale;
        let [cam_x, cam_y] =
            camera.unwrap_or([(width as f32) * 0.5 / cs, (height as f32) * 0.5 / cs]);

        let draw_data = DrawData {
            bounds_origin: GpuVec2::new(0.0, 0.0),
            camera_position: GpuVec2::new(cam_x, cam_y),
            camera_zoom: zoom,
            scale_factor: scale,
            time,
            debug_flags: 0,
            entry_count: gpu_entries.len() as u32,
            entry_start: 0,
            grid_cols,
            grid_rows,
            tile_base: 0,
            _pad0: 0,
            mouse_px: GpuVec2::ZERO,
        };

        // Encode to GPU format via encase
        let draws_buf = self.create_storage(&[draw_data]);
        let entries_buf = self.create_storage(&gpu_entries);
        let segments_buf = self.create_storage(&gpu_segments);
        let styles_buf = self.create_storage(&gpu_styles);

        let tile_counts_buf = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (total_tiles as u64) * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tile_slots_buf = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (total_tiles as u64) * (SLOT_STRIDE as u64) * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_uniforms = ComputeUniforms {
            draw_index: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let cu_buf = self.create_uniform(&compute_uniforms);

        // Bind groups
        let render_bg = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.render_group0_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: draws_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: entries_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: segments_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: styles_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: tile_counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: tile_slots_buf.as_entire_binding(),
                },
            ],
        });
        let compute_bg0 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group0_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: draws_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: entries_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: segments_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: styles_buf.as_entire_binding(),
                },
            ],
        });
        let compute_bg1 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group1_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: cu_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: tile_counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: tile_slots_buf.as_entire_binding(),
                },
            ],
        });

        // Render target
        let texture = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());

        // Readback buffer
        let row_bytes = width * 4;
        let padded_row = (row_bytes + 255) & !255; // align to 256
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (padded_row * height) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Execute
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());

        // Compute pass
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &compute_bg0, &[]);
            pass.set_bind_group(1, &compute_bg1, &[]);
            pass.dispatch_workgroups(grid_cols.div_ceil(16), grid_rows.div_ceil(16), 1);
        }

        // Render pass
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &render_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // Copy texture to readback buffer
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let sub_idx = self.queue.submit(std::iter::once(encoder.finish()));

        // Map and read
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(sub_idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();

        let data = slice.get_mapped_range();
        let mut pixels = vec![[0u8; 4]; (width * height) as usize];
        for y in 0..height {
            let src_offset = (y * padded_row) as usize;
            let dst_offset = (y * width) as usize;
            for x in 0..width as usize {
                let i = src_offset + x * 4;
                pixels[dst_offset + x] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
            }
        }
        drop(data);
        readback.unmap();

        pixels
    }

    fn render(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
    ) -> Vec<[u8; 4]> {
        self.render_opts(drawables, width, height, zoom, true)
    }

    fn pixel_at(pixels: &[[u8; 4]], width: u32, x: u32, y: u32) -> [u8; 4] {
        pixels[(y * width + x) as usize]
    }

    /// Render with fully explicit tile-grid placement: `bounds_origin` and
    /// `grid` are in physical pixels, `camera` in world units. Lets a test move
    /// the tile grid independently of the rendered content.
    #[allow(clippy::too_many_arguments)]
    fn render_scene_phys(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        scale: f32,
        bounds_origin_phys: [f32; 2],
        grid_phys: [u32; 2],
        camera: [f32; 2],
    ) -> Vec<[u8; 4]> {
        let mut gpu_segments = Vec::new();
        let mut gpu_entries = Vec::new();
        let mut gpu_styles = Vec::new();
        for (i, (drawable, style)) in drawables.iter().enumerate() {
            let seg_offset = gpu_segments.len() as u32;
            let (mut entry, gpu_style) =
                compile_drawable(drawable, style, i as u32, 0, &mut gpu_segments);
            entry.style_idx = gpu_styles.len() as u32;
            entry.segment_start = seg_offset;
            gpu_entries.push(entry);
            gpu_styles.push(gpu_style);
        }
        let grid_cols = (grid_phys[0] as f32 / TILE_SIZE).ceil() as u32;
        let grid_rows = (grid_phys[1] as f32 / TILE_SIZE).ceil() as u32;
        let total_tiles = grid_cols * grid_rows;
        let draw_data = DrawData {
            bounds_origin: GpuVec2::new(bounds_origin_phys[0], bounds_origin_phys[1]),
            camera_position: GpuVec2::new(camera[0], camera[1]),
            camera_zoom: zoom,
            scale_factor: scale,
            time: 0.0,
            debug_flags: 0,
            entry_count: gpu_entries.len() as u32,
            entry_start: 0,
            grid_cols,
            grid_rows,
            tile_base: 0,
            _pad0: 0,
            mouse_px: GpuVec2::ZERO,
        };
        self.execute_render(
            &gpu_entries,
            &gpu_segments,
            &gpu_styles,
            draw_data,
            total_tiles,
            width,
            height,
            grid_cols,
            grid_rows,
        )
    }

    /// Render an `SdfPrimitive` through the REAL `SdfPipeline` (prepare + draw) to
    /// RGBA pixels - the production path, so a test sees exactly what the widget
    /// would draw (dedup, translate, cull, fragment), not a hand-built dispatch.
    /// `width` must be a multiple of 64 so `bytes_per_row` needs no padding.
    fn render_primitive(
        &self,
        prim: &crate::primitive::SdfPrimitive,
        width: u32,
        height: u32,
    ) -> Vec<[u8; 4]> {
        use iced_wgpu::graphics::Viewport;
        use iced_wgpu::primitive::{Pipeline, Primitive};

        let mut pipeline = crate::primitive::SdfPipeline::new(
            &self.device,
            &self.queue,
            TextureFormat::Rgba8Unorm,
        );
        let viewport = Viewport::with_physical_size(iced::Size::new(width, height), 1.0);
        let bounds = iced::Rectangle::new(
            iced::Point::ORIGIN,
            iced::Size::new(width as f32, height as f32),
        );

        let target = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&TextureViewDescriptor::default());
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (width * height * 4) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Pipeline::trim(&mut pipeline);
        prim.prepare(&mut pipeline, &self.device, &self.queue, &bounds, &viewport);
        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            prim.draw(&pipeline, &mut pass);
        }
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let idx = self.queue.submit(Some(enc.finish()));
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        let data = slice.get_mapped_range();
        let px: Vec<[u8; 4]> = data
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        drop(data);
        readback.unmap();
        px
    }

    /// Like [`render_primitives_bounded`] but DRAWS each primitive in its OWN
    /// render pass with a SCISSOR set to its clip rect - replicating how iced_wgpu
    /// renders layered custom primitives (each `with_layer` is a clipped pass).
    fn render_primitives_scissored(
        &self,
        prims: &[(&crate::primitive::SdfPrimitive, iced::Rectangle)],
        width: u32,
        height: u32,
    ) -> Vec<[u8; 4]> {
        use iced_wgpu::graphics::Viewport;
        use iced_wgpu::primitive::{Pipeline, Primitive};

        let mut pipeline = crate::primitive::SdfPipeline::new(
            &self.device,
            &self.queue,
            TextureFormat::Rgba8Unorm,
        );
        let viewport = Viewport::with_physical_size(iced::Size::new(width, height), 1.0);
        let target = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&TextureViewDescriptor::default());
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (width * height * 4) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Pipeline::trim(&mut pipeline);
        for (p, bounds) in prims {
            p.prepare(&mut pipeline, &self.device, &self.queue, bounds, &viewport);
        }
        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        for (i, (p, bounds)) in prims.iter().enumerate() {
            let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: if i == 0 {
                            LoadOp::Clear(Color::TRANSPARENT)
                        } else {
                            LoadOp::Load
                        },
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let sx = bounds.x.max(0.0) as u32;
            let sy = bounds.y.max(0.0) as u32;
            let sw = (bounds.width as u32).min(width - sx);
            let sh = (bounds.height as u32).min(height - sy);
            pass.set_scissor_rect(sx, sy, sw, sh);
            p.draw(&pipeline, &mut pass);
        }
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let idx = self.queue.submit(Some(enc.finish()));
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        let data = slice.get_mapped_range();
        let px: Vec<[u8; 4]> = data
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        drop(data);
        readback.unmap();
        px
    }

    /// Render SEVERAL `SdfPrimitive`s through ONE `SdfPipeline` in order (prepare
    /// all, then draw all) - the real multi-primitive frame. Each prepare allocates
    /// its own tile region (`tile_base` accumulates), so enough primitives grow the
    /// tile buffer mid-frame and exercise the growth-with-copy path that a single
    /// primitive never hits. `width` must be a multiple of 64.
    fn render_primitives(
        &self,
        prims: &[&crate::primitive::SdfPrimitive],
        width: u32,
        height: u32,
    ) -> Vec<[u8; 4]> {
        use iced_wgpu::graphics::Viewport;
        use iced_wgpu::primitive::{Pipeline, Primitive};

        let mut pipeline = crate::primitive::SdfPipeline::new(
            &self.device,
            &self.queue,
            TextureFormat::Rgba8Unorm,
        );
        let viewport = Viewport::with_physical_size(iced::Size::new(width, height), 1.0);
        let bounds = iced::Rectangle::new(
            iced::Point::ORIGIN,
            iced::Size::new(width as f32, height as f32),
        );

        let target = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&TextureViewDescriptor::default());
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (width * height * 4) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Pipeline::trim(&mut pipeline);
        for p in prims {
            p.prepare(&mut pipeline, &self.device, &self.queue, &bounds, &viewport);
        }
        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            for p in prims {
                p.draw(&pipeline, &mut pass);
            }
        }
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let idx = self.queue.submit(Some(enc.finish()));
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        let data = slice.get_mapped_range();
        let px: Vec<[u8; 4]> = data
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        drop(data);
        readback.unmap();
        px
    }

    fn create_storage<T: ShaderType + ShaderSize + WriteInto>(&self, items: &[T]) -> Buffer {
        let mut scratch = Vec::new();
        let mut writer = StorageBuffer::new(&mut scratch);
        writer.write(items).expect("Failed to write storage buffer");
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &scratch,
                usage: BufferUsages::STORAGE,
            })
    }

    fn create_uniform<T: ShaderType + ShaderSize + WriteInto>(&self, item: &T) -> Buffer {
        let mut scratch = Vec::new();
        let mut writer = UniformBuffer::new(&mut scratch);
        writer.write(item).expect("Failed to write uniform buffer");
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &scratch,
                usage: BufferUsages::UNIFORM,
            })
    }

    fn create_render_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_storage(
                    0,
                    ShaderStages::VERTEX_FRAGMENT,
                    DrawData::SHADER_SIZE.get(),
                ),
                bgl_storage(1, ShaderStages::FRAGMENT, GpuDrawEntry::SHADER_SIZE.get()),
                bgl_storage(2, ShaderStages::FRAGMENT, GpuSegment::SHADER_SIZE.get()),
                bgl_storage(3, ShaderStages::FRAGMENT, GpuStyle::SHADER_SIZE.get()),
                bgl_storage(4, ShaderStages::FRAGMENT, 4),
                bgl_storage(5, ShaderStages::FRAGMENT, 4),
            ],
        })
    }

    fn create_compute_layout0(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_storage(0, ShaderStages::COMPUTE, DrawData::SHADER_SIZE.get()),
                bgl_storage(1, ShaderStages::COMPUTE, GpuDrawEntry::SHADER_SIZE.get()),
                bgl_storage(2, ShaderStages::COMPUTE, GpuSegment::SHADER_SIZE.get()),
                bgl_storage(3, ShaderStages::COMPUTE, GpuStyle::SHADER_SIZE.get()),
            ],
        })
    }

    fn create_compute_layout1(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(ComputeUniforms::SHADER_SIZE),
                    },
                    count: None,
                },
                bgl_storage_rw(1, 4),
                bgl_storage_rw(2, 4),
            ],
        })
    }
}

fn bgl_storage(binding: u32, visibility: ShaderStages, min_size: u64) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(min_size),
        },
        count: None,
    }
}

fn bgl_storage_rw(binding: u32, min_size: u64) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(min_size),
        },
        count: None,
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Solid stroke on a horizontal line must not show tile boundary seams.
/// Samples every x position at y = center (on the line).
/// All pixels on the stroke must have the same alpha.
#[test]
fn solid_stroke_no_tile_seams() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 64u32;
    let zoom = 1.0;

    let line = Curve::line([-60.0, 0.0], [60.0, 0.0]);
    let style = Style::stroke(iced::Color::WHITE, Pattern::solid(6.0));

    let pixels = renderer.render(&[(&line, &style)], width, height, zoom);

    // y=32 is the center line (world y=0 with camera centered)
    let y = height / 2;
    let mut alphas = Vec::new();
    // Sample x from 8 to 120 (where the line is definitely visible)
    for x in 8..120 {
        let px = TestRenderer::pixel_at(&pixels, width, x, y);
        alphas.push((x, px[3]));
    }

    // All on-line pixels should have the same (nonzero) alpha
    let nonzero: Vec<_> = alphas.iter().filter(|(_, a)| *a > 0).collect();
    assert!(
        !nonzero.is_empty(),
        "No visible pixels on the stroke center line"
    );

    let expected_alpha = nonzero[0].1;
    for &&(x, alpha) in &nonzero {
        assert_eq!(
            alpha,
            expected_alpha,
            "Tile seam: alpha differs at x={x} (got {alpha}, expected {expected_alpha}). \
             Tile boundary at x={}",
            (x / TILE_SIZE as u32) * TILE_SIZE as u32,
        );
    }
}

/// Dashed pattern on a horizontal line must not show tile boundary gaps.
/// At tile boundaries, there must not be a 1-pixel dropout where both
/// neighbors have significantly higher alpha (indicating a culling gap).
#[test]
fn dashed_stroke_no_tile_seams() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 64u32;
    let zoom = 1.0;

    let line = Curve::line([-60.0, 0.0], [60.0, 0.0]);
    let style = Style::stroke(iced::Color::WHITE, Pattern::dashed(6.0, 14.0, 8.0));

    let pixels = renderer.render(&[(&line, &style)], width, height, zoom);

    let y = height / 2;
    let mut gaps = Vec::new();

    // Check tile boundary pixels (and their immediate neighbors) for dropout gaps
    for tile_x in (TILE_SIZE as u32..width - TILE_SIZE as u32).step_by(TILE_SIZE as usize) {
        // Sample the boundary pixel and neighbors on each side
        let left = TestRenderer::pixel_at(&pixels, width, tile_x - 1, y)[3];
        let boundary = TestRenderer::pixel_at(&pixels, width, tile_x, y)[3];
        let right = TestRenderer::pixel_at(&pixels, width, tile_x + 1, y)[3];

        // A culling gap: boundary pixel is significantly darker than BOTH neighbors
        let min_neighbor = left.min(right);
        if min_neighbor > 20 && boundary < min_neighbor.saturating_sub(30) {
            gaps.push((tile_x, left, boundary, right));
        }
    }

    assert!(
        gaps.is_empty(),
        "Tile boundary gaps in dashed pattern (x, left, boundary, right): {gaps:?}",
    );
}

/// A bezier whose control points overshoot — the classic shape produced by
/// dragging an edge near its origin or wiring two nodes a few pixels apart —
/// folds back on itself and develops multiple local distance minima. With a
/// single 16-sample seed + Newton, tiles near the inner "armpit" snap to the
/// wrong local min, the SDF reports the wrong distance, and the cull discards
/// them. Visually it shows up as quad-shaped holes rotated along the curve.
/// This walks the visible centerline of the S and asserts every centerline
/// pixel renders opaque — pre-fix, alternating tile-shaped chunks dropped.
#[test]
fn tight_overshoot_bezier_renders_without_holes() {
    let renderer = shared_renderer();
    let width = 256u32;
    let height = 128u32;
    let zoom = 1.0;

    // Real 2D S-curve with overshooting control points (40px endpoints,
    // 60px control extension into each lobe).
    let p0 = [-20.0_f32, -15.0];
    let p1 = [40.0, -15.0];
    let p2 = [-40.0, 15.0];
    let p3 = [20.0, 15.0];
    let curve = Curve::bezier(p0, p1, p2, p3);
    let style = Style::stroke(iced::Color::WHITE, Pattern::solid(4.0));

    let pixels = renderer.render(&[(&curve, &style)], width, height, zoom);

    // Helper: cubic bezier point.
    let bp = |t: f32| -> [f32; 2] {
        let u = 1.0 - t;
        [
            u * u * u * p0[0]
                + 3.0 * u * u * t * p1[0]
                + 3.0 * u * t * t * p2[0]
                + t * t * t * p3[0],
            u * u * u * p0[1]
                + 3.0 * u * u * t * p1[1]
                + 3.0 * u * t * t * p2[1]
                + t * t * t * p3[1],
        ]
    };

    let cx = (width / 2) as i32;
    let cy = (height / 2) as i32;
    let mut holes = Vec::new();
    // Sample the centerline at 64 points; each must be opaque.
    for i in 0..=64 {
        let t = i as f32 / 64.0;
        let p = bp(t);
        let sx = (cx + p[0].round() as i32) as u32;
        let sy = (cy + p[1].round() as i32) as u32;
        if sx >= width || sy >= height {
            continue;
        }
        let a = TestRenderer::pixel_at(&pixels, width, sx, sy)[3];
        if a < 200 {
            holes.push((t, p[0], p[1], a));
        }
    }
    assert!(
        holes.is_empty(),
        "Tight overshoot bezier has SDF holes along its centerline ({} bad of 65): first = {:?}",
        holes.len(),
        &holes[..holes.len().min(5)],
    );
}

/// Angled dashed strokes must not lose entire tiles to per-segment culling.
/// `apply_pattern` shears `shifted_u = u + dist*tan(angle)`, so a pixel away
/// from the tile center can fall inside a dash even when the tile center is
/// well outside one. Without an angle-aware cull margin in the compute pass,
/// the segment is dropped for those tiles and visible coverage collapses as
/// the angle grows. Compare opaque coverage at 0° vs 40°: they should be
/// within tens of percent, not >50% drop.
#[test]
fn dashed_stroke_at_angle_preserves_coverage() {
    let renderer = shared_renderer();
    let width = 384u32;
    let height = 96u32;
    let zoom = 1.0;

    let line = Curve::line([-160.0, 0.0], [160.0, 0.0]);
    let measure = |angle_deg: f32| -> u32 {
        let style = Style::stroke(
            iced::Color::WHITE,
            Pattern::dashed_angle(6.0, 14.0, 8.0, angle_deg.to_radians()),
        );
        let pixels = renderer.render(&[(&line, &style)], width, height, zoom);
        let cy = height / 2;
        let mut opaque = 0u32;
        // Sample the whole stroke band (≈ thickness 6 plus a few px of AA).
        for y in (cy - 5)..=(cy + 5) {
            for x in 40..(width - 40) {
                if TestRenderer::pixel_at(&pixels, width, x, y)[3] > 150 {
                    opaque += 1;
                }
            }
        }
        opaque
    };

    let baseline = measure(0.0);
    assert!(baseline > 100, "baseline coverage too low: {baseline}");
    for &ang in &[20.0_f32, 30.0, 40.0, 45.0] {
        let c = measure(ang);
        let ratio = c as f32 / baseline as f32;
        assert!(
            ratio > 0.6,
            "Angle {ang}°: coverage {c} = {:.0}% of baseline {baseline} \
             — culling is dropping dashed tiles at angle",
            ratio * 100.0,
        );
    }
}

/// Multi-style bezier (like edge editor) must not show horizontal artifacts
/// at tile row boundaries. Checks that pixels at y=tile_boundary are
/// consistent with their vertical neighbors.
#[test]
fn bezier_multi_style_no_row_artifacts() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    // S-curve bezier like the edge editor default
    let bezier = Curve::bezier([-50.0, -20.0], [-15.0, -20.0], [15.0, 20.0], [50.0, 20.0]);

    // Multi-style: stroke + border + shadow (mimics edge editor)
    let stroke = Style::stroke(iced::Color::WHITE, Pattern::solid(6.0));
    let border = Style::stroke(
        iced::Color::from_rgb(0.8, 0.6, 0.2),
        Pattern::solid(14.0), // thickness > stroke = border behind
    );
    let shadow = Style::shadow(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3), 10.0);

    let pixels = renderer.render(
        &[(&bezier, &stroke), (&bezier, &border), (&bezier, &shadow)],
        width,
        height,
        zoom,
    );

    // Check every tile row boundary for horizontal artifacts.
    // At each boundary row y, compare pixel with y-1 and y+1.
    // A wrong-color artifact shows as a pixel whose color deviates sharply
    // from BOTH vertical neighbors.
    let mut artifacts = Vec::new();
    for tile_y in (TILE_SIZE as u32..height - TILE_SIZE as u32).step_by(TILE_SIZE as usize) {
        for x in 10..width - 10 {
            let above = TestRenderer::pixel_at(&pixels, width, x, tile_y - 1);
            let at = TestRenderer::pixel_at(&pixels, width, x, tile_y);
            let below = TestRenderer::pixel_at(&pixels, width, x, tile_y + 1);

            // Check each channel: artifact = pixel differs sharply from both neighbors
            for ch in 0..4 {
                let a = above[ch] as i32;
                let b = at[ch] as i32;
                let c = below[ch] as i32;

                let diff_above = (b - a).abs();
                let diff_below = (b - c).abs();
                let neighbor_diff = (a - c).abs();

                // Artifact: pixel differs from both neighbors by more than
                // the neighbors differ from each other, with significant magnitude
                if diff_above > 15 && diff_below > 15 && neighbor_diff < 10 {
                    artifacts.push((x, tile_y, ch, above, at, below));
                    break; // one channel is enough to flag this pixel
                }
            }
        }
    }

    assert!(
        artifacts.is_empty(),
        "Horizontal artifacts at tile row boundaries ({} pixels).\n\
         First 5: {:?}",
        artifacts.len(),
        &artifacts[..artifacts.len().min(5)],
    );
}

/// Exact edge editor default setup: 2 crossing S-beziers, 4 style layers.
/// Must not show horizontal 1-pixel artifacts at tile row boundaries.
#[test]
fn edge_editor_defaults_no_row_artifacts() {
    let renderer = shared_renderer();
    // Simulate a canvas region similar to the edge editor at typical window size
    let width = 800u32;
    let height = 500u32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    // The two default crossing S-curves
    let fwd = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let mir = Curve::bezier([120.0, -40.0], [40.0, -40.0], [-40.0, 40.0], [-120.0, 40.0]);

    // Default edge editor styles (all visible)
    let thickness = 6.0_f32;
    let stroke = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.2, 0.85, 1.0, 1.0),
        iced::Color::from_rgba(0.6, 0.2, 1.0, 1.0),
        Pattern::solid(thickness),
    );
    let outline_total = thickness + 1.2 * 2.0;
    let outline = Style::stroke(
        iced::Color::from_rgba(0.05, 0.05, 0.15, 1.0),
        Pattern::solid(outline_total),
    );
    let border_total = thickness + 2.0 * 2.0 + 3.0 * 2.0;
    let border = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.95, 0.75, 0.2, 1.0),
        iced::Color::from_rgba(1.0, 0.3, 0.2, 1.0),
        Pattern::solid(border_total),
    );
    let shadow = Style::shadow(iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35), 10.0);

    // Each style applied to both edges (like SdfEdgeCanvas does)
    let mut drawables: Vec<(&crate::drawable::Drawable, &Style)> = Vec::new();
    let edges = [&fwd, &mir];
    let styles = [&stroke, &outline, &border, &shadow];
    for style in &styles {
        for edge in &edges {
            drawables.push((edge, style));
        }
    }

    let tiled = renderer.render_opts(&drawables, width, height, zoom, true);
    let untiled = renderer.render_opts(&drawables, width, height, zoom, false);

    // Find visible pixels where tiled and untiled differ significantly.
    let mut significant_diffs: Vec<PixelDiff> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let t = TestRenderer::pixel_at(&tiled, width, x, y);
            let u = TestRenderer::pixel_at(&untiled, width, x, y);
            // Only care about solidly visible pixels (shadow edge differences are expected)
            if t[3] < 100 && u[3] < 100 {
                continue;
            }
            let max_diff = (0..4)
                .map(|ch| (t[ch] as i32 - u[ch] as i32).abs())
                .max()
                .unwrap();
            if max_diff > 3 {
                significant_diffs.push((x, y, t, u, max_diff));
            }
        }
    }

    // Check if diffs cluster at tile column boundaries
    let at_col_boundary = significant_diffs
        .iter()
        .filter(|&&(x, _, _, _, _)| {
            x % (TILE_SIZE as u32) <= 1 || x % (TILE_SIZE as u32) >= (TILE_SIZE as u32) - 1
        })
        .count();

    assert!(
        significant_diffs.is_empty(),
        "Tiled vs untiled rendering differs in visible areas: {} pixels \
         ({} at tile column boundaries).\n\
         First 10: {:?}",
        significant_diffs.len(),
        at_col_boundary,
        &significant_diffs[..significant_diffs.len().min(10)],
    );
}

/// Single bezier stroke edge must be smooth (no periodic wobble).
/// Tests the untiled path to isolate SDF evaluation from tiling.
#[test]
fn bezier_stroke_edge_is_smooth() {
    let renderer = shared_renderer();
    let width = 800u32;
    let height = 500u32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    // Use the actual edge editor bezier (longer, endpoints further from view center)
    let bezier = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let thickness = 6.0_f32;
    let border_total = thickness + 2.0 * 2.0 + 3.0 * 2.0;
    // Test with flat color (no gradient) vs gradient to isolate arc-length cause
    let flat_border = Style::stroke(
        iced::Color::from_rgba(0.95, 0.75, 0.2, 1.0),
        Pattern::solid(border_total),
    );
    let drawables: Vec<(&crate::drawable::Drawable, &Style)> = vec![(&bezier, &flat_border)];

    // Test untiled first to confirm wobble is tiling-specific
    let pixels = renderer.render_opts(&drawables, width, height, zoom, false);

    // Find the stroke edge: for each x, find the y where alpha transitions
    // from >200 to <50 (top edge of stroke). Track edge y position.
    let mut edge_positions: Vec<(u32, f32)> = Vec::new();
    // Skip curve endpoints (first/last 35%) where endpoint caps create natural kinks
    let x_start = (width as f32 * 0.35) as u32;
    let x_end = (width as f32 * 0.65) as u32;
    for x in x_start..x_end {
        // Scan from top to find first row with alpha > 128
        let mut edge_y = None;
        for y in 0..height - 1 {
            let a0 = TestRenderer::pixel_at(&pixels, width, x, y)[3];
            let a1 = TestRenderer::pixel_at(&pixels, width, x, y + 1)[3];
            // Subpixel edge: interpolate where alpha crosses 128
            if a0 < 128 && a1 >= 128 {
                let t = (128.0 - a0 as f32) / (a1 as f32 - a0 as f32);
                edge_y = Some(y as f32 + t);
                break;
            }
        }
        if let Some(ey) = edge_y {
            edge_positions.push((x, ey));
        }
    }

    assert!(edge_positions.len() > 10, "Not enough edge positions found");

    // The edge should be smooth: no periodic jumps.
    // Check that second derivative (acceleration) is small and continuous.
    let mut max_accel = 0.0f32;
    let mut wobbles = Vec::new();
    for i in 1..edge_positions.len() - 1 {
        let (x, y_prev) = edge_positions[i - 1];
        let (_, y_curr) = edge_positions[i];
        let (_, y_next) = edge_positions[i + 1];
        let accel = (y_next - 2.0 * y_curr + y_prev).abs();
        if accel > max_accel {
            max_accel = accel;
        }
        // Flag positions where acceleration is suspiciously high
        if accel > 0.15 {
            wobbles.push((x, y_curr, accel));
        }
    }

    // Check if wobbles correlate with tile boundaries
    let at_tile_boundary: Vec<_> = wobbles
        .iter()
        .filter(|&&(x, _, _)| {
            x % (TILE_SIZE as u32) <= 1 || x % (TILE_SIZE as u32) >= (TILE_SIZE as u32) - 1
        })
        .collect();

    assert!(
        wobbles.is_empty(),
        "Stroke edge has {} wobbles (max accel={max_accel:.2}), {} at tile boundaries.\n\
         First 10: {:?}",
        wobbles.len(),
        at_tile_boundary.len(),
        &wobbles[..wobbles.len().min(10)],
    );
}

/// CPU-side bezier closest-point (diagnostic, not regression).
#[test]
#[ignore]
fn bezier_closest_point_smooth_cpu() {
    let p0 = [-100.0f32, -30.0];
    let p1 = [-30.0, -30.0];
    let p2 = [30.0, 30.0];
    let p3 = [100.0, 30.0];
    let zoom = 500.0f32 * 0.333 / 160.0;

    // Scan along the outer edge of a 16-wide border at zoom
    let half_t = 8.0f32;
    let cam_x = 800.0 * 0.5 / zoom;

    let mut edge_y_positions = Vec::new();
    for px_x in 200..600u32 {
        let world_x = px_x as f32 / zoom - cam_x;
        // Binary search for the world_y where dist = half_t (edge)
        let mut y_lo = -100.0f32;
        let mut y_hi = 100.0f32;
        for _ in 0..40 {
            let y_mid = (y_lo + y_hi) * 0.5;
            let dist = cpu_bezier_dist(world_x, y_mid, &p0, &p1, &p2, &p3);
            if dist < half_t {
                y_lo = y_mid;
            } else {
                y_hi = y_mid;
            }
        }
        edge_y_positions.push((px_x, (y_lo + y_hi) * 0.5));
    }

    // Check smoothness of edge y
    let mut wobbles = Vec::new();
    for i in 1..edge_y_positions.len() - 1 {
        let (x, y_prev) = edge_y_positions[i - 1];
        let (_, y_curr) = edge_y_positions[i];
        let (_, y_next) = edge_y_positions[i + 1];
        let accel = (y_next - 2.0 * y_curr + y_prev).abs();
        if accel > 0.001 {
            wobbles.push((x, y_curr, accel));
        }
    }
    assert!(
        wobbles.is_empty(),
        "CPU bezier distance has {} wobbles. First 10: {:?}",
        wobbles.len(),
        &wobbles[..wobbles.len().min(10)],
    );
}

fn cpu_newton_refine(
    px: f32,
    py: f32,
    t0: f32,
    p0: &[f32; 2],
    p1: &[f32; 2],
    p2: &[f32; 2],
    p3: &[f32; 2],
) -> (f32, f32) {
    let mut t = t0;
    for _ in 0..4 {
        let bp = cpu_bez_pt(p0, p1, p2, p3, t);
        let bd = cpu_bez_deriv(p0, p1, p2, p3, t);
        let bdd = cpu_bez_deriv2(p0, p1, p2, p3, t);
        let dx = bp[0] - px;
        let dy = bp[1] - py;
        let num = dx * bd[0] + dy * bd[1];
        let den = bd[0] * bd[0] + bd[1] * bd[1] + dx * bdd[0] + dy * bdd[1];
        if den.abs() > 1e-8 {
            t = (t - num / den).clamp(0.0, 1.0);
        }
    }
    let cp = cpu_bez_pt(p0, p1, p2, p3, t);
    let d = ((px - cp[0]).powi(2) + (py - cp[1]).powi(2)).sqrt();
    (t, d)
}

fn cpu_bezier_dist(
    px: f32,
    py: f32,
    p0: &[f32; 2],
    p1: &[f32; 2],
    p2: &[f32; 2],
    p3: &[f32; 2],
) -> f32 {
    // Coarse search: track best AND second-best
    let mut best_t = 0.0f32;
    let mut best_dist = 1e20f32;
    let mut second_t = 0.0f32;
    let mut second_dist = 1e20f32;
    for i in 0..=16 {
        let t = i as f32 / 16.0;
        let bp = cpu_bez_pt(p0, p1, p2, p3, t);
        let d = ((px - bp[0]).powi(2) + (py - bp[1]).powi(2)).sqrt();
        if d < best_dist {
            second_t = best_t;
            second_dist = best_dist;
            best_dist = d;
            best_t = t;
        } else if d < second_dist {
            second_t = t;
            second_dist = d;
        }
    }
    // Refine both candidates
    let (_, d1) = cpu_newton_refine(px, py, best_t, p0, p1, p2, p3);
    let (_, d2) = cpu_newton_refine(px, py, second_t, p0, p1, p2, p3);
    d1.min(d2)
}

fn cpu_bez_pt(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u * u * u * p0[0] + 3.0 * u * u * t * p1[0] + 3.0 * u * t * t * p2[0] + t * t * t * p3[0],
        u * u * u * p0[1] + 3.0 * u * u * t * p1[1] + 3.0 * u * t * t * p2[1] + t * t * t * p3[1],
    ]
}

fn cpu_bez_deriv(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        3.0 * u * u * (p1[0] - p0[0])
            + 6.0 * u * t * (p2[0] - p1[0])
            + 3.0 * t * t * (p3[0] - p2[0]),
        3.0 * u * u * (p1[1] - p0[1])
            + 6.0 * u * t * (p2[1] - p1[1])
            + 3.0 * t * t * (p3[1] - p2[1]),
    ]
}

fn cpu_bez_deriv2(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        6.0 * u * (p2[0] - 2.0 * p1[0] + p0[0]) + 6.0 * t * (p3[0] - 2.0 * p2[0] + p1[0]),
        6.0 * u * (p2[1] - 2.0 * p1[1] + p0[1]) + 6.0 * t * (p3[1] - 2.0 * p2[1] + p1[1]),
    ]
}

/// Check for missing rows at tile boundaries inside the stroke.
/// The stroke center should have consistent alpha - any drop at y%16==0 is a bug.
#[test]
fn no_missing_rows_in_stroke() {
    let renderer = shared_renderer();
    let width = 800u32;
    let height = 500u32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    // Actual edge editor bezier + all 4 style layers on both edges
    let fwd = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let mir = Curve::bezier([120.0, -40.0], [40.0, -40.0], [-40.0, 40.0], [-120.0, 40.0]);

    let thickness = 6.0_f32;
    let stroke = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.2, 0.85, 1.0, 1.0),
        iced::Color::from_rgba(0.6, 0.2, 1.0, 1.0),
        Pattern::solid(thickness),
    );
    let outline_total = thickness + 1.2 * 2.0;
    let outline = Style::stroke(
        iced::Color::from_rgba(0.05, 0.05, 0.15, 1.0),
        Pattern::solid(outline_total),
    );
    let border_total = thickness + 2.0 * 2.0 + 3.0 * 2.0;
    let border = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.95, 0.75, 0.2, 1.0),
        iced::Color::from_rgba(1.0, 0.3, 0.2, 1.0),
        Pattern::solid(border_total),
    );
    let shadow = Style::shadow(iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35), 10.0);

    let edges = [&fwd, &mir];
    let styles_list = [&stroke, &outline, &border, &shadow];
    let mut drawables: Vec<(&crate::drawable::Drawable, &Style)> = Vec::new();
    for style in &styles_list {
        for edge in &edges {
            drawables.push((edge, style));
        }
    }

    // Test with 4K display scale factor 2.0
    let scale = 2.0_f32;
    let pixels = renderer.render_full(&drawables, width, height, zoom, scale, true);

    // For each tile row boundary, check if alpha drops compared to rows above and below
    let mut missing_rows = Vec::new();
    for tile_y in (TILE_SIZE as u32..height - TILE_SIZE as u32).step_by(TILE_SIZE as usize) {
        // Sample across a wide x range
        let mut drop_count = 0u32;
        let mut total_checked = 0u32;
        for x in (width / 4)..(width * 3 / 4) {
            let above = TestRenderer::pixel_at(&pixels, width, x, tile_y - 1)[3];
            let at = TestRenderer::pixel_at(&pixels, width, x, tile_y)[3];
            let below = TestRenderer::pixel_at(&pixels, width, x, tile_y + 1)[3];

            // Only check where we're solidly inside the stroke (neighbors alpha > 200)
            if above > 200 && below > 200 {
                total_checked += 1;
                if at < above.min(below) - 5 {
                    drop_count += 1;
                }
            }
        }
        if drop_count > 0 {
            missing_rows.push((tile_y, drop_count, total_checked));
        }
    }

    assert!(
        missing_rows.is_empty(),
        "Missing rows at tile boundaries (y, drops, checked): {:?}",
        missing_rows,
    );
}

/// Diagnose dump (not a regression test).
#[test]
#[ignore]
fn diagnose_wobble_at_288() {
    let renderer = shared_renderer();
    let width = 800u32;
    let height = 500u32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    let bezier = Curve::bezier([-100.0, -30.0], [-30.0, -30.0], [30.0, 30.0], [100.0, 30.0]);
    let border_total = 6.0 + 2.0 * 2.0 + 3.0 * 2.0;
    let style = Style::stroke(iced::Color::WHITE, Pattern::solid(border_total));

    let pixels = renderer.render_opts(&[(&bezier, &style)], width, height, zoom, false);

    // Dump alpha at x=286..290 for y=210..216 to see the edge shape
    let mut lines = Vec::new();
    for x in 286..292 {
        let mut row = format!("x={x}: ");
        for y in 210..218 {
            let a = TestRenderer::pixel_at(&pixels, width, x, y)[3];
            row.push_str(&format!("{a:3} "));
        }
        lines.push(row);
    }
    panic!("Alpha values around wobble point:\n{}", lines.join("\n"));
}

/// Distance field visualization must show two distinct colors (signed).
#[test]
fn distance_field_shows_both_sides() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    let line = Curve::line([-50.0, 0.0], [50.0, 0.0]);
    let style = Style::distance_field();

    let pixels = renderer.render(&[(&line, &style)], width, height, zoom);

    // y=54 is 10 pixels below center (world y=+10, "outside" of line)
    // y=74 is 10 pixels above center (world y=-10, "inside")
    let above = TestRenderer::pixel_at(&pixels, width, 64, 54);
    let below = TestRenderer::pixel_at(&pixels, width, 64, 74);

    // Both should be non-black (visible)
    assert!(above[3] > 0, "No rendering above the line");
    assert!(below[3] > 0, "No rendering below the line");

    // They should have different colors (signed DF shows orange vs blue)
    assert_ne!(
        above[0..3],
        below[0..3],
        "Distance field should show different colors on each side of the line. \
         Above: {above:?}, Below: {below:?}",
    );
}

/// Regression: when a draw_primitive uses a non-zero `bounds_origin`
/// (e.g. clipped per-shape draws inside a larger widget), the rendered shape
/// must appear at the SAME screen position as when `bounds_origin` is zero,
/// provided the caller compensates by shifting `camera_position` by
/// `-bounds_origin / zoom` (in world units).
///
/// This locks in the camera-adjustment fix in iced_nodegraph's widget so it
/// can't silently regress if the shader's local-pixel convention changes.
#[test]
fn bounds_origin_shift_preserves_shape_position() {
    let renderer = shared_renderer();
    let width = 256u32;
    let height = 128u32;
    let zoom = 1.0;

    // Shape at world (0, 0). With bounds_origin=(0,0) and camera centering
    // world on screen, the shape lands at screen center.
    let shape = Curve::rounded_rect([0.0, 0.0], [40.0, 25.0], 6.0);
    let style = Style::solid(iced::Color::from_rgb(1.0, 0.0, 0.0));

    let cs = zoom;
    let cam_centered = [(width as f32) * 0.5 / cs, (height as f32) * 0.5 / cs];

    // Baseline: grid covers full texture, bounds_origin = (0, 0).
    let baseline = renderer.render_with_origin(
        &[(&shape, &style)],
        width,
        height,
        zoom,
        [0.0, 0.0],
        width,
        height,
        cam_centered,
    );

    // Shifted: bounds_origin at (50, 30), grid sized to still cover the
    // shape, camera shifted by -bounds_origin/zoom in world units (which the
    // widget computes as widget_origin - draw_bounds.origin = -bounds for a
    // zero-origin widget).
    let bounds_x = 50.0_f32;
    let bounds_y = 30.0_f32;
    let cam_shifted = [
        cam_centered[0] - bounds_x / zoom,
        cam_centered[1] - bounds_y / zoom,
    ];
    let shifted = renderer.render_with_origin(
        &[(&shape, &style)],
        width,
        height,
        zoom,
        [bounds_x, bounds_y],
        width - bounds_x as u32,
        height - bounds_y as u32,
        cam_shifted,
    );

    // Sample a row of pixels through the shape center on both images.
    // The shape should appear at the same screen position in both.
    let cy = height / 2;
    let mut mismatches = Vec::new();
    for x in 20..width - 20 {
        let bp = TestRenderer::pixel_at(&baseline, width, x, cy);
        let sp = TestRenderer::pixel_at(&shifted, width, x, cy);
        // Allow tiny AA differences at edges, but a full miss = shape moved.
        let diff_a = (bp[3] as i32 - sp[3] as i32).abs();
        if diff_a > 30 {
            mismatches.push((x, bp[3], sp[3]));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Shifted bounds_origin moved the shape (count={}, first 5={:?}). \
         The shader's local-pixel coord system requires camera_position to \
         be adjusted by -bounds_origin/zoom when bounds_origin moves.",
        mismatches.len(),
        &mismatches[..mismatches.len().min(5)],
    );

    // Also assert the shape is actually rendered in the shifted output
    // (catches the case where culling kills everything).
    let center = TestRenderer::pixel_at(&shifted, width, width / 2, cy);
    assert!(
        center[3] > 200,
        "Shifted render is empty at expected shape center: {center:?}",
    );
}

/// A stroke style on a closed rounded_rect must produce a visible border
/// all around the shape — no missing edges or culling holes along the contour.
#[test]
fn closed_stroke_border_complete() {
    let renderer = shared_renderer();
    let width = 256u32;
    let height = 128u32;
    let zoom = 1.0;

    let shape = Curve::rounded_rect([0.0, 0.0], [80.0, 40.0], 10.0);
    let style = Style::stroke(iced::Color::WHITE, Pattern::solid(3.0));

    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    // Sample exactly along the border (y = top of shape, x = right of shape).
    // Top border: world y = -40, screen y = height/2 - 40 = 24.
    // Sample x along the top edge from -70..70 world (away from corners).
    let cy = (height as i32 / 2) as u32; // center of screen
    let top_y = (cy as i32 - 40) as u32;
    let mut gaps = Vec::new();
    for dx in -70..=70 {
        let sx = (width as i32 / 2 + dx) as u32;
        let px = TestRenderer::pixel_at(&pixels, width, sx, top_y);
        if px[3] < 100 {
            gaps.push((dx, px[3]));
        }
    }
    assert!(
        gaps.is_empty(),
        "Stroke border has missing pixels on top edge: count={}, first 5={:?}",
        gaps.len(),
        &gaps[..gaps.len().min(5)],
    );
}

/// A large closed rounded_rect with Style::solid must fill its interior
/// completely — interior tiles many tile-widths from any boundary must
/// not be culled.
#[test]
fn closed_solid_fill_large_no_interior_holes() {
    let renderer = shared_renderer();
    let width = 512u32;
    let height = 256u32;
    let zoom = 1.0;

    // Center is ~100 px from the nearest boundary (many tile widths).
    let shape = Curve::rounded_rect([0.0, 0.0], [200.0, 100.0], 12.0);
    let style = Style::solid(iced::Color::from_rgb(1.0, 0.0, 0.0));

    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    // Sample interior points well away from any boundary.
    let mut holes = Vec::new();
    for dy in (-80..=80).step_by(8) {
        for dx in (-180..=180).step_by(8) {
            let sx = (width as i32 / 2 + dx) as u32;
            let sy = (height as i32 / 2 + dy) as u32;
            let px = TestRenderer::pixel_at(&pixels, width, sx, sy);
            if px[3] < 200 {
                holes.push((dx, dy, px[3]));
            }
        }
    }
    assert!(
        holes.is_empty(),
        "Interior tiles of large rounded_rect were culled (count={}): first 5 = {:?}",
        holes.len(),
        &holes[..holes.len().min(5)],
    );
}

/// A solid-filled `Curve::circle` must not leak its fill color outside the
/// circle. Regression: a bug in `sd_arc_segment`'s angle normalization (clamp
/// to [-PI, PI]) caused a full-sweep arc to classify points on the
/// wrap-around half as off-arc and assign them a negative signed distance via
/// the start-endpoint sign branch, painting Style::solid across most of the
/// surrounding plane. Visible in iced_nodegraph as a pin-colored block
/// covering the node body adjacent to each pin.
#[test]
fn closed_circle_solid_fill_does_not_leak_outside() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    let shape = Curve::circle([0.0, 0.0], 20.0);
    let style = Style::solid(iced::Color::from_rgb(1.0, 0.0, 0.0));

    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    // Camera centers world (0,0) at screen (w/2, h/2). Sample a ring of
    // points well outside the radius (>= 30 world units) — they must be
    // transparent. Pre-fix, the bottom-left half-plane outside the circle
    // was filled solid.
    let cx_s = (width / 2) as i32;
    let cy_s = (height / 2) as i32;
    let mut leaks = Vec::new();
    for (dx, dy) in &[
        (-40, 0),
        (-40, -20),
        (-40, 20),
        (40, 0),
        (40, -20),
        (40, 20),
        (0, -40),
        (0, 40),
        (-30, 30),
        (30, -30),
        (-30, -30),
        (30, 30),
    ] {
        let sx = (cx_s + dx) as u32;
        let sy = (cy_s + dy) as u32;
        let px = TestRenderer::pixel_at(&pixels, width, sx, sy);
        if px[3] > 20 {
            leaks.push((*dx, *dy, px));
        }
    }
    assert!(
        leaks.is_empty(),
        "Curve::circle solid fill leaked outside the radius: {leaks:?}",
    );

    // Sanity: the interior is actually filled.
    let center = TestRenderer::pixel_at(&pixels, width, cx_s as u32, cy_s as u32);
    assert!(center[3] > 200, "Circle interior not filled: {center:?}");
}

/// A closed rounded_rect with Style::solid must fill its interior completely.
/// Tiles deep inside the shape must not be culled.
#[test]
fn closed_solid_fill_no_interior_holes() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    // Big enough that the center is many tiles away from any boundary.
    let shape = Curve::rounded_rect([0.0, 0.0], [50.0, 35.0], 8.0);
    let style = Style::solid(iced::Color::from_rgb(1.0, 0.0, 0.0));

    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    // Camera centers world (0,0) at screen (width/2, height/2).
    // Sample a 20x20 grid of points well inside the shape: world [-30..30] x [-15..15].
    let mut holes = Vec::new();
    for dy in (-15..=15).step_by(3) {
        for dx in (-30..=30).step_by(3) {
            let sx = (width as i32 / 2 + dx) as u32;
            let sy = (height as i32 / 2 + dy) as u32;
            let px = TestRenderer::pixel_at(&pixels, width, sx, sy);
            if px[3] < 200 {
                holes.push((dx, dy, px));
            }
        }
    }
    assert!(
        holes.is_empty(),
        "Interior tiles of solid-filled rounded_rect were culled (showing as holes): {holes:?}",
    );
}

/// Composite premultiplied-alpha RGBA pixels over a dark background and save a
/// PNG (for visual inspection of the edge-editor render).
fn save_rgba_png(path: &str, width: u32, height: u32, pixels: &[[u8; 4]], bg: [u8; 3]) {
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for p in pixels {
        let a = p[3] as f32 / 255.0;
        // Pixels are premultiplied; composite over bg.
        for c in 0..3 {
            let v = p[c] as f32 + bg[c] as f32 * (1.0 - a);
            bytes.push(v.round().clamp(0.0, 255.0) as u8);
        }
        bytes.push(255);
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&bytes)
        .unwrap();
}

/// Renders the edge editor's exact default layer stack (stroke + outline +
/// border + shadow) on the two crossing S-curves, but with each layer a single
/// distinguishable flat color, and dumps a 160x80 PNG centered on the crossing.
/// Not an assertion - a visual probe for the reported tile-boundary artifact.
/// Run on demand: `cargo test -p iced_nodegraph_sdf dump_edge_editor_center -- --ignored --nocapture`.
#[test]
#[ignore]
fn dump_edge_editor_center() {
    let renderer = shared_renderer();
    let width = 160u32;
    let height = 80u32;
    let scale = 2.0_f32;
    // Match the real canvas: zoom ~1.85 logical * scale 2.0 => cs ~3.7 px/world.
    let zoom = 1.85_f32;

    let fwd = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let mir = Curve::bezier([120.0, -40.0], [40.0, -40.0], [-40.0, 40.0], [-120.0, 40.0]);

    // Edge editor defaults: thickness 6, outline 1.2, border gap 2 + thick 3,
    // shadow expand 10. Gradients replaced by flat, distinguishable colors.
    let c = |r, g, b, a| iced::Color::from_rgba(r, g, b, a);
    let stroke = Style::arc_gradient_stroke(
        c(0.0, 0.9, 1.0, 1.0),
        c(0.0, 0.9, 1.0, 1.0),
        Pattern::solid(6.0),
    );
    let outline = Style::stroke(c(1.0, 0.1, 0.1, 1.0), Pattern::solid(6.0 + 1.2 * 2.0));
    let border = Style::arc_gradient_stroke(
        c(0.1, 1.0, 0.1, 1.0),
        c(0.1, 1.0, 0.1, 1.0),
        Pattern::solid(6.0 + 2.0 * 2.0 + 3.0 * 2.0),
    );
    let shadow = Style::shadow(c(0.3, 0.3, 1.0, 0.9), 10.0);

    // SdfEdgeCanvas order: each style applied to both edges, front-to-back.
    let edges = [&fwd, &mir];
    let layers = [&stroke, &outline, &border, &shadow];
    let mut scene: Vec<(&crate::drawable::Drawable, &Style)> = Vec::new();
    for s in layers {
        for e in edges {
            scene.push((e, s));
        }
    }

    let pixels = renderer.render_full(&scene, width, height, zoom, scale, true);
    std::fs::create_dir_all("../out").ok();
    save_rgba_png(
        "../out/edge_artifact.png",
        width,
        height,
        &pixels,
        [26, 26, 31],
    );

    // 4x nearest-neighbor upscale so 1px seams are visible to the eye.
    let f = 4u32;
    let mut big = vec![[0u8; 4]; (width * f * height * f) as usize];
    for y in 0..height * f {
        for x in 0..width * f {
            big[(y * width * f + x) as usize] = pixels[((y / f) * width + (x / f)) as usize];
        }
    }
    save_rgba_png(
        "../out/edge_artifact_4x.png",
        width * f,
        height * f,
        &big,
        [26, 26, 31],
    );

    // Programmatic seam scan: at every 16px tile boundary, compare the boundary
    // row/col to its immediate interior neighbor; flag large jumps that the
    // neighbor-of-neighbor does not show (i.e. a 1px anomaly, not a real edge).
    let px = |x: u32, y: u32| pixels[(y * width + x) as usize];
    let diff = |a: [u8; 4], b: [u8; 4]| {
        (0..4)
            .map(|c| (a[c] as i32 - b[c] as i32).abs())
            .max()
            .unwrap()
    };
    let mut seams = Vec::new();
    for by in (16..height).step_by(16) {
        for x in 0..width {
            let across = diff(px(x, by - 1), px(x, by));
            let below = diff(px(x, by), px(x, (by + 1).min(height - 1)));
            let above = diff(px(x, by - 2), px(x, by - 1));
            // A seam: the boundary step is much larger than the gradient just
            // above/below it (a real edge would ramp smoothly across rows).
            if across > 24 && across > above * 2 + 8 && across > below * 2 + 8 {
                seams.push(('y', x, by, across));
            }
        }
    }
    eprintln!(
        "wrote ../out/edge_artifact.png + _4x (cs={}); horizontal-boundary seams: {} {:?}",
        zoom * scale,
        seams.len(),
        &seams[..seams.len().min(12)],
    );
}

/// Tiled variant of `bezier_stroke_edge_is_smooth` at HiDPI scale 2.0: scan the
/// stroke's top edge in a TILED render and assert it has no wobble correlated
/// with tile boundaries. This is the single-edge form of the reported artifact.
#[test]
fn bezier_stroke_edge_is_smooth_tiled_hidpi() {
    let renderer = shared_renderer();
    let width = 512u32;
    let height = 384u32;
    let scale = 2.0_f32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    let bezier = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let stroke = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.2, 0.85, 1.0, 1.0),
        iced::Color::from_rgba(0.6, 0.2, 1.0, 1.0),
        Pattern::solid(6.0),
    );
    let drawables: Vec<(&crate::drawable::Drawable, &Style)> = vec![(&bezier, &stroke)];

    let pixels = renderer.render_full(&drawables, width, height, zoom, scale, true);

    let mut edge_positions: Vec<(u32, f32)> = Vec::new();
    let x_start = (width as f32 * 0.35) as u32;
    let x_end = (width as f32 * 0.65) as u32;
    for x in x_start..x_end {
        let mut edge_y = None;
        for y in 0..height - 1 {
            let a0 = TestRenderer::pixel_at(&pixels, width, x, y)[3];
            let a1 = TestRenderer::pixel_at(&pixels, width, x, y + 1)[3];
            if a0 < 128 && a1 >= 128 {
                let t = (128.0 - a0 as f32) / (a1 as f32 - a0 as f32);
                edge_y = Some(y as f32 + t);
                break;
            }
        }
        if let Some(ey) = edge_y {
            edge_positions.push((x, ey));
        }
    }
    assert!(edge_positions.len() > 10, "not enough edge positions");

    let mut wobbles = Vec::new();
    for i in 1..edge_positions.len() - 1 {
        let (x, y_prev) = edge_positions[i - 1];
        let (_, y_curr) = edge_positions[i];
        let (_, y_next) = edge_positions[i + 1];
        let accel = (y_next - 2.0 * y_curr + y_prev).abs();
        if accel > 0.15 {
            wobbles.push((x, y_curr, accel));
        }
    }
    assert!(
        wobbles.is_empty(),
        "tiled stroke edge wobbles at {} points (scale {scale}): {:?}",
        wobbles.len(),
        &wobbles[..wobbles.len().min(10)],
    );
}

/// The spatial-index tile grid is an internal optimization: rendering the same
/// content must not depend on where the tile boundaries fall. We render a
/// multi-edge, multi-style scene (the edge editor's failure case) twice with
/// the tile grid shifted by 8 physical px and the camera compensated so the
/// content lands on the exact same pixels. Any per-pixel difference is a
/// tile-alignment artifact (e.g. AA derivatives evaluated across a tile
/// boundary). Reproduces the 1px seam seen in the sdf_basic edge editor.
#[test]
fn tiling_alignment_is_invisible() {
    let r = shared_renderer();
    let (w, h) = (256u32, 256u32);
    let zoom = 0.7_f32;
    let scale = 2.0_f32; // 4K-style HiDPI, as in the bug report
    let cs = zoom * scale;

    // Two crossing S-curves, the edge editor's default content.
    let e1 = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let e2 = Curve::bezier([120.0, -40.0], [40.0, -40.0], [-40.0, 40.0], [-120.0, 40.0]);
    let stroke = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.2, 0.85, 1.0, 1.0),
        iced::Color::from_rgba(0.6, 0.2, 1.0, 1.0),
        Pattern::solid(6.0),
    );
    let outline = Style::stroke(
        iced::Color::from_rgba(0.05, 0.05, 0.15, 1.0),
        Pattern::solid(8.4),
    );
    let border = Style::arc_gradient_stroke(
        iced::Color::from_rgba(0.95, 0.75, 0.2, 1.0),
        iced::Color::from_rgba(1.0, 0.3, 0.2, 1.0),
        Pattern::solid(16.0),
    );
    // SdfEdgeCanvas applies each style to every edge -> same-style adjacency.
    let scene: Vec<(&crate::drawable::Drawable, &Style)> = vec![
        (&e1, &border),
        (&e2, &border),
        (&e1, &outline),
        (&e2, &outline),
        (&e1, &stroke),
        (&e2, &stroke),
    ];

    let cam_a = [w as f32 * 0.5 / cs, h as f32 * 0.5 / cs];
    let a = r.render_scene_phys(&scene, w, h, zoom, scale, [0.0, 0.0], [w, h], cam_a);

    // Shift the tile grid by -8px and compensate the camera by +8/cs world
    // units so identical content lands on identical pixels.
    let shift = 8.0_f32;
    let cam_b = [cam_a[0] + shift / cs, cam_a[1] + shift / cs];
    let b = r.render_scene_phys(
        &scene,
        w,
        h,
        zoom,
        scale,
        [-shift, -shift],
        [w + 16, h + 16],
        cam_b,
    );

    let mut n_diff = 0u32;
    let mut max_diff = 0i32;
    let mut first: Vec<(u32, u32)> = Vec::new();
    for i in 0..(w * h) as usize {
        let mut px_diff = 0;
        for c in 0..4 {
            px_diff = px_diff.max((a[i][c] as i32 - b[i][c] as i32).abs());
        }
        if px_diff > max_diff {
            max_diff = px_diff;
        }
        if px_diff > 2 {
            n_diff += 1;
            if first.len() < 20 {
                first.push((i as u32 % w, i as u32 / w));
            }
        }
    }
    assert_eq!(
        n_diff, 0,
        "tile-grid alignment changed {n_diff} pixels (max channel diff {max_diff}); \
         tiling must be invisible. First diffs (x,y): {first:?}",
    );
}

/// A node shadow is a single outward distance band (full at the silhouette,
/// fading to nothing at `d`). Walking outward across the edge, its alpha must
/// fall monotonically with no local brightening: any dip-then-recover is the
/// premultiplied-compositing seam that multi-band tilings produced (#15).
#[test]
fn shadow_band_outward_alpha_has_no_seam() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    let radius = 20.0;
    let d = 12.0; // ramp spans ~12px so a seam, if any, is several pixels wide
    let shape = Curve::circle([0.0, 0.0], radius);
    // Outward glow: full at the silhouette, fading to transparent at d. Opaque
    // white so the alpha channel reads the band coverage directly.
    let style = Style::shadow(iced::Color::WHITE, d);

    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    // Camera centers world (0,0) at screen (w/2, h/2); +x is world distance.
    let cx = width / 2;
    let cy = height / 2;
    // Walk from just past the silhouette's AA zone (r+2) out to past the edge.
    let r0 = radius as u32 + 2;
    let r1 = (radius + d) as u32 + 4;
    let alphas: Vec<u8> = (r0..=r1)
        .map(|r| TestRenderer::pixel_at(&pixels, width, cx + r, cy)[3])
        .collect();

    let peak = *alphas.iter().max().unwrap();
    assert!(
        peak > 200,
        "shadow band never reaches full strength: {alphas:?}"
    );
    assert!(
        *alphas.last().unwrap() < 40,
        "shadow band did not fade out past its distance: {alphas:?}",
    );
    // After the peak the ramp must only descend (allow a small AA tolerance).
    let peak_idx = alphas.iter().position(|&a| a == peak).unwrap();
    for i in peak_idx..alphas.len() - 1 {
        assert!(
            alphas[i + 1] <= alphas[i] + 4,
            "alpha rose from {} to {} at offset {} (seam): {alphas:?}",
            alphas[i],
            alphas[i + 1],
            i + 1,
        );
    }
}

/// Two abutting opaque bands in ONE stop chain (the case that seamed when built
/// as separate composited entries) must stay fully opaque across their shared
/// boundary - the chain is evaluated in a single pass, so no premultiplied dip.
#[test]
fn abutting_chain_bands_stay_opaque_across_boundary() {
    let renderer = shared_renderer();
    let width = 128u32;
    let height = 128u32;
    let zoom = 1.0;

    let radius = 20.0;
    let red = iced::Color::from_rgb(0.9, 0.1, 0.1);
    let green = iced::Color::from_rgb(0.1, 0.9, 0.1);
    let clear = |c: iced::Color| iced::Color { a: 0.0, ..c };
    // Red ring [0,10] abutting a green ring [10,20], both opaque, one chain.
    let style = Style {
        stops: vec![
            crate::style::Stop::new(0.0, clear(red)),
            crate::style::Stop::new(0.0, red),
            crate::style::Stop::new(10.0, red),
            crate::style::Stop::new(10.0, green),
            crate::style::Stop::new(20.0, green),
            crate::style::Stop::new(20.0, clear(green)),
        ],
        pattern: None,
        distance_field: false,
        transfer: Default::default(),
    };
    let shape = Curve::circle([0.0, 0.0], radius);
    let pixels = renderer.render(&[(&shape, &style)], width, height, zoom);

    let cx = width / 2;
    let cy = height / 2;
    // Walk across the whole [0,20] band, including the red|green boundary at 10.
    let mut min_alpha = 255u8;
    for off in 2..=18 {
        let a = TestRenderer::pixel_at(&pixels, width, cx + radius as u32 + off, cy)[3];
        min_alpha = min_alpha.min(a);
    }
    assert!(
        min_alpha > 230,
        "chain dipped to alpha {min_alpha} across the shared boundary (seam)",
    );
    // Sanity: both colors are actually present (red inner, green outer).
    let inner = TestRenderer::pixel_at(&pixels, width, cx + radius as u32 + 4, cy);
    let outer = TestRenderer::pixel_at(&pixels, width, cx + radius as u32 + 15, cy);
    assert!(inner[0] > inner[1], "inner band should read red: {inner:?}");
    assert!(
        outer[1] > outer[0],
        "outer band should read green: {outer:?}"
    );
}

// ===========================================================================
// Golden corpus (STEP 0): the v2-vs-(v2/v3) oracle.
//
// Each scene is a self-contained (Drawable, Style) set rendered at a fixed
// resolution / zoom / time. The corpus spans every pattern, every tiling, a
// segment-dense pin node, deep z-overlap, far-from-origin coordinates, and a
// pinned-time animated edge. The STEP 0 gate is v2-vs-v2 self-consistency:
// (a) byte-identical across repeated renders (determinism), and (b) the tiled
// spatial-index path matches the brute-force untiled path within AA tolerance
// (the existing dual-path oracle, generalized over the whole corpus). When v3
// lands behind `sdf-v3`, the same corpus diffs v3 against this v2 oracle.
// ===========================================================================

use crate::drawable::Drawable;

/// Per-channel AA tolerance for the tiled-vs-untiled oracle on visible pixels.
const CORPUS_AA_TOL: i32 = 3;
/// Alpha below which a pixel is treated as background (ignored by the oracle).
const CORPUS_ALPHA_FLOOR: u8 = 100;

/// One golden-corpus scene: owned geometry + per-item style, plus the camera
/// setup needed to frame it. Items are listed in z-order (first drawn first,
/// i.e. farthest from the viewer).
struct Scene {
    name: &'static str,
    width: u32,
    height: u32,
    zoom: f32,
    /// Pinned animation time (seconds); fixed so animated scenes diff
    /// deterministically.
    time: f32,
    /// Explicit `camera_position`, or `None` to auto-center the world origin.
    camera: Option<[f32; 2]>,
    /// Whether the brute-force "untiled" path is a valid cross-check. The
    /// untiled fallback (`shader.wgsl` `grid_cols == 0`) evaluates the style
    /// once PER SEGMENT and composites, instead of folding all of an entry's
    /// segments into one nearest-distance SDF the way the tiled path does. It
    /// is therefore only correct when every entry is a SINGLE segment (strokes,
    /// beziers, tilings); a multi-segment fill (rounded rect, pin node) renders
    /// wrong under it. Multi-segment scenes are still covered by the
    /// determinism gate and the future v3-vs-v2-tiled diff; they just cannot
    /// use the tiled-vs-untiled oracle.
    untiled_safe: bool,
    items: Vec<(Drawable, Style)>,
}

impl Scene {
    fn pairs(&self) -> Vec<(&Drawable, &Style)> {
        self.items.iter().map(|(d, s)| (d, s)).collect()
    }
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> iced::Color {
    iced::Color::from_rgba(r, g, b, a)
}

/// The standard crossing-S edge used by the pattern scenes (matches the edge
/// editor default so pattern layout is exercised at a real curvature).
fn corpus_edge() -> Drawable {
    Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0])
}

/// `camera_position` that centers world point `p` in a `w`x`h` viewport at
/// `zoom` (scale 1.0). Mirrors `render_full`'s auto-center math.
fn camera_centered_on(p: [f32; 2], w: u32, h: u32, zoom: f32) -> [f32; 2] {
    [
        (w as f32) * 0.5 / zoom - p[0],
        (h as f32) * 0.5 / zoom - p[1],
    ]
}

/// A segment-dense pin node: a rounded body punched by a ring of pin cutouts,
/// composed into ONE shape via `difference_many` (the `boolean.rs` pin path).
fn pin_dense_node(center: [f32; 2]) -> Drawable {
    let body = Curve::rounded_rect(center, [70.0, 44.0], 10.0);
    let mut cuts = Vec::new();
    // Small cutouts centered on the left and right borders, mirroring the
    // widget's pin punches: each is a notch in the boundary, the composed
    // contour stays simple.
    for i in 0..6 {
        let t = -30.0 + i as f32 * 12.0;
        cuts.push(Curve::circle([center[0] - 70.0, center[1] + t], 3.5));
        cuts.push(Curve::circle([center[0] + 70.0, center[1] + t], 3.5));
    }
    crate::boolean::difference_many(&body, &cuts)
}

fn corpus() -> Vec<Scene> {
    let mut scenes = Vec::new();

    // --- Every pattern type, as a stroked edge ---
    let stroke_color = rgba(0.2, 0.85, 1.0, 1.0);
    let patterns: [(&'static str, Pattern); 6] = [
        ("pattern_solid", Pattern::solid(6.0)),
        ("pattern_dashed", Pattern::dashed(6.0, 14.0, 8.0)),
        ("pattern_arrowed", Pattern::arrowed(6.0, 16.0, 9.0)),
        ("pattern_dotted", Pattern::dotted(12.0, 4.0)),
        (
            "pattern_dash_dotted",
            Pattern::dash_dotted(6.0, 14.0, 8.0, 3.0),
        ),
        (
            "pattern_arrow_dotted",
            Pattern::arrow_dotted(6.0, 12.0, 8.0, 3.0),
        ),
    ];
    for (name, pat) in patterns {
        scenes.push(Scene {
            name,
            width: 256,
            height: 256,
            zoom: 1.0,
            time: 0.0,
            camera: None,
            untiled_safe: true,
            items: vec![(corpus_edge(), Style::stroke(stroke_color, pat))],
        });
    }

    // --- Pinned-time animated flowing dashed edge ---
    scenes.push(Scene {
        name: "animated_flow_dashed",
        width: 256,
        height: 256,
        zoom: 1.0,
        time: 0.37,
        camera: None,
        untiled_safe: true,
        items: vec![(
            corpus_edge(),
            Style::stroke(stroke_color, Pattern::dashed(6.0, 14.0, 8.0).flow(40.0)),
        )],
    });

    // --- Every tiling background ---
    let tile_color = rgba(0.5, 0.55, 0.65, 1.0);
    use crate::drawable::TilingType;
    let tilings: [(&'static str, Drawable, Style); 4] = [
        (
            "tiling_grid",
            Drawable::new_tiling(TilingType::Grid, [32.0, 32.0, 1.5, 0.0]),
            Style::solid(tile_color).expand(0.75),
        ),
        (
            "tiling_dots",
            Drawable::new_tiling(TilingType::Dots, [32.0, 32.0, 3.0, 0.0]),
            Style::solid(tile_color),
        ),
        (
            "tiling_triangles",
            Drawable::new_tiling(TilingType::Triangles, [40.0, 0.0, 1.5, 0.0]),
            Style::solid(tile_color).expand(0.75),
        ),
        (
            "tiling_hex",
            Drawable::new_tiling(TilingType::Hex, [40.0, 0.0, 1.5, 0.0]),
            Style::solid(tile_color).expand(0.75),
        ),
    ];
    for (name, drawable, style) in tilings {
        scenes.push(Scene {
            name,
            width: 256,
            height: 256,
            zoom: 1.0,
            time: 0.0,
            camera: None,
            untiled_safe: true,
            items: vec![(drawable, style)],
        });
    }

    // --- Segment-dense pin node (exercises one shape with ~20 segments) ---
    scenes.push(Scene {
        name: "pin_dense_node",
        width: 256,
        height: 256,
        zoom: 1.4,
        time: 0.0,
        camera: None,
        untiled_safe: false,
        items: vec![(
            pin_dense_node([0.0, 0.0]),
            Style::solid(rgba(0.3, 0.6, 0.9, 1.0)),
        )],
    });

    // --- Deep z-overlap: a stack of staggered filled rects ---
    {
        // Six staggered rects (4 segments each = 24 slots) all overlapping at
        // the center: within the single-tile slot budget so the untiled oracle
        // applies, while still exercising the z-order composite.
        let mut items = Vec::new();
        for i in 0..6 {
            let f = i as f32;
            let c = Curve::rect([-25.0 + f * 10.0, -25.0 + f * 10.0], [34.0, 34.0]);
            let col = rgba(0.15 + f * 0.13, 0.9 - f * 0.1, 0.4 + f * 0.08, 0.85);
            items.push((c, Style::solid(col)));
        }
        scenes.push(Scene {
            name: "z_deep_overlap",
            width: 256,
            height: 256,
            zoom: 1.2,
            time: 0.0,
            camera: None,
            untiled_safe: false,
            items,
        });
    }

    // --- Far-from-origin coordinates (precision: tiled must match untiled) ---
    {
        let p = [20000.0, 20000.0];
        scenes.push(Scene {
            name: "far_origin_node",
            width: 256,
            height: 256,
            zoom: 1.2,
            time: 0.0,
            camera: Some(camera_centered_on(p, 256, 256, 1.2)),
            untiled_safe: false,
            items: vec![(
                Curve::rounded_rect(p, [70.0, 44.0], 10.0),
                Style::solid(rgba(0.85, 0.55, 0.3, 1.0)),
            )],
        });
    }

    // --- Segment-dense pin node at a far origin (overflow + precision) ---
    {
        let p = [-15000.0, 12000.0];
        scenes.push(Scene {
            name: "pin_dense_far_origin",
            width: 256,
            height: 256,
            zoom: 1.4,
            time: 0.0,
            camera: Some(camera_centered_on(p, 256, 256, 1.4)),
            untiled_safe: false,
            items: vec![(pin_dense_node(p), Style::solid(rgba(0.3, 0.6, 0.9, 1.0)))],
        });
    }

    scenes
}

/// Render a corpus scene through the v2 SDF path with the tile spatial index
/// on or off.
fn render_scene_v2(r: &TestRenderer, scene: &Scene, use_tiles: bool) -> Vec<[u8; 4]> {
    r.render_full_t(
        &scene.pairs(),
        scene.width,
        scene.height,
        scene.zoom,
        1.0,
        use_tiles,
        scene.time,
        scene.camera,
        false,
    )
}

/// Render a corpus scene through the v3 keystone path: geometry localized to
/// each drawable's bounds-center, placement carried in the per-segment
/// translate. By translate invariance this must be pixel-identical to v2.
fn render_scene_v3(r: &TestRenderer, scene: &Scene, use_tiles: bool) -> Vec<[u8; 4]> {
    r.render_full_t(
        &scene.pairs(),
        scene.width,
        scene.height,
        scene.zoom,
        1.0,
        use_tiles,
        scene.time,
        scene.camera,
        true,
    )
}

/// Result of [`corpus_diff`]: worst per-channel diff, count exceeding
/// `CORPUS_AA_TOL`, and the first offending `(index, a, b)` sample.
type CorpusDiff = (i32, usize, Option<(usize, [u8; 4], [u8; 4])>);

/// Worst per-channel diff over visible pixels, plus the count exceeding
/// `CORPUS_AA_TOL` and the first offending sample.
fn corpus_diff(a: &[[u8; 4]], b: &[[u8; 4]]) -> CorpusDiff {
    let mut worst = 0i32;
    let mut over = 0usize;
    let mut sample = None;
    for (i, (pa, pb)) in a.iter().zip(b.iter()).enumerate() {
        if pa[3] < CORPUS_ALPHA_FLOOR && pb[3] < CORPUS_ALPHA_FLOOR {
            continue;
        }
        let d = (0..4)
            .map(|c| (pa[c] as i32 - pb[c] as i32).abs())
            .max()
            .unwrap();
        worst = worst.max(d);
        if d > CORPUS_AA_TOL {
            over += 1;
            if sample.is_none() {
                sample = Some((i, *pa, *pb));
            }
        }
    }
    (worst, over, sample)
}

/// STEP 0 gate, part (a): the v2 oracle is deterministic. Rendering a scene
/// twice yields a byte-identical framebuffer, so any future v3 diff reflects a
/// real change, not renderer nondeterminism.
#[test]
fn corpus_v2_is_deterministic() {
    let r = shared_renderer();
    for scene in corpus() {
        let a = render_scene_v2(&r, &scene, true);
        let b = render_scene_v2(&r, &scene, true);
        assert!(
            a == b,
            "scene `{}` is not deterministic across repeated renders",
            scene.name,
        );
    }
}

/// Oracle sanity: every corpus scene renders a plausible amount of content in
/// the shipping tiled path. Rules out the two silent-failure modes the
/// determinism gate alone would miss: a scene that renders nothing (so
/// determinism passes trivially) and a scene that fills the whole viewport (the
/// segment-overflow sign inversion). Each scene must cover between 1% and 92%
/// of the viewport with visible pixels.
#[test]
fn corpus_scenes_render_plausible_coverage() {
    let r = shared_renderer();
    for scene in corpus() {
        let px = render_scene_v2(&r, &scene, true);
        let total = px.len() as f32;
        let visible = px.iter().filter(|p| p[3] >= CORPUS_ALPHA_FLOOR).count() as f32;
        let frac = visible / total;
        assert!(
            (0.01..=0.92).contains(&frac),
            "scene `{}` covers {:.1}% of the viewport (expected 1%..92%); \
             0% means it rendered nothing, ~100% means a fill-everywhere bug",
            scene.name,
            frac * 100.0,
        );
    }
}

/// Phase A1 keystone gate: the v3 local-geometry + per-instance translate path
/// is pixel-equivalent to the v2 world-baked path on every corpus scene. This
/// is the translate-invariance proof (`|grad| = 1`, so distances - and thus AA
/// and band thresholds - are unchanged) that the whole dedup/arena architecture
/// rests on. Phase A gates on pixel-equality only.
#[test]
fn corpus_v3_localized_matches_v2() {
    let r = shared_renderer();
    for scene in corpus() {
        let v2 = render_scene_v2(&r, &scene, true);
        let v3 = render_scene_v3(&r, &scene, true);
        let (worst, over, sample) = corpus_diff(&v2, &v3);
        assert!(
            over == 0,
            "scene `{}`: v3 localized vs v2 differs on {over} visible pixels \
             (worst per-channel {worst}). First: {sample:?}",
            scene.name,
        );
    }
}

/// Phase A4 gate: an edge rendered as a v3 arc-spline (bezier approximated by
/// arcs/lines) is pixel-equal to the v2 cubic-bezier edge WITHIN AA TOLERANCE.
/// The arc-spline is within `tol` world units of the curve, so the SDF differs
/// by <= `tol`: a thin sub-pixel delta confined to the antialiased edge band,
/// never a structural divergence (this is the plan's accepted delta, not the
/// bit-identical bar). Asserts the two renders are structurally identical (no
/// pixel grossly different) and only a thin edge band differs.
#[test]
fn arc_spline_edge_matches_bezier() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let cps = (
        glam::Vec2::new(-120.0, -40.0),
        glam::Vec2::new(-40.0, -40.0),
        glam::Vec2::new(40.0, 40.0),
        glam::Vec2::new(120.0, 40.0),
    );
    // The oracle is the TRUE cubic (SEG_CUBIC / sd_bezier), not another
    // arc-spline: `Curve::bezier` now itself fits arcs, so comparing against it
    // would be arc-spline-vs-arc-spline and could not catch an arc-spline error.
    let bez = crate::drawable::Drawable::single_bezier(cps.0, cps.1, cps.2, cps.3);
    // Zoom-aware fine tolerance (sub-pixel at this zoom).
    let tol = 0.1 / zoom;
    let arcs = crate::drawable::Drawable::bezier_arcs(cps.0, cps.1, cps.2, cps.3, tol);

    // Arc-length must match so dash spacing / flow stay aligned with v2.
    let rel_len = (arcs.total_arc_length() - bez.total_arc_length()).abs() / bez.total_arc_length();
    assert!(
        rel_len < 0.01,
        "arc-spline arc-length drifted {rel_len} from bezier"
    );

    let color = rgba(0.2, 0.85, 1.0, 1.0);
    for (name, style) in [
        ("solid", Style::stroke(color, Pattern::solid(6.0))),
        (
            "dashed",
            Style::stroke(color, Pattern::dashed(6.0, 14.0, 8.0)),
        ),
    ] {
        let v2 = r.render_opts(&[(&bez, &style)], w, h, zoom, true);
        let v3 = r.render_opts(&[(&arcs, &style)], w, h, zoom, true);

        let total = v2.len();
        let mut visible = 0usize;
        let mut differ = 0usize; // per-channel diff over a clear threshold
        let mut worst = 0i32;
        for (a, b) in v2.iter().zip(v3.iter()) {
            if a[3] < CORPUS_ALPHA_FLOOR && b[3] < CORPUS_ALPHA_FLOOR {
                continue;
            }
            visible += 1;
            let d = (0..4)
                .map(|c| (a[c] as i32 - b[c] as i32).abs())
                .max()
                .unwrap();
            worst = worst.max(d);
            if d > 16 {
                differ += 1;
            }
        }
        // No structural divergence (a missing/extra arc would blow this up).
        assert!(
            worst < 140,
            "edge `{name}`: worst per-channel diff {worst} indicates a structural \
             arc-spline error, not a sub-pixel edge delta",
        );
        // Only a thin AA edge band may differ; the bulk must match exactly.
        let frac = differ as f32 / visible.max(1) as f32;
        assert!(
            frac < 0.04,
            "edge `{name}`: {:.1}% of visible pixels differ (>{differ} px of \
             {visible}); expected only a thin sub-pixel edge band. total={total}",
            frac * 100.0,
        );
    }
}

/// Backward / looping edge: a "backward" graph edge (output-right pin to a node
/// to the LEFT) has bezier control points that point away from each other, so
/// the cubic self-loops. The arc-spline of such a curve must still match the
/// true cubic - a full-circle or giant-arc artifact here (the reported bug)
/// blows up the diff. Rendered against the SEG_CUBIC oracle, not another
/// arc-spline. Also sweeps a non-origin offset (world-space fit precision).
#[test]
fn backward_edge_arc_spline_matches_cubic() {
    let r = shared_renderer();
    let (w, h, zoom) = (400u32, 300u32, 1.0f32);
    // Centered backward-loop control polygon (output heads right, target is left).
    let bases = [
        (
            glam::Vec2::new(100.0, -25.0),
            glam::Vec2::new(180.0, -25.0),
            glam::Vec2::new(-180.0, 25.0),
            glam::Vec2::new(-100.0, 25.0),
        ),
        // A vertical-ish backward edge (top/bottom pins) for a second config.
        (
            glam::Vec2::new(-30.0, -90.0),
            glam::Vec2::new(-30.0, 0.0),
            glam::Vec2::new(30.0, 0.0),
            glam::Vec2::new(30.0, -90.0),
        ),
    ];
    let color = rgba(0.2, 0.85, 1.0, 1.0);
    let style = Style::stroke(color, Pattern::solid(6.0));
    for (i, (a, b, c, d)) in bases.into_iter().enumerate() {
        let bez = crate::drawable::Drawable::single_bezier(a, b, c, d);
        let arcs = crate::drawable::Drawable::bezier_arcs(a, b, c, d, 0.1 / zoom);

        let v2 = r.render_opts(&[(&bez, &style)], w, h, zoom, true);
        let v3 = r.render_opts(&[(&arcs, &style)], w, h, zoom, true);

        let mut visible = 0usize;
        let mut differ = 0usize;
        let mut worst = 0i32;
        for (x, y) in v2.iter().zip(v3.iter()) {
            if x[3] < CORPUS_ALPHA_FLOOR && y[3] < CORPUS_ALPHA_FLOOR {
                continue;
            }
            visible += 1;
            let dd = (0..4)
                .map(|cc| (x[cc] as i32 - y[cc] as i32).abs())
                .max()
                .unwrap();
            worst = worst.max(dd);
            if dd > 16 {
                differ += 1;
            }
        }
        let frac = differ as f32 / visible.max(1) as f32;
        assert!(
            worst < 140,
            "backward edge {i}: worst diff {worst} - arc-spline diverged from \
             the cubic (full-circle/giant-arc artifact)",
        );
        assert!(
            frac < 0.05,
            "backward edge {i}: {:.1}% pixels differ - structural arc-spline error",
            frac * 100.0,
        );
    }
}

/// STEP 0 gate, part (b): the tiled spatial-index path matches the brute-force
/// untiled path within AA tolerance on every corpus scene. This is the v2
/// self-diff-zero oracle that v3 must later reproduce.
#[test]
fn corpus_v2_tiled_matches_untiled() {
    let r = shared_renderer();
    for scene in corpus() {
        if !scene.untiled_safe {
            continue;
        }
        let tiled = render_scene_v2(&r, &scene, true);
        let untiled = render_scene_v2(&r, &scene, false);
        let (worst, over, sample) = corpus_diff(&tiled, &untiled);
        assert!(
            over == 0,
            "scene `{}`: tiled vs untiled differs on {over} visible pixels \
             (worst per-channel {worst}). First: {sample:?}",
            scene.name,
        );
    }
}

/// C1 correctness guard (Phase C): the tile cull bins against the pattern's
/// PERPENDICULAR envelope and is conservative - every tile that renders a
/// non-zero pixel must have been binned, for EVERY pattern at EVERY angle
/// (under-inclusion is the bug; over-inclusion is fine). Verified through the
/// tiled-vs-untiled oracle: the untiled path applies no cull, so if the cull
/// dropped a tile the tiled render would be MISSING pixels the untiled render
/// has. A single straight stroke is one segment (untiled-safe), swept across
/// angles that straddle tile boundaries.
#[test]
fn c1_cull_conservative_for_all_patterns_at_swept_angles() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let color = rgba(0.9, 0.7, 0.2, 1.0);
    let patterns = [
        ("solid", Pattern::solid(6.0)),
        ("dashed", Pattern::dashed(6.0, 14.0, 8.0)),
        ("dotted", Pattern::dotted(16.0, 5.0)),
        ("arrowed", Pattern::arrowed(7.0, 18.0, 10.0)),
        ("arrow_dotted", Pattern::arrow_dotted(6.0, 16.0, 9.0, 4.0)),
    ];
    let angles_deg = [0.0f32, 17.0, 33.0, 45.0, 61.0, 79.0, 90.0, 113.0, 135.0];
    let l = 110.0f32;
    for (pname, pat) in patterns {
        let style = Style::stroke(color, pat);
        for deg in angles_deg {
            let a = deg.to_radians();
            let (c, s) = (a.cos(), a.sin());
            let line = Curve::line([-l * c, -l * s], [l * c, l * s]);
            let tiled = r.render_opts(&[(&line, &style)], w, h, zoom, true);
            let untiled = r.render_opts(&[(&line, &style)], w, h, zoom, false);
            let (worst, over, sample) = corpus_diff(&tiled, &untiled);
            assert!(
                over == 0,
                "C1 cull dropped pixels: pattern `{pname}` at {deg} deg - \
                 {over} px differ (worst {worst}). First: {sample:?}",
            );
        }
    }
}

/// C2 correctness guard (Phase C): when a 16px tile overflows its 32-slot budget,
/// the result degrades DETERMINISTICALLY and never flickers. The cull keeps the
/// NEAREST segments by distance-to-tile-centre (not insertion order), so even
/// though the regional candidate gather uses nondeterministic atomics, the kept
/// set - and thus the rendered output - is identical every frame. Renders a
/// segment-dense overlapping stack (far exceeding 32 slots in central tiles)
/// many times and asserts byte-identical output across all frames.
#[test]
fn c2_overflow_is_deterministic_no_flicker() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    // ~40 overlapping circles crowded into the centre: each is several segments,
    // so the central 16px tiles hold far more than the 32-slot budget.
    let mut drawables = Vec::new();
    for i in 0..40u32 {
        let a = i as f32 * 0.41;
        let rad = 6.0 + (i % 5) as f32;
        let cx = (a.cos()) * 10.0;
        let cy = (a.sin()) * 10.0;
        drawables.push(Curve::circle([cx, cy], rad));
    }
    let style = Style::solid(rgba(0.3, 0.6, 0.9, 1.0));
    let refs: Vec<(&Drawable, &Style)> = drawables.iter().map(|d| (d, &style)).collect();

    let first = r.render_opts(&refs, w, h, zoom, true);
    for frame in 1..64u32 {
        let again = r.render_opts(&refs, w, h, zoom, true);
        let differ = first
            .iter()
            .zip(again.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            differ == 0,
            "C2 overflow flickered: frame {frame} differs from frame 0 on \
             {differ} pixels (nondeterministic overflow drop)",
        );
    }
}

/// B0 primitive: `Buffer::write_at` rewrites exactly one slot in place (the
/// basis for R2 incremental command updates), leaving its neighbours untouched.
/// Reuses the shared device (a second device risks the documented multi-device
/// deadlock).
#[test]
fn buffer_write_at_updates_one_slot() {
    let r = shared_renderer();
    let mut buf: crate::pipeline::buffer::Buffer<GpuVec4> = crate::pipeline::buffer::Buffer::new(
        &r.device,
        Some("write_at_test"),
        BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
    );
    let _ = buf.push(&r.device, &r.queue, GpuVec4::new(1.0, 1.0, 1.0, 1.0));
    let _ = buf.push(&r.device, &r.queue, GpuVec4::new(2.0, 2.0, 2.0, 2.0));
    let _ = buf.push(&r.device, &r.queue, GpuVec4::new(3.0, 3.0, 3.0, 3.0));

    buf.write_at(&r.queue, 1, GpuVec4::new(9.0, 9.0, 9.0, 9.0));

    let item_size = GpuVec4::SHADER_SIZE.get() as usize;
    let total = (item_size * 3) as u64;
    let readback = r.device.create_buffer(&BufferDescriptor {
        label: None,
        size: total,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = r
        .device
        .create_command_encoder(&CommandEncoderDescriptor::default());
    enc.copy_buffer_to_buffer(buf.wgpu_buffer(), 0, &readback, 0, total);
    let idx = r.queue.submit(std::iter::once(enc.finish()));
    let slice = readback.slice(..);
    slice.map_async(MapMode::Read, |_| {});
    r.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(idx),
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    let data = slice.get_mapped_range();
    let floats: &[f32] = bytemuck::cast_slice(&data);
    assert_eq!(floats[0], 1.0, "slot 0 must be untouched");
    assert_eq!(floats[4], 9.0, "slot 1 must be rewritten");
    assert_eq!(floats[8], 3.0, "slot 2 must be untouched");
    drop(data);
    readback.unmap();
}

/// Static-background texture cache (Phase C, v3): a cache hit/populate blit must
/// be pixel-identical to a direct render, and a camera change must invalidate the
/// cache (render direct again, different pixels). The no-regression rule: the
/// FIRST sight of a key renders direct (v2 path); only a repeated key populates
/// the texture and then blits it - so a continuously panning scene never blits.
#[test]
fn bg_cache_blit_matches_direct_and_invalidates() {
    use iced_wgpu::graphics::Viewport;
    use iced_wgpu::primitive::{Pipeline, Primitive};

    let r = shared_renderer();
    let mut pipeline =
        crate::primitive::SdfPipeline::new(&r.device, &r.queue, TextureFormat::Rgba8Unorm);
    let (w, h) = (256u32, 256u32); // 256*4 = aligned bytes_per_row, no padding
    let viewport = Viewport::with_physical_size(iced::Size::new(w, h), 1.0);
    let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(w as f32, h as f32));

    let target = r.device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&TextureViewDescriptor::default());
    let readback = r.device.create_buffer(&BufferDescriptor {
        label: None,
        size: (w * h * 4) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let tiling_shape = crate::shape::Shape::tiling(crate::tiling::Tiling::grid(32.0, 32.0, 1.5));
    let style = Style::solid(rgba(0.5, 0.55, 0.65, 1.0));
    let make = |cx: f32| {
        let mut p = crate::primitive::SdfPrimitive::new();
        p.push(&tiling_shape, &style, [0.0, 0.0]);
        p.camera(cx, 20.0, 1.0).background()
    };

    let run = |pipeline: &mut crate::primitive::SdfPipeline,
               prim: &crate::primitive::SdfPrimitive|
     -> Vec<[u8; 4]> {
        Pipeline::trim(pipeline);
        prim.prepare(pipeline, &r.device, &r.queue, &bounds, &viewport);
        let mut enc = r
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            prim.draw(&*pipeline, &mut pass);
        }
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let idx = r.queue.submit(std::iter::once(enc.finish()));
        r.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: None,
            })
            .unwrap();
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        r.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        let data = slice.get_mapped_range();
        let px: Vec<[u8; 4]> = data
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        drop(data);
        readback.unmap();
        px
    };

    let f1 = run(&mut pipeline, &make(10.0));
    assert!(
        !pipeline.bg_cache_blitted(),
        "frame 1 (first key sight) must render direct, no regression on dynamic frames",
    );
    let f2 = run(&mut pipeline, &make(10.0));
    assert!(
        pipeline.bg_cache_blitted(),
        "frame 2 (key repeated) must populate the cache and blit",
    );
    let f3 = run(&mut pipeline, &make(10.0));
    assert!(
        pipeline.bg_cache_blitted(),
        "frame 3 must blit the cached texture"
    );

    assert_eq!(
        f1, f2,
        "populate+blit must be pixel-identical to a direct render"
    );
    assert_eq!(
        f1, f3,
        "cache-hit blit must be pixel-identical to a direct render"
    );

    // A camera change invalidates: render direct again, and the panned grid must
    // differ from the original frame.
    let f4 = run(&mut pipeline, &make(70.0));
    assert!(
        !pipeline.bg_cache_blitted(),
        "a changed camera must invalidate the cache and render direct",
    );
    assert_ne!(
        f1, f4,
        "panning the background must change the rendered pixels"
    );
}

/// A3 band-fold premultiplied blend: a falloff from opaque GREEN to TRANSPARENT
/// RED must stay green through the fade, not fringe toward red. Straight-alpha
/// in-loop mixing (the old behavior) pulls RGB toward the transparent stop's red
/// and fails this; premultiplied mixing keeps it green.
#[test]
fn premultiplied_band_blend_no_rgb_fringe() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let green = iced::Color::from_rgba(0.0, 1.0, 0.0, 1.0);
    let red_clear = iced::Color::from_rgba(1.0, 0.0, 0.0, 0.0);
    let style = Style {
        stops: vec![
            crate::style::Stop::new(0.0, green),
            crate::style::Stop::new(15.0, red_clear),
        ],
        pattern: None,
        distance_field: false,
        transfer: Default::default(),
    };
    let radius = 50.0_f32;
    let circle = Curve::circle([0.0, 0.0], radius);
    let px = r.render_opts(&[(&circle, &style)], w, h, zoom, true);

    // ~7 world px outside the edge (mid-falloff). Auto-camera centers world 0 at
    // the viewport center, so world (radius+7, 0) -> screen (128 + 57, 128).
    let sx = w / 2 + radius as u32 + 7;
    let sy = h / 2;
    let p = TestRenderer::pixel_at(&px, w, sx, sy);
    assert!(p[3] > 40, "falloff pixel should be visible: {p:?}");
    assert!(
        p[1] as i32 > p[0] as i32 + 40,
        "falloff must stay green (premultiplied), not fringe red: {p:?}",
    );
}

/// A2 gate (time-uniform hoist): time and camera are per-frame uniform values,
/// NOT baked into the input buffers (segments/entries/styles). Its plan gate is
/// "pan-static-graph pixel-equal": panning a static graph re-renders correctly
/// from the SAME geometry, and advancing time animates the pattern from that
/// same geometry - so a frame-surviving input buffer stays valid across both.
/// (`time` already lives in the per-frame `DrawData` uniform, never in the
/// input buffers, so the literal "separate uniform" hoist is unnecessary here.)
#[test]
fn a2_time_and_camera_are_per_frame_uniforms() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let edge = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let flow = Style::stroke(
        rgba(0.2, 0.85, 1.0, 1.0),
        Pattern::dashed(6.0, 14.0, 8.0).flow(40.0),
    );
    let d = [(&edge, &flow)];

    // Same geometry, two times: the flow phase animates (time is a per-frame
    // uniform driving the pattern, not baked into the segment buffer).
    let t0 = r.render_full_t(&d, w, h, zoom, 1.0, true, 0.0, None, false);
    let t1 = r.render_full_t(&d, w, h, zoom, 1.0, true, 0.25, None, false);
    assert!(
        corpus_diff(&t0, &t1).1 > 0,
        "advancing time must animate the flow"
    );

    // Same geometry, two cameras: panning a STATIC graph re-renders correctly
    // (the plan's pan-static gate) - the view shifts, the geometry does not.
    let solid = Style::stroke(rgba(0.9, 0.6, 0.2, 1.0), Pattern::solid(6.0));
    let s = [(&edge, &solid)];
    let cam_a = [(w as f32) * 0.5 / zoom, (h as f32) * 0.5 / zoom];
    let cam_b = [cam_a[0] - 30.0, cam_a[1]];
    let pa = r.render_full_t(&s, w, h, zoom, 1.0, true, 0.0, Some(cam_a), false);
    let pb = r.render_full_t(&s, w, h, zoom, 1.0, true, 0.0, Some(cam_b), false);
    assert!(
        corpus_diff(&pa, &pb).1 > 0,
        "panning a static graph must shift the view"
    );
    // And both pans render plausible content (the static geometry survives both).
    let vis = |px: &[[u8; 4]]| px.iter().filter(|p| p[3] >= CORPUS_ALPHA_FLOOR).count();
    assert!(
        vis(&pa) > 100 && vis(&pb) > 100,
        "both pans must render the edge"
    );
}

/// Regression (found in the 500-node visual sign-off): the two-level cull bins a
/// region's candidates into a 256-slot workgroup array; when MORE than 256
/// entries crowd one 256px region (e.g. zoomed far out), candidates past 256 were
/// silently dropped, so edges vanished. The overflow fallback (scan all entries
/// for the tile) must render every entry. Single-segment circles make the untiled
/// brute-force path a faithful oracle.
#[test]
fn crowded_region_over_256_entries_no_dropped_shapes() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let mut circles = Vec::new();
    for gy in 0..18 {
        for gx in 0..18 {
            let x = -110.0 + gx as f32 * 13.0;
            let y = -110.0 + gy as f32 * 13.0;
            circles.push(Curve::circle([x, y], 4.0));
        }
    } // 324 shapes in one 256px workgroup region -> exceeds the 256 cap
    let style = Style::solid(rgba(1.0, 1.0, 1.0, 1.0));
    let d: Vec<_> = circles.iter().map(|c| (c, &style)).collect();

    let tiled = r.render_opts(&d, w, h, zoom, true);
    let untiled = r.render_opts(&d, w, h, zoom, false);
    let vis = |px: &[[u8; 4]]| px.iter().filter(|p| p[3] >= CORPUS_ALPHA_FLOOR).count();
    let (vt, vu) = (vis(&tiled), vis(&untiled));
    assert!(vu > 1000, "oracle coverage too low: {vu}");
    let ratio = vt as f32 / vu as f32;
    assert!(
        ratio > 0.97,
        "tiled cull dropped crowded entries: {vt}/{vu} = {:.0}% (overflow fallback broken)",
        ratio * 100.0,
    );
}

/// Zoomed-OUT variant of the crowded-region regression: many entries spread over
/// a wide world area, rendered zoomed out so each screen-space region covers many
/// of them - the exact condition (per the 500-node sign-off) where the region
/// cull used to overflow and drop edges. Per-tile density stays low, so this
/// isolates the region-overflow fallback, not the per-tile slot cap.
#[test]
fn zoomed_out_many_entries_no_dropped_shapes() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 0.35f32);
    let mut circles = Vec::new();
    for gy in 0..20 {
        for gx in 0..20 {
            let x = -340.0 + gx as f32 * 36.0;
            let y = -340.0 + gy as f32 * 36.0;
            circles.push(Curve::circle([x, y], 6.0));
        }
    } // 400 shapes over a wide area -> one zoomed-out region holds >256
    let style = Style::solid(rgba(1.0, 1.0, 1.0, 1.0));
    let d: Vec<_> = circles.iter().map(|c| (c, &style)).collect();
    let tiled = r.render_opts(&d, w, h, zoom, true);
    let untiled = r.render_opts(&d, w, h, zoom, false);
    let vis = |px: &[[u8; 4]]| px.iter().filter(|p| p[3] >= CORPUS_ALPHA_FLOOR).count();
    let (vt, vu) = (vis(&tiled), vis(&untiled));
    assert!(vu > 300, "oracle coverage too low: {vu}");
    let ratio = vt as f32 / vu as f32;
    assert!(
        ratio > 0.95,
        "zoomed-out cull dropped entries: {vt}/{vu} = {:.0}%",
        ratio * 100.0,
    );
}

/// A tile holding more than MAX_SLOTS_PER_TILE segments must keep the NEAREST
/// ones, not an arbitrary first-32 by scan order. A near, tile-filling shape
/// pushed LAST (highest scan index) would be dropped by first-32; keep-nearest
/// retains it because its |dist| at the tile centre is smallest.
#[test]
fn overflowing_tile_keeps_nearest_not_first() {
    let r = shared_renderer();
    let (w, h, zoom) = (16u32, 16u32, 1.0f32); // a single 16px tile
    let white = Style::stroke(rgba(1.0, 1.0, 1.0, 1.0), Pattern::solid(1.0));
    let mut rings: Vec<crate::drawable::Drawable> = Vec::new();
    for k in 0..36 {
        let a = k as f32 / 36.0 * std::f32::consts::TAU;
        rings.push(Curve::circle([a.cos() * 7.0, a.sin() * 7.0], 0.8));
    }
    let near = Curve::circle([0.0, 0.0], 2.0);
    let red = Style::stroke(rgba(1.0, 0.1, 0.1, 1.0), Pattern::solid(8.0));
    let mut d: Vec<(&crate::drawable::Drawable, &Style)> =
        rings.iter().map(|c| (c, &white)).collect();
    d.push((&near, &red));

    let px = r.render_opts(&d, w, h, zoom, true);
    let p = TestRenderer::pixel_at(&px, w, 8, 8);
    assert!(
        p[0] as i32 > p[1] as i32 + 40 && p[0] as i32 > p[2] as i32 + 40,
        "overflowing tile must keep the nearest (red) shape pushed last, got {p:?}",
    );
}

/// At-scale integration gate: many realistic multi-segment nodes (rounded-rect
/// minus pin cutouts, as the widget builds) rendered through the REAL tiled cull,
/// v3 keystone (localized geometry + translate) vs v2 (world-baked). Exercises the
/// cull + keystone together at node-grid scale - the path the 500-node demo runs,
/// where the sign-off found bugs the small corpus scenes missed.
#[test]
fn many_multiseg_nodes_localized_matches_world_tiled() {
    let r = shared_renderer();
    let (w, h, zoom) = (512u32, 384u32, 1.0f32);
    let half = [40.0f32, 22.0f32];
    let radius = 6.0f32;
    let cut_offsets = [[-40.0f32, -8.0], [-40.0, 8.0], [40.0, 0.0]];
    let (n, cols) = (24usize, 6usize);
    let mut nodes: Vec<crate::drawable::Drawable> = Vec::new();
    for i in 0..n {
        let p = [(i % cols) as f32 * 80.0, (i / cols) as f32 * 60.0];
        let body = Curve::rounded_rect(p, half, radius);
        let cuts: Vec<_> = cut_offsets
            .iter()
            .map(|c| Curve::circle([p[0] + c[0], p[1] + c[1]], 3.0))
            .collect();
        nodes.push(crate::boolean::difference_many(&body, &cuts));
    }
    let style = Style::solid(rgba(0.8, 0.85, 0.9, 1.0));
    let d: Vec<(&crate::drawable::Drawable, &Style)> =
        nodes.iter().map(|nd| (nd, &style)).collect();

    let v3 = r.render_full_t(&d, w, h, zoom, 1.0, true, 0.0, None, true);
    let v2 = r.render_full_t(&d, w, h, zoom, 1.0, true, 0.0, None, false);
    let (worst, over, sample) = corpus_diff(&v3, &v2);
    assert!(
        over < 200,
        "v3 localized != v2 at node-grid scale: worst {worst}, over {over}, {sample:?}",
    );
}

/// A3 transfer (variant B) new-capability golden: a Gamma transfer warps the
/// stop-blend parameter `t`, biasing a RED->BLUE falloff toward the near (red)
/// stop versus the Linear identity. Gated as a NEW capability (it deliberately
/// differs from the untransformed render), not a v2 match.
#[test]
fn transfer_gamma_warps_blend_toward_near_stop() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let red = rgba(1.0, 0.0, 0.0, 1.0);
    let blue = rgba(0.0, 0.0, 1.0, 1.0);
    let mk = |t: crate::style::Transfer| crate::style::Style {
        stops: vec![
            crate::style::Stop::new(0.0, red),
            crate::style::Stop::new(20.0, blue),
        ],
        pattern: None,
        distance_field: false,
        transfer: t,
    };
    let circle = Curve::circle([0.0, 0.0], 30.0);
    let lin = mk(crate::style::Transfer::Linear);
    let gam = mk(crate::style::Transfer::Gamma(2.5));
    let px_lin = r.render_opts(&[(&circle, &lin)], w, h, zoom, true);
    let px_gam = r.render_opts(&[(&circle, &gam)], w, h, zoom, true);
    // ~10px outside the edge (mid-falloff): world (40,0) -> screen (128+40, 128).
    let (sx, sy) = (w / 2 + 40, h / 2);
    let pl = TestRenderer::pixel_at(&px_lin, w, sx, sy);
    let pg = TestRenderer::pixel_at(&px_gam, w, sx, sy);
    assert!(
        pl[3] > 200 && pg[3] > 200,
        "falloff must be opaque: {pl:?} {pg:?}"
    );
    assert!(
        pg[0] as i32 > pl[0] as i32 + 40,
        "Gamma must bias toward the near (red) stop vs Linear: lin {pl:?} gam {pg:?}",
    );
}

/// A3 sign-aware patterns new-capability golden: a DOTTED pattern on a CLOSED
/// contour keeps its dots on the OUTER half plus a thin inner line, so the
/// interior stays clean (no inward dot bulge). At dist -4 inside the contour the
/// old symmetric dot was opaque; sign-aware leaves it transparent. The dots still
/// appear on the outer half. Gated as a NEW capability, not a v2 match.
#[test]
fn sign_aware_dotted_border_no_inward_bulge() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let circle = Curve::circle([0.0, 0.0], 30.0);
    let style = Style::stroke(rgba(1.0, 1.0, 1.0, 1.0), Pattern::dotted(14.0, 5.0));
    let px = r.render_opts(&[(&circle, &style)], w, h, zoom, true);
    let at = |radius: f32, deg: f32| -> [u8; 4] {
        let a = deg.to_radians();
        let sx = (128.0 + radius * a.cos()) as u32;
        let sy = (128.0 + radius * a.sin()) as u32;
        TestRenderer::pixel_at(&px, w, sx, sy)
    };
    let mut inner_opaque = 0; // dist ~ -4 (inside): must be clean
    let mut outer_opaque = 0; // dist ~ +3 (outside): dots present
    for k in 0..72 {
        let deg = k as f32 * 5.0;
        if at(26.0, deg)[3] >= CORPUS_ALPHA_FLOOR {
            inner_opaque += 1;
        }
        if at(33.0, deg)[3] >= CORPUS_ALPHA_FLOOR {
            outer_opaque += 1;
        }
    }
    assert!(
        inner_opaque <= 4,
        "dots bulged inward (not sign-aware): {inner_opaque}/72"
    );
    assert!(
        outer_opaque >= 6,
        "dots missing on the outer half: {outer_opaque}/72"
    );
}

/// Rewrite `const {name}: u32 = <n>u;` in `src` to `= {value}u;`, whatever the
/// current literal - so the cap sweep stays independent of the production cap (it
/// would otherwise break the moment `MAX_SLOTS_PER_TILE` is bumped).
fn with_u32_const(src: &str, name: &str, value: u32) -> String {
    let needle = format!("const {name}: u32 = ");
    let i = src.find(&needle).expect("const decl present");
    let after = i + needle.len();
    let semi = src[after..].find(';').expect("decl ends in ;") + after;
    format!("{}{}u{}", &src[..after], value, &src[semi..])
}

/// The compute cull's two output buffers, read straight back for inspection: the
/// per-tile occupancy and slot list (the spatial index). The slot's second u32 is
/// the draw-entry index (world-baked compile, translate `(0,0)`), so a test can
/// ask which draw entries a tile actually holds.
struct CullReadback {
    grid_cols: u32,
    grid_rows: u32,
    /// Per-tile slot capacity this readback ran with (`MAX_SLOTS_PER_TILE` by
    /// default; the cap sweep varies it). Sets the per-tile stride into `slots`.
    slots_per_tile: u32,
    counts: Vec<u32>,
    /// Flattened `tile * slots_per_tile + slot` -> `[seg_idx, entry_idx]`.
    slots: Vec<[u32; 2]>,
}

impl CullReadback {
    /// Whether any slot of ANY tile references draw-entry `entry_idx`.
    fn entry_indexed_anywhere(&self, entry_idx: u32) -> bool {
        let stride = self.slots_per_tile as usize;
        for (t, &n) in self.counts.iter().enumerate() {
            for k in 0..n as usize {
                if self.slots[t * stride + k][1] == entry_idx {
                    return true;
                }
            }
        }
        false
    }

    fn tiles_at_cap(&self) -> usize {
        self.counts
            .iter()
            .filter(|&&c| c >= self.slots_per_tile)
            .count()
    }

    fn max_count(&self) -> u32 {
        self.counts.iter().copied().max().unwrap_or(0)
    }

    /// Whether tile `(col, row)` references draw-entry `entry_idx` in any slot.
    fn tile_references(&self, col: u32, row: u32, entry_idx: u32) -> bool {
        let t = (row * self.grid_cols + col) as usize;
        let n = self.counts[t] as usize;
        let base = t * self.slots_per_tile as usize;
        (0..n).any(|k| self.slots[base + k][1] == entry_idx)
    }

    /// Tile columns whose every tile is empty (a fully-empty column reads as a
    /// gray vertical bar when a background fills the rest).
    fn empty_columns(&self) -> usize {
        (0..self.grid_cols)
            .filter(|&col| {
                (0..self.grid_rows)
                    .all(|row| self.counts[(row * self.grid_cols + col) as usize] == 0)
            })
            .count()
    }
}

/// The demo-scale zoomed-out node+edge grid the 500-node demo runs (25x20 nodes,
/// 640 fan-out edges from a few source pins), the scene where edges were reported
/// missing. Returns owned drawables (nodes `0..n`, then edges) plus the viewport
/// `(w, h)`, the fit `zoom`, the centred `camera`, and `n`.
#[allow(clippy::type_complexity)]
fn edge_grid_cull_scene() -> (
    Vec<crate::drawable::Drawable>,
    Vec<crate::drawable::Drawable>,
    u32,
    u32,
    f32,
    [f32; 2],
    usize,
) {
    let (w, h) = (600u32, 400u32);
    let (cols, rows) = (25usize, 20usize);
    let (sx, sy) = (90.0f32, 80.0f32);
    let zoom = (w as f32 / (cols as f32 * sx)).min(h as f32 / (rows as f32 * sy)) * 0.92;
    let cam = [
        w as f32 * 0.5 / zoom - cols as f32 * sx * 0.5,
        h as f32 * 0.5 / zoom - rows as f32 * sy * 0.5,
    ];

    let n = cols * rows;
    let half = [35.0f32, 30.0];
    let mut nodes = Vec::with_capacity(n);
    for i in 0..n {
        let (c, rr) = ((i % cols) as f32, (i / cols) as f32);
        nodes.push(Curve::rounded_rect([c * sx, rr * sy], half, 6.0));
    }
    let sources = 16usize;
    let mut edges = Vec::new();
    for e in 0..640usize {
        let from = e % sources;
        let to = sources + (e * 7 + 3) % (n - sources);
        if to == from {
            continue;
        }
        let (fc, fr) = ((from % cols) as f32, (from / cols) as f32);
        let (tc, tr) = ((to % cols) as f32, (to / cols) as f32);
        let a = [fc * sx + half[0], fr * sy];
        let b = [tc * sx - half[0], tr * sy];
        let dx = (b[0] - a[0]).abs().max(sx);
        edges.push(Curve::bezier(
            a,
            [a[0] + dx * 0.5, a[1]],
            [b[0] - dx * 0.5, b[1]],
            b,
        ));
    }
    (nodes, edges, w, h, zoom, cam, n)
}

/// Visible edges (world bounds intersect the viewport) absent from EVERY tile of
/// `cull` - the directly-measurable "missing edges" count. `n` is the node count;
/// edge `ei` is draw-entry `n + ei`.
fn missing_visible_edges(
    cull: &CullReadback,
    edges: &[crate::drawable::Drawable],
    n: usize,
    w: u32,
    h: u32,
    zoom: f32,
    cam: [f32; 2],
) -> usize {
    let (wx0, wy0) = (-cam[0], -cam[1]);
    let (wx1, wy1) = (w as f32 / zoom - cam[0], h as f32 / zoom - cam[1]);
    edges
        .iter()
        .enumerate()
        .filter(|(ei, edge)| {
            let b = edge.bounds();
            let visible = b[2] >= wx0 && b[0] <= wx1 && b[3] >= wy0 && b[1] <= wy1;
            visible && !cull.entry_indexed_anywhere((n + ei) as u32)
        })
        .count()
}

/// Per-tile slot-cap sweep over the two-level cull, on the demo-scale grid: for
/// each `MAX_SLOTS_PER_TILE` candidate, recompile the cull, read the spatial index
/// straight back, and report how many visible edges it drops, tile saturation, and
/// the slot-buffer cost. This LOCALIZES the missing-edges bug to Layer 2 (per-tile
/// budget) and quantifies the capacity needed to stop dropping edges - the data to
/// choose a fix. Asserts the default cap (32) drops edges while a large-enough cap
/// drops none (capacity is the root cause), so the sweep stays a live regression.
#[test]
fn cull_cap_sweep_localizes_edge_drops() {
    let r = shared_renderer();
    let (nodes, edges, w, h, zoom, cam, n) = edge_grid_cull_scene();
    let node_style = Style::solid(rgba(0.8, 0.85, 0.9, 1.0));
    let edge_style = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));
    let mut d: Vec<(&crate::drawable::Drawable, &Style)> =
        nodes.iter().map(|nd| (nd, &node_style)).collect();
    for e in &edges {
        d.push((e, &edge_style));
    }

    let caps = [32u32, 64, 128, 256];
    let mut results = Vec::new();
    for &cap in &caps {
        let pipeline = r.compute_pipeline_capped(cap);
        let cull = r.cull_readback_with(&pipeline, cap, &d, w, h, zoom, cam);
        let missing = missing_visible_edges(&cull, &edges, n, w, h, zoom, cam);
        let slot_kb = cull.counts.len() as u64 * (cap as u64 * 2) * 4 / 1024;
        eprintln!(
            "cap {cap:>3}: max count {:>3}, {:>3} tiles at cap, {} edges MISSING, \
             {} empty cols, slot buffer {} KB",
            cull.max_count(),
            cull.tiles_at_cap(),
            missing,
            cull.empty_columns(),
            slot_kb,
        );
        results.push((cap, missing));
    }

    let at_default = results[0].1;
    let at_largest = results.last().unwrap().1;
    assert!(
        at_default > 0,
        "expected the default {}-slot cap to drop edges on this dense scene",
        caps[0],
    );
    assert_eq!(
        at_largest,
        0,
        "even a {}-slot cap still drops edges - the budget is not the (whole) cause",
        caps.last().unwrap(),
    );
}

/// HYPOTHESIS TEST: `SdfPrimitive::draw` reads its `DrawData` by a global counter
/// that assumes draw order == prepare order. iced renders with LAYERS, which can
/// draw in a different order than primitives were prepared. Here three primitives
/// are PREPARED as [left, edges, right] but DRAWN as [left, right, edges] - so the
/// edge's draw index points at the WRONG `DrawData`. If the edge then blobs, the
/// draw-order assumption is the root cause of the widget boxes.
#[test]
fn draw_order_mismatch_corrupts_edge() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;
    use iced_wgpu::graphics::Viewport;
    use iced_wgpu::primitive::{Pipeline, Primitive};

    let r = shared_renderer();
    let (w, h) = (640u32, 448u32);
    let viewport = Viewport::with_physical_size(iced::Size::new(w, h), 1.0);
    let full = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(w as f32, h as f32));
    let gray = Style::solid(rgba(0.3, 0.3, 0.3, 1.0));
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));

    let mut left = SdfPrimitive::new();
    left.push(&Shape::circle(20.0), &gray, [80.0, 80.0]);
    let left = left.camera(0.0, 0.0, 1.0);
    let mut edges = SdfPrimitive::new();
    edges.push(
        &Shape::bezier(
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
        &green,
        [0.0, 0.0],
    );
    let edges = edges.camera(0.0, 0.0, 1.0);
    let mut right = SdfPrimitive::new();
    right.push(&Shape::circle(20.0), &gray, [560.0, 80.0]);
    let right = right.camera(0.0, 0.0, 1.0);

    let mut pipeline =
        crate::primitive::SdfPipeline::new(&r.device, &r.queue, TextureFormat::Rgba8Unorm);
    let target = r.device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let vw = target.create_view(&TextureViewDescriptor::default());
    let readback = r.device.create_buffer(&BufferDescriptor {
        label: None,
        size: (w * h * 4) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    Pipeline::trim(&mut pipeline);
    // PREPARE order: left, edges, right.
    left.prepare(&mut pipeline, &r.device, &r.queue, &full, &viewport);
    edges.prepare(&mut pipeline, &r.device, &r.queue, &full, &viewport);
    right.prepare(&mut pipeline, &r.device, &r.queue, &full, &viewport);
    let mut enc = r
        .device
        .create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &vw,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        // DRAW order: left, right, edges (edges LAST - different from prepare).
        left.draw(&pipeline, &mut pass);
        right.draw(&pipeline, &mut pass);
        edges.draw(&pipeline, &mut pass);
    }
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: None,
            },
        },
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    let idx = r.queue.submit(Some(enc.finish()));
    let slice = readback.slice(..);
    slice.map_async(MapMode::Read, |_| {});
    r.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(idx),
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    let data = slice.get_mapped_range();
    let px: Vec<[u8; 4]> = data
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    drop(data);
    readback.unmap();

    let g = px
        .iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count();
    eprintln!("edge green with draw order != prepare order: {g} px");
    assert!(
        g < 6000,
        "draw-order mismatch made the edge render as a box: {g} green px",
    );
}

/// Replicate the EXACT widget frame sequence (from the logged `prepare` calls):
/// bg + shadows + the EDGE batch all at full (600x400) bounds, THEN eight small
/// per-node primitives (fills + pins) with their own offset clips - which grow the
/// tile buffer AFTER the edge batch's tiles are written. If the n2->n1 edge blobs
/// HERE, the tile-buffer growth-with-copy corrupts the already-written edge tiles.
#[test]
fn widget_frame_sequence_keeps_edges_as_strokes() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;
    use crate::tiling::Tiling;
    use iced::Rectangle;

    let r = shared_renderer();
    let (w, h) = (640u32, 448u32);
    let full = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(600.0, 400.0));

    let gray = Style::solid(rgba(0.30, 0.32, 0.40, 1.0));
    let dark = Style::solid(rgba(0.12, 0.13, 0.16, 1.0));
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));
    let positions = [
        [40.0f32, 40.0],
        [420.0, 60.0],
        [80.0, 300.0],
        [440.0, 320.0],
    ];

    let mut bg = SdfPrimitive::new();
    bg.push(
        &Shape::tiling(Tiling::grid(40.0, 40.0, 1.0)),
        &dark,
        [0.0, 0.0],
    );
    let bg = bg.camera(0.0, 0.0, 1.0).background();

    let mut shadows = SdfPrimitive::new();
    for p in positions {
        shadows.push(
            &Shape::rounded_box([70.0, 60.0], [6.0; 4]),
            &gray,
            [p[0] + 39.0, p[1] + 34.0],
        );
    }
    let shadows = shadows.camera(0.0, 0.0, 1.0);

    let mut edges = SdfPrimitive::new();
    for (p0, c0, c1, p1) in [
        ([110.0, 55.0], [190.0, 55.0], [340.0, 105.0], [420.0, 105.0]),
        ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
        (
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ] {
        edges.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
    }
    let edges = edges.camera(0.0, 0.0, 1.0);

    // Per-node fill + pins primitives with their own offset clips (logged bounds),
    // each carrying a small shape so it allocates tiles and grows the buffer.
    let per_node: Vec<(SdfPrimitive, Rectangle)> = positions
        .iter()
        .flat_map(|p| {
            let center = [p[0] + 35.0, p[1] + 30.0];
            let mut fill = SdfPrimitive::new();
            fill.push(&Shape::rounded_box([70.0, 60.0], [6.0; 4]), &gray, center);
            let fill = fill.camera(-p[0] + 2.0, -p[1] + 2.0, 1.0);
            let fill_bounds = Rectangle::new(
                iced::Point::new(p[0] - 2.0, p[1] - 2.0),
                iced::Size::new(74.0, 64.0),
            );

            let mut pins = SdfPrimitive::new();
            pins.push(&Shape::circle(4.0), &gray, [p[0], center[1]]);
            pins.push(&Shape::circle(4.0), &gray, [p[0] + 70.0, center[1]]);
            let pins = pins.camera(-p[0] + 5.36, -p[1] + 3.0, 1.0);
            let pins_bounds = Rectangle::new(
                iced::Point::new(p[0] - 5.36, p[1] - 3.0),
                iced::Size::new(79.76, 66.0),
            );
            [(fill, fill_bounds), (pins, pins_bounds)]
        })
        .collect();

    let mut seq: Vec<(&SdfPrimitive, Rectangle)> =
        vec![(&bg, full), (&shadows, full), (&edges, full)];
    for (p, b) in &per_node {
        seq.push((p, *b));
    }

    let px = r.render_primitives_scissored(&seq, w, h);
    write_png(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../seq_render.png"),
        &px,
        w,
        h,
    );
    let g = px
        .iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count();
    eprintln!("edge green in the full widget frame sequence: {g} px");
    assert!(
        g < 6000,
        "edges in the widget frame sequence rendered as boxes: {g} green px",
    );
}

/// Reproduce the minimal widget scene as the SAME primitive SEQUENCE the widget
/// submits (background tiling, then node fills, then the edge batch), rendered
/// through one pipeline like iced does. If the n2->n1 blob appears HERE, it is the
/// multi-primitive interaction (tile-base growth / bg cache), not the edge alone.
#[test]
fn minimal_scene_as_primitive_sequence_renders_edges_as_strokes() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;
    use crate::tiling::Tiling;

    let r = shared_renderer();
    let (w, h) = (640u32, 448u32);

    // Background tiling (marked as the cacheable background, like the widget).
    let mut bg = SdfPrimitive::new();
    bg.push(
        &Shape::tiling(Tiling::grid(40.0, 40.0, 1.0)),
        &Style::solid(rgba(0.12, 0.13, 0.16, 1.0)),
        [0.0, 0.0],
    );
    let bg = bg.camera(0.0, 0.0, 1.0).background();

    // Node fills (gray rounded boxes at the 4 node positions).
    let node_style = Style::solid(rgba(0.30, 0.32, 0.40, 1.0));
    let positions = [[40.0, 40.0], [420.0, 60.0], [80.0, 300.0], [440.0, 320.0]];
    let mut nodes = SdfPrimitive::new();
    for p in positions {
        nodes.push(
            &Shape::rounded_box([70.0, 60.0], [6.0; 4]),
            &node_style,
            [p[0] + 35.0, p[1] + 30.0],
        );
    }
    let nodes = nodes.camera(0.0, 0.0, 1.0);

    // Edge batch (the 3 edges, the blob is n2->n1).
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));
    let mut edges = SdfPrimitive::new();
    for (p0, c0, c1, p1) in [
        ([110.0, 55.0], [190.0, 55.0], [340.0, 105.0], [420.0, 105.0]),
        ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
        (
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ] {
        edges.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
    }
    let edges = edges.camera(0.0, 0.0, 1.0);

    let px = r.render_primitives(&[&bg, &nodes, &edges], w, h);
    write_png(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../seq_render.png"),
        &px,
        w,
        h,
    );
    let g = px
        .iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count();
    eprintln!("edge green in primitive sequence: {g} px");
    assert!(
        g < 6000,
        "edges in the widget primitive sequence rendered as boxes: {g} green px",
    );
}

/// The three EXACT widget edges from the minimal-scene PNG (pin coords computed by
/// hand). In the widget, node2->node1 renders as a filled blob while the other two
/// are clean curves. Rendered here at the same camera (0,0, zoom 1) the widget
/// uses, so a fill HERE means the geometry is the trigger, not the widget path.
#[test]
fn exact_widget_edges_render_as_strokes() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (w, h) = (640u32, 448u32);
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));

    type Cfg = (&'static str, [f32; 2], [f32; 2], [f32; 2], [f32; 2]);
    let edges: [Cfg; 3] = [
        (
            "n0->n1",
            [110.0, 55.0],
            [190.0, 55.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
        (
            "n0->n3",
            [110.0, 55.0],
            [190.0, 55.0],
            [360.0, 365.0],
            [440.0, 365.0],
        ),
        (
            "n2->n1 (blob)",
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ];

    // All three in ONE primitive (the widget batches all edges together).
    use iced_wgpu::graphics::Viewport;
    use iced_wgpu::primitive::{Pipeline, Primitive};
    let mut prim = SdfPrimitive::new();
    for (_, p0, c0, c1, p1) in edges {
        prim.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
    }
    let prim = prim.camera(0.0, 0.0, 1.0);

    // Dump the per-entry segment indices through the real prepare.
    let mut pipeline =
        crate::primitive::SdfPipeline::new(&r.device, &r.queue, TextureFormat::Rgba8Unorm);
    let viewport = Viewport::with_physical_size(iced::Size::new(w, h), 1.0);
    let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(w as f32, h as f32));
    Pipeline::trim(&mut pipeline);
    prim.prepare(&mut pipeline, &r.device, &r.queue, &bounds, &viewport);
    let segs = pipeline.segments_mirror().len();
    for (i, e) in pipeline.entries_mirror().iter().enumerate() {
        eprintln!(
            "entry {i} ({}): seg[{}..{}] / {segs} total, count {}",
            edges[i].0,
            e.segment_start,
            e.segment_start + e.segment_count,
            e.segment_count,
        );
    }

    let px = r.render_primitive(&prim, w, h);
    let g = px
        .iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count();
    eprintln!("all 3 edges together: {g} green px");
    assert!(
        g < 6000,
        "edges in one batch rendered as filled boxes: {g} green px",
    );
}

/// Write RGBA pixels to a PNG (debug aid: render a scene, then look at it).
fn write_png(path: &str, px: &[[u8; 4]], w: u32, h: u32) {
    let file = std::fs::File::create(path).unwrap();
    let bw = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(bw, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();
}

/// Render the demo's fan-out edge scene to a PNG so the boxes can be SEEN headless.
/// Edges only (green on transparent), the exact 25x20 / 640-edge / 16-source layout
/// the demo runs. Not an assertion - a visual probe. Writes to the repo root.
#[test]
#[ignore = "visual probe: writes edge_scene_render.png for manual inspection"]
fn dump_edge_scene_png() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (cols, rows) = (25usize, 20usize);
    let (sx, sy) = (90.0f32, 80.0f32);
    let n = cols * rows;
    let half = [35.0f32, 30.0];
    let (w, h) = (640u32, 448u32);
    let zoom = (w as f32 / (cols as f32 * sx)).min(h as f32 / (rows as f32 * sy)) * 0.92;
    let cam = [
        w as f32 * 0.5 / zoom - cols as f32 * sx * 0.5,
        h as f32 * 0.5 / zoom - rows as f32 * sy * 0.5,
    ];

    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));
    let mut prim = SdfPrimitive::new();
    let sources = 16usize;
    for e in 0..640usize {
        let from = e % sources;
        let to = sources + (e * 7 + 3) % (n - sources);
        if to == from {
            continue;
        }
        let (fc, fr) = ((from % cols) as f32, (from / cols) as f32);
        let (tc, tr) = ((to % cols) as f32, (to / cols) as f32);
        let a = [fc * sx + half[0], fr * sy];
        let b = [tc * sx - half[0], tr * sy];
        let dx = (b[0] - a[0]).abs().max(sx);
        prim.push(
            &Shape::bezier(a, [a[0] + dx * 0.5, a[1]], [b[0] - dx * 0.5, b[1]], b),
            &green,
            [0.0, 0.0],
        );
    }
    let prim = prim.camera(cam[0], cam[1], zoom);
    let px = r.render_primitive(&prim, w, h);
    write_png(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../edge_scene_render.png"),
        &px,
        w,
        h,
    );
}

/// Point on the cubic bezier at parameter `t`.
fn cubic_point(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

/// Distance from point `p` to the line segment `a`-`b` (clamped to the segment).
fn point_seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let c = [a[0] + t * ab[0], a[1] + t * ab[1]];
    ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt()
}

/// Ground-truth oracle for the two-level cull: a stroked line is rotated through a
/// small tile grid (it "dances" around the centre), and for EVERY orientation we
/// compute, per tile, whether the cull MUST reference the line's segment (the tile
/// centre is well inside the line) or MUST NOT (it is well outside). The readback
/// must agree. This pins the cull's spatial correctness AND that the segment data
/// reaches the right tiles - a dropped reference is the missing-edges failure, a
/// spurious far reference is the gray-bar failure. Distractor entries on either
/// side of the line make the line a non-zero entry index, exercising the index
/// plumbing. The middle band (corner-clip tiles) is left unasserted because the
/// cull is deliberately conservative (over-inclusion is allowed, drops are not).
#[test]
fn cull_references_line_in_expected_tiles() {
    let r = shared_renderer();
    let (w, h, zoom) = (128u32, 128u32, 1.0f32);
    // Camera so world origin sits at the viewport centre: world = screen/zoom - cam.
    let cam = [w as f32 * 0.5 / zoom, h as f32 * 0.5 / zoom];
    let grid_cols = (w as f32 / TILE_SIZE).ceil() as u32;
    let grid_rows = (h as f32 / TILE_SIZE).ceil() as u32;
    let thd = TILE_SIZE * 0.707_106_77 / zoom; // tile half-diagonal in world
    let half = TILE_SIZE * 0.5 / zoom; // tile half-width in world

    let stroke = Style::stroke(rgba(1.0, 1.0, 1.0, 1.0), Pattern::solid(2.0));
    let dot = Style::solid(rgba(1.0, 0.0, 0.0, 1.0));

    for k in 0..24u32 {
        let ang = k as f32 / 24.0 * std::f32::consts::TAU;
        let (dx, dy) = (ang.cos(), ang.sin());
        let len = 44.0;
        let a = [-dx * len, -dy * len];
        let b = [dx * len, dy * len];

        // Distractors far off-grid so the LINE is entry index 1 (index plumbing).
        let c0 = Curve::circle([1000.0, 1000.0], 5.0);
        let line = Curve::line(a, b);
        let c2 = Curve::circle([-1000.0, -1000.0], 5.0);
        let d: [(&crate::drawable::Drawable, &Style); 3] =
            [(&c0, &dot), (&line, &stroke), (&c2, &dot)];
        let cull = r.cull_readback(&d, w, h, zoom, cam);
        let line_idx = 1u32;

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                let center = [
                    (col as f32 + 0.5) * TILE_SIZE / zoom - cam[0],
                    (row as f32 + 0.5) * TILE_SIZE / zoom - cam[1],
                ];
                let dist = point_seg_dist(center, a, b);
                let refed = cull.tile_references(col, row, line_idx);

                // MUST reference: tile centre is within the line's own footprint.
                if dist <= half {
                    assert!(
                        refed,
                        "angle {k} ({:.0} deg): tile ({col},{row}) d={dist:.1} on the \
                         line but the cull dropped it",
                        ang.to_degrees(),
                    );
                }
                // MUST NOT reference: tile centre is well beyond any stroke reach.
                if dist > thd + 12.0 / zoom {
                    assert!(
                        !refed,
                        "angle {k} ({:.0} deg): tile ({col},{row}) d={dist:.1} far from \
                         the line but the cull referenced it (spurious)",
                        ang.to_degrees(),
                    );
                }
            }
        }
    }
}

/// Dedup/index validation through the REAL `SdfPipeline.prepare()` (the path the
/// widget runs, with `frame_shape_slots` sharing segment ranges between identical
/// shapes). The risk this pins: a DISTINCT edge wrongly sharing another's segment
/// range (false dedup) would make it render as a copy of the wrong geometry - a
/// "broken data fed to the cull" failure. Asserts: every entry's segment range is
/// in-buffer and matches its own shape's arc count, distinct edges get DISTINCT
/// ranges, a duplicate SHARES its original's range, and all bounds are finite.
#[test]
fn prepare_gives_distinct_edges_distinct_ranges() {
    use crate::primitive::{SdfPipeline, SdfPrimitive};
    use crate::shape::Shape;
    use iced_wgpu::graphics::Viewport;
    use iced_wgpu::primitive::{Pipeline, Primitive};

    let r = shared_renderer();
    let mut pipeline = SdfPipeline::new(&r.device, &r.queue, TextureFormat::Rgba8Unorm);
    let (w, h) = (256u32, 256u32);
    let viewport = Viewport::with_physical_size(iced::Size::new(w, h), 1.0);
    let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(w as f32, h as f32));

    let style = Style::stroke(rgba(1.0, 1.0, 1.0, 1.0), Pattern::solid(2.0));
    let edges: Vec<Shape> = (0..6u32)
        .map(|i| {
            let f = i as f32;
            Shape::bezier(
                [f * 7.0, 0.0],
                [20.0 + f, 10.0],
                [40.0 - f, -10.0 + f],
                [60.0, f * 5.0],
            )
        })
        .collect();

    let mut prim = SdfPrimitive::new();
    for e in &edges {
        prim.push(e, &style, [0.0, 0.0]);
    }
    // A duplicate of edge 0 (same control points) must SHARE its segment range.
    prim.push(&edges[0], &style, [0.0, 0.0]);

    Pipeline::trim(&mut pipeline);
    prim.prepare(&mut pipeline, &r.device, &r.queue, &bounds, &viewport);

    let entries = pipeline.entries_mirror().to_vec();
    let seg_len = pipeline.segments_mirror().len();
    assert_eq!(
        entries.len(),
        7,
        "6 distinct edges + 1 duplicate = 7 entries"
    );

    for (i, e) in entries.iter().enumerate() {
        let shape = if i < 6 { &edges[i] } else { &edges[0] };
        let want = shape.evaluate().segment_count();
        assert_eq!(
            e.segment_count as usize, want,
            "entry {i}: segment_count {} != shape's arc count {want}",
            e.segment_count,
        );
        assert!(
            (e.segment_start + e.segment_count) as usize <= seg_len,
            "entry {i}: range [{}, {}) out of the {seg_len}-segment buffer",
            e.segment_start,
            e.segment_start + e.segment_count,
        );
        assert!(
            e.bounds.0.iter().all(|v| v.is_finite()),
            "entry {i}: non-finite bounds {:?}",
            e.bounds.0,
        );
    }

    // The duplicate (entry 6) shares edge 0's (entry 0) range.
    assert_eq!(
        entries[6].segment_start, entries[0].segment_start,
        "duplicate edge must share the original's segment range",
    );
    // The 6 distinct edges must NOT share ranges (false dedup = wrong geometry).
    let mut starts: Vec<u32> = (0..6).map(|i| entries[i].segment_start).collect();
    starts.sort_unstable();
    starts.dedup();
    assert_eq!(
        starts.len(),
        6,
        "distinct edges collapsed onto shared ranges (false dedup): {:?}",
        (0..6).map(|i| entries[i].segment_start).collect::<Vec<_>>(),
    );
}

/// A stroked edge must render as a thin STROKE, not a solid fill of its bounding
/// box (the reported regression: some edges paint as a filled AABB in edge colour,
/// diagonals collapsing to smaller per-segment boxes). Sweeps a range of edge
/// orientations and aggressive tangents through the REAL pipeline and asserts the
/// green coverage stays stroke-sized, not box-sized.
#[test]
fn diagonal_edge_renders_as_stroke_not_box() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (w, h) = (256u32, 256u32);
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(3.0));

    // (label, p0, cp0, cp1, p1) - all in world coords centred near the origin,
    // mixing orientations and aggressive tangents (the widget builds edges with
    // pin-direction tangents that overshoot, so vertical/diagonal edges swing far).
    type Cfg = (&'static str, [f32; 2], [f32; 2], [f32; 2], [f32; 2]);
    let configs: [Cfg; 7] = [
        (
            "horizontal",
            [-80.0, 0.0],
            [-20.0, 0.0],
            [20.0, 0.0],
            [80.0, 0.0],
        ),
        (
            "diagonal",
            [-70.0, -70.0],
            [-10.0, -70.0],
            [10.0, 70.0],
            [70.0, 70.0],
        ),
        (
            "vertical-htan",
            [0.0, -60.0],
            [90.0, -60.0],
            [-90.0, 60.0],
            [0.0, 60.0],
        ),
        (
            "crossed-cusp",
            [-40.0, 0.0],
            [60.0, 0.0],
            [-60.0, 0.0],
            [40.0, 0.0],
        ),
        (
            "huge-tangent",
            [0.0, -50.0],
            [400.0, -50.0],
            [-400.0, 50.0],
            [0.0, 50.0],
        ),
        (
            "short-bigtan",
            [-8.0, -8.0],
            [120.0, -8.0],
            [-120.0, 8.0],
            [8.0, 8.0],
        ),
        (
            "backwards",
            [60.0, -50.0],
            [120.0, -50.0],
            [-120.0, 50.0],
            [-60.0, 50.0],
        ),
    ];

    let mut worst = (0usize, "", 0u32);
    for (i, (label, p0, c0, c1, p1)) in configs.iter().enumerate() {
        let mut prim = SdfPrimitive::new();
        // A leading entry so the edge under test is never entry 0 (real batches).
        prim.push(
            &Shape::line([-100.0, 95.0], [100.0, 95.0]),
            &green,
            [0.0, 0.0],
        );
        prim.push(&Shape::bezier(*p0, *c0, *c1, *p1), &green, [0.0, 0.0]);
        let prim = prim.camera(128.0, 128.0, 1.0); // world origin at viewport centre
        let px = r.render_primitive(&prim, w, h);
        let g = px
            .iter()
            .filter(|p| {
                (p[1] as i32) > (p[0] as i32) + 40
                    && (p[1] as i32) > (p[2] as i32) + 40
                    && p[1] > 80
            })
            .count() as u32;
        eprintln!("config {i} {label}: {g} green px");
        if g > worst.2 {
            worst = (i, label, g);
        }
    }

    // A leading 200px line (~600px) plus one bezier stroke (~few hundred px) is a
    // few thousand px; a filled bounding box would be 5-6 figures.
    assert!(
        worst.2 < 8000,
        "edge '{}' (config {}) rendered as a filled box: {} green px",
        worst.1,
        worst.0,
        worst.2,
    );
}

/// Edges are WORLD-BAKED (pushed with placement `[0,0]`, geometry in absolute
/// world coords) - unlike nodes (local geometry + a per-instance translate). The
/// 500-node demo spreads nodes thousands of units from the origin, so an edge's
/// bezier control points (and its biarc arc centres/radii) reach large magnitudes
/// where f32 loses precision. This renders ONE diagonal edge at growing world
/// offsets, viewport-centred each time, and asserts it stays a thin stroke - a
/// filled-box blowup at large offset is the reported "edges as boxes" regression.
#[test]
fn edge_at_large_world_coords_stays_a_stroke() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (w, h) = (256u32, 256u32);
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(3.0));

    for &off in &[0.0f32, 500.0, 3000.0, 30000.0, 300_000.0] {
        let e = Shape::bezier(
            [-70.0 + off, -70.0 + off],
            [-10.0 + off, -70.0 + off],
            [10.0 + off, 70.0 + off],
            [70.0 + off, 70.0 + off],
        );
        let mut prim = SdfPrimitive::new();
        prim.push(&e, &green, [0.0, 0.0]);
        // Camera so the edge centre (off, off) lands at the viewport centre.
        let prim = prim.camera(128.0 - off, 128.0 - off, 1.0);
        let px = r.render_primitive(&prim, w, h);
        let g = px
            .iter()
            .filter(|p| {
                (p[1] as i32) > (p[0] as i32) + 40
                    && (p[1] as i32) > (p[2] as i32) + 40
                    && p[1] > 80
            })
            .count();
        eprintln!("world offset {off:>8}: {g} green px");
        assert!(
            g < 6000,
            "edge at world offset {off} rendered as a filled box: {g} green px (f32 precision)",
        );
    }
}

/// Long cross-graph edges viewed ZOOMED OUT (the demo's actual condition for the
/// reported boxes): a long bezier biarc-subdivides into many arcs, each spanning a
/// large world region; zoomed out, a per-segment AABB fill would read as the small
/// boxes the user saw. Asserts the edge stays a thin stroke at several lengths/zooms.
#[test]
fn long_edge_zoomed_out_stays_a_stroke() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (w, h) = (256u32, 256u32);
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(3.0));

    for &(len, zoom) in &[(400.0f32, 0.5), (2000.0, 0.1), (6000.0, 0.03)] {
        // Diagonal-ish bezier with horizontal pin tangents (the widget's shape).
        let e = Shape::bezier(
            [-len, -len * 0.6],
            [-len * 0.3, -len * 0.6],
            [len * 0.3, len * 0.6],
            [len, len * 0.6],
        );
        let mut prim = SdfPrimitive::new();
        prim.push(&e, &green, [0.0, 0.0]);
        // World origin at the viewport centre: world = screen/zoom - cam => cam = 128/zoom.
        let prim = prim.camera(128.0 / zoom, 128.0 / zoom, zoom);
        let px = r.render_primitive(&prim, w, h);
        let g = px
            .iter()
            .filter(|p| {
                (p[1] as i32) > (p[0] as i32) + 40
                    && (p[1] as i32) > (p[2] as i32) + 40
                    && p[1] > 80
            })
            .count();
        eprintln!("len {len:>6} zoom {zoom}: {g} green px");
        assert!(
            g < 8000,
            "long edge (len {len}, zoom {zoom}) rendered as boxes: {g} green px",
        );
    }
}

/// An edge in a MULTI-PRIMITIVE frame must survive the tile-buffer growth that
/// later primitives trigger. The demo frame is bg + shadows + nodes + edges + pins
/// (and 45029 tiles), so the tile buffer grows several times mid-frame, each time
/// copy-preserving earlier primitives' tile regions. If that copy garbles the edge
/// primitive's region (it is NOT the last), the edge renders wrong - a candidate
/// for the reported boxes. The edge is rendered FIRST, then filler primitives each
/// allocate a full grid and force growth past the initial 256-tile capacity.
#[test]
fn edge_survives_multi_primitive_tile_growth() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    let r = shared_renderer();
    let (w, h) = (256u32, 256u32); // 16x16 = 256 tiles per primitive = the capacity
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(3.0));
    let red = Style::solid(rgba(1.0, 0.0, 0.0, 1.0));

    let mut edge = SdfPrimitive::new();
    edge.push(
        &Shape::bezier([-70.0, -70.0], [-10.0, -70.0], [10.0, 70.0], [70.0, 70.0]),
        &green,
        [0.0, 0.0],
    );
    let edge = edge.camera(128.0, 128.0, 1.0);

    // Eight filler primitives, each a full grid (256 tiles), drive the tile buffer
    // through several growths after the edge's region is already populated.
    let fillers: Vec<SdfPrimitive> = (0..8)
        .map(|i| {
            let mut p = SdfPrimitive::new();
            p.push(
                &Shape::circle(4.0).translate([-90.0, -90.0 + i as f32]),
                &red,
                [0.0, 0.0],
            );
            p.camera(128.0, 128.0, 1.0)
        })
        .collect();

    let mut prims: Vec<&SdfPrimitive> = vec![&edge];
    prims.extend(fillers.iter());
    let px = r.render_primitives(&prims, w, h);

    let g = px
        .iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count();
    eprintln!(
        "edge green after {} primitives (tile growth): {g} px",
        prims.len()
    );
    assert!(
        g < 6000,
        "edge garbled by multi-primitive tile growth: {g} green px",
    );
}

/// The widget builds edges with HORIZONTAL pin tangents and `adaptive_bezier_length`
/// (`L = min(80, dist/2)`). When the output pin is to the RIGHT of the input
/// (a "backwards" edge), the control points overshoot PAST each other and the
/// bezier curls into a loop - the widget's own comment warns "the SDF cannot
/// resolve cleanly and the cull drops along the inner side". This replicates that
/// exact shape across forward/backward/short configs and asserts each renders as a
/// thin stroke, not a filled AABB (the reported boxes).
#[test]
fn widget_edge_shape_renders_as_stroke() {
    use crate::primitive::SdfPrimitive;
    use crate::shape::Shape;

    // The widget's edge: Right (output) pin tangent +x, Left (input) pin tangent -x.
    fn widget_edge(p0: [f32; 2], p1: [f32; 2]) -> Shape {
        let d = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
        let l = 80.0f32.min(d * 0.5).max(1.0);
        Shape::bezier(p0, [p0[0] + l, p0[1]], [p1[0] - l, p1[1]], p1)
    }

    let r = shared_renderer();
    let (w, h) = (256u32, 256u32);
    let green = Style::stroke(rgba(0.0, 1.0, 0.0, 1.0), Pattern::solid(2.0));

    let configs: [(&str, [f32; 2], [f32; 2]); 8] = [
        ("forward", [-60.0, 0.0], [60.0, 0.0]),
        ("backward-flat", [60.0, 0.0], [-60.0, 0.0]),
        ("backward-tilt", [60.0, -10.0], [-60.0, 10.0]),
        ("backward-short", [30.0, -5.0], [-30.0, 5.0]),
        ("backward-tiny", [12.0, -2.0], [-12.0, 2.0]),
        ("backward-steep", [10.0, 60.0], [-10.0, -60.0]),
        ("backward-diag", [50.0, -50.0], [-50.0, 50.0]),
        ("near-coincident", [6.0, 1.0], [-6.0, -1.0]),
    ];

    let mut worst = ("", 0u32);
    for (label, p0, p1) in configs {
        let mut prim = SdfPrimitive::new();
        prim.push(&widget_edge(p0, p1), &green, [0.0, 0.0]);
        let prim = prim.camera(128.0, 128.0, 1.0);
        let px = r.render_primitive(&prim, w, h);
        let g = px
            .iter()
            .filter(|p| {
                (p[1] as i32) > (p[0] as i32) + 40
                    && (p[1] as i32) > (p[2] as i32) + 40
                    && p[1] > 80
            })
            .count() as u32;
        eprintln!("{label:>16}: {g} green px");
        if g > worst.1 {
            worst = (label, g);
        }
    }
    assert!(
        worst.1 < 6000,
        "widget edge '{}' rendered as a filled box: {} green px",
        worst.0,
        worst.1,
    );
}

/// Does the CULL reference a bezier edge's segments only along its CURVE, or does
/// it over-include them across the whole AABB? The biarc fits near-straight
/// sections with huge-radius arcs (radius ~1000 for a small edge); if the arc is
/// culled by distance to the FULL circle (ignoring its angular range), tiles far
/// from the actual curve but near the huge circle get the segment - the AABB-shaped
/// occupancy seen in the edge tile-debug heatmap. Reads the cull back for one
/// diagonal bezier and reports how many referenced tiles are FAR from the curve.
#[test]
fn cull_bezier_follows_curve_not_aabb() {
    let r = shared_renderer();
    let (w, h, zoom) = (256u32, 256u32, 1.0f32);
    let cam = [w as f32 * 0.5 / zoom, h as f32 * 0.5 / zoom];
    let grid_cols = (w as f32 / TILE_SIZE).ceil() as u32;
    let grid_rows = (h as f32 / TILE_SIZE).ceil() as u32;
    let thd = TILE_SIZE * 0.707_106_77 / zoom;

    let stroke = Style::stroke(rgba(1.0, 1.0, 1.0, 1.0), Pattern::solid(3.0));
    let (p0, c0, c1, p1) = ([-90.0, -90.0], [-30.0, -90.0], [30.0, 90.0], [90.0, 90.0]);
    let bez = Curve::bezier(p0, c0, c1, p1);
    let cull = r.cull_readback(&[(&bez, &stroke)], w, h, zoom, cam);

    // Dense samples of the true curve.
    let samples: Vec<[f32; 2]> = (0..=200)
        .map(|i| cubic_point(p0, c0, c1, p1, i as f32 / 200.0))
        .collect();

    let mut referenced = 0u32;
    let mut far = 0u32;
    let mut farthest = 0.0f32;
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !cull.tile_references(col, row, 0) {
                continue;
            }
            referenced += 1;
            let center = [
                (col as f32 + 0.5) * TILE_SIZE / zoom - cam[0],
                (row as f32 + 0.5) * TILE_SIZE / zoom - cam[1],
            ];
            let d = samples
                .iter()
                .map(|s| ((center[0] - s[0]).powi(2) + (center[1] - s[1]).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min);
            farthest = farthest.max(d);
            // A tile whose centre is > tile-diagonal + a generous stroke reach from
            // the true curve has no business holding the segment.
            if d > thd + 12.0 / zoom {
                far += 1;
            }
        }
    }
    eprintln!(
        "bezier cull: {referenced} tiles reference the edge, {far} are FAR from the curve (farthest {farthest:.0}px)",
    );
    assert_eq!(
        far, 0,
        "cull over-includes the bezier into {far} tiles far from its curve (AABB fill)",
    );
}
