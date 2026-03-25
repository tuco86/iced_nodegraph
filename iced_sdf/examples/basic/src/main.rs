use std::f32::consts::FRAC_PI_2;

use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Color, Element, Fill, Length, Rectangle, Size, Subscription, Theme};
use iced_sdf::{Curve, Drawable, Pattern, SdfPrimitive, Style};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("SDF Basic - iced_sdf")
        .theme(App::theme)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

// --- Shape definitions ---

/// A layer: drawable reference + style.
struct Layer {
    drawable_idx: usize,
    style: Style,
}

struct ShapeEntry {
    name: &'static str,
    drawables: Vec<Drawable>,
    layers: Vec<Layer>,
    extent: f32,
}

fn build_entries() -> Vec<ShapeEntry> {
    let edge = Curve::bezier([-80.0, 30.0], [-30.0, -60.0], [30.0, 60.0], [80.0, -30.0]);
    let node = build_node_shape();
    vec![
        // --- Distance field views ---
        ShapeEntry {
            name: "Line (DF)",
            drawables: vec![Curve::line([-80.0, -40.0], [80.0, 40.0])],
            layers: vec![Layer { drawable_idx: 0, style: Style::distance_field() }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Bezier (DF)",
            drawables: vec![edge.clone()],
            layers: vec![Layer { drawable_idx: 0, style: Style::distance_field() }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Point (DF)",
            drawables: vec![Curve::point([0.0, 0.0], FRAC_PI_2)],
            layers: vec![Layer { drawable_idx: 0, style: Style::distance_field() }],
            extent: 80.0,
        },
        ShapeEntry {
            name: "Arc (DF)",
            drawables: vec![Curve::arc_segment([0.0, 0.0], 50.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::PI)],
            layers: vec![Layer { drawable_idx: 0, style: Style::distance_field() }],
            extent: 80.0,
        },
        ShapeEntry {
            name: "Node (DF)",
            drawables: vec![build_node_shape()],
            layers: vec![Layer { drawable_idx: 0, style: Style::distance_field() }],
            extent: 140.0,
        },
        // --- Pattern views ---
        ShapeEntry {
            name: "Solid",
            drawables: vec![edge.clone()],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(Color::from_rgb(0.2, 0.85, 1.0), Pattern::solid(3.0)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Dashed",
            drawables: vec![edge.clone()],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(Color::from_rgb(0.2, 0.85, 1.0), Pattern::dashed(3.0, 12.0, 6.0)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Arrowed",
            drawables: vec![edge.clone()],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(Color::from_rgb(0.95, 0.75, 0.2), Pattern::arrowed(3.0, 10.0, 5.0)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Dotted",
            drawables: vec![edge.clone()],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(Color::from_rgb(0.8, 0.3, 1.0), Pattern::dotted(10.0, 2.0)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Dash-Dot",
            drawables: vec![edge.clone()],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(Color::from_rgb(0.3, 1.0, 0.5), Pattern::dash_dotted(3.0, 12.0, 6.0, 1.5)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Flow",
            drawables: vec![edge],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::stroke(
                    Color::from_rgb(1.0, 0.5, 0.2),
                    Pattern::dashed(3.0, 10.0, 5.0).flow(30.0),
                ),
            }],
            extent: 120.0,
        },
        // --- Style features ---
        ShapeEntry {
            name: "Gradient",
            drawables: vec![Curve::bezier(
                [-80.0, 30.0], [-30.0, -60.0], [30.0, 60.0], [80.0, -30.0],
            )],
            layers: vec![Layer {
                drawable_idx: 0,
                style: Style::arc_gradient(
                    Color::from_rgb(0.2, 0.85, 1.0),
                    Color::from_rgb(0.6, 0.2, 1.0),
                ).with_pattern(Pattern::solid(4.0)),
            }],
            extent: 120.0,
        },
        ShapeEntry {
            name: "Outline",
            drawables: vec![build_node_shape()],
            layers: vec![
                Layer {
                    drawable_idx: 0,
                    style: Style::solid(Color::from_rgb(0.14, 0.14, 0.16))
                        .outline(1.5, Color::from_rgb(0.4, 0.8, 1.0)),
                },
            ],
            extent: 140.0,
        },
        // --- Styled node ---
        ShapeEntry {
            name: "Node (Styled)",
            drawables: vec![node],
            layers: vec![
                Layer {
                    drawable_idx: 0,
                    style: Style::stroke(Color::from_rgb(0.3, 0.3, 0.35), Pattern::solid(1.0)),
                },
                Layer {
                    drawable_idx: 0,
                    style: Style::solid(Color::from_rgb(0.14, 0.14, 0.16)),
                },
                Layer {
                    drawable_idx: 0,
                    style: Style::solid(Color::from_rgba(0.0, 0.0, 0.0, 0.3))
                        .expand(4.0).blur(8.0),
                },
            ],
            extent: 140.0,
        },
    ]
}

fn build_node_shape() -> Drawable {
    let w: f32 = 120.0;
    let h: f32 = 80.0;
    let cr: f32 = 8.0;  // corner radius
    let pr: f32 = 5.0;  // pin radius

    // Pin Y-positions relative to node center
    let left_pins: &[f32] = &[-25.0, 0.0, 25.0];
    let right_pins: &[f32] = &[-15.0, 15.0];

    // Start after top-left corner, heading RIGHT (PI/2). CW turtle construction.
    let mut s = Curve::shape([-w / 2.0 + cr, -h / 2.0], FRAC_PI_2);

    // Top edge → (minus 2 corner radii)
    s = s.line(w - 2.0 * cr);
    s = s.arc(cr, FRAC_PI_2); // top-right rounded corner

    // Right edge ↓ with pin cutouts (minus 2 corner radii)
    s = edge_with_pins(s, h - 2.0 * cr, right_pins, pr);
    s = s.arc(cr, FRAC_PI_2); // bottom-right rounded corner

    // Bottom edge ←
    s = s.line(w - 2.0 * cr);
    s = s.arc(cr, FRAC_PI_2); // bottom-left rounded corner

    // Left edge ↑ with pin cutouts (negate Y because going upward)
    let left_reversed: Vec<f32> = left_pins.iter().rev().map(|&y| -y).collect();
    s = edge_with_pins(s, h - 2.0 * cr, &left_reversed, pr);
    s = s.arc(cr, FRAC_PI_2); // top-left rounded corner

    s.close()
}

/// Build an edge of total `length` going forward with semicircular pin cutouts.
/// `pins` are pin center offsets from edge midpoint.
/// Pure turtle: `line` + `angle` + `arc`.
fn edge_with_pins(
    mut s: iced_sdf::ShapeBuilder,
    length: f32,
    pins: &[f32],
    pr: f32,
) -> iced_sdf::ShapeBuilder {
    let half = length / 2.0;
    let mut sorted: Vec<f32> = pins.iter().map(|&y| y + half).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut pos = 0.0;
    for &pin_pos in &sorted {
        // Straight to pin start
        let gap = pin_pos - pr - pos;
        if gap > 0.01 {
            s = s.line(gap);
        }

        // Semicircular cutout: kink left, CCW arc, kink left
        // Net heading change: PI/2 + (-PI) + PI/2 = 0
        s = s.angle(FRAC_PI_2)               // turn left (inward for CW contour)
             .arc(pr, -std::f32::consts::PI)  // CCW half-circle (center ahead)
             .angle(FRAC_PI_2);              // turn left back to forward

        pos = pin_pos + pr;
    }

    // Remaining edge
    let remaining = length - pos;
    if remaining > 0.01 {
        s = s.line(remaining);
    }
    s
}

// --- App ---

struct App {
    selected: usize,
    debug_tiles: bool,
    time: f32,
    entries: Vec<ShapeEntry>,
}

impl Default for App {
    fn default() -> Self {
        Self { selected: 0, debug_tiles: false, time: 0.0, entries: build_entries() }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    ToggleDebugTiles(bool),
    Tick,
}

impl App {
    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Select(i) => self.selected = i,
            Message::ToggleDebugTiles(v) => self.debug_tiles = v,
            Message::Tick => self.time += 1.0 / 60.0,
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        // Check if current entry has animated styles
        let has_anim = self.entries[self.selected].layers.iter()
            .any(|l| l.style.is_animated());
        if has_anim {
            iced::window::frames().map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // Sidebar
        let mut sidebar = column![].spacing(4).padding(8).width(160);
        for (i, entry) in self.entries.iter().enumerate() {
            let btn = button(text(entry.name).size(14))
                .width(Fill)
                .on_press(Message::Select(i))
                .style(if i == self.selected {
                    button::primary
                } else {
                    button::secondary
                });
            sidebar = sidebar.push(btn);
        }
        sidebar = sidebar.push(iced::widget::Space::new().height(8));
        sidebar = sidebar.push(
            checkbox(self.debug_tiles)
                .label("Debug Tiles")
                .on_toggle(Message::ToggleDebugTiles)
                .size(14)
        );

        let sidebar = container(sidebar).height(Fill);

        // Main area: SDF canvas
        let canvas = SdfCanvas {
            entry: &self.entries[self.selected],
            debug_tiles: self.debug_tiles,
            time: self.time,
        };

        let main = container(canvas)
            .width(Fill)
            .height(Fill);

        row![sidebar, main].into()
    }
}

// --- SdfCanvas widget ---

struct SdfCanvas<'a> {
    entry: &'a ShapeEntry,
    debug_tiles: bool,
    time: f32,
}

impl<'a, Message, Renderer> iced::advanced::Widget<Message, Theme, Renderer> for SdfCanvas<'a>
where
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        let size = limits
            .width(Length::Fill)
            .height(Length::Fill)
            .resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        // Camera: center world origin in widget, zoom to fit ~2/3 of smaller dimension
        let viewport_min = bounds.width.min(bounds.height);
        let zoom = viewport_min * 0.333 / self.entry.extent;

        let cam_x = bounds.width * 0.5 / zoom;
        let cam_y = bounds.height * 0.5 / zoom;

        let sb = [bounds.x, bounds.y, bounds.width, bounds.height];

        let mut prim = SdfPrimitive::new();
        for layer in &self.entry.layers {
            prim.push(&self.entry.drawables[layer.drawable_idx], &layer.style, sb);
        }
        let prim = prim
            .camera(cam_x, cam_y, zoom)
            .time(self.time)
            .debug_tiles(self.debug_tiles);

        renderer.draw_primitive(bounds, prim);
    }
}

impl<'a, Message: 'a, Renderer> From<SdfCanvas<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(canvas: SdfCanvas<'a>) -> Self {
        Element::new(canvas)
    }
}
