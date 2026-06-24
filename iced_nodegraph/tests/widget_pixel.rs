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
use iced_nodegraph::{NodeGraph, node};
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
