//! Headless pixel-level tests for SDF rendering.
//!
//! Renders predefined shapes to an offscreen texture and checks specific pixels.
//! Catches tile culling bugs, sign leaks, and pattern artifacts.

#![cfg(test)]

use encase::{ShaderSize, ShaderType, StorageBuffer, UniformBuffer, internal::WriteInto};
use wgpu::*;
use wgpu::util::DeviceExt;

use crate::compile::compile_drawable;
use crate::curve::Curve;
use crate::pattern::Pattern;
use crate::pipeline::types::*;
use crate::style::Style;

// Must match WGSL constants
const TILE_SIZE: f32 = 16.0;
const MAX_SLOTS_PER_TILE: u32 = 32;
const SLOT_STRIDE: u32 = MAX_SLOTS_PER_TILE * 2;

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

        let (device, queue) = pollster::block_on(adapter.request_device(
            &DeviceDescriptor {
                label: Some("sdf_test_device"),
                required_features: Features::empty(),
                required_limits: Limits::default(),
                ..Default::default()
            },
        ))
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
            _pad1: 0,
            _pad2: 0,
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
                BindGroupEntry { binding: 0, resource: draws_buf.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: entries_buf.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: segments_buf.as_entire_binding() },
                BindGroupEntry { binding: 3, resource: styles_buf.as_entire_binding() },
                BindGroupEntry { binding: 4, resource: tile_counts_buf.as_entire_binding() },
                BindGroupEntry { binding: 5, resource: tile_slots_buf.as_entire_binding() },
            ],
        });
        let compute_bg0 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group0_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: draws_buf.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: entries_buf.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: segments_buf.as_entire_binding() },
                BindGroupEntry { binding: 3, resource: styles_buf.as_entire_binding() },
            ],
        });
        let compute_bg1 = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.compute_group1_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: cu_buf.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: tile_counts_buf.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: tile_slots_buf.as_entire_binding() },
            ],
        });

        // Render target
        let texture = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d { width, height, depth_or_array_layers: 1 },
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
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor::default());

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
            TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d { width, height, depth_or_array_layers: 1 },
        );

        let sub_idx = self.queue.submit(std::iter::once(encoder.finish()));

        // Map and read
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(sub_idx),
            timeout: Some(std::time::Duration::from_secs(5)),
        }).unwrap();

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

    fn create_storage<T: ShaderType + ShaderSize + WriteInto>(&self, items: &[T]) -> Buffer {
        let mut scratch = Vec::new();
        let mut writer = StorageBuffer::new(&mut scratch);
        writer.write(items).expect("Failed to write storage buffer");
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &scratch,
            usage: BufferUsages::STORAGE,
        })
    }

    fn create_uniform<T: ShaderType + ShaderSize + WriteInto>(&self, item: &T) -> Buffer {
        let mut scratch = Vec::new();
        let mut writer = UniformBuffer::new(&mut scratch);
        writer.write(item).expect("Failed to write uniform buffer");
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &scratch,
            usage: BufferUsages::UNIFORM,
        })
    }

    fn create_render_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_storage(0, ShaderStages::VERTEX_FRAGMENT, DrawData::SHADER_SIZE.get()),
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
    let renderer = TestRenderer::new();
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
    assert!(!nonzero.is_empty(), "No visible pixels on the stroke center line");

    let expected_alpha = nonzero[0].1;
    for &&(x, alpha) in &nonzero {
        assert_eq!(
            alpha, expected_alpha,
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
    let renderer = TestRenderer::new();
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

/// Multi-style bezier (like edge editor) must not show horizontal artifacts
/// at tile row boundaries. Checks that pixels at y=tile_boundary are
/// consistent with their vertical neighbors.
#[test]
fn bezier_multi_style_no_row_artifacts() {
    let renderer = TestRenderer::new();
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
    let shadow = Style {
        near_start: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        near_end: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        far_start: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        far_end: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        dist_from: 0.0,
        dist_to: 10.0,
        pattern: None,
        distance_field: false,
    };

    let pixels = renderer.render(
        &[(&bezier, &stroke), (&bezier, &border), (&bezier, &shadow)],
        width, height, zoom,
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
    let renderer = TestRenderer::new();
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
    let shadow = Style {
        near_start: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35),
        near_end: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35),
        far_start: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.0),
        far_end: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.0),
        dist_from: 0.0,
        dist_to: 10.0,
        pattern: None,
        distance_field: false,
    };

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
    let mut significant_diffs: Vec<(u32, u32, [u8; 4], [u8; 4], i32)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let t = TestRenderer::pixel_at(&tiled, width, x, y);
            let u = TestRenderer::pixel_at(&untiled, width, x, y);
            // Only care about solidly visible pixels (shadow edge differences are expected)
            if t[3] < 100 && u[3] < 100 { continue; }
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
    let at_col_boundary = significant_diffs.iter()
        .filter(|&&(x, _, _, _, _)| x % (TILE_SIZE as u32) <= 1 || x % (TILE_SIZE as u32) >= (TILE_SIZE as u32) - 1)
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
    let renderer = TestRenderer::new();
    let width = 800u32;
    let height = 500u32;
    let extent = 160.0_f32;
    let zoom = height.min(width) as f32 * 0.333 / extent;

    // Use the actual edge editor bezier (longer, endpoints further from view center)
    let bezier = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
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
    // Test with flat color (no gradient) vs gradient to isolate arc-length cause
    let flat_border = Style::stroke(
        iced::Color::from_rgba(0.95, 0.75, 0.2, 1.0),
        Pattern::solid(border_total),
    );
    let drawables: Vec<(&crate::drawable::Drawable, &Style)> = vec![
        (&bezier, &flat_border),
    ];

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
        if accel > max_accel { max_accel = accel; }
        // Flag positions where acceleration is suspiciously high
        if accel > 0.15 {
            wobbles.push((x, y_curr, accel));
        }
    }

    // Check if wobbles correlate with tile boundaries
    let at_tile_boundary: Vec<_> = wobbles.iter()
        .filter(|&&(x, _, _)| x % (TILE_SIZE as u32) <= 1 || x % (TILE_SIZE as u32) >= (TILE_SIZE as u32) - 1)
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
    let p0 = [-100.0f32, -30.0]; let p1 = [-30.0, -30.0];
    let p2 = [30.0, 30.0]; let p3 = [100.0, 30.0];
    let zoom = 500.0f32 * 0.333 / 160.0;

    // Scan along the outer edge of a 16-wide border at zoom
    let half_t = 8.0f32;
    let cam_x = 800.0 * 0.5 / zoom;
    let cam_y = 500.0 * 0.5 / zoom;

    let mut edge_y_positions = Vec::new();
    for px_x in 200..600u32 {
        let world_x = px_x as f32 / zoom - cam_x;
        // Binary search for the world_y where dist = half_t (edge)
        let mut y_lo = -100.0f32;
        let mut y_hi = 100.0f32;
        for _ in 0..40 {
            let y_mid = (y_lo + y_hi) * 0.5;
            let dist = cpu_bezier_dist(world_x, y_mid, &p0, &p1, &p2, &p3);
            if dist < half_t { y_lo = y_mid; } else { y_hi = y_mid; }
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
        wobbles.len(), &wobbles[..wobbles.len().min(10)],
    );
}

fn cpu_newton_refine(px: f32, py: f32, t0: f32, p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2]) -> (f32, f32) {
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

fn cpu_bezier_dist(px: f32, py: f32, p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2]) -> f32 {
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
            second_t = best_t; second_dist = best_dist;
            best_dist = d; best_t = t;
        } else if d < second_dist {
            second_t = t; second_dist = d;
        }
    }
    // Refine both candidates
    let (_, d1) = cpu_newton_refine(px, py, best_t, p0, p1, p2, p3);
    let (_, d2) = cpu_newton_refine(px, py, second_t, p0, p1, p2, p3);
    d1.min(d2)
}

fn cpu_bez_pt(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [u*u*u*p0[0] + 3.0*u*u*t*p1[0] + 3.0*u*t*t*p2[0] + t*t*t*p3[0],
     u*u*u*p0[1] + 3.0*u*u*t*p1[1] + 3.0*u*t*t*p2[1] + t*t*t*p3[1]]
}

fn cpu_bez_deriv(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [3.0*u*u*(p1[0]-p0[0]) + 6.0*u*t*(p2[0]-p1[0]) + 3.0*t*t*(p3[0]-p2[0]),
     3.0*u*u*(p1[1]-p0[1]) + 6.0*u*t*(p2[1]-p1[1]) + 3.0*t*t*(p3[1]-p2[1])]
}

fn cpu_bez_deriv2(p0: &[f32; 2], p1: &[f32; 2], p2: &[f32; 2], p3: &[f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [6.0*u*(p2[0]-2.0*p1[0]+p0[0]) + 6.0*t*(p3[0]-2.0*p2[0]+p1[0]),
     6.0*u*(p2[1]-2.0*p1[1]+p0[1]) + 6.0*t*(p3[1]-2.0*p2[1]+p1[1])]
}

/// Check for missing rows at tile boundaries inside the stroke.
/// The stroke center should have consistent alpha - any drop at y%16==0 is a bug.
#[test]
fn no_missing_rows_in_stroke() {
    let renderer = TestRenderer::new();
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
    let shadow = Style {
        near_start: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35),
        near_end: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.35),
        far_start: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.0),
        far_end: iced::Color::from_rgba(0.0, 0.0, 0.1, 0.0),
        dist_from: 0.0, dist_to: 10.0,
        pattern: None, distance_field: false,
    };

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
    let renderer = TestRenderer::new();
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
    let renderer = TestRenderer::new();
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
        above[0..3], below[0..3],
        "Distance field should show different colors on each side of the line. \
         Above: {above:?}, Below: {below:?}",
    );
}
