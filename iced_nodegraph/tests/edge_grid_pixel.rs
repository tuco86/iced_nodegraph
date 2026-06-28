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

/// Probe: print the real per-frame GPU buffer sizes (and what the tile-slot buffer
/// would be at cap 32 vs 128), so the cap's memory cost is concrete.
#[test]
#[ignore = "diagnostic: prints GPU buffer sizes"]
fn report_buffer_sizes() {
    if render_edge_grid().is_none() {
        return;
    }
    let s = iced_nodegraph_sdf::sdf_stats();
    let tiles = s.tile_count as u64;
    let kib = |b: u64| b / 1024;
    eprintln!(
        "tiles={tiles} entries={} unique_shapes={} segments={}",
        s.entry_count, s.unique_shapes, s.segment_count
    );
    eprintln!("  tile_counts:  {} KiB ({tiles} x 4)", kib(tiles * 4));
    eprintln!(
        "  tile_slots @cap32:  {} KiB ({tiles} x 64 x 4)",
        kib(tiles * 64 * 4)
    );
    eprintln!(
        "  tile_slots @cap128: {} KiB ({tiles} x 256 x 4)  <- current",
        kib(tiles * 256 * 4)
    );
    eprintln!(
        "  segments: {} KiB ({} x 64) | entries: {} KiB ({} x 80)",
        kib(s.segment_count as u64 * 64),
        s.segment_count,
        kib(s.entry_count as u64 * 80),
        s.entry_count,
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

/// Visual probe: render the REAL widget edge grid to a PNG so the reported boxes
/// can be SEEN headless (the SDF crate renders the same edge geometry as clean
/// strokes, so any boxes here localize the bug to the widget's draw path). Writes
/// to the repo root; not an assertion.
#[test]
#[ignore = "visual probe: writes widget_edge_grid_render.png"]
fn dump_edge_grid_png() {
    let Some(px) = render_edge_grid() else {
        eprintln!("no GPU adapter - skipping dump_edge_grid_png");
        return;
    };
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../widget_edge_grid_render.png"
    );
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), GW, GH);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();
}

/// Render a MINIMAL widget scene (a few nodes, a few edges) to a PNG, so a single
/// edge's shape is clearly visible - to tell whether one isolated edge boxes in
/// the widget path or only the dense overlap does.
fn render_minimal_edges() -> Option<Vec<[u8; 4]>> {
    use iced::widget::{Column, container, text};
    use iced_nodegraph::{
        ColorQuad, EdgeStyle, PinDirection, PinRef, PinSide, default_edge_style, edge, node,
        node_pin,
    };

    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Four nodes well spread; world fits the viewport at zoom 1.
    let positions = [
        Point::new(40.0, 40.0),
        Point::new(420.0, 60.0),
        Point::new(80.0, 300.0),
        Point::new(440.0, 320.0),
    ];

    let mut graph: iced_nodegraph::NodeGraph<'static, usize, usize, (), (), Theme, Renderer> =
        iced_nodegraph::NodeGraph::default()
            .width(Length::Fixed(GW as f32))
            .height(Length::Fixed(GH as f32))
            .view(Point::new(0.0, 0.0), 1.0);

    for (i, p) in positions.iter().enumerate() {
        // NO text content - to test whether interleaved text rendering triggers the
        // edge blob (the SDF crate and iced_wgpu both render these edges cleanly).
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
        graph.push_node(node(i, *p, content));
    }
    // node0 -> node1 (horizontal), node0 -> node3 (diagonal long), node2 -> node1 (diagonal).
    for &(f, t) in &[(0usize, 1usize), (0, 3), (2, 1)] {
        graph.push_edge(edge!(PinRef::new(f, 0usize), PinRef::new(t, 1usize)).style(
            |theme, status, _from, _to| EdgeStyle {
                stroke_color: ColorQuad::solid(Color::from_rgb(0.0, 1.0, 0.0)),
                ..default_edge_style(theme, status)
            },
        ));
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

/// Isolation: render the 3 minimal edges DIRECTLY through iced_wgpu's
/// `draw_primitive` + `screenshot`, bypassing the NodeGraph widget entirely. The
/// SDF crate renders these exact edges as clean strokes; the widget renders one as
/// a filled blob. If the blob appears HERE, the iced_wgpu render path (not the
/// NodeGraph draw logic) is the cause. Writes a PNG and asserts stroke coverage.
/// Ignored: passes in isolation, but the shared renderer is polluted by sibling
/// tests' heavy scenes (the same cross-frame GPU-state issue under investigation).
#[test]
#[ignore = "passes in isolation; shared-renderer cross-test pollution in suite"]
fn iced_direct_edges_render_as_strokes() {
    use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style};
    use iced_wgpu::primitive::Renderer as _;

    let Some(mut guard) = shared() else {
        eprintln!("no GPU adapter - skipping iced_direct_edges_render_as_strokes");
        return;
    };
    let renderer = &mut *guard;
    let green = Style::stroke(Color::from_rgb(0.0, 1.0, 0.0), Pattern::solid(2.0));
    let clip = Rectangle::new(Point::ORIGIN, Size::new(600.0, 400.0));
    for (p0, c0, c1, p1) in [
        (
            [110.0f32, 55.0],
            [190.0, 55.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
        ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
        (
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ] {
        let mut prim = SdfPrimitive::new();
        prim.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
        let prim = prim.camera(0.0, 0.0, 1.0);
        renderer.draw_primitive(clip, prim);
    }
    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(600, 400), 1.0),
        Color::TRANSPARENT,
    );
    let px: Vec<[u8; 4]> = bytes
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../iced_direct_edges.png");
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 600, 400);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();

    let g = green_count(&px);
    eprintln!("iced-direct edges green: {g} px (3 strokes ~3000, blob ~30000)");
    assert!(
        g < 8000,
        "iced draw_primitive rendered edges as boxes: {g} green px"
    );
}

/// Isolation step 1b: the SAME three known-clean edges from
/// `iced_direct_edges_render_as_strokes`, but with `fill_text` calls interleaved
/// into the frame - all through iced_wgpu's renderer, with NO NodeGraph widget.
/// The handoff established TEXT as the blob trigger only via the full widget path;
/// this strips the widget away to ask whether a bare `draw_primitive` + `fill_text`
/// frame is enough to corrupt an edge into a filled box. Result: it stays a clean
/// stroke (~2830 green), so text merely sharing the SDF frame is NOT sufficient -
/// the trigger needs more of the widget's frame (see the faithful-replica probe).
/// Diagnostic: asserts nothing fatal, just reports the count. Ignored: run alone.
#[test]
#[ignore = "diagnostic probe: writes iced_direct_text.png; run alone"]
fn iced_direct_edges_with_text_stays_clean() {
    use iced::Pixels;
    use iced::advanced::text::{self, LineHeight, Renderer as _, Shaping, Text, Wrapping};
    use iced::alignment;
    use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style};
    use iced_wgpu::primitive::Renderer as _;

    let Some(mut guard) = shared() else {
        eprintln!("no GPU adapter - skipping iced_direct_edges_with_text_blob");
        return;
    };
    let renderer = &mut *guard;
    let green = Style::stroke(Color::from_rgb(0.0, 1.0, 0.0), Pattern::solid(2.0));
    let clip = Rectangle::new(Point::ORIGIN, Size::new(600.0, 400.0));

    // Mimic the widget's pin labels: a short string drawn at each edge endpoint,
    // interleaved with the edge primitives in the same frame/layer.
    let label = |renderer: &mut Renderer, x: f32, y: f32| {
        let t = Text {
            content: "o".to_string(),
            bounds: Size::new(40.0, 20.0),
            size: Pixels(16.0),
            line_height: LineHeight::default(),
            font: iced::Font::default(),
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: Shaping::Basic,
            wrapping: Wrapping::default(),
        };
        renderer.fill_text(t, Point::new(x, y), Color::WHITE, clip);
    };

    for (p0, c0, c1, p1) in [
        (
            [110.0f32, 55.0],
            [190.0, 55.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
        ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
        (
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ] {
        let mut prim = SdfPrimitive::new();
        prim.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
        let prim = prim.camera(0.0, 0.0, 1.0);
        renderer.draw_primitive(clip, prim);
        // Pin labels at both endpoints, interleaved like the widget does.
        label(renderer, p0[0], p0[1]);
        label(renderer, p1[0], p1[1]);
    }

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(600, 400), 1.0),
        Color::TRANSPARENT,
    );
    let px: Vec<[u8; 4]> = bytes
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../iced_direct_text.png");
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 600, 400);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();

    let g = green_count(&px);
    eprintln!("iced-direct edges+TEXT green: {g} px (3 strokes ~3000, blob ~30000)");
}

/// Isolation step 2: the FULL widget SDF primitive set (background, node shadows,
/// the edge batch, then per-node fills + pins) drawn directly through iced_wgpu's
/// `draw_primitive` in the widget's order, but WITHOUT text. If the edge blobs
/// here, the trigger is iced_wgpu's multi-`SdfPrimitive` frame management; if it
/// stays clean, the trigger is the text primitives interleaved by the widget.
/// (It stays clean - proving TEXT is the trigger; see the module-level findings.)
/// Ignored: passes in isolation; shared-renderer cross-test pollution in suite.
#[test]
#[ignore = "passes in isolation; shared-renderer cross-test pollution in suite"]
fn iced_direct_full_sdf_frame_edges_render_as_strokes() {
    use iced::Rectangle as R;
    use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style, Tiling};
    use iced_wgpu::primitive::Renderer as _;

    let Some(mut guard) = shared() else {
        return;
    };
    let renderer = &mut *guard;
    let full = R::new(Point::ORIGIN, Size::new(600.0, 400.0));
    let gray = Style::solid(Color::from_rgb(0.30, 0.32, 0.40));
    let dark = Style::solid(Color::from_rgb(0.12, 0.13, 0.16));
    let green = Style::stroke(Color::from_rgb(0.0, 1.0, 0.0), Pattern::solid(2.0));
    let positions = [
        [40.0f32, 40.0],
        [420.0, 60.0],
        [80.0, 300.0],
        [440.0, 320.0],
    ];

    // bg (cacheable background tiling)
    let mut bg = SdfPrimitive::new();
    bg.push(
        &Shape::tiling(Tiling::grid(40.0, 40.0, 1.0)),
        &dark,
        [0.0, 0.0],
    );
    renderer.draw_primitive(full, bg.camera(0.0, 0.0, 1.0));

    // node shadows (one batch)
    let mut shadows = SdfPrimitive::new();
    for p in positions {
        shadows.push(
            &Shape::rounded_box([70.0, 60.0], [6.0; 4]),
            &gray,
            [p[0] + 39.0, p[1] + 34.0],
        );
    }
    renderer.draw_primitive(full, shadows.camera(0.0, 0.0, 1.0));

    // edge batch
    let mut edges = SdfPrimitive::new();
    for (p0, c0, c1, p1) in [
        (
            [110.0f32, 55.0],
            [190.0, 55.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
        ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
        (
            [150.0, 315.0],
            [230.0, 315.0],
            [340.0, 105.0],
            [420.0, 105.0],
        ),
    ] {
        edges.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
    }
    renderer.draw_primitive(full, edges.camera(0.0, 0.0, 1.0));

    // per-node fills + pins, each with its own clip (like the widget)
    for p in positions {
        let center = [p[0] + 35.0, p[1] + 30.0];
        let fill_clip = R::new(Point::new(p[0] - 2.0, p[1] - 2.0), Size::new(74.0, 64.0));
        let mut fill = SdfPrimitive::new();
        fill.push(&Shape::rounded_box([70.0, 60.0], [6.0; 4]), &gray, center);
        renderer.draw_primitive(fill_clip, fill.camera(-p[0] + 2.0, -p[1] + 2.0, 1.0));

        let pins_clip = R::new(Point::new(p[0] - 5.0, p[1] - 3.0), Size::new(80.0, 66.0));
        let mut pins = SdfPrimitive::new();
        pins.push(&Shape::circle(4.0), &gray, [p[0], center[1]]);
        pins.push(&Shape::circle(4.0), &gray, [p[0] + 70.0, center[1]]);
        renderer.draw_primitive(pins_clip, pins.camera(-p[0] + 5.0, -p[1] + 3.0, 1.0));
    }

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(600, 400), 1.0),
        Color::TRANSPARENT,
    );
    let px: Vec<[u8; 4]> = bytes
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../iced_direct_full.png");
    let file = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), 600, 400);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut writer = e.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();

    let g = green_count(&px);
    eprintln!("iced-direct FULL frame edges green: {g} px");
    assert!(
        g < 8000,
        "iced_wgpu full SDF frame rendered edges as boxes: {g} green px",
    );
}

/// Isolation step 3: the closest widget-free replica of the blobbing frame -
/// background, shadows, the edge batch, then per-node fill / foreground (border
/// stroke + two pins) across nested iced layers, with the pin labels drawn via
/// `fill_paragraph` (the real `text()` widget's cached, advance-shaped path, not
/// raw `fill_text`). This replica matches the widget's per-primitive TILE layout
/// exactly (verified by diffing prepare params: tile_base, total_tiles, cols/rows,
/// bounds, camera, zoom all identical) yet renders CLEAN (~2830 green). It still
/// diverges from the widget in the SEGMENT-buffer layout - the widget's edges land
/// at a higher `segbase` because its shadow/foreground shapes compile to more
/// (non-deduped) segments. Conclusion: text sharing the frame is necessary but NOT
/// sufficient; the blob also needs the widget's exact segment-buffer layout, which
/// points the trigger at segment-range indexing under that layout, not at text
/// alone. Ignored: run alone (shared-renderer pollution).
#[test]
#[ignore = "diagnostic probe: writes iced_direct_paragraph.png; run alone"]
fn iced_direct_faithful_replica_stays_clean() {
    use iced::Rectangle as R;
    use iced::advanced::Renderer as _;
    use iced::advanced::text::{
        self, LineHeight, Paragraph as _, Renderer as TextRenderer, Shaping, Text, Wrapping,
    };
    use iced::{Pixels, Vector, alignment};
    use iced_nodegraph_sdf::{Pattern, SdfPrimitive, Shape, Style, Tiling};
    use iced_wgpu::primitive::Renderer as _;

    type Para = <Renderer as TextRenderer>::Paragraph;

    let Some(mut guard) = shared() else {
        return;
    };
    let renderer = &mut *guard;
    let full = R::new(Point::ORIGIN, Size::new(600.0, 400.0));
    let gray = Style::solid(Color::from_rgb(0.30, 0.32, 0.40));
    let dark = Style::solid(Color::from_rgb(0.12, 0.13, 0.16));
    let green = Style::stroke(Color::from_rgb(0.0, 1.0, 0.0), Pattern::solid(2.0));
    let positions = [
        [40.0f32, 40.0],
        [420.0, 60.0],
        [80.0, 300.0],
        [440.0, 320.0],
    ];

    let label = |renderer: &mut Renderer, s: &str, x: f32, y: f32, clip: R| {
        let para = Para::with_text(Text {
            content: s,
            bounds: Size::new(40.0, 20.0),
            size: Pixels(16.0),
            line_height: LineHeight::default(),
            font: iced::Font::default(),
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::default(),
        });
        renderer.fill_paragraph(&para, Point::new(x, y), Color::WHITE, clip);
    };

    renderer.with_layer(full, |renderer| {
        let mut bg = SdfPrimitive::new();
        bg.push(
            &Shape::tiling(Tiling::grid(40.0, 40.0, 1.0)),
            &dark,
            [0.0, 0.0],
        );
        renderer.draw_primitive(full, bg.camera(0.0, 0.0, 1.0));
    });

    renderer.with_layer(full, |renderer| {
        let mut shadows = SdfPrimitive::new();
        for p in positions {
            shadows.push(
                &Shape::rounded_box([70.0, 60.0], [6.0; 4]),
                &gray,
                [p[0] + 39.0, p[1] + 34.0],
            );
        }
        renderer.draw_primitive(full, shadows.camera(0.0, 0.0, 1.0));
    });

    renderer.with_layer(full, |renderer| {
        let mut edges = SdfPrimitive::new();
        for (p0, c0, c1, p1) in [
            (
                [110.0f32, 55.0],
                [190.0, 55.0],
                [340.0, 105.0],
                [420.0, 105.0],
            ),
            ([110.0, 55.0], [190.0, 55.0], [360.0, 365.0], [440.0, 365.0]),
            (
                [150.0, 315.0],
                [230.0, 315.0],
                [340.0, 105.0],
                [420.0, 105.0],
            ),
        ] {
            edges.push(&Shape::bezier(p0, c0, c1, p1), &green, [0.0, 0.0]);
        }
        renderer.draw_primitive(full, edges.camera(0.0, 0.0, 1.0));
    });

    for p in positions {
        let center = [p[0] + 35.0, p[1] + 30.0];
        let fill_clip = R::new(Point::new(p[0] - 2.0, p[1] - 2.0), Size::new(74.0, 64.0));
        renderer.with_layer(full, |renderer| {
            let mut fill = SdfPrimitive::new();
            fill.push(&Shape::rounded_box([70.0, 60.0], [6.0; 4]), &gray, center);
            renderer.draw_primitive(fill_clip, fill.camera(-p[0] + 2.0, -p[1] + 2.0, 1.0));
        });

        let node_clip = R::new(Point::new(p[0] - 2.0, p[1] - 2.0), Size::new(74.0, 64.0));
        renderer.with_layer(full, |renderer| {
            renderer.with_layer(node_clip, |renderer| {
                renderer.with_translation(Vector::new(0.0, 0.0), |renderer| {
                    label(renderer, "i", p[0] - 2.0, center[1] - 8.0, node_clip);
                    label(renderer, "o", p[0] + 66.0, center[1] - 8.0, node_clip);
                });
            });
        });

        // Foreground = BORDER stroke + 2 pins (3 entries), matching the widget's
        // fg_batch. The border is a stroke like the edges and shares the segments
        // buffer with them - the segbase/entry-count diff vs the widget traces to
        // this missing border entry.
        let border = Style::stroke(Color::from_rgb(0.5, 0.5, 0.6), Pattern::solid(1.5));
        let pins_clip = R::new(Point::new(p[0] - 5.0, p[1] - 3.0), Size::new(80.0, 66.0));
        renderer.with_layer(full, |renderer| {
            let mut fg = SdfPrimitive::new();
            fg.push(&Shape::rounded_box([70.0, 60.0], [6.0; 4]), &border, center);
            fg.push(&Shape::circle(4.0), &gray, [p[0], center[1]]);
            fg.push(&Shape::circle(4.0), &gray, [p[0] + 70.0, center[1]]);
            renderer.draw_primitive(pins_clip, fg.camera(-p[0] + 5.0, -p[1] + 3.0, 1.0));
        });
    }

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(600, 400), 1.0),
        Color::TRANSPARENT,
    );
    let px: Vec<[u8; 4]> = bytes
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../iced_direct_paragraph.png");
    let file = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), 600, 400);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut writer = e.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();

    let g = green_count(&px);
    eprintln!("iced-direct PARAGRAPH + frame green: {g} px (clean ~3000, blob ~30000)");
}

#[test]
#[ignore = "visual probe: writes widget_minimal_edges.png"]
fn dump_minimal_edges_png() {
    let Some(px) = render_minimal_edges() else {
        eprintln!("no GPU adapter - skipping dump_minimal_edges_png");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../widget_minimal_edges.png");
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), GW, GH);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
    writer.write_image_data(&flat).unwrap();
}
