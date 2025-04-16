use iced::{
    Length, Point,
    widget::{self, column, container, mouse_area, row, stack, text},
};
use iced_nodegraph::{PinSide, node_graph, node_pin};

pub fn main() -> iced::Result {
    iced::application(Application::new, Application::update, Application::view)
        .title("Node Graph Example")
        .theme(|_| iced::Theme::CatppuccinFrappe)
        .run()
}

#[derive(Debug, Clone, Copy, Default)]
enum ApplicationMessage {
    #[default]
    Noop,
}

#[derive(Default)]
struct Application {}

impl Application {
    fn new() -> Self {
        Self {}
    }

    fn update(&mut self, message: ApplicationMessage) {
        match message {
            ApplicationMessage::Noop => (),
        }
    }

    fn view(&self) -> iced::Element<ApplicationMessage> {
        let mut ng = node_graph();
        ng.push_node(Point::new(100.0, 50.0), node("Node 1"));
        ng.push_node(Point::new(325.0, 50.0), node("Node 2"));
        stack!(ng).width(Length::Fill).height(Length::Fill).into()
    }
}

fn node<'a, Message>(name: impl text::IntoFragment<'a>) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    // Title bar with collapse button, name, burger button
    let collapse_button = widget::button(text("-"));
    let burger_button = widget::button(text("≡"));
    let title_text = widget::text(name).size(16).width(Length::Fill);
    let title_bar = container(
        row![collapse_button, title_text, burger_button,]
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 4]);

    column!(
        title_bar,
        node_pin(
            PinSide::Left,
            mouse_area(text!("pin a")).interaction(iced::mouse::Interaction::Move)
        ),
        node_pin(PinSide::Right, text!("pin b")),
        node_pin(PinSide::Top, text!("pin c")),
        node_pin(PinSide::Bottom, text!("pin d")),
    )
        .width(200.0)
        .into()
}
