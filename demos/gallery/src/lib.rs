//! The demo gallery: every demo behind one name table.
//!
//! On wasm the crate is the single module the documentation loads; it runs an
//! `iced::daemon` that opens one window per visible embed, all sharing one
//! compositor. On native the `gallery_screenshots` binary renders the same
//! scenes headlessly to the PNG fallbacks the documentation shows before the
//! wasm module is loaded.

use demo_common::SceneDef;

/// Every scene the gallery can open, in the order the landing page lists them.
///
/// `name` is the contract shared by `data-scene` in the embed markup, the PNG
/// file name and the JS API.
pub static SCENES: &[SceneDef] = &[
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
