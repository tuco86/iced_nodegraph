//! Renders every gallery scene headlessly to the PNG fallback the docs show.
//!
//! Usage: `gallery_screenshots <out_dir> [scene]`. Without a scene name the
//! process re-executes itself once per scene: the SDF substrate caches
//! device-bound resources in a process-global keyed to the first wgpu device,
//! so two scenes in one process would share a corrupted pipeline cache.

/// Rendering needs a native wgpu adapter and the `png` encoder, neither of
/// which the package's wasm target carries; cargo still builds every bin of the
/// package for that target.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::error::Error;
    use std::fs::{self, File};
    use std::io::BufWriter;
    use std::path::Path;
    use std::process;

    use demo_gallery::SCENES;
    use iced_runtime::core::{
        self as core, Event, Font, Pixels, Size, clipboard, mouse,
        renderer::{self, Headless},
        window,
    };
    use iced_runtime::user_interface::{self, UserInterface};

    /// Logical size of an embed; mirrors `.demo-frame` max-width and height in
    /// `site/demo.css` so the still image is a 1:1 crop of the live canvas.
    const SIZE: Size = Size {
        width: 900.0,
        height: 600.0,
    };

    const SCALE: f32 = 2.0;

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut args = std::env::args().skip(1);

        let out_dir = args
            .next()
            .ok_or("usage: gallery_screenshots <out_dir> [scene]")?;

        match args.next() {
            Some(scene) => render(&out_dir, &scene),
            None => render_all(&out_dir),
        }
    }

    fn render_all(out_dir: &str) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(out_dir)?;

        let exe = std::env::current_exe()?;

        for def in SCENES {
            let status = process::Command::new(&exe)
                .args([out_dir, def.name])
                .status()?;

            if !status.success() {
                return Err(format!("rendering {} failed: {status}", def.name).into());
            }
        }

        Ok(())
    }

    fn render(out_dir: &str, scene: &str) -> Result<(), Box<dyn Error>> {
        let def = SCENES
            .iter()
            .find(|def| def.name == scene)
            .ok_or_else(|| format!("unknown scene: {scene}"))?;

        let (scene, _boot) = (def.boot)();

        let mut renderer = pollster::block_on(<iced::Renderer as Headless>::new(
            Font::with_name("Fira Sans"),
            Pixels(16.0),
            None,
        ))
        .ok_or("no wgpu adapter: install a Vulkan driver (CI: mesa-vulkan-drivers)")?;

        let theme = scene.theme();

        let mut ui = UserInterface::build(
            scene.view(),
            SIZE,
            user_interface::Cache::default(),
            &mut renderer,
        );

        let mut messages = Vec::new();
        let _ = ui.update(
            &[Event::Window(window::Event::RedrawRequested(
                core::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );

        ui.draw(
            &mut renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            mouse::Cursor::Unavailable,
        );

        let physical = Size::new((SIZE.width * SCALE) as u32, (SIZE.height * SCALE) as u32);
        // The wgpu renderer has an inherent `screenshot` taking a `Viewport`;
        // name the trait so the logical-size-plus-scale one is selected.
        let rgba = Headless::screenshot(&mut renderer, physical, SCALE, theme.palette().background);

        let path = Path::new(out_dir).join(format!("{}.png", def.name));
        write_png(&path, physical, &rgba)?;

        Ok(())
    }

    fn write_png(path: &Path, size: Size<u32>, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
        let writer = BufWriter::new(File::create(path)?);

        let mut encoder = png::Encoder::new(writer, size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(rgba)?;

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
