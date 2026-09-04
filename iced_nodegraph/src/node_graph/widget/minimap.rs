//! The minimap overlay: where the graph is, and where the viewport sits in it.
//!
//! Two spaces meet here and nothing else: world coordinates, which the host
//! authors nodes in, and the widget's screen space, which the map's rectangle
//! lives in. No camera transform runs between them - the map keeps its size and
//! its corner at every zoom - so [`Projection`] is the whole mapping, used by
//! `draw` to place the marks and by `update` to turn a press into a camera
//! position.

use iced_wgpu::core::renderer::Quad;
use iced_widget::core::{Background, Border, Point, Rectangle, Shadow};

use crate::node_graph::euclid::{WorldPoint, WorldRect};
use crate::node_graph::{Corner, Minimap};
use crate::style::MinimapStyle;

/// Smallest on-screen footprint of a node mark, in pixels.
///
/// A graph large enough to want an overview scales most of its nodes below one
/// pixel, and a mark rounded to nothing leaves the map blank - which is the one
/// thing an overview may not be.
const MIN_MARK_SIDE: f32 = 1.0;

/// The map's own screen rectangle inside the widget's `bounds`.
///
/// Capped at what the margin leaves on either side, so a graph smaller than the
/// requested map still draws the map inside itself.
pub(super) fn rect(minimap: &Minimap, bounds: Rectangle) -> Rectangle {
    let margin = minimap.margin;
    let width = minimap.size.width.min(bounds.width - margin * 2.0).max(0.0);
    let height = minimap
        .size
        .height
        .min(bounds.height - margin * 2.0)
        .max(0.0);
    let right = bounds.x + bounds.width - margin - width;
    let bottom = bounds.y + bounds.height - margin - height;
    let (x, y) = match minimap.corner {
        Corner::TopLeft => (bounds.x + margin, bounds.y + margin),
        Corner::TopRight => (right, bounds.y + margin),
        Corner::BottomLeft => (bounds.x + margin, bottom),
        Corner::BottomRight => (right, bottom),
    };
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

/// The world rectangle a map covers: every node, plus what the viewport shows.
///
/// The visible rectangle is part of the union so the viewport marker lies
/// inside the map by construction - including over an empty graph, where it is
/// the entire extent and the map reads as fully covered.
pub(super) fn world_bounds(
    nodes: impl IntoIterator<Item = WorldRect>,
    visible: WorldRect,
) -> WorldRect {
    nodes
        .into_iter()
        .fold(visible, |whole, node| whole.union(&node))
}

/// The uniform, centered mapping between a map's world rectangle and its screen
/// rectangle.
#[derive(Debug, Clone, Copy)]
pub(super) struct Projection {
    map_center: Point,
    world_center: WorldPoint,
    /// Screen pixels per world unit.
    scale: f32,
}

impl Projection {
    /// Fits `world` into `map`, uniformly and centered.
    ///
    /// Uniform because the two aspect ratios differ in general, and a stretched
    /// map would report the wrong axis as a node's longer one; centered because
    /// the leftover strip then splits evenly instead of biasing one edge.
    pub(super) fn new(map: Rectangle, world: WorldRect) -> Self {
        // A zero-extent axis constrains nothing rather than dividing by zero;
        // both zero (no nodes and an empty viewport) leaves the scale entirely
        // unconstrained, and 1:1 is then the only value that is not arbitrary.
        let fit = |screen: f32, extent: f32| {
            if extent > 0.0 {
                screen / extent
            } else {
                f32::INFINITY
            }
        };
        let scale = fit(map.width, world.size.width).min(fit(map.height, world.size.height));
        Self {
            map_center: map.center(),
            world_center: world.center(),
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
        }
    }

    pub(super) fn world_to_map(&self, p: WorldPoint) -> Point {
        Point::new(
            self.map_center.x + (p.x - self.world_center.x) * self.scale,
            self.map_center.y + (p.y - self.world_center.y) * self.scale,
        )
    }

    pub(super) fn map_to_world(&self, p: Point) -> WorldPoint {
        WorldPoint::new(
            self.world_center.x + (p.x - self.map_center.x) / self.scale,
            self.world_center.y + (p.y - self.map_center.y) / self.scale,
        )
    }

    fn rect_to_map(&self, r: WorldRect) -> Rectangle {
        let origin = self.world_to_map(r.origin);
        Rectangle {
            x: origin.x,
            y: origin.y,
            width: r.size.width * self.scale,
            height: r.size.height * self.scale,
        }
    }
}

/// Draws the map: background pane, one quad per node, then the viewport
/// rectangle on top.
///
/// Screen space throughout, no camera transform and no SDF work: the marks are
/// axis-aligned quads through iced's own quad pipeline, one per node.
pub(super) fn draw<Renderer>(
    renderer: &mut Renderer,
    map: Rectangle,
    projection: &Projection,
    style: &MinimapStyle,
    nodes: impl IntoIterator<Item = (WorldRect, bool)>,
    visible: WorldRect,
) where
    Renderer: iced_wgpu::core::renderer::Renderer,
{
    let quad = |bounds: Rectangle, border: Border| Quad {
        bounds,
        border,
        shadow: Shadow::default(),
        snap: true,
    };

    renderer.fill_quad(
        quad(
            map,
            Border {
                color: style.border_color,
                width: style.border_width,
                ..Border::default()
            },
        ),
        Background::Color(style.background),
    );

    for (node, selected) in nodes {
        let mark = projection.rect_to_map(node);
        let color = if selected {
            style.selected_node_color
        } else {
            style.node_color
        };
        renderer.fill_quad(
            quad(
                Rectangle {
                    width: mark.width.max(MIN_MARK_SIDE),
                    height: mark.height.max(MIN_MARK_SIDE),
                    ..mark
                },
                Border::default(),
            ),
            Background::Color(color),
        );
    }

    renderer.fill_quad(
        quad(
            projection.rect_to_map(visible),
            Border {
                color: style.viewport_border_color,
                width: style.viewport_border_width,
                ..Border::default()
            },
        ),
        Background::Color(style.viewport_fill),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::euclid::WorldSize;

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    };

    fn map() -> Minimap {
        Minimap::default()
    }

    #[test]
    fn every_corner_keeps_the_margin_on_its_two_edges() {
        for (corner, expected) in [
            (Corner::TopLeft, (12.0, 12.0)),
            (Corner::TopRight, (1000.0 - 12.0 - 200.0, 12.0)),
            (Corner::BottomLeft, (12.0, 800.0 - 12.0 - 150.0)),
            (
                Corner::BottomRight,
                (1000.0 - 12.0 - 200.0, 800.0 - 12.0 - 150.0),
            ),
        ] {
            let r = rect(&Minimap { corner, ..map() }, BOUNDS);
            assert_eq!(
                (r.x, r.y),
                expected,
                "{corner:?} must sit one margin off its own two edges",
            );
            assert_eq!((r.width, r.height), (200.0, 150.0));
        }
    }

    /// A graph smaller than the requested map must not paint chrome outside
    /// itself: the size is what the margin leaves, not what was asked for.
    #[test]
    fn a_map_larger_than_the_graph_shrinks_into_it() {
        let bounds = Rectangle {
            width: 100.0,
            height: 60.0,
            ..BOUNDS
        };
        let r = rect(&map(), bounds);
        assert_eq!((r.width, r.height), (76.0, 36.0));
        assert!(bounds.contains(Point::new(r.x, r.y)));
        assert!(bounds.contains(Point::new(r.x + r.width, r.y + r.height)));
    }

    /// The map exists to be pressed, so the two directions must agree exactly:
    /// what `draw` puts at a pixel is what `update` reads back from it.
    #[test]
    fn projection_round_trips_through_both_directions() {
        let world = WorldRect::new(WorldPoint::new(-300.0, 40.0), WorldSize::new(900.0, 200.0));
        let p = Projection::new(rect(&map(), BOUNDS), world);
        for at in [
            WorldPoint::new(-300.0, 40.0),
            world.center(),
            WorldPoint::new(600.0, 240.0),
        ] {
            let back = p.map_to_world(p.world_to_map(at));
            assert!(
                (back.x - at.x).abs() < 1e-3 && (back.y - at.y).abs() < 1e-3,
                "{at:?} round-tripped to {back:?}",
            );
        }
    }

    /// Uniform scale, and the whole world extent inside the map: a viewport
    /// marker that leaves the pane would point at nothing.
    #[test]
    fn the_world_extent_fits_the_map_centered() {
        let map_rect = rect(&map(), BOUNDS);
        let world = WorldRect::new(WorldPoint::new(0.0, 0.0), WorldSize::new(4000.0, 500.0));
        let p = Projection::new(map_rect, world);
        let projected = p.rect_to_map(world);
        assert!(
            (projected.width / world.size.width - projected.height / world.size.height).abs()
                < 1e-4,
            "scale must be uniform, got {projected:?}",
        );
        assert!(
            projected.x >= map_rect.x - 1e-3
                && projected.y >= map_rect.y - 1e-3
                && projected.x + projected.width <= map_rect.x + map_rect.width + 1e-3
                && projected.y + projected.height <= map_rect.y + map_rect.height + 1e-3,
            "{projected:?} must lie inside {map_rect:?}",
        );
        assert!(
            (projected.center().x - map_rect.center().x).abs() < 1e-3
                && (projected.center().y - map_rect.center().y).abs() < 1e-3,
            "the extent must be centered in the map",
        );
    }

    /// With no nodes the map still has to answer a press, so the mapping must
    /// stay finite when the union has no extent at all.
    #[test]
    fn an_extentless_world_still_maps_both_ways() {
        let map_rect = rect(&map(), BOUNDS);
        let world = WorldRect::new(WorldPoint::new(10.0, 20.0), WorldSize::zero());
        let p = Projection::new(map_rect, world);
        let at = p.map_to_world(map_rect.center());
        assert_eq!((at.x, at.y), (10.0, 20.0));
        let mark = p.world_to_map(WorldPoint::new(10.0, 20.0));
        assert!(mark.x.is_finite() && mark.y.is_finite());
    }

    #[test]
    fn world_bounds_covers_the_viewport_and_every_node() {
        let visible = WorldRect::new(WorldPoint::new(0.0, 0.0), WorldSize::new(800.0, 600.0));
        let nodes = [
            WorldRect::new(WorldPoint::new(-500.0, 100.0), WorldSize::new(60.0, 30.0)),
            WorldRect::new(WorldPoint::new(900.0, 700.0), WorldSize::new(60.0, 30.0)),
        ];
        let whole = world_bounds(nodes, visible);
        assert_eq!(whole.origin, WorldPoint::new(-500.0, 0.0));
        assert_eq!(whole.size, WorldSize::new(1460.0, 730.0));
    }

    /// Without nodes the map shows exactly the viewport, so the marker fills it.
    #[test]
    fn world_bounds_of_an_empty_graph_is_the_viewport() {
        let visible = WorldRect::new(WorldPoint::new(-40.0, -20.0), WorldSize::new(800.0, 600.0));
        assert_eq!(world_bounds(std::iter::empty(), visible), visible);
    }
}
