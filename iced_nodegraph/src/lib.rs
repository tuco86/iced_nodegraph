//! # iced_nodegraph
//!
//! A high-performance node graph editor widget for the [Iced](https://github.com/iced-rs/iced) GUI framework,
//! featuring GPU-accelerated rendering with custom WGPU shaders and type-safe coordinate transformations.
//!
//! ## Interactive Demo
//!
//! <div id="demo-container" style="margin: 2em 0;">
//!   <style>
//!     #demo-container canvas,
//!     #demo-container #demo-canvas-container {
//!       display: block !important;
//!       position: absolute !important;
//!       top: 0 !important;
//!       left: 0 !important;
//!       width: 100% !important;
//!       height: 100% !important;
//!       pointer-events: auto !important;
//!     }
//!     #demo-loading {
//!       position: absolute;
//!       top: 50%;
//!       left: 50%;
//!       transform: translate(-50%, -50%);
//!       text-align: center;
//!       color: #89b4fa;
//!     }
//!     .demo-spinner {
//!       width: 40px;
//!       height: 40px;
//!       border: 3px solid #313244;
//!       border-top-color: #89b4fa;
//!       border-radius: 50%;
//!       animation: demo-spin 1s linear infinite;
//!       margin: 0 auto 1em;
//!     }
//!     @keyframes demo-spin {
//!       to { transform: rotate(360deg); }
//!     }
//!     #demo-info {
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
//!     #demo-info h4 {
//!       color: #89b4fa;
//!       font-size: 0.875rem;
//!       margin-bottom: 0.5rem;
//!     }
//!     #demo-info ul {
//!       list-style: none;
//!       line-height: 1.6;
//!       margin: 0;
//!       padding: 0;
//!     }
//!     #demo-info li:before {
//!       content: "▸ ";
//!       color: #89b4fa;
//!     }
//!     #demo-error {
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
//!     <div id="demo-loading">
//!       <div class="demo-spinner"></div>
//!       <p>Loading hello_world demo...</p>
//!     </div>
//!     <div id="demo-canvas-container"></div>
//!     <div id="demo-info">
//!       <h4>Controls</h4>
//!       <ul>
//!         <li>Cmd/Ctrl+K: Command palette</li>
//!         <li>Drag nodes to move</li>
//!         <li>Drag pins to connect</li>
//!         <li>Click edges to disconnect</li>
//!         <li>Scroll to zoom</li>
//!         <li>Middle-drag to pan</li>
//!       </ul>
//!     </div>
//!   </div>
//!   
//!   <div id="demo-error">
//!     <strong>Failed to load demo.</strong> WebGPU required.
//!   </div>
//!   
//!   <script type="module">
//!     let demoInitialized = false;
//!     
//!     async function initDemo() {
//!       if (demoInitialized) return;
//!       
//!       try {
//!         const demo = await import('./demo/iced_nodegraph_demo_hello_world.js');
//!         await demo.default();
//!         
//!         document.getElementById('demo-loading').style.display = 'none';
//!         
//!         demoInitialized = true;
//!         demo.run_demo();
//!         
//!         setTimeout(() => {
//!           const canvas = document.querySelector('#demo-canvas-container canvas');
//!           if (canvas) {
//!             canvas.setAttribute('tabindex', '0');
//!             canvas.focus();
//!           }
//!         }, 100);
//!         
//!       } catch (error) {
//!         console.error('Demo error:', error);
//!         document.getElementById('demo-loading').style.display = 'none';
//!         document.getElementById('demo-error').style.display = 'block';
//!       }
//!     }
//!     
//!     initDemo();
//!   </script>
//! </div>
//!
//! ## Features
//!
//! - **Nodes** - Draggable containers for your custom widgets
//! - **Pins** - Connection points on nodes with type checking and visual feedback
//! - **Edges** - Connect pins to build data flow graphs
//! - **Interactive Connections** - Drag to connect, click edges to re-route (cable-like unplugging)
//! - **Zoom & Pan** - Smooth infinite canvas navigation
//! - **GPU Rendering** - High-performance visualization with custom WGPU shaders
//! - **Smooth Animations** - Monitor-synchronized pin pulsing and transitions
//! - **Theme Support** - Integrates with Iced's theming system
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use iced_nodegraph::NodeGraph;
//! use iced::{Element, Theme, Point};
//! use iced::widget::text;
//! use iced_wgpu::Renderer;
//!
//! // Simple message type for handling events
//! #[derive(Debug, Clone)]
//! enum Message {
//!     // Handle node graph events here
//! }
//!
//! fn view() -> Element<'static, Message, Theme, Renderer> {
//!     // Create the node graph widget
//!     let mut node_graph = NodeGraph::default();
//!     
//!     // Add a node with text content at position (100, 100)
//!     node_graph.push_node(Point::new(100.0, 100.0), text("Hello Node!"));
//!     
//!     // Convert to Iced Element
//!     node_graph.into()
//! }
//! ```
//!
//! ## Demonstration Projects
//!
//! This library includes comprehensive demo applications in the workspace:
//!
//! ### [hello_world](https://github.com/tuco86/iced_nodegraph/tree/main/demos/hello_world)
//! Basic node graph with command palette demonstrating:
//! - Node creation and positioning
//! - Pin connections with type colors
//! - Camera controls (pan/zoom)
//! - Theme switching with live preview
//! - Email processing workflow example
//!
//! ```bash
//! cargo run -p iced_nodegraph_demo_hello_world
//! ```
//!
//! ### [styling](https://github.com/tuco86/iced_nodegraph/tree/main/demos/styling) *(Planned)*
//! Visual customization showcase:
//! - Custom node styles (colors, borders, shadows)
//! - Pin appearance per type
//! - Light/dark theme integration
//! - Edge styling variations
//!
//! ### [interaction](https://github.com/tuco86/iced_nodegraph/tree/main/demos/interaction) *(Planned)*
//! Pin rules and validation:
//! - Input/output directionality
//! - Type-based connection validation
//! - Single vs. multiple connections per pin
//! - Visual feedback for valid/invalid attempts
//!
//! See the [demos directory](https://github.com/tuco86/iced_nodegraph/tree/main/demos)
//! for complete source code and detailed README specifications.
//!
//! ## Platform Support
//!
//! ### Native (Windows, macOS, Linux)
//! Full WGPU with custom shaders for high-performance rendering.
//!
//! ### WebAssembly (Browser)  
//! WebGPU acceleration with fallback to WebGL where needed.
//! See the interactive demo above for a live example.
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
