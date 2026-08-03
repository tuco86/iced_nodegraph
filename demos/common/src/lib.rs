//! Shared utilities for iced_nodegraph demos: themed node content (title bar
//! plus body) and `--screenshot` capture.

mod content;
mod screenshot;

pub use content::{NodeContentStyle, simple_node};
pub use screenshot::{ScreenshotHelper, ScreenshotMessage};
