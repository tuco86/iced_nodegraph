//! SDF Gallery - Interactive showcase of 2D SDF primitives.
//!
//! Browse through SDF shapes from Inigo Quilez's 2D distance functions
//! library, rendered in real-time via iced_sdf.
//!
//! ## Interactive Demo
//!
//! <link rel="stylesheet" href="pkg/demo.css">
//! <div id="demo-container">
//!   <div id="demo-loading">
//!     <div class="demo-spinner"></div>
//!     <p>Loading demo...</p>
//!   </div>
//!   <div id="demo-canvas-container"></div>
//!   <div id="demo-error">
//!     <strong>Failed to load demo.</strong> WebGPU required.
//!   </div>
//! </div>
//! <script type="module" src="pkg/demo-loader.js"></script>
//!
//! ## Usage
//!
//! - Click shapes in the sidebar to preview them
//! - URL params: `?shape=<slug>` selects initial shape, `?embed=true` hides sidebar

mod shapes;
mod widget;

use std::collections::HashSet;

use iced::widget::{button, column, container, pick_list, row, scrollable, slider, text, toggler};
use iced::window;
use iced::{Center, Color, Element, Fill, Subscription, Theme};
use iced_sdf::{Layer, Pattern};
use web_time::Instant;

#[cfg(not(target_arch = "wasm32"))]
use demo_common::{ScreenshotHelper, ScreenshotMessage};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

pub fn main_with_target(target: String, shape: Option<String>, embed: bool) -> iced::Result {
    let mut selected = 0usize;
    if let Some(slug) = shape {
        let entries = shapes::all_shapes();
        if let Some(idx) = entries.iter().position(|e| e.slug == slug) {
            selected = idx;
        }
    }

    #[cfg(target_arch = "wasm32")]
    let window_settings = iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            target: Some(target),
        },
        ..Default::default()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let window_settings = {
        let _ = target;
        iced::window::Settings::default()
    };

    let init_selected = selected;
    let init_embed = embed;

    iced::application(
        move || App::new(init_selected, init_embed),
        App::update,
        App::view,
    )
    .title("SDF Gallery - iced_sdf")
    .theme(App::theme)
    .subscription(App::subscription)
    .window(window_settings)
    .antialiasing(true)
    .run()
}

pub fn main() -> iced::Result {
    #[allow(unused_mut, unused_assignments)]
    let mut shape = None;
    #[allow(unused_mut, unused_assignments)]
    let mut embed = false;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--shape" => {
                    shape = args.next();
                }
                "--list-shapes" => {
                    let entries = shapes::all_shapes();
                    for entry in &entries {
                        println!("{}", entry.slug);
                    }
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().unwrap();
        let search = window.location().search().unwrap_or_default();
        let params = web_sys::UrlSearchParams::new_with_str(&search).unwrap();
        shape = params.get("shape");
        embed = params.get("embed").map_or(false, |v| v == "true");
    }

    main_with_target("demo-canvas-container".into(), shape, embed)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_demo() {
    let _ = main();
}

/// Launch an embedded instance targeting a specific DOM element with a fixed shape.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_demo_in(target: &str, shape: &str) {
    let _ = main_with_target(target.into(), Some(shape.into()), true);
}

// ---------------------------------------------------------------------------
// Edge editor types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    Solid,
    Dashed,
    DashCapped,
    Arrowed,
    Dotted,
    DashDotted,
}

impl PatternKind {
    const ALL: &'static [PatternKind] = &[
        Self::Solid,
        Self::Dashed,
        Self::DashCapped,
        Self::Arrowed,
        Self::Dotted,
        Self::DashDotted,
    ];

    fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "edge_editor" => Some(Self::Solid),
            _ => None,
        }
    }
}

impl std::fmt::Display for PatternKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solid => write!(f, "Solid"),
            Self::Dashed => write!(f, "Dashed"),
            Self::DashCapped => write!(f, "DashCapped"),
            Self::Arrowed => write!(f, "Arrowed"),
            Self::Dotted => write!(f, "Dotted"),
            Self::DashDotted => write!(f, "DashDotted"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FloatParam {
    StrokeThickness,
    StrokeOutlineThickness,
    DashLength,
    DashGap,
    DashAngle,
    ArrowSegment,
    ArrowGap,
    ArrowAngle,
    DotGap,
    DotRadius,
    DdDash,
    DdGap,
    DdDotRadius,
    BorderGap,
    BorderThickness,
    BorderOutlineThickness,
    ShadowOffsetX,
    ShadowOffsetY,
    ShadowDistance,
    FlowSpeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ColorParam {
    StrokeColor,
    StrokeColorEnd,
    StrokeOutlineColor,
    BorderColor,
    BorderColorEnd,
    BorderBackground,
    BorderBackgroundEnd,
    BorderOutlineColor,
    ShadowColor,
    ShadowColorEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Shadow,
    Border,
    Stroke,
}

// ---------------------------------------------------------------------------
// Node editor types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum NFloatParam {
    CornerRadius,
    Opacity,
    BorderWidth,
    BorderOutlineWidth,
    BorderDashLen,
    BorderDashGap,
    ShadowOffsetX,
    ShadowOffsetY,
    ShadowBlur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NColorParam {
    Fill,
    Border,
    BorderOutline,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NLayerKind {
    Shadow,
    Fill,
    Border,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeBorderPattern {
    Solid,
    Dashed,
}

impl NodeBorderPattern {
    const ALL: &'static [Self] = &[Self::Solid, Self::Dashed];
}

impl std::fmt::Display for NodeBorderPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solid => write!(f, "Solid"),
            Self::Dashed => write!(f, "Dashed"),
        }
    }
}

struct RandomNodeData {
    center: [f32; 2],
    half_size: [f32; 2],
}

fn generate_random_node_data(count: usize) -> Vec<RandomNodeData> {
    let mut nodes = Vec::with_capacity(count);
    for i in 0..count {
        let seed = (i + 13) as f32;
        let cx = ((seed * 137.3) % 400.0) - 200.0;
        let cy = ((seed * 89.7) % 300.0) - 150.0;
        let hw = 40.0 + (seed * 31.1) % 80.0;
        let hh = 25.0 + (seed * 19.3) % 55.0;
        nodes.push(RandomNodeData {
            center: [cx, cy],
            half_size: [hw, hh],
        });
    }
    nodes
}

struct NodeEditorState {
    expanded_colors: HashSet<NColorParam>,

    shadow_visible: bool,
    fill_visible: bool,
    border_visible: bool,

    shadow_debug: bool,
    fill_debug: bool,
    border_debug: bool,

    node_count: u32,
    random_nodes: Vec<RandomNodeData>,

    fill_color: [f32; 4],
    corner_radius: f32,
    opacity: f32,

    border_pattern: NodeBorderPattern,
    border_width: f32,
    border_color: [f32; 4],
    border_outline_width: f32,
    border_outline_color: [f32; 4],
    border_dash_len: f32,
    border_dash_gap: f32,

    shadow_offset_x: f32,
    shadow_offset_y: f32,
    shadow_blur: f32,
    shadow_color: [f32; 4],
}

impl NodeEditorState {
    fn new() -> Self {
        Self {
            expanded_colors: HashSet::new(),

            shadow_visible: true,
            fill_visible: true,
            border_visible: true,

            shadow_debug: false,
            fill_debug: false,
            border_debug: false,

            node_count: 1,
            random_nodes: generate_random_node_data(99),

            fill_color: [0.14, 0.14, 0.16, 1.0],
            corner_radius: 8.0,
            opacity: 0.75,

            border_pattern: NodeBorderPattern::Solid,
            border_width: 1.0,
            border_color: [0.30, 0.30, 0.35, 1.0],
            border_outline_width: 0.0,
            border_outline_color: [0.05, 0.05, 0.15, 1.0],
            border_dash_len: 10.0,
            border_dash_gap: 6.0,

            shadow_offset_x: 4.0,
            shadow_offset_y: 4.0,
            shadow_blur: 8.0,
            shadow_color: [0.0, 0.0, 0.0, 0.3],
        }
    }

    fn build_shape(&self) -> iced_sdf::Sdf {
        use iced_sdf::Sdf;
        let node = Sdf::rounded_box([0.0, 0.0], [120.0, 80.0], self.corner_radius);
        let pin_r = 5.0;
        let x = 120.0; // half_size.x
        // 3 input pins on left, 2 output pins on right
        let pins = Sdf::circle([-x, -25.0], pin_r)
            | Sdf::circle([-x, 0.0], pin_r)
            | Sdf::circle([-x, 25.0], pin_r)
            | Sdf::circle([x, -15.0], pin_r)
            | Sdf::circle([x, 15.0], pin_r);
        node - pins
    }

    fn extra_shapes(&self) -> Vec<iced_sdf::Sdf> {
        let count = (self.node_count as usize).saturating_sub(1).min(self.random_nodes.len());
        self.random_nodes[..count]
            .iter()
            .map(|n| iced_sdf::Sdf::rounded_box(n.center, n.half_size, self.corner_radius))
            .collect()
    }

    fn build_layer_groups(&self) -> Vec<(Vec<Layer>, bool)> {
        let mut groups = Vec::new();

        // Shadow
        if self.shadow_visible && (self.shadow_blur > 0.01 || self.shadow_color[3] > 0.001) {
            let mut layer = Layer::solid(color_from(self.shadow_color))
                .expand(self.shadow_blur * 0.5)
                .blur(self.shadow_blur);
            if self.shadow_offset_x.abs() > 0.001 || self.shadow_offset_y.abs() > 0.001 {
                layer = layer.offset(self.shadow_offset_x, self.shadow_offset_y);
            }
            groups.push((vec![layer], self.shadow_debug));
        }

        // Fill
        if self.fill_visible {
            let c = Color::from_rgba(
                self.fill_color[0],
                self.fill_color[1],
                self.fill_color[2],
                self.fill_color[3] * self.opacity,
            );
            groups.push((vec![Layer::solid(c)], self.fill_debug));
        }

        // Border
        if self.border_visible && self.border_width > 0.01 {
            let pattern = match self.border_pattern {
                NodeBorderPattern::Solid => Pattern::solid(self.border_width),
                NodeBorderPattern::Dashed => Pattern::dashed(
                    self.border_width,
                    self.border_dash_len,
                    self.border_dash_gap,
                ),
            };
            let mut border = Layer::stroke(color_from(self.border_color), pattern);
            if self.border_outline_width > 0.01 {
                border = border.outline(
                    self.border_outline_width,
                    color_from(self.border_outline_color),
                );
            }
            groups.push((vec![border], self.border_debug));
        }

        groups
    }

    fn set_float(&mut self, param: NFloatParam, value: f32) {
        match param {
            NFloatParam::CornerRadius => self.corner_radius = value,
            NFloatParam::Opacity => self.opacity = value,
            NFloatParam::BorderWidth => self.border_width = value,
            NFloatParam::BorderOutlineWidth => self.border_outline_width = value,
            NFloatParam::BorderDashLen => self.border_dash_len = value,
            NFloatParam::BorderDashGap => self.border_dash_gap = value,
            NFloatParam::ShadowOffsetX => self.shadow_offset_x = value,
            NFloatParam::ShadowOffsetY => self.shadow_offset_y = value,
            NFloatParam::ShadowBlur => self.shadow_blur = value,
        }
    }

    fn set_color_channel(&mut self, param: NColorParam, channel: usize, value: f32) {
        let c = match param {
            NColorParam::Fill => &mut self.fill_color,
            NColorParam::Border => &mut self.border_color,
            NColorParam::BorderOutline => &mut self.border_outline_color,
            NColorParam::Shadow => &mut self.shadow_color,
        };
        if channel < 4 {
            c[channel] = value;
        }
    }
}

struct EdgeEditorState {
    // Which color editors are expanded (collapsed by default)
    expanded_colors: HashSet<ColorParam>,

    // Layer visibility
    shadow_visible: bool,
    border_visible: bool,
    stroke_visible: bool,

    // Per-layer debug
    shadow_debug: bool,
    border_debug: bool,
    stroke_debug: bool,

    // Extra edges
    edge_count: u32,
    extra_edges: Vec<iced_sdf::Sdf>,

    // Pattern selection
    pattern_kind: PatternKind,

    // Stroke
    stroke_thickness: f32,
    stroke_color: [f32; 4],
    stroke_outline_thickness: f32,
    stroke_outline_color: [f32; 4],

    // Dashed params (also used by DashCapped)
    dash_length: f32,
    dash_gap: f32,
    dash_angle: f32, // degrees

    // Arrowed params
    arrow_segment: f32,
    arrow_gap: f32,
    arrow_angle: f32, // degrees

    // Dotted params
    dot_gap: f32,
    dot_radius: f32,

    // DashDotted params
    dd_dash: f32,
    dd_gap: f32,
    dd_dot_radius: f32,

    stroke_color_end: [f32; 4],

    // Border
    border_gap: f32,
    border_thickness: f32,
    border_color: [f32; 4],
    border_background: [f32; 4],
    border_background_end: [f32; 4],
    border_outline_thickness: f32,
    border_outline_color: [f32; 4],

    border_color_end: [f32; 4],

    // Shadow
    shadow_offset_x: f32,
    shadow_offset_y: f32,
    shadow_distance: f32,
    shadow_color: [f32; 4],
    shadow_color_end: [f32; 4],

    // Flow
    flow_speed: f32,
}

fn generate_random_edges(count: usize) -> Vec<iced_sdf::Sdf> {
    let mut edges = Vec::with_capacity(count);
    for i in 0..count {
        // Simple deterministic pseudo-random based on index
        let seed = (i + 7) as f32;
        let x0 = ((seed * 131.7) % 400.0) - 200.0;
        let y0 = ((seed * 97.3) % 300.0) - 150.0;
        let x1 = ((seed * 173.1) % 400.0) - 200.0;
        let y1 = ((seed * 59.9) % 300.0) - 150.0;
        let offset = 40.0 + (seed * 23.7) % 60.0;
        let fwd = iced_sdf::Sdf::bezier(
            [x0, y0],
            [x0 + offset, y0],
            [x1 - offset, y1],
            [x1, y1],
        );
        edges.push(fwd);
    }
    edges
}

impl EdgeEditorState {
    fn new(kind: PatternKind) -> Self {
        let extra_edges = generate_random_edges(998);
        Self {
            expanded_colors: HashSet::new(),

            shadow_visible: true,
            border_visible: true,
            stroke_visible: true,

            shadow_debug: false,
            border_debug: false,
            stroke_debug: false,

            edge_count: 2,
            extra_edges,

            pattern_kind: kind,

            stroke_thickness: 6.0,
            stroke_color: [0.2, 0.85, 1.0, 1.0],
            stroke_outline_thickness: 1.2,
            stroke_outline_color: [0.05, 0.05, 0.15, 1.0],

            stroke_color_end: [0.6, 0.2, 1.0, 1.0],

            dash_length: 14.0,
            dash_gap: 8.0,
            dash_angle: 0.0,

            arrow_segment: 10.0,
            arrow_gap: 8.0,
            arrow_angle: 45.0,

            dot_gap: 6.0,
            dot_radius: 4.0,

            dd_dash: 14.0,
            dd_gap: 6.0,
            dd_dot_radius: 3.0,

            border_gap: 2.0,
            border_thickness: 3.0,
            border_color: [0.95, 0.75, 0.2, 1.0],
            border_background: [0.08, 0.06, 0.18, 0.5],
            border_background_end: [0.18, 0.06, 0.08, 0.5],
            border_outline_thickness: 0.8,
            border_outline_color: [0.05, 0.05, 0.15, 1.0],

            border_color_end: [1.0, 0.3, 0.2, 1.0],

            shadow_offset_x: 3.0,
            shadow_offset_y: 3.0,
            shadow_distance: 10.0,
            shadow_color: [0.0, 0.0, 0.1, 0.35],
            shadow_color_end: [0.0, 0.0, 0.1, 0.0],

            flow_speed: 0.0,
        }
    }

    fn build_pattern(&self) -> Pattern {
        let deg2rad = std::f32::consts::PI / 180.0;
        let p = match self.pattern_kind {
            PatternKind::Solid => Pattern::solid(self.stroke_thickness),
            PatternKind::Dashed => {
                Pattern::dashed_angle(self.stroke_thickness, self.dash_length, self.dash_gap, self.dash_angle * deg2rad)
            }
            PatternKind::DashCapped => {
                Pattern::dash_capped_angle(self.stroke_thickness, self.dash_length, self.dash_gap, self.dash_angle * deg2rad)
            }
            PatternKind::Arrowed => {
                Pattern::arrowed(self.stroke_thickness, self.arrow_segment, self.arrow_gap, self.arrow_angle * deg2rad)
            }
            PatternKind::Dotted => Pattern::dotted(self.dot_gap + self.dot_radius * 2.0, self.dot_radius),
            PatternKind::DashDotted => {
                Pattern::dash_dotted(self.stroke_thickness, self.dd_dash, self.dd_gap, self.dd_dot_radius)
            }
        };
        if self.flow_speed.abs() > 0.01 {
            p.flow(self.flow_speed)
        } else {
            p
        }
    }

    /// Build layers split by group: (shadow, border_bg + border_stroke, stroke).
    /// Each group gets its own SdfPrimitive so debug_flags can be set independently.
    fn build_layer_groups(&self) -> Vec<(Vec<Layer>, bool)> {
        let arc_scale = 1.0 / approx_bezier_arc_length();
        let stroke_half = match self.pattern_kind {
            PatternKind::Dotted => self.dot_radius,
            PatternKind::DashDotted => (self.stroke_thickness * 0.5).max(self.dd_dot_radius),
            _ => self.stroke_thickness * 0.5,
        };
        let border_center = stroke_half + self.border_gap + self.border_thickness * 0.5;
        let border_outer = border_center + self.border_thickness * 0.5;
        let has_border = self.border_visible && self.border_thickness > 0.01;
        let exp = &self.expanded_colors;
        let _ = exp; // suppress unused warning

        let mut groups: Vec<(Vec<Layer>, bool)> = Vec::new();

        // Shadow group
        if self.shadow_visible
            && self.shadow_distance > 0.01
            && (self.shadow_color[3] > 0.001 || self.shadow_color_end[3] > 0.001)
        {
            groups.push((vec![
                Layer::gradient(
                    color_from(self.shadow_color),
                    color_from(self.shadow_color_end),
                    0.0,
                )
                .expand(border_outer + self.shadow_distance)
                .blur(self.shadow_distance)
                .offset(self.shadow_offset_x, self.shadow_offset_y),
            ], self.shadow_debug));
        }

        // Border group (background + stroke)
        if self.border_visible {
            let mut border_layers = Vec::new();
            if self.border_background[3] > 0.001 || self.border_background_end[3] > 0.001 {
                border_layers.push(
                    Layer::solid(color_from(self.border_background))
                        .expand(border_outer)
                        .gradient_color(color_from(self.border_background_end))
                        .gradient_along_u(true)
                        .gradient_scale(arc_scale),
                );
            }
            if has_border {
                let mut border = Layer::stroke(
                    color_from(self.border_color),
                    Pattern::solid(self.border_thickness),
                )
                .expand(border_center)
                .gradient_color(color_from(self.border_color_end))
                .gradient_along_u(true)
                .gradient_scale(arc_scale);
                if self.border_outline_thickness > 0.01 {
                    border = border.outline(self.border_outline_thickness, color_from(self.border_outline_color));
                }
                border_layers.push(border);
            }
            if !border_layers.is_empty() {
                groups.push((border_layers, self.border_debug));
            }
        }

        // Stroke group
        if self.stroke_visible {
            let mut stroke = Layer::stroke(color_from(self.stroke_color), self.build_pattern())
                .gradient_color(color_from(self.stroke_color_end))
                .gradient_along_u(true)
                .gradient_scale(arc_scale);
            if self.stroke_outline_thickness > 0.01 {
                stroke = stroke.outline(self.stroke_outline_thickness, color_from(self.stroke_outline_color));
            }
            groups.push((vec![stroke], self.stroke_debug));
        }

        groups
    }

    fn set_float(&mut self, param: FloatParam, value: f32) {
        match param {
            FloatParam::StrokeThickness => self.stroke_thickness = value,
            FloatParam::StrokeOutlineThickness => self.stroke_outline_thickness = value,
            FloatParam::DashLength => self.dash_length = value,
            FloatParam::DashGap => self.dash_gap = value,
            FloatParam::DashAngle => self.dash_angle = value,
            FloatParam::ArrowSegment => self.arrow_segment = value,
            FloatParam::ArrowGap => self.arrow_gap = value,
            FloatParam::ArrowAngle => self.arrow_angle = value,
            FloatParam::DotGap => self.dot_gap = value,
            FloatParam::DotRadius => self.dot_radius = value,
            FloatParam::DdDash => self.dd_dash = value,
            FloatParam::DdGap => self.dd_gap = value,
            FloatParam::DdDotRadius => self.dd_dot_radius = value,
            FloatParam::BorderGap => self.border_gap = value,
            FloatParam::BorderThickness => self.border_thickness = value,
            FloatParam::BorderOutlineThickness => self.border_outline_thickness = value,
            FloatParam::ShadowOffsetX => self.shadow_offset_x = value,
            FloatParam::ShadowOffsetY => self.shadow_offset_y = value,
            FloatParam::ShadowDistance => self.shadow_distance = value,
            FloatParam::FlowSpeed => self.flow_speed = value,
        }
    }

    fn set_color_channel(&mut self, param: ColorParam, channel: usize, value: f32) {
        let c = match param {
            ColorParam::StrokeColor => &mut self.stroke_color,
            ColorParam::StrokeColorEnd => &mut self.stroke_color_end,
            ColorParam::StrokeOutlineColor => &mut self.stroke_outline_color,
            ColorParam::BorderColor => &mut self.border_color,
            ColorParam::BorderColorEnd => &mut self.border_color_end,
            ColorParam::BorderBackground => &mut self.border_background,
            ColorParam::BorderBackgroundEnd => &mut self.border_background_end,
            ColorParam::BorderOutlineColor => &mut self.border_outline_color,
            ColorParam::ShadowColor => &mut self.shadow_color,
            ColorParam::ShadowColorEnd => &mut self.shadow_color_end,
        };
        if channel < 4 {
            c[channel] = value;
        }
    }
}

fn color_from(rgba: [f32; 4]) -> Color {
    Color::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Approximate arc-length of the fixed edge bezier curve via subdivision.
/// Control points: [-120,-40], [-40,-40], [40,40], [120,40].
fn approx_bezier_arc_length() -> f32 {
    let p0 = (-120.0_f32, -40.0_f32);
    let p1 = (-40.0, -40.0);
    let p2 = (40.0, 40.0);
    let p3 = (120.0, 40.0);
    let steps = 64;
    let mut length = 0.0_f32;
    let mut prev = p0;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let it = 1.0 - t;
        let x = it * it * it * p0.0 + 3.0 * it * it * t * p1.0 + 3.0 * it * t * t * p2.0 + t * t * t * p3.0;
        let y = it * it * it * p0.1 + 3.0 * it * it * t * p1.1 + 3.0 * it * t * t * p2.1 + t * t * t * p3.1;
        let dx = x - prev.0;
        let dy = y - prev.1;
        length += (dx * dx + dy * dy).sqrt();
        prev = (x, y);
    }
    length
}

enum EditorKind {
    Edge(EdgeEditorState),
    Node(NodeEditorState),
}

fn editor_for_selected(selected: usize) -> Option<EditorKind> {
    let entries = shapes::all_shapes();
    let entry = entries.get(selected)?;
    if let Some(kind) = PatternKind::from_slug(entry.slug) {
        return Some(EditorKind::Edge(EdgeEditorState::new(kind)));
    }
    if entry.slug == "node_editor" {
        return Some(EditorKind::Node(NodeEditorState::new()));
    }
    None
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct App {
    selected: usize,
    embed: bool,
    start_time: Instant,
    editor: Option<EditorKind>,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: ScreenshotHelper,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Tick,
    SetPatternKind(PatternKind),
    SetFloat(FloatParam, f32),
    SetColorChannel(ColorParam, usize, f32),
    ToggleLayer(LayerKind, bool),
    ToggleColorEditor(ColorParam),
    ToggleDebugLayer(LayerKind, bool),
    SetEdgeCount(u32),
    SetNFloat(NFloatParam, f32),
    SetNColorChannel(NColorParam, usize, f32),
    ToggleNLayer(NLayerKind, bool),
    ToggleNColorEditor(NColorParam),
    ToggleNDebugLayer(NLayerKind, bool),
    SetNodeCount(u32),
    SetNodeBorderPattern(NodeBorderPattern),
    #[cfg(not(target_arch = "wasm32"))]
    Screenshot(demo_common::ScreenshotMessage),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ScreenshotMessage> for Message {
    fn from(msg: ScreenshotMessage) -> Self {
        Message::Screenshot(msg)
    }
}

impl App {
    fn new(selected: usize, embed: bool) -> (Self, iced::Task<Message>) {
        let editor = editor_for_selected(selected);
        (
            Self {
                selected,
                embed,
                start_time: Instant::now(),
                editor,
                #[cfg(not(target_arch = "wasm32"))]
                screenshot: ScreenshotHelper::from_args(),
            },
            iced::Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        #[cfg(not(target_arch = "wasm32"))]
        let screenshot_sub = self.screenshot.subscription().map(Message::Screenshot);
        #[cfg(target_arch = "wasm32")]
        let screenshot_sub = Subscription::none();

        if self.embed {
            // Embed mode: SdfCanvas widget drives redraws via shell.request_redraw()
            // when SDF animations are active. No continuous frame subscription needed.
            return Subscription::batch([screenshot_sub]);
        }

        // Full gallery: window::frames() for application-level animation
        // (time-varying shapes via build closures).
        Subscription::batch([
            window::frames().map(|_| Message::Tick),
            screenshot_sub,
        ])
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Select(idx) => {
                self.selected = idx;
                self.editor = editor_for_selected(idx);
            }
            Message::SetPatternKind(kind) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    e.pattern_kind = kind;
                }
            }
            Message::SetFloat(param, value) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    e.set_float(param, value);
                }
            }
            Message::SetColorChannel(param, ch, value) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    e.set_color_channel(param, ch, value);
                }
            }
            Message::ToggleLayer(kind, visible) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    match kind {
                        LayerKind::Shadow => e.shadow_visible = visible,
                        LayerKind::Border => e.border_visible = visible,
                        LayerKind::Stroke => e.stroke_visible = visible,
                    }
                }
            }
            Message::SetEdgeCount(count) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    e.edge_count = count;
                }
            }
            Message::ToggleDebugLayer(kind, enabled) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor {
                    match kind {
                        LayerKind::Shadow => e.shadow_debug = enabled,
                        LayerKind::Border => e.border_debug = enabled,
                        LayerKind::Stroke => e.stroke_debug = enabled,
                    }
                }
            }
            Message::ToggleColorEditor(param) => {
                if let Some(EditorKind::Edge(e)) = &mut self.editor
                    && !e.expanded_colors.remove(&param)
                {
                    e.expanded_colors.insert(param);
                }
            }
            Message::SetNFloat(param, value) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    n.set_float(param, value);
                }
            }
            Message::SetNColorChannel(param, ch, value) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    n.set_color_channel(param, ch, value);
                }
            }
            Message::ToggleNLayer(kind, visible) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    match kind {
                        NLayerKind::Shadow => n.shadow_visible = visible,
                        NLayerKind::Fill => n.fill_visible = visible,
                        NLayerKind::Border => n.border_visible = visible,
                    }
                }
            }
            Message::ToggleNDebugLayer(kind, enabled) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    match kind {
                        NLayerKind::Shadow => n.shadow_debug = enabled,
                        NLayerKind::Fill => n.fill_debug = enabled,
                        NLayerKind::Border => n.border_debug = enabled,
                    }
                }
            }
            Message::ToggleNColorEditor(param) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor
                    && !n.expanded_colors.remove(&param)
                {
                    n.expanded_colors.insert(param);
                }
            }
            Message::SetNodeCount(count) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    n.node_count = count;
                }
            }
            Message::SetNodeBorderPattern(pat) => {
                if let Some(EditorKind::Node(n)) = &mut self.editor {
                    n.border_pattern = pat;
                }
            }
            Message::Tick => {}
            #[cfg(not(target_arch = "wasm32"))]
            Message::Screenshot(msg) => return self.screenshot.update(msg),
        }
        iced::Task::none()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<'_, Message> {
        let entries = shapes::all_shapes();
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let entry = &entries[self.selected];

        // Embed mode: only the SDF canvas, no sidebar or text
        if self.embed {
            let sdf_view = widget::sdf_canvas(entry, elapsed, None, false, &[], None);
            return container(sdf_view).width(Fill).height(Fill).into();
        }

        // Sidebar with shape list
        let sidebar = {
            let mut items = column![].spacing(2).padding(8);

            for (i, entry) in entries.iter().enumerate() {
                let is_selected = i == self.selected;
                let label = text(entry.name).size(14);

                let btn = button(label)
                    .on_press(Message::Select(i))
                    .width(Fill)
                    .style(if is_selected {
                        button::primary
                    } else {
                        button::secondary
                    });

                items = items.push(btn);
            }

            container(scrollable(items).height(Fill))
                .width(200)
                .height(Fill)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(
                        0.12, 0.12, 0.15,
                    ))),
                    ..Default::default()
                })
        };

        // Main content area
        let canvas = {
            let title = text(entry.name).size(20);
            let description = text(entry.description).size(13);

            let (layer_groups, extra_shapes_vec, shape_override) = match &self.editor {
                Some(EditorKind::Edge(e)) => {
                    let extra_count = (e.edge_count as usize).saturating_sub(2).min(e.extra_edges.len());
                    (
                        Some(e.build_layer_groups()),
                        e.extra_edges[..extra_count].to_vec(),
                        None,
                    )
                }
                Some(EditorKind::Node(n)) => (
                    Some(n.build_layer_groups()),
                    n.extra_shapes(),
                    Some(n.build_shape()),
                ),
                None => (None, vec![], None),
            };
            let sdf_view = widget::sdf_canvas(entry, elapsed, layer_groups, false, &extra_shapes_vec, shape_override);

            let mut content = column![title, description]
                .spacing(8)
                .padding(16)
                .width(Fill)
                .height(Fill);

            match &self.editor {
                Some(EditorKind::Edge(editor)) => {
                    content = content.push(edge_editor_ui(editor));
                }
                Some(EditorKind::Node(editor)) => {
                    content = content.push(node_editor_ui(editor));
                }
                None => {}
            }

            content.push(sdf_view)
        };

        row![sidebar, canvas].into()
    }
}

// ---------------------------------------------------------------------------
// Edge editor UI (three-column layout)
// ---------------------------------------------------------------------------

fn edge_editor_ui(editor: &EdgeEditorState) -> Element<'static, Message> {
    let col_layers = layers_column(editor);
    let col_stroke = stroke_column(editor);
    let col_common = common_column(editor);

    row![
        scrollable(col_layers).height(220),
        scrollable(col_stroke).height(220),
        scrollable(col_common).height(220),
    ]
    .spacing(12)
    .into()
}

fn layers_column(editor: &EdgeEditorState) -> Element<'static, Message> {
    column![
        section_header("Layers"),
        layer_row("Shadow", editor.shadow_visible, LayerKind::Shadow, editor.shadow_debug),
        layer_row("Border", editor.border_visible, LayerKind::Border, editor.border_debug),
        layer_row("Stroke", editor.stroke_visible, LayerKind::Stroke, editor.stroke_debug),
        section_header("Edges"),
        row![
            text(format!("{}", editor.edge_count)).size(11).width(36),
            slider(2.0..=1000.0_f32, editor.edge_count as f32, |v| Message::SetEdgeCount(v as u32)).step(1.0),
        ]
        .spacing(4)
        .align_y(Center),
    ]
    .spacing(4)
    .width(180)
    .into()
}

fn layer_row(name: &str, visible: bool, kind: LayerKind, debug: bool) -> Element<'static, Message> {
    row![
        toggler(visible)
            .label(name.to_string())
            .on_toggle(move |v| Message::ToggleLayer(kind, v))
            .size(16)
            .text_size(12),
        toggler(debug)
            .label("Tiles")
            .on_toggle(move |v| Message::ToggleDebugLayer(kind, v))
            .size(14)
            .text_size(10),
    ]
    .spacing(8)
    .into()
}

fn stroke_column(editor: &EdgeEditorState) -> Element<'static, Message> {
    let mut col = column![section_header("Stroke")].spacing(3);

    // Pattern picker
    col = col.push(
        row![
            text("Pattern").size(12).width(70),
            pick_list(
                PatternKind::ALL,
                Some(editor.pattern_kind),
                Message::SetPatternKind,
            )
            .text_size(12),
        ]
        .spacing(4)
        .align_y(Center),
    );

    // Stroke thickness (not for Dotted - uses dot radius instead)
    if editor.pattern_kind != PatternKind::Dotted {
        col = col.push(float_slider("Thickness", FloatParam::StrokeThickness, 0.1, 20.0, 0.1, editor.stroke_thickness));
    }

    // Pattern-specific params
    match editor.pattern_kind {
        PatternKind::Solid => {}
        PatternKind::Dashed | PatternKind::DashCapped => {
            col = col
                .push(float_slider("Dash", FloatParam::DashLength, 0.1, 50.0, 0.1, editor.dash_length))
                .push(float_slider("Gap", FloatParam::DashGap, 0.1, 50.0, 0.1, editor.dash_gap))
                .push(float_slider("Angle", FloatParam::DashAngle, -90.0, 90.0, 1.0, editor.dash_angle));
        }
        PatternKind::Arrowed => {
            col = col
                .push(float_slider("Segment", FloatParam::ArrowSegment, 0.1, 50.0, 0.1, editor.arrow_segment))
                .push(float_slider("Gap", FloatParam::ArrowGap, 0.1, 50.0, 0.1, editor.arrow_gap))
                .push(float_slider("Angle", FloatParam::ArrowAngle, -90.0, 90.0, 1.0, editor.arrow_angle));
        }
        PatternKind::Dotted => {
            col = col
                .push(float_slider("Gap", FloatParam::DotGap, 0.1, 50.0, 0.1, editor.dot_gap))
                .push(float_slider("Radius", FloatParam::DotRadius, 0.1, 50.0, 0.1, editor.dot_radius));
        }
        PatternKind::DashDotted => {
            col = col
                .push(float_slider("Dash", FloatParam::DdDash, 0.1, 50.0, 0.1, editor.dd_dash))
                .push(float_slider("Gap", FloatParam::DdGap, 0.1, 50.0, 0.1, editor.dd_gap))
                .push(float_slider("Dot r", FloatParam::DdDotRadius, 0.1, 50.0, 0.1, editor.dd_dot_radius));
        }
    }

    // Stroke color (start → end gradient along curve) + outline
    let exp = &editor.expanded_colors;
    col = col
        .push(color_editor("Start", editor.stroke_color, ColorParam::StrokeColor, exp.contains(&ColorParam::StrokeColor)))
        .push(color_editor("End", editor.stroke_color_end, ColorParam::StrokeColorEnd, exp.contains(&ColorParam::StrokeColorEnd)))
        .push(float_slider("Outline", FloatParam::StrokeOutlineThickness, 0.0, 20.0, 0.1, editor.stroke_outline_thickness))
        .push(color_editor("Outline", editor.stroke_outline_color, ColorParam::StrokeOutlineColor, exp.contains(&ColorParam::StrokeOutlineColor)));

    if editor.pattern_kind != PatternKind::Solid {
        col = col.push(float_slider("Flow", FloatParam::FlowSpeed, -10.0, 10.0, 0.1, editor.flow_speed));
    }

    col.width(Fill).into()
}

fn common_column(editor: &EdgeEditorState) -> Element<'static, Message> {
    let mut col = column![section_header("Border")].spacing(3);
    let exp = &editor.expanded_colors;

    col = col
        .push(float_slider("Gap", FloatParam::BorderGap, 0.0, 20.0, 0.1, editor.border_gap))
        .push(float_slider("Thickness", FloatParam::BorderThickness, 0.0, 20.0, 0.1, editor.border_thickness))
        .push(color_editor("Start", editor.border_color, ColorParam::BorderColor, exp.contains(&ColorParam::BorderColor)))
        .push(color_editor("End", editor.border_color_end, ColorParam::BorderColorEnd, exp.contains(&ColorParam::BorderColorEnd)))
        .push(color_editor("Background start", editor.border_background, ColorParam::BorderBackground, exp.contains(&ColorParam::BorderBackground)))
        .push(color_editor("Background end", editor.border_background_end, ColorParam::BorderBackgroundEnd, exp.contains(&ColorParam::BorderBackgroundEnd)))
        .push(float_slider("Outline", FloatParam::BorderOutlineThickness, 0.0, 20.0, 0.1, editor.border_outline_thickness))
        .push(color_editor("Outline", editor.border_outline_color, ColorParam::BorderOutlineColor, exp.contains(&ColorParam::BorderOutlineColor)));

    col = col.push(section_header("Shadow"));
    col = col
        .push(float_slider("Offset X", FloatParam::ShadowOffsetX, -10.0, 10.0, 0.1, editor.shadow_offset_x))
        .push(float_slider("Offset Y", FloatParam::ShadowOffsetY, -10.0, 10.0, 0.1, editor.shadow_offset_y))
        .push(float_slider("Distance", FloatParam::ShadowDistance, 0.0, 50.0, 0.1, editor.shadow_distance))
        .push(color_editor("Start", editor.shadow_color, ColorParam::ShadowColor, exp.contains(&ColorParam::ShadowColor)))
        .push(color_editor("End", editor.shadow_color_end, ColorParam::ShadowColorEnd, exp.contains(&ColorParam::ShadowColorEnd)));

    col.width(Fill).into()
}

// ---------------------------------------------------------------------------
// Node editor UI (three-column layout)
// ---------------------------------------------------------------------------

fn node_editor_ui(editor: &NodeEditorState) -> Element<'static, Message> {
    row![
        scrollable(node_layers_column(editor)).height(220),
        scrollable(node_fill_column(editor)).height(220),
        scrollable(node_border_shadow_column(editor)).height(220),
    ]
    .spacing(12)
    .into()
}

fn node_layers_column(editor: &NodeEditorState) -> Element<'static, Message> {
    column![
        section_header("Layers"),
        node_layer_row("Shadow", editor.shadow_visible, NLayerKind::Shadow, editor.shadow_debug),
        node_layer_row("Fill", editor.fill_visible, NLayerKind::Fill, editor.fill_debug),
        node_layer_row("Border", editor.border_visible, NLayerKind::Border, editor.border_debug),
        section_header("Nodes"),
        row![
            text(format!("{}", editor.node_count)).size(11).width(36),
            slider(1.0..=100.0_f32, editor.node_count as f32, |v| Message::SetNodeCount(v as u32)).step(1.0),
        ]
        .spacing(4)
        .align_y(Center),
    ]
    .spacing(4)
    .width(180)
    .into()
}

fn node_layer_row(name: &str, visible: bool, kind: NLayerKind, debug: bool) -> Element<'static, Message> {
    row![
        toggler(visible)
            .label(name.to_string())
            .on_toggle(move |v| Message::ToggleNLayer(kind, v))
            .size(16)
            .text_size(12),
        toggler(debug)
            .label("Tiles")
            .on_toggle(move |v| Message::ToggleNDebugLayer(kind, v))
            .size(14)
            .text_size(10),
    ]
    .spacing(8)
    .into()
}

fn node_fill_column(editor: &NodeEditorState) -> Element<'static, Message> {
    let exp = &editor.expanded_colors;
    column![
        section_header("Fill"),
        ncolor_editor("Color", editor.fill_color, NColorParam::Fill, exp.contains(&NColorParam::Fill)),
        nfloat_slider("Radius", NFloatParam::CornerRadius, 0.0, 40.0, 0.5, editor.corner_radius),
        nfloat_slider("Opacity", NFloatParam::Opacity, 0.0, 1.0, 0.01, editor.opacity),
    ]
    .spacing(3)
    .width(Fill)
    .into()
}

fn node_border_shadow_column(editor: &NodeEditorState) -> Element<'static, Message> {
    let exp = &editor.expanded_colors;
    let mut col = column![section_header("Border")].spacing(3);

    col = col.push(
        row![
            text("Pattern").size(12).width(70),
            pick_list(
                NodeBorderPattern::ALL,
                Some(editor.border_pattern),
                Message::SetNodeBorderPattern,
            )
            .text_size(12),
        ]
        .spacing(4)
        .align_y(Center),
    );

    col = col
        .push(nfloat_slider("Width", NFloatParam::BorderWidth, 0.0, 10.0, 0.1, editor.border_width))
        .push(ncolor_editor("Color", editor.border_color, NColorParam::Border, exp.contains(&NColorParam::Border)));

    if editor.border_pattern == NodeBorderPattern::Dashed {
        col = col
            .push(nfloat_slider("Dash", NFloatParam::BorderDashLen, 1.0, 30.0, 0.5, editor.border_dash_len))
            .push(nfloat_slider("Gap", NFloatParam::BorderDashGap, 1.0, 20.0, 0.5, editor.border_dash_gap));
    }

    col = col
        .push(nfloat_slider("Outline", NFloatParam::BorderOutlineWidth, 0.0, 5.0, 0.1, editor.border_outline_width))
        .push(ncolor_editor("Outline", editor.border_outline_color, NColorParam::BorderOutline, exp.contains(&NColorParam::BorderOutline)));

    col = col.push(section_header("Shadow"));
    col = col
        .push(nfloat_slider("Offset X", NFloatParam::ShadowOffsetX, -20.0, 20.0, 0.5, editor.shadow_offset_x))
        .push(nfloat_slider("Offset Y", NFloatParam::ShadowOffsetY, -20.0, 20.0, 0.5, editor.shadow_offset_y))
        .push(nfloat_slider("Blur", NFloatParam::ShadowBlur, 0.0, 30.0, 0.5, editor.shadow_blur))
        .push(ncolor_editor("Color", editor.shadow_color, NColorParam::Shadow, exp.contains(&NColorParam::Shadow)));

    col.width(Fill).into()
}

fn nfloat_slider(
    label: &str,
    param: NFloatParam,
    min: f32,
    max: f32,
    step: f32,
    value: f32,
) -> Element<'static, Message> {
    let label = label.to_string();
    let value_text = format!("{value:.1}");
    row![
        text(label).size(11).width(60),
        slider(min..=max, value, move |v| Message::SetNFloat(param, v)).step(step),
        text(value_text).size(11).width(36),
    ]
    .spacing(4)
    .align_y(Center)
    .into()
}

fn ncolor_editor(
    label: &str,
    rgba: [f32; 4],
    param: NColorParam,
    expanded: bool,
) -> Element<'static, Message> {
    let swatch = button(
        container(text("").size(1))
            .width(16)
            .height(16)
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced::Background::Color(color_from(rgba))),
                border: iced::Border {
                    color: Color::from_rgb(0.5, 0.5, 0.5),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            }),
    )
    .on_press(Message::ToggleNColorEditor(param))
    .padding(0)
    .style(button::text);

    let header = row![
        text(label.to_string()).size(11).width(60),
        swatch,
    ]
    .spacing(4)
    .align_y(Center);

    if !expanded {
        return header.into();
    }

    let channel_labels = ["R", "G", "B", "A"];
    let mut sliders = column![].spacing(1);
    for (i, ch_label) in channel_labels.iter().enumerate() {
        let ch = i;
        let val = rgba[i];
        sliders = sliders.push(
            row![
                text(ch_label.to_string()).size(9).width(12),
                slider(0.0..=1.0_f32, val, move |v| Message::SetNColorChannel(param, ch, v)).step(0.01),
            ]
            .spacing(2)
            .align_y(Center),
        );
    }

    column![header, sliders].spacing(2).into()
}

// ---------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------

fn section_header(title: &str) -> Element<'static, Message> {
    text(title.to_string())
        .size(14)
        .color(Color::from_rgb(0.7, 0.7, 0.8))
        .into()
}

fn float_slider(
    label: &str,
    param: FloatParam,
    min: f32,
    max: f32,
    step: f32,
    value: f32,
) -> Element<'static, Message> {
    let label = label.to_string();
    let value_text = format!("{value:.1}");
    row![
        text(label).size(11).width(60),
        slider(min..=max, value, move |v| Message::SetFloat(param, v)).step(step),
        text(value_text).size(11).width(36),
    ]
    .spacing(4)
    .align_y(Center)
    .into()
}

fn color_editor(
    label: &str,
    rgba: [f32; 4],
    param: ColorParam,
    expanded: bool,
) -> Element<'static, Message> {
    let swatch = button(
        container(text("").size(1))
            .width(16)
            .height(16)
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced::Background::Color(color_from(rgba))),
                border: iced::Border {
                    color: Color::from_rgb(0.5, 0.5, 0.5),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            }),
    )
    .on_press(Message::ToggleColorEditor(param))
    .padding(0)
    .style(button::text);

    let header = row![
        text(label.to_string()).size(11).width(60),
        swatch,
    ]
    .spacing(4)
    .align_y(Center);

    if !expanded {
        return header.into();
    }

    let channel_labels = ["R", "G", "B", "A"];
    let mut sliders = column![].spacing(1);
    for (i, ch_label) in channel_labels.iter().enumerate() {
        let ch = i;
        let val = rgba[i];
        sliders = sliders.push(
            row![
                text(ch_label.to_string()).size(9).width(12),
                slider(0.0..=1.0_f32, val, move |v| Message::SetColorChannel(param, ch, v)).step(0.01),
            ]
            .spacing(2)
            .align_y(Center),
        );
    }

    column![header, sliders].spacing(2).into()
}
