// camera.rs
use super::euclid::{
    IntoEuclid, IntoIced, Screen, ScreenPoint, ScreenRect, ScreenToWorld, ScreenVector, World, WorldPoint, WorldSize, WorldToScreen, WorldVector
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
        let zoom = self.zoom.get();
        Transform2D::translation(-self.position.x, -self.position.y)
            .pre_scale(zoom, zoom)
    }

    pub fn world_to_screen(&self) -> WorldToScreen {
        self.screen_to_world().inverse().unwrap()
    }

    pub fn move_by(&self, offset: WorldVector) -> Self {
        Self {
            zoom: self.zoom,
            position: self.position + offset,
        }
    }

    pub fn zoom_at(&self, cursor: WorldPoint, offset: f32) -> Self {
        Self {
            zoom: self.zoom,
            position: self.position,
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
