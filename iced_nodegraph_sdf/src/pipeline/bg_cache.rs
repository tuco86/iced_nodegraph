//! Static-background texture cache (Phase C).
//!
//! The one full-coverage fragment cost the tile cull cannot prune is the bottom
//! background tiling: it reaches every tile by construction (grid/dots/hex over
//! the whole viewport). When the camera and the background style are static, that
//! fullscreen SDF fragment pass produces the SAME pixels every frame, so re-running
//! it is wasted fill-rate - the lever the plan names to cut fragment COST (not just
//! rate) for "static background + animated foreground" frames.
//!
//! This caches the rendered background to an owned texture and blits it on frames
//! whose background key is unchanged. Correctness rests on a no-regression rule:
//! on a CHANGED key (pan/zoom/style/resize, or a flowing animated background) the
//! background renders DIRECTLY to the frame with no extra pass - so a continuously
//! dynamic scene never pays a cost. Only once a key repeats (static
//! detected) does it populate the texture once and blit thereafter.
//!
//! The blit is a passthrough of premultiplied-alpha texels under the same
//! premultiplied blend the SDF pass uses, so a cache hit is pixel-identical to a
//! direct render (the dedicated golden test asserts this).

use iced::wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendState,
    ColorTargetState, ColorWrites, Device, Extent3d, FilterMode, FragmentState, FrontFace,
    MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, RenderPass, RenderPipeline, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexState,
};

/// Fullscreen-triangle passthrough blit: sample the cached background texture
/// at the fragment's screen UV and return it unchanged (premultiplied alpha).
const BLIT_WGSL: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_blit(@builtin(vertex_index) vid: u32) -> VsOut {
    // Same fullscreen triangle as the SDF pass (vid 0,1,2 -> covers the screen).
    let x = f32(i32(vid & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vid >> 1u)) * 4.0 - 1.0;
    var out: VsOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    // Clip xy in [-1,1] (y up) -> uv in [0,1] (y down) for texture sampling.
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_smp, in.uv);
}
"#;

/// Owned background-cache resources. Lazily created on first cacheable background
/// frame; the texture is recreated when the viewport size changes.
pub(crate) struct BgCache {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
    sampler: Sampler,
    format: TextureFormat,
    texture: Option<Texture>,
    view: Option<TextureView>,
    bind_group: Option<BindGroup>,
    size: (u32, u32),
    /// Background key the cached texture was rendered with; `None` when empty.
    key: Option<u64>,
    /// The key seen last frame, to detect a static (repeated) background.
    last_key: Option<u64>,
}

impl BgCache {
    pub(crate) fn new(device: &Device, format: TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("sdf_bg_blit_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sdf_bg_blit"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(BLIT_WGSL)),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sdf_bg_blit_pl"),
            bind_group_layouts: &[&layout],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sdf_bg_blit_pipe"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &module,
                entry_point: Some("vs_blit"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                front_face: FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &module,
                entry_point: Some("fs_blit"),
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
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("sdf_bg_blit_smp"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            layout,
            pipeline,
            sampler,
            format,
            texture: None,
            view: None,
            bind_group: None,
            size: (0, 0),
            key: None,
            last_key: None,
        }
    }

    /// What to do with the background this frame, given its content key and the
    /// current viewport size. Call once per frame for the background primitive.
    pub(crate) fn decide(&mut self, device: &Device, key: Option<u64>, w: u32, h: u32) -> BgMode {
        // A `None` key means "never cache" (animated/flowing background): always
        // render direct, and forget any static streak.
        let Some(key) = key else {
            self.key = None;
            self.last_key = None;
            return BgMode::Direct;
        };

        self.ensure_texture(device, w, h);
        let was = self.last_key.replace(key);

        if self.key == Some(key) {
            // Texture already holds this exact background: blit it (the win).
            BgMode::Blit
        } else if was == Some(key) {
            // Static detected (same key two frames running) but not yet cached:
            // render to the texture this transition frame, then blit henceforth.
            self.key = Some(key);
            BgMode::Populate
        } else {
            // Changed key (pan/zoom/style/resize): render direct, no extra pass,
            // so a continuously dynamic scene never regresses.
            self.key = None;
            BgMode::Direct
        }
    }

    fn ensure_texture(&mut self, device: &Device, w: u32, h: u32) {
        let w = w.max(1);
        let h = h.max(1);
        if self.texture.is_some() && self.size == (w, h) {
            return;
        }
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("sdf_bg_cache_tex"),
            size: Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: self.format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf_bg_blit_bg"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.texture = Some(texture);
        self.view = Some(view);
        self.bind_group = Some(bind_group);
        self.size = (w, h);
        // The new texture holds nothing valid for any key yet.
        self.key = None;
    }

    /// The texture view to render the background INTO (for `BgMode::Populate`).
    pub(crate) fn target_view(&self) -> &TextureView {
        self.view.as_ref().expect("ensure_texture ran in decide")
    }

    /// Blit the cached background into `pass` (for `BgMode::Blit`).
    pub(crate) fn blit(&self, pass: &mut RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, self.bind_group.as_ref().unwrap(), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// How to handle the background primitive this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BgMode {
    /// Render the SDF background straight to the frame (no cache, no extra cost).
    Direct,
    /// Render it to the cache texture this frame (transition), then blit it.
    Populate,
    /// Reuse the cached texture: just blit (the static-camera win).
    Blit,
}
