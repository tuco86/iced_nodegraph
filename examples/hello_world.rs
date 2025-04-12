use iced::{
    advanced::mouse, widget::{self, column, container, row, stack, text}, Element, Length, Point
};
use iced_nodegraph::node_graph;

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
        stack!(ng)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn node<'a, Message>(name: impl text::IntoFragment<'a>) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    // Title bar with collapse button, name, burger button
    let collapse_button = widget::button(text("-"));
    // .on_press(NodeMessage::ToggleCollapse);

    let burger_button = widget::button(text("≡"));
    // .on_press(NodeMessage::ShowContextMenu);

    let title_text = widget::MouseArea::new(text(name).size(16).width(Length::Fill))
        // .on_press(NodeMessage::DragStart)
        // .on_release(NodeMessage::DragEnd) // TODO: release is better handled by NodeGraph
        .interaction(iced::mouse::Interaction::Move);

    let title_bar = container(
        row![collapse_button, title_text, burger_button,]
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 4]);

    column!(
        title_bar,
        node_pin("pin a"),
        node_pin("pin b"),
        node_pin("pin c"),
        node_pin("pin d"),
    ).width(200.0).into()
}

fn node_pin<'a, Message>(text: impl widget::text::IntoFragment<'a>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    row![
        widget::mouse_area(widget::text("#")).interaction(mouse::Interaction::Grab),
        widget::text(text).size(12),
    ].into()
}