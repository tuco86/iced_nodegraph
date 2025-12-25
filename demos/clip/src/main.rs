use iced::{
    Border, Color, Element, Length, Padding, Task, Theme,
    widget::{column, container, text},
};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Clip Demo")
        .theme(App::theme)
        .run()
}

#[derive(Default)]
struct App;

#[derive(Debug, Clone)]
enum Message {}

impl App {
    fn new() -> (Self, Task<Message>) {
        (Self, Task::none())
    }

    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<Message> {
        // Header without rounded corners (rectangular)
        let header = container(text("Header - No Rounded Corners").size(14))
            .padding(Padding::new(8.0))
            .width(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(Color::from_rgb(0.3, 0.5, 0.3).into()),
                ..Default::default()
            });

        // Body content
        let body = container(text("Body content goes here...").size(12))
            .padding(Padding::new(12.0))
            .width(Length::Fill)
            .height(Length::Fill);

        // Inner content: header + body
        let inner_content = column![header, body].width(Length::Fill);

        // Outer container with rounded corners and clip
        let outer_container = container(inner_content)
            .width(250.0)
            .height(150.0)
            .clip(true) // Enable clipping
            .style(|_theme: &Theme| container::Style {
                background: Some(Color::from_rgb(0.15, 0.15, 0.18).into()),
                border: Border {
                    color: Color::from_rgb(0.3, 0.3, 0.35),
                    width: 2.0,
                    radius: 12.0.into(), // Rounded corners
                },
                ..Default::default()
            });

        // Center the demo container
        container(outer_container)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(Color::from_rgb(0.1, 0.1, 0.12).into()),
                ..Default::default()
            })
            .into()
    }
}
