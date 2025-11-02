//! Camera system for coordinate space transformations.
//!
//! # Coordinate Spaces
//!
//! This module manages two coordinate spaces:
//! - **Screen Space**: Raw pixel coordinates from user input (mouse, viewport)
//! - **World Space**: Virtual canvas where nodes exist, affected by camera zoom/pan
//!
//! The `euclid` crate provides type safety to prevent mixing coordinate spaces.
//!
//! # Transformation Formula
//!
//! **Screen → World:**
//! ```text
//! world_point = (screen_point * zoom) - camera_position
//! ```
//!
//! **World → Screen:**
//! ```text
//! screen_point = (world_point + camera_position) / zoom
//! ```
//!
//! # Key Functions
//!
//! - [`screen_to_world()`](Camera2D::screen_to_world) - Get transformation matrix
//! - [`zoom_at()`](Camera2D::zoom_at) - Zoom while keeping cursor position fixed
//! - [`move_by()`](Camera2D::move_by) - Pan camera in world space
//!
//! # Examples
//!
//! ```rust,ignore
//! // Transform mouse input to world coordinates
//! let screen_pos: ScreenPoint = cursor.position().into_euclid();
//! let world_pos: WorldPoint = camera.screen_to_world().transform_point(screen_pos);
//!
//! // Zoom at cursor position (keeps point under cursor fixed)
//! let new_camera = camera.zoom_at(world_cursor, 0.5); // zoom in by 0.5
//! ```
//!
//! See [`COORDINATE_SYSTEM.md`](./COORDINATE_SYSTEM.md) for detailed documentation.

use super::euclid::{
    IntoEuclid, IntoIced, Screen, ScreenPoint, ScreenRect, ScreenToWorld, World, WorldPoint, WorldToScreen, WorldVector
};
use euclid::{Scale, Transform2D};
use iced::{
    Rectangle,
    advanced::{mouse, renderer},
};

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

    pub fn zoom(&self) -> f32 {
        self.zoom.get()
    }

    pub fn position(&self) -> WorldPoint {
        self.position
    }

    pub fn screen_to_world(&self) -> ScreenToWorld {
        // Converts screen coordinates to world coordinates, factoring in zoom and position.
        // Original formula that matches the existing tests
        Transform2D::translation(-self.position.x, -self.position.y)
            .pre_scale(self.zoom.get(), self.zoom.get())
    }

    pub fn world_to_screen(&self) -> WorldToScreen {
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

    pub fn zoom_at(&self, cursor: WorldPoint, offset: f32) -> Self {
        // Adjusts the zoom level, keeping the cursor position stable in world space.
        let old_zoom = self.zoom;
        let zoom = Scale::new(self.zoom.get() + offset);
        let offset = zoom.transform_point(old_zoom.inverse().transform_point(cursor)) - cursor;
        Self {
            zoom,
            position: self.position + offset,
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
        let zoom = self.zoom;
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
            mouse::Cursor::Available(pos) => {
                mouse::Cursor::Available(screen_to_world.transform_point(pos.into_euclid()).into_iced())
            }
            mouse::Cursor::Levitating(pos) => {
                mouse::Cursor::Levitating(screen_to_world.transform_point(pos.into_euclid()).into_iced())
            }
            mouse::Cursor::Unavailable => mouse::Cursor::Unavailable,
        }
    }

    fn viewport_screen_to_world(&self, viewport: &Rectangle<f32>) -> Rectangle<f32> {
        let viewport: ScreenRect = viewport.into_euclid();
        self.zoom.transform_rect(&viewport).translate(-self.position.to_vector()).into_iced()
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
        
        assert!(approx_eq(world.x, 100.0), "x: expected 100.0, got {}", world.x);
        assert!(approx_eq(world.y, 200.0), "y: expected 200.0, got {}", world.y);
    }
    
    #[test]
    fn test_zoom_transform() {
        // Zooming in 2x means screen coordinates map to smaller world area
        // screen pixel (100, 200) at zoom 2.0 = world (200, 400)
        let mut camera = Camera2D::new();
        camera.zoom = euclid::Scale::new(2.0);
        
        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);
        
        assert!(approx_eq(world.x, 200.0), "x: expected 200.0, got {}", world.x);
        assert!(approx_eq(world.y, 400.0), "y: expected 400.0, got {}", world.y);
    }
    
    #[test]
    fn test_pan_transform() {
        // Moving camera position should offset world coordinates
        let mut camera = Camera2D::new();
        camera.position = WorldPoint::new(50.0, 100.0);
        
        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);
        
        // world = screen * zoom - position
        // world = (100, 200) * 1.0 - (50, 100) = (50, 100)
        assert!(approx_eq(world.x, 50.0), "x: expected 50.0, got {}", world.x);
        assert!(approx_eq(world.y, 100.0), "y: expected 100.0, got {}", world.y);
    }
    
    #[test]
    fn test_zoom_and_pan() {
        // Combined zoom + pan
        let mut camera = Camera2D::new();
        camera.zoom = euclid::Scale::new(2.0);
        camera.position = WorldPoint::new(100.0, 200.0);
        
        let screen = ScreenPoint::new(100.0, 200.0);
        let world = camera.screen_to_world().transform_point(screen);
        
        // world = screen * zoom - position
        // world = (100, 200) * 2.0 - (100, 200) = (200, 400) - (100, 200) = (100, 200)
        assert!(approx_eq(world.x, 100.0), "x: expected 100.0, got {}", world.x);
        assert!(approx_eq(world.y, 200.0), "y: expected 200.0, got {}", world.y);
    }
    
    #[test]
    fn test_zoom_at_cursor() {
        // When zooming at a cursor position, that world point should stay fixed
        let camera = Camera2D::new();
        let cursor_screen = ScreenPoint::new(400.0, 300.0);
        let cursor_world_before = camera.screen_to_world().transform_point(cursor_screen);
        
        // Zoom in by 1.0
        let camera = camera.zoom_at(cursor_world_before, 1.0);
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
        
        assert!(approx_eq(screen_orig.x, screen_back.x), "x roundtrip failed");
        assert!(approx_eq(screen_orig.y, screen_back.y), "y roundtrip failed");
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
        
        let camera = camera.zoom_at(WorldPoint::origin(), 0.5);
        assert!(approx_eq(camera.zoom(), 1.5), "zoomed in");
        
        let camera = camera.zoom_at(WorldPoint::origin(), -0.5);
        assert!(approx_eq(camera.zoom(), 1.0), "zoomed back");
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
