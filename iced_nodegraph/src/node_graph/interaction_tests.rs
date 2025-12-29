//! Integration tests for pin detection and edge clicking across zoom levels
//!
//! These tests verify that coordinate transformations work correctly for:
//! - Pin detection at different zoom levels
//! - Edge click detection at different zoom levels
//! - Pin positions matching between layout and rendering

#[cfg(test)]
mod tests {
    use crate::node_graph::camera::Camera2D;
    use crate::node_graph::euclid::*;
    use iced::Point;

    #[test]
    fn test_pin_detection_at_zoom_1() {
        // At zoom 1.0, pin at world(100, 100) should be clickable at screen(100, 100)
        let camera = Camera2D::new();
        let pin_world = WorldPoint::new(100.0, 100.0);
        let cursor_screen = ScreenPoint::new(100.0, 100.0);

        let cursor_world = camera.screen_to_world().transform_point(cursor_screen);

        let distance = ((cursor_world.x - pin_world.x).powi(2)
            + (cursor_world.y - pin_world.y).powi(2))
        .sqrt();

        assert!(
            distance < 5.0,
            "At zoom 1.0: pin at world {:?} should be clickable at screen {:?}, but cursor_world={:?}, distance={:.2}",
            pin_world,
            cursor_screen,
            cursor_world,
            distance
        );
    }

    #[test]
    fn test_pin_detection_at_zoom_2() {
        // At zoom 2.0, pin at world(100, 100) renders at screen(200, 200)
        // So clicking screen(200, 200) should hit the pin
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::origin());

        let pin_world = WorldPoint::new(100.0, 100.0);
        let cursor_screen = ScreenPoint::new(200.0, 200.0); // Where it renders

        let cursor_world = camera.screen_to_world().transform_point(cursor_screen);

        let distance = ((cursor_world.x - pin_world.x).powi(2)
            + (cursor_world.y - pin_world.y).powi(2))
        .sqrt();

        assert!(
            distance < 5.0,
            "At zoom 2.0: pin at world {:?} renders at screen {:?}, cursor_world={:?}, distance={:.2}",
            pin_world,
            cursor_screen,
            cursor_world,
            distance
        );
    }

    #[test]
    fn test_pin_detection_with_pan() {
        // Camera panned to (50, 50), zoom 1.0
        // Pin at world(100, 100) renders at screen(150, 150)
        let camera = Camera2D::with_zoom_and_position(1.0, WorldPoint::new(50.0, 50.0));

        let pin_world = WorldPoint::new(100.0, 100.0);
        let cursor_screen = ScreenPoint::new(150.0, 150.0);

        let cursor_world = camera.screen_to_world().transform_point(cursor_screen);

        let distance = ((cursor_world.x - pin_world.x).powi(2)
            + (cursor_world.y - pin_world.y).powi(2))
        .sqrt();

        assert!(
            distance < 5.0,
            "With pan (50,50): pin at world {:?} renders at screen {:?}, cursor_world={:?}, distance={:.2}",
            pin_world,
            cursor_screen,
            cursor_world,
            distance
        );
    }

    #[test]
    fn test_pin_detection_with_zoom_and_pan() {
        // Camera: zoom=2.0, position=(100, 100)
        // Pin at world(200, 200) renders at:
        // CORRECT formula: screen = (world + position) * zoom
        // screen = (200 + 100, 200 + 100) * 2 = (600, 600)
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::new(100.0, 100.0));

        let pin_world = WorldPoint::new(200.0, 200.0);
        let cursor_screen = ScreenPoint::new(600.0, 600.0); // Fixed: was 500, should be 600

        let cursor_world = camera.screen_to_world().transform_point(cursor_screen);

        let distance = ((cursor_world.x - pin_world.x).powi(2)
            + (cursor_world.y - pin_world.y).powi(2))
        .sqrt();

        assert!(
            distance < 5.0,
            "Zoom=2, pan=(100,100): pin at world {:?} renders at screen {:?}, cursor_world={:?}, distance={:.2}",
            pin_world,
            cursor_screen,
            cursor_world,
            distance
        );
    }

    #[test]
    fn test_edge_click_detection_at_zoom_1() {
        // Edge from (100, 100) to (200, 100) at zoom 1.0
        // Clicking at screen(150, 100) should hit the edge
        let camera = Camera2D::new();

        let from_world = Point::new(100.0, 100.0);
        let to_world = Point::new(200.0, 100.0);
        let cursor_screen = ScreenPoint::new(150.0, 100.0);

        let cursor_world: Point = camera
            .screen_to_world()
            .transform_point(cursor_screen)
            .into_iced();

        let dist = distance_to_segment(cursor_world, from_world, to_world);

        assert!(
            dist < 5.0,
            "At zoom 1.0: edge from {:?} to {:?}, cursor at screen {:?} (world {:?}), distance={:.2}",
            from_world,
            to_world,
            cursor_screen,
            cursor_world,
            dist
        );
    }

    #[test]
    fn test_edge_click_detection_at_zoom_2() {
        // Edge from world(100, 100) to world(200, 100) at zoom 2.0
        // Renders from screen(200, 200) to screen(400, 200)
        // Clicking at screen(300, 200) should hit the edge
        let camera = Camera2D::with_zoom_and_position(2.0, WorldPoint::origin());

        let from_world = Point::new(100.0, 100.0);
        let to_world = Point::new(200.0, 100.0);
        let cursor_screen = ScreenPoint::new(300.0, 200.0);

        let cursor_world: Point = camera
            .screen_to_world()
            .transform_point(cursor_screen)
            .into_iced();

        let dist = distance_to_segment(cursor_world, from_world, to_world);

        assert!(
            dist < 5.0,
            "At zoom 2.0: edge from {:?} to {:?}, cursor at screen {:?} (world {:?}), distance={:.2}",
            from_world,
            to_world,
            cursor_screen,
            cursor_world,
            dist
        );
    }

    #[test]
    fn test_edge_click_with_pan() {
        // Camera panned to (50, 50), zoom 1.0
        // Edge from world(100, 100) to world(200, 100)
        // Renders from screen(150, 150) to screen(250, 150)
        // Clicking at screen(200, 150) should hit the edge
        let camera = Camera2D::with_zoom_and_position(1.0, WorldPoint::new(50.0, 50.0));

        let from_world = Point::new(100.0, 100.0);
        let to_world = Point::new(200.0, 100.0);
        let cursor_screen = ScreenPoint::new(200.0, 150.0);

        let cursor_world: Point = camera
            .screen_to_world()
            .transform_point(cursor_screen)
            .into_iced();

        let dist = distance_to_segment(cursor_world, from_world, to_world);

        assert!(
            dist < 5.0,
            "With pan (50,50): edge from {:?} to {:?}, cursor at screen {:?} (world {:?}), distance={:.2}",
            from_world,
            to_world,
            cursor_screen,
            cursor_world,
            dist
        );
    }

    #[test]
    fn test_vertical_edge_click() {
        // Vertical edge from world(100, 100) to world(100, 200)
        let camera = Camera2D::new();

        let from_world = Point::new(100.0, 100.0);
        let to_world = Point::new(100.0, 200.0);
        let cursor_screen = ScreenPoint::new(100.0, 150.0);

        let cursor_world: Point = camera
            .screen_to_world()
            .transform_point(cursor_screen)
            .into_iced();

        let dist = distance_to_segment(cursor_world, from_world, to_world);

        assert!(
            dist < 5.0,
            "Vertical edge: from {:?} to {:?}, cursor at screen {:?} (world {:?}), distance={:.2}",
            from_world,
            to_world,
            cursor_screen,
            cursor_world,
            dist
        );
    }

    #[test]
    fn test_diagonal_edge_click() {
        // Diagonal edge from world(100, 100) to world(200, 200)
        let camera = Camera2D::new();

        let from_world = Point::new(100.0, 100.0);
        let to_world = Point::new(200.0, 200.0);
        let cursor_screen = ScreenPoint::new(150.0, 150.0);

        let cursor_world: Point = camera
            .screen_to_world()
            .transform_point(cursor_screen)
            .into_iced();

        let dist = distance_to_segment(cursor_world, from_world, to_world);

        assert!(
            dist < 5.0,
            "Diagonal edge: from {:?} to {:?}, cursor at screen {:?} (world {:?}), distance={:.2}",
            from_world,
            to_world,
            cursor_screen,
            cursor_world,
            dist
        );
    }

    // Helper function - copy from widget.rs
    fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
        let pa = Point::new(p.x - a.x, p.y - a.y);
        let ba = Point::new(b.x - a.x, b.y - a.y);

        let h = (pa.x * ba.x + pa.y * ba.y) / (ba.x * ba.x + ba.y * ba.y);
        let h = h.clamp(0.0, 1.0);

        let closest = Point::new(a.x + h * ba.x, a.y + h * ba.y);
        let dx = p.x - closest.x;
        let dy = p.y - closest.y;
        (dx * dx + dy * dy).sqrt()
    }
}
