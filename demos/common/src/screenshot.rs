//! Screenshot capture utility for demos.
//!
//! Adds `--screenshot <path.png>` support to any Iced application.
//! The app renders, captures the window, saves as PNG, and exits.

use iced::{Subscription, Task, window};
use std::path::PathBuf;

/// Messages produced by the screenshot system.
#[derive(Clone)]
pub enum ScreenshotMessage {
    /// Window opened, we now have its ID.
    WindowOpened(window::Id),
    /// Take the screenshot now (after delay).
    TakeScreenshot(window::Id),
    /// Screenshot bytes received from the window.
    Captured(window::Screenshot),
}

impl std::fmt::Debug for ScreenshotMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowOpened(id) => f.debug_tuple("WindowOpened").field(id).finish(),
            Self::TakeScreenshot(id) => f.debug_tuple("TakeScreenshot").field(id).finish(),
            Self::Captured(s) => write!(f, "Captured({}x{})", s.size.width, s.size.height),
        }
    }
}

/// Manages screenshot capture lifecycle.
///
/// Embed this in your app state and wire up the subscription, update, and
/// task methods. Call `ScreenshotHelper::from_args()` to parse CLI flags.
///
/// # Usage
///
/// ```ignore
/// struct App {
///     screenshot: ScreenshotHelper,
///     // ...
/// }
///
/// // In new():
/// let screenshot = ScreenshotHelper::from_args();
///
/// // In update():
/// Message::Screenshot(msg) => return self.screenshot.update(msg);
///
/// // In subscription():
/// self.screenshot.subscription().map(Message::Screenshot)
/// ```
#[derive(Debug, Clone)]
pub struct ScreenshotHelper {
    output_path: Option<PathBuf>,
    done: bool,
}

impl ScreenshotHelper {
    /// Parse `--screenshot <path>` from CLI arguments.
    /// Returns a helper that does nothing if the flag is absent.
    pub fn from_args() -> Self {
        let mut output_path = None;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--screenshot" {
                output_path = args.next().map(PathBuf::from);
            }
        }
        Self {
            output_path,
            done: false,
        }
    }

    /// Whether screenshot mode is active.
    pub fn is_active(&self) -> bool {
        self.output_path.is_some() && !self.done
    }

    /// Subscription that detects window open and schedules a delayed screenshot.
    /// Returns `Subscription::none()` if screenshot mode is inactive.
    pub fn subscription(&self) -> Subscription<ScreenshotMessage> {
        if !self.is_active() {
            return Subscription::none();
        }
        window::open_events().map(ScreenshotMessage::WindowOpened)
    }

    /// Handle screenshot messages. Returns a Task that should be returned
    /// from the app's update function.
    pub fn update<M>(
        &mut self,
        message: ScreenshotMessage,
    ) -> Task<M>
    where
        M: From<ScreenshotMessage> + Send + 'static,
    {
        match message {
            ScreenshotMessage::WindowOpened(window_id) => {
                // Wait a bit for the GPU pipeline to render, then take screenshot
                Task::perform(
                    async {
                        async_io_sleep(500).await;
                    },
                    move |()| M::from(ScreenshotMessage::TakeScreenshot(window_id)),
                )
            }
            ScreenshotMessage::TakeScreenshot(window_id) => {
                window::screenshot(window_id)
                    .map(ScreenshotMessage::Captured)
                    .map(M::from)
            }
            ScreenshotMessage::Captured(screenshot) => {
                if let Some(ref path) = self.output_path {
                    if let Err(e) = save_png(path, &screenshot) {
                        eprintln!("Failed to save screenshot: {e}");
                    } else {
                        eprintln!("Screenshot saved: {}", path.display());
                    }
                }
                self.done = true;
                iced::exit()
            }
        }
    }
}

/// Platform-agnostic async sleep (no tokio dependency needed).
async fn async_io_sleep(ms: u64) {
    // Use a simple thread::sleep in a blocking task
    // This works because Iced's executor handles blocking tasks
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

fn save_png(path: &std::path::Path, screenshot: &window::Screenshot) -> Result<(), String> {
    let width = screenshot.size.width;
    let height = screenshot.size.height;
    let rgba = &screenshot.rgba;

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let writer = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png_writer = encoder.write_header().map_err(|e| e.to_string())?;
    png_writer.write_image_data(rgba).map_err(|e| e.to_string())?;

    Ok(())
}
