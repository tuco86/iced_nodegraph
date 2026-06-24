//! Edge-drop investigation oracle (headless full-widget repro).
//!
//! A faithful stand-in for the `demo_500_nodes` missing-edge report: a dense grid
//! of real nodes wired by many edges, rendered through the REAL renderer so a test
//! can assert edges actually rasterize and that their coverage stays stable across
//! frames. It lives in its OWN test binary (separate process from `widget_pixel`)
//! so its heavy scene cannot corrupt the golden tests' light scene through the
//! frame-surviving pipeline state - see `common`.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_widget::core::clipboard;

use common::shared;
use iced_wgpu::Renderer;
use iced_wgpu::graphics::Viewport;

const GW: u32 = 600;
const GH: u32 = 400;

/// Faithful full-widget reproduction for the edge-drop investigation: a grid of
/// REAL nodes (each an Output pin on the right, an Input pin on the left) wired by
/// many edges, including long cross-graph ones. Edges are forced GREEN so edge
/// pixels are countable against the dark theme and node fills, which lets a test
/// assert edges actually rasterize (`edge_grid_edges_are_visible`) and that their
/// coverage stays stable across frames (`edge_grid_stable_across_frames`).
fn render_edge_grid() -> Option<Vec<[u8; 4]>> {
    use iced::widget::{Column, container, text};
    use iced_nodegraph::{
        ColorQuad, EdgeStyle, PinDirection, PinRef, PinSide, default_edge_style, edge, node,
        node_pin,
    };

    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Match the demo density: ~500 nodes spread over a large world, zoomed out to
    // fit the viewport ("see all 500 nodes"), wired by ~640 edges.
    let (cols, rows) = (25usize, 20usize);
    let n = cols * rows;
    let (sx, sy) = (90.0f32, 80.0f32);
    let zoom = (GW as f32 / (cols as f32 * sx)).min(GH as f32 / (rows as f32 * sy)) * 0.92;

    let mut graph: iced_nodegraph::NodeGraph<'static, usize, usize, (), (), Theme, Renderer> =
        iced_nodegraph::NodeGraph::default()
            .width(Length::Fixed(GW as f32))
            .height(Length::Fixed(GH as f32))
            .view(
                Point::new(
                    GW as f32 * 0.5 / zoom - cols as f32 * sx * 0.5,
                    GH as f32 * 0.5 / zoom - rows as f32 * sy * 0.5,
                ),
                zoom,
            );

    for i in 0..n {
        let (c, r) = ((i % cols) as f32, (i / cols) as f32);
        // Large node body (like the demo's title-bar + multi-pin nodes), so an
        // opaque fill can occlude edges running behind it.
        let pins = Column::with_children(vec![
            Element::from(
                node_pin(PinSide::Right, 0usize, text("o")).direction(PinDirection::Output),
            ),
            Element::from(
                node_pin(PinSide::Left, 1usize, text("i")).direction(PinDirection::Input),
            ),
        ]);
        let content: Element<'static, (), Theme, Renderer> = container(pins)
            .width(Length::Fixed(70.0))
            .height(Length::Fixed(60.0))
            .into();
        graph.push_node(node(i, Point::new(c * sx, r * sy), content));
    }

    // Heavy fan-out like the demo: many targets share a FEW source output pins,
    // so those source-pin tiles carry dozens of overlapping edge origins.
    let edge_count = 640usize;
    let sources = 16usize; // only this many distinct source pins feed everything
    for e in 0..edge_count {
        let from = e % sources;
        let to = sources + (e * 7 + 3) % (n - sources);
        if to == from {
            continue;
        }
        graph.push_edge(
            edge!(PinRef::new(from, 0usize), PinRef::new(to, 1usize)).style(
                |theme, status, _from, _to| EdgeStyle {
                    stroke_color: ColorQuad::solid(Color::from_rgb(0.0, 1.0, 0.0)),
                    ..default_edge_style(theme, status)
                },
            ),
        );
    }

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(GW as f32, GH as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(GW as f32, GH as f32));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_widget::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    graph.update(
        &mut tree,
        &iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        layout,
        mouse::Cursor::Unavailable,
        &*renderer,
        &mut clipboard,
        &mut shell,
        &viewport_rect,
    );

    graph.draw(
        &tree,
        renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport_rect,
    );

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(GW, GH), 1.0),
        Color::TRANSPARENT,
    );
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect(),
    )
}

/// Counts how many GREEN edge pixels the grid render produces. With ~72 green
/// edges spread across the frame, a healthy render has thousands; near-zero means
/// edges are being dropped. PRINTS the count for the edge-drop investigation.
fn green_count(px: &[[u8; 4]]) -> usize {
    px.iter()
        .filter(|p| {
            (p[1] as i32) > (p[0] as i32) + 40 && (p[1] as i32) > (p[2] as i32) + 40 && p[1] > 80
        })
        .count()
}

#[test]
fn edge_grid_edges_are_visible() {
    let Some(px) = render_edge_grid() else {
        eprintln!("no GPU adapter - skipping edge_grid_edges_are_visible");
        return;
    };
    let green = green_count(&px);
    eprintln!("edge_grid GREEN edge pixels: {green} / {}", px.len());
    assert!(
        green > 50,
        "almost no edges rendered: only {green} green pixels"
    );
}

/// The live demo renders MANY frames against a persistent `SdfPipeline` whose
/// frame-surviving state (the shape cache, the static-background texture cache) is
/// exactly what a single-frame test cannot exercise. This renders the same scene
/// repeatedly through the shared renderer/pipeline and asserts the edge coverage
/// stays stable - if the pipeline drops edges on later frames, that is the
/// zoom-independent live-demo failure the corpus never caught.
#[test]
fn edge_grid_stable_across_frames() {
    let mut counts = Vec::new();
    for _ in 0..6 {
        let Some(px) = render_edge_grid() else {
            eprintln!("no GPU adapter - skipping edge_grid_stable_across_frames");
            return;
        };
        counts.push(green_count(&px));
    }
    eprintln!("edge_grid GREEN per frame: {counts:?}");
    let first = counts[0];
    for (i, &c) in counts.iter().enumerate() {
        assert!(
            (c as i64 - first as i64).abs() < (first as i64) / 20 + 200,
            "edge coverage drifted on frame {i}: {c} vs frame0 {first} (cross-frame state bug?)",
        );
    }
}
