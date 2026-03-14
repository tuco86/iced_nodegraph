//! SDF rendering primitive for Iced.
//!
//! Each `SdfPrimitive` represents a single SDF shape. Multiple primitives
//! share the same `SdfPipeline`, which batches them into shared GPU buffers
//! automatically via Iced's prepare/draw cycle.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use encase::ShaderSize;
use iced::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages, Device, Queue,
    TextureFormat,
};
use iced::Rectangle;
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};

use smallvec::SmallVec;

use crate::batch::SdfBatch;
use crate::compile::compile_into;
use crate::eval;
use crate::layer::Layer;
use crate::pipeline::{buffer, types};
use crate::shape::Sdf;
use crate::shared::SharedSdfResources;

/// Tile size in pixels for culling.
const TILE_SIZE: f32 = 16.0;

/// Minimum shape dimension (in pixels) to bother tiling.
/// Shapes smaller than this skip tiling entirely.
const MIN_TILING_SIZE: f32 = 64.0;

/// A single SDF shape primitive for rendering.
///
/// Each primitive carries its SDF tree and styling. During `prepare()`,
/// the shape is compiled to RPN ops and pushed into the pipeline's shared
/// GPU buffers. During `draw()`, the shape's instanced quad is rendered.
#[derive(Debug, Clone)]
pub struct SdfPrimitive {
    /// The SDF shape to render.
    pub shape: Sdf,
    /// Rendering layers (back to front). Inline for 1-3 layers (common case).
    pub layers: SmallVec<[Layer; 3]>,
    /// Screen-space bounding box [x, y, width, height] for the instanced quad.
    pub screen_bounds: [f32; 4],
    /// Camera position (world origin offset).
    pub camera_position: (f32, f32),
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Animation time in seconds.
    pub time: f32,
    /// Tile size override. None = use default TILE_SIZE. Some(0) = disable tiling.
    pub tile_size: Option<u32>,
    /// Debug visualization flags (bit 0: show tile borders).
    pub debug_flags: u32,
}

impl SdfPrimitive {
    /// Create a new SDF primitive with default settings.
    pub fn new(shape: Sdf) -> Self {
        Self {
            shape,
            layers: smallvec::smallvec![Layer::solid(iced::Color::WHITE)],
            screen_bounds: [0.0, 0.0, 100.0, 100.0],
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            tile_size: None,
            debug_flags: 0,
        }
    }

    /// Set rendering layers.
    pub fn layers(mut self, layers: impl Into<SmallVec<[Layer; 3]>>) -> Self {
        self.layers = layers.into();
        self
    }

    /// Set the screen-space bounding box for the instanced quad.
    pub fn screen_bounds(mut self, bounds: [f32; 4]) -> Self {
        self.screen_bounds = bounds;
        self
    }

    /// Set camera position and zoom.
    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    /// Set animation time.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }

    /// Set tile size for culling. `None` uses default (16px), `Some(0)` disables tiling.
    pub fn tile_size(mut self, size: Option<u32>) -> Self {
        self.tile_size = size;
        self
    }

    /// Set debug visualization flags (bit 0: show tile borders).
    pub fn debug_flags(mut self, flags: u32) -> Self {
        self.debug_flags = flags;
        self
    }

    /// Enable or disable tile culling debug overlay.
    pub fn debug_tiles(self, enabled: bool) -> Self {
        self.debug_flags(if enabled { 1 } else { 0 })
    }
}

/// Shared pipeline for all SDF primitives.
///
/// Accumulates shape data from multiple `prepare()` calls into shared GPU
/// buffers, then renders all shapes via instanced draw calls.
pub struct SdfPipeline {
    shared: Arc<SharedSdfResources>,
    uniform_buffer: iced::wgpu::Buffer,
    shapes_buffer: buffer::Buffer<types::ShapeInstance>,
    ops_buffer: buffer::Buffer<types::SdfOp>,
    layers_buffer: buffer::Buffer<types::SdfLayer>,
    bind_group: BindGroup,
    bind_group_generations: (u64, u64, u64),
    /// Tile counts per primitive (how many ShapeInstances each prepare() emitted).
    tile_counts: Vec<u32>,
    /// Atomic index into tile_counts for draw() calls.
    draw_index: AtomicU32,
    /// Scratch buffer reused across prepare calls for SDF compilation.
    compile_scratch: Vec<types::SdfOp>,
    /// Scratch buffer reused for uniform serialization.
    uniform_scratch: Vec<u8>,
}

impl Pipeline for SdfPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        let shared = SharedSdfResources::get_or_init(device, format);

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf_uniforms"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shapes_buffer = buffer::Buffer::new(
            device,
            Some("sdf_shapes_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let ops_buffer = buffer::Buffer::new(
            device,
            Some("sdf_ops_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let layers_buffer = buffer::Buffer::new(
            device,
            Some("sdf_layers_buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf_bind_group"),
            layout: &shared.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: shapes_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: ops_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: layers_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            shared,
            uniform_buffer,
            shapes_buffer,
            ops_buffer,
            layers_buffer,
            bind_group,
            bind_group_generations: (0, 0, 0),
            tile_counts: Vec::new(),
            draw_index: AtomicU32::new(0),
            compile_scratch: Vec::new(),
            uniform_scratch: Vec::new(),
        }
    }

    fn trim(&mut self) {
        self.shapes_buffer.clear();
        self.ops_buffer.clear();
        self.layers_buffer.clear();
        self.tile_counts.clear();
        self.draw_index.store(0, Ordering::Relaxed);
    }
}

impl Primitive for SdfPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Compile SDF tree to RPN ops (reuses scratch buffer)
        compile_into(self.shape.node(), &mut pipeline.compile_scratch);
        let ops_offset = pipeline.ops_buffer.len() as u32;
        let ops_count = pipeline.compile_scratch.len() as u32;
        let _ = pipeline
            .ops_buffer
            .push_bulk(device, queue, &pipeline.compile_scratch);

        // Convert and push layers (bulk)
        let layers_offset = pipeline.layers_buffer.len() as u32;
        let layers_count = self.layers.len() as u32;
        let gpu_layers: Vec<_> = self.layers.iter().map(|l| l.to_gpu()).collect();
        let _ = pipeline
            .layers_buffer
            .push_bulk(device, queue, &gpu_layers);

        // Determine effective tile size
        let effective_tile_size = match self.tile_size {
            Some(0) => 0.0, // tiling disabled
            Some(ts) => ts as f32,
            None => TILE_SIZE,
        };

        let [sx, sy, sw, sh] = self.screen_bounds;
        let use_tiling = effective_tile_size > 0.0
            && sw >= MIN_TILING_SIZE
            && sh >= MIN_TILING_SIZE;

        let tile_count = if use_tiling {
            // Compute max effect radius from all layers
            let max_radius = self.layers.iter().map(|l| l.max_effect_radius()).fold(0.0f32, f32::max);
            // Check if any layer fills the shape interior (no pattern = solid fill).
            // Fill layers render everywhere inside the boundary, so interior tiles
            // must never be culled.
            let has_fill = self.layers.iter().any(|l| l.is_fill());

            let tile = effective_tile_size;
            let tile_half_diag = tile * std::f32::consts::FRAC_1_SQRT_2;
            let inv_zoom = 1.0 / self.camera_zoom;

            let cols = ((sw / tile).ceil() as u32).max(1);
            let rows = ((sh / tile).ceil() as u32).max(1);

            let mut count = 0u32;
            for row in 0..rows {
                for col in 0..cols {
                    // Tile bounds in screen space
                    let tx = sx + col as f32 * tile;
                    let ty = sy + row as f32 * tile;
                    let tw = tile.min(sx + sw - tx);
                    let th = tile.min(sy + sh - ty);

                    // Tile center in screen space -> world space
                    let center_sx = tx + tw * 0.5;
                    let center_sy = ty + th * 0.5;
                    let world_x = center_sx * inv_zoom - self.camera_position.0;
                    let world_y = center_sy * inv_zoom - self.camera_position.1;

                    // Evaluate SDF at tile center
                    let result = eval::evaluate(
                        self.shape.node(),
                        glam::Vec2::new(world_x, world_y),
                    );

                    // Convert tile half-diagonal to world space for comparison
                    let world_half_diag = tile_half_diag * inv_zoom;

                    // Culling strategy depends on whether we have fill layers:
                    // - Outside tiles (dist > 0): always cull if beyond max_radius
                    // - Inside tiles (dist < 0): only cull for stroke-only shapes
                    //   (no fill). Fill layers render the entire interior, so
                    //   interior tiles must be kept.
                    let dist = result.dist;
                    let cull_dist = if has_fill { dist } else { dist.abs() };
                    if cull_dist - world_half_diag > max_radius {
                        continue;
                    }

                    let _ = pipeline.shapes_buffer.push(
                        device,
                        queue,
                        types::ShapeInstance {
                            bounds: glam::Vec4::new(tx, ty, tw, th),
                            ops_offset,
                            ops_count,
                            layers_offset,
                            layers_count,
                        },
                    );
                    count += 1;
                }
            }

            // If all tiles were culled, emit at least one (the full shape)
            // to avoid visual artifacts from rounding
            if count == 0 {
                let _ = pipeline.shapes_buffer.push(
                    device,
                    queue,
                    types::ShapeInstance {
                        bounds: glam::Vec4::new(sx, sy, sw, sh),
                        ops_offset,
                        ops_count,
                        layers_offset,
                        layers_count,
                    },
                );
                1
            } else {
                count
            }
        } else {
            // No tiling: single instance for the whole shape
            let _ = pipeline.shapes_buffer.push(
                device,
                queue,
                types::ShapeInstance {
                    bounds: glam::Vec4::new(sx, sy, sw, sh),
                    ops_offset,
                    ops_count,
                    layers_offset,
                    layers_count,
                },
            );
            1
        };

        pipeline.tile_counts.push(tile_count);

        // Write uniforms every prepare (bounds may change per-primitive)
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            camera_position: glam::Vec2::new(
                self.camera_position.0,
                self.camera_position.1,
            ),
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            num_ops: pipeline.ops_buffer.len() as u32,
            num_layers: pipeline.layers_buffer.len() as u32,
            debug_flags: self.debug_flags,
        };

        pipeline.uniform_scratch.clear();
        let mut uniform_writer = encase::UniformBuffer::new(&mut pipeline.uniform_scratch);
        uniform_writer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniform_buffer, 0, &pipeline.uniform_scratch);

        // Recreate bind group if any buffer was resized
        let current_generations = (
            pipeline.shapes_buffer.generation(),
            pipeline.ops_buffer.generation(),
            pipeline.layers_buffer.generation(),
        );
        if current_generations != pipeline.bind_group_generations {
            pipeline.bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("sdf_bind_group"),
                layout: &pipeline.shared.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniform_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: pipeline.shapes_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.ops_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: pipeline.layers_buffer.as_entire_binding(),
                    },
                ],
            });
            pipeline.bind_group_generations = current_generations;
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        let index = pipeline.draw_index.fetch_add(1, Ordering::Relaxed) as usize;
        let tile_count = pipeline
            .tile_counts
            .get(index)
            .copied()
            .unwrap_or(1);

        // Compute the starting slot by summing all previous tile counts
        let slot: u32 = pipeline.tile_counts[..index].iter().sum();

        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        // Draw all tiles for this shape as instanced quads
        render_pass.draw(0..6, slot..slot + tile_count);

        true
    }
}

/// A batch of SDF shapes rendered in a single draw call.
///
/// Uses the same [`SdfPipeline`] as [`SdfPrimitive`], sharing GPU buffers.
/// All shapes in the batch are drawn with one instanced draw call, avoiding
/// per-shape pipeline/bind-group overhead.
#[derive(Debug)]
pub struct SdfBatchPrimitive {
    /// The collected batch of shapes.
    pub batch: SdfBatch,
    /// Camera position (world origin offset).
    pub camera_position: (f32, f32),
    /// Camera zoom factor.
    pub camera_zoom: f32,
    /// Animation time in seconds.
    pub time: f32,
    /// Debug visualization flags.
    pub debug_flags: u32,
}

impl SdfBatchPrimitive {
    /// Create a new batch primitive.
    pub fn new(batch: SdfBatch) -> Self {
        Self {
            batch,
            camera_position: (0.0, 0.0),
            camera_zoom: 1.0,
            time: 0.0,
            debug_flags: 0,
        }
    }

    /// Set camera position and zoom.
    pub fn camera(mut self, x: f32, y: f32, zoom: f32) -> Self {
        self.camera_position = (x, y);
        self.camera_zoom = zoom;
        self
    }

    /// Set animation time.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }
}

impl Primitive for SdfBatchPrimitive {
    type Pipeline = SdfPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let shape_count = self.batch.shape_count() as u32;

        if shape_count == 0 {
            pipeline.tile_counts.push(0);
            return;
        }

        // Upload batch to shared pipeline buffers (handles offset adjustment)
        self.batch.upload(
            &mut pipeline.shapes_buffer,
            &mut pipeline.ops_buffer,
            &mut pipeline.layers_buffer,
            device,
            queue,
        );

        // Each shape = 1 instance (no tiling in batch mode)
        pipeline.tile_counts.push(shape_count);

        // Write uniforms
        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),
            camera_position: glam::Vec2::new(
                self.camera_position.0,
                self.camera_position.1,
            ),
            camera_zoom: self.camera_zoom,
            scale_factor: scale,
            time: self.time,
            num_ops: pipeline.ops_buffer.len() as u32,
            num_layers: pipeline.layers_buffer.len() as u32,
            debug_flags: self.debug_flags,
        };

        pipeline.uniform_scratch.clear();
        let mut uniform_writer = encase::UniformBuffer::new(&mut pipeline.uniform_scratch);
        uniform_writer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&pipeline.uniform_buffer, 0, &pipeline.uniform_scratch);

        // Recreate bind group if any buffer was resized
        let current_generations = (
            pipeline.shapes_buffer.generation(),
            pipeline.ops_buffer.generation(),
            pipeline.layers_buffer.generation(),
        );
        if current_generations != pipeline.bind_group_generations {
            pipeline.bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("sdf_bind_group"),
                layout: &pipeline.shared.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniform_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: pipeline.shapes_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: pipeline.ops_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: pipeline.layers_buffer.as_entire_binding(),
                    },
                ],
            });
            pipeline.bind_group_generations = current_generations;
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut iced::wgpu::RenderPass<'_>,
    ) -> bool {
        let index = pipeline.draw_index.fetch_add(1, Ordering::Relaxed) as usize;
        let tile_count = pipeline
            .tile_counts
            .get(index)
            .copied()
            .unwrap_or(0);

        if tile_count == 0 {
            return true;
        }

        let slot: u32 = pipeline.tile_counts[..index].iter().sum();

        render_pass.set_pipeline(&pipeline.shared.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..6, slot..slot + tile_count);

        true
    }
}
