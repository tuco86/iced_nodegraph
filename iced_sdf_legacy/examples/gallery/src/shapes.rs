//! SDF shape definitions for the gallery.
//!
//! Each entry defines a shape using iced_sdf primitives and CSG operations,
//! matching Inigo Quilez's 2D SDF library where available.

use iced::Color;
use iced_sdf::{Layer, Pattern, Sdf};

/// A gallery entry describing one SDF shape.
pub struct ShapeEntry {
    pub name: &'static str,
    pub description: &'static str,
    /// URL slug for deep-linking (e.g., `?shape=circle`).
    /// Used on wasm32 targets for URL parameter matching.
    #[allow(dead_code)]
    pub slug: &'static str,
    /// Builds the SDF shape. Receives time in seconds for animation.
    pub build: fn(f32) -> Sdf,
    /// Builds the rendering layers.
    pub layers: fn() -> Vec<Layer>,
    /// Approximate shape radius in world units (used for auto-zoom).
    pub extent: f32,
}

/// All available shapes in the gallery.
pub fn all_shapes() -> Vec<ShapeEntry> {
    vec![
        // ================================================================
        // Primitives
        // ================================================================
        ShapeEntry {
            name: "Circle",
            description: "sdCircle(p, r) - Basic circle with radius.",
            slug: "circle",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Box",
            description: "sdBox(p, b) - Axis-aligned rectangle.",
            slug: "box",
            build: |_t| Sdf::rect([0.0, 0.0], [100.0, 60.0]),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Rounded Box",
            description: "sdRoundBox(p, b, r) - Rectangle with rounded corners.",
            slug: "rounded_box",
            build: |_t| Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 16.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Segment",
            description: "sdSegment(p, a, b) - Line segment between two points.",
            slug: "segment",
            build: |_t| Sdf::line([-80.0, -40.0], [80.0, 40.0]),
            layers: stroke_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Bezier",
            description: "Cubic bezier curve with 4 control points.",
            slug: "bezier",
            build: |_t| {
                Sdf::bezier(
                    [-100.0, 50.0],
                    [-30.0, -80.0],
                    [30.0, 80.0],
                    [100.0, -50.0],
                )
            },
            layers: stroke_layers,
            extent: 110.0,
        },
        // ================================================================
        // CSG Operations
        // ================================================================
        ShapeEntry {
            name: "Union",
            description: "opUnion(a, b) - Combine two shapes (a | b).",
            slug: "union",
            build: |t| {
                let d = (t * 0.8).sin() * 30.0;
                Sdf::circle([-40.0 - d, 0.0], 60.0) | Sdf::circle([40.0 + d, 0.0], 60.0)
            },
            layers: default_layers,
            extent: 130.0,
        },
        ShapeEntry {
            name: "Subtract",
            description: "opSubtract(a, b) - Cut shape b from shape a (a - b).",
            slug: "subtract",
            build: |t| {
                let d = (t * 0.8).sin() * 25.0;
                Sdf::rounded_box([0.0, 0.0], [80.0, 80.0], 8.0)
                    - Sdf::circle([d, 0.0], 50.0)
            },
            layers: default_layers,
            extent: 110.0,
        },
        ShapeEntry {
            name: "Intersect",
            description: "opIntersect(a, b) - Keep only the overlapping region.",
            slug: "intersect",
            build: |t| {
                let d = (t * 0.8).sin() * 25.0;
                Sdf::circle([-30.0 - d, 0.0], 60.0)
                    .intersect(Sdf::circle([30.0 + d, 0.0], 60.0))
            },
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Smooth Union",
            description: "opSmoothUnion(a, b, k) - Blend two shapes together smoothly.",
            slug: "smooth_union",
            build: |t| {
                let d = (t * 0.8).sin() * 30.0;
                Sdf::circle([-40.0 - d, 0.0], 50.0)
                    .union_smooth(Sdf::circle([40.0 + d, 0.0], 50.0), 20.0)
            },
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Smooth Subtract",
            description: "opSmoothSubtract(a, b, k) - Smooth boolean subtraction.",
            slug: "smooth_subtract",
            build: |t| {
                let d = (t * 0.8).sin() * 25.0;
                Sdf::rounded_box([0.0, 0.0], [80.0, 80.0], 8.0)
                    .subtract_smooth(Sdf::circle([30.0 + d, 0.0], 50.0), 15.0)
            },
            layers: default_layers,
            extent: 110.0,
        },
        // ================================================================
        // Modifiers
        // ================================================================
        ShapeEntry {
            name: "Round",
            description: "opRound(sdf, r) - Expand shape boundary by radius.",
            slug: "round",
            build: |_t| Sdf::rect([0.0, 0.0], [60.0, 30.0]).round(15.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Onion",
            description: "opOnion(sdf, t) - Create hollow outline from shape.",
            slug: "onion",
            build: |_t| Sdf::circle([0.0, 0.0], 70.0).onion(8.0),
            layers: default_layers,
            extent: 78.0,
        },
        ShapeEntry {
            name: "Nested Onion",
            description: "Multiple onion layers creating concentric rings.",
            slug: "nested_onion",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0).onion(12.0).onion(4.0),
            layers: default_layers,
            extent: 96.0,
        },
        // ================================================================
        // Composed Shapes
        // ================================================================
        ShapeEntry {
            name: "Capsule",
            description: "Line segment with round modifier (pill shape).",
            slug: "capsule",
            build: |_t| Sdf::line([-50.0, 0.0], [50.0, 0.0]).round(25.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Cross",
            description: "Union of two rectangles forming a plus shape.",
            slug: "cross_composed",
            build: |_t| {
                Sdf::rect([0.0, 0.0], [80.0, 25.0])
                    .union_smooth(Sdf::rect([0.0, 0.0], [25.0, 80.0]), 8.0)
            },
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Rounded X",
            description: "Two diagonal capsules forming an X shape.",
            slug: "rounded_x_composed",
            build: |_t| {
                let arm1 = Sdf::line([-50.0, -50.0], [50.0, 50.0]).round(10.0);
                let arm2 = Sdf::line([-50.0, 50.0], [50.0, -50.0]).round(10.0);
                arm1 | arm2
            },
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Node (Nodegraph)",
            description: "Typical node graph node: rounded box with shadow, border, and pins.",
            slug: "node",
            build: |_t| {
                let body = Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 8.0);
                let pin_l1 = Sdf::circle([-100.0, -25.0], 6.0);
                let pin_l2 = Sdf::circle([-100.0, 25.0], 6.0);
                let pin_r = Sdf::circle([100.0, 0.0], 6.0);
                body - pin_l1 - pin_l2 - pin_r
            },
            layers: default_layers,
            extent: 110.0,
        },
        ShapeEntry {
            name: "Node (Arrowed)",
            description: "Node shape with animated arrow pattern applied.",
            slug: "node_arrowed",
            build: |_t| {
                let body = Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 8.0);
                let pin_l1 = Sdf::circle([-100.0, -25.0], 6.0);
                let pin_l2 = Sdf::circle([-100.0, 25.0], 6.0);
                let pin_r = Sdf::circle([100.0, 0.0], 6.0);
                (body - pin_l1 - pin_l2 - pin_r).arrow(10.0, 6.0, 4.0, 0.6, 30.0)
            },
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Nodes (Smooth Union)",
            description: "Two overlapping node shapes blended with smooth union.",
            slug: "nodes_smooth_union",
            build: |_t| {
                let body_a = Sdf::rounded_box([-60.0, -30.0], [80.0, 50.0], 8.0);
                let pin_a1 = Sdf::circle([-140.0, -45.0], 6.0);
                let pin_a2 = Sdf::circle([20.0, -30.0], 6.0);
                let node_a = body_a - pin_a1 - pin_a2;

                let body_b = Sdf::rounded_box([60.0, 30.0], [80.0, 50.0], 8.0);
                let pin_b1 = Sdf::circle([-20.0, 30.0], 6.0);
                let pin_b2 = Sdf::circle([140.0, 45.0], 6.0);
                let node_b = body_b - pin_b1 - pin_b2;

                node_a.union_smooth(node_b, 20.0)
            },
            layers: default_layers,
            extent: 160.0,
        },
        ShapeEntry {
            name: "Edge (Arrowed)",
            description: "Animated arrowed bezier edge between two node pins.",
            slug: "edge_arrowed",
            build: |_t| {
                // Typical node graph edge: horizontal bezier from output to input
                let from = [-120.0, -40.0];
                let to = [120.0, 40.0];
                let offset = 80.0;
                Sdf::bezier(
                    from,
                    [from[0] + offset, from[1]],
                    [to[0] - offset, to[1]],
                    to,
                )
                .arrow(10.0, 6.0, 4.0, 0.6, 40.0)
            },
            layers: default_layers,
            extent: 140.0,
        },
        ShapeEntry {
            name: "Edge (Dashed)",
            description: "Animated dashed bezier edge between two node pins.",
            slug: "edge_dashed",
            build: |_t| {
                let from = [-120.0, -40.0];
                let to = [120.0, 40.0];
                let offset = 80.0;
                Sdf::bezier(
                    from,
                    [from[0] + offset, from[1]],
                    [to[0] - offset, to[1]],
                    to,
                )
                .dash(18.0, 10.0, 4.0, 0.0, 50.0)
            },
            layers: default_layers,
            extent: 140.0,
        },
        ShapeEntry {
            name: "Dashed Circle",
            description: "Circle outline with dashed stroke pattern.",
            slug: "dashed_circle",
            build: |_t| Sdf::circle([0.0, 0.0], 70.0),
            layers: || {
                vec![Layer::stroke(
                    Color::from_rgb(0.4, 0.8, 1.0),
                    Pattern::dashed(3.0, 15.0, 8.0),
                )]
            },
            extent: 75.0,
        },
        ShapeEntry {
            name: "Dotted Box",
            description: "Rounded box with dotted stroke pattern.",
            slug: "dotted_box",
            build: |_t| Sdf::rounded_box([0.0, 0.0], [90.0, 60.0], 12.0),
            layers: || {
                vec![Layer::stroke(
                    Color::from_rgb(1.0, 0.6, 0.2),
                    Pattern::dotted(12.0, 3.0),
                )]
            },
            extent: 90.0,
        },
        ShapeEntry {
            name: "Animated Flow",
            description: "Bezier curve with animated dashed flow pattern.",
            slug: "animated_flow",
            build: |_t| {
                Sdf::bezier(
                    [-100.0, 40.0],
                    [-30.0, -60.0],
                    [30.0, 60.0],
                    [100.0, -40.0],
                )
            },
            layers: || {
                vec![
                    Layer::stroke(
                        Color::from_rgba(0.3, 0.6, 1.0, 0.3),
                        Pattern::solid(6.0),
                    ),
                    Layer::stroke(
                        Color::from_rgb(0.3, 0.8, 1.0),
                        Pattern::dashed(2.0, 12.0, 6.0).flow(80.0),
                    ),
                ]
            },
            extent: 110.0,
        },
        ShapeEntry {
            name: "Shadow Layers",
            description: "Shape with multiple shadow layers demonstrating expand + blur.",
            slug: "shadow_layers",
            build: |_t| Sdf::rounded_box([0.0, 0.0], [80.0, 50.0], 12.0),
            layers: || {
                vec![
                    Layer::solid(Color::from_rgba(0.2, 0.5, 1.0, 0.15))
                        .expand(20.0)
                        .blur(12.0),
                    Layer::solid(Color::from_rgba(0.0, 0.0, 0.0, 0.4))
                        .expand(4.0)
                        .blur(8.0),
                    Layer::solid(Color::from_rgb(0.15, 0.15, 0.2)),
                    Layer::solid(Color::from_rgba(1.0, 1.0, 1.0, 0.08)).expand(-2.0),
                ]
            },
            extent: 100.0,
        },
        ShapeEntry {
            name: "Gradient Border",
            description: "Shape with gradient fill along arc-length.",
            slug: "gradient_border",
            build: |_t| Sdf::rounded_box([0.0, 0.0], [90.0, 60.0], 10.0).onion(3.0),
            layers: || {
                vec![Layer::gradient_u(
                    Color::from_rgb(1.0, 0.3, 0.3),
                    Color::from_rgb(0.3, 0.3, 1.0),
                )]
            },
            extent: 93.0,
        },
        // ================================================================
        // IQ Primitives - Polygons
        // ================================================================
        ShapeEntry {
            name: "Ellipse",
            description: "sdEllipse(p, ab) - Ellipse with semi-axes.",
            slug: "ellipse",
            build: |_t| Sdf::ellipse([100.0, 60.0]),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Triangle",
            description: "sdTriangle(p, p0, p1, p2) - Arbitrary triangle.",
            slug: "triangle",
            build: |_t| Sdf::triangle([0.0, -70.0], [-80.0, 50.0], [80.0, 50.0]),
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Equilateral Triangle",
            description: "sdEquilateralTriangle(p, r) - Regular equilateral triangle.",
            slug: "equilateral_triangle",
            build: |_t| Sdf::equilateral_triangle(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Isosceles Triangle",
            description: "sdTriangleIsosceles(p, q) - Isosceles triangle.",
            slug: "isosceles_triangle",
            build: |_t| Sdf::isosceles_triangle([60.0, 80.0]),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Rhombus",
            description: "sdRhombus(p, b) - Diamond/rhombus shape.",
            slug: "rhombus",
            build: |_t| Sdf::rhombus([80.0, 60.0]),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Trapezoid",
            description: "sdTrapezoid(p, r1, r2, he) - Isosceles trapezoid.",
            slug: "trapezoid",
            build: |_t| Sdf::trapezoid(80.0, 50.0, 50.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Parallelogram",
            description: "sdParallelogram(p, wi, he, sk) - Skewed rectangle.",
            slug: "parallelogram",
            build: |_t| Sdf::parallelogram(80.0, 50.0, 30.0),
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Pentagon",
            description: "sdPentagon(p, r) - Regular pentagon.",
            slug: "pentagon",
            build: |_t| Sdf::pentagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Hexagon",
            description: "sdHexagon(p, r) - Regular hexagon.",
            slug: "hexagon",
            build: |_t| Sdf::hexagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Octagon",
            description: "sdOctogon(p, r) - Regular octagon.",
            slug: "octagon",
            build: |_t| Sdf::octagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        // ================================================================
        // IQ Primitives - Stars
        // ================================================================
        ShapeEntry {
            name: "Hexagram",
            description: "sdHexagram(p, r) - Six-pointed star (Star of David).",
            slug: "hexagram",
            build: |_t| Sdf::hexagram(60.0),
            layers: default_layers,
            extent: 70.0,
        },
        ShapeEntry {
            name: "Star (5-point)",
            description: "sdStar(p, r, n, m) - Five-pointed star.",
            slug: "star_5",
            build: |_t| Sdf::star(80.0, 5, 3.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Star (8-point)",
            description: "sdStar(p, r, n, m) - Eight-pointed star.",
            slug: "star_8",
            build: |_t| Sdf::star(80.0, 8, 5.0),
            layers: default_layers,
            extent: 85.0,
        },
        // ================================================================
        // IQ Primitives - Circular/Arcs
        // ================================================================
        ShapeEntry {
            name: "Pie",
            description: "sdPie(p, c, r) - Pie/sector shape.",
            slug: "pie",
            build: |_t| Sdf::pie(std::f32::consts::FRAC_PI_4, 80.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Arc",
            description: "sdArc(p, sc, ra, rb) - Arc segment.",
            slug: "arc",
            build: |_t| Sdf::arc(std::f32::consts::FRAC_PI_3, 70.0, 8.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Cut Disk",
            description: "sdCutDisk(p, r, h) - Disk with flat cut.",
            slug: "cut_disk",
            build: |_t| Sdf::cut_disk(80.0, 30.0),
            layers: default_layers,
            extent: 85.0,
        },
        // ================================================================
        // IQ Primitives - Curves & Special
        // ================================================================
        ShapeEntry {
            name: "Heart",
            description: "sdHeart(p) - Heart shape (unit-scale, zoomed to fit).",
            slug: "heart",
            build: |_t| Sdf::heart(),
            layers: default_layers,
            extent: 1.2,
        },
        ShapeEntry {
            name: "Egg",
            description: "sdEgg(p, ra, rb) - Egg shape.",
            slug: "egg",
            build: |_t| Sdf::egg(60.0, 15.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Moon",
            description: "sdMoon(p, d, ra, rb) - Crescent moon.",
            slug: "moon",
            build: |_t| Sdf::moon(40.0, 70.0, 60.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Vesica",
            description: "sdVesica(p, r, d) - Vesica piscis (lens shape).",
            slug: "vesica",
            build: |_t| Sdf::vesica(80.0, 40.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Uneven Capsule",
            description: "sdUnevenCapsule(p, r1, r2, h) - Capsule with different end radii.",
            slug: "uneven_capsule",
            build: |_t| Sdf::uneven_capsule(25.0, 15.0, 80.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Oriented Box",
            description: "sdOrientedBox(p, a, b, th) - Rotated rectangle.",
            slug: "oriented_box",
            build: |_t| Sdf::oriented_box([-60.0, -30.0], [60.0, 30.0], 20.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Horseshoe",
            description: "sdHorseshoe(p, c, r, w) - Horseshoe/arc segment.",
            slug: "horseshoe",
            build: |_t| Sdf::horseshoe(1.3, 60.0, [20.0, 8.0]),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Rounded X (SDF)",
            description: "sdRoundedX(p, w, r) - Dedicated rounded X SDF.",
            slug: "rounded_x",
            build: |_t| Sdf::rounded_x(80.0, 12.0),
            layers: default_layers,
            extent: 65.0,
        },
        ShapeEntry {
            name: "Cross (SDF)",
            description: "sdCross(p, b, r) - Dedicated cross/plus SDF.",
            slug: "cross",
            build: |_t| Sdf::cross([80.0, 30.0], 0.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Quad Bezier",
            description: "sdBezier(p, A, B, C) - Quadratic bezier (unsigned distance).",
            slug: "quad_bezier",
            build: |_t| Sdf::quad_bezier([-80.0, 50.0], [0.0, -60.0], [80.0, 50.0]),
            layers: stroke_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Parabola",
            description: "sdParabola(p, k) - Parabola y = k*x^2.",
            slug: "parabola",
            build: |_t| Sdf::parabola(0.01),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Cool S",
            description: "sdfCoolS(p) - The classic 'Cool S' shape (unit-scale).",
            slug: "cool_s",
            build: |_t| Sdf::cool_s(),
            layers: default_layers,
            extent: 2.0,
        },
        ShapeEntry {
            name: "Blobby Cross",
            description: "sdBlobbyCross(p, he) - Cross with curved bulging sides.",
            slug: "blobby_cross",
            build: |_t| Sdf::blobby_cross(0.5),
            layers: default_layers,
            extent: 1.0,
        },
        // ================================================================
        // Pattern Operations
        // ================================================================
        ShapeEntry {
            name: "Dashed Circle",
            description: "opDash - Repeating dashes along a circle contour.",
            slug: "dash_circle",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0).dash(30.0, 15.0, 6.0, 0.0, 0.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Dashed Angled",
            description: "opDash(angle) - Dashes with angled parallelogram caps.",
            slug: "dash_angled",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0).dash(30.0, 15.0, 6.0, 0.5, 0.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Dashed Animated",
            description: "opDash(speed) - Animated dashes flowing along contour.",
            slug: "dash_animated",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0).dash(30.0, 15.0, 6.0, 0.0, 50.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Dashed Bezier",
            description: "opDash on bezier - Dashes along a cubic bezier curve.",
            slug: "dash_bezier",
            build: |_t| {
                Sdf::bezier([-100.0, 50.0], [-30.0, -80.0], [30.0, 80.0], [100.0, -50.0])
                    .dash(25.0, 12.0, 5.0, 0.0, 0.0)
            },
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Arrow Line",
            description: "opArrow - Angled slashes crossing a line segment.",
            slug: "arrow_line",
            build: |_t| Sdf::line([-100.0, 0.0], [100.0, 0.0]).arrow(15.0, 10.0, 6.0, 0.7, 0.0),
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Arrow Animated",
            description: "opArrow(speed) - Animated arrow slashes flowing along contour.",
            slug: "arrow_animated",
            build: |_t| Sdf::circle([0.0, 0.0], 80.0).arrow(12.0, 8.0, 5.0, 0.7, 40.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Arrow Bezier",
            description: "opArrow on bezier - Angled slashes along a cubic bezier.",
            slug: "arrow_bezier",
            build: |_t| {
                Sdf::bezier([-100.0, 50.0], [-30.0, -80.0], [30.0, 80.0], [100.0, -50.0])
                    .arrow(12.0, 8.0, 5.0, 0.6, 0.0)
            },
            layers: default_layers,
            extent: 120.0,
        },
        ShapeEntry {
            name: "Dashed Box",
            description: "opDash on box - Dashed outline of a rectangle.",
            slug: "dash_box",
            build: |_t| Sdf::rect([0.0, 0.0], [80.0, 50.0]).onion(2.0).dash(20.0, 10.0, 4.0, 0.3, 0.0),
            layers: default_layers,
            extent: 100.0,
        },
        // ================================================================
        // Edge Pattern Editor (Layer-level)
        // ================================================================
        ShapeEntry {
            name: "Edge Editor",
            description: "Interactive multi-layer edge with pattern, border, and shadow controls.",
            slug: "edge_editor",
            build: edge_bezier,
            layers: || edge_layers(Pattern::solid(3.0)),
            extent: 140.0,
        },
        // ================================================================
        // Node Editor
        // ================================================================
        ShapeEntry {
            name: "Node Editor",
            description: "Interactive node with shadow, fill, and border controls.",
            slug: "node_editor",
            build: |_t| Sdf::rounded_box([0.0, 0.0], [120.0, 80.0], 8.0),
            layers: || vec![
                Layer::solid(Color::from_rgba(0.0, 0.0, 0.0, 0.3))
                    .expand(4.0).blur(8.0).offset(4.0, 4.0),
                Layer::solid(Color::from_rgba(0.14, 0.14, 0.16, 0.75)),
                Layer::stroke(Color::from_rgb(0.20, 0.20, 0.22), Pattern::solid(1.0)),
            ],
            extent: 140.0,
        },
    ]
}

fn edge_bezier(_t: f32) -> Sdf {
    let from = [-120.0, -40.0];
    let to = [120.0, 40.0];
    let offset = 80.0;
    let fwd = Sdf::bezier(from, [from[0] + offset, from[1]], [to[0] - offset, to[1]], to);
    // Mirror across vertical center axis (negate x)
    let mir = Sdf::bezier(
        [-from[0], from[1]],
        [-(from[0] + offset), from[1]],
        [-(to[0] - offset), to[1]],
        [-to[0], to[1]],
    );
    fwd | mir
}

/// Build the multi-layer stack for an edge with the given stroke pattern.
///
/// Layer structure (bottom to top):
///   1. Shadow  - soft dark glow behind the entire edge
///   2. Border  - solid ring around the stroke, with comic-style outline
///   3. Stroke  - the visible patterned line, with outline following the pattern
fn edge_layers(pattern: Pattern) -> Vec<Layer> {
    // Stroke is 3.0 wide by default (half = 1.5)
    let stroke_half = 1.5;
    let gap = 1.0;
    let border_thickness = 1.5;
    let border_center = stroke_half + gap + border_thickness * 0.5;
    let outline = Color::from_rgba(0.0, 0.0, 0.0, 0.8);

    vec![
        // Shadow: extends past the border, fades to transparent
        Layer::solid(Color::from_rgba(0.0, 0.0, 0.0, 0.15))
            .expand(border_center + border_thickness * 0.5 + 6.0)
            .blur(8.0),
        // Border: solid ring outside the stroke, with outline around it
        Layer::stroke(
            Color::from_rgb(0.12, 0.12, 0.2),
            Pattern::solid(border_thickness),
        )
        .expand(border_center)
        .outline(0.8, outline),
        // Stroke: patterned line, outline follows the pattern shape
        Layer::stroke(Color::from_rgb(0.35, 0.8, 1.0), pattern)
            .outline(0.6, outline),
    ]
}

fn default_layers() -> Vec<Layer> {
    vec![Layer::distance_field_default()]
}

fn stroke_layers() -> Vec<Layer> {
    vec![Layer::distance_field_default()]
}

