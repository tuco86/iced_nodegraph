//! The project site's opening scene: two oscillators and a phase knob wired
//! into an XY scope that draws their Lissajous figure.
//!
//! Every node body is an ordinary iced widget - sliders, text and a `canvas` -
//! and the host owns the graph: it keeps the node positions and the edge list,
//! applies what the widget reports through `on_move`, `on_connect` and
//! `on_disconnect`, and evaluates the figure from its own state on every
//! `view`. Pins carry a [`Port`] payload; `can_connect` only joins an output
//! to a free input of the same port, so a `Number` wire never reaches a
//! `Signal` input.
//!
//! The scene frames itself: it fits every node into the window on boot and
//! again whenever a window opens, resizes or changes its device pixel ratio,
//! so it reads on any screen size.
//!
//! Nothing animates: the figure changes only when a message changes the host
//! state. There is no frame subscription, only the window events that reframe
//! the camera.

use std::f32::consts::TAU;

use demo_common::{Demo, NodeContentStyle, Scene, SceneMessage, erase, node_title_bar};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{column, container, row, slider, text};
use iced::{
    Alignment, Color, Element, Event, Length, Padding, Point, Rectangle, Renderer, Subscription,
    Task, Theme, Vector, event, mouse, widget, window,
};
use iced_nodegraph::connection::default_can_connect;
use iced_nodegraph::prelude::*;

/// Boots the scope scene for the gallery.
pub fn scene() -> (Box<dyn Scene>, Task<SceneMessage>) {
    erase::<Scope>()
}

/// What a pin carries: a waveform or a plain number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Port {
    Signal,
    Number,
}

impl Port {
    /// The pin and wire colour of this port, from the theme so the scene
    /// follows the page it is embedded in.
    fn color(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Port::Signal => palette.primary,
            Port::Number => palette.warning,
        }
    }
}

/// A node of the scene. The name is what the message tape prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Unit {
    OscX,
    OscY,
    Phase,
    Scope,
}

/// A pin within its node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Jack {
    Out,
    Phase,
    X,
    Y,
}

/// The scene's id vocabulary: [`Unit`] nodes, [`Jack`] pins, [`Port`] on
/// every pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ScopeIds;

impl Ids for ScopeIds {
    type NodeId = Unit;
    type PinId = Jack;
    type EdgeId = ();
    type AnchorId = usize;
    type Payload = Port;
}

type Pin = PinRef<ScopeIds>;

/// Tuple variants keep the gallery's message tape short:
/// `Connected(PinRef { node_id: OscX, pin_id: Out }, PinRef { node_id: Scope, pin_id: X })`,
/// `FrequencyX(3)`.
#[derive(Debug, Clone)]
enum Message {
    Moved(Vector, Vec<Unit>),
    Connected(Pin, Pin),
    Disconnected(Pin, Pin),
    FrequencyX(u32),
    FrequencyY(u32),
    /// Phase offset of OSC Y in whole degrees.
    Phase(u32),
    /// A window of the runtime opened; the scene reframes.
    WindowOpened,
    /// A window of the runtime changed its logical size (resize or a new
    /// device pixel ratio); the scene reframes.
    WindowResized,
}

struct Scope {
    /// Addresses this instance's graph. Unique per scene: a `focus` request
    /// goes to the first graph carrying its id in any window, so a shared id
    /// would frame one embed and leave every other one stale.
    graph: widget::Id,
    theme: Theme,
    /// World position of each node, indexed by `Unit as usize`.
    positions: [Point; 4],
    /// Connections, output pin first.
    edges: Vec<(Pin, Pin)>,
    frequency_x: u32,
    frequency_y: u32,
    phase_degrees: u32,
}

/// A sine oscillator as the scope sees it: `sin(ratio * t + phase)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Wave {
    ratio: u32,
    /// Radians.
    phase: f32,
}

impl Wave {
    fn at(self, t: f32) -> f32 {
        (self.ratio as f32 * t + self.phase).sin()
    }
}

impl Scope {
    /// The node wired into `jack` of `unit`, if any. `default_can_connect`
    /// keeps an input to one edge, so the first match is the only one.
    fn source(&self, unit: Unit, jack: Jack) -> Option<Unit> {
        let input = PinRef::new(unit, jack);
        self.edges
            .iter()
            .find(|(_, to)| *to == input)
            .map(|(from, _)| from.node_id)
    }

    /// The waveform at `unit`'s signal output; `None` for a node without one.
    fn wave(&self, unit: Unit) -> Option<Wave> {
        match unit {
            Unit::OscX => Some(Wave {
                ratio: self.frequency_x,
                phase: 0.0,
            }),
            Unit::OscY => {
                let phase = match self.source(Unit::OscY, Jack::Phase) {
                    Some(Unit::Phase) => (self.phase_degrees as f32).to_radians(),
                    _ => 0.0,
                };
                Some(Wave {
                    ratio: self.frequency_y,
                    phase,
                })
            }
            Unit::Phase | Unit::Scope => None,
        }
    }

    fn trace(&self) -> Trace {
        Trace {
            x: self
                .source(Unit::Scope, Jack::X)
                .and_then(|unit| self.wave(unit)),
            y: self
                .source(Unit::Scope, Jack::Y)
                .and_then(|unit| self.wave(unit)),
        }
    }

    fn position(&self, unit: Unit) -> Point {
        self.positions[unit as usize]
    }

    /// Fits every node into the window at once, with room around the
    /// outermost pins so they stay clickable; never zooms in past native
    /// size.
    fn frame(&self) -> Task<Message> {
        focus(
            self.graph.clone(),
            FocusTarget::<ScopeIds>::All,
            FocusOptions {
                padding: Padding::new(32.0),
                min_zoom: None,
                max_zoom: Some(1.0),
                animation: None,
            },
        )
    }
}

impl Demo for Scope {
    type Message = Message;

    fn boot() -> (Self, Task<Message>) {
        let scope = Self {
            graph: widget::Id::unique(),
            theme: Theme::Dark,
            positions: [
                // OscX, OscY, Phase, Scope
                Point::new(290.0, 110.0),
                Point::new(290.0, 320.0),
                Point::new(50.0, 330.0),
                Point::new(560.0, 140.0),
            ],
            edges: vec![
                (
                    PinRef::new(Unit::Phase, Jack::Out),
                    PinRef::new(Unit::OscY, Jack::Phase),
                ),
                (
                    PinRef::new(Unit::OscX, Jack::Out),
                    PinRef::new(Unit::Scope, Jack::X),
                ),
                (
                    PinRef::new(Unit::OscY, Jack::Out),
                    PinRef::new(Unit::Scope, Jack::Y),
                ),
            ],
            frequency_x: 3,
            frequency_y: 2,
            phase_degrees: 90,
        };

        let frame = scope.frame();

        (scope, frame)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Moved(delta, units) => {
                for unit in units {
                    self.positions[unit as usize] += delta;
                }
            }
            Message::Connected(from, to) => self.edges.push((from, to)),
            Message::Disconnected(from, to) => self.edges.retain(|&edge| edge != (from, to)),
            Message::FrequencyX(ratio) => self.frequency_x = ratio,
            Message::FrequencyY(ratio) => self.frequency_y = ratio,
            Message::Phase(degrees) => self.phase_degrees = degrees,
            Message::WindowOpened | Message::WindowResized => return self.frame(),
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let theme = &self.theme;

        let osc_x = column![
            knob(1..=7, self.frequency_x, Message::FrequencyX, |ratio| {
                format!("{ratio}x")
            }),
            output(),
        ];
        let osc_y = column![
            pin!(Left, Jack::Phase, label("phase"), Input, Port::Number),
            knob(1..=7, self.frequency_y, Message::FrequencyY, |ratio| {
                format!("{ratio}x")
            }),
            output(),
        ];
        let phase = column![
            knob(0..=360, self.phase_degrees, Message::Phase, |degrees| {
                format!("{degrees}\u{b0}")
            }),
            pin!(
                Right,
                Jack::Out,
                container(label("deg")).align_right(Length::Fill),
                Output,
                Port::Number
            ),
        ];
        let screen = column![
            pin!(Left, Jack::X, label("X"), Input, Port::Signal),
            pin!(Left, Jack::Y, label("Y"), Input, Port::Signal),
            Canvas::new(self.trace()).width(220.0).height(220.0),
        ];

        // The pins carry `Port`, so the graph is over `ScopeIds` rather than
        // the `Indexed` vocabulary `node_graph()` builds.
        NodeGraph::<ScopeIds, _, _, _>::new()
            .id(self.graph.clone())
            .on_move(Message::Moved)
            .on_connect(Message::Connected)
            .on_disconnect(Message::Disconnected)
            .can_connect(|from, to| default_can_connect(from, to) && from.info() == to.info())
            // The dragged edge (one loose end) takes the held pin's colour.
            .dragging_edge_style(|theme, pin| EdgeStyle {
                stroke_color: pin.info().color(theme).into(),
                ..default_edge_style(theme, EdgeStatus::Idle)
            })
            .push_node(
                unit(
                    Unit::OscX,
                    self.position(Unit::OscX),
                    "OSC X",
                    NodeContentStyle::input(theme),
                    180.0,
                    osc_x,
                )
                .style(NodeStyle::input),
            )
            .push_node(
                unit(
                    Unit::OscY,
                    self.position(Unit::OscY),
                    "OSC Y",
                    NodeContentStyle::input(theme),
                    180.0,
                    osc_y,
                )
                .style(NodeStyle::input),
            )
            .push_node(
                unit(
                    Unit::Phase,
                    self.position(Unit::Phase),
                    "PHASE",
                    NodeContentStyle::process(theme),
                    180.0,
                    phase,
                )
                .style(NodeStyle::process),
            )
            .push_node(unit(
                Unit::Scope,
                self.position(Unit::Scope),
                "XY SCOPE",
                NodeContentStyle::output(theme),
                240.0,
                screen,
            ))
            // Both ends of an edge share a port, so the start pin's colour is
            // the wire's.
            .edges(self.edges.iter().map(|&(from, to)| {
                edge((), from, to).style(|theme, status, start, _end| EdgeStyle {
                    stroke_color: start.info().color(theme).into(),
                    ..default_edge_style(theme, status)
                })
            }))
            .into()
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, _window| match event {
            Event::Window(window::Event::Opened { .. }) => Some(Message::WindowOpened),
            Event::Window(window::Event::Resized(_) | window::Event::Rescaled(_)) => {
                Some(Message::WindowResized)
            }
            _ => None,
        })
    }
}

/// Colours a pin by its [`Port`] payload.
fn pin_style(
    theme: &Theme,
    pin: &PinInfo<'_, ScopeIds>,
    _other: Option<&PinInfo<'_, ScopeIds>>,
    status: PinStatus,
) -> PinStyle {
    PinStyle {
        color: pin.info().color(theme).into(),
        ..default_pin_style(theme, status)
    }
}

/// A node of fixed `width`: a themed title bar over `body`, with the shared
/// pin styling.
fn unit<'a>(
    id: Unit,
    position: Point,
    title: &'a str,
    style: NodeContentStyle,
    width: f32,
    body: impl Into<Element<'a, Message>>,
) -> Node<'a, ScopeIds, Message, Theme, Renderer> {
    let content = column![
        node_title_bar(title, style),
        container(column![body.into()].spacing(6)).padding(Padding {
            top: 6.0,
            bottom: 8.0,
            left: 10.0,
            right: 10.0,
        }),
    ]
    .width(width);

    node(id, position, content).pin_style(pin_style)
}

fn label<'a>(name: &'a str) -> Element<'a, Message> {
    text(name).size(11).into()
}

/// The signal output every oscillator ends in.
fn output<'a>() -> Element<'a, Message> {
    pin!(
        Right,
        Jack::Out,
        container(label("out")).align_right(Length::Fill),
        Output,
        Port::Signal
    )
    .into()
}

/// An integer slider with its value printed beside it.
fn knob<'a>(
    range: std::ops::RangeInclusive<u32>,
    value: u32,
    on_change: fn(u32) -> Message,
    readout: impl Fn(u32) -> String,
) -> Element<'a, Message> {
    row![
        slider(range, value, on_change),
        text(readout(value)).size(13).width(36.0),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// The scope screen: the waves wired into X and Y, as the canvas draws them.
///
/// An unwired X sweeps the beam left to right, an unwired Y holds it on the
/// centre line, and with neither the beam rests as a dot in the centre.
struct Trace {
    x: Option<Wave>,
    y: Option<Wave>,
}

impl Trace {
    /// Samples over one full period of `t`, enough that the tightest figure
    /// the 1..=7 ratios allow still reads as a smooth curve.
    const SAMPLES: usize = 720;

    /// Beam deflection at sample `i`, both axes in -1..=1.
    fn beam(&self, i: usize) -> (f32, f32) {
        let s = i as f32 / (Self::SAMPLES - 1) as f32;
        let t = s * TAU;
        let x = self.x.map_or(s * 2.0 - 1.0, |wave| wave.at(t));
        let y = self.y.map_or(0.0, |wave| wave.at(t));
        (x, y)
    }
}

impl<Message> canvas::Program<Message> for Trace {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        const DIVISIONS: usize = 8;

        let palette = theme.palette();
        let mut frame = Frame::new(renderer, bounds.size());
        let size = frame.width().min(frame.height());
        let origin = Point::new((frame.width() - size) / 2.0, (frame.height() - size) / 2.0);

        let graticule = Path::new(|path| {
            for i in 0..=DIVISIONS {
                let offset = size * i as f32 / DIVISIONS as f32;
                path.move_to(origin + Vector::new(offset, 0.0));
                path.line_to(origin + Vector::new(offset, size));
                path.move_to(origin + Vector::new(0.0, offset));
                path.line_to(origin + Vector::new(size, offset));
            }
        });
        frame.stroke(
            &graticule,
            Stroke::default()
                .with_width(1.0)
                .with_color(palette.text.scale_alpha(0.12)),
        );

        let center = origin + Vector::new(size / 2.0, size / 2.0);
        // Leave the beam a margin inside the outer graticule line.
        let half = size / 2.0 * 0.88;
        let to_screen = |(x, y): (f32, f32)| center + Vector::new(x * half, -y * half);

        if self.x.is_none() && self.y.is_none() {
            frame.fill(&Path::circle(center, 2.5), palette.primary);
            return vec![frame.into_geometry()];
        }

        let beam = Path::new(|path| {
            path.move_to(to_screen(self.beam(0)));
            for i in 1..Self::SAMPLES {
                path.line_to(to_screen(self.beam(i)));
            }
        });
        // A wide faint pass under the sharp one reads as phosphor bloom.
        frame.stroke(
            &beam,
            Stroke::default()
                .with_width(5.0)
                .with_color(palette.primary.scale_alpha(0.18)),
        );
        frame.stroke(
            &beam,
            Stroke::default()
                .with_width(1.5)
                .with_color(palette.primary),
        );

        vec![frame.into_geometry()]
    }
}
