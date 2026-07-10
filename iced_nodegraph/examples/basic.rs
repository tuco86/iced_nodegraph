//! A small live logic playground built on iced_nodegraph.
//!
//! Every pin carries a `Port` payload (the graph's `UI` type parameter), which
//! drives three things: the node colors its pins by that payload, each edge
//! derives its gradient from the ports it connects (read via `PinInfo::info()`
//! in the edge `style` closure), and `can_connect` only joins an output to an
//! input of the SAME port type. So a `Number` wire (orange) refuses to enter the
//! `AND` gate's `Bool` inputs (green) - you must run it through the `>0`
//! comparator first.
//!
//! The graph is evaluated live every frame: drag the slider or toggle the
//! switch and the lamp reacts. Unplug a pin (click it) and rewire to build a
//! different condition.
//!
//! Run with:
//!
//!     cargo run -p iced_nodegraph --example basic

use iced::widget::{checkbox, column, container, slider, text};
use iced::{Color, Element, Length, Padding, Point, Theme, Vector};
use iced_nodegraph::prelude::*;

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("iced_nodegraph - basic")
        .theme(App::theme)
        .run()
}

/// The per-pin payload carried through the graph as its `UI` type. It drives
/// pin color and connection compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Port {
    Number,
    Bool,
}

impl Port {
    fn color(self) -> Color {
        match self {
            Port::Number => Color::from_rgb(0.90, 0.55, 0.20), // orange
            Port::Bool => Color::from_rgb(0.30, 0.75, 0.45),   // green
        }
    }
}

/// A value flowing along a wire during evaluation.
#[derive(Clone, Copy)]
enum Value {
    Num(f32),
    Bool(bool),
}

impl Value {
    fn as_num(self) -> f32 {
        match self {
            Value::Num(n) => n,
            Value::Bool(b) => b as i32 as f32,
        }
    }
    fn as_bool(self) -> bool {
        matches!(self, Value::Bool(true))
    }
}

/// A connection endpoint with the default `usize` node and pin ids.
type Pin = PinRef<usize, usize>;

// Node ids.
const VALUE: usize = 0; // slider  -> Number out (pin 0)
const SWITCH: usize = 1; // toggle  -> Bool out   (pin 0)
const GT0: usize = 2; // Number in (0) -> Bool out (1)
const AND: usize = 3; // Bool in (0), Bool in (1) -> Bool out (2)
const LAMP: usize = 4; // Bool in (0)

struct App {
    /// Node positions in world space, indexed by node id.
    positions: Vec<Point>,
    /// Active connections between pins.
    edges: Vec<(Pin, Pin)>,
    /// Live input state of the two source nodes.
    value: f32,
    switch: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            positions: vec![
                Point::new(60.0, 80.0),
                Point::new(60.0, 250.0),
                Point::new(300.0, 90.0),
                Point::new(520.0, 170.0),
                Point::new(740.0, 195.0),
            ],
            // Pre-wired into "value > 0 AND switch -> lamp" so it works on launch.
            edges: vec![
                (PinRef::new(VALUE, 0), PinRef::new(GT0, 0)),
                (PinRef::new(GT0, 1), PinRef::new(AND, 0)),
                (PinRef::new(SWITCH, 0), PinRef::new(AND, 1)),
                (PinRef::new(AND, 2), PinRef::new(LAMP, 0)),
            ],
            value: 0.5,
            switch: true,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Moved { delta: Vector, ids: Vec<usize> },
    Connected { from: Pin, to: Pin },
    Disconnected { from: Pin, to: Pin },
    Value(f32),
    Switch(bool),
}

/// Edge stroke that follows its endpoints' port colors, laid out output -> input.
/// The library default is a single concrete color; pin-color inheritance is a
/// userland pattern: both this and `pin_style` read the same `Port::color()`.
fn edge_stroke(start: Port, end: Port) -> ColorQuad {
    ColorQuad::arc(start.color(), end.color())
}

/// Colors a pin by its `Port` payload (read via `pin.info()`). The node owns the
/// pin styling; the pin itself carries no style.
fn pin_style(
    theme: &Theme,
    pin: &PinInfo<'_, usize, Port>,
    _other: Option<&PinInfo<'_, usize, Port>>,
    status: PinStatus,
) -> PinStyle {
    PinStyle {
        color: pin.info().color().into(),
        ..default_pin_style(theme, status)
    }
}

/// Wraps node content in a fixed-width body and attaches the shared pin styling.
fn gate<'a>(
    id: usize,
    pos: Point,
    body: impl Into<Element<'a, Message>>,
) -> Node<'a, usize, usize, Port, Message, Theme, iced::Renderer> {
    node(id, pos, container(body).width(150.0)).pin_style(pin_style)
}

/// A titled node interior: a colored header bar over the body. `node_header`
/// rounds the bar's top corners to `5.0` so it matches the node's rendered
/// silhouette (the widget draws the fill with the same corner radius).
fn framed<'a>(
    title: &'a str,
    header_bg: Color,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let pad = Padding {
        top: 4.0,
        bottom: 4.0,
        left: 8.0,
        right: 8.0,
    };
    column![
        node_header(container(text(title).size(13)).padding(pad), header_bg, 5.0),
        container(body).width(Length::Fill).padding(pad),
    ]
    .width(Length::Fill)
    .into()
}

impl App {
    fn theme(&self) -> Theme {
        Theme::SolarizedLight
    }

    /// The value produced at a node's (single) output, following edges. The
    /// depth budget guards against cycles a user might wire by hand.
    fn output(&self, node: usize, depth: u8) -> Option<Value> {
        if depth == 0 {
            return None;
        }
        Some(match node {
            VALUE => Value::Num(self.value),
            SWITCH => Value::Bool(self.switch),
            GT0 => Value::Bool(self.input(GT0, 0, depth)?.as_num() > 0.0),
            AND => Value::Bool(
                self.input(AND, 0, depth)?.as_bool() && self.input(AND, 1, depth)?.as_bool(),
            ),
            _ => return None,
        })
    }

    /// The value arriving at an input pin: whatever its incoming edge carries.
    fn input(&self, node: usize, pin: usize, depth: u8) -> Option<Value> {
        let (from, _) = self
            .edges
            .iter()
            .find(|(_, to)| to.node_id == node && to.pin_id == pin)?;
        self.output(from.node_id, depth - 1)
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Moved { delta, ids } => {
                for id in ids {
                    self.positions[id] += delta;
                }
            }
            Message::Connected { from, to } => self.edges.push((from, to)),
            Message::Disconnected { from, to } => self.edges.retain(|&e| e != (from, to)),
            Message::Value(v) => self.value = v,
            Message::Switch(b) => self.switch = b,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let p = &self.positions;
        // Header colors keyed on node role, derived from the active theme palette.
        let pal = theme.extended_palette();
        let input_bg = pal.primary.base.color;
        let process_bg = pal.success.base.color;
        let output_bg = pal.secondary.base.color;

        // The graph is parameterized over `Port` as its pin payload (`UI`), so it
        // cannot use the `node_graph()` helper (which fixes `UI = ()`).
        let mut ng: NodeGraph<usize, usize, Port, Message, Theme, iced::Renderer> =
            NodeGraph::default()
                .on_move(|delta, ids| Message::Moved { delta, ids })
                .on_connect(|from, to| Message::Connected { from, to })
                .on_disconnect(|from, to| Message::Disconnected { from, to })
                // Authoritative: opposite directions and matching port type.
                .can_connect(|from, to| {
                    from.direction() != to.direction() && from.info() == to.info()
                })
                // The dragged edge (one loose end) takes the held pin's color.
                .dragging_edge_style(|theme, pin| EdgeStyle {
                    stroke_color: edge_stroke(*pin.info(), *pin.info()),
                    ..default_edge_style(theme, EdgeStatus::Idle)
                });

        ng.push_node(gate(
            VALUE,
            p[VALUE],
            framed(
                "Value",
                input_bg,
                column![
                    slider(-1.0..=1.0, self.value, Message::Value).step(0.01_f32),
                    text(format!("{:.2}", self.value)).size(11),
                    pin!(Right, 0usize, text("n"), Output, Port::Number),
                ]
                .spacing(4),
            ),
        ));

        ng.push_node(gate(
            SWITCH,
            p[SWITCH],
            framed(
                "Switch",
                input_bg,
                column![
                    checkbox(self.switch).on_toggle(Message::Switch),
                    pin!(Right, 0usize, text("b"), Output, Port::Bool),
                ]
                .spacing(4),
            ),
        ));

        ng.push_node(gate(
            GT0,
            p[GT0],
            framed(
                ">0",
                process_bg,
                column![
                    pin!(Left, 0usize, text("x"), Input, Port::Number),
                    pin!(Right, 1usize, text("out"), Output, Port::Bool),
                ]
                .spacing(4),
            ),
        ));

        ng.push_node(gate(
            AND,
            p[AND],
            framed(
                "AND",
                process_bg,
                column![
                    pin!(Left, 0usize, text("a"), Input, Port::Bool),
                    pin!(Left, 1usize, text("b"), Input, Port::Bool),
                    pin!(Right, 2usize, text("out"), Output, Port::Bool),
                ]
                .spacing(4),
            ),
        ));

        let lit = self.input(LAMP, 0, 8).map(Value::as_bool).unwrap_or(false);
        let lamp = if lit {
            Color::from_rgb(0.95, 0.85, 0.20)
        } else {
            Color::from_rgb(0.40, 0.40, 0.40)
        };
        ng.push_node(gate(
            LAMP,
            p[LAMP],
            framed(
                "Lamp",
                output_bg,
                column![
                    pin!(Left, 0usize, text("in"), Input, Port::Bool),
                    text("\u{25CF}").size(22).color(lamp),
                ]
                .spacing(4),
            ),
        ));

        for &(from, to) in &self.edges {
            // Each edge derives its gradient from the two connected pins' ports.
            ng.push_edge(
                edge!(from, to).style(|theme, status, start, end| EdgeStyle {
                    stroke_color: edge_stroke(*start.info(), *end.info()),
                    ..default_edge_style(theme, status)
                }),
            );
        }

        ng.into()
    }
}
