//! Renders every gallery scene headlessly to the PNG stills the docs show.
//!
//! Usage: `gallery_screenshots <out_dir> [scene theme]`. Without a scene the
//! process re-executes itself once per scene and rustdoc theme: one renderer
//! means one `SdfPipeline`, and that pipeline carries frame-surviving state -
//! the shape cache, the static-background texture cache, GPU buffers - so two
//! renders sharing it would corrupt each other. Each pair lands in
//! `<scene>.<theme>.png`; `<scene>.png` is a copy of the dark one, the file
//! GitHub, docs.rs and the landing-page cards show.

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

    use demo_common::{RUSTDOC_THEMES, rustdoc_theme};
    use demo_gallery::SCENES;
    use iced_runtime::core::{
        self as core, Event, Font, Pixels, Size, clipboard, mouse,
        renderer::{self, Headless},
        window,
    };
    use iced_runtime::user_interface::{self, UserInterface};

    /// Logical size of an embed; mirrors `.demo-frame` max-width and height in
    /// `site/demo.css`, which lays the still out at that size in CSS pixels so
    /// it lands where the live canvas draws.
    const SIZE: Size = Size {
        width: 900.0,
        height: 600.0,
    };

    /// Renders at 2x so the still stays sharp on a HiDPI display, where the
    /// canvas has the same physical resolution.
    const SCALE: f32 = 2.0;

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut args = std::env::args().skip(1);

        let out_dir = args
            .next()
            .ok_or("usage: gallery_screenshots <out_dir> [scene theme]")?;

        match (args.next(), args.next()) {
            (Some(scene), Some(theme)) => render(&out_dir, &scene, &theme),
            (None, None) => render_all(&out_dir),
            _ => Err("usage: gallery_screenshots <out_dir> [scene theme]".into()),
        }
    }

    fn render_all(out_dir: &str) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(out_dir)?;

        let exe = std::env::current_exe()?;

        for def in SCENES {
            for theme in RUSTDOC_THEMES {
                let status = process::Command::new(&exe)
                    .args([out_dir, def.name, theme])
                    .status()?;

                if !status.success() {
                    return Err(format!("rendering {} ({theme}) failed: {status}", def.name).into());
                }
            }

            let out = Path::new(out_dir);
            fs::copy(
                out.join(format!("{}.dark.png", def.name)),
                out.join(format!("{}.png", def.name)),
            )?;
        }

        Ok(())
    }

    fn render(out_dir: &str, scene: &str, theme: &str) -> Result<(), Box<dyn Error>> {
        let def = SCENES
            .iter()
            .find(|def| def.name == scene)
            .ok_or_else(|| format!("unknown scene: {scene}"))?;
        let page_theme =
            rustdoc_theme(theme).ok_or_else(|| format!("unknown rustdoc theme: {theme}"))?;

        let (mut scene, _boot) = (def.boot)();
        scene.set_theme(page_theme.clone());

        let mut renderer = pollster::block_on(<iced::Renderer as Headless>::new(
            Font::with_name("Fira Sans"),
            Pixels(16.0),
            None,
        ))
        .ok_or("no wgpu adapter: install a Vulkan driver (CI: mesa-vulkan-drivers)")?;

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
            &page_theme,
            &renderer::Style {
                text_color: page_theme.palette().text,
            },
            mouse::Cursor::Unavailable,
        );

        let physical = Size::new((SIZE.width * SCALE) as u32, (SIZE.height * SCALE) as u32);
        // The wgpu renderer has an inherent `screenshot` taking a `Viewport`;
        // name the trait so the logical-size-plus-scale one is selected.
        let rgba = Headless::screenshot(
            &mut renderer,
            physical,
            SCALE,
            page_theme.palette().background,
        );

        let path = Path::new(out_dir).join(format!("{}.{theme}.png", def.name));
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
