//! # Interaction Demo
//!
//! Demonstrates connection validation with typed pins, directional flow,
//! and user feedback messages.

use demo_common::{ScreenshotHelper, ScreenshotMessage};
use iced::{
    alignment::Horizontal,
    Color, Element, Length, Point, Subscription, Theme,
    widget::{button, column, container, row, scrollable, text, Space},
};
use iced_nodegraph::{
    NodeContentStyle, PinRef, node_graph, pin, simple_node,
};
use std::collections::HashMap;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

// -- Marker types for pin data_type matching --

struct Integer;
struct Float;
struct StringType;
struct AnyType;

// -- Pin metadata for validation --

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinType {
    Integer,
    Float,
    String,
    #[allow(dead_code)]
    Boolean,
    Any,
}

impl PinType {
    fn name(self) -> &'static str {
        match self {
            PinType::Integer => "Integer",
            PinType::Float => "Float",
            PinType::String => "String",
            PinType::Boolean => "Boolean",
            PinType::Any => "Any",
        }
    }

    fn color(self) -> Color {
        match self {
            PinType::Integer => Color::from_rgb(0.2, 0.6, 1.0),
            PinType::Float => Color::from_rgb(0.2, 0.9, 0.4),
            PinType::String => Color::from_rgb(1.0, 0.8, 0.2),
            PinType::Boolean => Color::from_rgb(0.9, 0.3, 0.3),
            PinType::Any => Color::from_rgb(0.7, 0.7, 0.7),
        }
    }

    fn is_compatible(self, other: PinType) -> bool {
        if self == PinType::Any || other == PinType::Any {
            return true;
        }
        if self == other {
            return true;
        }
        // Integer -> Float implicit conversion
        if self == PinType::Integer && other == PinType::Float {
            return true;
        }
        if self == PinType::Float && other == PinType::Integer {
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinDir {
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone)]
struct PinInfo {
    pin_type: PinType,
    direction: PinDir,
    single_connection: bool,
    label: &'static str,
}

// -- Application --

#[derive(Debug, Clone)]
enum Message {
    EdgeConnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    EdgeDisconnected {
        from: PinRef<usize, usize>,
        to: PinRef<usize, usize>,
    },
    NodeMoved {
        node_id: usize,
        position: Point,
    },
    ClearAll,
    Reset,
    ToggleRules,
    Screenshot(ScreenshotMessage),
}

impl From<ScreenshotMessage> for Message {
    fn from(msg: ScreenshotMessage) -> Self {
        Message::Screenshot(msg)
    }
}

struct App {
    edges: Vec<(PinRef<usize, usize>, PinRef<usize, usize>)>,
    node_positions: HashMap<usize, Point>,
    pin_registry: HashMap<(usize, usize), PinInfo>,
    feedback: Vec<String>,
    show_rules: bool,
    screenshot: ScreenshotHelper,
}

impl App {
    fn new() -> (Self, iced::Task<Message>) {
        let mut app = Self {
            edges: Vec::new(),
            node_positions: Self::default_positions(),
            pin_registry: HashMap::new(),
            feedback: vec!["Drag between pins to connect nodes.".into()],
            show_rules: false,
            screenshot: ScreenshotHelper::from_args(),
        };
        app.register_pins();
        (app, iced::Task::none())
    }

    fn default_positions() -> HashMap<usize, Point> {
        let mut m = HashMap::new();
        m.insert(0, Point::new(50.0, 120.0));
        m.insert(1, Point::new(350.0, 50.0));
        m.insert(2, Point::new(350.0, 250.0));
        m.insert(3, Point::new(650.0, 120.0));
        m.insert(4, Point::new(350.0, 440.0));
        m
    }

    fn register_pins(&mut self) {
        self.pin_registry.clear();

        // Node 0: Number Generator
        self.register(0, 0, PinType::Integer, PinDir::Output, false, "Int Out");
        self.register(0, 1, PinType::Float, PinDir::Output, false, "Float Out");

        // Node 1: Math Operations
        self.register(1, 0, PinType::Float, PinDir::Input, true, "A");
        self.register(1, 1, PinType::Float, PinDir::Input, true, "B");
        self.register(1, 2, PinType::Float, PinDir::Output, false, "Result");

        // Node 2: Type Converter
        self.register(2, 0, PinType::Any, PinDir::Input, false, "In");
        self.register(2, 1, PinType::Integer, PinDir::Output, false, "Int");
        self.register(2, 2, PinType::Float, PinDir::Output, false, "Float");
        self.register(2, 3, PinType::String, PinDir::Output, false, "String");

        // Node 3: Display
        self.register(3, 0, PinType::Any, PinDir::Input, false, "Value");
        self.register(3, 1, PinType::String, PinDir::Input, false, "Label");

        // Node 4: Bidirectional Hub
        self.register(4, 0, PinType::Float, PinDir::Bidirectional, false, "Float");
        self.register(4, 1, PinType::Integer, PinDir::Bidirectional, false, "Int");
        self.register(4, 2, PinType::Any, PinDir::Bidirectional, false, "Any");
        self.register(4, 3, PinType::String, PinDir::Bidirectional, false, "Str");
    }

    fn register(
        &mut self,
        node: usize,
        pin: usize,
        pin_type: PinType,
        direction: PinDir,
        single_connection: bool,
        label: &'static str,
    ) {
        self.pin_registry.insert(
            (node, pin),
            PinInfo {
                pin_type,
                direction,
                single_connection,
                label,
            },
        );
    }

    fn validate_connection(
        &self,
        from: &PinRef<usize, usize>,
        to: &PinRef<usize, usize>,
    ) -> Result<String, String> {
        let from_info = self
            .pin_registry
            .get(&(from.node_id, from.pin_id))
            .ok_or("Unknown source pin")?;
        let to_info = self
            .pin_registry
            .get(&(to.node_id, to.pin_id))
            .ok_or("Unknown target pin")?;

        // Check direction compatibility (Input<->Input is the only invalid combo)
        let dir_ok = !matches!(
            (from_info.direction, to_info.direction),
            (PinDir::Input, PinDir::Input) | (PinDir::Output, PinDir::Output)
        );
        if !dir_ok {
            return Err(format!(
                "Direction mismatch: cannot connect {:?} to {:?}",
                from_info.direction, to_info.direction
            ));
        }

        // Check type compatibility
        if !from_info.pin_type.is_compatible(to_info.pin_type) {
            return Err(format!(
                "Type mismatch: {} is not compatible with {}",
                from_info.pin_type.name(),
                to_info.pin_type.name()
            ));
        }

        // Check single-connection constraint on target
        if to_info.single_connection {
            let already_connected = self.edges.iter().any(|(_, t)| {
                t.node_id == to.node_id && t.pin_id == to.pin_id
            });
            if already_connected {
                return Err(format!(
                    "Pin '{}' only accepts a single connection",
                    to_info.label
                ));
            }
        }

        // Check single-connection constraint on source
        if from_info.single_connection {
            let already_connected = self.edges.iter().any(|(f, _)| {
                f.node_id == from.node_id && f.pin_id == from.pin_id
            });
            if already_connected {
                return Err(format!(
                    "Pin '{}' only accepts a single connection",
                    from_info.label
                ));
            }
        }

        // Check duplicate
        let duplicate = self.edges.iter().any(|(f, t)| {
            (f.node_id == from.node_id && f.pin_id == from.pin_id
                && t.node_id == to.node_id && t.pin_id == to.pin_id)
                || (f.node_id == to.node_id && f.pin_id == to.pin_id
                    && t.node_id == from.node_id && t.pin_id == from.pin_id)
        });
        if duplicate {
            return Err("Connection already exists".into());
        }

        Ok(format!(
            "Connected: {} -> {} ({} -> {})",
            from_info.label, to_info.label,
            from_info.pin_type.name(), to_info.pin_type.name()
        ))
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Screenshot(msg) => return self.screenshot.update(msg),
            Message::EdgeConnected { from, to } => {
                match self.validate_connection(&from, &to) {
                    Ok(msg) => {
                        self.edges.push((from, to));
                        self.feedback.push(msg);
                    }
                    Err(msg) => {
                        self.feedback.push(format!("Rejected: {}", msg));
                    }
                }
            }
            Message::EdgeDisconnected { from, to } => {
                self.edges.retain(|(f, t)| {
                    !((f.node_id == from.node_id && f.pin_id == from.pin_id
                        && t.node_id == to.node_id && t.pin_id == to.pin_id)
                        || (f.node_id == to.node_id && f.pin_id == to.pin_id
                            && t.node_id == from.node_id && t.pin_id == from.pin_id))
                });
                if let (Some(from_info), Some(to_info)) = (
                    self.pin_registry.get(&(from.node_id, from.pin_id)),
                    self.pin_registry.get(&(to.node_id, to.pin_id)),
                ) {
                    self.feedback.push(format!(
                        "Disconnected: {} -- {}",
                        from_info.label, to_info.label
                    ));
                }
            }
            Message::NodeMoved { node_id, position } => {
                self.node_positions.insert(node_id, position);
            }
            Message::ClearAll => {
                self.edges.clear();
                self.feedback.push("All connections cleared.".into());
            }
            Message::Reset => {
                self.edges.clear();
                self.node_positions = Self::default_positions();
                self.feedback = vec!["Reset to initial state.".into()];
            }
            Message::ToggleRules => {
                self.show_rules = !self.show_rules;
            }
        }

        // Keep feedback log bounded
        if self.feedback.len() > 50 {
            self.feedback.drain(0..self.feedback.len() - 50);
        }
        iced::Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        self.screenshot.subscription().map(Message::Screenshot)
    }

    fn view(&self) -> Element<'_, Message> {
        let theme = Theme::Dark;

        let registry = self.pin_registry.clone();
        let mut ng = node_graph()
            .can_connect(move |from, to| {
                let from_info = registry.get(&(from.node_id, from.pin_id));
                let to_info = registry.get(&(to.node_id, to.pin_id));
                match (from_info, to_info) {
                    (Some(f), Some(t)) => f.pin_type.is_compatible(t.pin_type),
                    _ => false,
                }
            })
            .on_connect(|from, to| Message::EdgeConnected { from, to })
            .on_disconnect(|from, to| Message::EdgeDisconnected { from, to })
            .on_move(|node_id, position| Message::NodeMoved { node_id, position });

        // Node 0: Number Generator
        let pos = self.node_positions.get(&0).copied().unwrap_or(Point::ORIGIN);
        ng.push_node(
            0usize,
            pos,
            self.number_generator_node(&theme),
        );

        // Node 1: Math Operations
        let pos = self.node_positions.get(&1).copied().unwrap_or(Point::ORIGIN);
        ng.push_node(1usize, pos, self.math_operations_node(&theme));

        // Node 2: Type Converter
        let pos = self.node_positions.get(&2).copied().unwrap_or(Point::ORIGIN);
        ng.push_node(2usize, pos, self.type_converter_node(&theme));

        // Node 3: Display
        let pos = self.node_positions.get(&3).copied().unwrap_or(Point::ORIGIN);
        ng.push_node(3usize, pos, self.display_node(&theme));

        // Node 4: Bidirectional Hub
        let pos = self.node_positions.get(&4).copied().unwrap_or(Point::ORIGIN);
        ng.push_node(4usize, pos, self.bidirectional_hub_node(&theme));

        // Add edges
        for (from, to) in &self.edges {
            ng.push_edge(*from, *to);
        }

        // Toolbar
        let toolbar = container(
            row![
                button("Clear All").on_press(Message::ClearAll),
                button("Reset").on_press(Message::Reset),
                button(if self.show_rules { "Hide Rules" } else { "Show Rules" })
                    .on_press(Message::ToggleRules),
                Space::new().width(Length::Fill),
                text(format!("{} connections", self.edges.len())).size(13),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .padding(4),
        )
        .padding(4)
        .width(Length::Fill);

        // Status bar with scrollable feedback log
        let feedback_content: Element<Message> = if self.show_rules {
            self.rules_panel()
        } else {
            let items: Vec<Element<Message>> = self
                .feedback
                .iter()
                .rev()
                .take(10)
                .map(|msg| text(msg).size(12).into())
                .collect();
            scrollable(column(items).spacing(2).padding(4))
                .height(Length::Fixed(100.0))
                .into()
        };

        let status_bar = container(feedback_content)
            .width(Length::Fill)
            .height(Length::Fixed(if self.show_rules { 180.0 } else { 100.0 }));

        column![toolbar, Element::from(ng), status_bar]
            .height(Length::Fill)
            .into()
    }

    fn rules_panel(&self) -> Element<'_, Message> {
        let rules = column![
            text("Connection Rules").size(14),
            text("- Output pins connect to Input pins").size(12),
            text("- Bidirectional pins connect to any direction").size(12),
            text("- Same types connect (Integer, Float, String, Boolean)").size(12),
            text("- Any type is compatible with all types").size(12),
            text("- Integer implicitly converts to Float").size(12),
            text("- Single-connection pins reject additional connections").size(12),
            text("- Duplicate connections are rejected").size(12),
        ]
        .spacing(4)
        .padding(8);
        scrollable(rules).height(Length::Fixed(180.0)).into()
    }

    fn number_generator_node<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let style = NodeContentStyle::custom(theme, Color::from_rgb(0.2, 0.5, 0.9));
        container(simple_node(
            "Number Generator",
            style,
            column![
                right_pin(pin!(Right, 0usize, text("Int Out").size(12), Output, Integer, PinType::Integer.color())),
                right_pin(pin!(Right, 1usize, text("Float Out").size(12), Output, Float, PinType::Float.color())),
            ]
            .spacing(4),
        ))
        .width(160.0)
        .into()
    }

    fn math_operations_node<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let style = NodeContentStyle::custom(theme, Color::from_rgb(0.1, 0.7, 0.3));
        container(simple_node(
            "Math Operations",
            style,
            column![
                pin!(Left, 0usize, text("A (Float)").size(12), Input, Float, PinType::Float.color()),
                pin!(Left, 1usize, text("B (Float)").size(12), Input, Float, PinType::Float.color()),
                right_pin(pin!(Right, 2usize, text("Result").size(12), Output, Float, PinType::Float.color())),
            ]
            .spacing(4),
        ))
        .width(160.0)
        .into()
    }

    fn type_converter_node<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let style = NodeContentStyle::custom(theme, Color::from_rgb(0.6, 0.4, 0.8));
        container(simple_node(
            "Type Converter",
            style,
            column![
                pin!(Left, 0usize, text("In (Any)").size(12), Input, AnyType, PinType::Any.color()),
                right_pin(pin!(Right, 1usize, text("Int").size(12), Output, Integer, PinType::Integer.color())),
                right_pin(pin!(Right, 2usize, text("Float").size(12), Output, Float, PinType::Float.color())),
                right_pin(pin!(Right, 3usize, text("String").size(12), Output, StringType, PinType::String.color())),
            ]
            .spacing(4),
        ))
        .width(160.0)
        .into()
    }

    fn display_node<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let style = NodeContentStyle::custom(theme, Color::from_rgb(0.9, 0.6, 0.1));
        container(simple_node(
            "Display",
            style,
            column![
                pin!(Left, 0usize, text("Value (Any)").size(12), Input, AnyType, PinType::Any.color()),
                pin!(Left, 1usize, text("Label (String)").size(12), Input, StringType, PinType::String.color()),
            ]
            .spacing(4),
        ))
        .width(160.0)
        .into()
    }

    fn bidirectional_hub_node<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let style = NodeContentStyle::custom(theme, Color::from_rgb(0.5, 0.5, 0.5));
        container(simple_node(
            "Bidirectional Hub",
            style,
            column![
                pin!(Top, 0usize, text("Float").size(12), Both, Float, PinType::Float.color()),
                right_pin(pin!(Right, 1usize, text("Int").size(12), Both, Integer, PinType::Integer.color())),
                pin!(Bottom, 2usize, text("Any").size(12), Both, AnyType, PinType::Any.color()),
                pin!(Left, 3usize, text("Str").size(12), Both, StringType, PinType::String.color()),
            ]
            .spacing(4),
        ))
        .width(160.0)
        .into()
    }
}

/// Wraps a Right-side pin in a right-aligned container so the pin dot
/// appears at the right edge of the node.
fn right_pin<'a, Message: Clone + 'a>(
    pin: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(pin)
        .width(Length::Fill)
        .align_x(Horizontal::Right)
        .into()
}

pub fn main() -> iced::Result {
    #[cfg(feature = "wasm")]
    let window_settings = iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            target: Some(String::from("demo-canvas-container")),
        },
        ..Default::default()
    };

    #[cfg(not(feature = "wasm"))]
    let window_settings = iced::window::Settings {
        size: iced::Size::new(1100.0, 750.0),
        ..Default::default()
    };

    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .title("Interaction Demo - Connection Validation")
        .window(window_settings)
        .run()
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn run_demo() {
    let _ = main();
}
