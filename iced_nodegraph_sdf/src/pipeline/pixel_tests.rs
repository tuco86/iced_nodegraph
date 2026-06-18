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
const MAX_SLOTS_PER_TILE: u32 = 32;
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

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("sdf_test_device"),
            required_features: Features::empty(),
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

    fn render_full(
        &self,
        drawables: &[(&crate::drawable::Drawable, &Style)],
        width: u32,
        height: u32,
        zoom: f32,
        scale: f32,
        use_tiles: bool,
    ) -> Vec<[u8; 4]> {
        // Compile Rust -> GPU data
        let mut gpu_segments = Vec::new();
        let mut gpu_entries = Vec::new();
        let mut gpu_styles = Vec::new();

        for (i, (drawable, style)) in drawables.iter().enumerate() {
            let seg_offset = gpu_segments.len() as u32;
            let (mut entry, gpu_style) =
                compile_drawable(drawable, style, i as u32, 0, &mut gpu_segments);
            entry.style_idx = gpu_styles.len() as u32;
            // Fix segment_start: compile_drawable uses segment_base=0, offset is already correct
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
        let cam_x = (width as f32) * 0.5 / cs;
        let cam_y = (height as f32) * 0.5 / cs;

        let draw_data = DrawData {
            bounds_origin: GpuVec2::new(0.0, 0.0),
            camera_position: GpuVec2::new(cam_x, cam_y),
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
