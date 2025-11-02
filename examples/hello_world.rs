use iced::{
    event, keyboard,
    widget::{self, column, container, mouse_area, row, stack, text, text_input}, Event, Length, Point, Subscription, Theme
};
use iced_nodegraph::{PinSide, node_graph, node_pin};

pub fn main() -> iced::Result {
    iced::application(Application::new, Application::update, Application::view)
        .subscription(Application::subscription)
        .title("Node Graph Example")
        .theme(Application::theme)
        .run()
}

#[derive(Debug, Clone)]
enum ApplicationMessage {
    Noop,
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
    ChangeTheme(Theme),
    NavigateToSubmenu(String),
    NavigateBack,
}

#[derive(Debug, Clone, PartialEq)]
enum PaletteView {
    Main,
    Submenu(String),
}

struct Application {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, String)>, // position and name
    command_palette_open: bool,
    command_input: String,
    current_theme: Theme,
    palette_view: PaletteView,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            edges: vec![
                ((0, 0), (1, 0)),  // Email Trigger -> Email Parser
                ((1, 0), (2, 0)),  // Email Parser subject -> Filter
                ((1, 1), (3, 0)),  // Email Parser datetime -> Calendar
                ((2, 0), (3, 1)),  // Filter -> Calendar title
            ],
            nodes: vec![
                (Point::new(100.0, 150.0), "email_trigger".to_string()),
                (Point::new(350.0, 150.0), "email_parser".to_string()),
                (Point::new(350.0, 350.0), "filter".to_string()),
                (Point::new(650.0, 250.0), "calendar".to_string()),
            ],
            command_palette_open: false,
            command_input: String::new(),
            current_theme: Theme::CatppuccinFrappe,
            palette_view: PaletteView::Main,
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
                    self.palette_view = PaletteView::Main;
                } else {
                    self.palette_view = PaletteView::Main;
                }
            }
            ApplicationMessage::CommandPaletteInput(input) => {
                self.command_input = input;
            }
            ApplicationMessage::SpawnNode { x, y, name } => {
                // Use node type directly
                self.nodes.push((Point::new(x, y), name));
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
            }
            ApplicationMessage::ChangeTheme(theme) => {
                self.current_theme = theme;
                self.command_palette_open = false;
                self.command_input.clear();
                self.palette_view = PaletteView::Main;
            }
            ApplicationMessage::NavigateToSubmenu(submenu) => {
                self.palette_view = PaletteView::Submenu(submenu);
                self.command_input.clear();
            }
            ApplicationMessage::NavigateBack => {
                self.palette_view = PaletteView::Main;
                self.command_input.clear();
            }
        }
    }
    
    fn theme(&self) -> Theme {
        self.current_theme.clone()
    }

    fn view(&self) -> iced::Element<'_, ApplicationMessage> {
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
            ng.push_node(*position, node(name.as_str(), &self.current_theme));
        }
        
        // Add stored edges
        for ((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            ng.push_edge(*from_node, *from_pin, *to_node, *to_pin);
        }
        
        let graph_view = ng.into();
        
        if self.command_palette_open {
            stack!(
                graph_view,
                command_palette(&self.command_input, &self.palette_view)
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

// Email Trigger Node - Only outputs
fn email_trigger_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();
    
    let title_bar = container(widget::text("📧 Email Trigger").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| {
            container::Style {
                background: None,
                text_color: Some(palette.background.base.text),
                ..container::Style::default()
            }
        });

    let pin_list = column![
        node_pin(PinSide::Right, container(text!("on email").size(11)).padding([0, 8])),
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

// Email Parser Node - Input + multiple outputs
fn email_parser_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();
    
    let title_bar = container(widget::text("📨 Email Parser").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| {
            container::Style {
                background: None,
                text_color: Some(palette.background.base.text),
                ..container::Style::default()
            }
        });

    let pin_list = column![
        node_pin(PinSide::Left, container(text!("email").size(11)).padding([0, 8])),
        node_pin(PinSide::Right, container(text!("subject").size(11)).padding([0, 8])),
        node_pin(PinSide::Right, container(text!("datetime").size(11)).padding([0, 8])),
        node_pin(PinSide::Right, container(text!("body").size(11)).padding([0, 8])),
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

// Filter Node - Input + output
fn filter_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();
    
    let title_bar = container(widget::text("🔍 Filter").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| {
            container::Style {
                background: None,
                text_color: Some(palette.background.base.text),
                ..container::Style::default()
            }
        });

    let pin_list = column![
        node_pin(PinSide::Left, container(text!("input").size(11)).padding([0, 8])),
        node_pin(PinSide::Right, container(text!("matches").size(11)).padding([0, 8])),
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(140.0).into()
}

// Calendar Node - Only inputs
fn calendar_node<'a, Message>(theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let palette = theme.extended_palette();
    
    let title_bar = container(widget::text("📅 Create Event").size(13).width(Length::Fill))
        .width(Length::Fill)
        .padding([2, 8])
        .style(move |_theme: &iced::Theme| {
            container::Style {
                background: None,
                text_color: Some(palette.background.base.text),
                ..container::Style::default()
            }
        });

    let pin_list = column![
        node_pin(PinSide::Left, container(text!("datetime").size(11)).padding([0, 8])),
        node_pin(PinSide::Left, container(text!("title").size(11)).padding([0, 8])),
        node_pin(PinSide::Left, container(text!("description").size(11)).padding([0, 8])),
    ]
    .spacing(2);

    let pin_section = container(pin_list).padding([6, 0]);
    column![title_bar, pin_section].width(160.0).into()
}

fn node<'a, Message>(node_type: &str, theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    match node_type {
        "email_trigger" => email_trigger_node(theme),
        "email_parser" => email_parser_node(theme),
        "filter" => filter_node(theme),
        "calendar" => calendar_node(theme),
        _ => email_trigger_node(theme), // fallback
    }
}

fn command_palette<'a>(input: &str, view: &PaletteView) -> iced::Element<'a, ApplicationMessage>
{
    use iced::widget::{button, scrollable};
    
    let mut commands: Vec<(&str, ApplicationMessage)> = Vec::new();
    let title_text: &str;
    
    match view {
        PaletteView::Main => {
            title_text = "Command Palette";
            commands.push((
                "Add Nodes...",
                ApplicationMessage::NavigateToSubmenu("nodes".to_string())
            ));
            commands.push((
                "Choose Theme...",
                ApplicationMessage::NavigateToSubmenu("themes".to_string())
            ));
        }
        PaletteView::Submenu(submenu) if submenu == "nodes" => {
            title_text = "Add Node";
            commands.push((
                "📧 Email Trigger",
                ApplicationMessage::SpawnNode { 
                    x: 400.0, 
                    y: 300.0, 
                    name: "email_trigger".to_string() 
                }
            ));
            commands.push((
                "📨 Email Parser",
                ApplicationMessage::SpawnNode { 
                    x: 400.0, 
                    y: 300.0, 
                    name: "email_parser".to_string() 
                }
            ));
            commands.push((
                "🔍 Filter",
                ApplicationMessage::SpawnNode { 
                    x: 400.0, 
                    y: 300.0, 
                    name: "filter".to_string() 
                }
            ));
            commands.push((
                "📅 Create Calendar Event",
                ApplicationMessage::SpawnNode { 
                    x: 400.0, 
                    y: 300.0, 
                    name: "calendar".to_string() 
                }
            ));
        }
        PaletteView::Submenu(submenu) if submenu == "themes" => {
            title_text = "Choose Theme";
            // Add all available themes
            for theme in Theme::ALL {
                let theme_label = format!("{}", theme);
                commands.push((
                    Box::leak(theme_label.into_boxed_str()),
                    ApplicationMessage::ChangeTheme(theme.clone()),
                ));
            }
        }
        _ => {
            title_text = "Command Palette";
        }
    }
    
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
    
    // Build header with back button if in submenu
    let header = if matches!(view, PaletteView::Submenu(_)) {
        row![
            button(text("← Back").size(14))
                .on_press(ApplicationMessage::NavigateBack)
                .padding(4),
            text(title_text).size(18).width(Length::Fill),
            button(text("✕").size(16))
                .on_press(ApplicationMessage::ToggleCommandPalette)
                .padding(4)
        ]
        .align_y(iced::Alignment::Center)
    } else {
        row![
            text(title_text).size(18).width(Length::Fill),
            button(text("✕").size(16))
                .on_press(ApplicationMessage::ToggleCommandPalette)
                .padding(4)
        ]
        .align_y(iced::Alignment::Center)
    };
    
    let palette_content = container(
        column![
            header,
            text_input("Type to search...", input)
                .on_input(ApplicationMessage::CommandPaletteInput)
                .padding(8)
                .width(Length::Fill),
            scrollable(command_list)
                .height(Length::Fixed(300.0))
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
