//! A theme other than `iced::Theme` hosts the graph through its own `Catalog`.
//!
//! `Mono` picks `()` for every class and answers each resolver with the dark
//! iced default plus two sentinel colours: a pure red node body and a pure
//! blue canvas. The pixel test then reads those sentinels back from the
//! framebuffer, which proves the widget resolves through the foreign catalog
//! rather than through `iced::Theme`. Its own binary because it is a pixel
//! oracle: the shared `SdfPipeline` carries state across renders, so scenes
//! do not share a binary (see `common/mod.rs`).
#![cfg(not(target_arch = "wasm32"))]

mod common;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::widget::Space;
use iced::{Color, Element, Length, Point, Rectangle, Size};
use iced_wgpu::Renderer;
use iced_wgpu::core::clipboard;
use iced_wgpu::graphics::Viewport;

use common::record::Recorder;
use common::shared;
use iced_nodegraph::{
    AnchorStatus, AnchorStyle, Catalog, ColorQuad, CuttingToolStyle, EdgeStatus, EdgeStyle,
    GraphStyle, Ids, Indexed, MinimapStyle, NodeGraph, NodeStatus, NodeStyle, PinInfo, PinStatus,
    PinStyle, SelectionBoxStyle, default_anchor_style, default_cutting_tool_style,
    default_edge_style, default_minimap_style, default_node_style, default_pin_style,
    default_selection_box_style, node,
};

const W: u32 = 320;
const H: u32 = 240;

const RED: Color = Color::from_rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::from_rgb(0.0, 0.0, 1.0);

/// A theme with one look: every class is `()`, every style is the dark iced
/// default with the node body red and the canvas blue.
struct Mono;

impl Catalog for Mono {
    type NodeClass<'a> = ();
    type PinClass<'a, I: Ids> = ();
    type EdgeClass<'a, I: Ids> = ();
    type DragEdgeClass<'a, I: Ids> = ();
    type AnchorClass<'a> = ();
    type GraphClass<'a> = ();
    type SelectionBoxClass<'a> = ();
    type CuttingToolClass<'a> = ();
    type MinimapClass<'a> = ();

    fn default_node<'a>() -> Self::NodeClass<'a> {}

    fn node(&self, (): &Self::NodeClass<'_>, status: NodeStatus) -> NodeStyle {
        NodeStyle {
            fill_color: ColorQuad::solid(RED),
            border_color: ColorQuad::solid(RED),
            ..default_node_style(&iced::Theme::Dark, status)
        }
    }

    fn default_pin<'a, I: Ids>() -> Self::PinClass<'a, I> {}

    fn pin<I: Ids>(
        &self,
        (): &Self::PinClass<'_, I>,
        _pin: &PinInfo<'_, I>,
        _other: Option<&PinInfo<'_, I>>,
        status: PinStatus,
    ) -> PinStyle {
        default_pin_style(&iced::Theme::Dark, status)
    }

    fn default_edge<'a, I: Ids>() -> Self::EdgeClass<'a, I> {}

    fn edge<I: Ids>(
        &self,
        (): &Self::EdgeClass<'_, I>,
        status: EdgeStatus,
        _from: PinInfo<'_, I>,
        _to: PinInfo<'_, I>,
    ) -> EdgeStyle {
        default_edge_style(&iced::Theme::Dark, status)
    }

    fn default_drag_edge<'a, I: Ids>() -> Self::DragEdgeClass<'a, I> {}

    fn drag_edge<I: Ids>(&self, (): &Self::DragEdgeClass<'_, I>, _: PinInfo<'_, I>) -> EdgeStyle {
        default_edge_style(&iced::Theme::Dark, EdgeStatus::Idle)
    }

    fn default_anchor<'a>() -> Self::AnchorClass<'a> {}

    fn anchor(&self, (): &Self::AnchorClass<'_>, status: AnchorStatus) -> AnchorStyle {
        default_anchor_style(&iced::Theme::Dark, status)
    }

    fn default_graph<'a>() -> Self::GraphClass<'a> {}

    fn graph(&self, (): &Self::GraphClass<'_>) -> GraphStyle {
        GraphStyle {
            background_color: BLUE,
            tiling: None,
        }
    }

    fn default_selection_box<'a>() -> Self::SelectionBoxClass<'a> {}

    fn selection_box(&self, (): &Self::SelectionBoxClass<'_>) -> SelectionBoxStyle {
        default_selection_box_style(&iced::Theme::Dark)
    }

    fn default_cutting_tool<'a>() -> Self::CuttingToolClass<'a> {}

    fn cutting_tool(&self, (): &Self::CuttingToolClass<'_>) -> CuttingToolStyle {
        default_cutting_tool_style(&iced::Theme::Dark)
    }

    fn default_minimap<'a>() -> Self::MinimapClass<'a> {}

    fn minimap(&self, (): &Self::MinimapClass<'_>) -> MinimapStyle {
        default_minimap_style(&iced::Theme::Dark)
    }
}

/// Renders a one-node graph under `Mono` to RGBA pixels, or `None` without a
/// GPU adapter.
fn render() -> Option<Vec<[u8; 4]>> {
    let mut guard = shared()?;
    let renderer = &mut *guard;

    // Camera centred so the node (world origin) lands mid-viewport at zoom 1.
    let mut graph: NodeGraph<'static, Indexed, (), Mono, Renderer> = NodeGraph::default()
        .width(Length::Fixed(W as f32))
        .height(Length::Fixed(H as f32))
        .camera(
            Point::new(W as f32 * 0.5 - 40.0, H as f32 * 0.5 - 20.0),
            1.0,
        );
    graph = graph.push_node(node(
        0_usize,
        Point::ORIGIN,
        Element::from(Space::new().width(60.0).height(40.0)),
    ));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Mono, Renderer>);
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
        &Mono,
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

/// The canvas and the node body carry `Mono`'s sentinel colours, so every
/// style the frame needed came from the foreign catalog.
#[test]
fn foreign_theme_styles_through_its_catalog() {
    let Some(px) = render() else {
        eprintln!("no GPU adapter - skipping foreign_theme_styles_through_its_catalog");
        return;
    };
    let at = |x: u32, y: u32| px[(y * W + x) as usize];

    assert_eq!(
        at(2, 2),
        [0, 0, 255, 255],
        "viewport corner is the blue canvas"
    );

    let [r, g, b, _] = at(W / 2 - 40 + 30, H / 2 - 20 + 20);
    assert!(
        r > 200 && g < 40 && b < 40,
        "node centre is the red body, got ({r}, {g}, {b})"
    );
}

/// A graph over a foreign theme converts into that theme's `Element`.
#[test]
fn foreign_theme_is_an_element() {
    let _: Element<'_, (), Mono, Recorder> = NodeGraph::<Indexed, (), Mono, Recorder>::new()
        .push_node(node(0_usize, Point::ORIGIN, Element::from(Space::new())))
        .into();
}
