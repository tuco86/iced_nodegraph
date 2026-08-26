//! Theme gallery probe: renders the same graph under every built-in iced
//! [`Theme`] so the theme-derived defaults can be judged as a set rather than
//! one palette at a time.
//!
//! The scene is identical across themes (same geometry, same content, one
//! selected node, edges crossing the canvas), so any difference in the output is
//! the palette mapping and nothing else. It lives in its own test binary because
//! the shared `SdfPipeline` carries frame-surviving caches - see `common`.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::widget::{Column, Row, Space, column, container, text};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_wgpu::Renderer;
use iced_wgpu::core::clipboard;
use iced_wgpu::graphics::Viewport;

use common::shared;
use iced_nodegraph::{NodeGraph, PinDirection, PinRef, PinSide, edge, node, node_pin};

/// One gallery cell, in physical pixels.
const CW: u32 = 420;
const CH: u32 = 280;

/// A node body with a title and labelled pins: input ids `0..`, output ids `10..`.
fn node_body(
    title: String,
    inputs: &'static [&'static str],
    outputs: &'static [&'static str],
) -> Element<'static, (), Theme, Renderer> {
    let ins: Vec<Element<'static, (), Theme, Renderer>> = inputs
        .iter()
        .enumerate()
        .map(|(i, label)| {
            Element::from(
                node_pin(PinSide::Left, i, text(*label).size(11)).direction(PinDirection::Input),
            )
        })
        .collect();
    let outs: Vec<Element<'static, (), Theme, Renderer>> = outputs
        .iter()
        .enumerate()
        .map(|(i, label)| {
            Element::from(
                node_pin(PinSide::Right, 10 + i, text(*label).size(11))
                    .direction(PinDirection::Output),
            )
        })
        .collect();

    container(
        column![
            text(title).size(14),
            Row::with_children(vec![
                Element::from(Column::with_children(ins).spacing(6)),
                Element::from(Space::new().width(Length::Fill)),
                Element::from(Column::with_children(outs).spacing(6)),
            ]),
        ]
        .spacing(10),
    )
    .padding(10)
    .width(Length::Fixed(150.0))
    .into()
}

/// Renders the gallery scene under `theme`, or `None` without a GPU adapter.
fn render_theme(theme: &Theme) -> Option<Vec<[u8; 4]>> {
    let mut guard = shared()?;
    let renderer = &mut *guard;

    let mut graph: NodeGraph<'static, usize, usize, (), usize, (), (), Renderer> =
        NodeGraph::default()
            .width(Length::Fixed(CW as f32))
            .height(Length::Fixed(CH as f32))
            .view(Point::ORIGIN, 1.0);

    graph.push_node(node(
        0_usize,
        Point::new(20.0, 24.0),
        node_body(theme.to_string(), &[], &["out"]),
    ));
    graph.push_node(
        node(
            1_usize,
            Point::new(238.0, 46.0),
            node_body("Filter".into(), &["in"], &["out"]),
        )
        .selected(true),
    );
    graph.push_node(node(
        2_usize,
        Point::new(126.0, 170.0),
        node_body("Output".into(), &["a", "b"], &[]),
    ));

    graph.push_edge(edge!(PinRef::new(0, 10_usize), PinRef::new(1, 0_usize)));
    graph.push_edge(edge!(PinRef::new(1, 10_usize), PinRef::new(2, 0_usize)));

    let mut tree = Tree::new(&graph as &dyn Widget<(), Theme, Renderer>);
    let layout_node = graph.layout(
        &mut tree,
        &*renderer,
        &layout::Limits::new(Size::ZERO, Size::new(CW as f32, CH as f32)),
    );
    let layout = Layout::new(&layout_node);
    let viewport_rect = Rectangle::new(Point::ORIGIN, Size::new(CW as f32, CH as f32));

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
        theme,
        &renderer::Style {
            text_color: theme.palette().text,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport_rect,
    );

    let bytes = renderer.screenshot(
        &Viewport::with_physical_size(Size::new(CW, CH), 1.0),
        Color::TRANSPARENT,
    );
    Some(bytes.as_chunks::<4>().0.to_vec())
}

/// Writes contact sheets of the whole built-in theme set to
/// `target/theme_gallery/sheet_N.png`, two cells wide.
#[test]
#[ignore = "visual probe: writes target/theme_gallery/sheet_*.png"]
fn probe_theme_gallery() {
    const COLS: usize = 2;
    const ROWS: usize = 3;

    let dir = std::path::Path::new("target/theme_gallery");
    std::fs::create_dir_all(dir).unwrap();

    let mut cells: Vec<Vec<[u8; 4]>> = Vec::new();
    for theme in Theme::ALL {
        let Some(px) = render_theme(theme) else {
            eprintln!("no GPU adapter - skipping probe_theme_gallery");
            return;
        };
        cells.push(px);
    }

    for (sheet, chunk) in cells.chunks(COLS * ROWS).enumerate() {
        let rows = chunk.len().div_ceil(COLS);
        let (sw, sh) = (CW as usize * COLS, CH as usize * rows);
        let mut buf = vec![0u8; sw * sh * 4];
        for (i, cell) in chunk.iter().enumerate() {
            let (ox, oy) = ((i % COLS) * CW as usize, (i / COLS) * CH as usize);
            for y in 0..CH as usize {
                for x in 0..CW as usize {
                    let src = cell[y * CW as usize + x];
                    let dst = ((oy + y) * sw + ox + x) * 4;
                    buf[dst..dst + 4].copy_from_slice(&src);
                }
            }
        }
        let path = dir.join(format!("sheet_{sheet}.png"));
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&buf).unwrap();
        eprintln!("wrote {}", path.display());
    }
}
