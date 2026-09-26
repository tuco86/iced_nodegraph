//! The demo gallery: every demo, plus the project site's `scope` scene, behind
//! one name table.
//!
//! On wasm the crate is the single module the documentation and the project
//! site load; it runs an `iced::daemon` that opens one window per visible
//! embed, all sharing one compositor. On native the `gallery_screenshots`
//! binary renders the same scenes headlessly to the PNG fallbacks shown before
//! the wasm module is loaded.

use demo_common::SceneDef;

mod scope;

/// Every scene the gallery can open: the project site's `scope` first, then
/// the demos in the order the documentation landing page lists them.
///
/// `name` is the contract shared by `data-scene` in the embed markup, the PNG
/// file name and the JS API.
pub static SCENES: &[SceneDef] = &[
    SceneDef {
        name: "scope",
        boot: scope::scene,
    },
    SceneDef {
        name: "hello_world",
        boot: demo_hello_world::scene,
    },
    SceneDef {
        name: "styling",
        boot: demo_styling::scene,
    },
    SceneDef {
        name: "interaction",
        boot: demo_interaction::scene,
    },
    SceneDef {
        name: "500_nodes",
        boot: demo_500_nodes::scene,
    },
    SceneDef {
        name: "shader_editor",
        boot: demo_shader_editor::scene,
    },
];

#[cfg(target_arch = "wasm32")]
mod web;
