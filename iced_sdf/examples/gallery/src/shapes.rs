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
    /// Builds the SDF shape.
    pub build: fn() -> Sdf,
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
            build: || Sdf::circle([0.0, 0.0], 80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Box",
            description: "sdBox(p, b) - Axis-aligned rectangle.",
            build: || Sdf::rect([0.0, 0.0], [100.0, 60.0]),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Rounded Box",
            description: "sdRoundBox(p, b, r) - Rectangle with rounded corners.",
            build: || Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 16.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Segment",
            description: "sdSegment(p, a, b) - Line segment between two points.",
            build: || Sdf::line([-80.0, -40.0], [80.0, 40.0]),
            layers: stroke_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Bezier",
            description: "Cubic bezier curve with 4 control points.",
            build: || {
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
            build: || Sdf::circle([-40.0, 0.0], 60.0) | Sdf::circle([40.0, 0.0], 60.0),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Subtract",
            description: "opSubtract(a, b) - Cut shape b from shape a (a - b).",
            build: || {
                Sdf::rounded_box([0.0, 0.0], [80.0, 80.0], 8.0)
                    - Sdf::circle([0.0, 0.0], 50.0)
            },
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Intersect",
            description: "opIntersect(a, b) - Keep only the overlapping region.",
            build: || {
                Sdf::circle([-30.0, 0.0], 60.0).intersect(Sdf::circle([30.0, 0.0], 60.0))
            },
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Smooth Union",
            description: "opSmoothUnion(a, b, k) - Blend two shapes together smoothly.",
            build: || {
                Sdf::circle([-40.0, 0.0], 50.0)
                    .union_smooth(Sdf::circle([40.0, 0.0], 50.0), 20.0)
            },
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Smooth Subtract",
            description: "opSmoothSubtract(a, b, k) - Smooth boolean subtraction.",
            build: || {
                Sdf::rounded_box([0.0, 0.0], [80.0, 80.0], 8.0)
                    .subtract_smooth(Sdf::circle([30.0, 0.0], 50.0), 15.0)
            },
            layers: default_layers,
            extent: 80.0,
        },
        // ================================================================
        // Modifiers
        // ================================================================
        ShapeEntry {
            name: "Round",
            description: "opRound(sdf, r) - Expand shape boundary by radius.",
            build: || Sdf::rect([0.0, 0.0], [60.0, 30.0]).round(15.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Onion",
            description: "opOnion(sdf, t) - Create hollow outline from shape.",
            build: || Sdf::circle([0.0, 0.0], 70.0).onion(8.0),
            layers: default_layers,
            extent: 78.0,
        },
        ShapeEntry {
            name: "Nested Onion",
            description: "Multiple onion layers creating concentric rings.",
            build: || Sdf::circle([0.0, 0.0], 80.0).onion(12.0).onion(4.0),
            layers: default_layers,
            extent: 96.0,
        },
        // ================================================================
        // Composed Shapes
        // ================================================================
        ShapeEntry {
            name: "Capsule",
            description: "Line segment with round modifier (pill shape).",
            build: || Sdf::line([-50.0, 0.0], [50.0, 0.0]).round(25.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Cross",
            description: "Union of two rectangles forming a plus shape.",
            build: || {
                Sdf::rect([0.0, 0.0], [80.0, 25.0])
                    .union_smooth(Sdf::rect([0.0, 0.0], [25.0, 80.0]), 8.0)
            },
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Rounded X",
            description: "Two diagonal capsules forming an X shape.",
            build: || {
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
            build: || {
                let body = Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 8.0);
                let pin_l1 = Sdf::circle([-100.0, -25.0], 6.0);
                let pin_l2 = Sdf::circle([-100.0, 25.0], 6.0);
                let pin_r = Sdf::circle([100.0, 0.0], 6.0);
                body - pin_l1 - pin_l2 - pin_r
            },
            layers: node_layers,
            extent: 110.0,
        },
        ShapeEntry {
            name: "Dashed Circle",
            description: "Circle outline with dashed stroke pattern.",
            build: || Sdf::circle([0.0, 0.0], 70.0),
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
            build: || Sdf::rounded_box([0.0, 0.0], [90.0, 60.0], 12.0),
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
            build: || {
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
            build: || Sdf::rounded_box([0.0, 0.0], [80.0, 50.0], 12.0),
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
            build: || Sdf::rounded_box([0.0, 0.0], [90.0, 60.0], 10.0).onion(3.0),
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
            build: || Sdf::ellipse([100.0, 60.0]),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Triangle",
            description: "sdTriangle(p, p0, p1, p2) - Arbitrary triangle.",
            build: || Sdf::triangle([0.0, -70.0], [-80.0, 50.0], [80.0, 50.0]),
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Equilateral Triangle",
            description: "sdEquilateralTriangle(p, r) - Regular equilateral triangle.",
            build: || Sdf::equilateral_triangle(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Isosceles Triangle",
            description: "sdTriangleIsosceles(p, q) - Isosceles triangle.",
            build: || Sdf::isosceles_triangle([60.0, 80.0]),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Rhombus",
            description: "sdRhombus(p, b) - Diamond/rhombus shape.",
            build: || Sdf::rhombus([80.0, 60.0]),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Trapezoid",
            description: "sdTrapezoid(p, r1, r2, he) - Isosceles trapezoid.",
            build: || Sdf::trapezoid(80.0, 50.0, 50.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Parallelogram",
            description: "sdParallelogram(p, wi, he, sk) - Skewed rectangle.",
            build: || Sdf::parallelogram(80.0, 50.0, 30.0),
            layers: default_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Pentagon",
            description: "sdPentagon(p, r) - Regular pentagon.",
            build: || Sdf::pentagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Hexagon",
            description: "sdHexagon(p, r) - Regular hexagon.",
            build: || Sdf::hexagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Octagon",
            description: "sdOctogon(p, r) - Regular octagon.",
            build: || Sdf::octagon(80.0),
            layers: default_layers,
            extent: 80.0,
        },
        // ================================================================
        // IQ Primitives - Stars
        // ================================================================
        ShapeEntry {
            name: "Hexagram",
            description: "sdHexagram(p, r) - Six-pointed star (Star of David).",
            build: || Sdf::hexagram(60.0),
            layers: default_layers,
            extent: 70.0,
        },
        ShapeEntry {
            name: "Star (5-point)",
            description: "sdStar(p, r, n, m) - Five-pointed star.",
            build: || Sdf::star(80.0, 5, 3.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Star (8-point)",
            description: "sdStar(p, r, n, m) - Eight-pointed star.",
            build: || Sdf::star(80.0, 8, 5.0),
            layers: default_layers,
            extent: 85.0,
        },
        // ================================================================
        // IQ Primitives - Circular/Arcs
        // ================================================================
        ShapeEntry {
            name: "Pie",
            description: "sdPie(p, c, r) - Pie/sector shape.",
            build: || Sdf::pie(std::f32::consts::FRAC_PI_4, 80.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Arc",
            description: "sdArc(p, sc, ra, rb) - Arc segment.",
            build: || Sdf::arc(std::f32::consts::FRAC_PI_3, 70.0, 8.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Cut Disk",
            description: "sdCutDisk(p, r, h) - Disk with flat cut.",
            build: || Sdf::cut_disk(80.0, 30.0),
            layers: default_layers,
            extent: 85.0,
        },
        // ================================================================
        // IQ Primitives - Curves & Special
        // ================================================================
        ShapeEntry {
            name: "Heart",
            description: "sdHeart(p) - Heart shape (unit-scale, zoomed to fit).",
            build: || Sdf::heart(),
            layers: default_layers,
            extent: 1.2,
        },
        ShapeEntry {
            name: "Egg",
            description: "sdEgg(p, ra, rb) - Egg shape.",
            build: || Sdf::egg(60.0, 15.0),
            layers: default_layers,
            extent: 80.0,
        },
        ShapeEntry {
            name: "Moon",
            description: "sdMoon(p, d, ra, rb) - Crescent moon.",
            build: || Sdf::moon(40.0, 70.0, 60.0),
            layers: default_layers,
            extent: 75.0,
        },
        ShapeEntry {
            name: "Vesica",
            description: "sdVesica(p, r, d) - Vesica piscis (lens shape).",
            build: || Sdf::vesica(80.0, 40.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Uneven Capsule",
            description: "sdUnevenCapsule(p, r1, r2, h) - Capsule with different end radii.",
            build: || Sdf::uneven_capsule(25.0, 15.0, 80.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Oriented Box",
            description: "sdOrientedBox(p, a, b, th) - Rotated rectangle.",
            build: || Sdf::oriented_box([-60.0, -30.0], [60.0, 30.0], 20.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Horseshoe",
            description: "sdHorseshoe(p, c, r, w) - Horseshoe/arc segment.",
            build: || Sdf::horseshoe(1.3, 60.0, [20.0, 8.0]),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Rounded X (SDF)",
            description: "sdRoundedX(p, w, r) - Dedicated rounded X SDF.",
            build: || Sdf::rounded_x(80.0, 12.0),
            layers: default_layers,
            extent: 65.0,
        },
        ShapeEntry {
            name: "Cross (SDF)",
            description: "sdCross(p, b, r) - Dedicated cross/plus SDF.",
            build: || Sdf::cross([80.0, 30.0], 0.0),
            layers: default_layers,
            extent: 85.0,
        },
        ShapeEntry {
            name: "Quad Bezier",
            description: "sdBezier(p, A, B, C) - Quadratic bezier (unsigned distance).",
            build: || Sdf::quad_bezier([-80.0, 50.0], [0.0, -60.0], [80.0, 50.0]),
            layers: stroke_layers,
            extent: 90.0,
        },
        ShapeEntry {
            name: "Parabola",
            description: "sdParabola(p, k) - Parabola y = k*x^2.",
            build: || Sdf::parabola(0.01),
            layers: default_layers,
            extent: 100.0,
        },
        ShapeEntry {
            name: "Cool S",
            description: "sdfCoolS(p) - The classic 'Cool S' shape (unit-scale).",
            build: || Sdf::cool_s(),
            layers: default_layers,
            extent: 2.0,
        },
        ShapeEntry {
            name: "Blobby Cross",
            description: "sdBlobbyCross(p, he) - Cross with curved bulging sides.",
            build: || Sdf::blobby_cross(1.0),
            layers: default_layers,
            extent: 1.2,
        },
    ]
}

fn default_layers() -> Vec<Layer> {
    vec![Layer::distance_field_default()]
}

fn stroke_layers() -> Vec<Layer> {
    vec![Layer::distance_field_default()]
}

fn node_layers() -> Vec<Layer> {
    vec![
        // Shadow
        Layer::solid(Color::from_rgba(0.0, 0.0, 0.0, 0.35))
            .expand(5.0)
            .blur(8.0),
        // Fill
        Layer::solid(Color::from_rgb(0.18, 0.18, 0.22)),
        // Border
        Layer::stroke(
            Color::from_rgb(0.4, 0.4, 0.5),
            Pattern::solid(1.5),
        ),
    ]
}
