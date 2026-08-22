//! Type-safe coordinate space conversions.
//!
//! This module provides phantom types and conversion traits for working with
//! three distinct coordinate spaces:
//!
//! - **Screen space** - Pixel coordinates from user input (mouse position, viewport)
//! - **World space** - Virtual canvas coordinates where nodes exist
//! - **Layout space** - `viewport_origin + world`, the space iced `Layout`
//!   bounds and child hit tests live in
//!
//! Using the [`euclid`](https://docs.rs/euclid) crate's phantom types prevents
//! accidental mixing of coordinate spaces at compile time.
//!
//! ## Type Aliases
//!
//! | Type | Description |
//! |------|-------------|
//! | [`WorldPoint`] | A point in world coordinates |
//! | [`ScreenPoint`] | A point in screen coordinates |
//! | [`LayoutPoint`] | A point in the widget's layout-absolute space |
//! | [`WorldVector`] | A displacement vector in world space |
//! | [`ScreenToWorld`] | Transform matrix from screen to world |
//!
//! ## Conversion Traits
//!
//! - [`IntoIced`] - Convert euclid types to iced types (for rendering)
//! - [`IntoEuclid`] - Convert iced types to euclid types (from input)
//!
//! These custom traits are used instead of `From`/`Into` to work around
//! orphan rules and provide symmetric, discoverable API.

use euclid::{Point2D, Rect, Size2D, Transform2D, Vector2D};
use iced_widget::core::{Point, Rectangle, Size, Vector};

#[derive(Debug, Clone, Copy)]
pub enum World {}

#[derive(Debug, Clone, Copy)]
pub enum Screen {}

/// The widget's layout-absolute space: `viewport_origin + world`.
///
/// iced `Layout` bounds, the positions of node child widgets, and every hit
/// test against them live here. It differs from [`World`] by the pure
/// translation `viewport_origin`, so a *displacement* is identical in both
/// spaces while a *position* is not. Fold the origin only through
/// `Camera2D::world_to_layout` and `Camera2D::layout_to_world`.
#[derive(Debug, Clone, Copy)]
pub enum LayoutSpace {}

pub type WorldPoint = Point2D<f32, World>;
pub type ScreenPoint = Point2D<f32, Screen>;

pub type WorldVector = Vector2D<f32, World>;

pub type ScreenVector = Vector2D<f32, Screen>;

pub type WorldSize = Size2D<f32, World>;

pub type WorldRect = Rect<f32, World>;

pub type ScreenRect = Rect<f32, Screen>;

pub type LayoutPoint = Point2D<f32, LayoutSpace>;

pub type LayoutVector = Vector2D<f32, LayoutSpace>;

pub type LayoutSize = Size2D<f32, LayoutSpace>;

pub type LayoutRect = Rect<f32, LayoutSpace>;

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
