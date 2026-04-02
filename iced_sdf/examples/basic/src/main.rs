use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text};
use iced::{Color, Element, Fill, Length, Rectangle, Size, Subscription, Theme};
use iced_sdf::{Curve, Drawable, Pattern, SdfPrimitive, Style, Tiling};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("SDF Basic - iced_sdf")
        .font(iced_aw::ICED_AW_FONT_BYTES)
        .theme(App::theme)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

// --- DF View system ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DfField { F0, F1, F2, F3, F4, F5, F6, F7 }

#[derive(Clone)]
enum DfEditor {
    Line { ax: f32, ay: f32, bx: f32, by: f32 },
    Point { x: f32, y: f32, heading: f32 },
    Arc { cx: f32, cy: f32, radius: f32, start: f32, sweep: f32 },
    Bezier { p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2] },
    Node { corner_radius: f32 },
    Grid { spacing_x: f32, spacing_y: f32, thickness: f32 },
    Dots { spacing_x: f32, spacing_y: f32, radius: f32 },
    Triangles { spacing: f32, thickness: f32 },
    Hex { spacing: f32, thickness: f32 },
}

impl DfEditor {
    fn build_drawable(&self) -> Drawable {
        match self {
            Self::Line { ax, ay, bx, by } => Curve::line([*ax, *ay], [*bx, *by]),
            Self::Point { x, y, heading } => Curve::point([*x, *y], *heading),
            Self::Arc { cx, cy, radius, start, sweep } =>
                Curve::arc_segment([*cx, *cy], *radius, *start, *sweep),
            Self::Bezier { p0, p1, p2, p3 } => Curve::bezier(*p0, *p1, *p2, *p3),
            Self::Node { corner_radius } => build_node_shape(*corner_radius),
            Self::Grid { spacing_x, spacing_y, thickness } =>
                Tiling::grid(*spacing_x, *spacing_y, *thickness),
            Self::Dots { spacing_x, spacing_y, radius } =>
                Tiling::dots(*spacing_x, *spacing_y, *radius),
            Self::Triangles { spacing, thickness } => Tiling::triangles(*spacing, *thickness),
            Self::Hex { spacing, thickness } => Tiling::hex(*spacing, *thickness),
        }
    }

    fn set_field(&mut self, field: DfField, val: f32) {
        use DfField::*;
        match (self, field) {
            (Self::Line { ax, .. }, F0) => *ax = val,
            (Self::Line { ay, .. }, F1) => *ay = val,
            (Self::Line { bx, .. }, F2) => *bx = val,
            (Self::Line { by, .. }, F3) => *by = val,

            (Self::Point { x, .. }, F0) => *x = val,
            (Self::Point { y, .. }, F1) => *y = val,
            (Self::Point { heading, .. }, F2) => *heading = val,

            (Self::Arc { cx, .. }, F0) => *cx = val,
            (Self::Arc { cy, .. }, F1) => *cy = val,
            (Self::Arc { radius, .. }, F2) => *radius = val,
            (Self::Arc { start, .. }, F3) => *start = val,
            (Self::Arc { sweep, .. }, F4) => *sweep = val,

            (Self::Bezier { p0, .. }, F0) => p0[0] = val,
            (Self::Bezier { p0, .. }, F1) => p0[1] = val,
            (Self::Bezier { p1, .. }, F2) => p1[0] = val,
            (Self::Bezier { p1, .. }, F3) => p1[1] = val,
            (Self::Bezier { p2, .. }, F4) => p2[0] = val,
            (Self::Bezier { p2, .. }, F5) => p2[1] = val,
            (Self::Bezier { p3, .. }, F6) => p3[0] = val,
            (Self::Bezier { p3, .. }, F7) => p3[1] = val,

            (Self::Node { corner_radius }, F0) => *corner_radius = val,

            (Self::Grid { spacing_x, .. }, F0) => *spacing_x = val,
            (Self::Grid { spacing_y, .. }, F1) => *spacing_y = val,
            (Self::Grid { thickness, .. }, F2) => *thickness = val,

            (Self::Dots { spacing_x, .. }, F0) => *spacing_x = val,
            (Self::Dots { spacing_y, .. }, F1) => *spacing_y = val,
            (Self::Dots { radius, .. }, F2) => *radius = val,

            (Self::Triangles { spacing, .. }, F0) => *spacing = val,
            (Self::Triangles { thickness, .. }, F1) => *thickness = val,

            (Self::Hex { spacing, .. }, F0) => *spacing = val,
            (Self::Hex { thickness, .. }, F1) => *thickness = val,

            _ => {}
        }
    }

    fn view_sliders(&self, idx: usize) -> iced::widget::Column<'_, Msg> {
        let mut col = column![].spacing(3);
        match self {
            Self::Line { ax, ay, bx, by } => {
                col = col
                    .push(df_slider("Start X", -200.0, 200.0, *ax, idx, DfField::F0))
                    .push(df_slider("Start Y", -200.0, 200.0, *ay, idx, DfField::F1))
                    .push(df_slider("End X", -200.0, 200.0, *bx, idx, DfField::F2))
                    .push(df_slider("End Y", -200.0, 200.0, *by, idx, DfField::F3));
            }
            Self::Point { x, y, heading } => {
                col = col
                    .push(df_slider("X", -200.0, 200.0, *x, idx, DfField::F0))
                    .push(df_slider("Y", -200.0, 200.0, *y, idx, DfField::F1))
                    .push(df_slider("Heading", 0.0, TAU, *heading, idx, DfField::F2));
            }
            Self::Arc { cx, cy, radius, start, sweep } => {
                col = col
                    .push(df_slider("Center X", -200.0, 200.0, *cx, idx, DfField::F0))
                    .push(df_slider("Center Y", -200.0, 200.0, *cy, idx, DfField::F1))
                    .push(df_slider("Radius", 1.0, 200.0, *radius, idx, DfField::F2))
                    .push(df_slider("Start", -PI, PI, *start, idx, DfField::F3))
                    .push(df_slider("Sweep", -TAU, TAU, *sweep, idx, DfField::F4));
            }
            Self::Bezier { p0, p1, p2, p3 } => {
                col = col
                    .push(df_slider("P0 X", -200.0, 200.0, p0[0], idx, DfField::F0))
                    .push(df_slider("P0 Y", -200.0, 200.0, p0[1], idx, DfField::F1))
                    .push(df_slider("P1 X", -200.0, 200.0, p1[0], idx, DfField::F2))
                    .push(df_slider("P1 Y", -200.0, 200.0, p1[1], idx, DfField::F3))
                    .push(df_slider("P2 X", -200.0, 200.0, p2[0], idx, DfField::F4))
                    .push(df_slider("P2 Y", -200.0, 200.0, p2[1], idx, DfField::F5))
                    .push(df_slider("P3 X", -200.0, 200.0, p3[0], idx, DfField::F6))
                    .push(df_slider("P3 Y", -200.0, 200.0, p3[1], idx, DfField::F7));
            }
            Self::Node { corner_radius } => {
                col = col.push(df_slider("Radius", 0.0, 40.0, *corner_radius, idx, DfField::F0));
            }
            Self::Grid { spacing_x, spacing_y, thickness } => {
                col = col
                    .push(df_slider("Width", 2.0, 100.0, *spacing_x, idx, DfField::F0))
                    .push(df_slider("Height", 2.0, 100.0, *spacing_y, idx, DfField::F1))
                    .push(df_slider("Thickness", 0.1, 10.0, *thickness, idx, DfField::F2));
            }
            Self::Dots { spacing_x, spacing_y, radius } => {
                col = col
                    .push(df_slider("Width", 2.0, 100.0, *spacing_x, idx, DfField::F0))
                    .push(df_slider("Height", 2.0, 100.0, *spacing_y, idx, DfField::F1))
                    .push(df_slider("Radius", 0.1, 20.0, *radius, idx, DfField::F2));
            }
            Self::Triangles { spacing, thickness } => {
                col = col
                    .push(df_slider("Spacing", 2.0, 100.0, *spacing, idx, DfField::F0))
                    .push(df_slider("Thickness", 0.1, 10.0, *thickness, idx, DfField::F1));
            }
            Self::Hex { spacing, thickness } => {
                col = col
                    .push(df_slider("Spacing", 2.0, 100.0, *spacing, idx, DfField::F0))
                    .push(df_slider("Thickness", 0.1, 10.0, *thickness, idx, DfField::F1));
            }
        }
        col
    }
}

#[derive(Clone)]
struct DfView {
    name: &'static str,
    editor: DfEditor,
    extent: f32,
}

impl DfView {
    fn build_style(&self) -> Style {
        Style::distance_field()
    }
}

fn build_df_views() -> Vec<DfView> {
    vec![
        DfView { name: "Line (DF)", extent: 120.0,
            editor: DfEditor::Line { ax: -80.0, ay: -40.0, bx: 80.0, by: 40.0 } },
        DfView { name: "Point (DF)", extent: 80.0,
            editor: DfEditor::Point { x: 0.0, y: 0.0, heading: FRAC_PI_2 } },
        DfView { name: "Arc (DF)", extent: 80.0,
            editor: DfEditor::Arc { cx: 0.0, cy: 0.0, radius: 50.0, start: -FRAC_PI_2, sweep: PI } },
        DfView { name: "Bezier (DF)", extent: 120.0,
            editor: DfEditor::Bezier { p0: [-80.0, 30.0], p1: [-30.0, -60.0], p2: [30.0, 60.0], p3: [80.0, -30.0] } },
        DfView { name: "Node (DF)", extent: 140.0,
            editor: DfEditor::Node { corner_radius: 8.0 } },
        DfView { name: "Grid (DF)", extent: 100.0,
            editor: DfEditor::Grid { spacing_x: 20.0, spacing_y: 20.0, thickness: 0.5 } },
        DfView { name: "Dots (DF)", extent: 60.0,
            editor: DfEditor::Dots { spacing_x: 15.0, spacing_y: 15.0, radius: 2.0 } },
        DfView { name: "Triangles (DF)", extent: 100.0,
            editor: DfEditor::Triangles { spacing: 20.0, thickness: 0.5 } },
        DfView { name: "Hex (DF)", extent: 100.0,
            editor: DfEditor::Hex { spacing: 20.0, thickness: 0.5 } },
    ]
}

fn df_slider<'a>(label: &'a str, min: f32, max: f32, value: f32, idx: usize, field: DfField) -> Element<'a, Msg> {
    row![
        text(label).size(12).width(60),
        slider(min..=max, value, move |v| Msg::DfParam(idx, field, v))
            .step(if max - min > 10.0 { 0.5 } else { 0.01 }),
        text(format!("{value:.2}")).size(11).width(40),
    ].spacing(4).into()
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

fn build_edge_drawables() -> Vec<Drawable> {
    // Two fixed crossing S-curves
    let fwd = Curve::bezier([-120.0, -40.0], [-40.0, -40.0], [40.0, 40.0], [120.0, 40.0]);
    let mir = Curve::bezier([120.0, -40.0], [40.0, -40.0], [-40.0, 40.0], [-120.0, 40.0]);
    let mut edges = vec![fwd, mir];
    // Random edges (deterministic pseudo-random)
    for i in 0..498 {
        let seed = (i + 7) as f32;
        let x0 = ((seed * 131.7) % 400.0) - 200.0;
        let y0 = ((seed * 97.3) % 300.0) - 150.0;
        let x1 = ((seed * 173.1) % 400.0) - 200.0;
        let y1 = ((seed * 59.9) % 300.0) - 150.0;
        let offset = 40.0 + (seed * 23.7) % 60.0;
        edges.push(Curve::bezier([x0, y0], [x0 + offset, y0], [x1 - offset, y1], [x1, y1]));
    }
    edges
}

fn rgba(c: &[f32; 4]) -> Color { Color::from_rgba(c[0], c[1], c[2], c[3]) }

// --- Color field identifiers for Edge Editor ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EColorField {
    Stroke, StrokeEnd, StrokeOutline,
    Border, BorderEnd,
    Shadow, ShadowEnd,
}

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
    // Color picker popup state
    open_pickers: HashSet<EColorField>,

    // Edges
    edge_count: u32,
    edges: Vec<Drawable>,

    // Pattern
    pattern: PatternKind,
    thickness: f32,

    // Dashed params
    dash: f32,
    dash_gap: f32,
    dash_angle: f32,

    // Arrowed params
    arrow_segment: f32,
    arrow_gap: f32,
    arrow_angle: f32,

    // Dotted params
    dot_gap: f32,
    dot_radius: f32,

    // DashDotted params
    dd_dash: f32,
    dd_gap: f32,
    dd_dot_radius: f32,

    // Flow
    flow_speed: f32,

    // Stroke
    stroke_visible: bool,
    stroke_color: [f32; 4],
    stroke_color_end: [f32; 4],
    stroke_outline_thickness: f32,
    stroke_outline_color: [f32; 4],

    // Border
    border_visible: bool,
    border_gap: f32,
    border_thickness: f32,
    border_color: [f32; 4],
    border_color_end: [f32; 4],

    // Shadow
    shadow_visible: bool,
    shadow_expand: f32,
    shadow_color: [f32; 4],
    shadow_color_end: [f32; 4],
}

impl Default for EdgeEditor {
    fn default() -> Self {
        Self {
            open_pickers: HashSet::new(),
            edge_count: 2,
            edges: build_edge_drawables(),
            pattern: PatternKind::Solid,
            thickness: 6.0,
            dash: 14.0, dash_gap: 8.0, dash_angle: 0.0,
            arrow_segment: 10.0, arrow_gap: 8.0, arrow_angle: 45.0,
            dot_gap: 6.0, dot_radius: 4.0,
            dd_dash: 14.0, dd_gap: 6.0, dd_dot_radius: 3.0,
            flow_speed: 0.0,
            stroke_visible: true,
            stroke_color: [0.2, 0.85, 1.0, 1.0],
            stroke_color_end: [0.6, 0.2, 1.0, 1.0],
            stroke_outline_thickness: 1.2,
            stroke_outline_color: [0.05, 0.05, 0.15, 1.0],
            border_visible: true,
            border_gap: 2.0, border_thickness: 3.0,
            border_color: [0.95, 0.75, 0.2, 1.0],
            border_color_end: [1.0, 0.3, 0.2, 1.0],
            shadow_visible: true,
            shadow_expand: 10.0,
            shadow_color: [0.0, 0.0, 0.1, 0.35],
            shadow_color_end: [0.0, 0.0, 0.1, 0.0],
        }
    }
}

impl EdgeEditor {
    fn set_color(&mut self, field: EColorField, c: Color) {
        let arr = [c.r, c.g, c.b, c.a];
        match field {
            EColorField::Stroke => self.stroke_color = arr,
            EColorField::StrokeEnd => self.stroke_color_end = arr,
            EColorField::StrokeOutline => self.stroke_outline_color = arr,
            EColorField::Border => self.border_color = arr,
            EColorField::BorderEnd => self.border_color_end = arr,
            EColorField::Shadow => self.shadow_color = arr,
            EColorField::ShadowEnd => self.shadow_color_end = arr,
        }
    }

    fn build_pattern(&self) -> Pattern {
        let p = match self.pattern {
            PatternKind::Solid => Pattern::solid(self.thickness),
            PatternKind::Dashed => Pattern::dashed_angle(self.thickness, self.dash, self.dash_gap, self.dash_angle.to_radians()),
            PatternKind::Arrowed => Pattern::arrowed_angle(self.thickness, self.arrow_segment, self.arrow_gap, self.arrow_angle.to_radians()),
            PatternKind::Dotted => Pattern::dotted(self.dot_gap + self.dot_radius * 2.0, self.dot_radius),
            PatternKind::DashDotted => Pattern::dash_dotted(self.thickness, self.dd_dash, self.dd_gap, self.dd_dot_radius),
        };
        if self.flow_speed != 0.0 { p.flow(self.flow_speed) } else { p }
    }

    fn build_styles(&self) -> Vec<Style> {
        let mut styles = Vec::new();
        // Stroke (front)
        if self.stroke_visible {
            styles.push(
                Style::arc_gradient_stroke(rgba(&self.stroke_color), rgba(&self.stroke_color_end), self.build_pattern())
            );
            // Outline behind stroke
            if self.stroke_outline_thickness > 0.01 {
                let outline_total = self.thickness + self.stroke_outline_thickness * 2.0;
                styles.push(
                    Style::stroke(rgba(&self.stroke_outline_color), Pattern::solid(outline_total))
                );
            }
        }
        // Border (middle)
        if self.border_visible {
            let border_total = self.thickness + self.border_gap * 2.0 + self.border_thickness * 2.0;
            styles.push(
                Style::arc_gradient_stroke(rgba(&self.border_color), rgba(&self.border_color_end), Pattern::solid(border_total))
            );
        }
        // Shadow (back)
        if self.shadow_visible {
            let sc = rgba(&self.shadow_color);
            let se = rgba(&self.shadow_color_end);
            styles.push(Style {
                near_start: sc, near_end: sc,
                far_start: se, far_end: se,
                dist_from: 0.0, dist_to: self.shadow_expand,
                pattern: None, distance_field: false,
            });
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
            );
        }
        if self.fill_visible {
            let mut c = self.fill_color;
            c[3] *= self.opacity;
            styles.push(Style::solid(rgba(&c)));
        }
        if self.shadow_visible {
            styles.push(Style::shadow(rgba(&self.shadow_color), self.shadow_blur));
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
    df_views: Vec<DfView>,
    edge_ed: EdgeEditor,
    node_ed: NodeEditor,
}

impl Default for App {
    fn default() -> Self {
        Self {
            selected: 0, debug_tiles: false, time: 0.0,
            df_views: build_df_views(),
            edge_ed: EdgeEditor::default(),
            node_ed: NodeEditor::default(),
        }
    }
}

#[derive(Debug, Clone)]
enum Msg {
    Select(usize), ToggleDebug(bool), Tick,
    // DF view params
    DfParam(usize, DfField, f32),
    // Edge editor - pattern & params
    EPattern(PatternKind), EThick(f32), EFlow(f32),
    EDash(f32), EDashGap(f32), EDashAngle(f32),
    EArrowSeg(f32), EArrowGap(f32), EArrowAngle(f32),
    EDotGap(f32), EDotRadius(f32),
    EDdDash(f32), EDdGap(f32), EDdDotR(f32),
    // Edge editor - color picker
    EColorOpen(EColorField),
    EColorSubmit(EColorField, Color),
    EColorCancel(EColorField),
    // Edge editor - visibility & misc
    EStrokeVis(bool), EBorderVis(bool), EShadowVis(bool),
    EBorderGap(f32), EBorderThick(f32),
    EShadowExpand(f32), EOutlineThick(f32),
    EEdgeCount(u32),
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
            // DF views
            Msg::DfParam(idx, field, val) => {
                if let Some(v) = self.df_views.get_mut(idx) { v.editor.set_field(field, val); }
            }
            // Edge editor - pattern
            Msg::EPattern(p) => self.edge_ed.pattern = p,
            Msg::EThick(v) => self.edge_ed.thickness = v,
            Msg::EFlow(v) => self.edge_ed.flow_speed = v,
            // Dashed
            Msg::EDash(v) => self.edge_ed.dash = v,
            Msg::EDashGap(v) => self.edge_ed.dash_gap = v,
            Msg::EDashAngle(v) => self.edge_ed.dash_angle = v,
            // Arrowed
            Msg::EArrowSeg(v) => self.edge_ed.arrow_segment = v,
            Msg::EArrowGap(v) => self.edge_ed.arrow_gap = v,
            Msg::EArrowAngle(v) => self.edge_ed.arrow_angle = v,
            // Dotted
            Msg::EDotGap(v) => self.edge_ed.dot_gap = v,
            Msg::EDotRadius(v) => self.edge_ed.dot_radius = v,
            // DashDotted
            Msg::EDdDash(v) => self.edge_ed.dd_dash = v,
            Msg::EDdGap(v) => self.edge_ed.dd_gap = v,
            Msg::EDdDotR(v) => self.edge_ed.dd_dot_radius = v,
            // Color pickers
            Msg::EColorOpen(f) => { let s = &mut self.edge_ed.open_pickers; if !s.remove(&f) { s.insert(f); } }
            Msg::EColorSubmit(f, c) => { self.edge_ed.set_color(f, c); self.edge_ed.open_pickers.remove(&f); }
            Msg::EColorCancel(f) => { self.edge_ed.open_pickers.remove(&f); }
            // Visibility & misc
            Msg::EStrokeVis(v) => self.edge_ed.stroke_visible = v,
            Msg::EBorderVis(v) => self.edge_ed.border_visible = v,
            Msg::EShadowVis(v) => self.edge_ed.shadow_visible = v,
            Msg::EBorderGap(v) => self.edge_ed.border_gap = v,
            Msg::EBorderThick(v) => self.edge_ed.border_thickness = v,
            Msg::EShadowExpand(v) => self.edge_ed.shadow_expand = v,
            Msg::EOutlineThick(v) => self.edge_ed.stroke_outline_thickness = v,
            Msg::EEdgeCount(v) => self.edge_ed.edge_count = v,
            // Node editor
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
            _ => false,
        };
        if animated { iced::window::frames().map(|_| Msg::Tick) } else { Subscription::none() }
    }

    fn view(&self) -> Element<'_, Msg> {
        let mut sidebar = column![].spacing(4).padding(8).width(160);
        for (i, v) in self.df_views.iter().enumerate() {
            sidebar = sidebar.push(
                button(text(v.name).size(13)).width(Fill)
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
                if let Some(v) = self.df_views.get(i) {
                    let drawable = v.editor.build_drawable();
                    let style = v.build_style();
                    let settings = scrollable(column![
                        text(v.name).size(14),
                    ].push(v.editor.view_sliders(i)).spacing(3).padding(8).width(200));
                    let canvas = SdfCanvasOwned {
                        drawables: vec![drawable], styles: vec![style],
                        extent: v.extent, debug_tiles: self.debug_tiles, time: self.time,
                    };
                    row![
                        container(settings).height(Fill),
                        container(canvas).width(Fill).height(Fill),
                    ].into()
                } else { text("?").into() }
            }
        };

        row![container(sidebar).height(Fill), main].into()
    }

    fn view_edge_editor(&self) -> Element<'_, Msg> {
        let ed = &self.edge_ed;

        // Column 1: Layers + Edge Count
        let col_layers = scrollable(column![
            text("Layers").size(14),
            checkbox(ed.stroke_visible).label("Stroke").on_toggle(Msg::EStrokeVis).size(13),
            checkbox(ed.border_visible).label("Border").on_toggle(Msg::EBorderVis).size(13),
            checkbox(ed.shadow_visible).label("Shadow").on_toggle(Msg::EShadowVis).size(13),
            iced::widget::Space::new().height(8),
            text("Edges").size(14),
            row![
                text(format!("{}", ed.edge_count)).size(11).width(36),
                slider(2.0..=500.0_f32, ed.edge_count as f32, |v| Msg::EEdgeCount(v as u32)).step(1.0),
            ].spacing(4),
        ].spacing(4).padding(8).width(160));

        // Column 2: Stroke + Pattern
        let mut col_stroke = column![
            text("Stroke").size(14),
            text("Pattern").size(12),
            pick_list(PatternKind::ALL, Some(ed.pattern), Msg::EPattern).width(Fill),
        ].spacing(3);
        if ed.pattern != PatternKind::Dotted {
            col_stroke = col_stroke.push(labeled_slider("Thickness", 0.1, 20.0, ed.thickness, Msg::EThick));
        }
        match ed.pattern {
            PatternKind::Solid => {}
            PatternKind::Dashed => {
                col_stroke = col_stroke
                    .push(labeled_slider("Dash", 0.1, 50.0, ed.dash, Msg::EDash))
                    .push(labeled_slider("Gap", 0.1, 50.0, ed.dash_gap, Msg::EDashGap))
                    .push(labeled_slider("Angle", -90.0, 90.0, ed.dash_angle, Msg::EDashAngle));
            }
            PatternKind::Arrowed => {
                col_stroke = col_stroke
                    .push(labeled_slider("Segment", 0.1, 50.0, ed.arrow_segment, Msg::EArrowSeg))
                    .push(labeled_slider("Gap", 0.1, 50.0, ed.arrow_gap, Msg::EArrowGap))
                    .push(labeled_slider("Angle", -90.0, 90.0, ed.arrow_angle, Msg::EArrowAngle));
            }
            PatternKind::Dotted => {
                col_stroke = col_stroke
                    .push(labeled_slider("Gap", 0.1, 50.0, ed.dot_gap, Msg::EDotGap))
                    .push(labeled_slider("Radius", 0.1, 50.0, ed.dot_radius, Msg::EDotRadius));
            }
            PatternKind::DashDotted => {
                col_stroke = col_stroke
                    .push(labeled_slider("Dash", 0.1, 50.0, ed.dd_dash, Msg::EDdDash))
                    .push(labeled_slider("Gap", 0.1, 50.0, ed.dd_gap, Msg::EDdGap))
                    .push(labeled_slider("Dot r", 0.1, 50.0, ed.dd_dot_radius, Msg::EDdDotR));
            }
        }
        let op = &ed.open_pickers;
        col_stroke = col_stroke
            .push(color_swatch("Start", ed.stroke_color, EColorField::Stroke, op.contains(&EColorField::Stroke)))
            .push(color_swatch("End", ed.stroke_color_end, EColorField::StrokeEnd, op.contains(&EColorField::StrokeEnd)))
            .push(labeled_slider("Outline", 0.0, 20.0, ed.stroke_outline_thickness, Msg::EOutlineThick))
            .push(color_swatch("Outline", ed.stroke_outline_color, EColorField::StrokeOutline, op.contains(&EColorField::StrokeOutline)));
        if ed.pattern != PatternKind::Solid {
            col_stroke = col_stroke.push(labeled_slider("Flow", -10.0, 10.0, ed.flow_speed, Msg::EFlow));
        }
        let col_stroke = scrollable(col_stroke.padding(8).width(Fill));

        // Column 3: Border + Shadow
        let col_common = scrollable(column![
            text("Border").size(14),
            labeled_slider("Gap", 0.0, 20.0, ed.border_gap, Msg::EBorderGap),
            labeled_slider("Thickness", 0.0, 20.0, ed.border_thickness, Msg::EBorderThick),
            color_swatch("Start", ed.border_color, EColorField::Border, op.contains(&EColorField::Border)),
            color_swatch("End", ed.border_color_end, EColorField::BorderEnd, op.contains(&EColorField::BorderEnd)),
            iced::widget::Space::new().height(8),
            text("Shadow").size(14),
            labeled_slider("Expand", 0.0, 50.0, ed.shadow_expand, Msg::EShadowExpand),
            color_swatch("Color", ed.shadow_color, EColorField::Shadow, op.contains(&EColorField::Shadow)),
            color_swatch("End", ed.shadow_color_end, EColorField::ShadowEnd, op.contains(&EColorField::ShadowEnd)),
        ].spacing(3).padding(8).width(Fill));

        let controls = row![col_layers, col_stroke, col_common].spacing(4);

        // Build canvas with multi-edge rendering
        let styles = ed.build_styles();
        let count = (ed.edge_count as usize).min(ed.edges.len());
        let canvas = SdfEdgeCanvas {
            edges: &ed.edges, edge_count: count, styles,
            extent: 160.0, debug_tiles: self.debug_tiles, time: self.time,
        };
        column![controls, container(canvas).width(Fill).height(Fill)].into()
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

fn color_swatch<'a>(label: &'a str, c: [f32; 4], field: EColorField, open: bool) -> Element<'a, Msg> {
    let color = Color::from_rgba(c[0], c[1], c[2], c[3]);
    let swatch_btn = button(
        container(text("").size(1))
            .width(16).height(16)
            .style(move |_: &Theme| container::Style {
                background: Some(iced::Background::Color(color)),
                border: iced::Border { color: Color::from_rgb(0.5, 0.5, 0.5), width: 1.0, radius: 2.0.into() },
                ..Default::default()
            })
    )
    .on_press(Msg::EColorOpen(field))
    .padding(0)
    .style(button::text);

    iced_aw::ColorPicker::new(
        open, color,
        row![text(label).size(11).width(50), swatch_btn].spacing(4),
        Msg::EColorCancel(field),
        move |c| Msg::EColorSubmit(field, c),
    ).into()
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

// --- SdfEdgeCanvas (multi-edge: each style applied to all edges) ---

struct SdfEdgeCanvas<'a> {
    edges: &'a [Drawable],
    edge_count: usize,
    styles: Vec<Style>,
    extent: f32,
    debug_tiles: bool,
    time: f32,
}

impl<'a, Message, R> iced::advanced::Widget<Message, Theme, R> for SdfEdgeCanvas<'a>
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
        for style in &self.styles {
            for edge in &self.edges[..self.edge_count] {
                prim.push(edge, style, sb);
            }
        }
        renderer.draw_primitive(b, prim.camera(b.width * 0.5 / z, b.height * 0.5 / z, z).time(self.time).debug_tiles(self.debug_tiles));
    }
}

impl<'a, M: 'a, R: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a> From<SdfEdgeCanvas<'a>> for Element<'a, M, Theme, R> {
    fn from(c: SdfEdgeCanvas<'a>) -> Self { Element::new(c) }
}
