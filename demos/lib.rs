//! # iced_nodegraph Demos
//!
//! This document provides an overview of all demonstration projects in the workspace.
//!
//! ## Running Demos
//!
//! All demos can be run from the workspace root with their package name:
//!
//! ```bash
//! cargo run -p demo_hello_world
//! cargo run -p demo_styling
//! cargo run -p demo_interaction
//! cargo run -p demo_500_nodes
//! cargo run -p demo_shader_editor
//! ```
//!
//! Or from the specific demo directory:
//!
//! ```bash
//! cd demos/hello_world
//! cargo run
//! ```
//!
//! ## Demo Projects
//!
//! All demos below are implemented and runnable.
//!
//! ### hello_world
//!
//! The most feature-complete demo. A pre-built workflow graph with a command
//! palette (Cmd/Ctrl+Space), 22 theme presets, live style-config nodes,
//! selection/clone/delete/group-move, edge cutting, and native persistence.
//!
//! Source: [`demos/hello_world/src/lib.rs`](../demos/hello_world/src/lib.rs).
//! Documentation: [`demos/hello_world/README.md`](../demos/hello_world/README.md).
//!
//! ### styling
//!
//! Visual customization showcase: node presets, theme switching, and live
//! style controls (corner radius, opacity, border width) applied to selection.
//!
//! Documentation: [`demos/styling/README.md`](../demos/styling/README.md).
//!
//! ### interaction
//!
//! Typed pin connection validation: input/output/bidirectional directions,
//! type compatibility, single-connection and duplicate rules, self-loop
//! rejection, and live snap feedback via `can_connect`.
//!
//! Documentation: [`demos/interaction/README.md`](../demos/interaction/README.md).
//!
//! ### 500_nodes
//!
//! Performance benchmark with a procedurally generated graph of 500+ nodes,
//! selection and group-move support, and per-layer SDF debug toggles.
//!
//! Source: [`demos/500_nodes/src/lib.rs`](../demos/500_nodes/src/lib.rs).
//!
//! ### shader_editor
//!
//! Visual WGSL shader graph with a category-grouped command palette, typed
//! sockets, and a compiler that validates and generates WGSL from the graph.
//!
//! Documentation: [`demos/shader_editor/README.md`](../demos/shader_editor/README.md).
//!
//! ## Shared Crate
//!
//! `demos/common` provides a `ScreenshotHelper` used by demos to support the
//! `--screenshot <path.png>` CLI flag for documentation captures.
//!
//! ## Building All Demos
//!
//! ```bash
//! cargo build --workspace
//! ```
//!
//! ## Architecture
//!
//! Each demo is a separate binary crate that depends on the core library:
//!
//! ```text
//! workspace/
//! |-- Cargo.toml              # Workspace manifest
//! |-- iced_nodegraph/         # Core widget library
//! |-- iced_nodegraph_sdf/               # Segment-based SDF renderer
//! `-- demos/
//!     |-- common/             # Shared screenshot helper
//!     |-- hello_world/
//!     |-- styling/
//!     |-- interaction/
//!     |-- 500_nodes/
//!     `-- shader_editor/
//! ```
//!
//! See [`docs/architecture.md`](../docs/architecture.md) for complete workspace documentation.

#![allow(unused)]

// This file provides documentation only.
// Each demo is a separate binary crate.
