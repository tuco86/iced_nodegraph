use euclid::{Point2D, Rect, Size2D, Transform2D, Vector2D};
use iced::{Point, Rectangle, Size, Vector};

#[derive(Debug, Clone, Copy)]
pub enum World {}

#[derive(Debug, Clone, Copy)]
pub enum Screen {}

pub type WorldPoint = Point2D<f32, World>;
pub type ScreenPoint = Point2D<f32, Screen>;

pub type WorldVector = Vector2D<f32, World>;

pub type WorldSize = Size2D<f32, World>;

pub type ScreenRect = Rect<f32, Screen>;

pub type ScreenToWorld = Transform2D<f32, Screen, World>;

// Define a custom Into trait
pub trait IntoIced<T> {
    fn into_iced(self) -> T;
}

pub trait IntoEuclid<T> {
    fn into_euclid(self) -> T;
}

// generically implement IntoIced for all euclid types
impl<Unit> IntoIced<Point> for Point2D<f32, Unit> {
    fn into_iced(self) -> Point {
        Point::new(self.x, self.y)
    }
}

impl<Unit> IntoIced<Vector> for Vector2D<f32, Unit> {
    fn into_iced(self) -> Vector {
        Vector::new(self.x, self.y)
    }
}

impl<Unit> IntoIced<Size> for Size2D<f32, Unit> {
    fn into_iced(self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl<Unit> IntoIced<Rectangle> for Rect<f32, Unit> {
    fn into_iced(self) -> Rectangle {
        Rectangle::new(self.origin.into_iced(), self.size.into_iced())
    }
}

// generically implement IntoEuclid for all iced types
impl<Unit> IntoEuclid<Point2D<f32, Unit>> for Point {
    fn into_euclid(self) -> Point2D<f32, Unit> {
        Point2D::new(self.x, self.y)
    }
}

impl<Unit> IntoEuclid<Vector2D<f32, Unit>> for Vector {
    fn into_euclid(self) -> Vector2D<f32, Unit> {
        Vector2D::new(self.x, self.y)
    }
}

impl<Unit> IntoEuclid<Size2D<f32, Unit>> for Size {
    fn into_euclid(self) -> Size2D<f32, Unit> {
        Size2D::new(self.width, self.height)
    }
}

impl<Unit> IntoEuclid<Rect<f32, Unit>> for Rectangle {
    fn into_euclid(self) -> Rect<f32, Unit> {
        Rect::new(self.position().into_euclid(), self.size().into_euclid())
    }
}

// generically implement IntoEuclid for euclid
impl<Unit> IntoEuclid<Point2D<f32, Unit>> for Point2D<f32, Unit> {
    fn into_euclid(self) -> Point2D<f32, Unit> {
        self
    }
}

impl<Unit> IntoEuclid<Vector2D<f32, Unit>> for Vector2D<f32, Unit> {
    fn into_euclid(self) -> Vector2D<f32, Unit> {
        self
    }
}
