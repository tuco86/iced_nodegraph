//! SDF shape primitives and CSG operations.
//!
//! Provides a builder API for constructing signed distance field shapes
//! that can be combined using boolean operations.

use glam::Vec2;
use std::ops::{BitOr, Sub};

/// An SDF shape node in the CSG tree.
///
/// Each node is either a primitive shape or a boolean operation
/// combining other shapes.
#[derive(Clone, Debug)]
pub enum SdfNode {
    // Primitives (all return distance and arc-length parameter u)
    /// Circle centered at `center` with `radius`.
    Circle { center: Vec2, radius: f32 },
    /// Axis-aligned box centered at `center` with `half_size`.
    Box { center: Vec2, half_size: Vec2 },
    /// Rounded box with corner radius.
    RoundedBox {
        center: Vec2,
        half_size: Vec2,
        corner_radius: f32,
    },
    /// Line segment from `a` to `b`.
    Line { a: Vec2, b: Vec2 },
    /// Cubic bezier curve with 4 control points.
    Bezier {
        p0: Vec2,
        p1: Vec2,
        p2: Vec2,
        p3: Vec2,
    },
    /// Quadratic bezier curve with 3 control points.
    QuadBezier { p0: Vec2, p1: Vec2, p2: Vec2 },
    /// Ellipse with semi-axes.
    Ellipse { ab: Vec2 },
    /// Arbitrary triangle with 3 vertices.
    Triangle { p0: Vec2, p1: Vec2, p2: Vec2 },
    /// Equilateral triangle with circumradius.
    EquilateralTriangle { radius: f32 },
    /// Isosceles triangle.
    IsoscelesTriangle { q: Vec2 },
    /// Rhombus (diamond) with half-diagonals.
    Rhombus { b: Vec2 },
    /// Trapezoid with half-widths and half-height.
    Trapezoid { r1: f32, r2: f32, he: f32 },
    /// Parallelogram with width, height, and skew.
    Parallelogram { wi: f32, he: f32, sk: f32 },
    /// Regular pentagon.
    Pentagon { radius: f32 },
    /// Regular hexagon.
    Hexagon { radius: f32 },
    /// Regular octagon.
    Octagon { radius: f32 },
    /// Six-pointed star (Star of David).
    Hexagram { radius: f32 },
    /// N-pointed star.
    Star { radius: f32, n: u32, m: f32 },
    /// Pie/sector shape.
    Pie { angle: f32, radius: f32 },
    /// Arc shape.
    Arc { angle: f32, ra: f32, rb: f32 },
    /// Disk with horizontal cut.
    CutDisk { radius: f32, h: f32 },
    /// Heart shape (unit-sized, scale with Round/expand).
    Heart,
    /// Egg shape.
    Egg { ra: f32, rb: f32 },
    /// Crescent moon.
    Moon { d: f32, ra: f32, rb: f32 },
    /// Vesica piscis (lens shape).
    Vesica { r: f32, d: f32 },
    /// Capsule with different end radii.
    UnevenCapsule { r1: f32, r2: f32, h: f32 },
    /// Oriented (rotated) box defined by endpoints and thickness.
    OrientedBox { a: Vec2, b: Vec2, thickness: f32 },
    /// Horseshoe shape.
    Horseshoe { angle: f32, radius: f32, w: Vec2 },
    /// Rounded X shape.
    RoundedX { w: f32, r: f32 },
    /// Cross/plus shape.
    Cross { b: Vec2, r: f32 },
    /// Parabola y = k*x^2.
    Parabola { k: f32 },
    /// Cool S shape (unit-sized).
    CoolS,
    /// Blobbycross shape.
    BlobbyCross { he: f32 },

    // Pattern modifiers
    /// Dash pattern along shape contour.
    Dash {
        node: Box<SdfNode>,
        dash: f32,
        gap: f32,
        thickness: f32,
        angle: f32,
        speed: f32,
    },
    /// Arrow (angled slash) pattern along shape contour.
    Arrow {
        node: Box<SdfNode>,
        segment: f32,
        gap: f32,
        thickness: f32,
        angle: f32,
        speed: f32,
    },

    // Boolean operations
    /// Union of two shapes (min distance).
    Union(Box<SdfNode>, Box<SdfNode>),
    /// Subtraction: first shape minus second (max(a, -b)).
    Subtract(Box<SdfNode>, Box<SdfNode>),
    /// Intersection of two shapes (max distance).
    Intersect(Box<SdfNode>, Box<SdfNode>),
    /// Smooth union with blend factor.
    SmoothUnion {
        a: Box<SdfNode>,
        b: Box<SdfNode>,
        k: f32,
    },
    /// Smooth subtraction with blend factor.
    SmoothSubtract {
        a: Box<SdfNode>,
        b: Box<SdfNode>,
        k: f32,
    },

    // Modifiers
    /// Expand/contract shape by offset.
    Round { node: Box<SdfNode>, radius: f32 },
    /// Create outline (annulus) from shape.
    Onion { node: Box<SdfNode>, thickness: f32 },
}

impl SdfNode {
    /// Whether this node tree contains time-dependent animations.
    ///
    /// Returns `true` if any `Dash` or `Arrow` node has a non-zero speed.
    pub fn has_animation(&self) -> bool {
        match self {
            SdfNode::Dash { speed, node, .. } | SdfNode::Arrow { speed, node, .. } => {
                *speed != 0.0 || node.has_animation()
            }
            SdfNode::Union(a, b)
            | SdfNode::Subtract(a, b)
            | SdfNode::Intersect(a, b) => a.has_animation() || b.has_animation(),
            SdfNode::SmoothUnion { a, b, .. } | SdfNode::SmoothSubtract { a, b, .. } => {
                a.has_animation() || b.has_animation()
            }
            SdfNode::Round { node, .. } | SdfNode::Onion { node, .. } => node.has_animation(),
            _ => false,
        }
    }
}

/// Builder for SDF shapes with method chaining.
///
/// Each shape method below includes a live GPU-rendered preview powered by the
/// [SDF Gallery](../../sdf_gallery/index.html). Each visible shape gets its
/// own WebGPU-rendered instance.
///
/// <style>
///   .sdf-shape-slot { position: relative; background: #1e1e2e; border-radius: 8px; overflow: hidden; margin: 0.5em 0; }
///   .sdf-target { width: 100%; height: 100%; }
///   .sdf-target canvas { display: block !important; width: 100% !important; height: 100% !important; }
/// </style>
/// <script type="module" src="../../sdf_gallery/pkg/sdf-shape-loader.js"></script>
#[derive(Clone, Debug)]
pub struct Sdf {
    root: SdfNode,
}

impl Sdf {
    // ================================================================
    // Primitive constructors
    // ================================================================

    /// Create a circle SDF.
    ///
    /// `sdCircle(p, r)` -- signed distance from point to circle boundary.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([0.0, 0.0], 50.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="circle" style="height:300px"><div id="sdf-target-circle" class="sdf-target"></div></div>
    pub fn circle(center: impl Into<Vec2>, radius: f32) -> Self {
        Self {
            root: SdfNode::Circle {
                center: center.into(),
                radius,
            },
        }
    }

    /// Create a box SDF (axis-aligned rectangle).
    ///
    /// `sdBox(p, b)` -- signed distance from point to axis-aligned box.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rect([0.0, 0.0], [100.0, 60.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="box" style="height:300px"><div id="sdf-target-box" class="sdf-target"></div></div>
    pub fn rect(center: impl Into<Vec2>, half_size: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Box {
                center: center.into(),
                half_size: half_size.into(),
            },
        }
    }

    /// Create a rounded box SDF.
    ///
    /// `sdRoundBox(p, b, r)` -- box with rounded corners.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rounded_box([0.0, 0.0], [100.0, 60.0], 16.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="rounded_box" style="height:300px"><div id="sdf-target-rounded_box" class="sdf-target"></div></div>
    pub fn rounded_box(
        center: impl Into<Vec2>,
        half_size: impl Into<Vec2>,
        corner_radius: f32,
    ) -> Self {
        Self {
            root: SdfNode::RoundedBox {
                center: center.into(),
                half_size: half_size.into(),
                corner_radius,
            },
        }
    }

    /// Create a line segment SDF.
    ///
    /// `sdSegment(p, a, b)` -- distance from point to line segment.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::line([-80.0, -40.0], [80.0, 40.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="segment" style="height:300px"><div id="sdf-target-segment" class="sdf-target"></div></div>
    pub fn line(a: impl Into<Vec2>, b: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Line {
                a: a.into(),
                b: b.into(),
            },
        }
    }

    /// Create a cubic bezier curve SDF.
    ///
    /// Cubic bezier with 4 control points `p0..p3`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::bezier([-100.0, 50.0], [-30.0, -80.0], [30.0, 80.0], [100.0, -50.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="bezier" style="height:300px"><div id="sdf-target-bezier" class="sdf-target"></div></div>
    pub fn bezier(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
        p3: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::Bezier {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
                p3: p3.into(),
            },
        }
    }

    /// Create a quadratic bezier curve SDF.
    ///
    /// `sdBezier(p, A, B, C)` -- unsigned distance to quadratic bezier.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::quad_bezier([-80.0, 50.0], [0.0, -60.0], [80.0, 50.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="quad_bezier" style="height:300px"><div id="sdf-target-quad_bezier" class="sdf-target"></div></div>
    pub fn quad_bezier(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::QuadBezier {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
            },
        }
    }

    /// Create an ellipse SDF.
    ///
    /// `sdEllipse(p, ab)` -- signed distance to ellipse with semi-axes `ab`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::ellipse([100.0, 60.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="ellipse" style="height:300px"><div id="sdf-target-ellipse" class="sdf-target"></div></div>
    pub fn ellipse(ab: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Ellipse { ab: ab.into() },
        }
    }

    /// Create an arbitrary triangle SDF from 3 vertices.
    ///
    /// `sdTriangle(p, p0, p1, p2)` -- signed distance to triangle.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::triangle([0.0, -70.0], [-80.0, 50.0], [80.0, 50.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="triangle" style="height:300px"><div id="sdf-target-triangle" class="sdf-target"></div></div>
    pub fn triangle(
        p0: impl Into<Vec2>,
        p1: impl Into<Vec2>,
        p2: impl Into<Vec2>,
    ) -> Self {
        Self {
            root: SdfNode::Triangle {
                p0: p0.into(),
                p1: p1.into(),
                p2: p2.into(),
            },
        }
    }

    /// Create an equilateral triangle SDF.
    ///
    /// `sdEquilateralTriangle(p, r)` -- regular equilateral triangle with circumradius.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::equilateral_triangle(80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="equilateral_triangle" style="height:300px"><div id="sdf-target-equilateral_triangle" class="sdf-target"></div></div>
    pub fn equilateral_triangle(radius: f32) -> Self {
        Self {
            root: SdfNode::EquilateralTriangle { radius },
        }
    }

    /// Create an isosceles triangle SDF.
    ///
    /// `sdTriangleIsosceles(p, q)` -- isosceles triangle with half-width and height.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::isosceles_triangle([60.0, 80.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="isosceles_triangle" style="height:300px"><div id="sdf-target-isosceles_triangle" class="sdf-target"></div></div>
    pub fn isosceles_triangle(q: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::IsoscelesTriangle { q: q.into() },
        }
    }

    /// Create a rhombus (diamond) SDF.
    ///
    /// `sdRhombus(p, b)` -- diamond shape with half-diagonals.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rhombus([80.0, 60.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="rhombus" style="height:300px"><div id="sdf-target-rhombus" class="sdf-target"></div></div>
    pub fn rhombus(b: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Rhombus { b: b.into() },
        }
    }

    /// Create a trapezoid SDF.
    ///
    /// `sdTrapezoid(p, r1, r2, he)` -- isosceles trapezoid with top/bottom half-widths and half-height.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::trapezoid(80.0, 50.0, 50.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="trapezoid" style="height:300px"><div id="sdf-target-trapezoid" class="sdf-target"></div></div>
    pub fn trapezoid(r1: f32, r2: f32, he: f32) -> Self {
        Self {
            root: SdfNode::Trapezoid { r1, r2, he },
        }
    }

    /// Create a parallelogram SDF.
    ///
    /// `sdParallelogram(p, wi, he, sk)` -- skewed rectangle with width, height, and skew.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::parallelogram(80.0, 50.0, 30.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="parallelogram" style="height:300px"><div id="sdf-target-parallelogram" class="sdf-target"></div></div>
    pub fn parallelogram(wi: f32, he: f32, sk: f32) -> Self {
        Self {
            root: SdfNode::Parallelogram { wi, he, sk },
        }
    }

    /// Create a regular pentagon SDF.
    ///
    /// `sdPentagon(p, r)` -- regular pentagon with circumradius.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::pentagon(80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="pentagon" style="height:300px"><div id="sdf-target-pentagon" class="sdf-target"></div></div>
    pub fn pentagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Pentagon { radius },
        }
    }

    /// Create a regular hexagon SDF.
    ///
    /// `sdHexagon(p, r)` -- regular hexagon with circumradius.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::hexagon(80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="hexagon" style="height:300px"><div id="sdf-target-hexagon" class="sdf-target"></div></div>
    pub fn hexagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Hexagon { radius },
        }
    }

    /// Create a regular octagon SDF.
    ///
    /// `sdOctogon(p, r)` -- regular octagon with circumradius.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::octagon(80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="octagon" style="height:300px"><div id="sdf-target-octagon" class="sdf-target"></div></div>
    pub fn octagon(radius: f32) -> Self {
        Self {
            root: SdfNode::Octagon { radius },
        }
    }

    /// Create a hexagram (Star of David) SDF.
    ///
    /// `sdHexagram(p, r)` -- six-pointed star.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::hexagram(60.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="hexagram" style="height:300px"><div id="sdf-target-hexagram" class="sdf-target"></div></div>
    pub fn hexagram(radius: f32) -> Self {
        Self {
            root: SdfNode::Hexagram { radius },
        }
    }

    /// Create an n-pointed star SDF.
    ///
    /// `sdStar(p, r, n, m)` -- star with `n` points and inner ratio `m`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::star(80.0, 5, 3.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="star_5" style="height:300px"><div id="sdf-target-star_5" class="sdf-target"></div></div>
    pub fn star(radius: f32, n: u32, m: f32) -> Self {
        Self {
            root: SdfNode::Star { radius, n, m },
        }
    }

    /// Create a pie/sector SDF.
    ///
    /// `sdPie(p, c, r)` -- pie shape with half-aperture `angle` in radians.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::pie(std::f32::consts::FRAC_PI_4, 80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="pie" style="height:300px"><div id="sdf-target-pie" class="sdf-target"></div></div>
    pub fn pie(angle: f32, radius: f32) -> Self {
        Self {
            root: SdfNode::Pie { angle, radius },
        }
    }

    /// Create an arc SDF.
    ///
    /// `sdArc(p, sc, ra, rb)` -- arc with half-aperture `angle`, outer radius `ra`, thickness `rb`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::arc(std::f32::consts::FRAC_PI_3, 70.0, 8.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="arc" style="height:300px"><div id="sdf-target-arc" class="sdf-target"></div></div>
    pub fn arc(angle: f32, ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Arc { angle, ra, rb },
        }
    }

    /// Create a cut disk SDF.
    ///
    /// `sdCutDisk(p, r, h)` -- disk with a horizontal flat cut at height `h`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::cut_disk(80.0, 30.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="cut_disk" style="height:300px"><div id="sdf-target-cut_disk" class="sdf-target"></div></div>
    pub fn cut_disk(radius: f32, h: f32) -> Self {
        Self {
            root: SdfNode::CutDisk { radius, h },
        }
    }

    /// Create a heart shape SDF (unit-sized, use round/expand to scale).
    ///
    /// `sdHeart(p)` -- heart shape at unit scale.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::heart();
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="heart" style="height:300px"><div id="sdf-target-heart" class="sdf-target"></div></div>
    pub fn heart() -> Self {
        Self {
            root: SdfNode::Heart,
        }
    }

    /// Create an egg shape SDF.
    ///
    /// `sdEgg(p, ra, rb)` -- egg with body radius `ra` and top radius `rb`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::egg(60.0, 15.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="egg" style="height:300px"><div id="sdf-target-egg" class="sdf-target"></div></div>
    pub fn egg(ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Egg { ra, rb },
        }
    }

    /// Create a crescent moon SDF.
    ///
    /// `sdMoon(p, d, ra, rb)` -- moon with displacement `d`, outer radius `ra`, inner radius `rb`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::moon(40.0, 70.0, 60.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="moon" style="height:300px"><div id="sdf-target-moon" class="sdf-target"></div></div>
    pub fn moon(d: f32, ra: f32, rb: f32) -> Self {
        Self {
            root: SdfNode::Moon { d, ra, rb },
        }
    }

    /// Create a vesica piscis (lens) SDF.
    ///
    /// `sdVesica(p, r, d)` -- lens shape with radius `r` and half-separation `d`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::vesica(80.0, 40.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="vesica" style="height:300px"><div id="sdf-target-vesica" class="sdf-target"></div></div>
    pub fn vesica(r: f32, d: f32) -> Self {
        Self {
            root: SdfNode::Vesica { r, d },
        }
    }

    /// Create an uneven capsule SDF.
    ///
    /// `sdUnevenCapsule(p, r1, r2, h)` -- capsule with different end radii.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::uneven_capsule(25.0, 15.0, 80.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="uneven_capsule" style="height:300px"><div id="sdf-target-uneven_capsule" class="sdf-target"></div></div>
    pub fn uneven_capsule(r1: f32, r2: f32, h: f32) -> Self {
        Self {
            root: SdfNode::UnevenCapsule { r1, r2, h },
        }
    }

    /// Create an oriented (rotated) box SDF.
    ///
    /// `sdOrientedBox(p, a, b, th)` -- rectangle defined by endpoints and thickness.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::oriented_box([-60.0, -30.0], [60.0, 30.0], 20.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="oriented_box" style="height:300px"><div id="sdf-target-oriented_box" class="sdf-target"></div></div>
    pub fn oriented_box(a: impl Into<Vec2>, b: impl Into<Vec2>, thickness: f32) -> Self {
        Self {
            root: SdfNode::OrientedBox {
                a: a.into(),
                b: b.into(),
                thickness,
            },
        }
    }

    /// Create a horseshoe SDF.
    ///
    /// `sdHorseshoe(p, c, r, w)` -- horseshoe/arc with half-aperture `angle`, radius, and width.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::horseshoe(1.3, 60.0, [20.0, 8.0]);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="horseshoe" style="height:300px"><div id="sdf-target-horseshoe" class="sdf-target"></div></div>
    pub fn horseshoe(angle: f32, radius: f32, w: impl Into<Vec2>) -> Self {
        Self {
            root: SdfNode::Horseshoe {
                angle,
                radius,
                w: w.into(),
            },
        }
    }

    /// Create a rounded X SDF.
    ///
    /// `sdRoundedX(p, w, r)` -- X shape with arm width `w` and corner radius `r`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rounded_x(80.0, 12.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="rounded_x" style="height:300px"><div id="sdf-target-rounded_x" class="sdf-target"></div></div>
    pub fn rounded_x(w: f32, r: f32) -> Self {
        Self {
            root: SdfNode::RoundedX { w, r },
        }
    }

    /// Create a cross/plus SDF.
    ///
    /// `sdCross(p, b, r)` -- cross shape with half-size `b` and corner radius `r`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::cross([80.0, 30.0], 0.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="cross" style="height:300px"><div id="sdf-target-cross" class="sdf-target"></div></div>
    pub fn cross(b: impl Into<Vec2>, r: f32) -> Self {
        Self {
            root: SdfNode::Cross { b: b.into(), r },
        }
    }

    /// Create a parabola SDF.
    ///
    /// `sdParabola(p, k)` -- parabola `y = k*x^2`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::parabola(0.01);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="parabola" style="height:300px"><div id="sdf-target-parabola" class="sdf-target"></div></div>
    pub fn parabola(k: f32) -> Self {
        Self {
            root: SdfNode::Parabola { k },
        }
    }

    /// Create a Cool S shape SDF (unit-sized).
    ///
    /// `sdfCoolS(p)` -- the classic "Cool S" / "Super S" shape at unit scale.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::cool_s();
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="cool_s" style="height:300px"><div id="sdf-target-cool_s" class="sdf-target"></div></div>
    pub fn cool_s() -> Self {
        Self {
            root: SdfNode::CoolS,
        }
    }

    /// Create a blobby cross SDF.
    ///
    /// `sdBlobbyCross(p, he)` -- cross shape with curved bulging sides.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::blobby_cross(1.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="blobby_cross" style="height:300px"><div id="sdf-target-blobby_cross" class="sdf-target"></div></div>
    pub fn blobby_cross(he: f32) -> Self {
        Self {
            root: SdfNode::BlobbyCross { he },
        }
    }

    // ================================================================
    // Boolean operations
    // ================================================================

    /// Union with another shape -- keeps the closest surface of either shape.
    ///
    /// `opUnion(a, b)` -- equivalent to `min(d1, d2)`. Also available as `a | b`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([-40.0, 0.0], 60.0).union(Sdf::circle([40.0, 0.0], 60.0));
    /// // or equivalently: Sdf::circle([-40.0, 0.0], 60.0) | Sdf::circle([40.0, 0.0], 60.0)
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="union" style="height:300px"><div id="sdf-target-union" class="sdf-target"></div></div>
    pub fn union(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Union(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Subtract another shape from this one.
    ///
    /// `opSubtract(a, b)` -- equivalent to `max(d1, -d2)`. Also available as `a - b`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rect([0.0, 0.0], [80.0, 80.0]).subtract(Sdf::circle([0.0, 0.0], 50.0));
    /// // or equivalently: Sdf::rect([0.0, 0.0], [80.0, 80.0]) - Sdf::circle([0.0, 0.0], 50.0)
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="subtract" style="height:300px"><div id="sdf-target-subtract" class="sdf-target"></div></div>
    pub fn subtract(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Subtract(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Intersect with another shape -- keeps only the overlapping region.
    ///
    /// `opIntersect(a, b)` -- equivalent to `max(d1, d2)`.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([-30.0, 0.0], 60.0).intersect(Sdf::circle([30.0, 0.0], 60.0));
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="intersect" style="height:300px"><div id="sdf-target-intersect" class="sdf-target"></div></div>
    pub fn intersect(self, other: Sdf) -> Self {
        Self {
            root: SdfNode::Intersect(Box::new(self.root), Box::new(other.root)),
        }
    }

    /// Smooth union with blend factor `k`.
    ///
    /// `opSmoothUnion(a, b, k)` -- blends two shapes smoothly at the seam.
    /// Larger `k` produces a wider blend region.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([-40.0, 0.0], 50.0)
    ///     .union_smooth(Sdf::circle([40.0, 0.0], 50.0), 20.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="smooth_union" style="height:300px"><div id="sdf-target-smooth_union" class="sdf-target"></div></div>
    pub fn union_smooth(self, other: Sdf, k: f32) -> Self {
        Self {
            root: SdfNode::SmoothUnion {
                a: Box::new(self.root),
                b: Box::new(other.root),
                k,
            },
        }
    }

    /// Smooth subtraction with blend factor `k`.
    ///
    /// `opSmoothSubtract(a, b, k)` -- smoothly cuts one shape from another.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rounded_box([0.0, 0.0], [80.0, 80.0], 8.0)
    ///     .subtract_smooth(Sdf::circle([30.0, 0.0], 50.0), 15.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="smooth_subtract" style="height:300px"><div id="sdf-target-smooth_subtract" class="sdf-target"></div></div>
    pub fn subtract_smooth(self, other: Sdf, k: f32) -> Self {
        Self {
            root: SdfNode::SmoothSubtract {
                a: Box::new(self.root),
                b: Box::new(other.root),
                k,
            },
        }
    }

    // ================================================================
    // Modifiers
    // ================================================================

    /// Round the shape by expanding its boundary outward by `radius`.
    ///
    /// `opRound(sdf, r)` -- offsets the distance field, rounding sharp corners.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::rect([0.0, 0.0], [60.0, 30.0]).round(15.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="round" style="height:300px"><div id="sdf-target-round" class="sdf-target"></div></div>
    pub fn round(self, radius: f32) -> Self {
        Self {
            root: SdfNode::Round {
                node: Box::new(self.root),
                radius,
            },
        }
    }

    /// Create an outline (hollow) version of the shape.
    ///
    /// `opOnion(sdf, t)` -- converts a filled shape into a ring/outline of thickness `t`.
    /// Can be chained for concentric rings.
    ///
    /// [IQ reference](https://iquilezles.org/articles/distfunctions2d/)
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([0.0, 0.0], 70.0).onion(8.0);
    /// ```
    ///
    /// <div class="sdf-shape-slot" data-shape="onion" style="height:300px"><div id="sdf-target-onion" class="sdf-target"></div></div>
    pub fn onion(self, thickness: f32) -> Self {
        Self {
            root: SdfNode::Onion {
                node: Box::new(self.root),
                thickness,
            },
        }
    }

    /// Apply a repeating dash pattern along the shape contour.
    ///
    /// Creates dashes of length `dash` separated by `gap`, with `thickness`
    /// controlling the stroke width. `angle` (radians) shears the dash caps
    /// for a parallelogram effect (0 = perpendicular caps). `speed` animates
    /// the pattern along the contour (world units/sec, 0 = static).
    ///
    /// For closed curves (circle, box, etc.), the period is automatically
    /// quantized to tile seamlessly around the perimeter.
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::circle([0.0, 0.0], 80.0).dash(20.0, 10.0, 4.0, 0.0, 0.0);
    /// ```
    pub fn dash(self, dash: f32, gap: f32, thickness: f32, angle: f32, speed: f32) -> Self {
        Self {
            root: SdfNode::Dash {
                node: Box::new(self.root),
                dash,
                gap,
                thickness,
                angle,
                speed,
            },
        }
    }

    /// Apply an arrow (angled slash) pattern along the shape contour.
    ///
    /// Creates repeating slashes that cross the shape at `angle` (radians).
    /// `segment` is the slash length, `gap` the spacing, and `thickness` the
    /// stroke width. Unlike `dash`, the shear uses absolute perpendicular
    /// distance for symmetric crossing slashes. `speed` animates the pattern
    /// (world units/sec, 0 = static).
    ///
    /// For closed curves (circle, box, etc.), the period is automatically
    /// quantized to tile seamlessly around the perimeter.
    ///
    /// # Example
    ///
    /// ```
    /// use iced_sdf::Sdf;
    /// let shape = Sdf::line([0.0, 0.0], [100.0, 0.0]).arrow(15.0, 10.0, 4.0, 0.7, 0.0);
    /// ```
    pub fn arrow(self, segment: f32, gap: f32, thickness: f32, angle: f32, speed: f32) -> Self {
        Self {
            root: SdfNode::Arrow {
                node: Box::new(self.root),
                segment,
                gap,
                thickness,
                angle,
                speed,
            },
        }
    }

    /// Consume the builder and return the root node.
    pub fn into_node(self) -> SdfNode {
        self.root
    }

    /// Get a reference to the root node.
    pub fn node(&self) -> &SdfNode {
        &self.root
    }
}

// Operator overloads for ergonomic API

impl BitOr for Sdf {
    type Output = Sdf;

    /// Union operator: `a | b`
    fn bitor(self, rhs: Sdf) -> Sdf {
        self.union(rhs)
    }
}

impl Sub for Sdf {
    type Output = Sdf;

    /// Subtraction operator: `a - b`
    fn sub(self, rhs: Sdf) -> Sdf {
        self.subtract(rhs)
    }
}

impl SdfNode {
    /// Returns the contour perimeter if this is a known closed curve.
    ///
    /// Used by Dash/Arrow to automatically quantize the repeat period
    /// so the pattern tiles seamlessly. Returns `None` for open curves
    /// (line, bezier) and CSG composites where perimeter is ambiguous.
    pub fn perimeter(&self) -> Option<f32> {
        use std::f32::consts::PI;
        match self {
            SdfNode::Circle { radius, .. } => Some(2.0 * PI * radius),
            SdfNode::Box { half_size, .. } => Some(4.0 * (half_size.x + half_size.y)),
            // u comes from sd_box, so perimeter matches the unrounded box
            SdfNode::RoundedBox { half_size, .. } => Some(4.0 * (half_size.x + half_size.y)),
            // Modifiers pass through the child's u unchanged
            SdfNode::Round { node, .. }
            | SdfNode::Onion { node, .. }
            | SdfNode::Dash { node, .. }
            | SdfNode::Arrow { node, .. } => node.perimeter(),
            // Open curves and CSG composites: no automatic quantization
            _ => None,
        }
    }
}

impl From<SdfNode> for Sdf {
    fn from(node: SdfNode) -> Self {
        Self { root: node }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_construction() {
        let sdf = Sdf::circle([100.0, 50.0], 25.0);
        match sdf.node() {
            SdfNode::Circle { center, radius } => {
                assert_eq!(*center, Vec2::new(100.0, 50.0));
                assert_eq!(*radius, 25.0);
            }
            _ => panic!("Expected Circle"),
        }
    }

    #[test]
    fn test_union_operator() {
        let a = Sdf::circle([0.0, 0.0], 10.0);
        let b = Sdf::circle([20.0, 0.0], 10.0);
        let combined = a | b;

        match combined.node() {
            SdfNode::Union(_, _) => {}
            _ => panic!("Expected Union"),
        }
    }

    #[test]
    fn test_subtract_operator() {
        let a = Sdf::rect([0.0, 0.0], [50.0, 50.0]);
        let b = Sdf::circle([0.0, 0.0], 25.0);
        let result = a - b;

        match result.node() {
            SdfNode::Subtract(_, _) => {}
            _ => panic!("Expected Subtract"),
        }
    }

    #[test]
    fn test_method_chaining() {
        let shape = Sdf::rounded_box([0.0, 0.0], [100.0, 50.0], 8.0)
            .subtract(Sdf::circle([50.0, 0.0], 10.0))
            .subtract(Sdf::circle([-50.0, 0.0], 10.0))
            .round(2.0);

        // Verify we can build complex shapes
        match shape.node() {
            SdfNode::Round { .. } => {}
            _ => panic!("Expected Round"),
        }
    }

    #[test]
    fn test_has_animation_static_nodes() {
        assert!(!Sdf::circle([0.0, 0.0], 10.0).node().has_animation());
        assert!(!Sdf::rect([0.0, 0.0], [10.0, 10.0]).node().has_animation());

        // Union of static shapes
        let shape = Sdf::circle([0.0, 0.0], 10.0) | Sdf::rect([0.0, 0.0], [10.0, 10.0]);
        assert!(!shape.node().has_animation());

        // Modifiers on static shapes
        assert!(!Sdf::circle([0.0, 0.0], 10.0).round(2.0).node().has_animation());
        assert!(!Sdf::circle([0.0, 0.0], 10.0).onion(1.0).node().has_animation());
    }

    #[test]
    fn test_has_animation_dash_with_speed() {
        let shape = Sdf::circle([0.0, 0.0], 50.0).dash(10.0, 5.0, 2.0, 0.0, 30.0);
        assert!(shape.node().has_animation());
    }

    #[test]
    fn test_has_animation_dash_zero_speed() {
        let shape = Sdf::circle([0.0, 0.0], 50.0).dash(10.0, 5.0, 2.0, 0.0, 0.0);
        assert!(!shape.node().has_animation());
    }

    #[test]
    fn test_has_animation_arrow_with_speed() {
        let shape = Sdf::circle([0.0, 0.0], 50.0).arrow(10.0, 5.0, 2.0, 0.5, 20.0);
        assert!(shape.node().has_animation());
    }

    #[test]
    fn test_has_animation_nested() {
        // Animated shape inside a union
        let animated = Sdf::circle([0.0, 0.0], 50.0).dash(10.0, 5.0, 2.0, 0.0, 30.0);
        let static_shape = Sdf::rect([0.0, 0.0], [10.0, 10.0]);
        let combined = animated | static_shape;
        assert!(combined.node().has_animation());

        // Animated shape inside round modifier
        let shape = Sdf::circle([0.0, 0.0], 50.0).dash(10.0, 5.0, 2.0, 0.0, 30.0).round(2.0);
        assert!(shape.node().has_animation());
    }
}
