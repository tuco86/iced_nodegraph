//! Shared utilities for iced_nodegraph demos: the theme-derived node content
//! palette and the title bar built from it, and the `Demo` trait every demo
//! implements so the gallery can run it.

mod content;
mod scene;

pub use content::{NodeContentStyle, node_title_bar};
pub use scene::{Demo, Scene, SceneDef, SceneMessage, erase};
