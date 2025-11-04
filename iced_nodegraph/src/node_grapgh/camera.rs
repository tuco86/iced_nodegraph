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
//! use crate::node_grapgh::euclid::IntoEuclid;
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
    IntoEuclid, IntoIced, Screen, ScreenPoint, ScreenRect, ScreenToWorld, World, WorldPoint,
    WorldRect, WorldSize, WorldVector,
};
use euclid::{Scale, Transform2D};
use iced::Rectangle;
use iced_widget::core::{mouse, renderer};

#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    zoom: Scale<f32, Screen, World>,
    position: WorldPoint,
}

impl Camera2D {
    pub fn new() -> Self {
        Self {
            zoom: Scale::new(1.0),
            position: WorldPoint::origin(),
        }
    }

    /// Create a camera with custom zoom and position (for testing)
    #[cfg(test)]
    pub fn with_zoom_and_position(zoom: f32, position: WorldPoint) -> Self {
        Self {
            zoom: Scale::new(zoom),
            position,
        }
    }

    pub fn zoom(&self) -> f32 {
        self.zoom.get()
    }

    pub fn position(&self) -> WorldPoint {
        self.position
    }

    pub fn screen_to_world(&self) -> ScreenToWorld {
        // Converts screen coordinates to world coordinates, factoring in zoom and position.
        // draw_with() does: screen = (world + position) * zoom
        // So inverse is: world = screen / zoom - position
        let inv_zoom = 1.0 / self.zoom.get();
        Transform2D::scale(inv_zoom, inv_zoom).then_translate(-self.position.to_vector())
    }

    #[cfg(test)]
    pub fn world_to_screen(&self) -> Transform2D<f32, World, Screen> {
        // Converts world coordinates to screen coordinates.
        self.screen_to_world().inverse().unwrap()
    }

    pub fn move_by(&self, offset: WorldVector) -> Self {
        // Moves the camera by a given offset in world space.
        Self {
            zoom: self.zoom,
            position: self.position + offset,
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
        let new_zoom = (old_zoom + offset).max(0.1).min(10.0); // Clamp zoom between 0.1 and 10.0

        // zoom_delta = 1/new_zoom - 1/old_zoom (not the other way around!)
        let zoom_delta = 1.0 / new_zoom - 1.0 / old_zoom;
        let position_offset =
            WorldVector::new(cursor_screen.x * zoom_delta, cursor_screen.y * zoom_delta);

        Self {
            zoom: Scale::new(new_zoom),
            position: self.position + position_offset,
        }
    }

    pub fn draw_with<'a, F, Renderer>(
        self,
        renderer: &mut Renderer,
        viewport: &iced::Rectangle,
        cursor: mouse::Cursor,
        f: F,
    ) where
        Renderer: renderer::Renderer,
        F: FnOnce(&mut Renderer, &iced::Rectangle, mouse::Cursor),
    {
        let _zoom = self.zoom;
        let offset = self.position;

        let transformed_cursor = self.cursor_screen_to_world(cursor);
        let world_viewport = self.viewport_screen_to_world(viewport);

        renderer.with_transformation(iced::Transformation::scale(self.zoom.get()), |renderer| {
            renderer.with_translation(offset.to_vector().into_iced(), |renderer| {
                f(renderer, &world_viewport, transformed_cursor)
            })
        })
    }

    pub fn update_with<'a, F>(self, viewport: &iced::Rectangle, cursor: mouse::Cursor, f: F)
    where
        F: FnOnce(&iced::Rectangle, mouse::Cursor),
    {
        let transformed_cursor = self.cursor_screen_to_world(cursor);
        let world_viewport = self.viewport_screen_to_world(viewport);
        f(&world_viewport, transformed_cursor)
    }

    fn cursor_screen_to_world(&self, cursor: mouse::Cursor) -> mouse::Cursor {
        let screen_to_world = self.screen_to_world();
        match cursor {
            mouse::Cursor::Available(pos) => mouse::Cursor::Available(
                screen_to_world
                    .transform_point(pos.into_euclid())
                    .into_iced(),
            ),
            mouse::Cursor::Levitating(pos) => mouse::Cursor::Levitating(
                screen_to_world
                    .transform_point(pos.into_euclid())
                    .into_iced(),
            ),
            mouse::Cursor::Unavailable => mouse::Cursor::Unavailable,
        }
    }

    fn viewport_screen_to_world(&self, viewport: &Rectangle<f32>) -> Rectangle<f32> {
        let viewport: ScreenRect = viewport.into_euclid();
        // Convert screen viewport to world space using same formula as screen_to_world
        // world = screen / zoom - position
        let inv_zoom = 1.0 / self.zoom.get();
        let world_viewport: WorldRect = WorldRect::new(
            WorldPoint::new(
                viewport.origin.x * inv_zoom - self.position.x,
                viewport.origin.y * inv_zoom - self.position.y,
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
}

// #[derive(Debug, Clone, Copy)]
// pub struct Camera2D {
//     pub transform: WorldToScreen,
//     pub inverse: ScreenToWorld,
// }

// impl Camera2D {
//     pub fn new() -> Self {
//         let transform = Transform2D::identity();
//         let inverse = transform.inverse().unwrap();
//         Self { transform, inverse }
//     }

//     pub fn screen_to_world(&self, screen: impl IntoEuclid<ScreenPoint>) -> WorldPoint {
//         self.inverse.transform_point(screen.into_euclid())
//     }

//     pub fn world_to_screen(&self, world: impl IntoEuclid<WorldPoint>) -> ScreenPoint {
//         self.transform.transform_point(world.into_euclid())
//     }

//     pub fn zoom_at(&mut self, screen_cursor: ScreenPoint, zoom_delta: f32) {
//         let old_zoom = self.zoom();
//         let new_zoom = (old_zoom + zoom_delta).clamp(0.1, 10.0);

//         let screen_to_world = self.inverse;
//         let old_world_at_cursor = screen_to_world.transform_point(screen_cursor.into_euclid());

//         self.transform = self
//             .transform
//             .then_translate([-old_world_at_cursor.x, -old_world_at_cursor.y].into())
//             .then_scale(new_zoom / old_zoom, new_zoom / old_zoom)
//             .then_translate([old_world_at_cursor.x, old_world_at_cursor.y].into());
//         self.inverse = self.transform.inverse().unwrap();

//         println!(
//             "zoom: {:?} -> {:?} offset: {:?} -> {:?}",
//             old_zoom,
//             new_zoom,
//             old_world_at_cursor,
//             self.inverse.transform_point(screen_cursor.into_euclid())
//         );
//     }

//     pub fn translate_screen(&mut self, delta: ScreenVector) {
//         self.transform = self.transform.then_translate(delta);
//         self.inverse = self.transform.inverse().unwrap();
//     }

//     pub fn translate_world(&mut self, delta: WorldVector) {
//         let screen_delta = self.transform.transform_vector(delta);
//         self.translate_screen(screen_delta);
//     }

//     pub fn zoom(&self) -> f32 {
//         self.transform.m11
//     }

//     pub fn with_extra_offset(&self, extra_offset: impl IntoEuclid<WorldVector>) -> Self {
//         let extra_offset = self.transform.transform_vector(extra_offset.into_euclid());
//         let transform = self
//             .transform
//             .then_translate(extra_offset)
//             .then_scale(self.zoom(), self.zoom());
//         let inverse = transform.inverse().unwrap();
//         Self { transform, inverse }
//     }

//     pub fn draw_with<'a, F, Renderer>(
//         self,
//         renderer: &mut Renderer,
//         viewport: &iced::Rectangle,
//         cursor: mouse::Cursor,
//         f: F,
//     ) where
//         Renderer: renderer::Renderer,
//         F: FnOnce(&mut Renderer, &iced::Rectangle, mouse::Cursor),
//     {
//         let zoom = self.zoom();
//         let offset = self.offset();

//         let transformed_cursor = self.cursor_screen_to_world(cursor);
//         let world_viewport = self.viewport_screen_to_world(viewport);

//         renderer.with_transformation(iced::Transformation::scale(zoom), |renderer| {
//             renderer.with_translation(offset.into_iced(), |renderer| {
//                 f(renderer, &world_viewport, transformed_cursor)
//             })
//         })
//     }

//     pub fn update_with<'a, F>(self, viewport: &iced::Rectangle, cursor: mouse::Cursor, f: F)
//     where
//         F: FnOnce(&iced::Rectangle, mouse::Cursor),
//     {
//         let transformed_cursor = self.cursor_screen_to_world(cursor);
//         let world_viewport = self.viewport_screen_to_world(viewport);
//         f(&world_viewport, transformed_cursor)
//     }

//     fn cursor_screen_to_world(&self, cursor: mouse::Cursor) -> mouse::Cursor {
//         match cursor {
//             mouse::Cursor::Available(pos) => {
//                 mouse::Cursor::Available(self.screen_to_world(pos).into_iced())
//             }
//             mouse::Cursor::Levitating(pos) => {
//                 mouse::Cursor::Levitating(self.screen_to_world(pos).into_iced())
//             }
//             mouse::Cursor::Unavailable => mouse::Cursor::Unavailable,
//         }
//     }

//     fn viewport_screen_to_world(&self, viewport: &Rectangle<f32>) -> Rectangle<f32> {
//         let top_left = self.screen_to_world(viewport.position());
//         let size = iced::Size::new(viewport.width / self.zoom(), viewport.height / self.zoom());
//         iced::Rectangle::new(iced::Point::new(top_left.x, top_left.y), size)
//     }

//     fn offset(&self) -> ScreenVector {
//         ScreenVector::new(self.transform.m31, self.transform.m32)
//     }
// }
