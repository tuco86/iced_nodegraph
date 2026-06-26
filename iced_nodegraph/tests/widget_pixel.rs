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
use iced_widget::core::clipboard;

use common::shared;
use iced_nodegraph::{ColorQuad, NodeGraph, NodeStyle, default_node_style, node};
use iced_wgpu::Renderer;
use iced_wgpu::graphics::Viewport;

const W: u32 = 320;
const H: u32 = 240;

/// Render a one-node graph (node carries hosted text content) to RGBA pixels.
/// Returns `None` if no GPU is available.
fn render_one_node() -> Option<Vec<[u8; 4]>> {
    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Camera centred so the node (world origin) lands mid-viewport at zoom 1.
    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Renderer> = NodeGraph::default()
        .width(Length::Fixed(W as f32))
        .height(Length::Fixed(H as f32))
        .view(
            Point::new(W as f32 * 0.5 - 40.0, H as f32 * 0.5 - 20.0),
            1.0,
        );
    graph.push_node(node(
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

    // One update syncs the controlled `view()` into the widget camera.
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
        &Viewport::with_physical_size(Size::new(W, H), 1.0),
        Color::TRANSPARENT,
    );
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect(),
    )
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
        // Rebuild the view each frame, exactly as a live app does.
        let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Renderer> =
            NodeGraph::default()
                .width(Length::Fixed(GW as f32))
                .height(Length::Fixed(GH as f32))
                .view(cam, zoom);
        for (id, &(tlx, tly)) in lattice.iter().enumerate() {
            let (wx, wy) = world_of(tlx, tly);
            graph.push_node(
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
            &Viewport::with_physical_size(Size::new(pw, ph), scale),
            Color::TRANSPARENT,
        );
        px = bytes
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
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

    let mut graph: NodeGraph<'static, usize, usize, (), (), Theme, Renderer> = NodeGraph::default()
        .width(Length::Fixed(GW as f32))
        .height(Length::Fixed(GH as f32))
        .view(cam, zoom);
    for (id, &(wx, wy)) in worlds.iter().enumerate() {
        graph.push_node(
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
        &Viewport::with_physical_size(Size::new(VW, GH), 1.0),
        Color::from_rgb(0.0, 0.0, 0.0),
    );
    let px: Vec<[u8; 4]> = bytes
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();

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
