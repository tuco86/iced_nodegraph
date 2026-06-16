//! Camera system for coordinate space transformations.
//!
//! # Coordinate Spaces
//!
//! This module manages two distinct coordinate spaces with compile-time type safety:
//!
//! - **Screen Space**: Raw pixel coordinates from user input (mouse position, viewport dimensions).
//!   Origin at top-left of viewport. Used for mouse events and rendering commands.
//!
//! - **World Space**: Virtual infinite canvas where nodes exist. Origin arbitrary.
//!   Unaffected by zoom/pan - node positions remain constant in world coordinates.
//!
//! The `euclid` crate provides phantom types (`Screen`, `World`) that prevent accidentally
//! mixing coordinate spaces at compile time.
//!
//! ## Camera2D Structure
//!
//! ```rust,ignore
//! pub struct Camera2D {
//!     zoom: Scale<f32, Screen, World>,  // Magnification level (1.0 = 1:1, 2.0 = zoomed in 2x)
//!     position: WorldPoint,              // Camera look-at point in world coordinates
//! }
//! ```
//!
//! # Transformation Formulas
//!
//! The core transformations between coordinate spaces follow these formulas:
//!
//! ## Screen → World (Mouse Input)
//!
//! ```text
//! world_point = screen_point / zoom - camera_position
//! ```
//!
//! Implementation uses `Transform2D::scale(1/zoom).then_translate(-position)`:
//! 1. Scale by inverse zoom (divide screen coordinates by zoom factor)
//! 2. Translate by negative camera position (shift to world origin)
//!
//! **Critical**: The order matters! `scale().then_translate()` is NOT equivalent to
//! `translate().then_scale()` or `.pre_scale()`. The chosen order ensures correct
//! mathematical inverse relationship with the rendering formula.
//!
//! ## World → Screen (Rendering)
//!
//! ```text
//! screen_point = (world_point + camera_position) * zoom
//! ```
//!
//! Implementation in `draw_with()`:
//! 1. Translate by camera position (shift world to camera view)
//! 2. Scale by zoom (magnify)
//!
//! This is applied to the renderer transformation stack, so GPU performs the conversion.
//!
//! ## Zoom at Cursor (Preserve Point Under Cursor)
//!
//! When zooming, we want the point under the cursor to remain visually fixed:
//!
//! ```text
//! new_position = old_position + cursor_screen * (1/new_zoom - 1/old_zoom)
//! ```
//!
//! **Derivation**: For a point to remain at the same screen position after zoom:
//! ```text
//! (world + pos1) * zoom1 = (world + pos2) * zoom2
//! world + pos1 = (world + pos2) * (zoom2 / zoom1)
//! pos2 = pos1 + world * (1 - zoom2/zoom1)
//! pos2 = pos1 + (cursor_screen / zoom1 - pos1) * (1 - zoom2/zoom1)
//! pos2 = pos1 + cursor_screen * (1/zoom1 - 1/zoom2)
//! ```
//!
//! # Common Pitfalls
//!
//! ## Wrong: Using .pre_scale()
//!
//! ```text
//! Transform2D::translation(-position).pre_scale(zoom)
//! Result: world = screen * zoom - position  ← INCORRECT INVERSE
//! ```
//!
//! ## Correct: Using .scale().then_translate()
//!
//! ```rust,ignore
//! // CORRECT - matches rendering inverse
//! let inv_zoom = 1.0 / zoom.get();
//! Transform2D::scale(inv_zoom, inv_zoom).then_translate(-position.to_vector())
//! // Result: world = screen / zoom - position  (CORRECT!)
//! ```
//!
//! ## Why It Matters
//!
//! The rendering pipeline uses `(world + position) * zoom`. If screen_to_world() doesn't
//! produce the mathematical inverse, click detection will fail - mouse clicks won't hit
//! the rendered elements at zoom levels != 1.0.
//!
//! # Usage Patterns
//!
//! ## Mouse Input → World Coordinates
//!
//! ```rust,ignore
//! use crate::node_graph::euclid::IntoEuclid;
//!
//! // Mouse events arrive in screen space
//! if let Some(cursor_position) = screen_cursor.position() {
//!     // Convert to typed screen point
//!     let cursor: ScreenPoint = cursor_position.into_euclid();
//!     
//!     // Transform to world space for hit testing
//!     let world_cursor: WorldPoint = camera.screen_to_world().transform_point(cursor);
//!     
//!     // Now compare with node positions (which are in world space)
//!     for (world_pos, node) in &nodes {
//!         if world_cursor.distance_to(*world_pos) < threshold {
//!             // Hit!
//!         }
//!     }
//! }
//! ```
//!
//! ## Rendering with Camera Transform
//!
//! ```rust,ignore
//! // Rendering happens through draw_with() which sets up GPU transforms
//! camera.draw_with(renderer, viewport, cursor, |renderer, world_viewport, world_cursor| {
//!     // Inside this closure:
//!     // - Renderer has camera transform applied (position + zoom)
//!     // - world_viewport is the visible rectangle in world coordinates
//!     // - world_cursor is mouse position in world coordinates
//!     
//!     for (world_pos, element) in &nodes {
//!         // Draw at world positions - GPU handles screen transform
//!         renderer.draw_rect(*world_pos, size);
//!     }
//! });
//! ```
//!
//! ## Zoom While Preserving Cursor Position
//!
//! ```rust,ignore
//! // User scrolls mouse wheel
//! let zoom_delta = 0.1; // Positive = zoom in, negative = zoom out
//! camera = camera.zoom_at(cursor_screen_pos, zoom_delta);
//! // Point under cursor stays visually fixed
//! ```
//!
//! # Testing
//!
//! This module includes 15 comprehensive tests covering:
//! - Identity transforms (no zoom/pan)
//! - Zoom-only transforms at various levels
//! - Pan-only transforms
//! - Combined zoom + pan
//! - Inverse consistency (screen→world→screen = identity)
//! - Multiple zoom steps (cursor drift prevention)
//! - Real-world scenarios from bug reports
//!
//! Run tests with: `cargo test --lib camera`

use super::euclid::{
    IntoEuclid, IntoIced, Screen, ScreenPoint, ScreenRect, ScreenToWorld, ScreenVector, World,
    WorldPoint, WorldRect, WorldSize, WorldVector,
};
use euclid::{Scale, Transform2D};
use iced::Rectangle;
use iced_widget::core::{mouse, renderer};

#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    zoom: Scale<f32, Screen, World>,
    position: WorldPoint,
    /// Screen-space offset of the widget's top-left within the window. The
    /// widget refreshes this every frame from its layout bounds so that
    /// rendering and hit-testing work when the graph is not at the window
    /// origin (e.g. below a toolbar). Persisted state ignores it; it is a
    /// per-frame render detail.
    viewport_origin: ScreenVector,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self::new()
    }
}

impl Camera2D {
    pub fn new() -> Self {
        Self {
            zoom: Scale::new(1.0),
            position: WorldPoint::origin(),
            viewport_origin: ScreenVector::zero(),
        }
    }

    /// Create a camera with custom zoom and position.
    ///
    /// Used for restoring camera state from persistence and testing.
    pub fn with_zoom_and_position(zoom: f32, position: WorldPoint) -> Self {
        Self {
            zoom: Scale::new(zoom),
            position,
            viewport_origin: ScreenVector::zero(),
        }
    }

    /// Returns a copy with the viewport origin set to the widget's screen
    /// position. Call this each frame before rendering or hit-testing.
    pub fn with_viewport_origin(mut self, origin: ScreenVector) -> Self {
        self.viewport_origin = origin;
        self
    }

    /// The widget's screen-space origin (top-left), as last set by
    /// [`with_viewport_origin`](Self::with_viewport_origin). Folded into the
    /// screen/world transforms so the graph can sit anywhere on screen.
    pub fn viewport_origin(&self) -> ScreenVector {
        self.viewport_origin
    }

    /// The current zoom factor; values above 1.0 zoom in.
    pub fn zoom(&self) -> f32 {
        self.zoom.get()
    }

    /// The current pan position (world-space origin offset).
    pub fn position(&self) -> WorldPoint {
        self.position
    }

    /// Screen-to-world transform: `world = (screen - viewport_origin) / zoom - position`.
    pub fn screen_to_world(&self) -> ScreenToWorld {
        // Converts screen coordinates to world coordinates, factoring in zoom,
        // position, and the widget's viewport origin.
        // Rendering does: screen = viewport_origin + (world + position) * zoom
        // So inverse is:  world  = (screen - viewport_origin) / zoom - position
        let inv_zoom = 1.0 / self.zoom.get();
        Transform2D::translation(-self.viewport_origin.x, -self.viewport_origin.y)
            .then_scale(inv_zoom, inv_zoom)
            .then_translate(-self.position.to_vector())
    }

    /// World-to-screen transform: `screen = (world + position) * zoom + viewport_origin`.
    pub fn world_to_screen(&self) -> Transform2D<f32, World, Screen> {
        // Converts world coordinates to screen coordinates.
        // The transform is always invertible since zoom is clamped to [0.1, 10.0].
        self.screen_to_world()
            .inverse()
            .expect("Camera transform must be invertible (zoom cannot be 0)")
    }

    pub fn move_by(&self, offset: WorldVector) -> Self {
        // Moves the camera by a given offset in world space.
        Self {
            zoom: self.zoom,
            position: self.position + offset,
            viewport_origin: self.viewport_origin,
        }
    }

    pub fn zoom_at(&self, cursor_screen: ScreenPoint, offset: f32) -> Self {
        // Adjusts the zoom level, keeping the screen cursor position stable.
        // This means the world point under the cursor stays at the same screen location.
        //
        // Rendering formula: screen = (world + position) * zoom
        // For a fixed screen point, we need:
        //   screen = (world + pos1) * zoom1 = (world + pos2) * zoom2
        //
        // Solving for pos2:
        //   pos2 = pos1 + screen * (1/zoom1 - 1/zoom2)

        let old_zoom = self.zoom.get();
        let new_zoom = (old_zoom + offset).clamp(0.1, 10.0);

        // Cursor must be relative to the widget origin; screen = origin + (world + pos) * zoom.
        let local_x = cursor_screen.x - self.viewport_origin.x;
        let local_y = cursor_screen.y - self.viewport_origin.y;

        // zoom_delta = 1/new_zoom - 1/old_zoom (not the other way around!)
        let zoom_delta = 1.0 / new_zoom - 1.0 / old_zoom;
        let position_offset = WorldVector::new(local_x * zoom_delta, local_y * zoom_delta);

        Self {
            zoom: Scale::new(new_zoom),
            position: self.position + position_offset,
            viewport_origin: self.viewport_origin,
        }
    }

    /// The renderer transformation that maps the widget's layout-absolute space
    /// (`viewport_origin + world`) to screen space, i.e. the same mapping
    /// applied to node content in [`draw_with`](Self::draw_with).
    ///
    /// A layout point `p` (= `viewport_origin + world`) is drawn at
    /// `screen = zoom * p + v`, where `v = viewport_origin * (1 - zoom) + zoom *
    /// position`. Composed as `translate(v) * scale(zoom)` so the renderer
    /// applies `scale * p` first, then the translation. Shared with the overlay
    /// path so pop-outs (combo box menus, tooltips) anchor and scale exactly
    /// like the node content under them.
    pub fn layer_transformation(&self) -> iced::Transformation {
        let zoom = self.zoom.get();
        let v_x = self.viewport_origin.x * (1.0 - zoom) + zoom * self.position.x;
        let v_y = self.viewport_origin.y * (1.0 - zoom) + zoom * self.position.y;
        iced::Transformation::translate(v_x, v_y) * iced::Transformation::scale(zoom)
    }

    pub fn draw_with<F, Renderer>(
        self,
        renderer: &mut Renderer,
        viewport: &iced::Rectangle,
        cursor: mouse::Cursor,
        f: F,
    ) where
        Renderer: renderer::Renderer,
        F: FnOnce(&mut Renderer, &iced::Rectangle, mouse::Cursor),
    {
        let transformed_cursor = self.cursor_screen_to_layout(cursor);
        let world_viewport = self.viewport_screen_to_layout(viewport);

        renderer.with_transformation(self.layer_transformation(), |renderer| {
            f(renderer, &world_viewport, transformed_cursor)
        })
    }

    pub fn update_with<F>(self, viewport: &iced::Rectangle, cursor: mouse::Cursor, f: F)
    where
        F: FnOnce(&iced::Rectangle, mouse::Cursor),
    {
        let transformed_cursor = self.cursor_screen_to_layout(cursor);
        let world_viewport = self.viewport_screen_to_layout(viewport);
        f(&world_viewport, transformed_cursor)
    }

    /// Converts a screen cursor into the widget's layout-absolute space, the
    /// space child layouts and node positions live in (`viewport_origin +
    /// world`). Equal to world space when the widget is at the window origin.
    /// Hit-testing and child event/draw propagation compare against layout
    /// positions, so they must use this space; drag deltas and emitted node
    /// positions are relative or use stored world coordinates, so the origin
    /// term cancels there.
    pub fn cursor_screen_to_layout(&self, cursor: mouse::Cursor) -> mouse::Cursor {
        let to_world = self.screen_to_world();
        let map = |pos: iced::Point| -> iced::Point {
            let w = to_world.transform_point(pos.into_euclid());
            iced::Point::new(w.x + self.viewport_origin.x, w.y + self.viewport_origin.y)
        };
        match cursor {
            mouse::Cursor::Available(pos) => mouse::Cursor::Available(map(pos)),
            mouse::Cursor::Levitating(pos) => mouse::Cursor::Levitating(map(pos)),
            mouse::Cursor::Unavailable => mouse::Cursor::Unavailable,
        }
    }

    fn viewport_screen_to_layout(&self, viewport: &Rectangle<f32>) -> Rectangle<f32> {
        let viewport: ScreenRect = viewport.into_euclid();
        // Screen -> layout-absolute: (screen - origin) / zoom - position + origin.
        let inv_zoom = 1.0 / self.zoom.get();
        let world_viewport: WorldRect = WorldRect::new(
            WorldPoint::new(
                (viewport.origin.x - self.viewport_origin.x) * inv_zoom - self.position.x
                    + self.viewport_origin.x,
                (viewport.origin.y - self.viewport_origin.y) * inv_zoom - self.position.y
                    + self.viewport_origin.y,
            ),
            WorldSize::new(
                viewport.size.width * inv_zoom,
                viewport.size.height * inv_zoom,
            ),
        );
        world_viewport.into_iced()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 0.001;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    fn point_approx_eq(a: WorldPoint, b: WorldPoint) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y)
    }

    #[test]
    fn test_identity_transform() {
        // At default zoom (1.0) and position (0,0), screen = world
        let camera = Camera2D::new();
        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);

        assert!(
            approx_eq(world.x, 100.0),
            "x: expected 100.0, got {}",
            world.x
        );
        assert!(
            approx_eq(world.y, 200.0),
            "y: expected 200.0, got {}",
            world.y
        );
    }

    #[test]
    fn test_zoom_transform() {
        // Zooming in 2x means screen coordinates are divided to get world coordinates
        // Rendering: screen = world * zoom, so at zoom 2.0, world 50 becomes screen 100
        // Inverse: world = screen / zoom, so screen 100 becomes world 50
        let mut camera = Camera2D::new();
        camera.zoom = euclid::Scale::new(2.0);

        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);

        // world = screen / zoom - position = (100, 200) / 2.0 - (0, 0) = (50, 100)
        assert!(
            approx_eq(world.x, 50.0),
            "x: expected 50.0, got {}",
            world.x
        );
        assert!(
            approx_eq(world.y, 100.0),
            "y: expected 100.0, got {}",
            world.y
        );
    }

    #[test]
    fn test_pan_transform() {
        // Moving camera position should offset world coordinates
        // Rendering: screen = (world + position) * zoom
        // Inverse: world = screen / zoom - position
        let mut camera = Camera2D::new();
        camera.position = WorldPoint::new(50.0, 100.0);

        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);

        // world = screen / zoom - position
        // world = (100, 200) / 1.0 - (50, 100) = (100, 200) - (50, 100) = (50, 100)
        assert!(
            approx_eq(world.x, 50.0),
            "x: expected 50.0, got {}",
            world.x
        );
        assert!(
            approx_eq(world.y, 100.0),
            "y: expected 100.0, got {}",
            world.y
        );
    }

    #[test]
    fn test_zoom_and_pan() {
        // Combined zoom + pan - most common real-world scenario
        let mut camera = Camera2D::new();
        camera.zoom = euclid::Scale::new(2.0);
        camera.position = WorldPoint::new(100.0, 200.0);

        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);

        // world = screen / zoom - position
        // world = (100, 200) / 2.0 - (100, 200) = (50, 100) - (100, 200) = (-50, -100)
        assert!(
            approx_eq(world.x, -50.0),
            "x: expected -50.0, got {}",
            world.x
        );
        assert!(
            approx_eq(world.y, -100.0),
            "y: expected -100.0, got {}",
            world.y
        );
    }

    #[test]
    fn test_zoom_at_cursor() {
        // When zooming at a cursor position, that world point should stay fixed
        let camera = Camera2D::new();
        let cursor_screen = ScreenPoint::new(400.0, 300.0);
        let cursor_world_before = camera.screen_to_world().transform_point(cursor_screen);

        // Zoom in by 1.0
        let camera = camera.zoom_at(cursor_screen, 1.0);
        let cursor_world_after = camera.screen_to_world().transform_point(cursor_screen);

        // The world point under the cursor should be the same
        assert!(
            point_approx_eq(cursor_world_before, cursor_world_after),
            "Cursor world position changed: {:?} -> {:?}",
            cursor_world_before,
            cursor_world_after
        );
    }

    #[test]
    fn test_round_trip() {
        // Screen -> World -> Screen should give original
        let camera = Camera2D::new();
        let screen_orig = ScreenPoint::new(150.0, 250.0);

        let world = camera.screen_to_world().transform_point(screen_orig);
        let screen_back = camera.world_to_screen().transform_point(world);

        assert!(
            approx_eq(screen_orig.x, screen_back.x),
            "x roundtrip failed"
        );
        assert!(
            approx_eq(screen_orig.y, screen_back.y),
            "y roundtrip failed"
        );
    }

    #[test]
    fn test_move_by() {
        // Camera move_by should offset world coordinates correctly
        let camera = Camera2D::new();
        let offset = WorldVector::new(100.0, 50.0);
        let camera = camera.move_by(offset);

        assert!(approx_eq(camera.position.x, 100.0), "position.x");
        assert!(approx_eq(camera.position.y, 50.0), "position.y");

        // A screen point should now map to different world position
        let screen = ScreenPoint::new(0.0, 0.0);
        let world = camera.screen_to_world().transform_point(screen);

        // At (0,0) screen, we should see world (-100, -50) because camera moved
        assert!(approx_eq(world.x, -100.0), "world.x at origin");
        assert!(approx_eq(world.y, -50.0), "world.y at origin");
    }

    #[test]
    fn test_world_to_screen_with_viewport_origin() {
        // With a non-zero widget origin, world maps to
        // origin + (world + position) * zoom.
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::new(10.0, 20.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let world = WorldPoint::new(30.0, 5.0);
        let screen = camera.world_to_screen().transform_point(world);

        // origin + (world + pos) * zoom = (40,100) + ((30,5)+(10,20))*2 = (40,100)+(80,50)
        assert!(
            approx_eq(screen.x, 120.0),
            "x: expected 120.0, got {}",
            screen.x
        );
        assert!(
            approx_eq(screen.y, 150.0),
            "y: expected 150.0, got {}",
            screen.y
        );
    }

    #[test]
    fn test_round_trip_with_viewport_origin() {
        // screen -> world -> screen is identity for any zoom/position/origin.
        let camera = Camera2D::with_zoom_and_position(1.7, WorldPoint::new(-12.0, 33.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let screen_orig = ScreenPoint::new(150.0, 250.0);

        let world = camera.screen_to_world().transform_point(screen_orig);
        let screen_back = camera.world_to_screen().transform_point(world);

        assert!(
            approx_eq(screen_orig.x, screen_back.x),
            "x roundtrip with origin"
        );
        assert!(
            approx_eq(screen_orig.y, screen_back.y),
            "y roundtrip with origin"
        );
    }

    #[test]
    fn test_zoom_at_cursor_with_viewport_origin() {
        // Zooming must keep the world point under the cursor visually fixed even
        // when the widget sits at a non-zero origin.
        let camera = Camera2D::with_zoom_and_position(1.0, WorldPoint::new(5.0, -7.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let cursor_screen = ScreenPoint::new(400.0, 300.0);
        let world_before = camera.screen_to_world().transform_point(cursor_screen);

        let camera = camera.zoom_at(cursor_screen, 1.3);
        let world_after = camera.screen_to_world().transform_point(cursor_screen);

        assert!(
            point_approx_eq(world_before, world_after),
            "cursor world moved under zoom with origin: {world_before:?} -> {world_after:?}",
        );
        // The origin must be preserved across zoom_at.
        assert!(
            approx_eq(camera.viewport_origin().x, 40.0),
            "origin x preserved"
        );
        assert!(
            approx_eq(camera.viewport_origin().y, 100.0),
            "origin y preserved"
        );
    }

    #[test]
    fn test_zoom_increases() {
        // Zoom value should increase/decrease correctly
        let camera = Camera2D::new();
        assert!(approx_eq(camera.zoom(), 1.0), "initial zoom");

        let camera = camera.zoom_at(ScreenPoint::new(0.0, 0.0), 0.5);
        assert!(approx_eq(camera.zoom(), 1.5), "zoomed in");

        let camera = camera.zoom_at(ScreenPoint::new(0.0, 0.0), -0.5);
        assert!(approx_eq(camera.zoom(), 1.0), "zoomed back");
    }

    // === NEW COMPREHENSIVE TESTS FOR TRANSFORMATION CONSISTENCY ===

    #[test]
    fn test_inverse_consistency_at_various_zooms() {
        // Test that screen_to_world and world_to_screen are true inverses
        // at different zoom levels
        let test_cases = vec![0.5, 1.0, 1.5, 2.0, 3.0];

        for zoom in test_cases {
            let mut camera = Camera2D::new();
            camera.zoom = euclid::Scale::new(zoom);

            let screen_orig = ScreenPoint::new(250.0, 180.0);
            let world = camera.screen_to_world().transform_point(screen_orig);
            let screen_back = camera.world_to_screen().transform_point(world);

            assert!(
                approx_eq(screen_orig.x, screen_back.x) && approx_eq(screen_orig.y, screen_back.y),
                "Round-trip failed at zoom {}: {:?} -> {:?} -> {:?}",
                zoom,
                screen_orig,
                world,
                screen_back
            );
        }
    }

    #[test]
    fn test_inverse_consistency_at_various_positions() {
        // Test inverse consistency at different camera positions
        let test_positions = vec![
            WorldPoint::new(0.0, 0.0),
            WorldPoint::new(100.0, 50.0),
            WorldPoint::new(-100.0, -50.0),
            WorldPoint::new(500.0, 300.0),
        ];

        for pos in test_positions {
            let mut camera = Camera2D::new();
            camera.position = pos;

            let screen_orig = ScreenPoint::new(320.0, 240.0);
            let world = camera.screen_to_world().transform_point(screen_orig);
            let screen_back = camera.world_to_screen().transform_point(world);

            assert!(
                approx_eq(screen_orig.x, screen_back.x) && approx_eq(screen_orig.y, screen_back.y),
                "Round-trip failed at position {:?}: {:?} -> {:?} -> {:?}",
                pos,
                screen_orig,
                world,
                screen_back
            );
        }
    }

    #[test]
    fn test_inverse_consistency_combined() {
        // Test with realistic zoom + pan combinations
        let test_cases = vec![
            (1.2, WorldPoint::new(-85.0, -45.0)),
            (1.5, WorldPoint::new(-150.0, -75.0)),
            (2.0, WorldPoint::new(-300.0, -200.0)),
            (0.8, WorldPoint::new(50.0, 30.0)),
        ];

        for (zoom, pos) in test_cases {
            let mut camera = Camera2D::new();
            camera.zoom = euclid::Scale::new(zoom);
            camera.position = pos;

            // Test multiple screen points
            let screen_points = vec![
                ScreenPoint::new(0.0, 0.0),
                ScreenPoint::new(400.0, 300.0),
                ScreenPoint::new(800.0, 600.0),
            ];

            for screen_orig in screen_points {
                let world = camera.screen_to_world().transform_point(screen_orig);
                let screen_back = camera.world_to_screen().transform_point(world);

                assert!(
                    approx_eq(screen_orig.x, screen_back.x)
                        && approx_eq(screen_orig.y, screen_back.y),
                    "Round-trip failed at zoom {} pos {:?}: {:?} -> {:?} -> {:?}",
                    zoom,
                    pos,
                    screen_orig,
                    world,
                    screen_back
                );
            }
        }
    }

    #[test]
    fn test_rendering_formula_consistency() {
        // Verify that our formula matches the rendering transformation:
        // Rendering: screen = (world + position) * zoom
        // Mouse: world = screen / zoom - position

        let mut camera = Camera2D::new();
        camera.zoom = euclid::Scale::new(1.4);
        camera.position = WorldPoint::new(-170.4, -91.8);

        // A node at world position (400, 224)
        let node_world = WorldPoint::new(400.0, 224.0);

        // Should render at screen position:
        // screen = (400 + (-170.4)) * 1.4 = 229.6 * 1.4 = 321.44
        // screen = (224 + (-91.8)) * 1.4 = 132.2 * 1.4 = 185.08
        let expected_screen_x = (node_world.x + camera.position.x) * camera.zoom.get();
        let expected_screen_y = (node_world.y + camera.position.y) * camera.zoom.get();
        let expected_screen = ScreenPoint::new(expected_screen_x, expected_screen_y);

        // Now verify mouse click at that screen position finds the node
        let calculated_world = camera.screen_to_world().transform_point(expected_screen);

        assert!(
            point_approx_eq(node_world, calculated_world),
            "Rendering formula mismatch: node at {:?} renders at {:?}, but mouse at {:?} calculates {:?}",
            node_world,
            expected_screen,
            expected_screen,
            calculated_world
        );
    }

    #[test]
    fn test_zoom_at_maintains_cursor_position() {
        // When zooming at various cursor positions, the world point under cursor stays fixed
        let test_cursors = vec![
            ScreenPoint::new(400.0, 300.0), // Center
            ScreenPoint::new(0.0, 0.0),     // Top-left
            ScreenPoint::new(800.0, 600.0), // Bottom-right
            ScreenPoint::new(200.0, 450.0), // Arbitrary
        ];

        for cursor_screen in test_cursors {
            let camera = Camera2D::new();
            let world_before = camera.screen_to_world().transform_point(cursor_screen);

            // Zoom in by 0.5
            let camera = camera.zoom_at(cursor_screen, 0.5);
            let world_after = camera.screen_to_world().transform_point(cursor_screen);

            assert!(
                point_approx_eq(world_before, world_after),
                "Cursor world position changed during zoom at {:?}: {:?} -> {:?}",
                cursor_screen,
                world_before,
                world_after
            );
        }
    }

    #[test]
    fn test_zoom_at_multiple_steps() {
        // Multiple zoom operations should maintain cursor stability
        let camera = Camera2D::new();
        let cursor_screen = ScreenPoint::new(426.0, 222.0);
        let world_initial = camera.screen_to_world().transform_point(cursor_screen);

        // Simulate scrolling in 4 times
        let mut camera = camera;
        for _ in 0..4 {
            camera = camera.zoom_at(cursor_screen, 0.1);
        }

        let world_final = camera.screen_to_world().transform_point(cursor_screen);

        assert!(
            point_approx_eq(world_initial, world_final),
            "Cursor position drifted after multiple zooms: {:?} -> {:?} (delta: {}, {})",
            world_initial,
            world_final,
            world_final.x - world_initial.x,
            world_final.y - world_initial.y
        );
    }

    #[test]
    fn test_real_world_scenario_from_bug_report() {
        // This is the exact scenario from the bug report that was failing
        let mut camera = Camera2D::new();

        // User zoomed 4 times at screen (426, 222)
        let cursor_screen = ScreenPoint::new(426.0, 222.0);
        for _ in 0..4 {
            camera = camera.zoom_at(cursor_screen, 0.1);
        }

        // Final state: zoom=1.40
        assert!(
            approx_eq(camera.zoom(), 1.4),
            "zoom should be 1.4, got {}",
            camera.zoom()
        );

        // The world point under the original cursor should still be the same
        let world_after_zoom = camera.screen_to_world().transform_point(cursor_screen);
        let world_before_zoom = Camera2D::new()
            .screen_to_world()
            .transform_point(cursor_screen);

        assert!(
            point_approx_eq(world_before_zoom, world_after_zoom),
            "Cursor position drifted: {:?} -> {:?}",
            world_before_zoom,
            world_after_zoom
        );
    }

    // === Viewport-origin layout transforms ===
    // These three functions map into the widget's layout-absolute space
    // (viewport_origin + world) and were previously only exercised indirectly.
    // The cases below use a non-1 zoom and non-zero origin so every term of
    // each formula is load-bearing (mutation-audit regressions).

    #[test]
    fn test_layer_transformation_matches_world_to_screen() {
        // The renderer transform must map a layout point (viewport_origin +
        // world) to exactly where world_to_screen places that world point.
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::new(10.0, 20.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let world = WorldPoint::new(30.0, 5.0);

        let layout_point = iced::Point::new(40.0 + world.x, 100.0 + world.y);
        let via_layer = layout_point * camera.layer_transformation();
        let via_world_to_screen = camera.world_to_screen().transform_point(world);

        // Both equal the value pinned by test_world_to_screen_with_viewport_origin.
        assert!(approx_eq(via_layer.x, 120.0), "x: got {}", via_layer.x);
        assert!(approx_eq(via_layer.y, 150.0), "y: got {}", via_layer.y);
        assert!(approx_eq(via_layer.x, via_world_to_screen.x), "layer vs w2s x");
        assert!(approx_eq(via_layer.y, via_world_to_screen.y), "layer vs w2s y");
    }

    #[test]
    fn test_cursor_screen_to_layout_adds_origin() {
        // Mapping a screen cursor yields screen_to_world(cursor) shifted into
        // layout space by the viewport origin.
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::new(10.0, 20.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let cursor = mouse::Cursor::Available(iced::Point::new(120.0, 150.0));

        let mapped = match camera.cursor_screen_to_layout(cursor) {
            mouse::Cursor::Available(p) => p,
            other => panic!("expected available cursor, got {other:?}"),
        };

        // screen_to_world((120,150)) = (30,5); + origin (40,100) = (70,105).
        assert!(approx_eq(mapped.x, 70.0), "x: got {}", mapped.x);
        assert!(approx_eq(mapped.y, 105.0), "y: got {}", mapped.y);
    }

    #[test]
    fn test_viewport_screen_to_layout_rect() {
        // The visible viewport rectangle maps to layout-absolute space:
        // origin = (screen - viewport_origin) / zoom - position + viewport_origin,
        // size  = screen_size / zoom.
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::new(10.0, 20.0))
            .with_viewport_origin(ScreenVector::new(40.0, 100.0));
        let viewport = Rectangle {
            x: 120.0,
            y: 150.0,
            width: 800.0,
            height: 600.0,
        };

        let layout = camera.viewport_screen_to_layout(&viewport);

        // x = (120-40)/2 - 10 + 40 = 70 ; y = (150-100)/2 - 20 + 100 = 105.
        assert!(approx_eq(layout.x, 70.0), "x: got {}", layout.x);
        assert!(approx_eq(layout.y, 105.0), "y: got {}", layout.y);
        assert!(approx_eq(layout.width, 400.0), "w: got {}", layout.width);
        assert!(approx_eq(layout.height, 300.0), "h: got {}", layout.height);
    }
}

/// Property tests generalizing the example-based invariants above over a
/// generated input space. The example tests stay as named regression anchors;
/// these assert the same laws hold for arbitrary `(zoom, position, origin,
/// point)` combinations within realistic bounds.
///
/// Tolerances are magnitude-scaled: f32 round-trip error grows with the largest
/// intermediate coordinate (the screen-space input divided by a small zoom, plus
/// the camera position), so a fixed absolute epsilon would be both too tight for
/// large coordinates and too loose for small ones.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Bounds chosen so f32 stays well-conditioned while still covering the full
    // zoom clamp range and off-origin widgets. zoom matches the [0.1, 10.0]
    // clamp used by `zoom_at`.
    const ZOOM: std::ops::RangeInclusive<f32> = 0.1..=10.0;
    const COORD: std::ops::RangeInclusive<f32> = -5000.0..=5000.0;
    const ORIGIN: std::ops::RangeInclusive<f32> = -1000.0..=1000.0;
    const SCREEN: std::ops::RangeInclusive<f32> = 0.0..=2000.0;

    /// Tolerance scaled by the magnitude of the values being compared, plus a
    /// small absolute floor. A genuinely wrong inverse (e.g. the `.pre_scale`
    /// bug documented in this module) is off by zoom *factors*, not by this
    /// margin, so the check still bites.
    fn close(a: f32, b: f32, scale: f32) -> bool {
        let eps = 1e-2 + 1e-4 * scale.abs();
        (a - b).abs() <= eps
    }

    fn camera(zoom: f32, px: f32, py: f32, ox: f32, oy: f32) -> Camera2D {
        Camera2D::with_zoom_and_position(zoom, WorldPoint::new(px, py))
            .with_viewport_origin(ScreenVector::new(ox, oy))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// screen -> world -> screen is the identity for any camera.
        #[test]
        fn screen_world_screen_round_trip(
            zoom in ZOOM, px in COORD, py in COORD, ox in ORIGIN, oy in ORIGIN,
            sx in SCREEN, sy in SCREEN,
        ) {
            let cam = camera(zoom, px, py, ox, oy);
            let screen = ScreenPoint::new(sx, sy);
            let world = cam.screen_to_world().transform_point(screen);
            let back = cam.world_to_screen().transform_point(world);

            let scale = world.x.abs() + world.y.abs() + zoom * (px.abs() + py.abs());
            prop_assert!(close(screen.x, back.x, scale), "x: {} -> {} (scale {})", screen.x, back.x, scale);
            prop_assert!(close(screen.y, back.y, scale), "y: {} -> {} (scale {})", screen.y, back.y, scale);
        }

        /// world -> screen -> world is the identity for any camera.
        #[test]
        fn world_screen_world_round_trip(
            zoom in ZOOM, px in COORD, py in COORD, ox in ORIGIN, oy in ORIGIN,
            wx in COORD, wy in COORD,
        ) {
            let cam = camera(zoom, px, py, ox, oy);
            let world = WorldPoint::new(wx, wy);
            let screen = cam.world_to_screen().transform_point(world);
            let back = cam.screen_to_world().transform_point(screen);

            let scale = wx.abs() + wy.abs() + px.abs() + py.abs();
            prop_assert!(close(world.x, back.x, scale), "x: {} -> {} (scale {})", world.x, back.x, scale);
            prop_assert!(close(world.y, back.y, scale), "y: {} -> {} (scale {})", world.y, back.y, scale);
        }

        /// The world point under the cursor is invariant across `zoom_at`.
        #[test]
        fn zoom_at_keeps_cursor_world_fixed(
            zoom in ZOOM, px in COORD, py in COORD, ox in ORIGIN, oy in ORIGIN,
            sx in SCREEN, sy in SCREEN, delta in -5.0f32..=5.0f32,
        ) {
            let cam = camera(zoom, px, py, ox, oy);
            let cursor = ScreenPoint::new(sx, sy);
            let before = cam.screen_to_world().transform_point(cursor);
            let zoomed = cam.zoom_at(cursor, delta);
            let after = zoomed.screen_to_world().transform_point(cursor);

            // Error here is dominated by the post-zoom world magnitude.
            let scale = before.x.abs() + before.y.abs() + after.x.abs() + after.y.abs();
            prop_assert!(close(before.x, after.x, scale), "x drift: {} -> {} (scale {})", before.x, after.x, scale);
            prop_assert!(close(before.y, after.y, scale), "y drift: {} -> {} (scale {})", before.y, after.y, scale);
            // zoom stays inside the documented clamp.
            prop_assert!(zoomed.zoom() >= 0.1 && zoomed.zoom() <= 10.0);
        }

        /// `move_by` composes additively: two moves equal one move by the sum.
        #[test]
        fn move_by_is_additive(
            zoom in ZOOM, px in COORD, py in COORD,
            ax in COORD, ay in COORD, bx in COORD, by in COORD,
        ) {
            let cam = Camera2D::with_zoom_and_position(zoom, WorldPoint::new(px, py));
            let stepwise = cam.move_by(WorldVector::new(ax, ay)).move_by(WorldVector::new(bx, by));
            let combined = cam.move_by(WorldVector::new(ax + bx, ay + by));

            let scale = px.abs() + py.abs() + ax.abs() + ay.abs() + bx.abs() + by.abs();
            prop_assert!(close(stepwise.position().x, combined.position().x, scale));
            prop_assert!(close(stepwise.position().y, combined.position().y, scale));
        }

        /// Setting the viewport origin is a pure translation in screen space:
        /// it shifts every world->screen result by exactly the origin vector and
        /// nothing else (no scaling, no interaction with zoom/position).
        #[test]
        fn viewport_origin_is_pure_screen_translation(
            zoom in ZOOM, px in COORD, py in COORD, ox in ORIGIN, oy in ORIGIN,
            wx in COORD, wy in COORD,
        ) {
            let base = Camera2D::with_zoom_and_position(zoom, WorldPoint::new(px, py));
            let shifted = base.with_viewport_origin(ScreenVector::new(ox, oy));
            let world = WorldPoint::new(wx, wy);

            let s0 = base.world_to_screen().transform_point(world);
            let s1 = shifted.world_to_screen().transform_point(world);

            let scale = s0.x.abs() + s0.y.abs();
            prop_assert!(close(s1.x, s0.x + ox, scale), "x: {} vs {}+{}", s1.x, s0.x, ox);
            prop_assert!(close(s1.y, s0.y + oy, scale), "y: {} vs {}+{}", s1.y, s0.y, oy);
        }
    }
}
