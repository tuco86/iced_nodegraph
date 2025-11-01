use std::time::Instant;

use iced::{
    widget::{self, column, container, mouse_area, row, stack, text, text_input}, window, Length, Point, Subscription
};
use iced_nodegraph::{PinSide, node_graph, node_pin};

pub fn main() -> iced::Result {
    iced::application(Application::new, Application::update, Application::view)
        // .subscription(Application::subscription)
        .title("Node Graph Example")
        .theme(|_| iced::Theme::CatppuccinFrappe)
        .run()
}

#[derive(Debug, Clone)]
enum ApplicationMessage {
    Noop,
    Tick(Instant),
    EdgeConnected {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
}

#[derive(Default)]
struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
}

impl Application {
    fn new() -> Self {
        Self {
            edges: Vec::new(),
        }
    }

    fn update(&mut self, message: ApplicationMessage) {
        match message {
            ApplicationMessage::Noop => (),
            ApplicationMessage::Tick(_) => (),
            ApplicationMessage::EdgeConnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                println!(
                    "Edge connected: node {} pin {} -> node {} pin {}",
                    from_node, from_pin, to_node, to_pin
                );
                self.edges.push(((from_node, from_pin), (to_node, to_pin)));
            }
        }
    }

    fn view(&self) -> iced::Element<ApplicationMessage> {
        let mut ng = node_graph()
            .on_connect(|from_node, from_pin, to_node, to_pin| {
                ApplicationMessage::EdgeConnected {
                    from_node,
                    from_pin,
                    to_node,
                    to_pin,
                }
            });
        
        ng.push_node(Point::new(200.0, 150.0), node("Node 1"));
        ng.push_node(Point::new(525.0, 175.0), node("Node 2"));
        ng.push_node(Point::new(200.0, 350.0), node("Node 3"));
        
        // Add stored edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }
        
        stack!(ng, command_palette("Commands")).width(Length::Fill).height(Length::Fill).into()
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        window::frames().map(ApplicationMessage::Tick)
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
    .padding(4.0)
    .into()
}

fn command_palette<'a>(name: impl text::IntoFragment<'a>) -> iced::Element<'a, ApplicationMessage>
{
    row!(
        column!().width(Length::FillPortion(1)),
        column!(
            text_input("", "")
                .on_input(|_| ApplicationMessage::Noop)
                .padding(8)
                .width(Length::Fill),
            text("Command 1"),
            text("Command 2"),
            text("Command 3"),
        )
        .width(Length::FillPortion(2)),
        column!().width(Length::FillPortion(1)),
    )
    .padding(8)
    .into()
}

mod iced_command_palette {
    struct CommandPalette;
    
}
