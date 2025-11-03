use wasm_bindgen::prelude::*;

// Import the `console.log` function from the browser's console
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// Define a macro to make console logging easier
macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

use iced::{
    Color, Length, Point, Theme,
    widget::{column, container, text},
    Application, Command, Element, Settings, Size, Subscription,
};

use iced_nodegraph::{PinDirection, PinSide, node_graph, node_pin};

// WASM-specific initialization
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console_log!("NodeGraph WASM demo initialized");
}

// Simplified demo application for WASM
#[derive(Debug, Clone)]
pub enum Message {
    NodeMoved(usize, Point),
    EdgeConnected(usize, usize, usize, usize),
    EdgeDisconnected(usize, usize, usize, usize),
}

pub struct WasmDemo {
    edges: Vec<((usize, usize), (usize, usize))>,
    nodes: Vec<(Point, String)>,
}

impl Application for WasmDemo {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        console_log!("Creating WASM demo application");
        
        let demo = Self {
            edges: vec![
                ((0, 0), (1, 0)), // node 0 pin 0 -> node 1 pin 0
                ((1, 1), (2, 0)), // node 1 pin 1 -> node 2 pin 0
            ],
            nodes: vec![
                (Point::new(100.0, 100.0), "Input".to_string()),
                (Point::new(300.0, 100.0), "Process".to_string()),
                (Point::new(500.0, 100.0), "Output".to_string()),
            ],
        };
        
        (demo, Command::none())
    }

    fn title(&self) -> String {
        "NodeGraph WASM Demo".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::NodeMoved(node_id, new_position) => {
                console_log!("Node {} moved to ({}, {})", node_id, new_position.x, new_position.y);
                if let Some((position, _)) = self.nodes.get_mut(node_id) {
                    *position = new_position;
                }
            }
            Message::EdgeConnected(from_node, from_pin, to_node, to_pin) => {
                console_log!("Edge connected: {} pin {} -> {} pin {}", from_node, from_pin, to_node, to_pin);
                self.edges.push(((from_node, from_pin), (to_node, to_pin)));
            }
            Message::EdgeDisconnected(from_node, from_pin, to_node, to_pin) => {
                console_log!("Edge disconnected: {} pin {} -> {} pin {}", from_node, from_pin, to_node, to_pin);
                self.edges.retain(|&edge| edge != ((from_node, from_pin), (to_node, to_pin)));
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let mut node_graph = node_graph()
            .width(Length::Fill)
            .height(Length::Fill)
            .on_move(Message::NodeMoved)
            .on_connect(Message::EdgeConnected)
            .on_disconnect(Message::EdgeDisconnected);

        // Add edges
        for &((from_node, from_pin), (to_node, to_pin)) in &self.edges {
            node_graph = node_graph.push_edge(from_node, from_pin, to_node, to_pin);
        }

        // Add nodes
        for (index, (position, name)) in self.nodes.iter().enumerate() {
            let node_content = container(
                column![
                    text(name).size(16),
                    node_pin("Input", PinDirection::Input, PinSide::Left)
                        .color(Color::from_rgb(0.3, 0.7, 0.3)),
                    node_pin("Output", PinDirection::Output, PinSide::Right)
                        .color(Color::from_rgb(0.7, 0.3, 0.3)),
                ]
                .spacing(5)
            )
            .padding(10)
            .style(|_: &Theme| container::Style {
                background: Some(Color::from_rgb(0.2, 0.2, 0.3).into()),
                border: iced::Border {
                    color: Color::from_rgb(0.4, 0.4, 0.5),
                    width: 1.0,
                    radius: 5.0.into(),
                },
                ..Default::default()
            });

            node_graph = node_graph.push_node(*position, node_content);
        }

        container(node_graph)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_: &Theme| container::Style {
                background: Some(Color::from_rgb(0.1, 0.1, 0.15).into()),
                ..Default::default()
            })
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }
}

// WASM entry point
#[wasm_bindgen]
pub async fn run_demo() -> Result<(), JsValue> {
    console_log!("Starting NodeGraph WASM demo...");
    
    let settings = Settings {
        window: iced::window::Settings {
            size: Size::new(1000.0, 600.0),
            ..Default::default()
        },
        ..Default::default()
    };
    
    WasmDemo::run(settings)
        .map_err(|e| JsValue::from_str(&format!("Failed to run demo: {}", e)))
}