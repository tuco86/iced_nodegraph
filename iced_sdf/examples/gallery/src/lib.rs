//! SDF Gallery - Interactive showcase of 2D SDF primitives.
//!
//! Browse through SDF shapes from Inigo Quilez's 2D distance functions
//! library, rendered in real-time via iced_sdf.

mod shapes;
mod widget;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Color, Element, Fill, Theme};
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
    start_time: Instant,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
}

impl App {
    fn new() -> (Self, iced::Task<Message>) {
        (
            Self {
                selected: 0,
                start_time: Instant::now(),
            },
            iced::Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Select(idx) => self.selected = idx,
        }
        iced::Task::none()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<'_, Message> {
        let entries = shapes::all_shapes();
        let elapsed = self.start_time.elapsed().as_secs_f32();

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
        let entry = &entries[self.selected];
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
