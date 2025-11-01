use std::time::Instant;

use iced::{
    event, keyboard,
    widget::{self, column, container, mouse_area, row, stack, text, text_input}, window, Event, Length, Point, Subscription
};
use iced_nodegraph::{PinSide, node_graph, node_pin};

pub fn main() -> iced::Result {
    iced::application(Application::new, Application::update, Application::view)
        .subscription(Application::subscription)
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
    NodeMoved {
        node_index: usize,
        new_position: Point,
    },
    EdgeDisconnected {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    ToggleCommandPalette,
    CommandPaletteInput(String),
    SpawnNode { x: f32, y: f32, name: String },
}

struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, String)>, // position and name
    command_palette_open: bool,
    command_input: String,
    cursor_position: Option<Point>,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            nodes: vec![
                (Point::new(200.0, 150.0), "Node 1".to_string()),
                (Point::new(525.0, 175.0), "Node 2".to_string()),
                (Point::new(200.0, 350.0), "Node 3".to_string()),
            ],
            command_palette_open: false,
            command_input: String::new(),
            cursor_position: None,
        }
    }
}

impl Application {
    fn new() -> Self {
        Self::default()
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
            ApplicationMessage::NodeMoved {
                node_index,
                new_position,
            } => {
                if let Some((position, _)) = self.nodes.get_mut(node_index) {
                    *position = new_position;
                    println!("Node {} moved to {:?}", node_index, new_position);
                }
            }
            ApplicationMessage::EdgeDisconnected {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                self.edges.retain(|edge| {
                    *edge != ((from_node, from_pin), (to_node, to_pin))
                });
                println!(
                    "Edge disconnected: node {} pin {} -> node {} pin {}",
                    from_node, from_pin, to_node, to_pin
                );
            }
            ApplicationMessage::ToggleCommandPalette => {
                self.command_palette_open = !self.command_palette_open;
                if !self.command_palette_open {
                    self.command_input.clear();
                }
            }
            ApplicationMessage::CommandPaletteInput(input) => {
                self.command_input = input;
            }
            ApplicationMessage::SpawnNode { x, y, name } => {
                let node_num = self.nodes.len() + 1;
                let node_name = if name.is_empty() {
                    format!("Node {}", node_num)
                } else {
                    name
                };
                self.nodes.push((Point::new(x, y), node_name));
                self.command_palette_open = false;
                self.command_input.clear();
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
            })
            .on_disconnect(|from_node, from_pin, to_node, to_pin| {
                ApplicationMessage::EdgeDisconnected {
                    from_node,
                    from_pin,
                    to_node,
                    to_pin,
                }
            })
            .on_move(|node_index, new_position| {
                ApplicationMessage::NodeMoved {
                    node_index,
                    new_position,
                }
            });
        
        // Add all nodes from state
        for (position, name) in &self.nodes {
            ng.push_node(*position, node(name.as_str()));
        }
        
        // Add stored edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }
        
        let graph_view = ng.into();
        
        if self.command_palette_open {
            stack!(
                graph_view,
                command_palette(&self.command_input)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            graph_view
        }
    }

    fn subscription(&self) -> Subscription<ApplicationMessage> {
        event::listen().map(|event| {
            match event {
                Event::Keyboard(keyboard::Event::KeyPressed { 
                    key: keyboard::Key::Character(c),
                    modifiers,
                    ..
                }) if modifiers.command() && c.as_ref() == "k" => {
                    ApplicationMessage::ToggleCommandPalette
                }
                _ => ApplicationMessage::Noop,
            }
        })
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

fn command_palette<'a>(input: &str) -> iced::Element<'a, ApplicationMessage>
{
    use iced::widget::{button, scrollable};
    
    let commands = vec![
        ("Add Node at Center", ApplicationMessage::SpawnNode { 
            x: 400.0, 
            y: 300.0, 
            name: String::new() 
        }),
    ];
    
    let filtered_commands: Vec<_> = commands
        .into_iter()
        .filter(|(label, _)| {
            input.is_empty() || label.to_lowercase().contains(&input.to_lowercase())
        })
        .collect();
    
    let mut command_items = Vec::new();
    for (label, msg) in filtered_commands {
        command_items.push(
            button(text(label).size(14))
                .on_press(msg)
                .width(Length::Fill)
                .padding(8)
                .into()
        );
    }
    
    let command_list = column(command_items).spacing(4);
    
    let palette_content = container(
        column![
            row![
                text("Command Palette").size(18).width(Length::Fill),
                button(text("✕").size(16))
                    .on_press(ApplicationMessage::ToggleCommandPalette)
                    .padding(4)
            ]
            .align_y(iced::Alignment::Center),
            text_input("Type to search...", input)
                .on_input(ApplicationMessage::CommandPaletteInput)
                .padding(8)
                .width(Length::Fill),
            scrollable(command_list)
                .height(Length::Fixed(200.0))
        ]
        .spacing(8)
        .padding(16)
        .width(500.0)
    );
    
    // Background overlay that closes on click
    mouse_area(
        container(palette_content)
            .center(Length::Fill)
            .style(|_theme: &iced::Theme| {
                container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
                    ..container::Style::default()
                }
            })
    )
    .on_press(ApplicationMessage::ToggleCommandPalette)
    .into()
}
