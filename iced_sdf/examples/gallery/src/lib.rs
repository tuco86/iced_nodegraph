//! SDF Gallery - Interactive showcase of 2D SDF primitives.
//!
//! Browse through SDF shapes from Inigo Quilez's 2D distance functions
//! library, rendered in real-time via iced_sdf.
//!
//! ## Interactive Demo
//!
//! <link rel="stylesheet" href="pkg/demo.css">
//! <div id="demo-container">
//!   <div id="demo-loading">
//!     <div class="demo-spinner"></div>
//!     <p>Loading demo...</p>
//!   </div>
//!   <div id="demo-canvas-container"></div>
//!   <div id="demo-error">
//!     <strong>Failed to load demo.</strong> WebGPU required.
//!   </div>
//! </div>
//! <script type="module" src="pkg/demo-loader.js"></script>
//!
//! ## Usage
//!
//! - Click shapes in the sidebar to preview them
//! - URL params: `?shape=<slug>` selects initial shape, `?embed=true` hides sidebar

mod shapes;
mod widget;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::window;
use iced::{Color, Element, Fill, Subscription, Theme};
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

pub fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("SDF Gallery - iced_sdf")
        .theme(App::theme)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_demo() {
    let _ = main();
}

struct App {
    selected: usize,
    embed: bool,
    start_time: Instant,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Tick,
}

/// Parse URL query parameters on WASM targets.
#[cfg(target_arch = "wasm32")]
fn parse_url_params() -> (Option<String>, bool) {
    let window = web_sys::window().unwrap();
    let search = window.location().search().unwrap_or_default();
    let params = web_sys::UrlSearchParams::new_with_str(&search).unwrap();
    let shape = params.get("shape");
    let embed = params.get("embed").map_or(false, |v| v == "true");
    (shape, embed)
}

/// Read the `window.__sdf_shape` global set by sdf-shape-loader.js.
/// Returns the slug string if present.
#[cfg(target_arch = "wasm32")]
fn read_js_shape_selection() -> Option<String> {
    let window = web_sys::window()?;
    let val = js_sys::Reflect::get(&window, &JsValue::from_str("__sdf_shape")).ok()?;
    val.as_string()
}

/// Check if `window.__sdf_shape` exists (set by sdf-shape-loader.js).
/// If it does, we are embedded in rustdoc and should hide UI chrome.
#[cfg(target_arch = "wasm32")]
fn is_doc_embedded() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    js_sys::Reflect::has(&window, &JsValue::from_str("__sdf_shape")).unwrap_or(false)
}

impl App {
    fn new() -> (Self, iced::Task<Message>) {
        #[allow(unused_mut)]
        let mut selected = 0usize;
        #[allow(unused_mut)]
        let mut embed = false;

        #[cfg(target_arch = "wasm32")]
        {
            let (shape_param, embed_param) = parse_url_params();
            embed = embed_param || is_doc_embedded();
            if let Some(slug) = shape_param.or_else(read_js_shape_selection) {
                let entries = shapes::all_shapes();
                if let Some(idx) = entries.iter().position(|e| e.slug == slug) {
                    selected = idx;
                }
            }
        }

        (
            Self {
                selected,
                embed,
                start_time: Instant::now(),
            },
            iced::Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        window::frames().map(|_| Message::Tick)
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Select(idx) => self.selected = idx,
            Message::Tick => {
                // Poll JS global for shape selection (set by sdf-shape-loader.js)
                #[cfg(target_arch = "wasm32")]
                if let Some(slug) = read_js_shape_selection() {
                    let entries = shapes::all_shapes();
                    if let Some(idx) = entries.iter().position(|e| e.slug == slug) {
                        if idx != self.selected {
                            self.selected = idx;
                        }
                    }
                }
            }
        }
        iced::Task::none()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<'_, Message> {
        let entries = shapes::all_shapes();
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let entry = &entries[self.selected];

        // Embed mode: only the SDF canvas, no sidebar or text
        if self.embed {
            let sdf_view = widget::sdf_canvas(entry, elapsed);
            return container(sdf_view)
                .width(Fill)
                .height(Fill)
                .into();
        }

        // Sidebar with shape list
        let sidebar = {
            let mut items = column![].spacing(2).padding(8);

            for (i, entry) in entries.iter().enumerate() {
                let is_selected = i == self.selected;
                let label = text(entry.name).size(14);

                let btn = button(label)
                    .on_press(Message::Select(i))
                    .width(Fill)
                    .style(if is_selected {
                        button::primary
                    } else {
                        button::secondary
                    });

                items = items.push(btn);
            }

            container(scrollable(items).height(Fill))
                .width(200)
                .height(Fill)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(
                        0.12, 0.12, 0.15,
                    ))),
                    ..Default::default()
                })
        };

        // Main canvas area
        let canvas = {
            let title = text(entry.name).size(20);
            let description = text(entry.description).size(13);

            let sdf_view = widget::sdf_canvas(entry, elapsed);

            column![title, description, sdf_view]
                .spacing(8)
                .padding(16)
                .width(Fill)
                .height(Fill)
        };

        row![sidebar, canvas].into()
    }
}
