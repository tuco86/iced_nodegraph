//! Shared headless GPU harness for the pixel-oracle test binaries.
//!
//! Each `tests/*.rs` file is its OWN test binary, so this module is compiled
//! once PER binary: every binary gets its own `shared()` renderer and thus an
//! isolated `SdfPipeline`. That isolation is deliberate - the pipeline carries
//! frame-surviving state (the shape cache, the static-background texture cache,
//! GPU buffers) ACROSS renders, so two different scenes sharing one pipeline can
//! corrupt each other's render. Keeping each scene's tests in their own binary
//! sidesteps that without touching production code; within a binary every test
//! renders the SAME scene, so the shared pipeline stays consistent.

#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};

use iced::{Font, Pixels};
use iced_wgpu::graphics::Shell;
use iced_wgpu::wgpu;
use iced_wgpu::{Engine, Renderer};

/// One shared headless renderer for the whole binary, behind a mutex.
///
/// A real app owns ONE wgpu device, and the SDF substrate caches device-bound
/// resources (`SharedSdfResources`) in a global keyed to the first device it
/// sees. A second device would make those resources invalid ("Invalid resource"
/// in wgpu-core), and many concurrent devices can deadlock some drivers. Sharing
/// one device and serializing the GPU-touching tests behind this mutex avoids
/// both. `None` => no GPU adapter, so callers skip rather than fail.
pub fn shared() -> Option<MutexGuard<'static, Renderer>> {
    static SHARED: OnceLock<Option<Mutex<Renderer>>> = OnceLock::new();
    SHARED
        .get_or_init(|| headless_renderer().map(Mutex::new))
        .as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A headless `iced_wgpu::Renderer`, or `None` when no GPU adapter is available
/// (so the suite skips instead of failing on a GPU-less CI box).
fn headless_renderer() -> Option<Renderer> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("widget_pixel_oracle"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits {
            max_bind_groups: 2,
            ..wgpu::Limits::default()
        },
        ..Default::default()
    }))
    .ok()?;
    // No MSAA: the SDF path is analytically antialiased and we want byte-stable
    // output for determinism checks (no multisample resolve variance).
    let engine = Engine::new(
        &adapter,
        device,
        queue,
        wgpu::TextureFormat::Rgba8Unorm,
        None,
        Shell::headless(),
    );
    Some(Renderer::new(engine, Font::default(), Pixels(16.0)))
}
