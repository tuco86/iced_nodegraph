use std::f32::consts::FRAC_PI_2;

use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text};
use iced::{Color, Element, Fill, Length, Rectangle, Size, Subscription, Theme};
use iced_sdf::{Curve, Drawable, Pattern, SdfPrimitive, Style, Tiling};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("SDF Basic - iced_sdf")
        .theme(App::theme)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

// --- Shared types ---

struct StaticEntry {
    name: &'static str,
    drawables: Vec<Drawable>,
    styles: Vec<Style>,
    extent: f32,
}

fn build_static_entries() -> Vec<StaticEntry> {
    vec![
        StaticEntry {
            name: "Line (DF)",
            drawables: vec![Curve::line([-80.0, -40.0], [80.0, 40.0])],
            styles: vec![Style::distance_field()],
            extent: 120.0,
        },
        StaticEntry {
            name: "Point (DF)",
            drawables: vec![Curve::point([0.0, 0.0], FRAC_PI_2)],
            styles: vec![Style::distance_field()],
            extent: 80.0,
        },
        StaticEntry {
            name: "Arc (DF)",
            drawables: vec![Curve::arc_segment([0.0, 0.0], 50.0, -FRAC_PI_2, std::f32::consts::PI)],
            styles: vec![Style::distance_field()],
            extent: 80.0,
        },
        StaticEntry {
            name: "Bezier (DF)",
            drawables: vec![Curve::bezier([-80.0, 30.0], [-30.0, -60.0], [30.0, 60.0], [80.0, -30.0])],
            styles: vec![Style::distance_field()],
            extent: 120.0,
        },
        StaticEntry {
            name: "Node (DF)",
            drawables: vec![build_node_shape(8.0)],
            styles: vec![Style::distance_field()],
            extent: 140.0,
        },
        StaticEntry {
            name: "Grid (DF)",
            drawables: vec![Tiling::grid(20.0, 20.0, 0.5)],
            styles: vec![Style::distance_field()],
            extent: 100.0,
        },
        StaticEntry {
            name: "Dots (DF)",
            drawables: vec![Tiling::dots(15.0, 15.0, 2.0)],
            styles: vec![Style::distance_field()],
            extent: 60.0,
        },
    ]
}

fn build_node_shape(cr: f32) -> Drawable {
    let w: f32 = 120.0;
    let h: f32 = 80.0;
    let pr: f32 = 5.0;
    let left_pins: &[f32] = &[-25.0, 0.0, 25.0];
    let right_pins: &[f32] = &[-15.0, 15.0];
    let mut s = Curve::shape([-w / 2.0 + cr, -h / 2.0], FRAC_PI_2);
    s = s.line(w - 2.0 * cr).arc(cr, FRAC_PI_2);
    s = edge_with_pins(s, h - 2.0 * cr, right_pins, pr);
    s = s.arc(cr, FRAC_PI_2).line(w - 2.0 * cr).arc(cr, FRAC_PI_2);
    let left_rev: Vec<f32> = left_pins.iter().rev().map(|&y| -y).collect();
    s = edge_with_pins(s, h - 2.0 * cr, &left_rev, pr);
    s = s.arc(cr, FRAC_PI_2);
    s.close()
}

fn edge_with_pins(mut s: iced_sdf::ShapeBuilder, length: f32, pins: &[f32], pr: f32) -> iced_sdf::ShapeBuilder {
    let half = length / 2.0;
    let mut sorted: Vec<f32> = pins.iter().map(|&y| y + half).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut pos = 0.0;
    for &pin_pos in &sorted {
        let gap = pin_pos - pr - pos;
        if gap > 0.01 { s = s.line(gap); }
        s = s.angle(FRAC_PI_2).arc(pr, -std::f32::consts::PI).angle(FRAC_PI_2);
        pos = pin_pos + pr;
    }
    let rem = length - pos;
    if rem > 0.01 { s = s.line(rem); }
    s
}

fn edge_bezier() -> Drawable {
    Curve::bezier([-80.0, 30.0], [-30.0, -60.0], [30.0, 60.0], [80.0, -30.0])
}

fn rgba(c: &[f32; 4]) -> Color { Color::from_rgba(c[0], c[1], c[2], c[3]) }

// --- Pattern picker ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternKind { Solid, Dashed, Arrowed, Dotted, DashDotted }

impl PatternKind {
    const ALL: &[Self] = &[Self::Solid, Self::Dashed, Self::Arrowed, Self::Dotted, Self::DashDotted];
}

impl std::fmt::Display for PatternKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Solid => "Solid", Self::Dashed => "Dashed", Self::Arrowed => "Arrowed",
            Self::Dotted => "Dotted", Self::DashDotted => "Dash-Dot",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeBorderKind { Solid, Dashed }

impl NodeBorderKind {
    const ALL: &[Self] = &[Self::Solid, Self::Dashed];
}

impl std::fmt::Display for NodeBorderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { Self::Solid => "Solid", Self::Dashed => "Dashed" })
    }
}

// --- Edge Editor ---

#[derive(Clone)]
struct EdgeEditor {
    pattern: PatternKind,
    thickness: f32,
    dash: f32,
    gap: f32,
    angle: f32,
    flow_speed: f32,
    stroke_color: [f32; 4],
    stroke_color_end: [f32; 4],
    outline_thickness: f32,
    outline_color: [f32; 4],
    border_visible: bool,
    border_gap: f32,
    border_thickness: f32,
    border_color: [f32; 4],
    border_color_end: [f32; 4],
    border_outline_thickness: f32,
    border_outline_color: [f32; 4],
    shadow_visible: bool,
    shadow_expand: f32,
    shadow_color: [f32; 4],
    shadow_color_end: [f32; 4],
    stroke_visible: bool,
}

impl Default for EdgeEditor {
    fn default() -> Self {
        Self {
            pattern: PatternKind::Solid,
            thickness: 6.0, dash: 14.0, gap: 8.0, angle: 0.0, flow_speed: 0.0,
            stroke_color: [0.2, 0.85, 1.0, 1.0],
            stroke_color_end: [0.6, 0.2, 1.0, 1.0],
            outline_thickness: 1.2,
            outline_color: [0.05, 0.05, 0.15, 1.0],
            border_visible: true,
            border_gap: 2.0, border_thickness: 3.0,
            border_color: [0.95, 0.75, 0.2, 1.0],
            border_color_end: [1.0, 0.3, 0.2, 1.0],
            border_outline_thickness: 0.8,
            border_outline_color: [0.05, 0.05, 0.15, 1.0],
            shadow_visible: true,
            shadow_expand: 10.0,
            shadow_color: [0.0, 0.0, 0.1, 0.35],
            shadow_color_end: [0.0, 0.0, 0.1, 0.0],
            stroke_visible: true,
        }
    }
}

impl EdgeEditor {
    fn build_pattern(&self) -> Pattern {
        let angle_rad = self.angle.to_radians();
        let p = match self.pattern {
            PatternKind::Solid => Pattern::solid(self.thickness),
            PatternKind::Dashed => Pattern::dashed_angle(self.thickness, self.dash, self.gap, angle_rad),
            PatternKind::Arrowed => Pattern::arrowed_angle(self.thickness, self.dash, self.gap, angle_rad),
            PatternKind::Dotted => Pattern::dotted(self.gap, self.dash * 0.3),
            PatternKind::DashDotted => Pattern::dash_dotted(self.thickness, self.dash, self.gap, self.dash * 0.2),
        };
        if self.flow_speed != 0.0 { p.flow(self.flow_speed) } else { p }
    }

    fn build_styles(&self) -> Vec<Style> {
        let mut styles = Vec::new();
        // Stroke (front)
        if self.stroke_visible {
            styles.push(
                Style::arc_gradient(rgba(&self.stroke_color), rgba(&self.stroke_color_end))
                    .with_pattern(self.build_pattern())
                    .outline(self.outline_thickness, rgba(&self.outline_color))
            );
        }
        // Border (middle)
        if self.border_visible {
            let border_total = self.thickness + self.border_gap * 2.0 + self.border_thickness * 2.0;
            styles.push(
                Style::arc_gradient(rgba(&self.border_color), rgba(&self.border_color_end))
                    .with_pattern(Pattern::solid(border_total))
                    .outline(self.border_outline_thickness, rgba(&self.border_outline_color))
            );
        }
        // Shadow (back)
        if self.shadow_visible {
            styles.push(
                Style::arc_gradient(rgba(&self.shadow_color), rgba(&self.shadow_color_end))
                    .expand(self.shadow_expand).blur(self.shadow_expand * 0.8)
                    .with_pattern(Pattern::solid(self.thickness + self.shadow_expand * 2.0))
            );
        }
        styles
    }
}

// --- Node Editor ---

#[derive(Clone)]
struct NodeEditor {
    fill_color: [f32; 4],
    corner_radius: f32,
    opacity: f32,
    border_pattern: NodeBorderKind,
    border_width: f32,
    border_color: [f32; 4],
    border_dash: f32,
    border_gap: f32,
    border_outline_width: f32,
    border_outline_color: [f32; 4],
    #[allow(dead_code)]
    shadow_offset_x: f32,
    #[allow(dead_code)]
    shadow_offset_y: f32,
    shadow_blur: f32,
    shadow_color: [f32; 4],
    fill_visible: bool,
    border_visible: bool,
    shadow_visible: bool,
}

impl Default for NodeEditor {
    fn default() -> Self {
        Self {
            fill_color: [0.14, 0.14, 0.16, 1.0],
            corner_radius: 8.0, opacity: 0.75,
            border_pattern: NodeBorderKind::Solid,
            border_width: 1.0,
            border_color: [0.30, 0.30, 0.35, 1.0],
            border_dash: 10.0, border_gap: 6.0,
            border_outline_width: 0.0,
            border_outline_color: [0.05, 0.05, 0.15, 1.0],
            shadow_offset_x: 4.0, shadow_offset_y: 4.0,
            shadow_blur: 8.0,
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            fill_visible: true, border_visible: true, shadow_visible: true,
        }
    }
}

impl NodeEditor {
    fn build_styles(&self) -> Vec<Style> {
        let mut styles = Vec::new();
        if self.border_visible && self.border_width > 0.001 {
            let pattern = match self.border_pattern {
                NodeBorderKind::Solid => Pattern::solid(self.border_width),
                NodeBorderKind::Dashed => Pattern::dashed(self.border_width, self.border_dash, self.border_gap),
            };
            styles.push(
                Style::stroke(rgba(&self.border_color), pattern)
                    .outline(self.border_outline_width, rgba(&self.border_outline_color))
            );
        }
        if self.fill_visible {
            let mut c = self.fill_color;
            c[3] *= self.opacity;
            styles.push(Style::solid(rgba(&c)));
        }
        if self.shadow_visible {
            styles.push(
                Style::solid(rgba(&self.shadow_color))
                    .expand(4.0).blur(self.shadow_blur)
            );
        }
        styles
    }
}

// --- App ---

const EDGE_ED: usize = 1000;
const NODE_ED: usize = 1001;

struct App {
    selected: usize,
    debug_tiles: bool,
    time: f32,
    entries: Vec<StaticEntry>,
    edge_ed: EdgeEditor,
    node_ed: NodeEditor,
}

impl Default for App {
    fn default() -> Self {
        Self {
            selected: 0, debug_tiles: false, time: 0.0,
            entries: build_static_entries(),
            edge_ed: EdgeEditor::default(),
            node_ed: NodeEditor::default(),
        }
    }
}

#[derive(Debug, Clone)]
enum Msg {
    Select(usize), ToggleDebug(bool), Tick,
    // Edge editor
    EPattern(PatternKind), EThick(f32), EDash(f32), EGap(f32), EAngle(f32), EFlow(f32),
    EColorR(f32), EColorG(f32), EColorB(f32),
    EColorEndR(f32), EColorEndG(f32), EColorEndB(f32),
    EOutline(f32), EBorderVis(bool), EBorderGap(f32), EBorderThick(f32),
    EShadowVis(bool), EShadowExpand(f32), EStrokeVis(bool),
    // Node editor
    NCorner(f32), NOpacity(f32), NBorderPat(NodeBorderKind), NBorderW(f32),
    NBorderDash(f32), NBorderGap(f32), NShadowBlur(f32),
    NFillR(f32), NFillG(f32), NFillB(f32),
    NBorderR(f32), NBorderG(f32), NBorderB(f32),
    NFillVis(bool), NBorderVis(bool), NShadowVis(bool),
}

impl App {
    fn theme(&self) -> Theme { Theme::Dark }

    fn update(&mut self, m: Msg) {
        match m {
            Msg::Select(i) => self.selected = i,
            Msg::ToggleDebug(v) => self.debug_tiles = v,
            Msg::Tick => self.time += 1.0 / 60.0,
            Msg::EPattern(p) => self.edge_ed.pattern = p,
            Msg::EThick(v) => self.edge_ed.thickness = v,
            Msg::EDash(v) => self.edge_ed.dash = v,
            Msg::EGap(v) => self.edge_ed.gap = v,
            Msg::EAngle(v) => self.edge_ed.angle = v,
            Msg::EFlow(v) => self.edge_ed.flow_speed = v,
            Msg::EColorR(v) => self.edge_ed.stroke_color[0] = v,
            Msg::EColorG(v) => self.edge_ed.stroke_color[1] = v,
            Msg::EColorB(v) => self.edge_ed.stroke_color[2] = v,
            // Alpha handled via opacity if needed
            Msg::EColorEndR(v) => self.edge_ed.stroke_color_end[0] = v,
            Msg::EColorEndG(v) => self.edge_ed.stroke_color_end[1] = v,
            Msg::EColorEndB(v) => self.edge_ed.stroke_color_end[2] = v,
            Msg::EOutline(v) => self.edge_ed.outline_thickness = v,
            Msg::EBorderVis(v) => self.edge_ed.border_visible = v,
            Msg::EBorderGap(v) => self.edge_ed.border_gap = v,
            Msg::EBorderThick(v) => self.edge_ed.border_thickness = v,
            Msg::EShadowVis(v) => self.edge_ed.shadow_visible = v,
            Msg::EShadowExpand(v) => self.edge_ed.shadow_expand = v,
            Msg::EStrokeVis(v) => self.edge_ed.stroke_visible = v,
            Msg::NCorner(v) => self.node_ed.corner_radius = v,
            Msg::NOpacity(v) => self.node_ed.opacity = v,
            Msg::NBorderPat(p) => self.node_ed.border_pattern = p,
            Msg::NBorderW(v) => self.node_ed.border_width = v,
            Msg::NBorderDash(v) => self.node_ed.border_dash = v,
            Msg::NBorderGap(v) => self.node_ed.border_gap = v,
            Msg::NShadowBlur(v) => self.node_ed.shadow_blur = v,
            Msg::NFillR(v) => self.node_ed.fill_color[0] = v,
            Msg::NFillG(v) => self.node_ed.fill_color[1] = v,
            Msg::NFillB(v) => self.node_ed.fill_color[2] = v,
            Msg::NBorderR(v) => self.node_ed.border_color[0] = v,
            Msg::NBorderG(v) => self.node_ed.border_color[1] = v,
            Msg::NBorderB(v) => self.node_ed.border_color[2] = v,
            Msg::NFillVis(v) => self.node_ed.fill_visible = v,
            Msg::NBorderVis(v) => self.node_ed.border_visible = v,
            Msg::NShadowVis(v) => self.node_ed.shadow_visible = v,
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let animated = match self.selected {
            EDGE_ED => self.edge_ed.flow_speed != 0.0,
            _ => self.entries.get(self.selected).is_some_and(|e| e.styles.iter().any(|s| s.is_animated())),
        };
        if animated { iced::window::frames().map(|_| Msg::Tick) } else { Subscription::none() }
    }

    fn view(&self) -> Element<'_, Msg> {
        let mut sidebar = column![].spacing(4).padding(8).width(160);
        for (i, e) in self.entries.iter().enumerate() {
            sidebar = sidebar.push(
                button(text(e.name).size(13)).width(Fill)
                    .on_press(Msg::Select(i))
                    .style(if i == self.selected { button::primary } else { button::secondary })
            );
        }
        sidebar = sidebar.push(iced::widget::Space::new().height(4));
        for &(idx, name) in &[(EDGE_ED, "Edge Editor"), (NODE_ED, "Node Editor")] {
            sidebar = sidebar.push(
                button(text(name).size(13)).width(Fill)
                    .on_press(Msg::Select(idx))
                    .style(if self.selected == idx { button::primary } else { button::secondary })
            );
        }
        sidebar = sidebar.push(iced::widget::Space::new().height(8));
        sidebar = sidebar.push(checkbox(self.debug_tiles).label("Debug Tiles").on_toggle(Msg::ToggleDebug).size(14));

        let main: Element<'_, Msg> = match self.selected {
            EDGE_ED => self.view_edge_editor(),
            NODE_ED => self.view_node_editor(),
            i => {
                if let Some(e) = self.entries.get(i) {
                    container(SdfCanvas { drawables: &e.drawables, styles: &e.styles, extent: e.extent, debug_tiles: self.debug_tiles, time: self.time })
                        .width(Fill).height(Fill).into()
                } else { text("?").into() }
            }
        };

        row![container(sidebar).height(Fill), main].into()
    }

    fn view_edge_editor(&self) -> Element<'_, Msg> {
        let ed = &self.edge_ed;
        let controls = scrollable(column![
            text("Stroke").size(14),
            checkbox(ed.stroke_visible).label("Visible").on_toggle(Msg::EStrokeVis).size(13),
            text("Pattern").size(12),
            pick_list(PatternKind::ALL, Some(ed.pattern), Msg::EPattern).width(140),
            labeled_slider("Thickness", 0.1, 20.0, ed.thickness, Msg::EThick),
            labeled_slider("Dash/Seg", 1.0, 50.0, ed.dash, Msg::EDash),
            labeled_slider("Gap", 1.0, 50.0, ed.gap, Msg::EGap),
            labeled_slider("Angle", -90.0, 90.0, ed.angle, Msg::EAngle),
            labeled_slider("Flow", -100.0, 100.0, ed.flow_speed, Msg::EFlow),
            text("Color Start").size(12),
            color_rgb(ed.stroke_color, Msg::EColorR, Msg::EColorG, Msg::EColorB),
            text("Color End").size(12),
            color_rgb(ed.stroke_color_end, Msg::EColorEndR, Msg::EColorEndG, Msg::EColorEndB),
            labeled_slider("Outline", 0.0, 10.0, ed.outline_thickness, Msg::EOutline),
            iced::widget::Space::new().height(8),
            text("Border").size(14),
            checkbox(ed.border_visible).label("Visible").on_toggle(Msg::EBorderVis).size(13),
            labeled_slider("Gap", 0.0, 20.0, ed.border_gap, Msg::EBorderGap),
            labeled_slider("Thickness", 0.0, 20.0, ed.border_thickness, Msg::EBorderThick),
            iced::widget::Space::new().height(8),
            text("Shadow").size(14),
            checkbox(ed.shadow_visible).label("Visible").on_toggle(Msg::EShadowVis).size(13),
            labeled_slider("Expand", 0.0, 50.0, ed.shadow_expand, Msg::EShadowExpand),
        ].spacing(3).padding(8).width(200));

        let styles = ed.build_styles();
        let d = vec![edge_bezier()];
        let canvas = SdfCanvasOwned { drawables: d, styles, extent: 120.0, debug_tiles: self.debug_tiles, time: self.time };
        row![container(controls).height(Fill), container(canvas).width(Fill).height(Fill)].into()
    }

    fn view_node_editor(&self) -> Element<'_, Msg> {
        let ed = &self.node_ed;
        let controls = scrollable(column![
            text("Fill").size(14),
            checkbox(ed.fill_visible).label("Visible").on_toggle(Msg::NFillVis).size(13),
            color_rgb(ed.fill_color, Msg::NFillR, Msg::NFillG, Msg::NFillB),
            labeled_slider("Radius", 0.0, 40.0, ed.corner_radius, Msg::NCorner),
            labeled_slider("Opacity", 0.0, 1.0, ed.opacity, Msg::NOpacity),
            iced::widget::Space::new().height(8),
            text("Border").size(14),
            checkbox(ed.border_visible).label("Visible").on_toggle(Msg::NBorderVis).size(13),
            pick_list(NodeBorderKind::ALL, Some(ed.border_pattern), Msg::NBorderPat).width(140),
            labeled_slider("Width", 0.0, 10.0, ed.border_width, Msg::NBorderW),
            color_rgb(ed.border_color, Msg::NBorderR, Msg::NBorderG, Msg::NBorderB),
            labeled_slider("Dash", 1.0, 30.0, ed.border_dash, Msg::NBorderDash),
            labeled_slider("Gap", 1.0, 20.0, ed.border_gap, Msg::NBorderGap),
            iced::widget::Space::new().height(8),
            text("Shadow").size(14),
            checkbox(ed.shadow_visible).label("Visible").on_toggle(Msg::NShadowVis).size(13),
            labeled_slider("Blur", 0.0, 30.0, ed.shadow_blur, Msg::NShadowBlur),
        ].spacing(3).padding(8).width(200));

        let shape = build_node_shape(ed.corner_radius);
        let styles = ed.build_styles();
        let canvas = SdfCanvasOwned { drawables: vec![shape], styles, extent: 140.0, debug_tiles: self.debug_tiles, time: self.time };
        row![container(controls).height(Fill), container(canvas).width(Fill).height(Fill)].into()
    }
}

fn labeled_slider<'a>(label: &'a str, min: f32, max: f32, value: f32, msg: fn(f32) -> Msg) -> Element<'a, Msg> {
    row![
        text(label).size(12).width(60),
        slider(min..=max, value, msg).step(if max - min > 10.0 { 0.5 } else { 0.01 }),
        text(format!("{value:.1}")).size(11).width(35),
    ].spacing(4).into()
}

fn color_rgb<'a>(c: [f32; 4], r: fn(f32) -> Msg, g: fn(f32) -> Msg, b: fn(f32) -> Msg) -> Element<'a, Msg> {
    column![
        row![text("R").size(11).width(14), slider(0.0..=1.0, c[0], r).step(0.01)].spacing(2),
        row![text("G").size(11).width(14), slider(0.0..=1.0, c[1], g).step(0.01)].spacing(2),
        row![text("B").size(11).width(14), slider(0.0..=1.0, c[2], b).step(0.01)].spacing(2),
    ].spacing(1).into()
}

// --- SdfCanvas (borrows) ---

struct SdfCanvas<'a> {
    drawables: &'a [Drawable],
    styles: &'a [Style],
    extent: f32,
    debug_tiles: bool,
    time: f32,
}

impl<'a, Message, R> iced::advanced::Widget<Message, Theme, R> for SdfCanvas<'a>
where R: iced::advanced::Renderer + iced_wgpu::primitive::Renderer {
    fn size(&self) -> Size<Length> { Size::new(Length::Fill, Length::Fill) }
    fn layout(&mut self, _: &mut iced::advanced::widget::Tree, _: &R, l: &iced::advanced::layout::Limits) -> iced::advanced::layout::Node {
        iced::advanced::layout::Node::new(l.width(Length::Fill).height(Length::Fill).resolve(Length::Fill, Length::Fill, Size::ZERO))
    }
    fn draw(&self, _: &iced::advanced::widget::Tree, renderer: &mut R, _: &Theme, _: &iced::advanced::renderer::Style, layout: iced::advanced::Layout<'_>, _: iced::advanced::mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let z = b.width.min(b.height) * 0.333 / self.extent;
        let sb = [b.x, b.y, b.width, b.height];
        let mut prim = SdfPrimitive::new();
        for (i, style) in self.styles.iter().enumerate() {
            prim.push(&self.drawables[i.min(self.drawables.len() - 1)], style, sb);
        }
        renderer.draw_primitive(b, prim.camera(b.width * 0.5 / z, b.height * 0.5 / z, z).time(self.time).debug_tiles(self.debug_tiles));
    }
}

impl<'a, M: 'a, R: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a> From<SdfCanvas<'a>> for Element<'a, M, Theme, R> {
    fn from(c: SdfCanvas<'a>) -> Self { Element::new(c) }
}

// --- SdfCanvasOwned (for editors) ---

struct SdfCanvasOwned {
    drawables: Vec<Drawable>,
    styles: Vec<Style>,
    extent: f32,
    debug_tiles: bool,
    time: f32,
}

impl<Message, R> iced::advanced::Widget<Message, Theme, R> for SdfCanvasOwned
where R: iced::advanced::Renderer + iced_wgpu::primitive::Renderer {
    fn size(&self) -> Size<Length> { Size::new(Length::Fill, Length::Fill) }
    fn layout(&mut self, _: &mut iced::advanced::widget::Tree, _: &R, l: &iced::advanced::layout::Limits) -> iced::advanced::layout::Node {
        iced::advanced::layout::Node::new(l.width(Length::Fill).height(Length::Fill).resolve(Length::Fill, Length::Fill, Size::ZERO))
    }
    fn draw(&self, _: &iced::advanced::widget::Tree, renderer: &mut R, _: &Theme, _: &iced::advanced::renderer::Style, layout: iced::advanced::Layout<'_>, _: iced::advanced::mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let z = b.width.min(b.height) * 0.333 / self.extent;
        let sb = [b.x, b.y, b.width, b.height];
        let mut prim = SdfPrimitive::new();
        for (i, style) in self.styles.iter().enumerate() {
            prim.push(&self.drawables[i.min(self.drawables.len() - 1)], style, sb);
        }
        renderer.draw_primitive(b, prim.camera(b.width * 0.5 / z, b.height * 0.5 / z, z).time(self.time).debug_tiles(self.debug_tiles));
    }
}

impl<'a, M: 'a, R: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a> From<SdfCanvasOwned> for Element<'a, M, Theme, R> {
    fn from(c: SdfCanvasOwned) -> Self { Element::new(c) }
}
