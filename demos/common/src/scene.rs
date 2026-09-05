//! The `Demo` trait and its type-erased `Scene` form.
//!
//! A demo crate implements [`Demo`]; the gallery runs several of them side by
//! side in one `iced::daemon`, which needs a single message type. [`erase`]
//! boxes a demo behind [`Scene`] and wraps its messages in [`SceneMessage`],
//! so the runtime never names the concrete demo.
//!
//! ```no_run
//! use demo_common::{Demo, Scene, SceneDef, SceneMessage, erase};
//! use iced::{Element, Task, Theme, widget::text};
//!
//! #[derive(Default)]
//! struct App;
//!
//! #[derive(Clone, Debug)]
//! enum Message {
//!     Tick,
//! }
//!
//! impl Demo for App {
//!     type Message = Message;
//!
//!     fn boot() -> (Self, Task<Message>) {
//!         (Self, Task::none())
//!     }
//!
//!     fn update(&mut self, _message: Message) -> Task<Message> {
//!         Task::none()
//!     }
//!
//!     fn view(&self) -> Element<'_, Message> {
//!         text("hello").into()
//!     }
//!
//!     fn theme(&self) -> Theme {
//!         Theme::CatppuccinFrappe
//!     }
//!
//!     fn set_theme(&mut self, _theme: Theme) {}
//! }
//!
//! pub fn scene() -> (Box<dyn Scene>, Task<SceneMessage>) {
//!     erase::<App>()
//! }
//!
//! static SCENES: &[SceneDef] = &[SceneDef {
//!     name: "example",
//!     boot: scene,
//! }];
//! ```

use std::any::Any;
use std::fmt;

use iced::{Element, Subscription, Task, Theme};

/// One demo application: the five methods `iced::application` takes, on a type.
///
/// The trait exists so the gallery can boot any demo through one signature.
/// Native `main` functions keep calling `iced::application` with the same
/// methods directly.
pub trait Demo: Sized + 'static {
    type Message: Any + Clone + Send + fmt::Debug;

    /// Builds the pristine scene the documentation shows.
    ///
    /// Never loads persisted state: an embedded demo must look the same for
    /// every visitor, and the screenshot must match what the canvas renders.
    fn boot() -> (Self, Task<Self::Message>);

    fn update(&mut self, message: Self::Message) -> Task<Self::Message>;

    fn view(&self) -> Element<'_, Self::Message>;

    fn theme(&self) -> Theme;

    /// Switches the demo onto `theme`, the way the documentation site follows
    /// rustdoc's page theme; the native binaries never call it.
    fn set_theme(&mut self, theme: Theme);

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

/// A demo message with its type erased, so scenes of different demos share one
/// runtime.
pub struct SceneMessage(Box<dyn ErasedMessage>);

trait ErasedMessage: Any + Send {
    fn clone_box(&self) -> Box<dyn ErasedMessage>;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<M: Any + Clone + Send + fmt::Debug> ErasedMessage for M {
    fn clone_box(&self) -> Box<dyn ErasedMessage> {
        Box::new(self.clone())
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl SceneMessage {
    pub fn new<M: Any + Clone + Send + fmt::Debug>(message: M) -> Self {
        Self(Box::new(message))
    }

    /// Recovers the concrete message.
    ///
    /// # Errors
    ///
    /// Returns `Err(self)` when the payload is another demo's message.
    pub fn downcast<M: Any>(self) -> Result<M, Self> {
        if (*self.0).type_id() != std::any::TypeId::of::<M>() {
            return Err(self);
        }

        match self.0.into_any().downcast::<M>() {
            Ok(message) => Ok(*message),
            Err(_) => unreachable!("type id was checked above"),
        }
    }
}

impl Clone for SceneMessage {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

impl fmt::Debug for SceneMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Object-safe view of a running demo.
pub trait Scene {
    fn update(&mut self, message: SceneMessage) -> Task<SceneMessage>;
    fn view(&self) -> Element<'_, SceneMessage>;
    fn theme(&self) -> Theme;
    fn set_theme(&mut self, theme: Theme);
    fn subscription(&self) -> Subscription<SceneMessage>;
}

struct Erased<A>(A);

impl<A: Demo> Scene for Erased<A> {
    fn update(&mut self, message: SceneMessage) -> Task<SceneMessage> {
        match message.downcast::<A::Message>() {
            Ok(message) => self.0.update(message).map(SceneMessage::new::<A::Message>),
            Err(_) => Task::none(),
        }
    }

    fn view(&self) -> Element<'_, SceneMessage> {
        self.0.view().map(SceneMessage::new::<A::Message>)
    }

    fn theme(&self) -> Theme {
        self.0.theme()
    }

    fn set_theme(&mut self, theme: Theme) {
        self.0.set_theme(theme);
    }

    fn subscription(&self) -> Subscription<SceneMessage> {
        // `Subscription::map` is compile-time checked to be zero-sized: pass
        // the fn item, never a closure.
        self.0.subscription().map(SceneMessage::new::<A::Message>)
    }
}

/// Boots a demo behind the object-safe [`Scene`] interface.
pub fn erase<A: Demo>() -> (Box<dyn Scene>, Task<SceneMessage>) {
    let (app, task) = A::boot();

    (
        Box::new(Erased(app)),
        task.map(SceneMessage::new::<A::Message>),
    )
}

/// A demo the gallery can open by name.
pub struct SceneDef {
    /// Stable name used by `data-scene`, the PNG file and the JS API.
    pub name: &'static str,
    pub boot: fn() -> (Box<dyn Scene>, Task<SceneMessage>),
}
