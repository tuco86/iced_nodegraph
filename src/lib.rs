//! # iced_nodegraph
//!
//! A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework,
//! featuring GPU-accelerated rendering with custom WGPU shaders and type-safe coordinate transformations.
//!
//! ## 🎮 Interactive WebGPU Demo
//!
//! <div id="wasm-demo-container" style="margin: 2em 0;">
//!   <style>
//!     #wasm-demo-container canvas,
//!     #wasm-demo-container #canvas-container {
//!       display: block !important;
//!       position: absolute !important;
//!       top: 0 !important;
//!       left: 0 !important;
//!       width: 100% !important;
//!       height: 100% !important;
//!       pointer-events: auto !important;
//!     }
//!     #wasm-demo-loading {
//!       position: absolute;
//!       top: 50%;
//!       left: 50%;
//!       transform: translate(-50%, -50%);
//!       text-align: center;
//!       color: #89b4fa;
//!     }
//!     .wasm-spinner {
//!       width: 40px;
//!       height: 40px;
//!       border: 3px solid #313244;
//!       border-top-color: #89b4fa;
//!       border-radius: 50%;
//!       animation: wasm-spin 1s linear infinite;
//!       margin: 0 auto 1em;
//!     }
//!     @keyframes wasm-spin {
//!       to { transform: rotate(360deg); }
//!     }
//!     #wasm-demo-info {
//!       position: absolute;
//!       bottom: 15px;
//!       right: 15px;
//!       background: rgba(30, 30, 46, 0.95);
//!       border: 1px solid #45475a;
//!       border-radius: 8px;
//!       padding: 0.75rem 1rem;
//!       font-size: 0.75rem;
//!       color: #cdd6f4;
//!     }
//!     #wasm-demo-info h4 {
//!       color: #89b4fa;
//!       font-size: 0.875rem;
//!       margin-bottom: 0.5rem;
//!     }
//!     #wasm-demo-info ul {
//!       list-style: none;
//!       line-height: 1.6;
//!       margin: 0;
//!       padding: 0;
//!     }
//!     #wasm-demo-info li:before {
//!       content: "▸ ";
//!       color: #89b4fa;
//!     }
//!     #wasm-demo-error {
//!       display: none;
//!       padding: 1.5rem;
//!       background: #f38ba8;
//!       color: #1e1e2e;
//!       border-radius: 8px;
//!       margin: 1em 0;
//!     }
//!   </style>
//!   
//!   <div style="position: relative; width: 100%; height: 600px; background: #1e1e2e; border-radius: 12px; overflow: hidden; box-shadow: 0 8px 32px rgba(0,0,0,0.3);">
//!     <div id="wasm-demo-loading">
//!       <div class="wasm-spinner"></div>
//!       <p>Loading WebGPU demo...</p>
//!     </div>
//!     <div id="canvas-container"></div>
//!     <div id="wasm-demo-info">
//!       <h4>🎯 Controls</h4>
//!       <ul>
//!         <li>Drag nodes to move</li>
//!         <li>Drag from pins to connect</li>
//!         <li>Click edges to disconnect</li>
//!         <li>Scroll to zoom</li>
//!         <li>Middle-drag to pan</li>
//!       </ul>
//!     </div>
//!   </div>
//!   
//!   <div id="wasm-demo-error">
//!     <strong>Failed to load demo.</strong> Make sure WebGPU is supported in your browser.
//!   </div>
//!   
//!   <script type="module">
//!     let initialized = false;
//!     
//!     async function initWasmDemo() {
//!       if (initialized) return;
//!       
//!       try {
//!         const wasm = await import('./pkg/iced_nodegraph.js');
//!         await wasm.default();
//!         
//!         document.getElementById('wasm-demo-loading').style.display = 'none';
//!         
//!         initialized = true;
//!         wasm.run();
//!         
//!         setTimeout(() => {
//!           const canvas = document.querySelector('#canvas-container canvas');
//!           if (canvas) {
//!             canvas.setAttribute('tabindex', '0');
//!             canvas.focus();
//!           }
//!         }, 100);
//!         
//!       } catch (error) {
//!         console.error('WASM demo error:', error);
//!         document.getElementById('wasm-demo-loading').style.display = 'none';
//!         document.getElementById('wasm-demo-error').style.display = 'block';
//!       }
//!     }
//!     
//!     initWasmDemo();
//!   </script>
//! </div>
//!
//! **Features:**
//! - 🎯 Interactive node dragging and positioning
//! - 📌 Animated connection pins with pulsing effects
//! - 🔗 Real-time edge dragging with visual feedback
//! - 🔍 Smooth zoom and pan navigation
//! - ⚡ GPU-accelerated rendering via WebGPU
//!
//! ## Features
//!
//! - **🎯 Nodes** - Draggable containers for your custom widgets
//! - **📌 Pins** - Connection points on nodes with type checking and visual feedback
//! - **🔗 Edges** - Connect pins to build data flow graphs
//! - **🖱️ Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
//! - **🔍 Zoom & Pan** - Smooth infinite canvas navigation
//! - **⚡ GPU Rendering** - High-performance visualization with custom WGPU shaders
//! - **✨ Smooth Animations** - Monitor-synchronized pin pulsing and transitions
//! - **🎨 Theme Support** - Integrates with Iced's theming system
//!
//! ## Quick Start
//!
//! ```rust
//! use iced_nodegraph::NodeGraph;
//! use iced::{Element, Point};
//!
//! let mut node_graph = NodeGraph::new();
//!
//! // Add nodes at world coordinates
//! node_graph.push(Point::new(200.0, 150.0), my_node_widget);
//! node_graph.push(Point::new(525.0, 175.0), another_node);
//!
//! // Create edges between pins
//! node_graph.on_connect(|from_node, from_pin, to_node, to_pin| {
//!     println!("Connected: node {} pin {} -> node {} pin {}",
//!              from_node, from_pin, to_node, to_pin);
//! });
//!
//! // Convert to Iced Element
//! let element: Element<Message> = node_graph.into();
//! ```
//!
//! ## Platform Support
//!
//! ### Native (Windows, macOS, Linux)
//! ```bash
//! cargo run --example hello_world
//! ```
//!
//! ### WebAssembly (Browser)
//! ```bash
//! # Build WASM bundle
//! wasm-pack build --target web --out-dir pkg --features wasm
//!
//! # Serve with HTTP server (file:// doesn't work due to CORS)
//! python -m http.server 8080
//! ```
//!
//! See the [examples directory](https://github.com/tuco86/iced_nodegraph/tree/main/examples)
//! for complete working examples.
//!
//! ## Architecture
//!
//! ### Coordinate System
//!
//! The widget uses two distinct coordinate spaces with compile-time type safety via the
//! [`euclid`](https://docs.rs/euclid) crate:
//!
//! - **Screen Space** - Pixel coordinates from user input (mouse, viewport)
//! - **World Space** - Virtual infinite canvas where nodes exist
//!
//! Transformations use mathematically consistent formulas:
//!
//! - **Screen → World**: `world = screen / zoom - position`
//! - **World → Screen**: `screen = (world + position) * zoom`
//! - **Zoom at Cursor**: `new_pos = old_pos + cursor_screen * (1/new_zoom - 1/old_zoom)`
//!
//! See [`node_grapgh::camera`] for implementation details and comprehensive test coverage.
//!
//! ### Custom Rendering
//!
//! Uses WGPU shaders for high-performance rendering:
//! - Background/Foreground layers for proper rendering order
//! - GPU-accelerated with custom vertex/fragment shaders
//! - Cross-platform support (native WGPU + WebGPU)
//!
//! See [`node_grapgh::effects`] for the rendering pipeline implementation.
//!
//! ## Interaction
//!
//! - **Pan**: Middle mouse button drag
//! - **Zoom**: Mouse wheel (maintains cursor position)
//! - **Connect Pins**: Left-click on source pin, drag to target pin
//! - **Re-route Edges**: Click on existing edge connection point - the clicked end unplugs like a physical cable
//! - **Move Nodes**: Left-click and drag node header
//!
//! ## Known Limitations
//!
//! - **Edge Rendering**: Static edge rendering between nodes is not fully implemented.
//!   Edge dragging works, but persistent edge display needs completion.
//! - **API Stability**: Expect breaking changes as the library evolves.
//!
//! ## Examples
//!
//! - [`hello_world`](https://github.com/tuco86/iced_nodegraph/blob/main/examples/hello_world.rs) - Basic node graph with interactive pins
//! - [Live WASM Demo](hello_world.html) - Browser-based demo with WebGPU acceleration

pub use node_grapgh::{NodeGraph, widget::node_graph};
pub use node_pin::{NodePin, PinDirection, PinSide, node_pin};

mod node;
mod node_grapgh;
mod node_pin;

// WASM hello_world demo
#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod hello_world_demo;
