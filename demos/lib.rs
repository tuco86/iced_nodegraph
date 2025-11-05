//! # iced_nodegraph Demos
//!
//! This document provides an overview of all demonstration projects in the workspace.
//!
//! ## Running Demos
//!
//! All demos can be run from the workspace root:
//!
//! ```bash
//! # Hello World demo
//! cargo run -p iced_nodegraph_demo_hello_world
//!
//! # Styling demo (when implemented)
//! cargo run -p iced_nodegraph_demo_styling
//!
//! # Interaction demo (when implemented)
//! cargo run -p iced_nodegraph_demo_interaction
//! ```
//!
//! Or navigate to the specific demo directory:
//!
//! ```bash
//! cd demos/hello_world
//! cargo run
//! ```
//!
//! ## Demo Projects
//!
//! ### hello_world
//!
//! **Status**: ✅ Implemented
//!
//! Basic node graph demonstration with command palette.
//!
//! **Features:**
//! - Pre-configured email processing workflow
//! - Command palette (Cmd/Ctrl+K) for actions
//! - Theme switcher with live preview
//! - Node creation from palette
//! - Pan (middle-mouse) and zoom (scroll)
//!
//! **Source**: [`demos/hello_world/src/main.rs`](../demos/hello_world/src/main.rs)  
//! **Documentation**: [`demos/hello_world/README.md`](../demos/hello_world/README.md)
//!
//! ### styling
//!
//! **Status**: 📋 Planned (README complete, ready for implementation)
//!
//! Visual customization and theming showcase.
//!
//! **Planned Features:**
//! - Custom node styles (colors, borders, shadows)
//! - Pin appearance customization
//! - Theme switching (light/dark modes)
//! - Edge styling variations
//! - Visual feedback states
//!
//! **Specification**: [`demos/styling/README.md`](../demos/styling/README.md)
//!
//! ### interaction
//!
//! **Status**: 📋 Planned (README complete, ready for implementation)
//!
//! Pin rules and connection validation demonstration.
//!
//! **Planned Features:**
//! - Input-only pins (left side)
//! - Output-only pins (right side)
//! - Bidirectional pins (top/bottom)
//! - Type validation (prevent incompatible connections)
//! - Single vs. multiple connection enforcement
//! - Visual feedback for valid/invalid attempts
//!
//! **Specification**: [`demos/interaction/README.md`](../demos/interaction/README.md)
//!
//! ## Building All Demos
//!
//! ```bash
//! # Build entire workspace
//! cargo build --workspace
//!
//! # Build only demos
//! cargo build -p iced_nodegraph_demo_hello_world
//! cargo build -p iced_nodegraph_demo_styling
//! cargo build -p iced_nodegraph_demo_interaction
//! ```
//!
//! ## Documentation
//!
//! Each demo includes:
//!
//! - **README.md**: Detailed specification with features, requirements, and implementation notes
//! - **Source code documentation**: Inline rustdoc comments explaining key concepts
//! - **Cargo.toml**: Demo-specific dependencies
//!
//! ## For Contributors
//!
//! ### Implementing a New Demo
//!
//! 1. **Read the README**: Each demo has a comprehensive README serving as specification
//! 2. **Check dependencies**: Review `Cargo.toml` for required crates
//! 3. **Follow patterns**: Use existing demos as reference for structure
//! 4. **Document thoroughly**: Add rustdoc comments explaining concepts
//! 5. **Test comprehensively**: Ensure all features work as specified
//!
//! ### Demo Requirements
//!
//! All demos must:
//! - Be self-contained and independently runnable
//! - Include comprehensive documentation
//! - Follow workspace coding standards
//! - Use consistent naming (iced_nodegraph_demo_*)
//! - Work on all supported platforms (Windows, macOS, Linux)
//!
//! ## Architecture
//!
//! Demos are separate binary crates in the workspace:
//!
//! ```text
//! workspace/
//! ├── Cargo.toml              # Workspace manifest
//! ├── iced_nodegraph/         # Core library
//! └── demos/
//!     ├── hello_world/
//!     │   ├── Cargo.toml
//!     │   ├── README.md
//!     │   └── src/main.rs
//!     ├── styling/
//!     └── interaction/
//! ```
//!
//! See [`docs/architecture.md`](../docs/architecture.md) for complete workspace documentation.

#![allow(unused)]

// This file provides documentation only
// Each demo is a separate binary crate
