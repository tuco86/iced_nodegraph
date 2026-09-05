//! Full-widget GPU pixel oracle.
//!
//! The SDF crate's `pixel_tests` render SDF primitives in isolation; the unit
//! `coordinate_tests` drive the full `NodeGraph::draw` path but through a MOCK
//! recording renderer (draw-call rects, no rasterization). Neither rasterizes the
//! whole widget - SDF layers AND hosted iced content (text) - to real pixels.
//!
//! This harness does: it drives `NodeGraph::draw` through the REAL
//! `iced_wgpu::Renderer` headlessly (via the shared `common` harness) and reads
//! back the framebuffer via `Renderer::screenshot`. That is the oracle the plan
//! calls for ("the golden harness must drive the FULL widget path ... with a real
//! text+caret node"), and the prerequisite for pixel-gating widget-level Phase C
//! work (layer collapse, static-background cache) without a human in the loop.
//!
//! Tests that need a GPU adapter skip gracefully when none is present (CI without
//! a GPU), exactly like a developer running headless.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::widget::text;
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_wgpu::core::clipboard;

use common::shared;
use iced_nodegraph::{ColorQuad, NodeGraph, NodeStyle, default_node_style, node};
use iced_wgpu::Renderer;
use iced_wgpu::graphics::Viewport;

const W: u32 = 320;
const H: u32 = 240;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RouteIds;

impl iced_nodegraph::Ids for RouteIds {
    type NodeId = usize;
    type PinId = usize;
    type EdgeId = usize;
    type AnchorId = usize;
    type Payload = ();
}

/// Render a one-node graph (node carries hosted text content) to RGBA pixels.
/// Returns `None` if no GPU is available.
fn render_one_node() -> Option<Vec<[u8; 4]>> {
    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Camera centred so the node (world origin) lands mid-viewport at zoom 1.
    let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .camera(
                Point::new(W as f32 * 0.5 - 40.0, H as f32 * 0.5 - 20.0),
                1.0,
            );
    graph = graph.push_node(node(
        0_usize,
        Point::new(0.0, 0.0),
        Element::from(text("Hi")),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));

    // One update syncs the controlled `camera()` into the widget camera.
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
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
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// The full widget rasterizes end-to-end: a full-bounds background PLUS a
/// localized node (fill/border/hosted text) on top. The node is detected against
/// the background - the most-frequent colour is the background; it must dominate
/// (so the frame is not full-screen garbage) yet not cover EVERYTHING (so the
/// node actually drew). Several distinct colours (bg + fill + border + text)
/// rule out a blank or single-colour frame.
#[test]
fn full_widget_renders_localized_node() {
    use std::collections::HashMap;

    let Some(px) = render_one_node() else {
        eprintln!("no GPU adapter - skipping full_widget_renders_localized_node");
        return;
    };
    assert_eq!(px.len(), (W * H) as usize);

    let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
    for p in &px {
        *counts.entry(*p).or_default() += 1;
    }
    let distinct = counts.len();
    let (&bg, &bg_count) = counts.iter().max_by_key(|(_, c)| **c).unwrap();
    let bg_frac = bg_count as f32 / px.len() as f32;

    assert!(
        distinct > 3,
        "near-uniform frame ({distinct} distinct colours): background or node \
         failed to render",
    );
    assert!(
        bg_frac < 0.97,
        "node did not render: background colour {bg:?} covers {:.1}% of the frame",
        bg_frac * 100.0,
    );
    assert!(
        bg_frac > 0.30,
        "full-screen garbage: no dominant background (top colour only {:.1}%)",
        bg_frac * 100.0,
    );
}

/// Render a zoom-out node GRID through the real widget and count nodes whose red
/// body fill is missing or mis-sized at its expected screen centre. `frames` runs
/// the SAME scene through the SHARED (persistent) pipeline that many times before
/// scoring the LAST frame: a static camera repeated across frames drives the
/// background cache Direct -> Populate -> Blit, the cross-frame state the live app
/// hits but a single render never does.
fn zoomout_grid_missing_nodes(scale: f32, frames: u32) -> Option<usize> {
    zoomout_grid_missing_nodes_at(scale, frames, Point::new(-327.7, -132.0), 0.24131)
}

fn zoomout_grid_missing_nodes_at(scale: f32, frames: u32, cam: Point, zoom: f32) -> Option<usize> {
    use iced::widget::container;
    use iced::widget::text;

    // Logical viewport; physical = logical * scale (DPI).
    const GW: u32 = 640;
    const GH: u32 = 480;
    let pw = (GW as f32 * scale) as u32;
    let ph = (GH as f32 * scale) as u32;

    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Camera-relative node grid: a FIXED screen lattice (top-left screen px),
    // with each node's WORLD position derived from the current camera so the grid
    // always fills the viewport at any pan offset. This models panning across
    // content - the node's world coordinate AND cam both vary, the way they do
    // when the user pans right looking for the flicker position.
    let nw = 60.0_f32;
    let nh = 40.0_f32;
    let lattice: Vec<(f32, f32)> = {
        let mut v = Vec::new();
        let mut tly = 30.0;
        while tly < GH as f32 - 30.0 {
            let mut tlx = 30.0;
            while tlx < GW as f32 - 30.0 {
                v.push((tlx, tly));
                tlx += 42.0;
            }
            tly += 38.0;
        }
        v
    };
    // world top-left = screen_topleft/zoom - cam; screen centre = topleft + body/2.
    let world_of = |tlx: f32, tly: f32| (tlx / zoom - cam.x, tly / zoom - cam.y);
    let centers: Vec<(f32, f32)> = lattice
        .iter()
        .map(|&(tlx, tly)| (tlx + nw * zoom * 0.5, tly + nh * zoom * 0.5))
        .collect();

    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(GW as f32, GH as f32));
    let mut px: Vec<[u8; 4]> = Vec::new();
    for _ in 0..frames.max(1) {
        // Rebuild the camera each frame, exactly as a live app does.
        let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
            NodeGraph::default()
                .width(Length::Fixed(GW as f32))
                .height(Length::Fixed(GH as f32))
                .camera(cam, zoom);
        for (id, &(tlx, tly)) in lattice.iter().enumerate() {
            let (wx, wy) = world_of(tlx, tly);
            graph = graph.push_node(
                node(
                    id,
                    Point::new(wx, wy),
                    Element::from(
                        container(text(""))
                            .width(Length::Fixed(nw))
                            .height(Length::Fixed(nh)),
                    ),
                )
                .style(|theme, status| NodeStyle {
                    fill_color: ColorQuad::solid(Color::from_rgb(1.0, 0.0, 0.0)),
                    ..default_node_style(theme, status)
                }),
            );
        }

        let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
        let layout_node = graph.layout(
            &mut tree,
            &*renderer,
            &layout::Limits::new(Size::ZERO, Size::new(GW as f32, GH as f32)),
        );
        let layout = Layout::new(&layout_node);

        let mut msgs: Vec<()> = Vec::new();
        let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
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
            &Viewport::with_physical_size(Size::new(pw, ph), scale),
            Color::TRANSPARENT,
        );
        px = bytes.as_chunks::<4>().0.to_vec();
    }

    let is_red = |p: &[u8; 4]| p[0] > 120 && p[1] < 90 && p[2] < 90;
    // Expected node body size in PHYSICAL pixels at this camera.
    let exp_w = nw * zoom * scale;
    let exp_h = nh * zoom * scale;
    let mut missing = 0usize;
    for (scx, scy) in &centers {
        // Only score nodes whose centre lands comfortably inside the viewport.
        if *scx < 16.0 || *scy < 16.0 || *scx > (GW as f32 - 16.0) || *scy > (GH as f32 - 16.0) {
            continue;
        }
        // Sample a generous window (PHYSICAL px) around the expected centre and
        // measure the red bounding box - both presence AND size. A "collapsed"
        // node is empty, shrunk, or blown up well past its true footprint.
        let cx = (*scx * scale) as i32;
        let cy = (*scy * scale) as i32;
        let win = (24.0 * scale) as i32;
        let (mut rminx, mut rminy, mut rmaxx, mut rmaxy) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut red = 0;
        for dy in -win..=win {
            for dx in -win..=win {
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= pw as i32 || y >= ph as i32 {
                    continue;
                }
                if is_red(&px[(y as u32 * pw + x as u32) as usize]) {
                    red += 1;
                    rminx = rminx.min(x);
                    rminy = rminy.min(y);
                    rmaxx = rmaxx.max(x);
                    rmaxy = rmaxy.max(y);
                }
            }
        }
        if red < 4 {
            missing += 1;
            continue;
        }
        let bw = (rmaxx - rminx + 1) as f32;
        let bh = (rmaxy - rminy + 1) as f32;
        // Flag a node whose red footprint is far from its true body size in either
        // axis (shrunk to a speck or ballooned across neighbours).
        let wrong_size = bw < exp_w * 0.5
            || bh < exp_h * 0.5
            || bw > exp_w * 2.2 + 6.0
            || bh > exp_h * 2.2 + 6.0;
        if wrong_size {
            missing += 1;
        }
    }
    Some(missing)
}

#[test]
fn zoomout_grid_all_nodes_render() {
    for scale in [1.0_f32, 1.5, 2.0] {
        let Some(missing) = zoomout_grid_missing_nodes(scale, 1) else {
            eprintln!("no GPU adapter - skipping zoomout_grid_all_nodes_render");
            return;
        };
        assert_eq!(
            missing, 0,
            "{missing} node fills did not render at zoom 0.24, scale {scale} \
             (zoom-out float collapse)",
        );
    }
}

/// Root-cause repro for the pan-dependent washed nodes (fill over text, no
/// border/pins): iced PREPARES every custom-primitive instance but SKIPS drawing
/// the ones whose bounds snap empty / fall off the viewport. The SDF pipeline pairs
/// prepare-order to draw-order with a draw counter, so ONE skipped node desyncs the
/// `DrawData` index of every later node - they then read the wrong camera/tiles and
/// misrender. Here node 0 sits off the right of the framebuffer (graph wider than
/// the screenshot viewport) so iced skips drawing it; the fully-visible nodes that
/// follow must still render their fill AND text.
#[test]
fn offscreen_node_does_not_desync_later_nodes() {
    use iced::widget::{container, text};

    // Graph wider than the framebuffer so a node can sit off-viewport-right.
    const GW: u32 = 800;
    const GH: u32 = 320;
    const VW: u32 = 640; // screenshot viewport (framebuffer) width
    let zoom = 1.0_f32;
    let cam = Point::new(0.0, 0.0);
    let nw = 80.0_f32;
    let nh = 44.0_f32;

    let Some(mut guard) = shared() else {
        eprintln!("no GPU adapter - skipping offscreen_node_does_not_desync_later_nodes");
        return;
    };
    let renderer = &mut *guard;

    // Node 0 is off the right of the framebuffer (x = 700, framebuffer is 640 wide)
    // but inside the 800-wide graph, so it is PREPARED yet skipped in render.
    // Nodes 1..=3 are fully visible and submitted after it.
    let visible = [(60.0_f32, 120.0_f32), (240.0, 120.0), (420.0, 120.0)];
    let mut worlds = vec![(700.0_f32, 120.0_f32)];
    worlds.extend_from_slice(&visible);

    let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(GW as f32))
            .height(Length::Fixed(GH as f32))
            .camera(cam, zoom);
    for (id, &(wx, wy)) in worlds.iter().enumerate() {
        graph = graph.push_node(
            node(
                id,
                Point::new(wx, wy),
                Element::from(
                    container(text("Xy"))
                        .width(Length::Fixed(nw))
                        .height(Length::Fixed(nh)),
                ),
            )
            .style(|theme, status| NodeStyle {
                fill_color: ColorQuad::solid(Color::from_rgb(0.10, 0.15, 0.55)),
                shadow_distance: 0.0,
                shadow_color: Color::TRANSPARENT,
                ..default_node_style(theme, status)
            }),
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
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
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
        &Viewport::with_physical_size(Size::new(VW, GH), 1.0),
        Color::from_rgb(0.0, 0.0, 0.0),
    );
    let px: Vec<[u8; 4]> = bytes.as_chunks::<4>().0.to_vec();

    let is_fill = |p: &[u8; 4]| p[2] > 90 && p[2] as i32 > p[0] as i32 + 30;
    let is_text = |p: &[u8; 4]| p[0] > 170 && p[1] > 170 && p[2] > 170;
    let mut broken = Vec::new();
    for (i, &(wx, wy)) in visible.iter().enumerate() {
        let cx = ((wx + nw * 0.5 + cam.x) * zoom) as i32;
        let cy = ((wy + nh * 0.5 + cam.y) * zoom) as i32;
        let (mut fill_px, mut text_px) = (0, 0);
        for dy in -20..=20i32 {
            for dx in -36..=36i32 {
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= VW as i32 || y >= GH as i32 {
                    continue;
                }
                let p = &px[(y as u32 * VW + x as u32) as usize];
                if is_fill(p) {
                    fill_px += 1;
                }
                if is_text(p) {
                    text_px += 1;
                }
            }
        }
        if fill_px < 100 || text_px < 3 {
            broken.push((i + 1, fill_px, text_px));
        }
    }
    assert!(
        broken.is_empty(),
        "off-viewport node 0 desynced later nodes' DrawData: broken (node, fill_px, text_px) = {broken:?}",
    );
}

/// The full-widget render is deterministic: a static graph produces byte-
/// identical pixels across renders. This is the property layer-collapse and the
/// background-texture cache must preserve, and what makes a golden image stable.
#[test]
fn full_widget_render_is_deterministic() {
    let Some(a) = render_one_node() else {
        eprintln!("no GPU adapter - skipping full_widget_render_is_deterministic");
        return;
    };
    let b = render_one_node().expect("GPU was available a moment ago");
    let differ = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    assert_eq!(differ, 0, "full-widget render flickered on {differ} pixels");
}

/// Writes idle and selected renders of the same node side by side, for eyeballing
/// the theme-derived selection default.
#[test]
#[ignore = "visual probe: writes selected_node_idle.png / selected_node_sel.png"]
fn probe_selected_node_appearance() {
    for (selected, name) in [(false, "idle"), (true, "sel")] {
        let Some(px) = render_node_selection(selected) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let path = format!("selected_node_{name}.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let flat: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
        w.write_image_data(&flat).unwrap();
        eprintln!("wrote {path}");
    }
}

/// One node, optionally selected via the controlled `selection()` channel.
fn render_node_selection(selected: bool) -> Option<Vec<[u8; 4]>> {
    let mut guard = shared()?;
    let renderer = &mut *guard;

    let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .camera(
                Point::new(W as f32 * 0.5 - 40.0, H as f32 * 0.5 - 20.0),
                1.0,
            );
    // A realistically sized body: the halo is judged relative to the node, and a
    // bare text label is an order of magnitude smaller than a real node.
    graph = graph.push_node(
        node(
            0_usize,
            Point::new(0.0, 0.0),
            Element::from(
                iced::widget::container(text("Node"))
                    .width(Length::Fixed(160.0))
                    .height(Length::Fixed(90.0)),
            ),
        )
        .selected(selected),
    );

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
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
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// One node whose single Output pin is drawn as `shape` in an unmistakable
/// magenta, big enough that its footprint spans tens of pixels.
fn render_pin_shape(shape: iced_nodegraph::PinShape) -> Option<Vec<[u8; 4]>> {
    use iced_nodegraph::{PinDirection, PinSide, PinStyle, default_pin_style, node_pin};

    let mut guard = shared()?;
    let renderer = &mut *guard;

    let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .camera(Point::ORIGIN, 1.0);
    graph = graph.push_node(
        node(
            0_usize,
            Point::new(90.0, 70.0),
            Element::from(
                iced::widget::container(
                    node_pin(PinSide::Left, 0_usize, text("")).direction(PinDirection::Output),
                )
                .width(Length::Fixed(120.0))
                .height(Length::Fixed(80.0)),
            ),
        )
        .pin_style(move |theme, _pin, _peer, status| PinStyle {
            color: Color::from_rgb(1.0, 0.0, 1.0).into(),
            radius: 20.0,
            shape,
            // A cutout or a border would add geometry outside the indicator and
            // blur the footprint this test measures.
            cutout_radius: 0.0,
            border_width: 0.0,
            ..default_pin_style(theme, status)
        }),
    );

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
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
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// Fraction of the magenta footprint's bounding box that the footprint covers.
/// A disc fills PI/4 of its box; an axis-aligned square fills all of it.
fn magenta_box_fill(px: &[[u8; 4]]) -> f32 {
    let hit = |p: &[u8; 4]| p[0] > 200 && p[2] > 200 && p[1] < 60;
    let (mut x0, mut y0, mut x1, mut y1) = (W, H, 0u32, 0u32);
    let mut count = 0u32;
    for (i, p) in px.iter().enumerate() {
        if !hit(p) {
            continue;
        }
        let (x, y) = (i as u32 % W, i as u32 / W);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
        count += 1;
    }
    assert!(count > 300, "pin indicator did not render ({count} px)");
    count as f32 / ((x1 - x0 + 1) * (y1 - y0 + 1)) as f32
}

/// `PinShape` is a rendering contract, not a label: every variant must reach
/// the GPU as its own silhouette. Circle and Square are drawn at equal area, so
/// only the footprint's shape tells them apart - a disc leaves its bounding
/// box's corners empty, a square fills them.
#[test]
fn every_pin_shape_draws_its_own_silhouette() {
    use iced_nodegraph::PinShape;

    let Some(circle) = render_pin_shape(PinShape::Circle) else {
        eprintln!("no GPU adapter - skipping every_pin_shape_draws_its_own_silhouette");
        return;
    };
    let square = render_pin_shape(PinShape::Square).expect("GPU was available a moment ago");

    let circle_fill = magenta_box_fill(&circle);
    let square_fill = magenta_box_fill(&square);
    assert!(
        circle_fill < 0.85,
        "Circle filled {circle_fill:.3} of its bounding box; a disc covers about 0.785",
    );
    assert!(
        square_fill > 0.93,
        "Square filled {square_fill:.3} of its bounding box; a square covers about 1.0 - \
         it is being drawn as a disc",
    );
}

/// Which routing scene to render: no anchor at all, an anchor no cable wraps, a
/// cable routed through that anchor, that cable with the anchor held mid-drag,
/// or with the cursor resting on the cable's grabbable output end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteScene {
    Bare,
    Anchored,
    Wrapped,
    Dragging,
    HoveringEnd,
}

/// Render two pin-carrying nodes joined by one edge, with an anchor placed well
/// below the direct pin-to-pin line and, in the routed scenes, named in that
/// edge's route.
///
/// Each node's content is one 60x50 pin body, so its pin anchor sits at the
/// middle of the side it is on: node 0's Right/output pin at world (60, 25) and
/// node 1's Left/input pin at (200, 25). `camera()` offsets world by (20, 20) at
/// zoom 1, so those are screen (80, 45) and (220, 45).
fn render_routed_edge(scene: RouteScene) -> Option<Vec<[u8; 4]>> {
    use iced::widget::container;
    use iced_nodegraph::{PinDirection, PinRef, PinSide, anchor, edge, node_pin};

    let mut guard = shared()?;
    let renderer = &mut *guard;

    const ANCHOR: usize = 9;

    let mut graph: NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .camera(Point::new(20.0, 20.0), 1.0)
            .on_connect(|_, _| ())
            .on_anchor_move(|_id, _position| ());

    let pin_body = || {
        container(text("p"))
            .width(Length::Fixed(60.0))
            .height(Length::Fixed(50.0))
    };
    graph = graph.push_node(node(
        0usize,
        Point::new(0.0, 0.0),
        Element::from(node_pin(PinSide::Right, 0usize, pin_body()).direction(PinDirection::Output)),
    ));
    graph = graph.push_node(node(
        1usize,
        Point::new(200.0, 0.0),
        Element::from(node_pin(PinSide::Left, 0usize, pin_body()).direction(PinDirection::Input)),
    ));

    let routed = matches!(
        scene,
        RouteScene::Wrapped | RouteScene::Dragging | RouteScene::HoveringEnd
    );
    let route: Vec<usize> = if routed { vec![ANCHOR] } else { Vec::new() };
    graph = graph
        .push_edge(edge((), PinRef::new(0usize, 0usize), PinRef::new(1usize, 0usize)).route(route));
    if scene != RouteScene::Bare {
        graph = graph.push_anchor(anchor(ANCHOR, Point::new(115.0, 160.0)));
    }

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    let mut feed =
        |graph: &mut NodeGraph<'static, iced_nodegraph::Indexed, (), Theme, Renderer>,
         tree: &mut Tree,
         event: iced::Event,
         cursor: mouse::Cursor| {
            graph.update(
                tree,
                &event,
                layout,
                cursor,
                &*renderer,
                &mut clipboard,
                &mut shell,
                &viewport_rect,
            );
        };
    feed(
        &mut graph,
        &mut tree,
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(-1.0, -1.0),
        }),
        mouse::Cursor::Unavailable,
    );

    let mut cursor = mouse::Cursor::Unavailable;
    if scene == RouteScene::HoveringEnd {
        // 20 px along the cable from the output pin: inside the 24 px end zone
        // a press would unplug, and 16 px clear of the pin itself.
        let on_end = GLOW_HOVER;
        feed(
            &mut graph,
            &mut tree,
            iced::Event::Mouse(mouse::Event::CursorMoved { position: on_end }),
            mouse::Cursor::Available(on_end),
        );
        cursor = mouse::Cursor::Available(on_end);
    }
    // Grab the anchor and hold the cursor elsewhere WITHOUT releasing, so the
    // frame shows the in-flight preview rather than a committed move.
    if scene == RouteScene::Dragging {
        let grab = Point::new(135.0, 180.0);
        let held = Point::new(95.0, 130.0);
        for (event, at) in [
            (
                iced::Event::Mouse(mouse::Event::CursorMoved { position: grab }),
                grab,
            ),
            (
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                grab,
            ),
            (
                iced::Event::Mouse(mouse::Event::CursorMoved { position: held }),
                held,
            ),
        ] {
            feed(&mut graph, &mut tree, event, mouse::Cursor::Available(at));
        }
        cursor = mouse::Cursor::Available(held);
    }

    graph.draw(
        &tree,
        renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        cursor,
        &viewport_rect,
    );

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// Renders the scene with `route` applied to the edge, optionally after driving
/// a route drag from the cable's mid-run onto the anchor.
///
/// `route` is what the HOST has caught up to, so an empty route with a drag in
/// flight is the frame between the widget publishing the attachment and the
/// host applying it - the frame the preview has to carry alone.
fn render_route_drag(route: &[usize], drag: bool) -> Option<Vec<[u8; 4]>> {
    use iced::widget::container;
    use iced_nodegraph::{PinDirection, PinRef, PinSide, anchor, edge, node_pin};

    let mut guard = shared()?;
    let renderer = &mut *guard;

    const ANCHOR: usize = 9;

    let mut graph: NodeGraph<'static, RouteIds, (), Theme, Renderer> = NodeGraph::default()
        .width(Length::Fixed(W as f32))
        .height(Length::Fixed(H as f32))
        .camera(Point::new(20.0, 20.0), 1.0)
        .on_anchor_create(|_, _| ())
        .on_route_attach(|_, _| ())
        .on_route_detach(|_, _| ());

    let pin_body = || {
        container(text("p"))
            .width(Length::Fixed(60.0))
            .height(Length::Fixed(50.0))
    };
    graph = graph.push_node(node(
        0usize,
        Point::new(0.0, 0.0),
        Element::from(node_pin(PinSide::Right, 0usize, pin_body()).direction(PinDirection::Output)),
    ));
    graph = graph.push_node(node(
        1usize,
        Point::new(200.0, 0.0),
        Element::from(node_pin(PinSide::Left, 0usize, pin_body()).direction(PinDirection::Input)),
    ));
    graph = graph.push_anchor(anchor(ANCHOR, Point::new(115.0, 160.0)));
    graph = graph.push_edge(
        edge(
            0usize,
            PinRef::new(0usize, 0usize),
            PinRef::new(1usize, 0usize),
        )
        .route(route.to_vec()),
    );

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));

    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;
    let mut cursor = mouse::Cursor::Unavailable;
    // One event before anything else, so `update` syncs `camera()` into the
    // camera and picks up the viewport origin. Without it a frame that feeds no
    // events is drawn from an unsynced camera and lands offset from one that
    // does - which would read as a geometry difference.
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
    if drag {
        // Grab the bare cable's mid-run, then hold the cursor on the anchor's
        // core without releasing.
        let mid = Point::new(150.0, 45.0);
        let core = Point::new(135.0, 180.0);
        for (event, at) in [
            (
                iced::Event::Mouse(mouse::Event::CursorMoved { position: mid }),
                mid,
            ),
            (
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                mid,
            ),
            (
                iced::Event::Mouse(mouse::Event::CursorMoved { position: core }),
                core,
            ),
        ] {
            graph.update(
                &mut tree,
                &event,
                layout,
                mouse::Cursor::Available(at),
                &*renderer,
                &mut clipboard,
                &mut shell,
                &viewport_rect,
            );
        }
        cursor = mouse::Cursor::Available(core);
    }

    graph.draw(
        &tree,
        renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        cursor,
        &viewport_rect,
    );

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// Drives the wrap-grab gesture across frames: grab the cable where it wraps
/// the anchor, pull it off, then put it back on the core.
///
/// The graph is rebuilt from a fresh route at each step over one surviving
/// `Tree`, so the host can be seen catching up with the detach - or not.
/// `after_detach` is its route once the detach has been reported,
/// `at_reattach` its route on the frame the drag snaps back on. The returned
/// pixels are that last frame.
fn render_regrab(after_detach: &[usize], at_reattach: &[usize]) -> Option<Vec<[u8; 4]>> {
    use iced::widget::container;
    use iced_nodegraph::{PinDirection, PinRef, PinSide, anchor, edge, node_pin};

    let mut guard = shared()?;
    let renderer = &mut *guard;

    const ANCHOR: usize = 9;

    let scene = |route: &[usize]| {
        let mut graph: NodeGraph<'static, RouteIds, (), Theme, Renderer> = NodeGraph::default()
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .camera(Point::new(20.0, 20.0), 1.0)
            .on_anchor_create(|_, _| ())
            .on_route_attach(|_, _| ())
            .on_route_detach(|_, _| ());
        let pin_body = || {
            container(text("p"))
                .width(Length::Fixed(60.0))
                .height(Length::Fixed(50.0))
        };
        graph = graph.push_node(node(
            0usize,
            Point::new(0.0, 0.0),
            Element::from(
                node_pin(PinSide::Right, 0usize, pin_body()).direction(PinDirection::Output),
            ),
        ));
        graph = graph.push_node(node(
            1usize,
            Point::new(200.0, 0.0),
            Element::from(
                node_pin(PinSide::Left, 0usize, pin_body()).direction(PinDirection::Input),
            ),
        ));
        graph = graph.push_anchor(anchor(ANCHOR, Point::new(115.0, 160.0)));
        graph = graph.push_edge(
            edge(
                0usize,
                PinRef::new(0usize, 0usize),
                PinRef::new(1usize, 0usize),
            )
            .route(route.to_vec()),
        );
        graph
    };

    let wrap = Point::new(135.0, 191.0);
    let away = Point::new(135.0, 240.0);
    let core = Point::new(135.0, 180.0);
    // Route in force for each step, and the event driving it.
    let steps: [(&[usize], iced::Event, Point); 4] = [
        (
            &[ANCHOR],
            iced::Event::Mouse(mouse::Event::CursorMoved { position: wrap }),
            wrap,
        ),
        (
            &[ANCHOR],
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            wrap,
        ),
        (
            &[ANCHOR],
            iced::Event::Mouse(mouse::Event::CursorMoved { position: away }),
            away,
        ),
        (
            after_detach,
            iced::Event::Mouse(mouse::Event::CursorMoved { position: core }),
            core,
        ),
    ];

    let mut graph = scene(&[ANCHOR]);
    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(W as f32, H as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(W as f32, H as f32));
    let mut msgs: Vec<()> = Vec::new();
    let mut shell = iced_wgpu::core::Shell::new(&mut msgs);
    let mut clipboard = clipboard::Null;

    for (route, event, at) in steps {
        let mut g = scene(route);
        tree.diff(&g as &dyn Widget<(), Theme, Renderer>);
        g.update(
            &mut tree,
            &event,
            layout,
            mouse::Cursor::Available(at),
            &*renderer,
            &mut clipboard,
            &mut shell,
            &viewport_rect,
        );
    }

    // The frame the user is looking at: snapped back on, host at `at_reattach`.
    let final_graph = scene(at_reattach);
    tree.diff(&final_graph as &dyn Widget<(), Theme, Renderer>);
    final_graph.draw(
        &tree,
        renderer,
        &Theme::Dark,
        &renderer::Style {
            text_color: Color::WHITE,
        },
        layout,
        mouse::Cursor::Available(core),
        &viewport_rect,
    );
    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    let _ = &mut graph;
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// A snapped route drag previews the cable it is about to commit.
///
/// The widget publishes the attachment on snap, so for at least one frame it
/// renders a route the host has not applied yet. That frame must already draw
/// the cable where the committed frame draws it, and must NOT still be drawing
/// the straight bare cable. The anchor's own rings are excluded from the
/// comparison: an offered orbit legitimately becomes an occupied one.
#[test]
fn a_snapped_route_preview_draws_the_cable_it_will_commit() {
    let Some(previewed) = render_route_drag(&[], true) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    // The end state, drawn without any drag in flight: what the preview is
    // promising the user they will get.
    let committed = render_route_drag(&[9], false).expect("adapter present");
    let bare = render_route_drag(&[], false).expect("adapter present");
    // Pull the cable off the anchor and put it straight back on. Empty
    // `at_reattach` is the frame before the host applies the re-attachment;
    // naming the anchor there is every frame after it has, which is the whole
    // rest of the drag - and the case where the drag's own record of what it
    // detached must not outrank being snapped to it.
    let regrabbed_pending = render_regrab(&[], &[]).expect("adapter present");
    let regrabbed_applied = render_regrab(&[], &[9]).expect("adapter present");

    // The anchor's core sits at screen (135, 180); its outermost drawn ring is
    // well inside 40 px of it.
    const CX: u32 = 135;
    const CY: u32 = 180;
    const R: u32 = 40;
    let outside_the_anchor = |a: &[[u8; 4]], b: &[[u8; 4]]| {
        let mut count = 0;
        for y in 0..H {
            for x in 0..W {
                let near = x.abs_diff(CX) <= R && y.abs_diff(CY) <= R;
                if !near && a[(y * W + x) as usize] != b[(y * W + x) as usize] {
                    count += 1;
                }
            }
        }
        count
    };

    let pc = outside_the_anchor(&previewed, &committed);
    let pb = outside_the_anchor(&previewed, &bare);
    let cb = outside_the_anchor(&committed, &bare);
    eprintln!("previewed vs committed: {pc}, previewed vs bare: {pb}, committed vs bare: {cb}");

    assert_eq!(
        pc, 0,
        "the previewed cable runs somewhere the committed one does not",
    );
    assert!(
        pb > 200,
        "the preview still draws the straight bare cable: the snapped wrap is missing",
    );

    // Putting the cable back on the anchor it was just pulled off must preview
    // exactly like any other snap, whether or not the host has applied the
    // re-attachment yet. Once it has, the drag's record of what it detached
    // names the very anchor it is snapped to: read as an exclusion, that
    // suppresses the real wrap while the offered one is skipped as "already
    // routed", and the cable draws as if it had never been grabbed.
    for (label, frame) in [
        ("host pending", &regrabbed_pending),
        ("host applied", &regrabbed_applied),
    ] {
        let vs_committed = outside_the_anchor(frame, &committed);
        let vs_bare = outside_the_anchor(frame, &bare);
        eprintln!("re-grabbed ({label}): vs committed {vs_committed}, vs bare {vs_bare}");
        assert_eq!(
            vs_committed, 0,
            "re-attaching to the anchor it was pulled off ({label}) draws a \
             different cable than committing it does",
        );
        assert!(
            vs_bare > 200,
            "re-attaching to the anchor it was pulled off ({label}) draws the \
             straight bare cable: the wrap is missing",
        );
    }
}

/// The screen point the glow test hovers: 20 px along the cable from node 0's
/// output pin, which leaves the pin at (80, 45) heading right.
const GLOW_HOVER: Point = Point::new(95.7, 48.1);

/// Pixels differing between two frames inside the screen rect
/// `x0..=x1` by `y0..=y1`.
fn diff_in(a: &[[u8; 4]], b: &[[u8; 4]], x0: u32, y0: u32, x1: u32, y1: u32) -> usize {
    let mut count = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (y * W + x) as usize;
            if a[i] != b[i] {
                count += 1;
            }
        }
    }
    count
}

/// Routing is a rendering contract, not bookkeeping. Four frames pin it:
/// pushing an anchor has to change the framebuffer (its core reaches the GPU
/// even with no cable on it), and naming that anchor in an edge's route has to
/// change it again - the cable leaves the direct curve to wrap the ring the
/// anchor now carries.
///
/// Comparing `Anchored` against `Wrapped` isolates the routing: the anchor is in
/// both, so every differing pixel belongs to the cable and its new ring. The
/// fourth frame holds a grab in flight - no release, nothing committed - and
/// must still move both the core and the cable hanging off it, because a drag
/// you cannot see is a drag that feels broken.
#[test]
fn full_widget_renders_routed_edge_and_anchor() {
    let Some(bare) = render_routed_edge(RouteScene::Bare) else {
        eprintln!("no GPU adapter - skipping full_widget_renders_routed_edge_and_anchor");
        return;
    };
    let anchored = render_routed_edge(RouteScene::Anchored).expect("GPU was available");
    let routed = render_routed_edge(RouteScene::Wrapped).expect("GPU was available");

    for (label, px) in [
        ("bare", &bare),
        ("anchored", &anchored),
        ("routed", &routed),
    ] {
        let distinct: std::collections::HashSet<[u8; 4]> = px.iter().copied().collect();
        assert!(
            distinct.len() > 3,
            "{label} frame is near-uniform ({} distinct colours): nothing rendered",
            distinct.len(),
        );
    }

    let diff = |a: &[[u8; 4]], b: &[[u8; 4]]| a.iter().zip(b).filter(|(x, y)| x != y).count();

    let anchor_px = diff(&bare, &anchored);
    assert!(
        anchor_px > 20,
        "pushing an anchor changed only {anchor_px} pixels: the core did not draw",
    );

    let cable_px = diff(&anchored, &routed);
    assert!(
        cable_px > 400,
        "routing the edge changed only {cable_px} pixels: the anchor did not bend the cable",
    );

    let dragging = render_routed_edge(RouteScene::Dragging).expect("GPU was available");
    let preview_px = diff(&routed, &dragging);
    assert!(
        preview_px > 400,
        "holding the anchor mid-drag changed only {preview_px} pixels: the preview is missing",
    );
}

/// Hovering a cable's grabbable end has to reach the framebuffer - the feedback
/// IS the pixels - and it has to mark THAT end only.
///
/// The glow is the stretch a press would take hold of, so a whole-cable
/// highlight is a different promise: the far end, whose own end zone a press
/// there would grab instead, must come out of the hover untouched.
#[test]
fn a_hovered_cable_end_glows() {
    let Some(idle) = render_routed_edge(RouteScene::Wrapped) else {
        eprintln!("no GPU adapter - skipping a_hovered_cable_end_glows");
        return;
    };
    let hovered = render_routed_edge(RouteScene::HoveringEnd).expect("GPU was available");

    // The hovered end zone runs from the output pin at (80, 45) to (98.3, 49.4);
    // this box holds it clear of node 0's body, which ends at x = 80.
    let near = diff_in(&idle, &hovered, 82, 38, 130, 58);
    // The end zone at the other end runs from (202.2, 49.1) to the input pin at
    // (220, 45); node 1's body starts at x = 220.
    let far = diff_in(&idle, &hovered, 195, 38, 218, 58);

    assert!(
        near > 40,
        "hovering the cable's output end changed only {near} pixels there: \
         the glow is missing",
    );
    assert_eq!(
        far, 0,
        "the far end changed too: the glow marks the whole cable instead of the \
         stretch the press would take",
    );
}
